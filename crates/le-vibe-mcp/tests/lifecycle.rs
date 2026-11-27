//! MCP lifecycle contract parity with pi's `mcp-servers.ts`:
//! - every request has a timeout (default 60 s, per-server `timeout` in seconds) so one wedged
//!   server cannot freeze the session;
//! - a tool result with `isError: true` surfaces as a tool error, not as normal output.

use le_vibe_mcp::McpManager;
use serde_json::json;

/// A mock server that answers `initialize`/`tools/list` and then, depending on argv[1]:
/// - `hang`  : accepts `tools/call` and never replies
/// - `iserr` : replies with `{isError: true, content: [{type: text, text: "boom"}]}`
/// - `ok`    : replies with plain text content
fn mock_script() -> &'static str {
    r#"
import sys, json, time
mode = sys.argv[1] if len(sys.argv) > 1 else "ok"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if "id" not in msg:
        continue
    m = msg["method"]
    if m == "initialize":
        res = {"protocolVersion": "2024-11-05", "serverInfo": {"name": "mock", "version": "1"}}
    elif m == "tools/list":
        res = {"tools": [{"name": "ping", "description": "Ping", "inputSchema": {"type": "object"}}]}
    elif m == "tools/call":
        if mode == "hang":
            time.sleep(3600)
            continue
        if mode == "iserr":
            res = {"isError": True, "content": [{"type": "text", "text": "boom"}]}
        else:
            res = {"content": [{"type": "text", "text": "pong"}]}
    else:
        res = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": res}), flush=True)
"#
}

fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

fn manager(mode: &str, timeout: Option<u64>) -> McpManager {
    let script = std::env::temp_dir().join(format!("le_vibe_mcp_mock_{mode}.py"));
    std::fs::write(&script, mock_script()).expect("write mock");
    let mut args = json!([script.display().to_string(), mode]);
    let mut server = json!({
        "command": python(),
        "args": args.take(),
    });
    if let Some(t) = timeout {
        server["timeout"] = json!(t);
    }
    let config = json!({ "mcpServers": { "mock": server } });
    McpManager::from_config_json(&config.to_string()).expect("manager")
}

#[test]
fn a_healthy_server_round_trips() {
    let mut mgr = manager("ok", None);
    let out = mgr
        .call_namespaced_tool("mcp__mock__ping", json!({}))
        .expect("call");
    assert!(out.contains("pong"), "{out}");
}

#[test]
fn tool_level_is_error_surfaces_as_a_tool_error() {
    let mut mgr = manager("iserr", None);
    let err = mgr
        .call_namespaced_tool("mcp__mock__ping", json!({}))
        .expect_err("isError must not look like success");
    let text = err.to_string();
    assert!(text.contains("boom"), "server message must survive: {text}");
}

#[test]
fn a_hanging_request_times_out_instead_of_blocking_forever() {
    let mut mgr = manager("hang", Some(1));
    let started = std::time::Instant::now();
    let err = mgr
        .call_namespaced_tool("mcp__mock__ping", json!({}))
        .expect_err("a wedged server must produce a timeout error");
    let elapsed = started.elapsed();
    let text = err.to_string();
    assert!(text.contains("timed out"), "{text}");
    assert!(
        elapsed < std::time::Duration::from_secs(20),
        "timeout must actually fire, took {elapsed:?}"
    );
}

#[test]
fn per_server_timeout_is_honoured_over_the_default() {
    // A 1s override must fire well before the 60s default.
    let mut mgr = manager("hang", Some(1));
    let started = std::time::Instant::now();
    let _ = mgr.call_namespaced_tool("mcp__mock__ping", json!({}));
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
}

#[test]
fn unknown_server_and_bad_names_are_reported_clearly() {
    let mut mgr = manager("ok", None);
    let err = mgr
        .call_namespaced_tool("mcp__missing__thing", json!({}))
        .expect_err("unknown server");
    assert!(err.to_string().contains("not connected"), "{err}");
}
