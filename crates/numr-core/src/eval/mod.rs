//! Expression evaluation engine

use std::collections::HashMap;

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;

use crate::cache::RateCache;
use crate::parser::{Ast, BinaryOp, Expr};
use crate::types::{unit, Currency, NumberBase, Unit, Value};

/// Evaluation context with variables and rates
#[derive(Clone)]
pub struct EvalContext {
    pub(crate) variables: HashMap<String, Value>,
    pub(crate) rate_cache: RateCache,
}

impl EvalContext {
    #[must_use]
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
    #[must_use]
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
        Expr::WithCompoundUnit { amount, unit } => Value::with_compound_unit(*amount, unit.clone()),

        Expr::Variable(name) => ctx
            .get_variable(name)
            .cloned()
            .or_else(|| math_constant(name))
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
                Value::WithCompoundUnit { amount, unit } => {
                    Value::with_compound_unit(amount * percentage, unit)
                }
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

    // Handle compound unit operations (multiply, divide, add, subtract)
    // e.g., 5m * 10m = 50 m², 100km / 2h = 50 km/h, 12 m² + 15 m² = 27 m²
    if let Some(result) = try_unit_compound_op(op, &left, &right) {
        return result;
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
        ResultType::Percentage => Value::Percentage(result),
        ResultType::Number => Value::Number(result),
    }
}

/// Try to handle percentage operations (e.g., 100 + 20% = 120)
fn try_percentage_op(op: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    let Value::Percentage(p) = right else {
        return None;
    };
    if matches!(left, Value::Percentage(_)) {
        return None; // let coerce_operands handle Percentage ± Percentage
    }
    let base = left.as_decimal()?;

    Some(match op {
        BinaryOp::Add => left.with_scaled_amount(base * (Decimal::ONE + p)),
        BinaryOp::Subtract => left.with_scaled_amount(base * (Decimal::ONE - p)),
        BinaryOp::Multiply => left.with_scaled_amount(base * p),
        BinaryOp::Divide if p.is_zero() => Value::Error("Division by zero".to_string()),
        BinaryOp::Divide => left.with_scaled_amount(base / p),
        BinaryOp::Power => left.with_scaled_amount(base.powd(*p)), // 100 ^ 50% = 100^0.5 = 10
        BinaryOp::Conversion => return None,
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
        // compound unit × currency → currency
        (
            Value::WithCompoundUnit { amount: l, .. },
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
            Value::WithCompoundUnit { amount: r, .. },
        ) => Some(Value::currency(l * r, *currency)),
        // currency × number → currency
        (Value::Currency { amount, currency }, Value::Number(n))
        | (Value::Currency { amount, currency }, Value::BaseNumber { amount: n, .. })
        | (Value::Number(n), Value::Currency { amount, currency })
        | (Value::BaseNumber { amount: n, .. }, Value::Currency { amount, currency }) => {
            Some(Value::currency(amount * n, *currency))
        }
        _ => None,
    }
}

/// Try to handle unit operations to create/manipulate compound units
/// e.g., 5m * 10m = 50 m², 100km / 2h = 50 km/h, 12 m² + 15 m² = 27 m²
fn try_unit_compound_op(op: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    // Extract compound units from either simple or compound unit values
    let (l_amount, l_unit) = match left {
        Value::WithUnit { amount, unit } => (*amount, unit.to_compound()),
        Value::WithCompoundUnit { amount, unit } => (*amount, unit.clone()),
        Value::Number(n) | Value::BaseNumber { amount: n, .. } => {
            // Number × Unit → preserve unit
            if let Value::WithUnit { amount, unit } = right {
                return match op {
                    BinaryOp::Multiply => Some(Value::with_unit(*n * *amount, *unit)),
                    _ => None,
                };
            }
            if let Value::WithCompoundUnit { amount, unit } = right {
                return match op {
                    BinaryOp::Multiply => {
                        Some(Value::with_compound_unit(*n * *amount, unit.clone()))
                    }
                    _ => None,
                };
            }
            return None;
        }
        _ => return None,
    };

    let (r_amount, r_unit) = match right {
        Value::WithUnit { amount, unit } => (*amount, unit.to_compound()),
        Value::WithCompoundUnit { amount, unit } => (*amount, unit.clone()),
        Value::Number(n) | Value::BaseNumber { amount: n, .. } => {
            // Unit × Number → preserve unit
            return match op {
                BinaryOp::Multiply => {
                    if matches!(left, Value::WithUnit { .. }) {
                        let Value::WithUnit { amount, unit } = left else {
                            unreachable!()
                        };
                        Some(Value::with_unit(*amount * *n, *unit))
                    } else {
                        Some(Value::with_compound_unit(l_amount * *n, l_unit))
                    }
                }
                BinaryOp::Divide if n.is_zero() => {
                    Some(Value::Error("Division by zero".to_string()))
                }
                BinaryOp::Divide => {
                    if matches!(left, Value::WithUnit { .. }) {
                        let Value::WithUnit { amount, unit } = left else {
                            unreachable!()
                        };
                        Some(Value::with_unit(*amount / *n, *unit))
                    } else {
                        Some(Value::with_compound_unit(l_amount / *n, l_unit))
                    }
                }
                BinaryOp::Power => Some(Value::Error(
                    "Power not supported for unit values".to_string(),
                )),
                _ => None,
            };
        }
        _ => return None,
    };

    match op {
        BinaryOp::Add | BinaryOp::Subtract => {
            // Can only add/subtract compound units with same dimensions
            if l_unit.dimensions != r_unit.dimensions {
                return Some(Value::Error(format!(
                    "Cannot {} {} and {} (incompatible dimensions)",
                    if op == BinaryOp::Add {
                        "add"
                    } else {
                        "subtract"
                    },
                    l_unit.symbol,
                    r_unit.symbol
                )));
            }
            // Convert right to left's unit scale
            let r_converted = if l_unit.symbol == r_unit.symbol {
                r_amount
            } else {
                // Convert through SI base
                let r_si = r_unit.to_si(r_amount);
                l_unit.from_si(r_si)
            };
            let result_amount = match op {
                BinaryOp::Add => l_amount + r_converted,
                BinaryOp::Subtract => l_amount - r_converted,
                _ => unreachable!(),
            };
            Some(Value::with_compound_unit(result_amount, l_unit))
        }
        BinaryOp::Multiply => {
            let result_amount = l_amount * r_amount;
            let result_unit = l_unit.multiply(&r_unit);
            Some(Value::with_compound_unit(result_amount, result_unit))
        }
        BinaryOp::Divide => {
            if r_amount.is_zero() {
                return Some(Value::Error("Division by zero".to_string()));
            }
            let result_amount = l_amount / r_amount;
            let result_unit = l_unit.divide(&r_unit);
            // If the result is dimensionless, return a plain number
            if result_unit.dimensions.is_dimensionless() {
                Some(Value::Number(result_amount))
            } else {
                Some(Value::with_compound_unit(result_amount, result_unit))
            }
        }
        BinaryOp::Power => Some(Value::Error(
            "Power not supported for unit values".to_string(),
        )),
        BinaryOp::Conversion => None,
    }
}

/// Result type for binary operations
enum ResultType {
    Currency(Currency),
    Unit(Unit),
    Number,
    Percentage,
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
            } else if let Some(converted) = unit::convert(*r, &ru.to_compound(), &lu.to_compound())
            {
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
            Value::BaseNumber { amount: l, .. },
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
        (
            Value::Currency {
                amount: l,
                currency,
            },
            Value::BaseNumber { amount: r, .. },
        ) => Ok((*l, *r, ResultType::Currency(*currency))),

        // Number + Unit: propagate unit
        (Value::Number(l), Value::WithUnit { amount: r, unit })
        | (Value::BaseNumber { amount: l, .. }, Value::WithUnit { amount: r, unit })
        | (Value::WithUnit { amount: l, unit }, Value::Number(r)) => {
            Ok((*l, *r, ResultType::Unit(*unit)))
        }
        (Value::WithUnit { amount: l, unit }, Value::BaseNumber { amount: r, .. }) => {
            Ok((*l, *r, ResultType::Unit(*unit)))
        }

        // Percentage + Percentage: preserve percentage type
        (Value::Percentage(l), Value::Percentage(r)) => Ok((*l, *r, ResultType::Percentage)),

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
        BinaryOp::Power if l.is_sign_negative() && r.fract() != Decimal::ZERO => {
            Err("Cannot raise negative number to fractional power".to_string())
        }
        BinaryOp::Power => Ok(l.powd(r)),
        BinaryOp::Conversion => unreachable!("Conversions are handled by eval_conversion"),
    }
}

fn eval_conversion(value: Value, target: &str, ctx: &EvalContext) -> Value {
    if let Some(base) = NumberBase::parse(target) {
        return eval_number_base_conversion(value, base);
    }

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

    // Try as unit (simple or compound)
    if let Some(target_compound) = unit::parse_unit(target) {
        match value {
            Value::WithUnit {
                amount,
                unit: from_unit,
            } => {
                if let Some(converted) =
                    unit::convert(amount, &from_unit.to_compound(), &target_compound)
                {
                    // If target is a simple unit, return WithUnit for backward compat
                    if let Some(target_unit) = Unit::parse(target) {
                        return Value::with_unit(converted, target_unit);
                    }
                    return Value::with_compound_unit(converted, target_compound);
                }
                return Value::Error(format!(
                    "Cannot convert {from_unit} to {}",
                    target_compound.symbol
                ));
            }
            Value::WithCompoundUnit {
                amount,
                unit: from_unit,
            } => {
                if let Some(converted) = unit::convert(amount, &from_unit, &target_compound) {
                    // If target is a simple unit, return WithUnit for backward compat
                    if let Some(target_unit) = Unit::parse(target) {
                        return Value::with_unit(converted, target_unit);
                    }
                    return Value::with_compound_unit(converted, target_compound);
                }
                return Value::Error(format!(
                    "Cannot convert {} to {}",
                    from_unit.symbol, target_compound.symbol
                ));
            }
            // Plain number → attach unit (e.g., "18.39 in months" → "18.39 months")
            Value::Number(n) => {
                if let Some(target_unit) = Unit::parse(target) {
                    return Value::with_unit(n, target_unit);
                }
                return Value::with_compound_unit(n, target_compound);
            }
            // Currency ratio → attach unit (e.g., "usd/usd in months" → dimensionless with unit)
            Value::Currency { amount, .. } => {
                if let Some(target_unit) = Unit::parse(target) {
                    return Value::with_unit(amount, target_unit);
                }
                return Value::with_compound_unit(amount, target_compound);
            }
            _ => {}
        }
    }

    Value::Error(format!("Unknown target unit: {target}"))
}

fn eval_number_base_conversion(value: Value, base: NumberBase) -> Value {
    let amount = match value {
        Value::Number(n) | Value::BaseNumber { amount: n, .. } => n,
        _ => return Value::Error("Base conversion requires a plain integer number".to_string()),
    };

    if !amount.fract().is_zero() {
        return Value::Error("Base conversion requires an integer".to_string());
    }

    Value::with_base(amount, base)
}

fn math_constant(name: &str) -> Option<Value> {
    let value = match name.to_lowercase().as_str() {
        "pi" => std::f64::consts::PI,
        "e" => std::f64::consts::E,
        "phi" => 1.618_033_988_749_895,
        _ => return None,
    };
    Decimal::from_f64(value).map(Value::Number)
}

fn decimal_from_f64(value: f64, name: &str) -> Value {
    Decimal::from_f64(value)
        .map(Value::Number)
        .unwrap_or_else(|| Value::Error(format!("{name} failed")))
}

fn require_number(name: &str, args: &[Value], f: impl Fn(Decimal) -> Value) -> Value {
    if args.len() != 1 {
        return Value::Error(format!("{name} requires exactly one argument"));
    }
    args[0]
        .as_decimal()
        .map(f)
        .unwrap_or_else(|| Value::Error(format!("{name} requires a number")))
}

fn eval_function(name: &str, args: &[Value]) -> Value {
    let require_f64 = |f: fn(f64) -> f64| -> Value {
        require_number(name, args, |n| match n.to_f64() {
            Some(v) => decimal_from_f64(f(v), name),
            None => Value::Error(format!("{name} requires a number")),
        })
    };

    // Check for error values in args before processing aggregates
    if let Some(err) = args.iter().find(|v| v.is_error()) {
        return err.clone();
    }

    // Helper to get all numeric values.
    // NOTE: as_decimal() strips Currency/Unit types. Aggregates like sum($100, $200)
    // return a plain number, not a currency. Supporting typed aggregates would require
    // checking all args share the same type and preserving it in the result.
    let numbers = || args.iter().filter_map(|v| v.as_decimal());

    match name.to_lowercase().as_str() {
        // Aggregate functions
        "sum" | "total" => {
            let vals: Vec<_> = numbers().collect();
            if vals.is_empty() {
                Value::Error(format!("{name} requires at least one value"))
            } else {
                Value::Number(vals.into_iter().sum())
            }
        }

        "avg" | "average" => {
            let vals: Vec<_> = numbers().collect();
            if vals.is_empty() {
                Value::Error(format!("{name} requires at least one value"))
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
        "abs" => require_number(name, args, |n| Value::Number(n.abs())),
        "round" => require_number(name, args, |n| Value::Number(n.round())),
        "floor" => require_number(name, args, |n| Value::Number(n.floor())),
        "ceil" => require_number(name, args, |n| Value::Number(n.ceil())),
        "sin" => require_f64(f64::sin),
        "cos" => require_f64(f64::cos),
        "tan" => require_f64(f64::tan),
        "sinh" => require_f64(f64::sinh),
        "cosh" => require_f64(f64::cosh),
        "tanh" => require_f64(f64::tanh),
        "exp" => require_f64(f64::exp),
        "ln" => require_f64(f64::ln),
        "log" => require_f64(f64::log10),

        "sqrt" => require_number(name, args, |n| {
            if n.is_sign_negative() {
                Value::Error("Cannot take sqrt of negative number".to_string())
            } else {
                match n.sqrt() {
                    Some(v) => Value::Number(v),
                    None => Value::Error(format!("sqrt({n}) failed")),
                }
            }
        }),

        "factorial" => require_number(name, args, |n| {
            let Some(n) = n.to_u64().filter(|_| n.fract().is_zero()) else {
                return Value::Error("factorial requires a non-negative integer".to_string());
            };
            Value::Number((1..=n).map(Decimal::from).product())
        }),

        "mod" => {
            if args.len() != 2 {
                return Value::Error("mod requires exactly two arguments".to_string());
            }
            match (args[0].as_decimal(), args[1].as_decimal()) {
                (Some(_), Some(r)) if r.is_zero() => Value::Error("Division by zero".to_string()),
                (Some(l), Some(r)) => Value::Number(l % r),
                _ => Value::Error("mod requires numbers".to_string()),
            }
        }

        "log_y" => {
            if args.len() != 2 {
                return Value::Error("log_y requires exactly two arguments".to_string());
            }
            match (args[0].as_decimal(), args[1].as_decimal()) {
                (Some(base), Some(value)) => match (base.to_f64(), value.to_f64()) {
                    (Some(base), Some(value)) => decimal_from_f64(value.log(base), name),
                    _ => Value::Error("log_y requires numbers".to_string()),
                },
                _ => Value::Error("log_y requires numbers".to_string()),
            }
        }

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

    fn eval_with_ctx(input: &str, ctx: &mut EvalContext) -> Value {
        let ast = parse_line(input).unwrap();
        evaluate(&ast, ctx)
    }

    fn assert_close(input: &str, expected: f64) {
        let actual = eval_str(input).as_f64().unwrap();
        assert!(
            (actual - expected).abs() < 1e-10,
            "{input}: {actual} != {expected}"
        );
    }

    // ========================================
    // Basic Arithmetic Operations
    // ========================================

    #[test]
    fn test_basic_arithmetic() {
        assert_eq!(eval_str("10 + 20").as_f64(), Some(30.0));
        assert_eq!(eval_str("100 - 25").as_f64(), Some(75.0));
        assert_eq!(eval_str("6 * 7").as_f64(), Some(42.0));
        assert_eq!(eval_str("100 / 4").as_f64(), Some(25.0));
    }

    #[test]
    fn test_division_by_zero() {
        let result = eval_str("10 / 0");
        assert!(result.is_error());
    }

    #[test]
    fn test_negative_numbers() {
        assert_eq!(eval_str("-5 + 10").as_f64(), Some(5.0));
        assert_eq!(eval_str("10 + -5").as_f64(), Some(5.0));
        assert_eq!(eval_str("-5 * -3").as_f64(), Some(15.0));
    }

    #[test]
    fn test_decimal_arithmetic() {
        assert_eq!(eval_str("1.5 + 2.5").as_f64(), Some(4.0));
        assert_eq!(eval_str("10.5 / 2").as_f64(), Some(5.25));
    }

    // ========================================
    // Percentage Operations
    // ========================================

    #[test]
    fn test_percentage_of() {
        assert_eq!(eval_str("20% of 150").as_f64(), Some(30.0));
    }

    #[test]
    fn test_percentage_addition() {
        // 100 + 20% = 120 (add 20% of the base)
        assert_eq!(eval_str("100 + 20%").as_f64(), Some(120.0));
    }

    #[test]
    fn test_percentage_subtraction() {
        // 100 - 20% = 80 (subtract 20% of the base)
        assert_eq!(eval_str("100 - 20%").as_f64(), Some(80.0));
    }

    #[test]
    fn test_percentage_multiplication() {
        // 100 * 50% = 50 (multiply by 0.5)
        assert_eq!(eval_str("100 * 50%").as_f64(), Some(50.0));
    }

    #[test]
    fn test_percentage_division() {
        // 100 / 50% = 200 (divide by 0.5)
        assert_eq!(eval_str("100 / 50%").as_f64(), Some(200.0));
    }

    // ========================================
    // Power Operations
    // ========================================

    #[test]
    fn test_power_basic() {
        assert_eq!(eval_str("2 ^ 3").as_f64(), Some(8.0));
        assert_eq!(eval_str("3 ^ 2").as_f64(), Some(9.0));
    }

    #[test]
    fn test_power_right_associativity() {
        // 2^3^2 should be 2^(3^2) = 2^9 = 512, not (2^3)^2 = 64
        assert_eq!(eval_str("2 ^ 3 ^ 2").as_f64(), Some(512.0));
    }

    // ========================================
    // Currency Operations
    // ========================================

    #[test]
    fn test_currency_addition() {
        let result = eval_str("$100 + $50");
        assert!(matches!(result, Value::Currency { .. }));
        assert_eq!(result.as_f64(), Some(150.0));
    }

    #[test]
    fn test_currency_subtraction() {
        let result = eval_str("$100 - $30");
        assert!(matches!(result, Value::Currency { .. }));
        assert_eq!(result.as_f64(), Some(70.0));
    }

    #[test]
    fn test_currency_multiply_by_number() {
        let result = eval_str("$50 * 3");
        assert!(matches!(result, Value::Currency { .. }));
        assert_eq!(result.as_f64(), Some(150.0));
    }

    #[test]
    fn test_number_multiply_currency() {
        let result = eval_str("3 * $50");
        assert!(matches!(result, Value::Currency { .. }));
        assert_eq!(result.as_f64(), Some(150.0));
    }

    #[test]
    fn test_currency_percentage_of() {
        let result = eval_str("20% of $100");
        assert!(matches!(result, Value::Currency { .. }));
        assert_eq!(result.as_f64(), Some(20.0));
    }

    #[test]
    fn test_currency_add_percentage() {
        // $100 + 10% = $110
        let result = eval_str("$100 + 10%");
        assert!(matches!(result, Value::Currency { .. }));
        assert_eq!(result.as_f64(), Some(110.0));
    }

    // ========================================
    // Unit Operations
    // ========================================

    #[test]
    fn test_unit_addition() {
        let result = eval_str("5 km + 3 km");
        // May return WithUnit or WithCompoundUnit depending on implementation
        assert!(
            matches!(
                result,
                Value::WithUnit { .. } | Value::WithCompoundUnit { .. }
            ),
            "Expected unit value, got {:?}",
            result
        );
        assert_eq!(result.as_f64(), Some(8.0));
    }

    #[test]
    fn test_unit_subtraction() {
        let result = eval_str("10 kg - 3 kg");
        // May return WithUnit or WithCompoundUnit depending on implementation
        assert!(
            matches!(
                result,
                Value::WithUnit { .. } | Value::WithCompoundUnit { .. }
            ),
            "Expected unit value, got {:?}",
            result
        );
        assert_eq!(result.as_f64(), Some(7.0));
    }

    #[test]
    fn test_unit_multiply_by_number() {
        let result = eval_str("5 km * 2");
        assert!(matches!(
            result,
            Value::WithUnit { .. } | Value::WithCompoundUnit { .. }
        ));
        assert_eq!(result.as_f64(), Some(10.0));
    }

    #[test]
    fn test_unit_divide_by_number() {
        let result = eval_str("10 km / 2");
        assert!(matches!(
            result,
            Value::WithUnit { .. } | Value::WithCompoundUnit { .. }
        ));
        assert_eq!(result.as_f64(), Some(5.0));
    }

    #[test]
    fn test_unit_division_to_number() {
        // 10 km / 5 km = 2 (dimensionless)
        let result = eval_str("10 km / 5 km");
        assert!(matches!(result, Value::Number(_)));
        assert_eq!(result.as_f64(), Some(2.0));
    }

    #[test]
    fn test_unit_times_currency() {
        // 8h * $50 = $400 (hours times hourly rate)
        let result = eval_str("8h * $50");
        assert!(matches!(result, Value::Currency { .. }));
        assert_eq!(result.as_f64(), Some(400.0));
    }

    // ========================================
    // Variable Operations
    // ========================================

    #[test]
    fn test_variable_assignment() {
        let mut ctx = EvalContext::new();
        eval_with_ctx("x = 10", &mut ctx);
        let result = eval_with_ctx("x + 5", &mut ctx);
        assert_eq!(result.as_f64(), Some(15.0));
    }

    #[test]
    fn test_variable_undefined() {
        let result = eval_str("undefined_var + 5");
        assert!(result.is_error());
    }

    #[test]
    fn test_variable_with_currency() {
        let mut ctx = EvalContext::new();
        eval_with_ctx("price = $100", &mut ctx);
        let result = eval_with_ctx("price + $50", &mut ctx);
        assert!(matches!(result, Value::Currency { .. }));
        assert_eq!(result.as_f64(), Some(150.0));
    }

    // ========================================
    // Function Calls
    // ========================================

    #[test]
    fn test_function_sum() {
        assert_eq!(eval_str("sum(1, 2, 3)").as_f64(), Some(6.0));
    }

    #[test]
    fn test_function_avg() {
        assert_eq!(eval_str("avg(10, 20, 30)").as_f64(), Some(20.0));
    }

    #[test]
    fn test_function_min() {
        assert_eq!(eval_str("min(5, 2, 8)").as_f64(), Some(2.0));
    }

    #[test]
    fn test_function_max() {
        assert_eq!(eval_str("max(5, 2, 8)").as_f64(), Some(8.0));
    }

    #[test]
    fn test_function_abs() {
        assert_eq!(eval_str("abs(-5)").as_f64(), Some(5.0));
        assert_eq!(eval_str("abs(5)").as_f64(), Some(5.0));
    }

    #[test]
    fn test_function_round() {
        assert_eq!(eval_str("round(3.7)").as_f64(), Some(4.0));
        assert_eq!(eval_str("round(3.2)").as_f64(), Some(3.0));
    }

    #[test]
    fn test_function_floor() {
        assert_eq!(eval_str("floor(3.9)").as_f64(), Some(3.0));
    }

    #[test]
    fn test_function_ceil() {
        assert_eq!(eval_str("ceil(3.1)").as_f64(), Some(4.0));
    }

    #[test]
    fn test_function_sqrt() {
        assert_eq!(eval_str("sqrt(16)").as_f64(), Some(4.0));
        assert_eq!(eval_str("sqrt(9)").as_f64(), Some(3.0));
    }

    #[test]
    fn test_function_sqrt_negative() {
        let result = eval_str("sqrt(-4)");
        assert!(result.is_error());
    }

    #[test]
    fn test_more_math_functions() {
        assert_close("sin(pi / 2)", 1.0);
        assert_close("cos(0)", 1.0);
        assert_close("tan(pi / 4)", 1.0);
        assert_close("sinh(0)", 0.0);
        assert_close("cosh(0)", 1.0);
        assert_close("tanh(0)", 0.0);
        assert_close("exp(1)", std::f64::consts::E);
        assert_close("ln(e)", 1.0);
        assert_close("log(100)", 2.0);
        assert_close("log_y(2, 8)", 3.0);
        assert_eq!(eval_str("factorial(5)").as_f64(), Some(120.0));
        assert_eq!(eval_str("mod(10, 3)").as_f64(), Some(1.0));
    }

    #[test]
    fn test_math_constants() {
        assert_close("pi", std::f64::consts::PI);
        assert_close("e", std::f64::consts::E);
        assert_close("phi", 1.618033988749895);
    }

    #[test]
    fn test_math_functions_with_existing_parser_features() {
        assert_close("sin(pi / 2) + 20% of 100", 21.0);
        assert_eq!(eval_str("round(2 km in m)").as_f64(), Some(2000.0));
        assert_eq!(
            eval_str("sum(1, mod(10, 3), factorial(3))").as_f64(),
            Some(8.0)
        );

        let mut ctx = EvalContext::new();
        assert_eq!(eval_with_ctx("pi = 3", &mut ctx).as_f64(), Some(3.0));
        assert_eq!(eval_with_ctx("pi", &mut ctx).as_f64(), Some(3.0));
    }

    #[test]
    fn test_function_unknown() {
        let result = eval_str("unknown_func(1, 2)");
        assert!(result.is_error());
    }

    // ========================================
    // Complex Expressions
    // ========================================

    #[test]
    fn test_mixed_operations() {
        // Test operator precedence: 2 + 3 * 4 = 2 + 12 = 14
        assert_eq!(eval_str("2 + 3 * 4").as_f64(), Some(14.0));
    }

    #[test]
    fn test_parentheses() {
        // (2 + 3) * 4 = 5 * 4 = 20
        assert_eq!(eval_str("(2 + 3) * 4").as_f64(), Some(20.0));
    }

    #[test]
    fn test_nested_parentheses() {
        // ((1 + 2) * (3 + 4)) = 3 * 7 = 21
        assert_eq!(eval_str("((1 + 2) * (3 + 4))").as_f64(), Some(21.0));
    }

    #[test]
    fn test_chained_operations() {
        // 100 / 4 / 5 = 25 / 5 = 5 (left-to-right for same precedence)
        assert_eq!(eval_str("100 / 4 / 5").as_f64(), Some(5.0));
    }
}
