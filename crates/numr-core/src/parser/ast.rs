//! Abstract Syntax Tree definitions

use crate::types::{Currency, Unit};
use pest::iterators::Pairs;
use rust_decimal::Decimal;
use std::str::FromStr;

use super::Rule;

/// Parse a number string, stripping comma separators (e.g., "1,234" -> 1234)
fn parse_number_str(s: &str) -> Result<Decimal, String> {
    let cleaned = s.replace(',', "");
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
    /// Value with unit
    WithUnit { amount: Decimal, unit: Unit },
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
}

/// Build AST from parsed pairs
pub fn build_ast(pairs: Pairs<'_, Rule>) -> Result<Ast, String> {
    for pair in pairs {
        if pair.as_rule() == Rule::line {
            let inner = pair.into_inner();
            for inner_pair in inner {
                match inner_pair.as_rule() {
                    Rule::assignment => return build_assignment(inner_pair.into_inner()),
                    Rule::expression => {
                        return Ok(Ast::Expression(build_expression(inner_pair.into_inner())?))
                    }
                    Rule::EOI => continue,
                    _ => {}
                }
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
    let mut conversion_target = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::calculation => {
                calculation_expr = Some(build_calculation(pair.into_inner())?);
            }
            Rule::conversion_suffix => {
                let inner = pair.into_inner();
                // Skip "in"/"to"
                // inner.next(); // "in" | "to" is part of the rule but maybe not a capture?
                // grammar: conversion_suffix = { ("in" | "to") ~ target_unit }
                // "in" | "to" are literals/rules.
                // target_unit is the capture we want.
                // Let's check inner pairs.
                for p in inner {
                    if p.as_rule() == Rule::target_unit {
                        conversion_target = Some(p.as_str().to_string());
                    }
                }
            }
            _ => {}
        }
    }

    let expr = calculation_expr.ok_or("Expected calculation")?;

    if let Some(target) = conversion_target {
        Ok(Expr::Conversion {
            value: Box::new(expr),
            target_unit: target,
        })
    } else {
        Ok(expr)
    }
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
            _ => {}
        }
    }

    // Build expression tree with precedence
    if terms.is_empty() {
        return Err("Empty expression".to_string());
    }

    // Pass 1: Power
    process_ops(&mut terms, &mut ops, &[BinaryOp::Power]);

    // Pass 2: Multiply, Divide
    process_ops(
        &mut terms,
        &mut ops,
        &[BinaryOp::Multiply, BinaryOp::Divide],
    );

    // Pass 3: Add, Subtract
    process_ops(&mut terms, &mut ops, &[BinaryOp::Add, BinaryOp::Subtract]);

    if terms.len() != 1 {
        return Err("Failed to reduce expression".to_string());
    }

    Ok(terms.remove(0))
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
        Ok(Expr::WithUnit { amount, unit })
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
