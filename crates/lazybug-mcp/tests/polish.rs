//! MCP config polish: `enabled` / `description`, and progress notifications resetting the timeout
//! (pi's `mcp-servers.ts`: “Progress notifications from the server reset it”).

use lazybug_mcp::McpManager;
use serde_json::json;

fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

fn write_mock(mode: &str) -> String {
    let script = r#"
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
        if mode == "progress":
            # Six notifications at 300ms each = 1.8s total, well past a 1s timeout unless each one
            # resets the deadline.
            for i in range(6):
                time.sleep(0.3)
                print(json.dumps({"jsonrpc": "2.0", "method": "notifications/progress",
                                  "params": {"progress": i}}), flush=True)
            res = {"content": [{"type": "text", "text": "slow-pong"}]}
        else:
            res = {"content": [{"type": "text", "text": "pong"}]}
    else:
        res = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": res}), flush=True)
"#;
    let path = std::env::temp_dir().join(format!("lazybug_mcp_polish_{mode}.py"));
    std::fs::write(&path, script).expect("write mock");
    path.display().to_string()
}

fn manager(mode: &str, timeout: u64) -> McpManager {
    let script = write_mock(mode);
    let config = json!({
        "mcpServers": {
            "mock": {
                "command": python(),
                "args": [script, mode],
                "timeout": timeout,
            }
        }
    });
    McpManager::from_config_json(&config.to_string()).expect("manager")
}

#[test]
fn progress_notifications_reset_the_request_timeout() {
    // 1s timeout, but the server keeps sending progress for ~1.8s before answering.
    let mut mgr = manager("progress", 1);
    let started = std::time::Instant::now();
    let out = mgr
        .call_namespaced_tool("mcp__mock__ping", json!({}))
        .expect("progress notifications must keep the request alive");
    assert!(out.contains("slow-pong"), "{out}");
    assert!(
        started.elapsed() >= std::time::Duration::from_millis(1_500),
        "the server really was slow: {:?}",
        started.elapsed()
    );
}

#[test]
fn a_disabled_server_is_not_spawned() {
    let script = write_mock("ok");
    let config = json!({
        "mcpServers": {
            "off": {
                "command": python(),
                "args": [script, "ok"],
                "enabled": false,
                "description": "not needed today",
            }
        }
    });
    let mut mgr = McpManager::from_config_json(&config.to_string()).expect("manager");
    let err = mgr
        .call_namespaced_tool("mcp__off__ping", json!({}))
        .expect_err("a disabled server must not be callable");
    assert!(err.to_string().contains("not connected"), "{err}");
    // The status list explains why rather than reporting a spawn failure.
    let status = mgr
        .status_summary()
        .into_iter()
        .find(|(name, _)| name == "off")
        .map(|(_, s)| s)
        .unwrap_or_default();
    assert!(status.contains("disabled"), "{status}");
}

#[test]
fn description_is_accepted_and_does_not_break_connection() {
    let script = write_mock("ok");
    let config = json!({
        "mcpServers": {
            "mock": {
                "command": python(),
                "args": [script, "ok"],
                "description": "Ko-fi donations and supporters",
                "enabled": true,
            }
        }
    });
    let mut mgr = McpManager::from_config_json(&config.to_string()).expect("manager");
    let out = mgr
        .call_namespaced_tool("mcp__mock__ping", json!({}))
        .expect("call");
    assert!(out.contains("pong"), "{out}");
}
