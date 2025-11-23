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

pub use eval::EvalContext;
pub use parser::{parse_line, Ast, BinaryOp, Expr};
pub use types::{Currency, CurrencyDef, Unit, UnitDef, UnitType, Value, CURRENCIES, UNITS};

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

    /// Get the sum of all computed values
    pub fn sum(&self) -> Value {
        let total: f64 = self.lines.iter().filter_map(|lr| lr.value.as_f64()).sum();
        Value::Number(total)
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
}
