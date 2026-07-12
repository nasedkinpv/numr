//! Expression parser using pest

mod ast;

pub use ast::{Ast, BinaryOp, Expr};

use pest::Parser;
use pest_derive::Parser;

use crate::ParseError;

#[derive(Parser)]
#[grammar = "parser/grammar.pest"]
pub struct NumrParser;

/// Resource limits applied before pest or the recursive evaluator see input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimits {
    pub max_input_bytes: usize,
    pub max_operations: usize,
    pub max_nesting: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024,
            max_operations: 256,
            max_nesting: 128,
        }
    }
}

fn validate_limits(input: &str, limits: ParseLimits) -> Result<(), ParseError> {
    if input.len() > limits.max_input_bytes {
        return Err(ParseError::InputTooLong {
            actual: input.len(),
            max: limits.max_input_bytes,
        });
    }

    let trimmed = input.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
        return Ok(());
    }

    let mut nesting = 0usize;
    let mut max_nesting = 0usize;
    let mut operations = 0usize;
    for ch in input.chars() {
        match ch {
            '(' => {
                nesting = nesting.saturating_add(1);
                max_nesting = max_nesting.max(nesting);
            }
            ')' => nesting = nesting.saturating_sub(1),
            '+' | '-' | '*' | '/' | '÷' | '×' | '^' | ',' | '=' => {
                operations = operations.saturating_add(1);
            }
            _ => {}
        }
    }
    if max_nesting > limits.max_nesting {
        return Err(ParseError::TooDeep {
            actual: max_nesting,
            max: limits.max_nesting,
        });
    }
    if operations > limits.max_operations {
        return Err(ParseError::TooComplex {
            actual: operations,
            max: limits.max_operations,
        });
    }
    Ok(())
}

/// Parse a single line of input (with fuzzy fallback for user input)
pub fn parse_line(input: &str) -> Result<Ast, ParseError> {
    parse_line_with_limits(input, ParseLimits::default())
}

/// Parse with caller-supplied resource limits.
pub fn parse_line_with_limits(input: &str, limits: ParseLimits) -> Result<Ast, ParseError> {
    validate_limits(input, limits)?;
    // Try parsing the full line first
    if let Ok(pairs) = NumrParser::parse(Rule::line, input) {
        if let Ok(ast) = ast::build_ast(pairs) {
            return Ok(ast);
        }
    }

    // Fuzzy parsing: try suffixes starting at word/token boundaries only.
    // This strips leading prose (e.g., "pay rate = $85/hr" → "$85/hr") while
    // avoiding O(n) parse attempts on every byte offset.
    let bytes = input.as_bytes();
    for (i, _) in input.char_indices().skip(1).take(128) {
        // Only try boundaries after whitespace or punctuation
        if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
            continue;
        }
        let suffix = &input[i..];
        if suffix.trim().is_empty() {
            continue;
        }

        if let Ok(pairs) = NumrParser::parse(Rule::line, suffix) {
            if let Ok(ast) = ast::build_ast(pairs) {
                return Ok(ast);
            }
        }
    }

    // If all else fails, return the original error from the full line parse
    // or a generic error
    Err(ParseError::InvalidSyntax)
}

/// Parse a line exactly (no fuzzy fallback) - used for continuation detection
pub fn try_parse_exact(input: &str) -> Result<Ast, ParseError> {
    try_parse_exact_with_limits(input, ParseLimits::default())
}

pub fn try_parse_exact_with_limits(input: &str, limits: ParseLimits) -> Result<Ast, ParseError> {
    validate_limits(input, limits)?;
    match NumrParser::parse(Rule::line_no_prose, input) {
        Ok(pairs) => ast::build_ast(pairs).map_err(ParseError::InvalidExpression),
        Err(_) => Err(ParseError::InvalidSyntax),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_number() {
        let result = parse_line("42");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_expression() {
        let result = parse_line("10 + 20");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_assignment() {
        let result = parse_line("tax = 15%");
        assert!(result.is_ok());
    }

    #[test]
    fn comments_only_consume_the_input_size_budget() {
        let comment = format!(
            "# {}",
            "+".repeat(ParseLimits::default().max_operations + 1)
        );
        assert!(parse_line(&comment).is_ok());
        assert!(try_parse_exact(&comment).is_ok());
    }

    /// Verify grammar.pest currency_symbol rule matches CURRENCIES registry.
    /// If this test fails, you need to sync grammar.pest with types/currency.rs
    #[test]
    fn test_grammar_currency_symbols_match_registry() {
        use crate::types::CURRENCIES;
        use std::collections::HashSet;

        // Read grammar.pest
        let grammar = include_str!("grammar.pest");

        // Extract symbols from: currency_symbol = { "$" | "€" | ... }
        let grammar_symbols: HashSet<&str> = grammar
            .lines()
            .find(|line| line.starts_with("currency_symbol"))
            .expect("currency_symbol rule not found in grammar.pest")
            .split(['"', '|', '{', '}'])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !s.contains("currency_symbol") && !s.contains("="))
            .collect();

        // Get unique symbols from CURRENCIES registry
        // Only single-char Unicode symbols go in grammar (multi-char like "C$" use code parsing)
        let registry_symbols: HashSet<&str> = CURRENCIES
            .iter()
            .map(|def| def.symbol)
            .filter(|s| {
                let chars: Vec<char> = s.chars().collect();
                // Single Unicode symbol OR "zł" (Polish złoty is 2-char but in grammar)
                chars.len() == 1 || *s == "zł"
            })
            .collect();

        // Check for symbols in grammar but not in registry
        let extra_in_grammar: Vec<_> = grammar_symbols.difference(&registry_symbols).collect();
        assert!(
            extra_in_grammar.is_empty(),
            "Symbols in grammar.pest but not in CURRENCIES: {:?}\n\
             Remove from grammar.pest or add to types/currency.rs",
            extra_in_grammar
        );

        // Check for symbols in registry but not in grammar
        let missing_from_grammar: Vec<_> = registry_symbols.difference(&grammar_symbols).collect();
        assert!(
            missing_from_grammar.is_empty(),
            "Symbols in CURRENCIES but not in grammar.pest: {:?}\n\
             Add to grammar.pest currency_symbol rule",
            missing_from_grammar
        );
    }
}
