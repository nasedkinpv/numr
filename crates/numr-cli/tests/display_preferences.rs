//! Verify the real CLI reads native preferences without changing protocol values.
#![cfg(target_os = "linux")]

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn server_reads_native_config_and_warns_on_invalid_precision() {
    let root = std::env::temp_dir().join(format!("numr-display-config-{}", std::process::id()));
    let config_dir = root.join("numr");
    std::fs::create_dir_all(&config_dir).unwrap();
    for (setting, expected, warning) in [("4", "0.3333", false), ("29", "0.33", true)] {
        std::fs::write(
            config_dir.join("config.toml"),
            format!("[preferences]\ndecimals = {setting}\nwrap_mode = true\n"),
        )
        .unwrap();
        let mut process = Command::new(env!("CARGO_BIN_EXE_numr-cli"))
            .arg("--server")
            .env("XDG_CONFIG_HOME", &root)
            .env("XDG_CACHE_HOME", root.join("cache"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        process.stdin.take().unwrap().write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eval\",\"params\":{\"expr\":\"1 / 3\"}}\n").unwrap();
        let output = process.wait_with_output().unwrap();
        assert!(output.status.success());
        let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["result"]["display"], expected);
        assert_eq!(response["result"]["value"], "0.33");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).contains("Config parse error"),
            warning
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn expression_pipe_file_and_repl_share_precision() {
    let root = std::env::temp_dir().join(format!("numr-display-modes-{}", std::process::id()));
    let config_dir = root.join("numr");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        "[preferences]\ndecimals = 4\n",
    )
    .unwrap();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(
        config_dir.join("rates.json"),
        serde_json::json!({"timestamp": timestamp, "rates": {"EUR": "0.92"}}).to_string(),
    )
    .unwrap();
    let file = root.join("example.numr");
    std::fs::write(&file, "1 / 3\n").unwrap();
    for (args, stdin, expected) in [
        (
            vec!["1 / 3", "--total"],
            "",
            "0.3333\n─────────────\nTotal: 0.3333\n",
        ),
        (vec![], "1 / 3\n", "0.3333\n"),
        (vec!["-f", file.to_str().unwrap()], "", " = 0.3333\n"),
        (vec!["-i"], "1 / 3\ntotal\nquit\n", "Total: 0.3333\n"),
    ] {
        let mut process = Command::new(env!("CARGO_BIN_EXE_numr-cli"))
            .args(args)
            .env("XDG_CONFIG_HOME", &root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        process
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let output = process.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
