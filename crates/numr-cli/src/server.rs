//! JSON-RPC 2.0 server mode for numr-cli
//!
//! Enables external tools to use numr as a calculation backend.
//! Reads JSON-RPC requests from stdin, writes responses to stdout.

use numr_core::{Engine, Value};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

/// JSON-RPC 2.0 request
#[derive(Deserialize)]
struct Request {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
    id: serde_json::Value,
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
    value: Option<f64>,
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
            value: Some(*n),
            unit: None,
            message: None,
            display: value.to_string(),
        },
        Value::Percentage(p) => EvalResult {
            result_type: "percentage",
            value: Some(*p),
            unit: None,
            message: None,
            display: value.to_string(),
        },
        Value::Currency { amount, currency } => EvalResult {
            result_type: "currency",
            value: Some(*amount),
            unit: Some(currency.code().to_string()),
            message: None,
            display: value.to_string(),
        },
        Value::WithUnit { amount, unit } => EvalResult {
            result_type: "unit",
            value: Some(*amount),
            unit: Some(unit.to_string()),
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

/// Handle a single JSON-RPC request
fn handle_request(engine: &mut Engine, input: &str) -> Response {
    // Parse request
    let request: Request = match serde_json::from_str(input) {
        Ok(r) => r,
        Err(e) => {
            return Response::error(
                serde_json::Value::Null,
                PARSE_ERROR,
                format!("Parse error: {e}"),
            );
        }
    };

    // Validate jsonrpc version
    if request.jsonrpc != "2.0" {
        return Response::error(request.id, INVALID_REQUEST, "Invalid JSON-RPC version");
    }

    // Dispatch method
    match request.method.as_str() {
        "eval" => handle_eval(engine, request.id, request.params),
        "eval_lines" => handle_eval_lines(engine, request.id, request.params),
        "clear" => handle_clear(engine, request.id),
        "get_totals" => handle_get_totals(engine, request.id),
        "get_variables" => handle_get_variables(engine, request.id),
        "reload_rates" => handle_reload_rates(engine, request.id),
        _ => Response::error(
            request.id,
            METHOD_NOT_FOUND,
            format!("Method not found: {}", request.method),
        ),
    }
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
    Response::success(id, serde_json::to_value(result).unwrap())
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

    Response::success(id, serde_json::to_value(results).unwrap())
}

/// Handle clear method - clear variables and history
fn handle_clear(engine: &mut Engine, id: serde_json::Value) -> Response {
    engine.clear();
    Response::success(id, serde_json::json!({"message": "Cleared"}))
}

/// Handle get_totals method - get grouped totals
fn handle_get_totals(engine: &mut Engine, id: serde_json::Value) -> Response {
    let totals = engine.grouped_totals();
    let results: Vec<EvalResult> = totals.iter().map(value_to_result).collect();
    Response::success(id, serde_json::to_value(results).unwrap())
}

/// Handle get_variables method - list defined variables
fn handle_get_variables(engine: &mut Engine, id: serde_json::Value) -> Response {
    let variables = engine.variables();
    let results: Vec<VariableInfo> = variables
        .iter()
        .map(|(name, value)| VariableInfo {
            name: name.clone(),
            value: value_to_result(value),
        })
        .collect();
    Response::success(id, serde_json::to_value(results).unwrap())
}

/// Handle reload_rates method - fetch fresh exchange rates
fn handle_reload_rates(engine: &mut Engine, id: serde_json::Value) -> Response {
    // Create a small runtime for fetching
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => return Response::error(id, -32000, format!("Failed to create runtime: {e}")),
    };

    match rt.block_on(numr_core::fetch_rates()) {
        Ok(rates) => {
            engine.apply_raw_rates(&rates);
            engine.save_rates_to_cache(&rates);
            Response::success(id, serde_json::json!({"message": "Rates reloaded"}))
        }
        Err(e) => Response::error(id, -32000, format!("Failed to fetch rates: {e}")),
    }
}

/// Run the JSON-RPC server loop
pub fn run_server(engine: &mut Engine) -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        let response = handle_request(engine, &line);
        let json = serde_json::to_string(&response)?;
        writeln!(stdout, "{json}")?;
        stdout.flush()?;
    }

    Ok(())
}
