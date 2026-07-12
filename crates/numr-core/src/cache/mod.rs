//! Rate caching for currency exchange rates
//!
//! On native platforms, rates are cached to `~/.config/numr/rates.json`.
//! Cache expires after 1 hour, after which fresh rates should be fetched.
//! On WASM, filesystem caching is not available - use defaults only.

use crate::error::{EvalError, RateError};
use crate::types::Currency;
use rust_decimal::Decimal;
#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache expiry time in seconds (1 hour)
#[cfg(not(target_arch = "wasm32"))]
const CACHE_EXPIRY_SECS: u64 = 3600;

/// Cached rates file format
#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize, Deserialize)]
struct CachedRates {
    /// Unix timestamp when rates were fetched
    timestamp: u64,
    /// Rates as code -> value (e.g., "EUR" -> 0.92, "BTC" -> 95000)
    rates: HashMap<String, Decimal>,
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
        let _ = self.try_set_rate(from, to, rate);
    }

    /// Set a rate and its reciprocal without allowing Decimal overflow.
    pub fn try_set_rate(
        &mut self,
        from: Currency,
        to: Currency,
        rate: Decimal,
    ) -> Result<(), EvalError> {
        if rate.is_sign_negative() || rate.is_zero() {
            return Err(EvalError::InvalidArgument(
                "exchange rate must be positive".to_string(),
            ));
        }
        let inverse = Decimal::ONE.checked_div(rate).ok_or(EvalError::Overflow {
            operation: "inverting an exchange rate",
        })?;
        self.rates.insert((from, to), rate);
        // Also store the inverse rate
        self.rates.insert((to, from), inverse);
        Ok(())
    }

    /// Get an exchange rate (uses BFS to find conversion path)
    #[must_use]
    pub fn get_rate(&self, from: Currency, to: Currency) -> Option<Decimal> {
        self.try_get_rate(from, to).ok().flatten()
    }

    /// Resolve an exchange-rate path while reporting arithmetic overflow.
    pub fn try_get_rate(&self, from: Currency, to: Currency) -> Result<Option<Decimal>, EvalError> {
        if from == to {
            return Ok(Some(Decimal::ONE));
        }
        if let Some(rate) = self.rates.get(&(from, to)) {
            return Ok(Some(*rate));
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
                return Ok(distances.get(&to).copied());
            }

            let Some(&current_rate) = distances.get(&current) else {
                continue; // Should never happen, but handle gracefully
            };

            for ((start, end), rate) in &self.rates {
                if *start == current && !visited.contains(end) {
                    visited.insert(*end);
                    let combined = current_rate.checked_mul(*rate).ok_or(EvalError::Overflow {
                        operation: "combining exchange rates",
                    })?;
                    distances.insert(*end, combined);
                    queue.push_back(*end);
                }
            }
        }

        Ok(None)
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
    pub fn load_from_file(&mut self) -> Result<bool, RateError> {
        let path = Self::cache_path().ok_or(RateError::CacheLocationUnavailable)?;
        self.load_from_path(&path)
    }

    /// Load a cache from an explicit path. Useful for deterministic adapters and tests.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_path(&mut self, path: &Path) -> Result<bool, RateError> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(RateError::Read(error)),
        };
        let cached: CachedRates = serde_json::from_str(&content).map_err(RateError::Deserialize)?;

        // Check if expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| RateError::Clock)?
            .as_secs();

        if cached.timestamp > now || now - cached.timestamp >= CACHE_EXPIRY_SECS {
            return Ok(false); // Expired
        }

        self.apply_raw_rates(&cached.rates)?;
        Ok(true)
    }

    /// Load rates from file cache (WASM stub - always returns None)
    #[cfg(target_arch = "wasm32")]
    pub fn load_from_file(&mut self) -> Result<bool, RateError> {
        Err(RateError::UnsupportedPlatform)
    }

    /// Save current rates to file cache (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self, raw_rates: &HashMap<String, Decimal>) -> Result<(), RateError> {
        let path = Self::cache_path().ok_or(RateError::CacheLocationUnavailable)?;
        self.save_to_path(&path, raw_rates)
    }

    /// Atomically persist a cache to an explicit path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_path(
        &self,
        path: &Path,
        raw_rates: &HashMap<String, Decimal>,
    ) -> Result<(), RateError> {
        let mut validation_cache = Self::new();
        validation_cache.apply_raw_rates(raw_rates)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(RateError::CreateDirectory)?;
        }

        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| RateError::Clock)?;
        let now = duration.as_secs();

        let supported_rates = raw_rates
            .iter()
            .filter(|(code, _)| code.parse::<Currency>().is_ok())
            .map(|(code, rate)| (code.clone(), *rate))
            .collect();
        let cached = CachedRates {
            timestamp: now,
            rates: supported_rates,
        };

        let content = serde_json::to_string_pretty(&cached).map_err(RateError::Serialize)?;
        let mut file = atomic_write_file::AtomicWriteFile::open(path).map_err(RateError::Write)?;
        file.write_all(content.as_bytes())
            .map_err(RateError::Write)?;
        file.flush().map_err(RateError::Write)?;
        file.sync_all().map_err(RateError::Write)?;
        file.commit().map_err(RateError::Write)?;
        Ok(())
    }

    /// Save current rates to file cache (WASM stub - no-op)
    #[cfg(target_arch = "wasm32")]
    pub fn save_to_file(&self, _raw_rates: &HashMap<String, Decimal>) -> Result<(), RateError> {
        Err(RateError::UnsupportedPlatform)
    }

    /// Apply raw rates from API response
    /// - Fiat rates: "1 USD = X currency" (from exchangerate-api)
    /// - Crypto rates: "1 TOKEN = X USD" (from coingecko)
    pub fn apply_raw_rates(
        &mut self,
        raw_rates: &HashMap<String, Decimal>,
    ) -> Result<usize, RateError> {
        let mut staged = self.clone();
        let mut applied = 0usize;
        for (code, rate) in raw_rates {
            if let Ok(currency) = code.parse::<Currency>() {
                let result = if currency.is_crypto() {
                    // Crypto: 1 TOKEN = X USD
                    staged.try_set_rate(currency, Currency::USD, *rate)
                } else {
                    // Fiat: 1 USD = X currency
                    staged.try_set_rate(Currency::USD, currency, *rate)
                };
                result.map_err(|error| RateError::InvalidRates(format!("{code}: {error}")))?;
                applied += 1;
            }
        }
        if applied == 0 {
            return Err(RateError::InvalidRates(
                "no supported currency rates were provided".to_string(),
            ));
        }
        self.rates = staged.rates;
        Ok(applied)
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
        // Defaults are deterministic. Filesystem loading is an explicit adapter action.
        cache.load_defaults();
        cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

    fn temporary_cache_path(name: &str) -> PathBuf {
        let id = TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "numr-cache-{name}-{}-{id}.json",
            std::process::id()
        ))
    }

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

    #[test]
    fn future_and_expired_cache_timestamps_are_rejected() {
        let path = temporary_cache_path("timestamps");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut rates = HashMap::new();
        rates.insert("EUR".to_string(), Decimal::ONE);
        for timestamp in [now + 60, now.saturating_sub(CACHE_EXPIRY_SECS)] {
            let content = serde_json::to_string(&CachedRates {
                timestamp,
                rates: rates.clone(),
            })
            .unwrap();
            fs::write(&path, content).unwrap();
            assert!(!RateCache::new().load_from_path(&path).unwrap());
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn cache_save_is_atomic_and_leaves_no_temporary_file() {
        let directory = temporary_cache_path("atomic").with_extension("");
        let path = directory.join("rates.json");
        let mut rates = HashMap::new();
        rates.insert("EUR".to_string(), Decimal::new(92, 2));
        RateCache::new().save_to_path(&path, &rates).unwrap();

        assert!(RateCache::new().load_from_path(&path).unwrap());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cache_save_atomically_replaces_an_existing_file() {
        let directory = temporary_cache_path("replace").with_extension("");
        let path = directory.join("rates.json");
        let first = HashMap::from([("EUR".to_string(), Decimal::new(92, 2))]);
        let second = HashMap::from([("EUR".to_string(), Decimal::new(85, 2))]);
        let cache = RateCache::new();
        cache.save_to_path(&path, &first).unwrap();
        cache.save_to_path(&path, &second).unwrap();

        let mut loaded = RateCache::new();
        assert!(loaded.load_from_path(&path).unwrap());
        assert_eq!(
            loaded.get_rate(Currency::USD, Currency::EUR),
            Some(Decimal::new(85, 2))
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn raw_rates_are_validated_before_mutating_or_persisting() {
        let mut cache = RateCache::new();
        cache.load_defaults();
        let original = cache.get_rate(Currency::USD, Currency::EUR);
        let invalid = HashMap::from([
            ("UNKNOWN".to_string(), Decimal::ONE),
            ("EUR".to_string(), Decimal::ZERO),
        ]);

        assert!(matches!(
            cache.apply_raw_rates(&invalid),
            Err(RateError::InvalidRates(_))
        ));
        assert_eq!(cache.get_rate(Currency::USD, Currency::EUR), original);

        let path = temporary_cache_path("invalid");
        assert!(matches!(
            cache.save_to_path(&path, &invalid),
            Err(RateError::InvalidRates(_))
        ));
        assert!(!path.exists());
    }
}
