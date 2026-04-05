#![cfg(feature = "mcp")]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

/// Send a JSON-RPC request (bare JSON line, no Content-Length framing).
fn send_jsonrpc(stdin: &mut impl Write, id: u64, method: &str, params: serde_json::Value) {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let body = serde_json::to_string(&msg).unwrap();
    stdin.write_all(body.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();
}

/// Send a JSON-RPC notification (no id, bare JSON line).
fn send_notification(stdin: &mut impl Write, method: &str) {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
    });
    let body = serde_json::to_string(&msg).unwrap();
    stdin.write_all(body.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();
}

/// Read one JSON-RPC response line from stdout.
fn read_response(stdout: &mut BufReader<impl Read>) -> serde_json::Value {
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

#[test]
fn mcp_server_responds_to_initialize() {
    // Build first
    let status = Command::new("cargo")
        .args(["build", "--features", "mcp"])
        .status()
        .expect("failed to build");
    assert!(status.success());

    // Find the binary
    let binary = std::env::current_dir().unwrap().join("target/debug/ccmd");

    let mut child = Command::new(&binary)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start mcp server");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // 1. Initialize
    send_jsonrpc(
        &mut stdin,
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1" }
        }),
    );

    let response = read_response(&mut stdout);
    assert_eq!(response["id"], 1);
    let server_name = response["result"]["serverInfo"]["name"]
        .as_str()
        .expect("serverInfo.name should be a string");
    assert!(
        server_name.contains("ccmd"),
        "Server info name should contain 'ccmd', got: {server_name:?}"
    );

    // 2. Send initialized notification
    send_notification(&mut stdin, "notifications/initialized");

    // 3. List tools
    send_jsonrpc(&mut stdin, 2, "tools/list", serde_json::json!({}));

    let response = read_response(&mut stdout);
    assert_eq!(response["id"], 2);
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools should be an array");
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    assert!(tool_names.contains(&"list_caches"), "Missing list_caches");
    assert!(tool_names.contains(&"get_summary"), "Missing get_summary");
    assert!(
        tool_names.contains(&"search_packages"),
        "Missing search_packages"
    );
    assert!(
        tool_names.contains(&"get_package_details"),
        "Missing get_package_details"
    );
    assert!(
        tool_names.contains(&"scan_vulnerabilities"),
        "Missing scan_vulnerabilities"
    );
    assert!(
        tool_names.contains(&"check_outdated"),
        "Missing check_outdated"
    );
    assert!(
        tool_names.contains(&"delete_packages"),
        "Missing delete_packages"
    );
    assert!(
        tool_names.contains(&"preview_delete"),
        "Missing preview_delete"
    );
    assert_eq!(
        tools.len(),
        8,
        "Expected exactly 8 tools, got {}",
        tools.len()
    );

    // 4. Clean shutdown
    drop(stdin);
    let _ = child.wait();
}
