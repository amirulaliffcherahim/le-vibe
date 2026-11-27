use le_vibe_pybridge::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("mcp server closed stdout")]
    Closed,
    #[error("mcp rpc error ({code}): {message}")]
    Rpc { code: i64, message: String },
    #[error("mcp request timed out after {seconds}s")]
    Timeout { seconds: u64 },
    /// A successful RPC whose result carries `isError: true`.
    #[error("mcp tool error: {0}")]
    Tool(String),
}

/// Default per-request timeout, matching pi's `mcpServers[].timeout` default.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Value,
}

pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    /// Lines produced by a dedicated reader thread, so a wedged server cannot block the caller.
    responses: std::sync::mpsc::Receiver<String>,
    next_id: u64,
    timeout: std::time::Duration,
}

fn build_platform_command(command: &str, args: &[String]) -> Command {
    if cfg!(windows) && !command.contains('\\') && !command.contains('/') && !command.contains('.') {
        // Resolve `.cmd`/`.bat` shims (e.g. `npx`, `npm`, `uvx`) so Windows `Command::new` finds them.
        if let Ok(out) = Command::new("where.exe").arg(command).output()
            && out.status.success()
        {
            let candidates: Vec<String> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if let Some(cmd_shim) = candidates.iter().find(|p| {
                let lower = p.to_ascii_lowercase();
                lower.ends_with(".cmd") || lower.ends_with(".bat")
            }) {
                let mut cmd = Command::new("cmd");
                cmd.arg("/C").arg(cmd_shim).args(args);
                return cmd;
            }
            if let Some(exe) = candidates.iter().find(|p| {
                let lower = p.to_ascii_lowercase();
                lower.ends_with(".exe") || lower.ends_with(".com")
            }) {
                let mut cmd = Command::new(exe);
                cmd.args(args);
                return cmd;
            }
        }
    }
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd
}

impl McpClient {
    pub fn spawn(command: &str, args: &[&str]) -> Result<Self, McpError> {
        let owned_args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        let mut cmd = build_platform_command(command, &owned_args);
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().ok_or(McpError::Closed)?;
        Self::from_child(child, stdin)
    }

    /// Hands stdout to a reader thread so every request can be awaited with a deadline.
    ///
    /// A blocking `read_line` has no timeout, so one unresponsive server would freeze the session.
    fn from_child(mut child: Child, stdin: ChildStdin) -> Result<Self, McpError> {
        let stdout = child.stdout.take().ok_or(McpError::Closed)?;
        let (tx, responses) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            responses,
            next_id: 1,
            timeout: std::time::Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
        })
    }

    pub fn spawn_with_env(cfg: &McpServerConfig) -> Result<Self, McpError> {
        let mut cmd = build_platform_command(&cfg.command, &cfg.args);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in &cfg.env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or(McpError::Closed)?;
        Self::from_child(child, stdin)
    }

    /// Override the per-request timeout (pi's `mcpServers[].timeout`, in seconds).
    pub fn set_timeout_secs(&mut self, secs: u64) {
        if secs > 0 {
            self.timeout = std::time::Duration::from_secs(secs);
        }
    }

    fn send_notification(&mut self, method: &str, params: Value) -> Result<(), McpError> {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let mut line = serde_json::to_string(&msg)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let mut line = serde_json::to_string(&msg)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;

        // Wait with a deadline; a server that never answers must not freeze the session. Lines that
        // are not the reply we asked for (notifications, progress) are skipped, not misread.
        let timeout_secs = self.timeout.as_secs();
        loop {
            // Each received line (including a progress notification) restarts the deadline, which is
            // pi's rule: progress proves the server is working, so a slow job is not a timeout.
            let resp_line = self
                .responses
                .recv_timeout(self.timeout)
                .map_err(|err| match err {
                    std::sync::mpsc::RecvTimeoutError::Timeout => McpError::Timeout {
                        seconds: timeout_secs,
                    },
                    std::sync::mpsc::RecvTimeoutError::Disconnected => McpError::Closed,
                })?;
            if resp_line.trim().is_empty() {
                continue;
            }
            let Ok(resp) = serde_json::from_str::<Value>(&resp_line) else {
                continue;
            };
            if resp.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(err) = resp.get("error") {
                return Err(McpError::Rpc {
                    code: err.get("code").and_then(Value::as_i64).unwrap_or(-1),
                    message: err
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error")
                        .to_string(),
                });
            }
            return Ok(resp.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    pub fn initialize(&mut self) -> Result<Value, McpError> {
        let res = self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "le-vibe", "version": "0.1.0" }
            }),
        )?;
        self.send_notification("notifications/initialized", json!({}))?;
        Ok(res)
    }

    pub fn list_tools(&mut self) -> Result<Vec<McpTool>, McpError> {
        let res = self.request("tools/list", json!({}))?;
        let tools_val = res.get("tools").cloned().unwrap_or_else(|| json!([]));
        Ok(serde_json::from_value(tools_val)?)
    }

    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, McpError> {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Per-request timeout in seconds (pi's `mcpServers[].timeout`, default 60).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Whether to connect at all (pi's `mcpServers[].enabled`, default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Human-readable description shown in status listings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McpConfigFile {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: BTreeMap<String, McpServerConfig>,
}

struct ConnectedServer {
    client: McpClient,
    tools: Vec<McpTool>,
}

#[derive(Default)]
pub struct McpManager {
    servers: BTreeMap<String, ConnectedServer>,
    statuses: BTreeMap<String, String>,
}

impl McpManager {
    pub fn from_config_files(paths: &[PathBuf]) -> Self {
        let mut merged: BTreeMap<String, McpServerConfig> = BTreeMap::new();
        for path in paths {
            if let Ok(raw) = std::fs::read_to_string(path)
                && let Ok(parsed) = serde_json::from_str::<McpConfigFile>(&raw)
            {
                for (name, cfg) in parsed.mcp_servers {
                    merged.insert(name, cfg);
                }
            }
        }
        Self::from_servers(merged)
    }

    pub fn from_config_json(raw: &str) -> Result<Self, McpError> {
        let parsed: McpConfigFile = serde_json::from_str(raw)?;
        Ok(Self::from_servers(parsed.mcp_servers))
    }

    fn from_servers(configs: BTreeMap<String, McpServerConfig>) -> Self {
        let mut manager = Self::default();
        for (name, cfg) in configs {
            if cfg.enabled == Some(false) {
                manager
                    .statuses
                    .insert(name, "disabled (enabled: false)".to_string());
                continue;
            }
            match McpClient::spawn_with_env(&cfg) {
                Ok(mut client) => {
                    if let Some(secs) = cfg.timeout {
                        client.set_timeout_secs(secs);
                    }
                    match client.initialize().and_then(|_| client.list_tools()) {
                    Ok(tools) => {
                        manager.statuses.insert(
                            name.clone(),
                            format!("connected ({} tools)", tools.len()),
                        );
                        manager.servers.insert(name, ConnectedServer { client, tools });
                    }
                        Err(err) => {
                            manager.statuses.insert(name, format!("error: {err}"));
                        }
                    }
                }
                Err(err) => {
                    manager.statuses.insert(name, format!("spawn error: {err}"));
                }
            }
        }
        manager
    }

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = Vec::new();
        for (server_name, server) in &self.servers {
            for tool in &server.tools {
                defs.push(ToolDefinition {
                    name: format!("mcp__{server_name}__{}", tool.name),
                    description: if tool.description.is_empty() {
                        format!("MCP tool `{}` on server `{server_name}`", tool.name)
                    } else {
                        format!("[MCP:{server_name}] {}", tool.description)
                    },
                    parameters: tool.input_schema.clone(),
                });
            }
        }
        defs
    }

    pub fn call_namespaced_tool(&mut self, full_name: &str, args: Value) -> Result<String, McpError> {
        let rest = full_name
            .strip_prefix("mcp__")
            .ok_or_else(|| McpError::Rpc {
                code: -32602,
                message: format!("invalid mcp tool name: {full_name}"),
            })?;
        let (server_name, tool_name) = rest.split_once("__").ok_or_else(|| McpError::Rpc {
            code: -32602,
            message: format!("invalid mcp tool name: {full_name}"),
        })?;
        let server = self.servers.get_mut(server_name).ok_or_else(|| McpError::Rpc {
            code: -32001,
            message: format!("mcp server not connected: {server_name}"),
        })?;
        let res = server.client.call_tool(tool_name, args)?;
        let text = match res.get("content").and_then(Value::as_array) {
            Some(items) => {
                let texts: Vec<&str> = items
                    .iter()
                    .filter_map(|item| item.get("text").and_then(Value::as_str))
                    .collect();
                if texts.is_empty() {
                    serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string())
                } else {
                    texts.join("\n")
                }
            }
            None => serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string()),
        };
        // MCP reports tool-level failure in-band, so without this a broken server looks like success.
        if res.get("isError").and_then(Value::as_bool) == Some(true) {
            return Err(McpError::Tool(text));
        }
        Ok(text)
    }

    pub fn status_summary(&self) -> Vec<(String, String)> {
        self.statuses
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_mcp_initialize_list_and_call() {
        let script = r#"
import sys, json
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    m = msg["method"]
    if m == "initialize":
        res = {"protocolVersion": "2024-11-05", "serverInfo": {"name": "mock", "version": "1.0"}}
    elif m == "tools/list":
        res = {"tools": [{"name": "ping", "description": "Ping tool", "inputSchema": {"type": "object"}}]}
    elif m == "tools/call":
        res = {"content": [{"type": "text", "text": "pong:" + msg["params"]["arguments"]["x"]}]}
    else:
        res = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": res}) + "\n")
    sys.stdout.flush()
"#;
        let mut client = McpClient::spawn("python", &["-c", script]).expect("spawn mock mcp");
        let init = client.initialize().expect("initialize");
        assert_eq!(init["serverInfo"]["name"], "mock");
        let tools = client.list_tools().expect("list_tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "ping");
        let out = client.call_tool("ping", json!({"x": "ok"})).expect("call_tool");
        assert_eq!(out["content"][0]["text"], "pong:ok");
    }

    #[test]
    fn propagates_jsonrpc_error_responses() {
        let script = r#"
import sys, json
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "error": {"code": -32601, "message": "method missing"}}) + "\n")
    sys.stdout.flush()
"#;
        let mut client = McpClient::spawn("python", &["-c", script]).expect("spawn mock mcp");
        let err = client.list_tools().unwrap_err();
        assert!(matches!(err, McpError::Rpc { code: -32601, .. }));
    }

    #[test]
    fn mcp_manager_loads_config_and_calls_namespaced_tool() {
        let script = r#"
import sys, json
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    m = msg["method"]
    if m == "initialize":
        res = {"protocolVersion": "2024-11-05", "serverInfo": {"name": "demo", "version": "1.0"}}
    elif m == "tools/list":
        res = {"tools": [{"name": "echo", "description": "Echo text", "inputSchema": {"type": "object"}}]}
    elif m == "tools/call":
        res = {"content": [{"type": "text", "text": "echo:" + msg["params"]["arguments"]["msg"]}]}
    else:
        res = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": res}) + "\n")
    sys.stdout.flush()
"#;
        let cfg = json!({
            "mcpServers": {
                "demo": {
                    "command": "python",
                    "args": ["-c", script]
                }
            }
        });
        let mut mgr = McpManager::from_config_json(&cfg.to_string()).expect("load config");
        assert_eq!(
            mgr.status_summary(),
            vec![("demo".into(), "connected (1 tools)".into())]
        );
        let defs = mgr.tool_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "mcp__demo__echo");
        let out = mgr
            .call_namespaced_tool("mcp__demo__echo", json!({"msg": "hello"}))
            .expect("call namespaced");
        assert_eq!(out, "echo:hello");
    }
}
