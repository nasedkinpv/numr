//! WebAssembly bindings for numr-core.

#![cfg(feature = "wasm")]

use std::collections::HashMap;

use wasm_bindgen::prelude::*;

use crate::{Decimal, Engine, Value};

/// Initialize the browser panic hook once.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// WASM-compatible wrapper for the numr engine.
#[wasm_bindgen]
pub struct WasmEngine {
    engine: Engine,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Create an engine without filesystem or network I/O.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            engine: Engine::new(),
        }
    }

    /// Evaluate a document and return results, totals, and variable names together.
    #[wasm_bindgen]
    pub fn eval_document_full(&mut self, content: &str) -> String {
        serde_json::to_string(&self.evaluate_document_json(content))
            .unwrap_or_else(|_| r#"{"results":[],"totals":[],"variable_names":""}"#.to_string())
    }

    /// Currency and rate-provider metadata from the core registry.
    #[wasm_bindgen]
    pub fn get_currency_catalog(&self) -> String {
        serde_json::to_string(&crate::catalog::currency_catalog())
            .unwrap_or_else(|_| "[]".to_string())
    }

    /// Apply exchange rates from a JSON object: {"EUR": 0.92, "BTC": 95000, ...}.
    #[wasm_bindgen]
    pub fn apply_rates(&mut self, rates_json: &str) -> Result<usize, String> {
        let rates = serde_json::from_str::<HashMap<String, Decimal>>(rates_json)
            .map_err(|error| format!("invalid rates JSON: {error}"))?;
        self.engine
            .apply_raw_rates(&rates)
            .map_err(|error| error.to_string())
    }
}

impl WasmEngine {
    fn evaluate_document_json(&mut self, content: &str) -> DocumentResultJson {
        let document = self.engine.evaluate_document(content);
        DocumentResultJson {
            results: document
                .lines
                .into_iter()
                .map(LineResultJson::from)
                .collect(),
            totals: document.totals.iter().map(format_value).collect(),
            variable_names: document
                .variables
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

impl Default for WasmEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Serialize)]
struct LineResultJson {
    input: String,
    result: String,
    is_error: bool,
    is_empty: bool,
}

impl From<crate::LineResult> for LineResultJson {
    fn from(line: crate::LineResult) -> Self {
        Self {
            input: line.input,
            result: format_value(&line.value),
            is_error: line.value.is_error(),
            is_empty: matches!(line.value, Value::Empty),
        }
    }
}

#[derive(serde::Serialize)]
struct DocumentResultJson {
    results: Vec<LineResultJson>,
    totals: Vec<String>,
    variable_names: String,
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Empty => String::new(),
        Value::Error(error) => format!("Error: {error}"),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn formats_public_result_shapes() {
        let cases = [
            (Value::Number(Decimal::from(42)), "42"),
            (Value::Empty, ""),
            (Value::error("Division by zero"), "Error: Division by zero"),
        ];

        for (value, expected) in cases {
            assert_eq!(format_value(&value), expected);
        }
    }

    #[test]
    fn formats_currency_results() {
        use crate::types::Currency;

        let value = Value::Currency {
            amount: Decimal::from(100),
            currency: Currency::USD,
        };
        assert_eq!(format_value(&value), "$100.00");
    }

    #[test]
    fn evaluates_document_in_one_wasm_call() {
        let mut engine = WasmEngine::new();
        let result = engine.eval_document_full("price = $10\nprice + $5");

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["results"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["totals"], serde_json::json!(["$25.00"]));
        assert_eq!(parsed["variable_names"], "price");
    }

    #[test]
    fn applies_rates_before_document_evaluation() {
        let mut engine = WasmEngine::new();
        assert_eq!(engine.apply_rates(r#"{"EUR":0.92,"GBP":0.79}"#).unwrap(), 2);

        let result = engine.eval_document_full("$100 in EUR");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["results"][0]["result"], "€92.00");
    }
}
