//! WebAssembly bindings for numr-editor
//!
//! This module provides wasm-bindgen bindings for syntax highlighting.
//! Enable the "wasm" feature to use these bindings.

#![cfg(feature = "wasm")]

use wasm_bindgen::prelude::*;

use crate::highlight::{
    tokenize as tokenize_line, tokenize_with_variables as tokenize_line_with_variables,
};
use std::collections::HashSet;

/// Initialize panic hook for better error messages in the browser console
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Tokenize a line and return JSON array of tokens
#[wasm_bindgen]
pub fn tokenize(input: &str) -> String {
    tokens_to_json(tokenize_line(input))
}

/// Tokenize a line and promote known variable references.
#[wasm_bindgen(js_name = tokenizeWithVariables)]
pub fn tokenize_with_variables(input: &str, variable_names: &str) -> String {
    let variables: HashSet<String> = variable_names.lines().map(str::to_owned).collect();
    tokens_to_json(tokenize_line_with_variables(input, &variables))
}

fn tokens_to_json(tokens: impl serde::Serialize) -> String {
    serde_json::to_string(&tokens).unwrap_or_else(|_| "[]".to_string())
}
