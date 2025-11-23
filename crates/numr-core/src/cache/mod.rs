//! Rate caching for currency exchange rates

use std::collections::HashMap;
use crate::types::Currency;

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

    /// Get an exchange rate
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

            // Find neighbors
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

    /// Load default/fallback rates (for offline use)
    pub fn load_defaults(&mut self) {
        // Approximate rates (USD base)
        self.set_rate(Currency::USD, Currency::EUR, 0.92);
        self.set_rate(Currency::USD, Currency::GBP, 0.79);
        self.set_rate(Currency::USD, Currency::JPY, 149.50);
        self.set_rate(Currency::USD, Currency::RUB, 92.0);
        self.set_rate(Currency::USD, Currency::ILS, 3.65);
        self.set_rate(Currency::BTC, Currency::USD, 60000.0);
    }
}

impl Default for RateCache {
    fn default() -> Self {
        let mut cache = Self::new();
        cache.load_defaults();
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
}
