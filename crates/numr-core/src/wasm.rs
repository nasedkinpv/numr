//! WebAssembly bindings for numr-core.

#![cfg(feature = "wasm")]

use std::collections::HashMap;

use wasm_bindgen::prelude::*;

use crate::{Decimal, DecimalPlaces, DisplayOptions, Engine, Value};

/// Initialize the browser panic hook once.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// WASM-compatible wrapper for the numr engine.
#[wasm_bindgen]
pub struct WasmEngine {
    engine: Engine,
    display_options: DisplayOptions,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Create an engine without filesystem or network I/O.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            engine: Engine::new(),
            display_options: DisplayOptions::default(),
        }
    }

    /// Set display-only decimal precision; None restores the default formatting.
    /// Existing values keep their full precision for subsequent calculations.
    #[wasm_bindgen]
    pub fn set_decimal_places(&mut self, decimals: Option<f64>) -> Result<(), String> {
        // Validate before conversion: JS numbers can be fractional, negative, or non-finite.
        if decimals.is_some_and(|places| {
            !places.is_finite()
                || places.fract() != 0.0
                || !(0.0..=f64::from(DecimalPlaces::MAX)).contains(&places)
        }) {
            return Err("decimal places must be between 0 and 28".to_string());
        }
        self.display_options = DisplayOptions {
            decimals: decimals
                .map(|places| DecimalPlaces::new(places as u32))
                .transpose()
                .map_err(|error| error.to_string())?,
        };
        Ok(())
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
                .map(|line| LineResultJson::with_options(line, self.display_options))
                .collect(),
            totals: document
                .totals
                .iter()
                .map(|value| value.format_with_options(self.display_options))
                .collect(),
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

impl LineResultJson {
    fn with_options(line: crate::LineResult, options: DisplayOptions) -> Self {
        Self {
            input: line.input,
            result: line.value.format_with_options(options),
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn decimal_places_format_results_and_totals_without_rounding_calculations() {
        let mut engine = WasmEngine::new();
        engine.set_decimal_places(Some(2.0)).unwrap();
        let result = engine.eval_document_full("third = 1 / 3\nthird * 3");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["results"][0]["result"], "0.33");
        assert_eq!(parsed["results"][1]["result"], "1");
        assert_eq!(parsed["totals"], serde_json::json!([]));
        assert_eq!(parsed["variable_names"], "third");
    }

    #[test]
    fn decimal_places_can_change_between_documents() {
        let mut engine = WasmEngine::new();
        engine.set_decimal_places(Some(4.0)).unwrap();
        let result = engine.eval_document_full("price = $12.34567");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["results"][0]["result"], "$12.3457");
        assert_eq!(parsed["totals"], serde_json::json!(["$12.3457"]));
        engine.set_decimal_places(Some(0.0)).unwrap();
        let result = engine.eval_document_full("price = $12.34567");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["results"][0]["result"], "$12");
        assert_eq!(parsed["totals"], serde_json::json!(["$12"]));
    }

    #[test]
    fn decimal_places_can_be_reset_to_default() {
        let mut engine = WasmEngine::new();
        let content = "$12.3456\n0.123456";
        let original = engine.eval_document_full(content);
        engine.set_decimal_places(Some(0.0)).unwrap();
        let rounded = engine.eval_document_full(content);
        assert_ne!(rounded, original);
        engine.set_decimal_places(None).unwrap();
        assert_eq!(engine.eval_document_full(content), original);
    }

    #[test]
    fn invalid_decimal_places_preserve_current_setting() {
        let mut engine = WasmEngine::new();
        engine.set_decimal_places(Some(2.0)).unwrap();
        let expected = engine.eval_document_full("1 / 3");
        for invalid in [
            -1.0,
            1.5,
            29.0,
            4294967296.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert!(engine.set_decimal_places(Some(invalid)).is_err());
            assert_eq!(engine.eval_document_full("1 / 3"), expected);
        }
        engine.set_decimal_places(Some(28.0)).unwrap();
    }
}
