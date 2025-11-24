//! Expression evaluation engine

use std::collections::HashMap;

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
    pub fn set_exchange_rate(&mut self, from: Currency, to: Currency, rate: f64) {
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
    // Handle percentage in operations (e.g., 100 + 20% = 120)
    if let Value::Percentage(p) = right {
        if let Some(base) = left.as_f64() {
            return match op {
                BinaryOp::Add => match &left {
                    Value::Currency { currency, .. } => {
                        Value::currency(base * (1.0 + p), *currency)
                    }
                    Value::WithUnit { unit, .. } => Value::with_unit(base * (1.0 + p), *unit),
                    _ => Value::Number(base * (1.0 + p)),
                },
                BinaryOp::Subtract => match &left {
                    Value::Currency { currency, .. } => {
                        Value::currency(base * (1.0 - p), *currency)
                    }
                    Value::WithUnit { unit, .. } => Value::with_unit(base * (1.0 - p), *unit),
                    _ => Value::Number(base * (1.0 - p)),
                },
                BinaryOp::Multiply => Value::Number(base * p),
                BinaryOp::Divide => {
                    if p == 0.0 {
                        Value::Error("Division by zero".to_string())
                    } else {
                        Value::Number(base / p)
                    }
                }
                BinaryOp::Power => Value::Number(base.powf(p)),
            };
        }
    }

    // Handle multiplication with mixed types (unit × currency = currency)
    // This handles cases like "45h * 85 usd" = $3825 (hours × rate = money)
    if op == BinaryOp::Multiply {
        match (&left, &right) {
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
            ) => {
                return Value::currency(l * r, *currency);
            }
            // currency × number → currency (e.g., $340 * 12 = $4080)
            (Value::Currency { amount, currency }, Value::Number(n))
            | (Value::Number(n), Value::Currency { amount, currency }) => {
                return Value::currency(amount * n, *currency);
            }
            _ => {}
        }
    }

    // Handle currency/unit conversion
    let (l_val, r_val, final_currency, final_unit) = match (&left, &right) {
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
                (*l, *r, Some(*lc), None)
            } else {
                // Convert right to left currency
                if let Some(rate) = ctx.rate_cache.get_rate(*rc, *lc) {
                    (*l, *r * rate, Some(*lc), None)
                } else {
                    return Value::Error(format!("No exchange rate for {rc} to {lc}"));
                }
            }
        }
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
                (*l, *r, None, Some(*lu))
            } else if let Some(converted) = unit::convert(*r, *ru, *lu) {
                (*l, converted, None, Some(*lu))
            } else {
                return Value::Error(format!("Cannot convert {ru} to {lu}"));
            }
        }
        _ => match (left.as_f64(), right.as_f64()) {
            (Some(l), Some(r)) => {
                let c = if let Value::Currency { currency, .. } = left {
                    Some(currency)
                } else {
                    None
                };
                let u = if let Value::WithUnit { unit, .. } = left {
                    Some(unit)
                } else {
                    None
                };
                (l, r, c, u)
            }
            _ => return Value::Error("Invalid operands".to_string()),
        },
    };

    let result = match op {
        BinaryOp::Add => l_val + r_val,
        BinaryOp::Subtract => l_val - r_val,
        BinaryOp::Multiply => l_val * r_val,
        BinaryOp::Divide => {
            if r_val == 0.0 {
                return Value::Error("Division by zero".to_string());
            }
            l_val / r_val
        }
        BinaryOp::Power => l_val.powf(r_val),
    };

    if let Some(c) = final_currency {
        Value::currency(result, c)
    } else if let Some(u) = final_unit {
        Value::with_unit(result, u)
    } else {
        Value::Number(result)
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
    match name.to_lowercase().as_str() {
        "sum" | "total" => {
            let sum: f64 = args.iter().filter_map(|v| v.as_f64()).sum();
            Value::Number(sum)
        }
        "avg" | "average" => {
            let values: Vec<f64> = args.iter().filter_map(|v| v.as_f64()).collect();
            if values.is_empty() {
                Value::Number(0.0)
            } else {
                Value::Number(values.iter().sum::<f64>() / values.len() as f64)
            }
        }
        "min" => args
            .iter()
            .filter_map(|v| v.as_f64())
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .map(Value::Number)
            .unwrap_or(Value::Error("No values for min".to_string())),
        "max" => args
            .iter()
            .filter_map(|v| v.as_f64())
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .map(Value::Number)
            .unwrap_or(Value::Error("No values for max".to_string())),
        "abs" => {
            if let Some(Value::Number(n)) = args.first() {
                Value::Number(n.abs())
            } else {
                Value::Error("abs requires a number".to_string())
            }
        }
        "sqrt" => {
            if let Some(Value::Number(n)) = args.first() {
                if *n >= 0.0 {
                    Value::Number(n.sqrt())
                } else {
                    Value::Error("Cannot take sqrt of negative number".to_string())
                }
            } else {
                Value::Error("sqrt requires a number".to_string())
            }
        }
        "round" => {
            if let Some(Value::Number(n)) = args.first() {
                Value::Number(n.round())
            } else {
                Value::Error("round requires a number".to_string())
            }
        }
        "floor" => {
            if let Some(Value::Number(n)) = args.first() {
                Value::Number(n.floor())
            } else {
                Value::Error("floor requires a number".to_string())
            }
        }
        "ceil" => {
            if let Some(Value::Number(n)) = args.first() {
                Value::Number(n.ceil())
            } else {
                Value::Error("ceil requires a number".to_string())
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
