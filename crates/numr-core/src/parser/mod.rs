//! Expression parser using pest

mod ast;

pub use ast::{Ast, BinaryOp, Expr};

use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "parser/grammar.pest"]
pub struct NumrParser;

/// Parse a single line of input (with fuzzy fallback for user input)
pub fn parse_line(input: &str) -> Result<Ast, String> {
    // Try parsing the full line first
    if let Ok(pairs) = NumrParser::parse(Rule::line, input) {
        return ast::build_ast(pairs);
    }

    // Fuzzy parsing: Try to find a valid suffix
    // We iterate through char indices to find a start position
    for (i, _) in input.char_indices() {
        if i == 0 {
            continue;
        } // Already tried full line

        let suffix = &input[i..];
        // Optimization: Only try if it looks like start of something (digit, variable, etc)
        // For now, just try everything to be safe, or maybe skip whitespace
        if suffix.trim().is_empty() {
            continue;
        }

        if let Ok(pairs) = NumrParser::parse(Rule::line, suffix) {
            // println!("Found valid suffix: {}", suffix);
            return ast::build_ast(pairs);
        }
    }

    // If all else fails, return the original error from the full line parse
    // or a generic error
    Err("Parse error: Could not understand line".to_string())
}

/// Parse a line exactly (no fuzzy fallback) - used for continuation detection
pub fn try_parse_exact(input: &str) -> Result<Ast, String> {
    match NumrParser::parse(Rule::line, input) {
        Ok(pairs) => ast::build_ast(pairs),
        Err(_) => Err("Parse error".to_string()),
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
