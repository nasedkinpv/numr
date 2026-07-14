//! Expression evaluation engine

use std::collections::HashMap;

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;

use crate::cache::RateCache;
use crate::error::EvalError;
use crate::parser::{Ast, BinaryOp, Expr};
use crate::types::{unit, Currency, NumberBase, Value};

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

    pub fn try_set_exchange_rate(
        &mut self,
        from: Currency,
        to: Currency,
        rate: Decimal,
    ) -> Result<(), EvalError> {
        self.rate_cache.try_set_rate(from, to, rate)
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
        Expr::WithCompoundUnit { amount, unit } => Value::with_compound_unit(*amount, unit.clone()),

        Expr::Variable(name) => ctx
            .get_variable(name)
            .cloned()
            .or_else(|| math_constant(name))
            .unwrap_or_else(|| Value::Error(EvalError::UnknownVariable(name.clone()))),

        Expr::BinaryOp { op, left, right } => {
            let lval = eval_expr(left, ctx);
            let rval = eval_expr(right, ctx);
            eval_binary_op(*op, lval, rval, ctx)
        }

        Expr::PercentageOf { percentage, value } => {
            let val = eval_expr(value, ctx);
            match val {
                Value::Number(n) => checked_mul(n, *percentage, "calculating a percentage")
                    .map(Value::Number)
                    .unwrap_or_else(error_value),
                Value::Currency { amount, currency } => {
                    checked_mul(amount, *percentage, "calculating a percentage")
                        .map(|amount| Value::currency(amount, currency))
                        .unwrap_or_else(error_value)
                }
                Value::WithCompoundUnit { amount, unit } => {
                    checked_mul(amount, *percentage, "calculating a percentage")
                        .map(|amount| Value::with_compound_unit(amount, unit))
                        .unwrap_or_else(error_value)
                }
                _ => Value::error("Cannot calculate percentage of this value"),
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

fn error_value(error: EvalError) -> Value {
    Value::Error(error)
}

fn checked_mul(
    left: Decimal,
    right: Decimal,
    operation: &'static str,
) -> Result<Decimal, EvalError> {
    left.checked_mul(right)
        .ok_or(EvalError::Overflow { operation })
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
        Err(e) => return error_value(e),
    };

    // Perform the arithmetic operation
    let result = match apply_op(op, l_val, r_val) {
        Ok(r) => r,
        Err(e) => return error_value(e),
    };

    // Wrap result in appropriate type
    match result_type {
        ResultType::Currency(c) => Value::currency(result, c),
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

    let amount = match op {
        BinaryOp::Add => base
            .checked_mul(*p)
            .and_then(|delta| base.checked_add(delta)),
        BinaryOp::Subtract => base
            .checked_mul(*p)
            .and_then(|delta| base.checked_sub(delta)),
        BinaryOp::Multiply => base.checked_mul(*p),
        BinaryOp::Divide if p.is_zero() => return Some(error_value(EvalError::DivisionByZero)),
        BinaryOp::Divide => base.checked_div(*p),
        BinaryOp::Power => base.checked_powd(*p),
        BinaryOp::Conversion => return None,
    };
    Some(
        amount
            .map(|amount| left.with_scaled_amount(amount))
            .unwrap_or_else(|| {
                error_value(EvalError::Overflow {
                    operation: "applying a percentage",
                })
            }),
    )
}

/// Try to handle mixed-type multiplication (unit × currency, currency × number)
fn try_multiply_mixed(left: &Value, right: &Value) -> Option<Value> {
    match (left, right) {
        // unit × currency → currency (e.g., 45h * $85 = $3825)
        // unit × currency → currency
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
        ) => Some(
            checked_mul(*l, *r, "multiplying a unit and currency")
                .map(|amount| Value::currency(amount, *currency))
                .unwrap_or_else(error_value),
        ),
        // currency × number → currency
        (Value::Currency { amount, currency }, Value::Number(n))
        | (Value::Currency { amount, currency }, Value::BaseNumber { amount: n, .. })
        | (Value::Number(n), Value::Currency { amount, currency })
        | (Value::BaseNumber { amount: n, .. }, Value::Currency { amount, currency }) => Some(
            checked_mul(*amount, *n, "multiplying currency")
                .map(|amount| Value::currency(amount, *currency))
                .unwrap_or_else(error_value),
        ),
        _ => None,
    }
}

/// Try to handle unit operations to create/manipulate compound units
/// e.g., 5m * 10m = 50 m², 100km / 2h = 50 km/h, 12 m² + 15 m² = 27 m²
fn try_unit_compound_op(op: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    // Extract the canonical unit representation.
    let (l_amount, l_unit) = match left {
        Value::WithCompoundUnit { amount, unit } => (*amount, unit.clone()),
        Value::Number(n) | Value::BaseNumber { amount: n, .. } => {
            // Number × Unit → preserve unit
            if let Value::WithCompoundUnit { amount, unit } = right {
                return match op {
                    BinaryOp::Multiply => Some(
                        checked_mul(*n, *amount, "multiplying a unit")
                            .map(|amount| Value::with_compound_unit(amount, unit.clone()))
                            .unwrap_or_else(error_value),
                    ),
                    _ => None,
                };
            }
            return None;
        }
        _ => return None,
    };

    let (r_amount, r_unit) = match right {
        Value::WithCompoundUnit { amount, unit } => (*amount, unit.clone()),
        Value::Number(n) | Value::BaseNumber { amount: n, .. } => {
            // Unit × Number → preserve unit
            return match op {
                BinaryOp::Multiply => Some(
                    checked_mul(l_amount, *n, "multiplying a unit")
                        .map(|amount| Value::with_compound_unit(amount, l_unit))
                        .unwrap_or_else(error_value),
                ),
                BinaryOp::Divide if n.is_zero() => Some(error_value(EvalError::DivisionByZero)),
                BinaryOp::Divide => Some(
                    l_amount
                        .checked_div(*n)
                        .map(|amount| Value::with_compound_unit(amount, l_unit))
                        .unwrap_or_else(|| {
                            error_value(EvalError::Overflow {
                                operation: "dividing a unit",
                            })
                        }),
                ),
                BinaryOp::Power => Some(Value::error("Power not supported for unit values")),
                _ => None,
            };
        }
        _ => return None,
    };

    match op {
        BinaryOp::Add | BinaryOp::Subtract => {
            // Can only add/subtract compound units with same dimensions
            if l_unit.dimensions != r_unit.dimensions {
                return Some(Value::error(format!(
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
                match r_unit.try_convert_to(r_amount, &l_unit) {
                    Ok(Some(converted)) => converted,
                    Ok(None) => return Some(Value::error("Incompatible unit scales")),
                    Err(error) => return Some(error_value(error)),
                }
            };
            let result_amount = match op {
                BinaryOp::Add => l_amount.checked_add(r_converted),
                BinaryOp::Subtract => l_amount.checked_sub(r_converted),
                _ => unreachable!(),
            };
            Some(
                result_amount
                    .map(|amount| Value::with_compound_unit(amount, l_unit))
                    .unwrap_or_else(|| {
                        error_value(EvalError::Overflow {
                            operation: "adding unit values",
                        })
                    }),
            )
        }
        BinaryOp::Multiply => {
            let result_amount = checked_mul(l_amount, r_amount, "multiplying unit values");
            let result_unit = l_unit.try_multiply(&r_unit);
            Some(match (result_amount, result_unit) {
                (Ok(amount), Ok(unit)) => Value::with_compound_unit(amount, unit),
                (Err(error), _) | (_, Err(error)) => error_value(error),
            })
        }
        BinaryOp::Divide => {
            if r_amount.is_zero() {
                return Some(error_value(EvalError::DivisionByZero));
            }
            let result_unit = match l_unit.try_divide(&r_unit) {
                Ok(unit) => unit,
                Err(error) => return Some(error_value(error)),
            };
            // If the result is dimensionless, return a plain number
            if result_unit.dimensions.is_dimensionless() {
                let result = l_unit.checked_to_si(l_amount).and_then(|left| {
                    r_unit
                        .checked_to_si(r_amount)
                        .and_then(|right| left.checked_div(right))
                });
                Some(result.map(Value::Number).unwrap_or_else(|| {
                    error_value(EvalError::Overflow {
                        operation: "dividing unit values",
                    })
                }))
            } else {
                Some(
                    l_amount
                        .checked_div(r_amount)
                        .map(|amount| Value::with_compound_unit(amount, result_unit))
                        .unwrap_or_else(|| {
                            error_value(EvalError::Overflow {
                                operation: "dividing unit values",
                            })
                        }),
                )
            }
        }
        BinaryOp::Power => Some(Value::error("Power not supported for unit values")),
        BinaryOp::Conversion => None,
    }
}

/// Result type for binary operations
enum ResultType {
    Currency(Currency),
    Number,
    Percentage,
}

/// Coerce operands to compatible decimal values, returning result type
fn coerce_operands(
    left: &Value,
    right: &Value,
    op: BinaryOp,
    ctx: &EvalContext,
) -> Result<(Decimal, Decimal, ResultType), EvalError> {
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
            if op == BinaryOp::Multiply {
                return Err(EvalError::InvalidOperands(
                    "cannot multiply currency by currency".to_string(),
                ));
            }
            let right = if lc == rc {
                *r
            } else if let Some(rate) = ctx.rate_cache.try_get_rate(*rc, *lc)? {
                r.checked_mul(rate).ok_or(EvalError::Overflow {
                    operation: "converting currency",
                })?
            } else {
                return Err(EvalError::InvalidOperands(format!(
                    "no exchange rate for {rc} to {lc}"
                )));
            };
            let result_type = if op == BinaryOp::Divide {
                ResultType::Number
            } else {
                ResultType::Currency(*lc)
            };
            Ok((*l, right, result_type))
        }

        // Number + Currency: propagate currency
        (
            Value::Currency {
                amount: l,
                currency,
            },
            Value::Number(r),
        ) => {
            if matches!(
                op,
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide
            ) {
                Ok((*l, *r, ResultType::Currency(*currency)))
            } else {
                Err(EvalError::InvalidOperands(
                    "invalid currency operation".to_string(),
                ))
            }
        }
        (
            Value::Currency {
                amount: l,
                currency,
            },
            Value::BaseNumber { amount: r, .. },
        ) => {
            if matches!(
                op,
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide
            ) {
                Ok((*l, *r, ResultType::Currency(*currency)))
            } else {
                Err(EvalError::InvalidOperands(
                    "invalid currency operation".to_string(),
                ))
            }
        }
        (
            Value::Number(left) | Value::BaseNumber { amount: left, .. },
            Value::Currency {
                amount: right,
                currency,
            },
        ) => {
            let result_type = if op == BinaryOp::Divide {
                ResultType::Number
            } else {
                ResultType::Currency(*currency)
            };
            Ok((*left, *right, result_type))
        }

        (Value::WithCompoundUnit { .. }, Value::Currency { .. })
        | (Value::Currency { .. }, Value::WithCompoundUnit { .. }) => {
            Err(EvalError::InvalidOperands("invalid operands".to_string()))
        }

        // Percentage + Percentage: preserve percentage type
        (Value::Percentage(l), Value::Percentage(r)) => Ok((*l, *r, ResultType::Percentage)),

        // Plain numbers
        _ => match (left.as_decimal(), right.as_decimal()) {
            (Some(l), Some(r)) => Ok((l, r, ResultType::Number)),
            _ => Err(EvalError::InvalidOperands("invalid operands".to_string())),
        },
    }
}

/// Apply arithmetic operation
fn apply_op(op: BinaryOp, l: Decimal, r: Decimal) -> Result<Decimal, EvalError> {
    match op {
        BinaryOp::Add => l.checked_add(r).ok_or(EvalError::Overflow {
            operation: "adding values",
        }),
        BinaryOp::Subtract => l.checked_sub(r).ok_or(EvalError::Overflow {
            operation: "subtracting values",
        }),
        BinaryOp::Multiply => l.checked_mul(r).ok_or(EvalError::Overflow {
            operation: "multiplying values",
        }),
        BinaryOp::Divide if r.is_zero() => Err(EvalError::DivisionByZero),
        BinaryOp::Divide => l.checked_div(r).ok_or(EvalError::Overflow {
            operation: "dividing values",
        }),
        BinaryOp::Power if l.is_sign_negative() && r.fract() != Decimal::ZERO => {
            Err(EvalError::InvalidOperands(
                "cannot raise negative number to a fractional power".to_string(),
            ))
        }
        BinaryOp::Power => l.checked_powd(r).ok_or(EvalError::Overflow {
            operation: "raising a value to a power",
        }),
        BinaryOp::Conversion => Err(EvalError::InvalidOperands(
            "conversion is not an arithmetic operation".to_string(),
        )),
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
            match ctx.rate_cache.try_get_rate(currency, target_currency) {
                Ok(Some(rate)) => {
                    return amount
                        .checked_mul(rate)
                        .map(|amount| Value::currency(amount, target_currency))
                        .unwrap_or_else(|| {
                            error_value(EvalError::Overflow {
                                operation: "converting currency",
                            })
                        });
                }
                Err(error) => return error_value(error),
                Ok(None) => {}
            }
            return Value::error(format!(
                "No exchange rate for {currency} to {target_currency}"
            ));
        }
    }

    // Try as unit (simple or compound)
    if let Some(target_compound) = unit::parse_unit(target) {
        match value {
            Value::WithCompoundUnit {
                amount,
                unit: from_unit,
            } => {
                match unit::try_convert(amount, &from_unit, &target_compound) {
                    Ok(Some(converted)) => {
                        return Value::with_compound_unit(converted, target_compound)
                    }
                    Err(error) => return error_value(error),
                    Ok(None) => {}
                }
                return Value::error(format!(
                    "Cannot convert {} to {}",
                    from_unit.symbol, target_compound.symbol
                ));
            }
            // Plain number → attach unit (e.g., "18.39 in months" → "18.39 months")
            Value::Number(n) => return Value::with_compound_unit(n, target_compound),
            // Currency ratio → attach unit (e.g., "usd/usd in months" → dimensionless with unit)
            Value::Currency { amount, .. } => {
                return Value::with_compound_unit(amount, target_compound)
            }
            _ => {}
        }
    }

    Value::Error(EvalError::UnknownTarget(target.to_string()))
}

fn eval_number_base_conversion(value: Value, base: NumberBase) -> Value {
    let amount = match value {
        Value::Number(n) | Value::BaseNumber { amount: n, .. } => n,
        _ => return Value::error("Base conversion requires a plain integer number"),
    };

    if !amount.fract().is_zero() {
        return Value::error("Base conversion requires an integer");
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
        .unwrap_or_else(|| Value::error(format!("{name} failed")))
}

fn require_number(name: &str, args: &[Value], f: impl Fn(Decimal) -> Value) -> Value {
    if args.len() != 1 {
        return Value::error(format!("{name} requires exactly one argument"));
    }
    args[0]
        .as_decimal()
        .map(f)
        .unwrap_or_else(|| Value::error(format!("{name} requires a number")))
}

fn plain_decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(value) | Value::BaseNumber { amount: value, .. } => Some(*value),
        _ => None,
    }
}

fn angle_in_radians(value: &Value) -> Result<f64, EvalError> {
    match value {
        Value::Number(value) | Value::BaseNumber { amount: value, .. } => value
            .to_f64()
            .ok_or_else(|| EvalError::InvalidArgument("angle requires a number".to_string())),
        Value::WithCompoundUnit {
            amount,
            unit: angle_unit,
        } if angle_unit.dimensions == unit::Dimensions::angle(1) => angle_unit
            .checked_to_si(*amount)
            .and_then(|value| value.to_f64())
            .ok_or(EvalError::Overflow {
                operation: "converting an angle to radians",
            }),
        _ => Err(EvalError::InvalidArgument(
            "trigonometric functions require a number or angle".to_string(),
        )),
    }
}

fn eval_function(name: &str, args: &[Value]) -> Value {
    let require_f64 = |f: fn(f64) -> f64| -> Value {
        require_number(name, args, |n| match n.to_f64() {
            Some(v) => decimal_from_f64(f(v), name),
            None => Value::error(format!("{name} requires a number")),
        })
    };
    let require_plain_f64 = |f: fn(f64) -> f64| -> Value {
        if args.len() != 1 {
            return Value::error(format!("{name} requires exactly one argument"));
        }
        plain_decimal(&args[0])
            .and_then(|value| value.to_f64())
            .map(|value| decimal_from_f64(f(value), name))
            .unwrap_or_else(|| Value::error(format!("{name} requires a plain number")))
    };
    let require_angle = |f: fn(f64) -> f64| -> Value {
        if args.len() != 1 {
            return Value::error(format!("{name} requires exactly one argument"));
        }
        angle_in_radians(&args[0])
            .map(|value| decimal_from_f64(f(value), name))
            .unwrap_or_else(error_value)
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

    let checked_sum = |values: &[Decimal]| -> Result<Decimal, EvalError> {
        values.iter().try_fold(Decimal::ZERO, |sum, value| {
            sum.checked_add(*value).ok_or(EvalError::Overflow {
                operation: "summing values",
            })
        })
    };

    match name.to_lowercase().as_str() {
        // Aggregate functions
        "sum" | "total" => {
            let vals: Vec<_> = numbers().collect();
            if vals.is_empty() {
                Value::error(format!("{name} requires at least one value"))
            } else {
                checked_sum(&vals)
                    .map(Value::Number)
                    .unwrap_or_else(error_value)
            }
        }

        "avg" | "average" => {
            let vals: Vec<_> = numbers().collect();
            if vals.is_empty() {
                Value::error(format!("{name} requires at least one value"))
            } else {
                checked_sum(&vals)
                    .and_then(|sum| {
                        sum.checked_div(Decimal::from(vals.len()))
                            .ok_or(EvalError::Overflow {
                                operation: "averaging values",
                            })
                    })
                    .map(Value::Number)
                    .unwrap_or_else(error_value)
            }
        }

        "min" => numbers()
            .min()
            .map(Value::Number)
            .unwrap_or_else(|| Value::error("No values for min")),

        "max" => numbers()
            .max()
            .map(Value::Number)
            .unwrap_or_else(|| Value::error("No values for max")),

        "median" => {
            let mut vals: Vec<_> = args.iter().filter_map(plain_decimal).collect();
            if vals.is_empty() {
                return Value::error("median requires at least one value");
            }
            if vals.len() != args.len() {
                return Value::error("median requires numbers");
            }
            vals.sort_unstable();
            let middle = vals.len() / 2;
            if vals.len() % 2 == 1 {
                Value::Number(vals[middle])
            } else {
                vals[middle - 1]
                    .checked_add(vals[middle])
                    .and_then(|sum| sum.checked_div(Decimal::TWO))
                    .map(Value::Number)
                    .unwrap_or_else(|| {
                        error_value(EvalError::Overflow {
                            operation: "calculating a median",
                        })
                    })
            }
        }

        "clamp" => {
            if args.len() != 3 {
                return Value::error("clamp requires exactly three arguments");
            }
            match (
                plain_decimal(&args[0]),
                plain_decimal(&args[1]),
                plain_decimal(&args[2]),
            ) {
                (Some(value), Some(min), Some(max)) if min <= max => {
                    Value::Number(value.clamp(min, max))
                }
                (Some(_), Some(_), Some(_)) => Value::error("clamp minimum cannot exceed maximum"),
                _ => Value::error("clamp requires numbers"),
            }
        }

        // Single-value math functions
        "abs" => require_number(name, args, |n| Value::Number(n.abs())),
        "round" => require_number(name, args, |n| Value::Number(n.round())),
        "floor" => require_number(name, args, |n| Value::Number(n.floor())),
        "ceil" => require_number(name, args, |n| Value::Number(n.ceil())),
        "sin" => require_angle(f64::sin),
        "cos" => require_angle(f64::cos),
        "tan" => require_angle(f64::tan),
        "rad" | "radians" => require_plain_f64(f64::to_radians),
        "deg" | "degrees" => require_plain_f64(f64::to_degrees),
        "sinh" => require_f64(f64::sinh),
        "cosh" => require_f64(f64::cosh),
        "tanh" => require_f64(f64::tanh),
        "exp" => require_f64(f64::exp),
        "ln" => require_f64(f64::ln),
        "log" => require_f64(f64::log10),

        "sqrt" => require_number(name, args, |n| {
            if n.is_sign_negative() {
                Value::error("Cannot take sqrt of negative number")
            } else {
                match n.sqrt() {
                    Some(v) => Value::Number(v),
                    None => Value::error(format!("sqrt({n}) failed")),
                }
            }
        }),

        "factorial" => require_number(name, args, |n| {
            let Some(n) = n.to_u64().filter(|_| n.fract().is_zero()) else {
                return Value::error("factorial requires a non-negative integer");
            };
            (1..=n)
                .try_fold(Decimal::ONE, |product, value| {
                    product
                        .checked_mul(Decimal::from(value))
                        .ok_or(EvalError::Overflow {
                            operation: "calculating factorial",
                        })
                })
                .map(Value::Number)
                .unwrap_or_else(error_value)
        }),

        "mod" => {
            if args.len() != 2 {
                return Value::error("mod requires exactly two arguments");
            }
            match (args[0].as_decimal(), args[1].as_decimal()) {
                (Some(_), Some(r)) if r.is_zero() => Value::Error(EvalError::DivisionByZero),
                (Some(l), Some(r)) => l.checked_rem(r).map(Value::Number).unwrap_or_else(|| {
                    error_value(EvalError::Overflow {
                        operation: "calculating a remainder",
                    })
                }),
                _ => Value::error("mod requires numbers"),
            }
        }

        "log_y" => {
            if args.len() != 2 {
                return Value::error("log_y requires exactly two arguments");
            }
            match (args[0].as_decimal(), args[1].as_decimal()) {
                (Some(base), Some(value)) => match (base.to_f64(), value.to_f64()) {
                    (Some(base), Some(value)) => decimal_from_f64(value.log(base), name),
                    _ => Value::error("log_y requires numbers"),
                },
                _ => Value::error("log_y requires numbers"),
            }
        }

        _ => Value::Error(EvalError::UnknownFunction(name.to_string())),
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
        assert!(
            matches!(result, Value::WithCompoundUnit { .. }),
            "Expected unit value, got {:?}",
            result
        );
        assert_eq!(result.as_f64(), Some(8.0));
    }

    #[test]
    fn test_unit_subtraction() {
        let result = eval_str("10 kg - 3 kg");
        assert!(
            matches!(result, Value::WithCompoundUnit { .. }),
            "Expected unit value, got {:?}",
            result
        );
        assert_eq!(result.as_f64(), Some(7.0));
    }

    #[test]
    fn test_unit_multiply_by_number() {
        let result = eval_str("5 km * 2");
        assert!(matches!(result, Value::WithCompoundUnit { .. }));
        assert_eq!(result.as_f64(), Some(10.0));
    }

    #[test]
    fn test_unit_divide_by_number() {
        let result = eval_str("10 km / 2");
        assert!(matches!(result, Value::WithCompoundUnit { .. }));
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
    fn test_median_and_clamp_functions() {
        assert_eq!(eval_str("median(3, 1, 2)").as_f64(), Some(2.0));
        assert_eq!(eval_str("median(1, 4, 2, 3)").as_f64(), Some(2.5));
        assert_eq!(eval_str("clamp(120, 0, 100)").as_f64(), Some(100.0));
        assert_eq!(eval_str("clamp(-5, 0, 100)").as_f64(), Some(0.0));
        assert_eq!(eval_str("clamp(40, 0, 100)").as_f64(), Some(40.0));

        assert!(eval_str("median()").is_error());
        assert!(eval_str("clamp(1, 10, 0)").is_error());
        assert!(eval_str("clamp(1, 2)").is_error());
    }

    #[test]
    fn test_degree_radian_conversion_composes_with_trigonometry() {
        assert_close("deg(pi)", 180.0);
        assert_close("rad(180)", std::f64::consts::PI);
        assert_close("sin(rad(90))", 1.0);
        assert_close("cos(rad(180))", -1.0);

        assert!(eval_str("rad()").is_error());
        assert!(eval_str("deg(1, 2)").is_error());
    }

    #[test]
    fn test_angle_units_convert_and_work_with_trigonometry() {
        assert_close("90 deg to rad", std::f64::consts::FRAC_PI_2);
        assert_close("3.141592653589793 rad to deg", 180.0);
        assert_close("sin(90deg)", 1.0);
        assert_close("cos(180 deg)", -1.0);
        assert_eq!(eval_str("180 degrees").to_string(), "180°");
        assert_eq!(eval_str("1 radian").to_string(), "1 rad");
    }

    #[test]
    fn test_new_math_functions_do_not_strip_unrelated_types() {
        assert!(eval_str("90 deg to m").is_error());
        assert!(eval_str("sin(1 m)").is_error());
        assert!(eval_str("deg(1 m)").is_error());
        assert!(eval_str("median(1, 2 m)").is_error());
        assert!(eval_str("clamp($5, 0, 10)").is_error());
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
