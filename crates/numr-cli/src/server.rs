//! JSON-RPC 2.0 server mode for `numr-cli`.
//!
//! The protocol handler is intentionally independent from stdin/stdout so clients and tests can
//! exercise the exact same parsing, validation, and dispatch path as the CLI server.

use numr_core::{format_currency_value, format_number, Decimal, Engine, Value as NumrValue};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};

/// Maximum size of one newline-delimited JSON-RPC message.
///
/// Expression-level limits remain owned by `numr-core`; this limit only bounds transport memory
/// and leaves enough room for batches of valid expressions.
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;
const SERVER_ERROR: i32 = -32000;

#[derive(Debug)]
struct Request {
    method: String,
    params: Option<Value>,
    id: RequestId,
}

#[derive(Debug)]
enum RequestId {
    Notification,
    Call(Value),
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcFailure>,
    id: Value,
}

#[derive(Debug, Serialize)]
struct RpcFailure {
    code: i32,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

/// Stable JSON representation of a calculator value.
///
/// This is the CLI protocol boundary. Keeping it named and public avoids different ad-hoc JSON
/// shapes across `eval`, `eval_lines`, variables, and totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RpcEvalResult {
    #[serde(rename = "type")]
    pub result_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub display: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvalParams {
    expr: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvalLinesParams {
    lines: Vec<String>,
}

#[derive(Debug, Serialize)]
struct VariableInfo {
    name: String,
    value: RpcEvalResult,
}

impl Response {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }

    fn failure(id: Value, failure: RpcFailure) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(failure),
            id,
        }
    }
}

impl RpcFailure {
    fn new(code: i32, message: &'static str) -> Self {
        Self {
            code,
            message,
            data: None,
        }
    }

    fn with_data(mut self, data: impl Into<Value>) -> Self {
        self.data = Some(data.into());
        self
    }

    fn parse(error: impl ToString) -> Self {
        Self::new(PARSE_ERROR, "Parse error").with_data(error.to_string())
    }

    fn invalid_request(detail: impl Into<Value>) -> Self {
        Self::new(INVALID_REQUEST, "Invalid Request").with_data(detail)
    }

    fn invalid_params(detail: impl Into<Value>) -> Self {
        Self::new(INVALID_PARAMS, "Invalid params").with_data(detail)
    }
}

/// Convert a core value into the single JSON representation used by every RPC method.
#[must_use]
pub fn value_to_result(value: &NumrValue) -> RpcEvalResult {
    match value {
        NumrValue::Number(n) => RpcEvalResult {
            result_type: "number",
            value: Some(format_number(*n)),
            unit: None,
            message: None,
            display: value.to_string(),
        },
        NumrValue::BaseNumber { amount, .. } => RpcEvalResult {
            result_type: "number",
            value: Some(format_number(*amount)),
            unit: None,
            message: None,
            display: value.to_string(),
        },
        NumrValue::Percentage(p) => match p.checked_mul(Decimal::from(100)) {
            Some(points) => {
                let formatted = format_number(points);
                RpcEvalResult {
                    result_type: "percentage",
                    value: Some(formatted.clone()),
                    unit: None,
                    message: None,
                    display: format!("{formatted}%"),
                }
            }
            None => RpcEvalResult {
                result_type: "error",
                value: None,
                unit: None,
                message: Some("percentage is outside the displayable range".to_string()),
                display: "Error: percentage is outside the displayable range".to_string(),
            },
        },
        NumrValue::Currency { amount, currency } => RpcEvalResult {
            result_type: "currency",
            value: Some(format_currency_value(*amount, *currency)),
            unit: Some(currency.code().to_string()),
            message: None,
            display: value.to_string(),
        },
        NumrValue::WithUnit { amount, unit } => RpcEvalResult {
            result_type: "unit",
            value: Some(format_number(*amount)),
            unit: Some(unit.to_string()),
            message: None,
            display: value.to_string(),
        },
        NumrValue::WithCompoundUnit { amount, unit } => RpcEvalResult {
            result_type: "unit",
            value: Some(format_number(*amount)),
            unit: Some(unit.symbol.clone()),
            message: None,
            display: value.to_string(),
        },
        NumrValue::Empty => RpcEvalResult {
            result_type: "empty",
            value: None,
            unit: None,
            message: None,
            display: String::new(),
        },
        NumrValue::Error(message) => RpcEvalResult {
            result_type: "error",
            value: None,
            unit: None,
            message: Some(message.to_string()),
            display: value.to_string(),
        },
    }
}

/// Stateful, transport-independent JSON-RPC handler.
pub struct JsonRpcHandler<'a> {
    engine: &'a mut Engine,
    runtime: &'a tokio::runtime::Runtime,
}

impl<'a> JsonRpcHandler<'a> {
    #[must_use]
    pub fn new(engine: &'a mut Engine, runtime: &'a tokio::runtime::Runtime) -> Self {
        Self { engine, runtime }
    }

    /// Handle one complete JSON-RPC message.
    ///
    /// Returns `None` when a notification (or a batch containing only notifications) was handled.
    #[must_use]
    pub fn handle(&mut self, input: &str) -> Option<Value> {
        let value = match serde_json::from_str(input) {
            Ok(value) => value,
            Err(error) => {
                return Some(response_value(Response::failure(
                    Value::Null,
                    RpcFailure::parse(error),
                )))
            }
        };

        self.handle_value(value)
    }

    fn handle_value(&mut self, value: Value) -> Option<Value> {
        match value {
            Value::Array(requests) if requests.is_empty() => {
                Some(response_value(Response::failure(
                    Value::Null,
                    RpcFailure::invalid_request("a batch must contain at least one request"),
                )))
            }
            Value::Array(requests) => {
                let responses: Vec<Value> = requests
                    .into_iter()
                    .filter_map(|request| self.handle_single(request))
                    .collect();

                (!responses.is_empty()).then_some(Value::Array(responses))
            }
            request => self.handle_single(request),
        }
    }

    fn handle_single(&mut self, value: Value) -> Option<Value> {
        let request = match validate_request(value) {
            Ok(request) => request,
            Err(failure) => {
                return Some(response_value(Response::failure(Value::Null, failure)));
            }
        };

        let result = self.dispatch(&request.method, request.params);
        match request.id {
            RequestId::Notification => None,
            RequestId::Call(id) => Some(response_value(match result {
                Ok(result) => Response::success(id, result),
                Err(failure) => Response::failure(id, failure),
            })),
        }
    }

    fn dispatch(&mut self, method: &str, params: Option<Value>) -> Result<Value, RpcFailure> {
        match method {
            "eval" => self.eval(params),
            "eval_lines" => self.eval_lines(params),
            "clear" => {
                ensure_no_params(params)?;
                self.engine.clear();
                Ok(serde_json::json!({"message": "Cleared"}))
            }
            "get_totals" => {
                ensure_no_params(params)?;
                to_json(
                    self.engine
                        .grouped_totals()
                        .iter()
                        .map(value_to_result)
                        .collect::<Vec<_>>(),
                )
            }
            "get_variables" => {
                ensure_no_params(params)?;
                let mut variables = self.engine.variables();
                variables.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
                to_json(
                    variables
                        .into_iter()
                        .map(|(name, value)| VariableInfo {
                            name,
                            value: value_to_result(&value),
                        })
                        .collect::<Vec<_>>(),
                )
            }
            "reload_rates" => {
                ensure_no_params(params)?;
                self.reload_rates()
            }
            _ => Err(RpcFailure::new(METHOD_NOT_FOUND, "Method not found")
                .with_data(serde_json::json!({"method": method}))),
        }
    }

    fn eval(&mut self, params: Option<Value>) -> Result<Value, RpcFailure> {
        let params: EvalParams = parse_params(params)?;
        to_json(value_to_result(&self.engine.eval(&params.expr)))
    }

    fn eval_lines(&mut self, params: Option<Value>) -> Result<Value, RpcFailure> {
        let params: EvalLinesParams = parse_params(params)?;
        to_json(
            self.engine
                .append_lines(params.lines.iter().map(String::as_str))
                .iter()
                .map(|line| value_to_result(&line.value))
                .collect::<Vec<_>>(),
        )
    }

    fn reload_rates(&mut self) -> Result<Value, RpcFailure> {
        match self.runtime.block_on(numr_core::fetch_rates()) {
            Ok(result) => {
                self.engine
                    .apply_raw_rates(&result.rates)
                    .map_err(|error| {
                        RpcFailure::new(SERVER_ERROR, "Server error")
                            .with_data(format!("rejected exchange rates: {error}"))
                    })?;
                let mut warnings = result.warning.into_iter().collect::<Vec<_>>();
                if let Err(error) = self.engine.save_rates_to_cache(&result.rates) {
                    warnings.push(format!(
                        "failed to persist the exchange-rate cache: {error}"
                    ));
                }
                let message = if warnings.is_empty() {
                    "Rates reloaded".to_string()
                } else {
                    format!("Rates reloaded ({})", warnings.join("; "))
                };
                Ok(serde_json::json!({"message": message}))
            }
            Err(error) => Err(RpcFailure::new(SERVER_ERROR, "Server error")
                .with_data(format!("failed to fetch exchange rates: {error}"))),
        }
    }
}

fn validate_request(value: Value) -> Result<Request, RpcFailure> {
    let object = value
        .as_object()
        .ok_or_else(|| RpcFailure::invalid_request("request must be an object"))?;

    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(RpcFailure::invalid_request(
            "jsonrpc must be the string \"2.0\"",
        ));
    }

    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcFailure::invalid_request("method must be a string"))?
        .to_string();

    let id = match object.get("id") {
        None => RequestId::Notification,
        Some(id @ (Value::Null | Value::String(_) | Value::Number(_))) => {
            RequestId::Call(id.clone())
        }
        Some(_) => {
            return Err(RpcFailure::invalid_request(
                "id must be a string, number, or null",
            ));
        }
    };

    Ok(Request {
        method,
        params: object.get("params").cloned(),
        id,
    })
}

fn parse_params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, RpcFailure> {
    let params = params.ok_or_else(|| RpcFailure::invalid_params("params are required"))?;
    serde_json::from_value(params).map_err(|error| RpcFailure::invalid_params(error.to_string()))
}

fn ensure_no_params(params: Option<Value>) -> Result<(), RpcFailure> {
    match params {
        None => Ok(()),
        Some(Value::Array(values)) if values.is_empty() => Ok(()),
        Some(Value::Object(values)) if values.is_empty() => Ok(()),
        Some(_) => Err(RpcFailure::invalid_params(
            "this method does not accept parameters",
        )),
    }
}

fn to_json(value: impl Serialize) -> Result<Value, RpcFailure> {
    serde_json::to_value(value).map_err(|error| {
        RpcFailure::new(INTERNAL_ERROR, "Internal error")
            .with_data(format!("failed to serialize response: {error}"))
    })
}

fn response_value(response: Response) -> Value {
    serde_json::to_value(response).unwrap_or_else(|error| {
        serde_json::json!({
            "jsonrpc": "2.0",
            "error": {
                "code": INTERNAL_ERROR,
                "message": "Internal error",
                "data": format!("failed to serialize response: {error}"),
            },
            "id": null,
        })
    })
}

enum Frame {
    Eof,
    Message(String),
    TooLarge,
}

fn read_frame(reader: &mut impl BufRead, buffer: &mut Vec<u8>) -> io::Result<Frame> {
    buffer.clear();
    let read = {
        let mut limited = std::io::Read::take(&mut *reader, (MAX_REQUEST_BYTES + 2) as u64);
        limited.read_until(b'\n', buffer)?
    };

    if read == 0 {
        return Ok(Frame::Eof);
    }

    let has_newline = buffer.last() == Some(&b'\n');
    let content_len = buffer
        .len()
        .saturating_sub(usize::from(has_newline))
        .saturating_sub(usize::from(
            has_newline && buffer.get(buffer.len() - 2) == Some(&b'\r'),
        ));

    if content_len > MAX_REQUEST_BYTES || !has_newline && buffer.len() > MAX_REQUEST_BYTES {
        if !has_newline {
            drain_line(reader)?;
        }
        return Ok(Frame::TooLarge);
    }

    if has_newline {
        buffer.pop();
        if buffer.last() == Some(&b'\r') {
            buffer.pop();
        }
    }

    Ok(Frame::Message(String::from_utf8_lossy(buffer).into_owned()))
}

fn drain_line(reader: &mut impl BufRead) -> io::Result<()> {
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(());
        }
        if let Some(index) = available.iter().position(|byte| *byte == b'\n') {
            reader.consume(index + 1);
            return Ok(());
        }
        let length = available.len();
        reader.consume(length);
    }
}

/// Run the newline-delimited protocol over arbitrary buffered input/output.
pub fn run_server_with_io(
    engine: &mut Engine,
    runtime: &tokio::runtime::Runtime,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    let mut handler = JsonRpcHandler::new(engine, runtime);
    let mut buffer = Vec::with_capacity(4096);

    loop {
        let response = match read_frame(&mut input, &mut buffer)? {
            Frame::Eof => break,
            Frame::Message(message) => handler.handle(&message),
            Frame::TooLarge => Some(response_value(Response::failure(
                Value::Null,
                RpcFailure::invalid_request(format!(
                    "request exceeds the {MAX_REQUEST_BYTES}-byte transport limit"
                )),
            ))),
        };

        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }

    Ok(())
}

/// Run the JSON-RPC server on stdin/stdout.
pub fn run_server(engine: &mut Engine) -> io::Result<()> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| io::Error::other(format!("failed to create runtime: {error}")))?;
    run_server_with_io(engine, &runtime, io::stdin().lock(), io::stdout().lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn handler_test(test: impl FnOnce(&mut JsonRpcHandler<'_>)) {
        let mut engine = Engine::new();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let mut handler = JsonRpcHandler::new(&mut engine, &runtime);
        test(&mut handler);
    }

    #[test]
    fn malformed_json_and_invalid_request_have_distinct_errors() {
        handler_test(|handler| {
            let malformed = handler.handle("{").expect("error response");
            assert_eq!(malformed["error"]["code"], PARSE_ERROR);
            assert_eq!(malformed["error"]["message"], "Parse error");
            assert_eq!(malformed["id"], Value::Null);

            let invalid = handler.handle("42").expect("error response");
            assert_eq!(invalid["error"]["code"], INVALID_REQUEST);
            assert_eq!(invalid["error"]["message"], "Invalid Request");
            assert_eq!(invalid["id"], Value::Null);
        });
    }

    #[test]
    fn null_id_is_a_call_but_missing_id_is_a_notification() {
        handler_test(|handler| {
            let response = handler
                .handle(r#"{"jsonrpc":"2.0","method":"eval","params":{"expr":"2 + 3"},"id":null}"#)
                .expect("id null must receive a response");
            assert_eq!(response["id"], Value::Null);
            assert_eq!(response["result"]["display"], "5");

            assert_eq!(
                handler.handle(
                    r#"{"jsonrpc":"2.0","method":"eval","params":{"expr":"notified = 7"}}"#
                ),
                None
            );

            let response = handler
                .handle(r#"{"jsonrpc":"2.0","method":"eval","params":{"expr":"notified"},"id":1}"#)
                .expect("response");
            assert_eq!(response["result"]["display"], "7");
        });
    }

    #[test]
    fn errors_use_exact_codes_and_preserve_call_ids() {
        handler_test(|handler| {
            let missing_method = handler
                .handle(r#"{"jsonrpc":"2.0","id":8}"#)
                .expect("response");
            assert_eq!(missing_method["error"]["code"], INVALID_REQUEST);
            assert_eq!(missing_method["id"], Value::Null);

            let unknown = handler
                .handle(r#"{"jsonrpc":"2.0","method":"missing","id":"call-1"}"#)
                .expect("response");
            assert_eq!(unknown["error"]["code"], METHOD_NOT_FOUND);
            assert_eq!(unknown["error"]["message"], "Method not found");
            assert_eq!(unknown["id"], "call-1");

            let params = handler
                .handle(r#"{"jsonrpc":"2.0","method":"eval","params":{"expression":"2"},"id":9}"#)
                .expect("response");
            assert_eq!(params["error"]["code"], INVALID_PARAMS);
            assert_eq!(params["error"]["message"], "Invalid params");
            assert_eq!(params["id"], 9);

            let unexpected = handler
                .handle(r#"{"jsonrpc":"2.0","method":"clear","params":{"all":true},"id":10}"#)
                .expect("response");
            assert_eq!(unexpected["error"]["code"], INVALID_PARAMS);
            assert_eq!(unexpected["id"], 10);
        });
    }

    #[test]
    fn batch_keeps_order_state_and_omits_notification_responses() {
        handler_test(|handler| {
            let response = handler
                .handle(
                    &json!([
                        {"jsonrpc":"2.0","method":"eval","params":{"expr":"x = 40"}},
                        {"jsonrpc":"2.0","method":"eval","params":{"expr":"x + 2"},"id":2},
                        false,
                        {"jsonrpc":"2.0","method":"get_variables","id":3}
                    ])
                    .to_string(),
                )
                .expect("batch response");

            let responses = response.as_array().expect("array response");
            assert_eq!(responses.len(), 3);
            assert_eq!(responses[0]["id"], 2);
            assert_eq!(responses[0]["result"]["display"], "42");
            assert_eq!(responses[1]["error"]["code"], INVALID_REQUEST);
            assert_eq!(responses[1]["id"], Value::Null);
            assert_eq!(responses[2]["id"], 3);
            assert_eq!(responses[2]["result"][0]["name"], "x");

            assert_eq!(
                handler.handle("[]").expect("response")["error"]["code"],
                INVALID_REQUEST
            );
            assert_eq!(
                handler.handle(
                    r#"[{"jsonrpc":"2.0","method":"clear"},{"jsonrpc":"2.0","method":"eval","params":{"expr":"x = 1"}}]"#
                ),
                None
            );
        });
    }

    #[test]
    fn variables_are_sorted_by_name() {
        handler_test(|handler| {
            for expression in ["zebra = 1", "alpha = 2", "middle = 3"] {
                let notification = json!({
                    "jsonrpc": "2.0",
                    "method": "eval",
                    "params": {"expr": expression}
                });
                assert_eq!(handler.handle(&notification.to_string()), None);
            }

            let response = handler
                .handle(r#"{"jsonrpc":"2.0","method":"get_variables","id":1}"#)
                .expect("response");
            let names: Vec<_> = response["result"]
                .as_array()
                .expect("variables")
                .iter()
                .map(|variable| variable["name"].as_str().expect("name"))
                .collect();
            assert_eq!(names, ["alpha", "middle", "zebra"]);
        });
    }

    #[test]
    fn eval_lines_uses_the_shared_result_shape() {
        handler_test(|handler| {
            let response = handler
                .handle(
                    r#"{"jsonrpc":"2.0","method":"eval_lines","params":{"lines":["price = $100","quantity = 5","price * quantity"]},"id":1}"#,
                )
                .expect("response");
            let lines = response["result"].as_array().expect("line results");
            assert_eq!(lines.len(), 3);
            assert_eq!(lines[2]["type"], "currency");
            assert!(lines[2]["display"].as_str().unwrap().contains("500"));
        });
    }

    #[test]
    fn transport_rejects_oversized_frame_and_recovers() {
        let mut input = vec![b' '; MAX_REQUEST_BYTES + 1];
        input.push(b'\n');
        input.extend_from_slice(
            br#"{"jsonrpc":"2.0","method":"eval","params":{"expr":"6 * 7"},"id":2}"#,
        );

        let mut engine = Engine::new();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let mut output = Vec::new();
        run_server_with_io(&mut engine, &runtime, input.as_slice(), &mut output).expect("server");

        let responses: Vec<Value> = String::from_utf8(output)
            .expect("utf8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("json"))
            .collect();
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["error"]["code"], INVALID_REQUEST);
        assert_eq!(responses[1]["result"]["display"], "42");
    }
}
