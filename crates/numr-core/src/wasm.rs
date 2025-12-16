//! WebAssembly bindings for numr-core
//!
//! This module provides wasm-bindgen bindings for use in web applications.
//! Enable the "wasm" feature to use these bindings.

#![cfg(feature = "wasm")]

use std::collections::HashMap;

use wasm_bindgen::prelude::*;

use crate::{Engine, Value};

/// Initialize panic hook for better error messages in the browser console
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// WASM-compatible wrapper for the numr Engine
#[wasm_bindgen]
pub struct WasmEngine {
    engine: Engine,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Create a new engine instance
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            engine: Engine::new(),
        }
    }

    /// Evaluate a single line and return the result as a JSON string
    #[wasm_bindgen]
    pub fn eval(&mut self, input: &str) -> String {
        let value = self.engine.eval(input);
        value_to_json(&value)
    }

    /// Evaluate without storing the result (for live preview)
    #[wasm_bindgen]
    pub fn eval_preview(&self, input: &str) -> String {
        let value = self.engine.eval_preview(input);
        value_to_json(&value)
    }

    /// Evaluate multiple lines from a document, returns JSON array of results
    #[wasm_bindgen]
    pub fn eval_document(&mut self, content: &str) -> String {
        // Clear and re-evaluate all lines
        self.engine.clear();

        let results: Vec<LineResultJson> = content
            .lines()
            .map(|line| {
                let value = self.engine.eval(line);
                LineResultJson {
                    input: line.to_string(),
                    result: format_value(&value),
                    is_error: value.is_error(),
                    is_empty: matches!(value, Value::Empty),
                }
            })
            .collect();

        serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get grouped totals as JSON
    #[wasm_bindgen]
    pub fn get_totals(&self) -> String {
        let totals = self.engine.grouped_totals();
        let formatted: Vec<String> = totals.iter().map(format_value).collect();
        serde_json::to_string(&formatted).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get all variables as JSON
    #[wasm_bindgen]
    pub fn get_variables(&self) -> String {
        let vars: Vec<VariableJson> = self
            .engine
            .variables()
            .iter()
            .map(|(name, value)| VariableJson {
                name: name.clone(),
                value: format_value(value),
            })
            .collect();
        serde_json::to_string(&vars).unwrap_or_else(|_| "[]".to_string())
    }

    /// Clear all state
    #[wasm_bindgen]
    pub fn clear(&mut self) {
        self.engine.clear();
    }

    /// Apply exchange rates from JSON object: {"EUR": 0.92, "BTC": 95000, ...}
    #[wasm_bindgen]
    pub fn apply_rates(&mut self, rates_json: &str) {
        if let Ok(rates) = serde_json::from_str::<HashMap<String, f64>>(rates_json) {
            self.engine.apply_raw_rates(&rates);
        }
    }

    /// Get all line results as JSON
    #[wasm_bindgen]
    pub fn get_lines(&self) -> String {
        let lines: Vec<LineResultJson> = self
            .engine
            .lines()
            .iter()
            .map(|lr| LineResultJson {
                input: lr.input.clone(),
                result: format_value(&lr.value),
                is_error: lr.value.is_error(),
                is_empty: matches!(lr.value, Value::Empty),
            })
            .collect();
        serde_json::to_string(&lines).unwrap_or_else(|_| "[]".to_string())
    }
}

impl Default for WasmEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// JSON representation of a line result
#[derive(serde::Serialize)]
struct LineResultJson {
    input: String,
    result: String,
    is_error: bool,
    is_empty: bool,
}

/// JSON representation of a variable
#[derive(serde::Serialize)]
struct VariableJson {
    name: String,
    value: String,
}

/// Convert a Value to a JSON string representation
fn value_to_json(value: &Value) -> String {
    let obj = ValueJson {
        formatted: format_value(value),
        is_error: value.is_error(),
        is_empty: matches!(value, Value::Empty),
        raw: value.as_decimal().map(|d| d.to_string()),
    };
    serde_json::to_string(&obj)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}

#[derive(serde::Serialize)]
struct ValueJson {
    formatted: String,
    is_error: bool,
    is_empty: bool,
    raw: Option<String>,
}

/// Format a Value to a display string
fn format_value(value: &Value) -> String {
    match value {
        Value::Empty => String::new(),
        Value::Error(e) => format!("Error: {}", e),
        _ => value.to_string(),
    }
}
