//! WebAssembly bindings for numr-editor
//!
//! This module provides wasm-bindgen bindings for syntax highlighting.
//! Enable the "wasm" feature to use these bindings.

#![cfg(feature = "wasm")]

use wasm_bindgen::prelude::*;

use crate::highlight::{tokenize as tokenize_line, Token, TokenType};

/// Initialize panic hook for better error messages in the browser console
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Tokenize a line and return JSON array of tokens
#[wasm_bindgen]
pub fn tokenize(input: &str) -> String {
    let tokens = tokenize_line(input);
    let json_tokens: Vec<TokenJson> = tokens.into_iter().map(TokenJson::from).collect();
    serde_json::to_string(&json_tokens).unwrap_or_else(|_| "[]".to_string())
}

/// JSON representation of a token
#[derive(serde::Serialize)]
struct TokenJson {
    text: String,
    token_type: String,
}

impl From<Token> for TokenJson {
    fn from(token: Token) -> Self {
        Self {
            text: token.text,
            token_type: token_type_to_str(token.token_type).to_owned(),
        }
    }
}

const fn token_type_to_str(tt: TokenType) -> &'static str {
    match tt {
        TokenType::Number => "number",
        TokenType::Operator => "operator",
        TokenType::Variable => "variable",
        TokenType::Unit => "unit",
        TokenType::Currency => "currency",
        TokenType::Keyword => "keyword",
        TokenType::Function => "function",
        TokenType::Comment => "comment",
        TokenType::Text => "text",
        TokenType::Whitespace => "whitespace",
        TokenType::Punctuation => "punctuation",
    }
}
