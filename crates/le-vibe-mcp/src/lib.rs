use le_vibe_pybridge::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
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
}

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
    stdout: BufReader<ChildStdout>,
    next_id: u64,
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
        let stdout = BufReader::new(child.stdout.take().ok_or(McpError::Closed)?);
        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: 1,
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
        let stdout = BufReader::new(child.stdout.take().ok_or(McpError::Closed)?);
        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        })
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

        let mut resp_line = String::new();
        let read = self.stdout.read_line(&mut resp_line)?;
        if read == 0 {
            return Err(McpError::Closed);
        }
        let resp: Value = serde_json::from_str(&resp_line)?;
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
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
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
            match McpClient::spawn_with_env(&cfg) {
                Ok(mut client) => match client.initialize().and_then(|_| client.list_tools()) {
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
                },
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
        if let Some(items) = res.get("content").and_then(Value::as_array) {
            let texts: Vec<&str> = items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect();
            if !texts.is_empty() {
                return Ok(texts.join("\n"));
            }
        }
        Ok(serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string()))
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
