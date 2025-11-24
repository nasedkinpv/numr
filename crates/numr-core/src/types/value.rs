//! Core value representation

use super::{Currency, Unit};
use serde::{Deserialize, Serialize};

/// A computed value with optional unit/currency
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Plain number
    Number(f64),
    /// Percentage (stored as decimal, e.g., 0.20 for 20%)
    Percentage(f64),
    /// Value with currency
    Currency { amount: f64, currency: Currency },
    /// Value with unit
    WithUnit { amount: f64, unit: Unit },
    /// No value (empty line or comment)
    Empty,
    /// Error during evaluation
    Error(String),
}

impl Value {
    /// Create a new number value
    pub fn number(n: f64) -> Self {
        Value::Number(n)
    }

    /// Create a new percentage value (input as percentage, e.g., 20 for 20%)
    pub fn percentage(p: f64) -> Self {
        Value::Percentage(p / 100.0)
    }

    /// Create a currency value
    pub fn currency(amount: f64, currency: Currency) -> Self {
        Value::Currency { amount, currency }
    }

    /// Create a value with unit
    pub fn with_unit(amount: f64, unit: Unit) -> Self {
        Value::WithUnit { amount, unit }
    }

    /// Get the numeric value, ignoring units
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Percentage(p) => Some(*p),
            Value::Currency { amount, .. } => Some(*amount),
            Value::WithUnit { amount, .. } => Some(*amount),
            Value::Empty | Value::Error(_) => None,
        }
    }

    /// Check if value is empty
    pub fn is_empty(&self) -> bool {
        matches!(self, Value::Empty)
    }

    /// Check if value is an error
    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", format_number(*n)),
            Value::Percentage(p) => write!(f, "{}%", format_number(p * 100.0)),
            Value::Currency { amount, currency } => {
                let formatted = format_currency(*amount);
                if currency.symbol_after() {
                    write!(f, "{}{}", formatted, currency.symbol())
                } else {
                    write!(f, "{}{}", currency.symbol(), formatted)
                }
            }
            Value::WithUnit { amount, unit } => {
                write!(f, "{} {}", format_number(*amount), unit)
            }
            Value::Empty => Ok(()),
            Value::Error(msg) => write!(f, "Error: {msg}"),
        }
    }
}

/// Format a number nicely (max 2 decimal places, remove trailing zeros)
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        // Round to 2 decimal places
        let rounded = (n * 100.0).round() / 100.0;
        let s = format!("{rounded:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Format currency amount (max 2 decimal places)
fn format_currency(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        let rounded = (n * 100.0).round() / 100.0;
        let s = format!("{rounded:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_format_number() {
        assert_eq!(format_number(42.0), "42");
        assert_eq!(format_number(3.14), "3.14");
        assert_eq!(format_number(100.500), "100.5");
    }
}
