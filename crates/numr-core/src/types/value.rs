//! Core value representation

use super::{CompoundUnit, Currency};
use crate::EvalError;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Number of decimal places for display formatting
const DISPLAY_PRECISION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberBase {
    Binary,
    Hexadecimal,
}

impl NumberBase {
    pub fn parse(target: &str) -> Option<Self> {
        match target.to_ascii_lowercase().as_str() {
            "bin" | "binary" => Some(Self::Binary),
            "hex" | "hexadecimal" => Some(Self::Hexadecimal),
            _ => None,
        }
    }
}

/// A computed value with optional unit/currency
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Value {
    /// Plain number
    Number(Decimal),
    /// Plain number displayed in a specific numeric base
    BaseNumber { amount: Decimal, base: NumberBase },
    /// Percentage (stored as decimal, e.g., 0.20 for 20%)
    Percentage(Decimal),
    /// Value with currency
    Currency { amount: Decimal, currency: Currency },
    /// Value with a physical unit (simple or compound, e.g., km, m², km/h)
    WithCompoundUnit { amount: Decimal, unit: CompoundUnit },
    /// No value (empty line or comment)
    Empty,
    /// Error during evaluation
    Error(EvalError),
}

impl Value {
    /// Create a new number value displayed in a specific numeric base
    pub fn with_base(amount: Decimal, base: NumberBase) -> Self {
        Value::BaseNumber { amount, base }
    }

    /// Create a currency value
    pub fn currency(amount: Decimal, currency: Currency) -> Self {
        Value::Currency { amount, currency }
    }

    /// Create a value with compound unit
    pub fn with_compound_unit(amount: Decimal, unit: CompoundUnit) -> Self {
        Value::WithCompoundUnit { amount, unit }
    }

    /// Create a typed evaluation error from a compatibility message.
    pub fn error(error: impl Into<EvalError>) -> Self {
        Value::Error(error.into())
    }

    /// Get the numeric value as Decimal, ignoring units
    pub fn as_decimal(&self) -> Option<Decimal> {
        match self {
            Value::Number(n) => Some(*n),
            Value::BaseNumber { amount, .. } => Some(*amount),
            Value::Percentage(p) => Some(*p),
            Value::Currency { amount, .. } => Some(*amount),
            Value::WithCompoundUnit { amount, .. } => Some(*amount),
            Value::Empty | Value::Error(_) => None,
        }
    }

    /// Get the numeric value as f64 (for backwards compatibility)
    pub fn as_f64(&self) -> Option<f64> {
        use rust_decimal::prelude::ToPrimitive;
        self.as_decimal().and_then(|d| d.to_f64())
    }

    /// Check if value is empty
    pub fn is_empty(&self) -> bool {
        matches!(self, Value::Empty)
    }

    /// Check if value is an error
    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }

    /// Return the structured error, if this value represents one.
    #[must_use]
    pub fn as_error(&self) -> Option<&EvalError> {
        match self {
            Value::Error(error) => Some(error),
            _ => None,
        }
    }

    /// Return a new value with the same type but different amount
    /// Used for percentage operations that preserve the value type
    pub fn with_scaled_amount(&self, new_amount: Decimal) -> Value {
        match self {
            Value::Currency { currency, .. } => Value::currency(new_amount, *currency),
            Value::WithCompoundUnit { unit, .. } => {
                Value::with_compound_unit(new_amount, unit.clone())
            }
            _ => Value::Number(new_amount),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", format_number(*n)),
            Value::BaseNumber { amount, base } => {
                write!(f, "{}", format_number_base(*amount, *base))
            }
            Value::Percentage(p) => match p.checked_mul(Decimal::from(100)) {
                Some(percent) => write!(f, "{}%", format_number(percent)),
                // Programmatic callers can construct a percentage outside the
                // representable display range. Formatting must remain total.
                None => write!(f, "{}×100%", format_number(*p)),
            },
            Value::Currency { amount, currency } => {
                let formatted = format_currency_value(*amount, *currency);
                if currency.symbol_after() {
                    write!(f, "{}{}", formatted, currency.symbol())
                } else {
                    write!(f, "{}{}", currency.symbol(), formatted)
                }
            }
            Value::WithCompoundUnit { amount, unit } => {
                if unit.symbol == "°" {
                    write!(f, "{}°", format_number(*amount))
                } else {
                    write!(f, "{} {}", format_number(*amount), unit)
                }
            }
            Value::Empty => Ok(()),
            Value::Error(msg) => write!(f, "Error: {msg}"),
        }
    }
}

/// Format a number nicely (max DISPLAY_PRECISION decimal places, unless that would hide a small non-zero value)
pub fn format_number(n: Decimal) -> String {
    let rounded = n.round_dp(DISPLAY_PRECISION);
    if rounded.is_zero() && !n.is_zero() {
        return n.normalize().to_string();
    }

    if rounded.fract().is_zero() {
        format!("{}", rounded.trunc())
    } else {
        format!("{:.prec$}", rounded, prec = DISPLAY_PRECISION as usize)
    }
}

fn format_number_base(n: Decimal, base: NumberBase) -> String {
    use rust_decimal::prelude::ToPrimitive;

    let Some(as_i128) = n.to_i128() else {
        return format_number(n);
    };

    let prefix = match base {
        NumberBase::Binary => "0b",
        NumberBase::Hexadecimal => "0x",
    };
    let magnitude = as_i128.unsigned_abs();
    let digits = match base {
        NumberBase::Binary => format!("{magnitude:b}"),
        NumberBase::Hexadecimal => format!("{magnitude:x}"),
    };

    if as_i128.is_negative() {
        format!("-{prefix}{digits}")
    } else {
        format!("{prefix}{digits}")
    }
}

/// Format currency amount using display rules from the currency registry.
pub fn format_currency_value(n: Decimal, currency: Currency) -> String {
    format_currency_with_precision(n, currency.display_precision())
}

/// Format currency amount using the requested precision.
/// Extra trailing zeros are trimmed for higher-precision currencies, but at least
/// two decimal places are preserved for readability.
fn format_currency_with_precision(n: Decimal, precision: u32) -> String {
    let mut formatted = format!(
        "{:.prec$}",
        n.round_dp(precision),
        prec = precision as usize
    );

    if precision > DISPLAY_PRECISION {
        if let Some(dot) = formatted.find('.') {
            while formatted.ends_with('0') && formatted.len() - dot - 1 > DISPLAY_PRECISION as usize
            {
                formatted.pop();
            }
        }
    }

    formatted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(Decimal::from(42)), "42");
        assert_eq!(format_number(Decimal::from_str("3.14").unwrap()), "3.14");
        assert_eq!(format_number(Decimal::from_str("0.001").unwrap()), "0.001");
        assert_eq!(
            format_number(Decimal::from_str("100.500").unwrap()),
            "100.50"
        );
    }

    #[test]
    fn test_format_number_base() {
        assert_eq!(
            Value::with_base(Decimal::from(22), NumberBase::Hexadecimal).to_string(),
            "0x16"
        );
        assert_eq!(
            Value::with_base(Decimal::from(22), NumberBase::Binary).to_string(),
            "0b10110"
        );
        assert_eq!(
            Value::with_base(Decimal::from(-10), NumberBase::Hexadecimal).to_string(),
            "-0xa"
        );
    }

    #[test]
    fn test_format_currency_with_precision() {
        assert_eq!(format_currency_with_precision(Decimal::from(1), 8), "1.00");
        assert_eq!(
            format_currency_with_precision(Decimal::from_str("0.1").unwrap(), 8),
            "0.10"
        );
        assert_eq!(
            format_currency_with_precision(Decimal::from_str("0.04").unwrap(), 8),
            "0.04"
        );
        assert_eq!(
            format_currency_with_precision(Decimal::from_str("0.0042105263").unwrap(), 8),
            "0.00421053"
        );
    }

    #[test]
    fn percentage_formatting_never_overflows() {
        assert_eq!(
            Value::Percentage(Decimal::MAX).to_string(),
            format!("{}×100%", Decimal::MAX)
        );
    }
}
