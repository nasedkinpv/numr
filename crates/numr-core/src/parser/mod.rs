//! Expression parser using pest

mod ast;

pub use ast::{Ast, BinaryOp, Expr};

use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "parser/grammar.pest"]
pub struct NumrParser;

/// Parse a single line of input
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
}
