//! Integration tests for JSON-RPC server mode
//!
//! These tests simulate real-world usage patterns like Raycast/vicinae
//! where each keystroke triggers an eval call.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Helper to send a JSON-RPC request and get response
fn send_request(stdin: &mut impl Write, stdout: &mut impl BufRead, request: Value) -> Value {
    writeln!(stdin, "{}", request).unwrap();
    stdin.flush().unwrap();

    let mut response = String::new();
    stdout.read_line(&mut response).unwrap();
    serde_json::from_str(&response).unwrap()
}

/// Helper to create eval request
fn eval_request(expr: &str, id: u32) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "eval",
        "params": {"expr": expr},
        "id": id
    })
}

/// Helper to extract result from response
fn get_result(response: &Value) -> &Value {
    response.get("result").expect("expected result in response")
}

/// Helper to get display string from result
fn get_display(response: &Value) -> &str {
    get_result(response)
        .get("display")
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

/// Helper to get result type
fn get_type(response: &Value) -> &str {
    get_result(response)
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

/// Spawn server process
fn spawn_server() -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_numr-cli"))
        .arg("--server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn server")
}

/// Test: User types "20% of 150" character by character
/// Simulates real-time evaluation as user types in launcher
#[test]
fn test_incremental_typing_percentage() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Simulate typing: "20% of 150"
    let keystrokes = [
        "2",
        "20",
        "20%",
        "20% ",
        "20% o",
        "20% of",
        "20% of ",
        "20% of 1",
        "20% of 15",
        "20% of 150",
    ];

    for (i, partial) in keystrokes.iter().enumerate() {
        let response = send_request(&mut stdin, &mut stdout, eval_request(partial, i as u32));

        // Early keystrokes might be errors or partial results
        // Final result should be 30
        if *partial == "20% of 150" {
            assert_eq!(get_display(&response), "30");
            assert_eq!(get_type(&response), "number");
        }
    }

    drop(stdin);
    child.wait().unwrap();
}

/// Test: User types currency expression "$100 in eur"
#[test]
fn test_incremental_currency_conversion() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let keystrokes = [
        "$",
        "$1",
        "$10",
        "$100",
        "$100 ",
        "$100 i",
        "$100 in",
        "$100 in ",
        "$100 in e",
        "$100 in eu",
        "$100 in eur",
    ];

    for (i, partial) in keystrokes.iter().enumerate() {
        let response = send_request(&mut stdin, &mut stdout, eval_request(partial, i as u32));

        // $100 alone should work
        if *partial == "$100" {
            assert_eq!(get_type(&response), "currency");
            assert!(get_display(&response).contains("100"));
        }

        // Final conversion should be EUR
        if *partial == "$100 in eur" {
            assert_eq!(get_type(&response), "currency");
            assert!(get_display(&response).contains("€"));
        }
    }

    drop(stdin);
    child.wait().unwrap();
}

/// Test: User types simple arithmetic "10 + 20"
#[test]
fn test_incremental_arithmetic() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let keystrokes = ["1", "10", "10 ", "10 +", "10 + ", "10 + 2", "10 + 20"];

    for (i, partial) in keystrokes.iter().enumerate() {
        let response = send_request(&mut stdin, &mut stdout, eval_request(partial, i as u32));

        // "10" alone is valid
        if *partial == "10" {
            assert_eq!(get_display(&response), "10");
        }

        // Final result
        if *partial == "10 + 20" {
            assert_eq!(get_display(&response), "30");
        }
    }

    drop(stdin);
    child.wait().unwrap();
}

/// Test: Variable assignment persists across calls
#[test]
fn test_variable_persistence() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Define variable
    let response = send_request(&mut stdin, &mut stdout, eval_request("tax = 15%", 1));
    assert_eq!(get_type(&response), "percentage");

    // Use variable in new expression
    let response = send_request(&mut stdin, &mut stdout, eval_request("100 + tax", 2));
    assert!(
        get_display(&response).starts_with("115"),
        "expected 115, got {}",
        get_display(&response)
    );

    // Define another variable
    let response = send_request(&mut stdin, &mut stdout, eval_request("price = $50", 3));
    assert_eq!(get_type(&response), "currency");

    // Use both variables
    let response = send_request(&mut stdin, &mut stdout, eval_request("price + tax", 4));
    assert_eq!(get_type(&response), "currency");

    drop(stdin);
    child.wait().unwrap();
}

/// Test: Error recovery - user types invalid then fixes
#[test]
fn test_error_recovery() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Invalid expression
    let response = send_request(&mut stdin, &mut stdout, eval_request("10 +", 1));
    assert_eq!(get_type(&response), "error");

    // User continues typing - now valid
    let response = send_request(&mut stdin, &mut stdout, eval_request("10 + 5", 2));
    assert_eq!(get_display(&response), "15");
    assert_eq!(get_type(&response), "number");

    // Another invalid
    let response = send_request(&mut stdin, &mut stdout, eval_request("unknown_var", 3));
    assert_eq!(get_type(&response), "error");

    // Fix by defining it
    let response = send_request(&mut stdin, &mut stdout, eval_request("unknown_var = 42", 4));
    assert_eq!(get_display(&response), "42");

    // Now it works
    let response = send_request(&mut stdin, &mut stdout, eval_request("unknown_var + 8", 5));
    assert_eq!(get_display(&response), "50");

    drop(stdin);
    child.wait().unwrap();
}

/// Test: Unit conversions
#[test]
fn test_incremental_unit_conversion() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let keystrokes = [
        "5",
        "5 ",
        "5 k",
        "5 km",
        "5 km ",
        "5 km i",
        "5 km in",
        "5 km in ",
        "5 km in m",
        "5 km in mi",
        "5 km in mil",
        "5 km in mile",
        "5 km in miles",
    ];

    for (i, partial) in keystrokes.iter().enumerate() {
        let response = send_request(&mut stdin, &mut stdout, eval_request(partial, i as u32));

        if *partial == "5 km" {
            assert_eq!(get_type(&response), "unit");
            assert!(get_display(&response).contains("km"));
        }

        if *partial == "5 km in miles" {
            assert_eq!(get_type(&response), "unit");
            assert!(get_display(&response).contains("mi"));
        }
    }

    drop(stdin);
    child.wait().unwrap();
}

/// Test: eval_lines for multi-line input (like paste or batch)
#[test]
fn test_eval_lines_batch() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let request = json!({
        "jsonrpc": "2.0",
        "method": "eval_lines",
        "params": {
            "lines": [
                "price = $100",
                "quantity = 5",
                "subtotal = price * quantity",
                "tax = 8%",
                "subtotal + tax"
            ]
        },
        "id": 1
    });

    let response = send_request(&mut stdin, &mut stdout, request);
    let results = response.get("result").unwrap().as_array().unwrap();

    assert_eq!(results.len(), 5);

    // price = $100
    assert_eq!(results[0]["type"], "currency");

    // quantity = 5
    assert_eq!(results[1]["type"], "number");

    // subtotal = $500
    assert_eq!(results[2]["type"], "currency");
    assert!(results[2]["display"].as_str().unwrap().contains("500"));

    // tax = 8%
    assert_eq!(results[3]["type"], "percentage");

    // subtotal + tax = $540
    assert_eq!(results[4]["type"], "currency");
    assert!(results[4]["display"].as_str().unwrap().contains("540"));

    drop(stdin);
    child.wait().unwrap();
}

/// Test: clear resets state
#[test]
fn test_clear_resets_state() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Define variable
    send_request(&mut stdin, &mut stdout, eval_request("x = 100", 1));

    // Verify it works
    let response = send_request(&mut stdin, &mut stdout, eval_request("x + 50", 2));
    assert_eq!(get_display(&response), "150");

    // Clear
    let clear_request = json!({
        "jsonrpc": "2.0",
        "method": "clear",
        "id": 3
    });
    let response = send_request(&mut stdin, &mut stdout, clear_request);
    assert!(response.get("result").is_some());

    // Variable should be gone
    let response = send_request(&mut stdin, &mut stdout, eval_request("x", 4));
    assert_eq!(get_type(&response), "error");

    drop(stdin);
    child.wait().unwrap();
}

/// Test: get_variables returns defined variables
#[test]
fn test_get_variables() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Define some variables
    send_request(&mut stdin, &mut stdout, eval_request("price = $100", 1));
    send_request(&mut stdin, &mut stdout, eval_request("tax = 15%", 2));
    send_request(&mut stdin, &mut stdout, eval_request("qty = 3", 3));

    // Get variables
    let request = json!({
        "jsonrpc": "2.0",
        "method": "get_variables",
        "id": 4
    });
    let response = send_request(&mut stdin, &mut stdout, request);
    let vars = response.get("result").unwrap().as_array().unwrap();

    assert_eq!(vars.len(), 3);

    let names: Vec<&str> = vars.iter().map(|v| v["name"].as_str().unwrap()).collect();

    assert!(names.contains(&"price"));
    assert!(names.contains(&"tax"));
    assert!(names.contains(&"qty"));

    drop(stdin);
    child.wait().unwrap();
}

/// Test: get_totals returns grouped totals
#[test]
fn test_get_totals() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Add some values
    send_request(&mut stdin, &mut stdout, eval_request("$100", 1));
    send_request(&mut stdin, &mut stdout, eval_request("$50", 2));
    send_request(&mut stdin, &mut stdout, eval_request("5 km", 3));
    send_request(&mut stdin, &mut stdout, eval_request("3 km", 4));

    // Get totals
    let request = json!({
        "jsonrpc": "2.0",
        "method": "get_totals",
        "id": 5
    });
    let response = send_request(&mut stdin, &mut stdout, request);
    let totals = response.get("result").unwrap().as_array().unwrap();

    // Should have currency total and unit total
    assert_eq!(totals.len(), 2);

    let types: Vec<&str> = totals.iter().map(|v| v["type"].as_str().unwrap()).collect();

    assert!(types.contains(&"currency"));
    assert!(types.contains(&"unit"));

    drop(stdin);
    child.wait().unwrap();
}

/// Test: Rapid sequential requests (simulates fast typing)
#[test]
fn test_rapid_sequential_requests() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Send 50 rapid requests
    for i in 0..50 {
        let expr = format!("{} + {}", i, i + 1);
        let response = send_request(&mut stdin, &mut stdout, eval_request(&expr, i));

        let expected = (i + i + 1) as f64;
        let value = get_result(&response)["value"].as_f64().unwrap();
        assert!((value - expected).abs() < 0.01);
    }

    drop(stdin);
    child.wait().unwrap();
}

/// Test: Empty and whitespace expressions
#[test]
fn test_empty_expressions() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Empty string
    let response = send_request(&mut stdin, &mut stdout, eval_request("", 1));
    assert_eq!(get_type(&response), "empty");

    // Whitespace only
    let response = send_request(&mut stdin, &mut stdout, eval_request("   ", 2));
    assert_eq!(get_type(&response), "empty");

    // Comment
    let response = send_request(
        &mut stdin,
        &mut stdout,
        eval_request("# this is a comment", 3),
    );
    assert_eq!(get_type(&response), "empty");

    drop(stdin);
    child.wait().unwrap();
}

/// Test: Complex real-world scenario - budget calculation
#[test]
fn test_budget_scenario() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // User builds up a budget calculation
    let steps = [
        ("rent = $1500", "currency"),
        ("utilities = $200", "currency"),
        ("groceries = $400", "currency"),
        ("transport = $150", "currency"),
        ("entertainment = $100", "currency"),
        (
            "monthly = rent + utilities + groceries + transport + entertainment",
            "currency",
        ),
        ("yearly = monthly * 12", "currency"),
        ("savings_rate = 20%", "percentage"),
        ("income = $4000", "currency"),
        ("savings = income - monthly", "currency"),
    ];

    for (i, (expr, expected_type)) in steps.iter().enumerate() {
        let response = send_request(&mut stdin, &mut stdout, eval_request(expr, i as u32));
        assert_eq!(
            get_type(&response),
            *expected_type,
            "failed at step: {}",
            expr
        );
    }

    // Final check: savings should be $4000 - $2350 = $1650
    let response = send_request(&mut stdin, &mut stdout, eval_request("savings", 100));
    assert!(get_display(&response).contains("1650") || get_display(&response).contains("1,650"));

    drop(stdin);
    child.wait().unwrap();
}

/// Test: Invalid JSON-RPC requests
#[test]
fn test_invalid_requests() {
    let mut child = spawn_server();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Missing method
    let request = json!({
        "jsonrpc": "2.0",
        "params": {"expr": "10"},
        "id": 1
    });
    writeln!(stdin, "{}", request).unwrap();
    stdin.flush().unwrap();

    let mut response = String::new();
    stdout.read_line(&mut response).unwrap();
    let response: Value = serde_json::from_str(&response).unwrap();
    assert!(response.get("error").is_some());

    // Unknown method
    let request = json!({
        "jsonrpc": "2.0",
        "method": "unknown_method",
        "id": 2
    });
    let response = send_request(&mut stdin, &mut stdout, request);
    assert!(response.get("error").is_some());
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not found"));

    // Wrong jsonrpc version
    let request = json!({
        "jsonrpc": "1.0",
        "method": "eval",
        "params": {"expr": "10"},
        "id": 3
    });
    let response = send_request(&mut stdin, &mut stdout, request);
    assert!(response.get("error").is_some());

    drop(stdin);
    child.wait().unwrap();
}
