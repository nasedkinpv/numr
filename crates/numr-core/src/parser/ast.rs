//! Abstract Syntax Tree definitions

use crate::types::{unit, CompoundUnit, Currency, Unit};
use pest::iterators::Pairs;
use rust_decimal::Decimal;
use std::str::FromStr;

use super::Rule;

/// Parse a number string, stripping comma/space separators (e.g., "1,234" or "75 000" -> 75000)
fn parse_number_str(s: &str) -> Result<Decimal, String> {
    let cleaned = s.replace([',', ' '], "");
    Decimal::from_str(&cleaned).map_err(|e| format!("{e}"))
}

/// Top-level AST node for a line
#[derive(Debug, Clone, PartialEq)]
pub enum Ast {
    /// Empty line
    Empty,
    /// Variable assignment: name = expr
    Assignment { name: String, expr: Box<Expr> },
    /// Expression to evaluate
    Expression(Expr),
}

/// Expression node
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Numeric literal
    Number(Decimal),
    /// Percentage literal (stored as decimal, e.g., 20% = 0.20)
    Percentage(Decimal),
    /// Currency value
    Currency { amount: Decimal, currency: Currency },
    /// Value with simple unit
    WithUnit { amount: Decimal, unit: Unit },
    /// Value with compound unit (e.g., 50 km/h, 100 m²)
    WithCompoundUnit { amount: Decimal, unit: CompoundUnit },
    /// Variable reference
    Variable(String),
    /// Binary operation
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Percentage of: 20% of 150
    PercentageOf {
        percentage: Decimal,
        value: Box<Expr>,
    },
    /// Unit/currency conversion: 100$ in EUR
    Conversion {
        value: Box<Expr>,
        target_unit: String,
    },
    /// Function call: sum(), avg()
    FunctionCall { name: String, args: Vec<Expr> },
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Conversion,
}

/// Build AST from parsed pairs
pub fn build_ast(pairs: Pairs<'_, Rule>) -> Result<Ast, String> {
    for pair in pairs {
        if pair.as_rule() == Rule::line || pair.as_rule() == Rule::line_no_prose {
            let inner = pair.into_inner();
            let mut assignment = None;
            let mut expression = None;
            let mut has_trailing = false;

            for inner_pair in inner {
                match inner_pair.as_rule() {
                    Rule::assignment => {
                        assignment = Some(build_assignment(inner_pair.into_inner())?);
                    }
                    Rule::expression => {
                        expression = Some(build_expression(inner_pair.into_inner())?);
                    }
                    Rule::trailing_text => {
                        has_trailing = true;
                    }
                    Rule::EOI => continue,
                    _ => {}
                }
            }

            if let Some(a) = assignment {
                return Ok(a);
            }

            if let Some(e) = expression {
                // Heuristic: If we matched just a single variable followed by prose,
                // it's likely leading junk (e.g., "string here before 1 + 2").
                // Returning an error forces fuzzy parsing to try suffixes.
                if has_trailing && matches!(e, Expr::Variable(_)) {
                    return Err("Ambiguous leading prose".to_string());
                }
                return Ok(Ast::Expression(e));
            }

            return Ok(Ast::Empty);
        }
    }
    Ok(Ast::Empty)
}

fn build_assignment(mut pairs: pest::iterators::Pairs<'_, Rule>) -> Result<Ast, String> {
    let name = pairs
        .next()
        .ok_or("Expected identifier")?
        .as_str()
        .to_string();

    let expr_pair = pairs.next().ok_or("Expected expression")?;
    let expr = build_expression(expr_pair.into_inner())?;

    Ok(Ast::Assignment {
        name,
        expr: Box::new(expr),
    })
}

fn build_expression(pairs: pest::iterators::Pairs<'_, Rule>) -> Result<Expr, String> {
    let mut calculation_expr = None;

    for pair in pairs {
        if pair.as_rule() == Rule::calculation {
            calculation_expr = Some(build_calculation(pair.into_inner())?);
        }
    }

    calculation_expr.ok_or("Expected calculation".to_string())
}

fn build_calculation(pairs: pest::iterators::Pairs<'_, Rule>) -> Result<Expr, String> {
    let mut terms: Vec<Expr> = Vec::new();
    let mut ops: Vec<BinaryOp> = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::number => {
                let n = parse_number_str(pair.as_str())?;
                terms.push(Expr::Number(n));
            }
            Rule::percentage => {
                let inner = pair.into_inner().next().ok_or("Expected number")?;
                let n = parse_number_str(inner.as_str())?;
                terms.push(Expr::Percentage(n / Decimal::from(100)));
            }
            Rule::currency_value => {
                let (amount, currency) = parse_currency_value(pair)?;
                terms.push(Expr::Currency { amount, currency });
            }
            Rule::suffixed_number => {
                terms.push(parse_suffixed_number(pair)?);
            }
            Rule::variable_ref => {
                let name = pair.as_str().to_string();
                terms.push(Expr::Variable(name));
            }
            Rule::parenthesized => {
                let inner = pair.into_inner().next().ok_or("Expected expression")?;
                terms.push(build_expression(inner.into_inner())?);
            }
            Rule::percentage_of => {
                let expr = parse_percentage_of(pair)?;
                terms.push(expr);
            }
            // Rule::atom_with_conversion removed
            Rule::function_call => {
                let expr = parse_function_call(pair)?;
                terms.push(expr);
            }
            Rule::add => ops.push(BinaryOp::Add),
            Rule::subtract => ops.push(BinaryOp::Subtract),
            Rule::multiply => ops.push(BinaryOp::Multiply),
            Rule::divide => ops.push(BinaryOp::Divide),
            Rule::power => ops.push(BinaryOp::Power),
            Rule::conversion_op => ops.push(BinaryOp::Conversion),
            _ => {}
        }
    }

    // Build expression tree with precedence
    if terms.is_empty() {
        return Err("Empty expression".to_string());
    }

    // Pass 1: Power (right-associative: 2^3^2 = 2^(3^2) = 512)
    process_ops_right_assoc(&mut terms, &mut ops, &[BinaryOp::Power]);

    // Pass 2: Multiply, Divide
    process_ops(
        &mut terms,
        &mut ops,
        &[BinaryOp::Multiply, BinaryOp::Divide],
    );

    // Pass 3: Add, Subtract, Conversion (same precedence, left-to-right)
    process_ops_with_conversions(&mut terms, &mut ops)?;

    if terms.len() != 1 {
        return Err("Failed to reduce expression".to_string());
    }

    Ok(terms.remove(0))
}

fn process_ops_with_conversions(
    terms: &mut Vec<Expr>,
    ops: &mut Vec<BinaryOp>,
) -> Result<(), String> {
    let mut i = 0;
    while i < ops.len() {
        match ops[i] {
            BinaryOp::Conversion => {
                ops.remove(i);
                let left = terms.remove(i);
                let right = terms.remove(i);

                // Right operand MUST be a variable (identifier) for the target unit
                let target_unit = match right {
                    Expr::Variable(name) => name,
                    _ => return Err("Conversion target must be a unit identifier".to_string()),
                };

                terms.insert(
                    i,
                    Expr::Conversion {
                        value: Box::new(left),
                        target_unit,
                    },
                );
            }
            BinaryOp::Add | BinaryOp::Subtract => {
                let op = ops.remove(i);
                let left = terms.remove(i);
                let right = terms.remove(i);

                terms.insert(
                    i,
                    Expr::BinaryOp {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                );
            }
            _ => i += 1,
        }
    }
    Ok(())
}

fn process_ops(terms: &mut Vec<Expr>, ops: &mut Vec<BinaryOp>, target_ops: &[BinaryOp]) {
    let mut i = 0;
    while i < ops.len() {
        if target_ops.contains(&ops[i]) {
            let op = ops.remove(i);
            let left = terms.remove(i);
            let right = terms.remove(i); // Was i+1, but after remove(i) it's at i

            terms.insert(
                i,
                Expr::BinaryOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            );
        } else {
            i += 1;
        }
    }
}

/// Process operators right-to-left for right-associative operators like power
/// e.g., 2^3^2 should be 2^(3^2) = 2^9 = 512, not (2^3)^2 = 64
fn process_ops_right_assoc(
    terms: &mut Vec<Expr>,
    ops: &mut Vec<BinaryOp>,
    target_ops: &[BinaryOp],
) {
    // Process from right to left
    let mut i = ops.len();
    while i > 0 {
        i -= 1;
        if target_ops.contains(&ops[i]) {
            let op = ops.remove(i);
            let left = terms.remove(i);
            let right = terms.remove(i);

            terms.insert(
                i,
                Expr::BinaryOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            );
        }
    }
}

fn parse_currency_value(
    pair: pest::iterators::Pair<'_, Rule>,
) -> Result<(Decimal, Currency), String> {
    let mut amount = Decimal::ZERO;
    let mut currency = Currency::USD;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::number => {
                amount = parse_number_str(inner.as_str())?;
            }
            Rule::currency_symbol => {
                currency = Currency::parse(inner.as_str()).ok_or("Unknown currency")?;
            }
            _ => {}
        }
    }

    Ok((amount, currency))
}

fn parse_suffixed_number(pair: pest::iterators::Pair<'_, Rule>) -> Result<Expr, String> {
    let mut inner = pair.into_inner();
    let num_pair = inner.next().ok_or("Expected number")?;
    let amount = parse_number_str(num_pair.as_str())?;

    let suffix_pair = inner.next().ok_or("Expected identifier")?;
    let suffix = suffix_pair.as_str();

    if let Some(currency) = Currency::parse(suffix) {
        Ok(Expr::Currency { amount, currency })
    } else if let Some(unit) = Unit::parse(suffix) {
        // Simple unit from legacy enum
        Ok(Expr::WithUnit { amount, unit })
    } else if let Some(compound_unit) = unit::parse_unit(suffix) {
        // Compound unit from new registry (e.g., kph, m2, mps)
        Ok(Expr::WithCompoundUnit {
            amount,
            unit: compound_unit,
        })
    } else {
        // Treat as implicit multiplication with variable
        Ok(Expr::BinaryOp {
            op: BinaryOp::Multiply,
            left: Box::new(Expr::Number(amount)),
            right: Box::new(Expr::Variable(suffix.to_string())),
        })
    }
}

fn parse_percentage_of(pair: pest::iterators::Pair<'_, Rule>) -> Result<Expr, String> {
    let mut inner = pair.into_inner();
    let pct_pair = inner.next().ok_or("Expected percentage")?;
    let pct_num = pct_pair.into_inner().next().ok_or("Expected number")?;
    let percentage = parse_number_str(pct_num.as_str())?;

    let value_pair = inner.next().ok_or("Expected value")?;
    let value = build_term(value_pair)?;

    Ok(Expr::PercentageOf {
        percentage: percentage / Decimal::from(100),
        value: Box::new(value),
    })
}

fn parse_function_call(pair: pest::iterators::Pair<'_, Rule>) -> Result<Expr, String> {
    let mut inner = pair.into_inner();
    let name = inner
        .next()
        .ok_or("Expected function name")?
        .as_str()
        .to_string();

    let mut args = Vec::new();
    for arg_pair in inner {
        if arg_pair.as_rule() == Rule::expression {
            args.push(build_expression(arg_pair.into_inner())?);
        }
    }

    Ok(Expr::FunctionCall { name, args })
}

fn build_term(pair: pest::iterators::Pair<'_, Rule>) -> Result<Expr, String> {
    match pair.as_rule() {
        Rule::number => {
            let n = parse_number_str(pair.as_str())?;
            Ok(Expr::Number(n))
        }
        Rule::percentage => {
            let inner = pair.into_inner().next().ok_or("Expected number")?;
            let n = parse_number_str(inner.as_str())?;
            Ok(Expr::Percentage(n / Decimal::from(100)))
        }
        Rule::currency_value => {
            let (amount, currency) = parse_currency_value(pair)?;
            Ok(Expr::Currency { amount, currency })
        }
        Rule::suffixed_number => parse_suffixed_number(pair),
        Rule::variable_ref => {
            let name = pair.as_str().to_string();
            Ok(Expr::Variable(name))
        }
        Rule::parenthesized => {
            let inner = pair.into_inner().next().ok_or("Expected expression")?;
            build_expression(inner.into_inner())
        }
        _ => Err(format!("Unexpected rule: {:?}", pair.as_rule())),
    }
}
