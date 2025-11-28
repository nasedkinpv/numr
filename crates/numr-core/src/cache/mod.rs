//! Rate caching for currency exchange rates
//!
//! Rates are cached to `~/.config/numr/rates.json` (or platform equivalent).
//! Cache expires after 1 hour, after which fresh rates should be fetched.

use crate::types::Currency;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache expiry time in seconds (1 hour)
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
    pub(crate) rates: HashMap<(Currency, Currency), f64>,
}

impl RateCache {
    pub fn new() -> Self {
        Self {
            rates: HashMap::new(),
        }
    }

    /// Set an exchange rate
    pub fn set_rate(&mut self, from: Currency, to: Currency, rate: f64) {
        self.rates.insert((from, to), rate);
        // Also store the inverse rate
        if rate != 0.0 {
            self.rates.insert((to, from), 1.0 / rate);
        }
    }

    /// Get an exchange rate (uses BFS to find conversion path)
    pub fn get_rate(&self, from: Currency, to: Currency) -> Option<f64> {
        if from == to {
            return Some(1.0);
        }

        // BFS to find conversion path
        let mut queue = std::collections::VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        let mut distances = HashMap::new();

        queue.push_back(from);
        visited.insert(from);
        distances.insert(from, 1.0);

        while let Some(current) = queue.pop_front() {
            if current == to {
                return distances.get(&to).copied();
            }

            let current_rate = *distances.get(&current).unwrap();

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

    /// Get the cache file path
    fn cache_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "numr").map(|dirs| dirs.config_dir().join("rates.json"))
    }

    /// Load rates from file cache if not expired
    /// Returns Some(()) if cache was loaded, None if expired or missing
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

    /// Save current rates to file cache
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

    /// Check if cache file exists and is not expired
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

    /// Apply raw rates from API response
    /// - Fiat rates: "1 USD = X currency" (from exchangerate-api)
    /// - Crypto rates: "1 TOKEN = X USD" (from coingecko)
    pub fn apply_raw_rates(&mut self, raw_rates: &HashMap<String, f64>) {
        for (code, rate) in raw_rates {
            if let Ok(currency) = code.parse::<Currency>() {
                if currency.is_crypto() {
                    // Crypto: 1 TOKEN = X USD
                    self.set_rate(currency, Currency::USD, *rate);
                } else {
                    // Fiat: 1 USD = X currency
                    self.set_rate(Currency::USD, currency, *rate);
                }
            }
        }
    }

    /// Load default/fallback rates (for offline use when no cache exists)
    pub fn load_defaults(&mut self) {
        // Fiat rates (1 USD = X currency) - approximate values
        self.set_rate(Currency::USD, Currency::EUR, 0.92);
        self.set_rate(Currency::USD, Currency::GBP, 0.79);
        self.set_rate(Currency::USD, Currency::JPY, 150.0);
        self.set_rate(Currency::USD, Currency::CHF, 0.88);
        self.set_rate(Currency::USD, Currency::CNY, 7.25);
        self.set_rate(Currency::USD, Currency::CAD, 1.40);
        self.set_rate(Currency::USD, Currency::AUD, 1.55);
        self.set_rate(Currency::USD, Currency::INR, 84.0);
        self.set_rate(Currency::USD, Currency::KRW, 1400.0);
        self.set_rate(Currency::USD, Currency::RUB, 92.0);
        self.set_rate(Currency::USD, Currency::ILS, 3.65);
        self.set_rate(Currency::USD, Currency::PLN, 4.0);
        self.set_rate(Currency::USD, Currency::UAH, 41.0);

        // Crypto rates (1 TOKEN = X USD) - approximate values
        self.set_rate(Currency::BTC, Currency::USD, 95000.0);
        self.set_rate(Currency::ETH, Currency::USD, 3500.0);
        self.set_rate(Currency::SOL, Currency::USD, 150.0);
        self.set_rate(Currency::USDT, Currency::USD, 1.0);
        self.set_rate(Currency::USDC, Currency::USD, 1.0);
        self.set_rate(Currency::BNB, Currency::USD, 650.0);
        self.set_rate(Currency::XRP, Currency::USD, 1.5);
        self.set_rate(Currency::ADA, Currency::USD, 1.0);
        self.set_rate(Currency::DOGE, Currency::USD, 0.40);
        self.set_rate(Currency::DOT, Currency::USD, 8.0);
        self.set_rate(Currency::LTC, Currency::USD, 100.0);
        self.set_rate(Currency::LINK, Currency::USD, 18.0);
        self.set_rate(Currency::AVAX, Currency::USD, 45.0);
        self.set_rate(Currency::MATIC, Currency::USD, 0.55);
        self.set_rate(Currency::TON, Currency::USD, 6.0);
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

    #[test]
    fn test_rate_cache() {
        let mut cache = RateCache::new();
        cache.set_rate(Currency::USD, Currency::EUR, 0.92);

        assert_eq!(cache.get_rate(Currency::USD, Currency::EUR), Some(0.92));
        assert!(cache.get_rate(Currency::EUR, Currency::USD).is_some());
    }

    #[test]
    fn test_same_currency() {
        let cache = RateCache::new();
        assert_eq!(cache.get_rate(Currency::USD, Currency::USD), Some(1.0));
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
