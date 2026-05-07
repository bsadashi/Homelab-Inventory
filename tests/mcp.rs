//! Subprocess test for the `racklog-mcp` binary.
//!
//! tools/list is a local-only JSON-RPC method (it doesn't reach
//! out to the RACKLOG HTTP server), so we can exercise the protocol
//! handshake without running a backend. Anchoring the tool count
//! catches accidental drift when someone adds or removes a method
//! from `tools_descriptor` and forgets the corresponding API
//! mapping in `dispatch_tool`.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

const EXPECTED_TOOLS: &[&str] = &[
    "list_items",
    "get_item",
    "lookup_barcode",
    "stats",
    "list_locations",
    "list_suppliers",
    "recent_activity",
    "verify_chain",
    "forecast_reorder",
];

#[test]
fn tools_list_returns_expected_tools() {
    let exe = env!("CARGO_BIN_EXE_racklog-mcp");

    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // RACKLOG_URL is read at startup but tools/list never hits
        // it, so a bogus value is fine and keeps the test hermetic.
        .env("RACKLOG_URL", "http://127.0.0.1:1")
        .spawn()
        .expect("spawn racklog-mcp");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let list = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        writeln!(stdin, "{}", init).unwrap();
        writeln!(stdin, "{}", list).unwrap();
        // Closing stdin signals EOF — main()'s reader loop exits
        // and the child shuts down cleanly without us having to
        // kill it.
    }
    drop(child.stdin.take());

    // Bound the wait so a hanging child doesn't stall the test.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        match child.try_wait().expect("try_wait") {
            Some(_) => break,
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    if child.try_wait().expect("try_wait").is_none() {
        let _ = child.kill();
        panic!("racklog-mcp did not exit within 5s after stdin close");
    }
    let output = child.wait_with_output().expect("wait_with_output");
    assert!(
        output.status.success(),
        "racklog-mcp exited with {:?}, stderr=\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let mut tools_resp: Option<serde_json::Value> = None;
    for line in stdout.lines() {
        let v: serde_json::Value = serde_json::from_str(line).expect("ndjson line");
        if v.get("id") == Some(&serde_json::json!(2)) {
            tools_resp = Some(v);
            break;
        }
    }
    let tools_resp = tools_resp.expect("tools/list response");
    let tools = tools_resp["result"]["tools"]
        .as_array()
        .expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        names.len(),
        EXPECTED_TOOLS.len(),
        "tool count drifted: got {names:?}",
    );
    for name in EXPECTED_TOOLS {
        assert!(
            names.contains(name),
            "expected tool {name} missing from {names:?}",
        );
    }
}
