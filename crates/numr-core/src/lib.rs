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

use rust_decimal::prelude::FromPrimitive;
use std::str::FromStr;

pub mod cache;
pub mod eval;
pub mod parser;
pub mod types;

#[cfg(feature = "fetch")]
pub mod fetch;

pub use eval::EvalContext;
pub use parser::{parse_line, try_parse_exact, Ast, BinaryOp, Expr};
pub use types::{Currency, CurrencyDef, Unit, UnitDef, UnitType, Value, CURRENCIES, UNITS};

// Re-export Decimal for tests and external use
pub use rust_decimal::Decimal;

/// Parse a string into a Decimal. Convenience for tests.
///
/// # Panics
/// Panics if the string is not a valid decimal number.
///
/// # Example
/// ```
/// use numr_core::decimal;
/// let d = decimal("123.45");
/// assert_eq!(d.to_string(), "123.45");
/// ```
pub fn decimal(s: &str) -> Decimal {
    Decimal::from_str(s).expect("Invalid decimal string")
}

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
    /// True if this line's value was consumed by a continuation (next line used it as `_`)
    pub is_continuation_source: bool,
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
        // Update 'total' variable with current sum
        self.context.set_variable("total".to_string(), self.sum());

        // Set '_', 'ANS', and 'ans' to the last valid result
        if let Some(last_value) = self.last_valid_line().map(|lr| lr.value.clone()) {
            self.context
                .set_variable("_".to_string(), last_value.clone());
            self.context
                .set_variable("ANS".to_string(), last_value.clone());
            self.context
                .set_variable("ans".to_string(), last_value);
        }

        // Try continuation-first if '_' exists, otherwise normal parse
        let (result, continuation_succeeded) = self.eval_with_continuation(input);

        // Mark previous line as consumed if continuation succeeded or input uses '_'
        if continuation_succeeded || Self::references_underscore(input) {
            if let Some(last) = self.last_valid_line_mut() {
                last.is_continuation_source = true;
            }
        }

        self.lines.push(LineResult {
            input: input.to_string(),
            value: result.clone(),
            is_continuation_source: false,
        });

        result
    }

    /// Try continuation parsing first, fall back to normal parsing
    /// Returns (result, whether_continuation_succeeded)
    fn eval_with_continuation(&mut self, input: &str) -> (Value, bool) {
        // Only try continuation if we have a previous result
        if self.context.get_variable("_").is_some() {
            let continued = format!("_ {}", input);
            // Use exact parsing to avoid fuzzy suffix matching
            if let Ok(ast) = try_parse_exact(&continued) {
                let result = eval::evaluate(&ast, &mut self.context);
                if !result.is_error() {
                    return (result, true);
                }
            }
        }
        // Fall back to normal parsing
        (self.parse_and_eval(input), false)
    }

    /// Check if input contains a standalone `_` or `ANS` reference (not part of another identifier)
    fn references_underscore(input: &str) -> bool {
        input
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == "_" || word.eq_ignore_ascii_case("ans"))
    }

    /// Find the last valid (non-empty, non-error) line result
    fn last_valid_line(&self) -> Option<&LineResult> {
        self.lines
            .iter()
            .rev()
            .find(|lr| !lr.value.is_empty() && !lr.value.is_error())
    }

    /// Find the last valid line result (mutable)
    fn last_valid_line_mut(&mut self) -> Option<&mut LineResult> {
        self.lines
            .iter_mut()
            .rev()
            .find(|lr| !lr.value.is_empty() && !lr.value.is_error())
    }

    /// Parse and evaluate input, returning the result
    fn parse_and_eval(&mut self, input: &str) -> Value {
        match parse_line(input) {
            Ok(ast) => eval::evaluate(&ast, &mut self.context),
            Err(e) => Value::Error(e),
        }
    }

    /// Evaluate without storing the result (for previews)
    pub fn eval_preview(&self, input: &str) -> Value {
        let mut ctx = self.context.clone();

        // Set '_', 'ANS', and 'ans' to the last valid result for preview context
        if let Some(last) = self.last_valid_line() {
            ctx.set_variable("_".to_string(), last.value.clone());
            ctx.set_variable("ANS".to_string(), last.value.clone());
            ctx.set_variable("ans".to_string(), last.value.clone());
        }

        // Try continuation-first if we have a previous value
        if ctx.get_variable("_").is_some() {
            if let Ok(ast) = try_parse_exact(&format!("_ {}", input)) {
                let result = eval::evaluate(&ast, &mut ctx);
                if !result.is_error() {
                    return result;
                }
            }
        }

        // Fallback to normal parse
        match parse_line(input) {
            Ok(ast) => eval::evaluate(&ast, &mut ctx),
            Err(e) => Value::Error(e),
        }
    }

    /// Get the sum of all computed values (as plain number)
    /// Excludes lines that were consumed by continuations
    pub fn sum(&self) -> Value {
        let total: Decimal = self
            .lines
            .iter()
            .filter(|lr| !lr.is_continuation_source)
            .filter_map(|lr| lr.value.as_decimal())
            .sum();
        Value::Number(total)
    }

    /// Get totals grouped by type (currency, unit, plain numbers)
    /// - Currencies are converted and summed to the last used currency
    /// - Units of the same type are converted to the last used unit
    /// - Plain numbers and percentages are summed separately
    /// - Excludes lines that were consumed by continuations
    pub fn grouped_totals(&self) -> Vec<Value> {
        use std::collections::HashMap;

        let mut currency_amounts: Vec<(Currency, Decimal)> = Vec::new();
        let mut unit_totals: HashMap<Unit, Decimal> = HashMap::new();
        let mut last_unit_by_type: HashMap<types::UnitType, Unit> = HashMap::new();

        // Collect all values, tracking last used currency/unit
        // Skip lines that were consumed by continuations
        for lr in self.lines.iter().filter(|lr| !lr.is_continuation_source) {
            match &lr.value {
                Value::Currency { amount, currency } => {
                    currency_amounts.push((*currency, *amount));
                }
                Value::WithUnit { amount, unit } => {
                    *unit_totals.entry(*unit).or_insert(Decimal::ZERO) += amount;
                    last_unit_by_type.insert(unit.unit_type(), *unit);
                }
                Value::Number(_) | Value::Percentage(_) | Value::Empty | Value::Error(_) => {}
            }
        }

        let mut result = Vec::new();

        // Sum all currencies, converting to the last used currency
        if !currency_amounts.is_empty() {
            let target_currency = currency_amounts.last().unwrap().0;
            let mut total_in_target = Decimal::ZERO;

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

            if !total_in_target.is_zero() {
                result.push(Value::Currency {
                    amount: total_in_target,
                    currency: target_currency,
                });
            }
        }

        // Group units by type and convert to last used unit of that type
        let mut unit_by_type: HashMap<types::UnitType, Decimal> = HashMap::new();
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

            *unit_by_type.entry(unit_type).or_insert(Decimal::ZERO) += converted;
        }

        // Add unit totals (one per unit type, using last used unit)
        for (unit_type, amount) in unit_by_type {
            if !amount.is_zero() {
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

    /// Get all user-defined variables (excludes 'total', '_', 'ANS', and 'ans')
    pub fn variables(&self) -> Vec<(String, Value)> {
        self.context
            .variables
            .iter()
            .filter(|(name, _)| {
                *name != "total" && *name != "_" && *name != "ANS" && *name != "ans"
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    /// Set an exchange rate
    pub fn set_exchange_rate(&mut self, from: Currency, to: Currency, rate: Decimal) {
        self.context.set_exchange_rate(from, to, rate);
    }

    /// Set an exchange rate from f64 (convenience for API compatibility)
    pub fn set_exchange_rate_f64(&mut self, from: Currency, to: Currency, rate: f64) {
        if let Some(decimal_rate) = Decimal::from_f64(rate) {
            self.context.set_exchange_rate(from, to, decimal_rate);
        }
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
    use std::str::FromStr;

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
        engine.set_exchange_rate(
            Currency::USD,
            Currency::EUR,
            Decimal::from_str("0.92").unwrap(),
        );

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
        let expected_eur = Decimal::from(338);
        assert!(totals.iter().any(|v| matches!(v,
            Value::Currency { amount, currency }
            if *currency == Currency::EUR && (*amount - expected_eur).abs() < Decimal::ONE
        )));

        // Length: 1000 m + 5 km = 1 km + 5 km = 6 km (last unit is km)
        let expected_km = Decimal::from(6);
        let tolerance = Decimal::from_str("0.01").unwrap();
        assert!(totals.iter().any(|v| matches!(v,
            Value::WithUnit { amount, unit }
            if *unit == Unit::Kilometer && (*amount - expected_km).abs() < tolerance
        )));
    }

    #[test]
    fn test_grouped_totals_last_currency() {
        let mut engine = Engine::new();
        engine.set_exchange_rate(
            Currency::USD,
            Currency::EUR,
            Decimal::from_str("0.92").unwrap(),
        );

        engine.eval("€100");
        engine.eval("$50"); // Last is USD, so total in USD

        let totals = engine.grouped_totals();

        // €100 = $108.70 (100 / 0.92) + $50 = $158.70
        let expected_usd = Decimal::from_str("158.70").unwrap();
        assert!(totals.iter().any(|v| matches!(v,
            Value::Currency { amount, currency }
            if *currency == Currency::USD && (*amount - expected_usd).abs() < Decimal::ONE
        )));
    }

    #[test]
    fn test_continuation_basic() {
        let mut engine = Engine::new();
        assert_eq!(engine.eval("10").as_f64(), Some(10.0));
        assert_eq!(engine.eval("+ 5").as_f64(), Some(15.0));
        assert_eq!(engine.eval("* 2").as_f64(), Some(30.0));
        assert_eq!(engine.eval("/ 3").as_f64(), Some(10.0));
        assert_eq!(engine.eval("- 2").as_f64(), Some(8.0));
    }

    #[test]
    fn test_continuation_with_currency() {
        let mut engine = Engine::new();
        let result = engine.eval("$100");
        assert!(matches!(result, Value::Currency { .. }));

        let result = engine.eval("- 10");
        assert!(matches!(result, Value::Currency { amount, currency }
            if currency == Currency::USD && (amount - Decimal::from(90)).abs() < Decimal::ONE
        ));

        let result = engine.eval("+ $20");
        assert!(matches!(result, Value::Currency { amount, currency }
            if currency == Currency::USD && (amount - Decimal::from(110)).abs() < Decimal::ONE
        ));
    }

    #[test]
    fn test_continuation_conversion() {
        let mut engine = Engine::new();
        engine.set_exchange_rate(
            Currency::USD,
            Currency::EUR,
            Decimal::from_str("0.92").unwrap(),
        );

        engine.eval("$100");
        let result = engine.eval("in EUR");
        assert!(matches!(result, Value::Currency { amount, currency }
            if currency == Currency::EUR && (amount - Decimal::from(92)).abs() < Decimal::ONE
        ));
    }

    #[test]
    fn test_continuation_unit_conversion() {
        let mut engine = Engine::new();
        engine.eval("5 km");
        let result = engine.eval("to m");
        assert!(matches!(result, Value::WithUnit { amount, unit }
            if unit == Unit::Meter && (amount - Decimal::from(5000)).abs() < Decimal::ONE
        ));
    }

    #[test]
    fn test_underscore_variable() {
        let mut engine = Engine::new();
        engine.eval("42");
        assert_eq!(engine.eval("_ + 8").as_f64(), Some(50.0));
        assert_eq!(engine.eval("_ * 2").as_f64(), Some(100.0));
    }

    #[test]
    fn test_ans_alias() {
        let mut engine = Engine::new();
        engine.eval("42");
        // ANS works same as _
        assert_eq!(engine.eval("ANS + 8").as_f64(), Some(50.0));
        assert_eq!(engine.eval("ans * 2").as_f64(), Some(100.0)); // case insensitive
        // Can assign from ANS
        engine.eval("result = ANS");
        assert_eq!(engine.eval("result").as_f64(), Some(100.0));
    }

    #[test]
    fn test_continuation_negative_vs_subtract() {
        let mut engine = Engine::new();

        // Negative number (no previous result)
        assert_eq!(engine.eval("-5").as_f64(), Some(-5.0));

        // With previous result, continuation wins (subtraction)
        engine.clear();
        engine.eval("100");
        assert_eq!(engine.eval("-5").as_f64(), Some(95.0)); // 100 - 5

        // Same with space: no distinction
        engine.clear();
        engine.eval("100");
        assert_eq!(engine.eval("- 5").as_f64(), Some(95.0)); // 100 - 5
    }

    #[test]
    fn test_continuation_skips_empty() {
        let mut engine = Engine::new();
        engine.eval("100");
        engine.eval(""); // Empty line
        engine.eval("# comment"); // Comment (evaluates to Empty)
        assert_eq!(engine.eval("+ 50").as_f64(), Some(150.0));
    }

    #[test]
    fn test_continuation_power() {
        let mut engine = Engine::new();
        engine.eval("2");
        assert_eq!(engine.eval("^ 10").as_f64(), Some(1024.0));
    }

    #[test]
    fn test_continuation_totals_not_double_counted() {
        let mut engine = Engine::new();
        engine.eval("100");
        engine.eval("- 5"); // Marks 100 as consumed, result is 95

        // Sum should be 95, not 195
        assert_eq!(engine.sum().as_f64(), Some(95.0));
    }

    #[test]
    fn test_continuation_chain_totals() {
        let mut engine = Engine::new();
        engine.eval("$100");
        engine.eval("+ $50"); // 150, marks 100 as consumed
        engine.eval("* 2"); // 300, marks 150 as consumed

        let totals = engine.grouped_totals();
        assert_eq!(totals.len(), 1);

        // Should only have $300, not $100 + $150 + $300 = $550
        assert!(matches!(&totals[0],
            Value::Currency { amount, currency }
            if *currency == Currency::USD && (*amount - Decimal::from(300)).abs() < Decimal::ONE
        ));
    }

    #[test]
    fn test_continuation_source_flag() {
        let mut engine = Engine::new();
        engine.eval("100");
        engine.eval("+ 50");

        let lines = engine.lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].is_continuation_source); // 100 was consumed
        assert!(!lines[1].is_continuation_source); // 150 is final
    }

    #[test]
    fn test_standalone_values_not_consumed() {
        let mut engine = Engine::new();
        engine.eval("100");
        engine.eval("200"); // Standalone number, not continuation

        let lines = engine.lines();
        // "_ 200" doesn't parse as valid expression, so falls back to "200"
        // 100 should NOT be marked as consumed
        assert!(!lines[0].is_continuation_source);
        assert!(!lines[1].is_continuation_source);

        // Sum should be 300
        assert_eq!(engine.sum().as_f64(), Some(300.0));
    }

    // =========================================================================
    // REAL-WORLD CONTINUATION TOTALS TESTS
    // These mirror actual usage patterns from example.numr
    // =========================================================================

    /// Freelance calculation: hours * rate, then subtract tax
    /// Pattern: techcorp_hours * techcorp_rate, then - 25%
    #[test]
    fn test_freelance_calculation_totals() {
        let mut engine = Engine::new();

        // techcorp: 45h * $85 = $3825 gross
        engine.eval("techcorp_hours = 45"); // Plain number, not in currency totals
        engine.eval("techcorp_rate = 85 usd"); // $85 - standalone, counted
        engine.eval("techcorp_gross = techcorp_hours * techcorp_rate"); // $3825
        engine.eval("- 25%"); // Tax deduction: $3825 - 25% = $2868.75, consumes gross
        engine.eval("techcorp_net = _"); // $2868.75, consumes previous

        let totals = engine.grouped_totals();

        // Totals include:
        // - techcorp_rate ($85) - standalone rate definition, used via variable
        // - techcorp_net ($2868.75) - final result after continuation chain
        // NOT included:
        // - techcorp_gross ($3825) - consumed by "- 25%"
        // - "- 25%" intermediate ($2868.75) - consumed by "techcorp_net = _"
        assert_eq!(totals.len(), 1);
        let amount = totals[0].as_decimal().unwrap();
        let expected = Decimal::from(85) + Decimal::from_str("2868.75").unwrap();
        assert!(
            (amount - expected).abs() < Decimal::ONE,
            "Expected ~$2953.75, got {amount}"
        );
    }

    /// Simple addition chain: $2200 + $400
    /// Pattern from example.numr: startup income
    #[test]
    fn test_simple_addition_chain_totals() {
        let mut engine = Engine::new();

        engine.eval("$2200");
        engine.eval("+ $400");
        engine.eval("startup_total = _");

        let totals = engine.grouped_totals();

        // Should only count startup_total ($2600)
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].as_decimal(), Some(Decimal::from(2600)));
    }

    /// SaaS annual calculation: monthly * 12
    #[test]
    fn test_multiply_chain_totals() {
        let mut engine = Engine::new();

        engine.eval("saas_mrr = 340 usd");
        engine.eval("* 12");
        engine.eval("saas_annual = _");

        let totals = engine.grouped_totals();

        // Should only count saas_annual ($4080)
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].as_decimal(), Some(Decimal::from(4080)));
    }

    /// Hosting costs: multiple additions then multiply
    /// Pattern: 127 usd + 48 usd * 12
    #[test]
    fn test_multi_step_chain_totals() {
        let mut engine = Engine::new();

        engine.eval("127 usd");
        engine.eval("+ 48 usd"); // = 175
        engine.eval("* 12"); // = 2100
        engine.eval("hosting_annual = _");

        let totals = engine.grouped_totals();

        // Should only count hosting_annual ($2100)
        // Not: 127 + 175 + 2100 + 2100 = 4502
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].as_decimal(), Some(Decimal::from(2100)));
    }

    /// Multiple independent values plus one chain
    /// Some lines are standalone, some are continuations
    #[test]
    fn test_mixed_standalone_and_continuation() {
        let mut engine = Engine::new();

        // Standalone values
        engine.eval("rent = 1850 usd");
        engine.eval("gym = 50 usd");

        // Chain
        engine.eval("$100");
        engine.eval("+ $50"); // = 150
        engine.eval("bonus = _");

        // Another standalone
        engine.eval("groceries = 200 usd");

        let totals = engine.grouped_totals();

        // Total should be: rent(1850) + gym(50) + bonus(150) + groceries(200) = 2250
        // NOT: 1850 + 50 + 100 + 150 + 150 + 200 = 2500
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].as_decimal(), Some(Decimal::from(2250)));
    }

    /// Job offer calculation: base + bonus - tax
    /// Pattern from example.numr
    #[test]
    fn test_job_offer_calculation() {
        let mut engine = Engine::new();
        engine.set_exchange_rate(Currency::USD, Currency::RUB, Decimal::from(92));

        engine.eval("85000 rub"); // Base salary
        engine.eval("+ 15%"); // Plus 15% bonus = 97750
        engine.eval("- 13%"); // Minus 13% tax = 85042.5
        engine.eval("take_home = _");

        let totals = engine.grouped_totals();

        // Should only count take_home
        assert_eq!(totals.len(), 1);
        let amount = totals[0].as_decimal().unwrap();
        assert!(
            (amount - Decimal::from_str("85042.5").unwrap()).abs() < Decimal::ONE,
            "Expected ~85042.5 RUB, got {amount}"
        );
    }

    /// Unit conversion continuation: 5 km, to miles
    #[test]
    fn test_unit_conversion_continuation_totals() {
        let mut engine = Engine::new();

        engine.eval("flight = 9500 km");
        engine.eval("to miles");

        let totals = engine.grouped_totals();

        // Should only count the miles result, not km + miles
        assert_eq!(totals.len(), 1);
        let amount = totals[0].as_decimal().unwrap();
        // 9500 km ≈ 5903 miles
        assert!(
            (amount - Decimal::from_str("5903.4").unwrap()).abs() < Decimal::from(1),
            "Expected ~5903 miles, got {amount}"
        );
    }

    /// Currency conversion continuation: $100 in EUR
    #[test]
    fn test_currency_conversion_continuation_totals() {
        let mut engine = Engine::new();
        engine.set_exchange_rate(
            Currency::USD,
            Currency::EUR,
            Decimal::from_str("0.92").unwrap(),
        );

        engine.eval("$100");
        engine.eval("in EUR");

        let totals = engine.grouped_totals();

        // Should only count EUR result, not $100 + €92
        assert_eq!(totals.len(), 1);
        assert!(matches!(&totals[0],
            Value::Currency { amount, currency }
            if *currency == Currency::EUR && (*amount - Decimal::from(92)).abs() < Decimal::ONE
        ));
    }

    /// Multiple separate chains don't interfere
    #[test]
    fn test_multiple_separate_chains() {
        let mut engine = Engine::new();

        // First chain
        engine.eval("$100");
        engine.eval("+ $50");
        engine.eval("first = _"); // = 150

        // Second chain (separate)
        engine.eval("$200");
        engine.eval("* 2");
        engine.eval("second = _"); // = 400

        let totals = engine.grouped_totals();

        // Should be first(150) + second(400) = 550
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].as_decimal(), Some(Decimal::from(550)));
    }

    /// Percentage operations in chain
    #[test]
    fn test_percentage_chain_totals() {
        let mut engine = Engine::new();

        engine.eval("$1000");
        engine.eval("+ 10%"); // = 1100
        engine.eval("- 5%"); // = 1045
        engine.eval("final = _");

        let totals = engine.grouped_totals();

        // Should only count final ($1045)
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].as_decimal(), Some(Decimal::from(1045)));
    }
}
