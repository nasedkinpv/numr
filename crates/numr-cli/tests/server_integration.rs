//! End-to-end tests for the newline-delimited JSON-RPC server.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const EXIT_TIMEOUT: Duration = Duration::from_secs(5);

/// A timeout-safe server process. Background readers prevent pipe backpressure, every failed wait
/// includes stderr, and `Drop` always reaps (or kills) the child after a panic.
struct ServerProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: mpsc::Receiver<String>,
    stderr: Arc<Mutex<String>>,
    readers: Vec<JoinHandle<()>>,
}

impl ServerProcess {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_numr-cli"))
            .arg("--server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn JSON-RPC server");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");
        let mut stderr_pipe = child.stderr.take().expect("child stderr");

        let (responses_tx, responses) = mpsc::channel();
        let stdout_reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if responses_tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let stderr = Arc::new(Mutex::new(String::new()));
        let stderr_output = Arc::clone(&stderr);
        let stderr_reader = thread::spawn(move || {
            let mut output = String::new();
            let _ = stderr_pipe.read_to_string(&mut output);
            *stderr_output
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = output;
        });

        Self {
            child,
            stdin: Some(stdin),
            responses,
            stderr,
            readers: vec![stdout_reader, stderr_reader],
        }
    }

    fn request(&mut self, request: Value) -> Value {
        self.send_raw(&request.to_string());
        self.read_response()
    }

    fn send_raw(&mut self, message: &str) {
        let write_result = self
            .stdin
            .as_mut()
            .expect("server stdin is open")
            .write_all(format!("{message}\n").as_bytes())
            .and_then(|()| self.stdin.as_mut().expect("server stdin is open").flush());
        if let Err(error) = write_result {
            panic!(
                "failed to write request: {error}\nserver stderr:\n{}",
                self.stderr_output()
            );
        }
    }

    fn read_response(&self) -> Value {
        let line = self.responses.recv_timeout(RESPONSE_TIMEOUT).unwrap_or_else(|error| {
            panic!(
                "server did not return a response within {RESPONSE_TIMEOUT:?}: {error}\nserver stderr:\n{}",
                self.stderr_output()
            )
        });
        serde_json::from_str(&line).unwrap_or_else(|error| {
            panic!(
                "server returned invalid JSON ({error}): {line:?}\nserver stderr:\n{}",
                self.stderr_output()
            )
        })
    }

    fn finish(&mut self) {
        self.stdin.take();
        let status = self.wait_for_exit(EXIT_TIMEOUT).unwrap_or_else(|| {
            let _ = self.child.kill();
            let _ = self.child.wait();
            panic!(
                "server did not exit within {EXIT_TIMEOUT:?}\nserver stderr:\n{}",
                self.stderr_output()
            );
        });
        assert!(
            status.success(),
            "server exited with {status}\nserver stderr:\n{}",
            self.stderr_output()
        );
        self.join_readers();
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Ok(None) | Err(_) => return None,
            }
        }
    }

    fn stderr_output(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn join_readers(&mut self) {
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        self.stdin.take();
        if self.wait_for_exit(EXIT_TIMEOUT).is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.join_readers();
    }
}

fn eval_request(expr: &str, id: impl Into<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "eval",
        "params": {"expr": expr},
        "id": id.into()
    })
}

fn display(response: &Value) -> &str {
    response["result"]["display"].as_str().unwrap_or("")
}

fn result_type(response: &Value) -> &str {
    response["result"]["type"].as_str().unwrap_or("")
}

#[test]
fn incremental_evaluation_and_state_survive_errors() {
    let mut server = ServerProcess::spawn();

    for (id, expression) in ["1", "10", "10 +", "10 + 2", "10 + 20"]
        .into_iter()
        .enumerate()
    {
        let response = server.request(eval_request(expression, id as u64));
        if expression == "10 +" {
            assert_eq!(result_type(&response), "error");
        }
        if expression == "10 + 20" {
            assert_eq!(display(&response), "30");
        }
    }

    assert_eq!(
        result_type(&server.request(eval_request("tax = 15%", 10_u64))),
        "percentage"
    );
    assert_eq!(
        display(&server.request(eval_request("100 + tax", 11_u64))),
        "115"
    );
    server.finish();
}

#[test]
fn arithmetic_limits_do_not_terminate_the_server() {
    let mut server = ServerProcess::spawn();

    for (id, expression) in [
        "79228162514264337593543950335 * 2".to_string(),
        "factorial(100)".to_string(),
        std::iter::repeat_n("1", 5_000)
            .collect::<Vec<_>>()
            .join(" + "),
        "1".repeat(20_000),
    ]
    .into_iter()
    .enumerate()
    {
        let response = server.request(eval_request(&expression, id as u64));
        assert_eq!(
            result_type(&response),
            "error",
            "unsafe expression unexpectedly succeeded: {expression}"
        );
    }

    let response = server.request(eval_request("6 * 7", 99_u64));
    assert_eq!(response["id"], 99);
    assert_eq!(display(&response), "42");
    server.finish();
}

#[test]
fn oversized_transport_frame_is_bounded_and_server_recovers() {
    let mut server = ServerProcess::spawn();
    server.send_raw(&" ".repeat(numr_cli::server::MAX_REQUEST_BYTES + 1));
    let response = server.read_response();
    assert_eq!(response["error"]["code"], -32600);
    assert_eq!(response["id"], Value::Null);

    let response = server.request(eval_request("21 * 2", 2_u64));
    assert_eq!(display(&response), "42");
    server.finish();
}
