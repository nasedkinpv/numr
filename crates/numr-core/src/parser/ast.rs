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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;

    /// Helper to extract the expression from an AST
    fn get_expr(ast: &Ast) -> Option<&Expr> {
        match ast {
            Ast::Expression(e) => Some(e),
            Ast::Assignment { expr, .. } => Some(expr),
            Ast::Empty => None,
        }
    }

    /// Check if expr is a binary op with the given operator
    fn is_binary_op(expr: &Expr, expected_op: BinaryOp) -> bool {
        matches!(expr, Expr::BinaryOp { op, .. } if *op == expected_op)
    }

    /// Get the left operand of a binary op
    fn binary_left(expr: &Expr) -> Option<&Expr> {
        match expr {
            Expr::BinaryOp { left, .. } => Some(left),
            _ => None,
        }
    }

    /// Get the right operand of a binary op
    fn binary_right(expr: &Expr) -> Option<&Expr> {
        match expr {
            Expr::BinaryOp { right, .. } => Some(right),
            _ => None,
        }
    }

    // ========================================
    // Operator Precedence Tests
    // ========================================

    #[test]
    fn test_multiply_before_add() {
        // 2 + 3 * 4 should parse as 2 + (3 * 4)
        // The root should be Add, with right child being Multiply
        let ast = parse_line("2 + 3 * 4").unwrap();
        let expr = get_expr(&ast).unwrap();

        assert!(is_binary_op(expr, BinaryOp::Add), "Root should be Add");

        let right = binary_right(expr).unwrap();
        assert!(
            is_binary_op(right, BinaryOp::Multiply),
            "Right child should be Multiply"
        );
    }

    #[test]
    fn test_divide_before_subtract() {
        // 10 - 8 / 2 should parse as 10 - (8 / 2)
        let ast = parse_line("10 - 8 / 2").unwrap();
        let expr = get_expr(&ast).unwrap();

        assert!(
            is_binary_op(expr, BinaryOp::Subtract),
            "Root should be Subtract"
        );

        let right = binary_right(expr).unwrap();
        assert!(
            is_binary_op(right, BinaryOp::Divide),
            "Right child should be Divide"
        );
    }

    #[test]
    fn test_power_before_multiply() {
        // 2 * 3 ^ 2 should parse as 2 * (3 ^ 2)
        let ast = parse_line("2 * 3 ^ 2").unwrap();
        let expr = get_expr(&ast).unwrap();

        assert!(
            is_binary_op(expr, BinaryOp::Multiply),
            "Root should be Multiply"
        );

        let right = binary_right(expr).unwrap();
        assert!(
            is_binary_op(right, BinaryOp::Power),
            "Right child should be Power"
        );
    }

    #[test]
    fn test_power_right_associativity() {
        // 2 ^ 3 ^ 2 should parse as 2 ^ (3 ^ 2)
        // Root is Power with 2 on left, (3^2) on right
        let ast = parse_line("2 ^ 3 ^ 2").unwrap();
        let expr = get_expr(&ast).unwrap();

        assert!(is_binary_op(expr, BinaryOp::Power), "Root should be Power");

        // Left should be 2
        let left = binary_left(expr).unwrap();
        assert!(matches!(left, Expr::Number(n) if *n == Decimal::from(2)));

        // Right should be 3 ^ 2
        let right = binary_right(expr).unwrap();
        assert!(
            is_binary_op(right, BinaryOp::Power),
            "Right child should be Power (3^2)"
        );
    }

    #[test]
    fn test_left_to_right_same_precedence() {
        // 10 - 3 - 2 should parse as (10 - 3) - 2
        // Root is Subtract with right = 2, left = (10 - 3)
        let ast = parse_line("10 - 3 - 2").unwrap();
        let expr = get_expr(&ast).unwrap();

        assert!(
            is_binary_op(expr, BinaryOp::Subtract),
            "Root should be Subtract"
        );

        // Right should be 2
        let right = binary_right(expr).unwrap();
        assert!(matches!(right, Expr::Number(n) if *n == Decimal::from(2)));

        // Left should be (10 - 3)
        let left = binary_left(expr).unwrap();
        assert!(
            is_binary_op(left, BinaryOp::Subtract),
            "Left should be Subtract"
        );
    }

    #[test]
    fn test_division_left_to_right() {
        // 100 / 4 / 5 should parse as (100 / 4) / 5
        let ast = parse_line("100 / 4 / 5").unwrap();
        let expr = get_expr(&ast).unwrap();

        assert!(
            is_binary_op(expr, BinaryOp::Divide),
            "Root should be Divide"
        );

        // Right should be 5
        let right = binary_right(expr).unwrap();
        assert!(matches!(right, Expr::Number(n) if *n == Decimal::from(5)));

        // Left should be (100 / 4)
        let left = binary_left(expr).unwrap();
        assert!(
            is_binary_op(left, BinaryOp::Divide),
            "Left should be Divide"
        );
    }

    // ========================================
    // Parentheses Override Precedence
    // ========================================

    #[test]
    fn test_parentheses_override_precedence() {
        // (2 + 3) * 4 should parse as Multiply with left = (2 + 3)
        let ast = parse_line("(2 + 3) * 4").unwrap();
        let expr = get_expr(&ast).unwrap();

        assert!(
            is_binary_op(expr, BinaryOp::Multiply),
            "Root should be Multiply"
        );

        let left = binary_left(expr).unwrap();
        assert!(is_binary_op(left, BinaryOp::Add), "Left should be Add");
    }

    #[test]
    fn test_nested_parentheses() {
        // ((1 + 2) * 3) + 4 should have Add at root
        let ast = parse_line("((1 + 2) * 3) + 4").unwrap();
        let expr = get_expr(&ast).unwrap();

        assert!(is_binary_op(expr, BinaryOp::Add), "Root should be Add");

        // Right should be 4
        let right = binary_right(expr).unwrap();
        assert!(matches!(right, Expr::Number(n) if *n == Decimal::from(4)));
    }

    // ========================================
    // Assignment Parsing
    // ========================================

    #[test]
    fn test_assignment_parsing() {
        let ast = parse_line("x = 10").unwrap();
        let Ast::Assignment { name, expr } = ast else {
            panic!("Expected Assignment, got {:?}", ast);
        };
        assert_eq!(name, "x");
        assert!(matches!(*expr, Expr::Number(n) if n == Decimal::from(10)));
    }

    #[test]
    fn test_assignment_with_expression() {
        let ast = parse_line("total = 5 + 3").unwrap();
        let Ast::Assignment { name, expr } = ast else {
            panic!("Expected Assignment, got {:?}", ast);
        };
        assert_eq!(name, "total");
        assert!(is_binary_op(&expr, BinaryOp::Add));
    }

    // ========================================
    // Number Parsing
    // ========================================

    #[test]
    fn test_number_with_commas() {
        let ast = parse_line("1,234").unwrap();
        let expr = get_expr(&ast).unwrap();
        assert!(matches!(expr, Expr::Number(n) if *n == Decimal::from(1234)));
    }

    #[test]
    fn test_decimal_number() {
        let ast = parse_line("3.14").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::Number(n) = expr else {
            panic!("Expected Number, got {:?}", expr);
        };
        let expected = Decimal::from_str("3.14").unwrap();
        assert_eq!(*n, expected);
    }

    // ========================================
    // Percentage Parsing
    // ========================================

    #[test]
    fn test_percentage_literal() {
        let ast = parse_line("50%").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::Percentage(p) = expr else {
            panic!("Expected Percentage, got {:?}", expr);
        };
        let expected = Decimal::from_str("0.5").unwrap();
        assert_eq!(*p, expected);
    }

    #[test]
    fn test_percentage_of_expression() {
        let ast = parse_line("20% of 100").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::PercentageOf { percentage, value } = expr else {
            panic!("Expected PercentageOf, got {:?}", expr);
        };
        let expected_pct = Decimal::from_str("0.2").unwrap();
        assert_eq!(*percentage, expected_pct);
        assert!(matches!(**value, Expr::Number(n) if n == Decimal::from(100)));
    }

    // ========================================
    // Currency Parsing
    // ========================================

    #[test]
    fn test_currency_prefix() {
        let ast = parse_line("$100").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::Currency { amount, currency } = expr else {
            panic!("Expected Currency, got {:?}", expr);
        };
        assert_eq!(*amount, Decimal::from(100));
        assert_eq!(*currency, Currency::USD);
    }

    #[test]
    fn test_currency_suffix() {
        let ast = parse_line("100 USD").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::Currency { amount, currency } = expr else {
            panic!("Expected Currency, got {:?}", expr);
        };
        assert_eq!(*amount, Decimal::from(100));
        assert_eq!(*currency, Currency::USD);
    }

    // ========================================
    // Unit Parsing
    // ========================================

    #[test]
    fn test_unit_suffix() {
        let ast = parse_line("5 km").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::WithUnit { amount, unit } = expr else {
            panic!("Expected WithUnit, got {:?}", expr);
        };
        assert_eq!(*amount, Decimal::from(5));
        assert_eq!(*unit, Unit::Kilometer);
    }

    // ========================================
    // Conversion Parsing
    // ========================================

    #[test]
    fn test_conversion_in_keyword() {
        let ast = parse_line("$100 in EUR").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::Conversion { value, target_unit } = expr else {
            panic!("Expected Conversion, got {:?}", expr);
        };
        assert_eq!(target_unit, "EUR");
        assert!(matches!(**value, Expr::Currency { .. }));
    }

    #[test]
    fn test_conversion_to_keyword() {
        let ast = parse_line("5 km to miles").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::Conversion { value, target_unit } = expr else {
            panic!("Expected Conversion, got {:?}", expr);
        };
        assert_eq!(target_unit, "miles");
        assert!(matches!(**value, Expr::WithUnit { .. }));
    }

    // ========================================
    // Function Call Parsing
    // ========================================

    #[test]
    fn test_function_call_single_arg() {
        let ast = parse_line("sqrt(16)").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::FunctionCall { name, args } = expr else {
            panic!("Expected FunctionCall, got {:?}", expr);
        };
        assert_eq!(name, "sqrt");
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_function_call_multiple_args() {
        let ast = parse_line("sum(1, 2, 3)").unwrap();
        let expr = get_expr(&ast).unwrap();
        let Expr::FunctionCall { name, args } = expr else {
            panic!("Expected FunctionCall, got {:?}", expr);
        };
        assert_eq!(name, "sum");
        assert_eq!(args.len(), 3);
    }

    // ========================================
    // Complex Expression Parsing
    // ========================================

    #[test]
    fn test_complex_expression() {
        // Test that a complex expression parses without error
        let ast = parse_line("(10 + 5) * 2 - 3 / 1.5").unwrap();
        let expr = get_expr(&ast).unwrap();
        // Root should be Subtract (lowest precedence, last to be processed)
        assert!(
            is_binary_op(expr, BinaryOp::Subtract),
            "Root should be Subtract"
        );
    }
}
