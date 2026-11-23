//! End-to-end: spawn the real binary and speak MCP over its stdin/stdout.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn stdio_session_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("e2e.engram");
    let bin = env!("CARGO_BIN_EXE_engram-mcp");
    assert!(Command::new(bin).args(["init"]).arg(&file).args(["--name", "E2E"]).status().unwrap().success());

    let mut child = Command::new(bin)
        .arg("--file")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut rpc = |msg: Value| -> Option<Value> {
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        msg.get("id")?;
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        Some(serde_json::from_str(&line).unwrap())
    };

    let init = rpc(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}})).unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(rpc(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    let r = rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"upsert_memory","arguments":{"key":"lang","category":"tech_stack","content":"Rust 2021 + SwiftUI"}}})).unwrap();
    assert_eq!(r["result"]["isError"], false);
    let r = rpc(json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"engram://memory"}})).unwrap();
    assert!(r["result"]["contents"][0]["text"].as_str().unwrap().contains("Rust 2021 + SwiftUI"));
    drop(stdin);
    assert!(child.wait().unwrap().success());

    let on_disk = std::fs::read_to_string(&file).unwrap();
    assert!(on_disk.contains("\"key\": \"lang\""));
}
