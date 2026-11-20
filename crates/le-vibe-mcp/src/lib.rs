use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
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

impl McpClient {
    pub fn spawn(command: &str, args: &[&str]) -> Result<Self, McpError> {
        let mut child = Command::new(command)
            .args(args)
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
}
