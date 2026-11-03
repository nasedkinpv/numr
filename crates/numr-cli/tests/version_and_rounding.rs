//! Version flags must work before configuration, rates, stdin or terminal I/O.
use std::process::{Command, Stdio};

#[test]
fn cli_version_flags_print_the_workspace_version() {
    for flag in ["--version", "-V"] {
        let output = Command::new(env!("CARGO_BIN_EXE_numr-cli"))
            .arg(flag)
            .env(
                "XDG_CONFIG_HOME",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
            )
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("numr-cli {}\n", env!("CARGO_PKG_VERSION"))
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn server_uses_rounded_values_in_subsequent_calculations() {
    use std::io::Write;
    let mut process = Command::new(env!("CARGO_BIN_EXE_numr-cli"))
        .arg("--server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process.stdin.take().unwrap().write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eval\",\"params\":{\"expr\":\"rounded = round(1.234567, 4)\"}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"eval\",\"params\":{\"expr\":\"rounded * 10000\"}}\n").unwrap();
    let output = process.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["result"]["value"], "1.23");
    assert_eq!(responses[1]["result"]["value"], "12346");
}
