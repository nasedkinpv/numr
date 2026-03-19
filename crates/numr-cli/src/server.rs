//! JSON-RPC 2.0 server mode for numr-cli
//!
//! Enables external tools to use numr as a calculation backend.
//! Reads JSON-RPC requests from stdin, writes responses to stdout.

use numr_core::{format_currency_value, format_number, Decimal, Engine, Value};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

/// JSON-RPC 2.0 request
#[derive(Deserialize)]
struct Request {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
    #[serde(default)]
    id: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response
#[derive(Serialize)]
struct Response {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    id: serde_json::Value,
}

/// JSON-RPC error object
#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

/// Structured evaluation result
#[derive(Serialize)]
struct EvalResult {
    #[serde(rename = "type")]
    result_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    display: String,
}

/// Params for eval method
#[derive(Deserialize)]
struct EvalParams {
    expr: String,
}

/// Params for eval_lines method
#[derive(Deserialize)]
struct EvalLinesParams {
    lines: Vec<String>,
}

/// Variable info for get_variables response
#[derive(Serialize)]
struct VariableInfo {
    name: String,
    value: EvalResult,
}

// JSON-RPC error codes
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;
const SERVER_ERROR: i32 = -32000;

impl Response {
    fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
            id,
        }
    }
}

/// Convert Value to structured EvalResult
fn value_to_result(value: &Value) -> EvalResult {
    match value {
        Value::Number(n) => EvalResult {
            result_type: "number",
            value: Some(format_number(*n)),
            unit: None,
            message: None,
            display: value.to_string(),
        },
        Value::BaseNumber { amount, .. } => EvalResult {
            result_type: "number",
            value: Some(format_number(*amount)),
            unit: None,
            message: None,
            display: value.to_string(),
        },
        Value::Percentage(p) => EvalResult {
            result_type: "percentage",
            value: Some(format_number(*p * Decimal::from(100))),
            unit: None,
            message: None,
            display: value.to_string(),
        },
        Value::Currency { amount, currency } => EvalResult {
            result_type: "currency",
            value: Some(format_currency_value(*amount, *currency)),
            unit: Some(currency.code().to_string()),
            message: None,
            display: value.to_string(),
        },
        Value::WithUnit { amount, unit } => EvalResult {
            result_type: "unit",
            value: Some(format_number(*amount)),
            unit: Some(unit.to_string()),
            message: None,
            display: value.to_string(),
        },
        Value::WithCompoundUnit { amount, unit } => EvalResult {
            result_type: "unit",
            value: Some(format_number(*amount)),
            unit: Some(unit.symbol.clone()),
            message: None,
            display: value.to_string(),
        },
        Value::Empty => EvalResult {
            result_type: "empty",
            value: None,
            unit: None,
            message: None,
            display: String::new(),
        },
        Value::Error(msg) => EvalResult {
            result_type: "error",
            value: None,
            unit: None,
            message: Some(msg.clone()),
            display: value.to_string(),
        },
    }
}

/// Handle a single JSON-RPC request. Returns None for notifications (no id).
fn handle_request(
    engine: &mut Engine,
    rt: &tokio::runtime::Runtime,
    input: &str,
) -> Option<Response> {
    // Parse request
    let request: Request = match serde_json::from_str(input) {
        Ok(r) => r,
        Err(e) => {
            return Some(Response::error(
                serde_json::Value::Null,
                PARSE_ERROR,
                format!("Parse error: {e}"),
            ));
        }
    };

    // Notifications (no id) must not receive a response per JSON-RPC 2.0 spec
    let id = request.id?;

    // Validate jsonrpc version
    if request.jsonrpc != "2.0" {
        return Some(Response::error(
            id,
            INVALID_REQUEST,
            "Invalid JSON-RPC version",
        ));
    }

    // Dispatch method
    Some(match request.method.as_str() {
        "eval" => handle_eval(engine, id, request.params),
        "eval_lines" => handle_eval_lines(engine, id, request.params),
        "clear" => handle_clear(engine, id),
        "get_totals" => handle_get_totals(engine, id),
        "get_variables" => handle_get_variables(engine, id),
        "reload_rates" => handle_reload_rates(engine, rt, id),
        _ => Response::error(
            id,
            METHOD_NOT_FOUND,
            format!("Method not found: {}", request.method),
        ),
    })
}

/// Handle eval method - evaluate single expression
fn handle_eval(
    engine: &mut Engine,
    id: serde_json::Value,
    params: Option<serde_json::Value>,
) -> Response {
    let params: EvalParams = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(p) => p,
            Err(e) => return Response::error(id, INVALID_PARAMS, format!("Invalid params: {e}")),
        },
        None => return Response::error(id, INVALID_PARAMS, "Missing params"),
    };

    let value = engine.eval(&params.expr);
    let result = value_to_result(&value);
    match serde_json::to_value(result) {
        Ok(v) => Response::success(id, v),
        Err(e) => Response::error(id, INTERNAL_ERROR, format!("Serialization failed: {e}")),
    }
}

/// Handle eval_lines method - evaluate multiple lines (preserves variables)
fn handle_eval_lines(
    engine: &mut Engine,
    id: serde_json::Value,
    params: Option<serde_json::Value>,
) -> Response {
    let params: EvalLinesParams = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(p) => p,
            Err(e) => return Response::error(id, INVALID_PARAMS, format!("Invalid params: {e}")),
        },
        None => return Response::error(id, INVALID_PARAMS, "Missing params"),
    };

    let results: Vec<EvalResult> = params
        .lines
        .iter()
        .map(|line| {
            let value = engine.eval(line);
            value_to_result(&value)
        })
        .collect();

    match serde_json::to_value(results) {
        Ok(v) => Response::success(id, v),
        Err(e) => Response::error(id, INTERNAL_ERROR, format!("Serialization failed: {e}")),
    }
}

/// Handle clear method - clear variables and history
fn handle_clear(engine: &mut Engine, id: serde_json::Value) -> Response {
    engine.clear();
    Response::success(id, serde_json::json!({"message": "Cleared"}))
}

/// Handle get_totals method - get grouped totals
fn handle_get_totals(engine: &Engine, id: serde_json::Value) -> Response {
    let totals = engine.grouped_totals();
    let results: Vec<EvalResult> = totals.iter().map(value_to_result).collect();
    match serde_json::to_value(results) {
        Ok(v) => Response::success(id, v),
        Err(e) => Response::error(id, INTERNAL_ERROR, format!("Serialization failed: {e}")),
    }
}

/// Handle get_variables method - list defined variables
fn handle_get_variables(engine: &Engine, id: serde_json::Value) -> Response {
    let variables = engine.variables();
    let results: Vec<VariableInfo> = variables
        .iter()
        .map(|(name, value)| VariableInfo {
            name: name.clone(),
            value: value_to_result(value),
        })
        .collect();
    match serde_json::to_value(results) {
        Ok(v) => Response::success(id, v),
        Err(e) => Response::error(id, INTERNAL_ERROR, format!("Serialization failed: {e}")),
    }
}

/// Handle reload_rates method - fetch fresh exchange rates
fn handle_reload_rates(
    engine: &mut Engine,
    rt: &tokio::runtime::Runtime,
    id: serde_json::Value,
) -> Response {
    match rt.block_on(numr_core::fetch_rates()) {
        Ok(result) => {
            engine.apply_raw_rates(&result.rates);
            engine.save_rates_to_cache(&result.rates);
            let message = match result.warning {
                Some(w) => format!("Rates reloaded ({w})"),
                None => "Rates reloaded".to_string(),
            };
            Response::success(id, serde_json::json!({"message": message}))
        }
        Err(e) => Response::error(id, SERVER_ERROR, format!("Failed to fetch rates: {e}")),
    }
}

/// Run the JSON-RPC server loop
pub fn run_server(engine: &mut Engine) -> io::Result<()> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| io::Error::other(format!("Failed to create runtime: {e}")))?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        if let Some(response) = handle_request(engine, &rt, &line) {
            let json = serde_json::to_string(&response)?;
            writeln!(stdout, "{json}")?;
            stdout.flush()?;
        }
    }

    Ok(())
}
