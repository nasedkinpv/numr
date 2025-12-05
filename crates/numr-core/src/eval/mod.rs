//! Expression evaluation engine

use std::collections::HashMap;

use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;

use crate::cache::RateCache;
use crate::parser::{Ast, BinaryOp, Expr};
use crate::types::{unit, Currency, Unit, Value};

/// Evaluation context with variables and rates
#[derive(Clone)]
pub struct EvalContext {
    pub(crate) variables: HashMap<String, Value>,
    pub(crate) rate_cache: RateCache,
}

impl EvalContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            rate_cache: RateCache::default(),
        }
    }

    /// Set exchange rates (for testing or offline mode)
    pub fn set_exchange_rate(&mut self, from: Currency, to: Currency, rate: Decimal) {
        self.rate_cache.set_rate(from, to, rate);
    }

    /// Get a variable value
    pub fn get_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    /// Set a variable
    pub fn set_variable(&mut self, name: String, value: Value) {
        self.variables.insert(name, value);
    }

    /// Clear all variables
    pub fn clear_variables(&mut self) {
        self.variables.clear();
    }
}

impl Default for EvalContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluate an AST node
pub fn evaluate(ast: &Ast, ctx: &mut EvalContext) -> Value {
    match ast {
        Ast::Empty => Value::Empty,
        Ast::Assignment { name, expr } => {
            let value = eval_expr(expr, ctx);
            if !value.is_error() {
                ctx.set_variable(name.clone(), value.clone());
            }
            value
        }
        Ast::Expression(expr) => eval_expr(expr, ctx),
    }
}

fn eval_expr(expr: &Expr, ctx: &EvalContext) -> Value {
    match expr {
        Expr::Number(n) => Value::Number(*n),
        Expr::Percentage(p) => Value::Percentage(*p),
        Expr::Currency { amount, currency } => Value::currency(*amount, *currency),
        Expr::WithUnit { amount, unit } => Value::with_unit(*amount, *unit),

        Expr::Variable(name) => ctx
            .get_variable(name)
            .cloned()
            .unwrap_or_else(|| Value::Error(format!("Unknown variable: {name}"))),

        Expr::BinaryOp { op, left, right } => {
            let lval = eval_expr(left, ctx);
            let rval = eval_expr(right, ctx);
            eval_binary_op(*op, lval, rval, ctx)
        }

        Expr::PercentageOf { percentage, value } => {
            let val = eval_expr(value, ctx);
            match val {
                Value::Number(n) => Value::Number(n * percentage),
                Value::Currency { amount, currency } => {
                    Value::currency(amount * percentage, currency)
                }
                Value::WithUnit { amount, unit } => Value::with_unit(amount * percentage, unit),
                _ => Value::Error("Cannot calculate percentage of this value".to_string()),
            }
        }

        Expr::Conversion { value, target_unit } => {
            let val = eval_expr(value, ctx);
            eval_conversion(val, target_unit, ctx)
        }

        Expr::FunctionCall { name, args } => {
            let evaluated_args: Vec<Value> = args.iter().map(|a| eval_expr(a, ctx)).collect();
            eval_function(name, &evaluated_args)
        }
    }
}

fn eval_binary_op(op: BinaryOp, left: Value, right: Value, ctx: &EvalContext) -> Value {
    // Handle percentage operations (e.g., 100 + 20% = 120)
    if let Some(result) = try_percentage_op(op, &left, &right) {
        return result;
    }

    // Handle special multiplication cases (unit × currency, etc.)
    if op == BinaryOp::Multiply {
        if let Some(result) = try_multiply_mixed(&left, &right) {
            return result;
        }
    }

    // Coerce operands to compatible types
    let (l_val, r_val, result_type) = match coerce_operands(&left, &right, op, ctx) {
        Ok(vals) => vals,
        Err(e) => return Value::Error(e),
    };

    // Perform the arithmetic operation
    let result = match apply_op(op, l_val, r_val) {
        Ok(r) => r,
        Err(e) => return Value::Error(e),
    };

    // Wrap result in appropriate type
    match result_type {
        ResultType::Currency(c) => Value::currency(result, c),
        ResultType::Unit(u) => Value::with_unit(result, u),
        ResultType::Number => Value::Number(result),
    }
}

/// Try to handle percentage operations (e.g., 100 + 20% = 120)
fn try_percentage_op(op: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    let Value::Percentage(p) = right else {
        return None;
    };
    let base = left.as_decimal()?;

    Some(match op {
        BinaryOp::Add => left.with_scaled_amount(base * (Decimal::ONE + p)),
        BinaryOp::Subtract => left.with_scaled_amount(base * (Decimal::ONE - p)),
        BinaryOp::Multiply => Value::Number(base * p),
        BinaryOp::Divide if p.is_zero() => Value::Error("Division by zero".to_string()),
        BinaryOp::Divide => Value::Number(base / p),
        BinaryOp::Power => Value::Number(base.powd(*p)),
    })
}

/// Try to handle mixed-type multiplication (unit × currency, currency × number)
fn try_multiply_mixed(left: &Value, right: &Value) -> Option<Value> {
    match (left, right) {
        // unit × currency → currency (e.g., 45h * $85 = $3825)
        (
            Value::WithUnit { amount: l, .. },
            Value::Currency {
                amount: r,
                currency,
            },
        )
        | (
            Value::Currency {
                amount: l,
                currency,
            },
            Value::WithUnit { amount: r, .. },
        ) => Some(Value::currency(l * r, *currency)),
        // currency × number → currency
        (Value::Currency { amount, currency }, Value::Number(n))
        | (Value::Number(n), Value::Currency { amount, currency }) => {
            Some(Value::currency(amount * n, *currency))
        }
        _ => None,
    }
}

/// Result type for binary operations
enum ResultType {
    Currency(Currency),
    Unit(Unit),
    Number,
}

/// Coerce operands to compatible decimal values, returning result type
fn coerce_operands(
    left: &Value,
    right: &Value,
    op: BinaryOp,
    ctx: &EvalContext,
) -> Result<(Decimal, Decimal, ResultType), String> {
    match (left, right) {
        // Same currency
        (
            Value::Currency {
                amount: l,
                currency: lc,
            },
            Value::Currency {
                amount: r,
                currency: rc,
            },
        ) => {
            if lc == rc {
                Ok((*l, *r, ResultType::Currency(*lc)))
            } else if let Some(rate) = ctx.rate_cache.get_rate(*rc, *lc) {
                Ok((*l, *r * rate, ResultType::Currency(*lc)))
            } else {
                Err(format!("No exchange rate for {rc} to {lc}"))
            }
        }

        // Same unit type
        (
            Value::WithUnit {
                amount: l,
                unit: lu,
            },
            Value::WithUnit {
                amount: r,
                unit: ru,
            },
        ) => {
            if lu == ru {
                Ok((*l, *r, ResultType::Unit(*lu)))
            } else if let Some(converted) = unit::convert(*r, *ru, *lu) {
                Ok((*l, converted, ResultType::Unit(*lu)))
            } else {
                Err(format!("Cannot convert {ru} to {lu}"))
            }
        }

        // Unit + Currency: incompatible
        (Value::WithUnit { .. }, Value::Currency { .. })
        | (Value::Currency { .. }, Value::WithUnit { .. }) => {
            if matches!(op, BinaryOp::Add | BinaryOp::Subtract) {
                Err("Cannot add/subtract units and currency".to_string())
            } else {
                Err("Invalid operands".to_string())
            }
        }

        // Number + Currency: propagate currency
        (
            Value::Number(l),
            Value::Currency {
                amount: r,
                currency,
            },
        )
        | (
            Value::Currency {
                amount: l,
                currency,
            },
            Value::Number(r),
        ) => Ok((*l, *r, ResultType::Currency(*currency))),

        // Number + Unit: propagate unit
        (Value::Number(l), Value::WithUnit { amount: r, unit })
        | (Value::WithUnit { amount: l, unit }, Value::Number(r)) => {
            Ok((*l, *r, ResultType::Unit(*unit)))
        }

        // Plain numbers
        _ => match (left.as_decimal(), right.as_decimal()) {
            (Some(l), Some(r)) => Ok((l, r, ResultType::Number)),
            _ => Err("Invalid operands".to_string()),
        },
    }
}

/// Apply arithmetic operation
fn apply_op(op: BinaryOp, l: Decimal, r: Decimal) -> Result<Decimal, String> {
    match op {
        BinaryOp::Add => Ok(l + r),
        BinaryOp::Subtract => Ok(l - r),
        BinaryOp::Multiply => Ok(l * r),
        BinaryOp::Divide if r.is_zero() => Err("Division by zero".to_string()),
        BinaryOp::Divide => Ok(l / r),
        BinaryOp::Power => Ok(l.powd(r)),
    }
}

fn eval_conversion(value: Value, target: &str, ctx: &EvalContext) -> Value {
    // Try as currency first
    if let Some(target_currency) = Currency::parse(target) {
        if let Value::Currency { amount, currency } = value {
            if currency == target_currency {
                return Value::currency(amount, target_currency);
            }
            if let Some(rate) = ctx.rate_cache.get_rate(currency, target_currency) {
                return Value::currency(amount * rate, target_currency);
            }
            return Value::Error(format!(
                "No exchange rate for {currency} to {target_currency}"
            ));
        }
    }

    // Try as unit
    if let Some(target_unit) = Unit::parse(target) {
        match value {
            Value::WithUnit {
                amount,
                unit: from_unit,
            } => {
                if let Some(converted) = unit::convert(amount, from_unit, target_unit) {
                    return Value::with_unit(converted, target_unit);
                }
                return Value::Error(format!("Cannot convert {from_unit} to {target_unit}"));
            }
            // Plain number → attach unit (e.g., "18.39 in months" → "18.39 months")
            Value::Number(n) => {
                return Value::with_unit(n, target_unit);
            }
            // Currency ratio → attach unit (e.g., "usd/usd in months" → dimensionless with unit)
            Value::Currency { amount, .. } => {
                return Value::with_unit(amount, target_unit);
            }
            _ => {}
        }
    }

    Value::Error(format!("Unknown target unit: {target}"))
}

fn eval_function(name: &str, args: &[Value]) -> Value {
    // Helper for single-number functions
    let require_number = |f: fn(Decimal) -> Value| -> Value {
        args.first()
            .and_then(|v| v.as_decimal())
            .map(f)
            .unwrap_or_else(|| Value::Error(format!("{name} requires a number")))
    };

    // Helper to get all numeric values
    let numbers = || args.iter().filter_map(|v| v.as_decimal());

    match name.to_lowercase().as_str() {
        // Aggregate functions
        "sum" | "total" => Value::Number(numbers().sum()),

        "avg" | "average" => {
            let vals: Vec<_> = numbers().collect();
            if vals.is_empty() {
                Value::Number(Decimal::ZERO)
            } else {
                Value::Number(vals.iter().sum::<Decimal>() / Decimal::from(vals.len()))
            }
        }

        "min" => numbers()
            .min()
            .map(Value::Number)
            .unwrap_or_else(|| Value::Error("No values for min".to_string())),

        "max" => numbers()
            .max()
            .map(Value::Number)
            .unwrap_or_else(|| Value::Error("No values for max".to_string())),

        // Single-value math functions
        "abs" => require_number(|n| Value::Number(n.abs())),
        "round" => require_number(|n| Value::Number(n.round())),
        "floor" => require_number(|n| Value::Number(n.floor())),
        "ceil" => require_number(|n| Value::Number(n.ceil())),

        "sqrt" => require_number(|n| {
            if n.is_sign_negative() {
                Value::Error("Cannot take sqrt of negative number".to_string())
            } else {
                Value::Number(n.sqrt().unwrap_or(Decimal::ZERO))
            }
        }),

        _ => Value::Error(format!("Unknown function: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;

    fn eval_str(input: &str) -> Value {
        let mut ctx = EvalContext::new();
        let ast = parse_line(input).unwrap();
        evaluate(&ast, &mut ctx)
    }

    #[test]
    fn test_basic_arithmetic() {
        assert_eq!(eval_str("10 + 20").as_f64(), Some(30.0));
        assert_eq!(eval_str("100 - 25").as_f64(), Some(75.0));
        assert_eq!(eval_str("6 * 7").as_f64(), Some(42.0));
        assert_eq!(eval_str("100 / 4").as_f64(), Some(25.0));
    }

    #[test]
    fn test_percentage_of() {
        assert_eq!(eval_str("20% of 150").as_f64(), Some(30.0));
    }
}
