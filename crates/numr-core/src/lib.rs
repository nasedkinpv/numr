//! numr-core: Core calculation engine for numr
//!
//! This crate provides the pure logic for parsing and evaluating
//! natural language calculator expressions. It has no UI dependencies
//! and can be used in CLI, TUI, GUI, or WASM contexts.
//!
//! # Example
//!
//! ```
//! use numr_core::{Engine, Value};
//!
//! let mut engine = Engine::new();
//!
//! // Basic arithmetic
//! let result = engine.eval("10 + 20");
//! assert_eq!(result.as_f64(), Some(30.0));
//!
//! // Variables
//! engine.eval("tax = 15%");
//! let result = engine.eval("100 + tax");
//! // Result: 115.0
//!
//! // Percentage operations
//! let result = engine.eval("20% of 150");
//! assert_eq!(result.as_f64(), Some(30.0));
//! ```

pub mod cache;
pub mod eval;
pub mod parser;
pub mod types;

#[cfg(feature = "fetch")]
pub mod fetch;

pub use eval::EvalContext;
pub use parser::{parse_line, Ast, BinaryOp, Expr};
pub use types::{Currency, CurrencyDef, Unit, UnitDef, UnitType, Value, CURRENCIES, UNITS};

#[cfg(feature = "fetch")]
pub use fetch::fetch_rates;

/// Main engine for evaluating expressions
pub struct Engine {
    context: EvalContext,
    lines: Vec<LineResult>,
}

/// Result of evaluating a single line
#[derive(Debug, Clone)]
pub struct LineResult {
    pub input: String,
    pub value: Value,
}

impl Engine {
    /// Create a new engine instance
    pub fn new() -> Self {
        Self {
            context: EvalContext::new(),
            lines: Vec::new(),
        }
    }

    /// Evaluate a single line and store the result
    pub fn eval(&mut self, input: &str) -> Value {
        // Update 'total' variable
        let sum = self.sum();
        self.context.set_variable("total".to_string(), sum);

        let result = match parse_line(input) {
            Ok(ast) => eval::evaluate(&ast, &mut self.context),
            Err(e) => Value::Error(e),
        };

        self.lines.push(LineResult {
            input: input.to_string(),
            value: result.clone(),
        });

        result
    }

    /// Evaluate without storing the result (for previews)
    pub fn eval_preview(&self, input: &str) -> Value {
        let mut ctx = self.context.clone();
        match parse_line(input) {
            Ok(ast) => eval::evaluate(&ast, &mut ctx),
            Err(e) => Value::Error(e),
        }
    }

    /// Get the sum of all computed values (as plain number)
    pub fn sum(&self) -> Value {
        let total: f64 = self.lines.iter().filter_map(|lr| lr.value.as_f64()).sum();
        Value::Number(total)
    }

    /// Get totals grouped by type (currency, unit, plain numbers)
    /// - Currencies are converted and summed to the last used currency
    /// - Units of the same type are converted to the last used unit
    /// - Plain numbers and percentages are summed separately
    pub fn grouped_totals(&self) -> Vec<Value> {
        use std::collections::HashMap;

        let mut currency_amounts: Vec<(Currency, f64)> = Vec::new();
        let mut unit_totals: HashMap<Unit, f64> = HashMap::new();
        let mut last_unit_by_type: HashMap<types::UnitType, Unit> = HashMap::new();

        // Collect all values, tracking last used currency/unit
        for lr in &self.lines {
            match &lr.value {
                Value::Currency { amount, currency } => {
                    currency_amounts.push((*currency, *amount));
                }
                Value::WithUnit { amount, unit } => {
                    *unit_totals.entry(*unit).or_insert(0.0) += amount;
                    last_unit_by_type.insert(unit.unit_type(), *unit);
                }
                Value::Number(_) | Value::Percentage(_) | Value::Empty | Value::Error(_) => {}
            }
        }

        let mut result = Vec::new();

        // Sum all currencies, converting to the last used currency
        if !currency_amounts.is_empty() {
            let target_currency = currency_amounts.last().unwrap().0;
            let mut total_in_target = 0.0;

            for (currency, amount) in &currency_amounts {
                if *currency == target_currency {
                    total_in_target += amount;
                } else if let Some(rate) =
                    self.context.rate_cache.get_rate(*currency, target_currency)
                {
                    total_in_target += amount * rate;
                } else {
                    // Fallback: can't convert, just add the amount (not ideal but better than losing it)
                    total_in_target += amount;
                }
            }

            if total_in_target != 0.0 {
                result.push(Value::Currency {
                    amount: total_in_target,
                    currency: target_currency,
                });
            }
        }

        // Group units by type and convert to last used unit of that type
        let mut unit_by_type: HashMap<types::UnitType, f64> = HashMap::new();
        for (unit, amount) in &unit_totals {
            let unit_type = unit.unit_type();
            let target_unit = last_unit_by_type.get(&unit_type).unwrap_or(unit);

            let converted = if unit == target_unit {
                *amount
            } else if let Some(converted_amount) =
                types::unit::convert(*amount, *unit, *target_unit)
            {
                converted_amount
            } else {
                *amount // Can't convert, keep as is
            };

            *unit_by_type.entry(unit_type).or_insert(0.0) += converted;
        }

        // Add unit totals (one per unit type, using last used unit)
        for (unit_type, amount) in unit_by_type {
            if amount != 0.0 {
                if let Some(&unit) = last_unit_by_type.get(&unit_type) {
                    result.push(Value::WithUnit { amount, unit });
                }
            }
        }

        // Sort results for consistent display order (currencies first, then units by type)
        result.sort_by(|a, b| match (a, b) {
            (Value::Currency { .. }, Value::WithUnit { .. }) => std::cmp::Ordering::Less,
            (Value::WithUnit { .. }, Value::Currency { .. }) => std::cmp::Ordering::Greater,
            (Value::WithUnit { unit: u1, .. }, Value::WithUnit { unit: u2, .. }) => {
                u1.unit_type().cmp(&u2.unit_type())
            }
            _ => std::cmp::Ordering::Equal,
        });

        result
    }

    /// Get all line results
    pub fn lines(&self) -> &[LineResult] {
        &self.lines
    }

    /// Clear all lines and variables
    pub fn clear(&mut self) {
        self.lines.clear();
        self.context.clear_variables();
    }

    /// Set an exchange rate
    pub fn set_exchange_rate(&mut self, from: Currency, to: Currency, rate: f64) {
        self.context.set_exchange_rate(from, to, rate);
    }

    /// Apply raw rates from API response (delegates to rate cache)
    pub fn apply_raw_rates(&mut self, raw_rates: &std::collections::HashMap<String, f64>) {
        self.context.rate_cache.apply_raw_rates(raw_rates);
    }

    /// Save rates to file cache (delegates to rate cache)
    pub fn save_rates_to_cache(&self, raw_rates: &std::collections::HashMap<String, f64>) {
        self.context.rate_cache.save_to_file(raw_rates);
    }

    /// Check if rate cache file is valid (not expired)
    pub fn is_rate_cache_valid() -> bool {
        cache::RateCache::is_cache_valid()
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_basic() {
        let mut engine = Engine::new();
        let result = engine.eval("10 + 20");
        assert_eq!(result.as_f64(), Some(30.0));
    }

    #[test]
    fn test_engine_variables() {
        let mut engine = Engine::new();
        engine.eval("x = 100");
        let result = engine.eval("x + 50");
        assert_eq!(result.as_f64(), Some(150.0));
    }

    #[test]
    fn test_engine_sum() {
        let mut engine = Engine::new();
        engine.eval("10");
        engine.eval("20");
        engine.eval("30");
        assert_eq!(engine.sum().as_f64(), Some(60.0));
    }

    #[test]
    fn test_grouped_totals() {
        let mut engine = Engine::new();
        // Set explicit rate for test: 1 USD = 0.92 EUR, so 1 EUR = 1.087 USD
        engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.92);

        engine.eval("$100");
        engine.eval("$50");
        engine.eval("€200"); // Last currency is EUR, so total should be in EUR
        engine.eval("1000 m");
        engine.eval("5 km"); // Last unit is km, so total should be in km
        engine.eval("42"); // Plain numbers are ignored in totals

        let totals = engine.grouped_totals();
        assert_eq!(totals.len(), 2); // EUR (all currencies), km (all lengths) - no plain numbers

        // All currencies converted to EUR (last used)
        // $150 = €138 (150 * 0.92) + €200 = €338
        assert!(totals.iter().any(|v| matches!(v,
            Value::Currency { amount, currency }
            if *currency == Currency::EUR && (*amount - 338.0).abs() < 1.0
        )));

        // Length: 1000 m + 5 km = 1 km + 5 km = 6 km (last unit is km)
        assert!(totals.iter().any(|v| matches!(v,
            Value::WithUnit { amount, unit }
            if *unit == Unit::Kilometer && (*amount - 6.0).abs() < 0.01
        )));
    }

    #[test]
    fn test_grouped_totals_last_currency() {
        let mut engine = Engine::new();
        engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.92);

        engine.eval("€100");
        engine.eval("$50"); // Last is USD, so total in USD

        let totals = engine.grouped_totals();

        // €100 = $108.70 (100 / 0.92) + $50 = $158.70
        assert!(totals.iter().any(|v| matches!(v,
            Value::Currency { amount, currency }
            if *currency == Currency::USD && (*amount - 158.70).abs() < 1.0
        )));
    }
}
