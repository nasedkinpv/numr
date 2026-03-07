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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_format_value_number() {
        let value = Value::Number(Decimal::from(42));
        assert_eq!(format_value(&value), "42");
    }

    #[test]
    fn test_format_value_empty() {
        let value = Value::Empty;
        assert_eq!(format_value(&value), "");
    }

    #[test]
    fn test_format_value_error() {
        let value = Value::Error("Division by zero".to_string());
        assert_eq!(format_value(&value), "Error: Division by zero");
    }

    #[test]
    fn test_format_value_currency() {
        use crate::types::Currency;
        let value = Value::Currency {
            amount: Decimal::from(100),
            currency: Currency::USD,
        };
        assert_eq!(format_value(&value), "$100.00");
    }

    #[test]
    fn test_value_to_json_number() {
        let value = Value::Number(Decimal::from(42));
        let json = value_to_json(&value);

        // Parse and verify
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["formatted"], "42");
        assert_eq!(parsed["is_error"], false);
        assert_eq!(parsed["is_empty"], false);
        assert_eq!(parsed["raw"], "42");
    }

    #[test]
    fn test_value_to_json_empty() {
        let value = Value::Empty;
        let json = value_to_json(&value);

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["formatted"], "");
        assert_eq!(parsed["is_error"], false);
        assert_eq!(parsed["is_empty"], true);
    }

    #[test]
    fn test_value_to_json_error() {
        let value = Value::Error("Test error".to_string());
        let json = value_to_json(&value);

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["formatted"], "Error: Test error");
        assert_eq!(parsed["is_error"], true);
        assert_eq!(parsed["is_empty"], false);
    }

    #[test]
    fn test_wasm_engine_eval() {
        let mut engine = WasmEngine::new();
        let result = engine.eval("10 + 20");

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["formatted"], "30");
        assert_eq!(parsed["is_error"], false);
    }

    #[test]
    fn test_wasm_engine_eval_preview() {
        let engine = WasmEngine::new();
        let result = engine.eval_preview("5 * 5");

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["formatted"], "25");
    }

    #[test]
    fn test_wasm_engine_eval_document() {
        let mut engine = WasmEngine::new();
        let result = engine.eval_document("10\n20\n30");

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["result"], "10");
        assert_eq!(parsed[1]["result"], "20");
        assert_eq!(parsed[2]["result"], "30");
    }

    #[test]
    fn test_wasm_engine_variables() {
        let mut engine = WasmEngine::new();
        engine.eval("x = 42");
        engine.eval("y = 100");

        let vars_json = engine.get_variables();
        let vars: Vec<serde_json::Value> = serde_json::from_str(&vars_json).unwrap();

        assert!(vars.iter().any(|v| v["name"] == "x" && v["value"] == "42"));
        assert!(vars.iter().any(|v| v["name"] == "y" && v["value"] == "100"));
    }

    #[test]
    fn test_wasm_engine_clear() {
        let mut engine = WasmEngine::new();
        engine.eval("x = 42");
        engine.clear();

        let vars_json = engine.get_variables();
        let vars: Vec<serde_json::Value> = serde_json::from_str(&vars_json).unwrap();
        assert!(vars.is_empty());
    }

    #[test]
    fn test_wasm_engine_apply_rates() {
        let mut engine = WasmEngine::new();
        engine.apply_rates(r#"{"EUR":0.92,"GBP":0.79}"#);

        let result = engine.eval("$100 in EUR");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["formatted"], "€92.00");
    }

    #[test]
    fn test_wasm_engine_totals() {
        let mut engine = WasmEngine::new();
        engine.eval("$100");
        engine.eval("$50");
        engine.eval("$25");

        let totals_json = engine.get_totals();
        let totals: Vec<String> = serde_json::from_str(&totals_json).unwrap();
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0], "$175.00");
    }
}
