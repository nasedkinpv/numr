//! Rate caching for currency exchange rates
//!
//! On native platforms, rates are cached to `~/.config/numr/rates.json`.
//! Cache expires after 1 hour, after which fresh rates should be fetched.
//! On WASM, filesystem caching is not available - use defaults only.

use crate::types::Currency;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache expiry time in seconds (1 hour)
#[cfg(not(target_arch = "wasm32"))]
const CACHE_EXPIRY_SECS: u64 = 3600;

/// Cached rates file format
#[derive(Serialize, Deserialize)]
struct CachedRates {
    /// Unix timestamp when rates were fetched
    timestamp: u64,
    /// Rates as code -> value (e.g., "EUR" -> 0.92, "BTC" -> 95000)
    rates: HashMap<String, f64>,
}

/// Cache for exchange rates
#[derive(Clone)]
pub struct RateCache {
    pub(crate) rates: HashMap<(Currency, Currency), Decimal>,
}

impl RateCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rates: HashMap::new(),
        }
    }

    /// Set an exchange rate
    pub fn set_rate(&mut self, from: Currency, to: Currency, rate: Decimal) {
        self.rates.insert((from, to), rate);
        // Also store the inverse rate
        if !rate.is_zero() {
            self.rates.insert((to, from), Decimal::ONE / rate);
        }
    }

    /// Get an exchange rate (uses BFS to find conversion path)
    #[must_use]
    pub fn get_rate(&self, from: Currency, to: Currency) -> Option<Decimal> {
        if from == to {
            return Some(Decimal::ONE);
        }

        // BFS to find conversion path
        let mut queue = std::collections::VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        let mut distances = HashMap::new();

        queue.push_back(from);
        visited.insert(from);
        distances.insert(from, Decimal::ONE);

        while let Some(current) = queue.pop_front() {
            if current == to {
                return distances.get(&to).copied();
            }

            let Some(&current_rate) = distances.get(&current) else {
                continue; // Should never happen, but handle gracefully
            };

            for ((start, end), rate) in &self.rates {
                if *start == current && !visited.contains(end) {
                    visited.insert(*end);
                    distances.insert(*end, current_rate * rate);
                    queue.push_back(*end);
                }
            }
        }

        None
    }

    /// Clear all cached rates
    pub fn clear(&mut self) {
        self.rates.clear();
    }

    /// Get the cache file path (native only)
    #[cfg(not(target_arch = "wasm32"))]
    fn cache_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "numr").map(|dirs| dirs.config_dir().join("rates.json"))
    }

    /// Load rates from file cache if not expired (native only)
    /// Returns Some(()) if cache was loaded, None if expired or missing
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_file(&mut self) -> Option<()> {
        let path = Self::cache_path()?;
        let content = fs::read_to_string(&path).ok()?;
        let cached: CachedRates = serde_json::from_str(&content).ok()?;

        // Check if expired
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();

        if now - cached.timestamp > CACHE_EXPIRY_SECS {
            return None; // Expired
        }

        // Load rates
        self.apply_raw_rates(&cached.rates);
        Some(())
    }

    /// Load rates from file cache (WASM stub - always returns None)
    #[cfg(target_arch = "wasm32")]
    pub fn load_from_file(&mut self) -> Option<()> {
        None
    }

    /// Save current rates to file cache (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self, raw_rates: &HashMap<String, f64>) {
        let Some(path) = Self::cache_path() else {
            return;
        };

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let cached = CachedRates {
            timestamp: now,
            rates: raw_rates.clone(),
        };

        if let Ok(content) = serde_json::to_string_pretty(&cached) {
            let _ = fs::write(&path, content);
        }
    }

    /// Save current rates to file cache (WASM stub - no-op)
    #[cfg(target_arch = "wasm32")]
    pub fn save_to_file(&self, _raw_rates: &HashMap<String, f64>) {
        // No filesystem in WASM
    }

    /// Check if cache file exists and is not expired (native only)
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn is_cache_valid() -> bool {
        let Some(path) = Self::cache_path() else {
            return false;
        };

        let Ok(content) = fs::read_to_string(&path) else {
            return false;
        };

        let Ok(cached) = serde_json::from_str::<CachedRates>(&content) else {
            return false;
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        now - cached.timestamp <= CACHE_EXPIRY_SECS
    }

    /// Check if cache file exists and is not expired (WASM stub - always false)
    #[cfg(target_arch = "wasm32")]
    #[must_use]
    pub fn is_cache_valid() -> bool {
        false
    }

    /// Apply raw rates from API response
    /// - Fiat rates: "1 USD = X currency" (from exchangerate-api)
    /// - Crypto rates: "1 TOKEN = X USD" (from coingecko)
    pub fn apply_raw_rates(&mut self, raw_rates: &HashMap<String, f64>) {
        for (code, rate) in raw_rates {
            if let Ok(currency) = code.parse::<Currency>() {
                if let Some(decimal_rate) = Decimal::from_f64(*rate) {
                    if currency.is_crypto() {
                        // Crypto: 1 TOKEN = X USD
                        self.set_rate(currency, Currency::USD, decimal_rate);
                    } else {
                        // Fiat: 1 USD = X currency
                        self.set_rate(Currency::USD, currency, decimal_rate);
                    }
                }
            }
        }
    }

    /// Load default/fallback rates (for offline use when no cache exists)
    pub fn load_defaults(&mut self) {
        use std::str::FromStr;

        // Helper to create Decimal from string
        let d = |s: &str| Decimal::from_str(s).unwrap();

        // Fiat rates (1 USD = X currency) - approximate values
        self.set_rate(Currency::USD, Currency::EUR, d("0.92"));
        self.set_rate(Currency::USD, Currency::GBP, d("0.79"));
        self.set_rate(Currency::USD, Currency::JPY, d("150"));
        self.set_rate(Currency::USD, Currency::CHF, d("0.88"));
        self.set_rate(Currency::USD, Currency::CNY, d("7.25"));
        self.set_rate(Currency::USD, Currency::CAD, d("1.40"));
        self.set_rate(Currency::USD, Currency::AUD, d("1.55"));
        self.set_rate(Currency::USD, Currency::INR, d("84"));
        self.set_rate(Currency::USD, Currency::KRW, d("1400"));
        self.set_rate(Currency::USD, Currency::RUB, d("92"));
        self.set_rate(Currency::USD, Currency::ILS, d("3.65"));
        self.set_rate(Currency::USD, Currency::PLN, d("4"));
        self.set_rate(Currency::USD, Currency::UAH, d("41"));

        // Crypto rates (1 TOKEN = X USD) - approximate values
        self.set_rate(Currency::BTC, Currency::USD, d("95000"));
        self.set_rate(Currency::ETH, Currency::USD, d("3500"));
        self.set_rate(Currency::SOL, Currency::USD, d("150"));
        self.set_rate(Currency::USDT, Currency::USD, d("1"));
        self.set_rate(Currency::USDC, Currency::USD, d("1"));
        self.set_rate(Currency::BNB, Currency::USD, d("650"));
        self.set_rate(Currency::XRP, Currency::USD, d("1.5"));
        self.set_rate(Currency::ADA, Currency::USD, d("1"));
        self.set_rate(Currency::DOGE, Currency::USD, d("0.40"));
        self.set_rate(Currency::DOT, Currency::USD, d("8"));
        self.set_rate(Currency::LTC, Currency::USD, d("100"));
        self.set_rate(Currency::LINK, Currency::USD, d("18"));
        self.set_rate(Currency::AVAX, Currency::USD, d("45"));
        self.set_rate(Currency::MATIC, Currency::USD, d("0.55"));
        self.set_rate(Currency::TON, Currency::USD, d("6"));
    }
}

impl Default for RateCache {
    fn default() -> Self {
        let mut cache = Self::new();
        // Always load defaults first as a base
        cache.load_defaults();
        // Then try to load from file cache to override with fresher rates
        // (This way we always have crypto rates even if cache only has fiat)
        let _ = cache.load_from_file();
        cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_rate_cache() {
        let mut cache = RateCache::new();
        cache.set_rate(
            Currency::USD,
            Currency::EUR,
            Decimal::from_str("0.92").unwrap(),
        );

        assert_eq!(
            cache.get_rate(Currency::USD, Currency::EUR),
            Some(Decimal::from_str("0.92").unwrap())
        );
        assert!(cache.get_rate(Currency::EUR, Currency::USD).is_some());
    }

    #[test]
    fn test_same_currency() {
        let cache = RateCache::new();
        assert_eq!(
            cache.get_rate(Currency::USD, Currency::USD),
            Some(Decimal::ONE)
        );
    }

    #[test]
    fn test_default_has_all_currencies() {
        // Use load_defaults() directly instead of default() to avoid file cache interference
        let mut cache = RateCache::new();
        cache.load_defaults();
        // Should be able to convert any currency to USD
        assert!(cache.get_rate(Currency::ETH, Currency::USD).is_some());
        assert!(cache.get_rate(Currency::SOL, Currency::USD).is_some());
        assert!(cache.get_rate(Currency::PLN, Currency::USD).is_some());
    }

    #[test]
    fn test_cross_conversion() {
        // Use load_defaults() directly instead of default() to avoid file cache interference
        let mut cache = RateCache::new();
        cache.load_defaults();
        // ETH -> RUB should work via USD
        assert!(cache.get_rate(Currency::ETH, Currency::RUB).is_some());
    }
}
