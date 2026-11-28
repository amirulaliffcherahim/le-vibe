use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("failed to spawn python bridge: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("python bridge closed stdout unexpectedly")]
    Closed,
    #[error("cancelled by user")]
    Cancelled,
    #[error("python error ({kind}): {message}")]
    Remote { kind: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub reasoning_tokens: u64,
    #[serde(default)]
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantMessage {
    pub provider: String,
    pub model: String,
    pub content: String,
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub reasoning: bool,
    pub context_window: u64,
    pub max_tokens: u64,
    #[serde(default)]
    pub exhausted: bool,
    #[serde(default = "default_medium_thinking")]
    pub default_thinking: String,
}

fn default_medium_thinking() -> String {
    "medium".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyDecision {
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub delta: String,
    pub tool_call: Option<ToolCall>,
    pub message: Option<AssistantMessage>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedModel {
    pub provider: String,
    pub model: String,
    pub thinking_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionRequest {
    pub provider: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    pub thinking_level: String,
}

pub trait ProviderPort {
    fn list_providers(&mut self) -> Result<Vec<String>, BridgeError>;
    fn list_models(&mut self, provider: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError>;
    fn complete(&mut self, request: CompletionRequest) -> Result<AssistantMessage, BridgeError>;
    fn web_search(&mut self, queries: &[String], num_results: usize) -> Result<String, BridgeError> {
        let _ = (queries, num_results);
        Err(BridgeError::Remote {
            kind: "Unsupported".to_string(),
            message: "web_search not implemented on this bridge".to_string(),
        })
    }
    fn fetch_content(&mut self, url: &str, mode: &str) -> Result<String, BridgeError> {
        let _ = (url, mode);
        Err(BridgeError::Remote {
            kind: "Unsupported".to_string(),
            message: "fetch_content not implemented on this bridge".to_string(),
        })
    }
    fn stream_complete(
        &mut self,
        request: CompletionRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<AssistantMessage, BridgeError> {
        let msg = self.complete(request)?;
        if !msg.content.is_empty() {
            on_event(StreamEvent {
                kind: "text_delta".to_string(),
                delta: msg.content.clone(),
                tool_call: None,
                message: None,
                error: None,
            });
        }
        on_event(StreamEvent {
            kind: "done".to_string(),
            delta: String::new(),
            tool_call: None,
            message: Some(msg.clone()),
            error: None,
        });
        Ok(msg)
    }
}

impl PythonBridge {
    pub fn save_api_key(&mut self, provider: &str, api_key: &str) -> Result<String, BridgeError> {
        let val = self.call(
            "providers.save_api_key",
            json!({ "provider": provider, "api_key": api_key }),
        )?;
        Ok(val
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    pub fn login_vibe_oidc(&mut self) -> Result<String, BridgeError> {
        let val = self.call("providers.login_vibe_oidc", json!({}))?;
        Ok(val
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    pub fn login_agy_oauth(&mut self) -> Result<String, BridgeError> {
        let val = self.call("providers.login_agy_oauth", json!({}))?;
        Ok(val
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    pub fn auth_status(&mut self) -> Result<Value, BridgeError> {
        self.call("providers.auth_status", json!({}))
    }

    pub fn logout(&mut self, provider: &str) -> Result<bool, BridgeError> {
        let val = self.call("providers.logout", json!({ "provider": provider }))?;
        Ok(val.get("removed").and_then(Value::as_bool).unwrap_or(false))
    }

    pub fn reload_extensions(&mut self) -> Result<Value, BridgeError> {
        self.call("extensions.reload", json!({}))
    }

    pub fn reload_policy(&mut self) -> Result<Value, BridgeError> {
        self.call("policy.reload", json!({}))
    }

    pub fn policy_status(&mut self) -> Result<Value, BridgeError> {
        self.call("policy.status", json!({}))
    }

    pub fn extensions_status(&mut self) -> Result<Value, BridgeError> {
        self.call("extensions.status", json!({}))
    }

    pub fn list_extension_commands(&mut self) -> Result<Vec<(String, String)>, BridgeError> {
        let val = self.call("extensions.list_commands", json!({}))?;
        let mut out = Vec::new();
        if let Some(arr) = val.as_array() {
            for item in arr {
                if let Some(name) = item.get("name").and_then(Value::as_str) {
                    let desc = item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    out.push((name.to_string(), desc));
                }
            }
        }
        Ok(out)
    }

    pub fn run_extension_command(&mut self, name: &str, args: &str) -> Result<String, BridgeError> {
        let val = self.call(
            "extensions.run_command",
            json!({ "name": name, "args": args }),
        )?;
        Ok(val
            .get("output")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }
}

pub trait PolicyPort {
    fn evaluate_tool_call(
        &mut self,
        tool_name: &str,
        command: Option<&str>,
        target_path: Option<&str>,
    ) -> Result<PolicyDecision, BridgeError>;

    fn set_yolo(&mut self, _enabled: bool) -> Result<(), BridgeError> {
        Ok(())
    }

    fn grant_rule(
        &mut self,
        _tool_name: &str,
        _command: Option<&str>,
        _target_path: Option<&str>,
    ) -> Result<(), BridgeError> {
        Ok(())
    }
}

pub trait ExtensionPort {
    fn invoke_hook(&mut self, event: &str, payload: Value) -> Result<Value, BridgeError>;

    fn list_extension_tools(&mut self) -> Result<Vec<ToolDefinition>, BridgeError> {
        Ok(Vec::new())
    }

    fn execute_extension_tool(
        &mut self,
        _name: &str,
        _arguments: Value,
    ) -> Result<Value, BridgeError> {
        Ok(Value::Null)
    }
}

pub struct PythonBridge {
    python_exe: String,
    python_dir: PathBuf,
    child: Child,
    stdin: ChildStdin,
    stdout_rx: mpsc::Receiver<std::io::Result<String>>,
    next_id: u64,
    cancel: Option<Arc<AtomicBool>>,
    yolo_enabled: bool,
    granted_rules: Vec<(String, Option<String>, Option<String>)>,
}

impl PythonBridge {
    fn spawn_process(
        python_exe: &str,
        python_dir: &Path,
    ) -> Result<(Child, ChildStdin, mpsc::Receiver<std::io::Result<String>>), BridgeError> {
        let mut cmd = Command::new(python_exe);
        cmd.arg("-m")
            .arg("le_vibe.bridge")
            .env("PYTHONPATH", python_dir)
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or(BridgeError::Closed)?;
        let stdout = child.stdout.take().ok_or(BridgeError::Closed)?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        break;
                    }
                }
            }
        });
        Ok((child, stdin, rx))
    }

    pub fn spawn(python_exe: &str, python_dir: &Path) -> Result<Self, BridgeError> {
        let (child, stdin, stdout_rx) = Self::spawn_process(python_exe, python_dir)?;
        Ok(Self {
            python_exe: python_exe.to_string(),
            python_dir: python_dir.to_path_buf(),
            child,
            stdin,
            stdout_rx,
            next_id: 1,
            cancel: None,
            yolo_enabled: false,
            granted_rules: Vec::new(),
        })
    }

    pub fn set_cancel_token(&mut self, cancel: Arc<AtomicBool>) {
        self.cancel = Some(cancel);
    }

    fn respawn(&mut self) -> Result<(), BridgeError> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let (child, stdin, stdout_rx) = Self::spawn_process(&self.python_exe, &self.python_dir)?;
        self.child = child;
        self.stdin = stdin;
        self.stdout_rx = stdout_rx;
        if self.yolo_enabled {
            let _ = self.call_uncancellable("policy.set_yolo", json!({ "enabled": true }));
        }
        let rules = self.granted_rules.clone();
        for (tool_name, command, target_path) in rules {
            let _ = self.call_uncancellable(
                "policy.grant_rule",
                json!({
                    "tool_name": tool_name,
                    "command": command,
                    "target_path": target_path,
                }),
            );
        }
        Ok(())
    }

    fn call_uncancellable(&mut self, method: &str, params: Value) -> Result<Value, BridgeError> {
        let id = self.next_id;
        self.next_id += 1;
        let req = json!({ "id": id, "method": method, "params": params });
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;
        match self.stdout_rx.recv() {
            Ok(Ok(resp_line)) => {
                let resp: Value = serde_json::from_str(&resp_line)?;
                Ok(resp.get("result").cloned().unwrap_or(Value::Null))
            }
            Ok(Err(e)) => Err(BridgeError::Spawn(e)),
            Err(_) => Err(BridgeError::Closed),
        }
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, BridgeError> {
        self.call_streaming(method, params, |_| {})
    }

    pub fn call_streaming(
        &mut self,
        method: &str,
        params: Value,
        mut on_stream: impl FnMut(Value),
    ) -> Result<Value, BridgeError> {
        let is_cancellable = matches!(
            method,
            "providers.stream"
                | "providers.complete"
                | "providers.refresh_models"
                | "providers.usage"
                | "providers.resolve_model"
        );
        let id = self.next_id;
        self.next_id += 1;

        let req = json!({ "id": id, "method": method, "params": params });
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;

        let mut last_activity = std::time::Instant::now();
        let mut received_any = false;
        let first_byte_timeout = Duration::from_secs(180);
        let mid_stream_timeout = Duration::from_secs(120);
        loop {
            if is_cancellable
                && self
                    .cancel
                    .as_ref()
                    .is_some_and(|c| c.load(Ordering::SeqCst))
            {
                let _ = self.respawn();
                return Err(BridgeError::Cancelled);
            }
            let resp_line = match self.stdout_rx.recv_timeout(Duration::from_millis(25)) {
                Ok(Ok(l)) => {
                    last_activity = std::time::Instant::now();
                    received_any = true;
                    l
                }
                Ok(Err(e)) => return Err(BridgeError::Spawn(e)),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let limit = if received_any {
                        mid_stream_timeout
                    } else {
                        first_byte_timeout
                    };
                    if is_cancellable && last_activity.elapsed() > limit {
                        let _ = self.respawn();
                        return Err(BridgeError::Remote {
                            kind: "StreamTimeout".to_string(),
                            message: format!(
                                "Provider stream stalled (no bytes received for {}s).",
                                limit.as_secs()
                            ),
                        });
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(BridgeError::Closed),
            };

            let resp: Value = serde_json::from_str(&resp_line)?;
            if resp.get("stream").and_then(Value::as_bool) == Some(true) {
                if let Some(ev) = resp.get("event").cloned() {
                    on_stream(ev);
                }
                continue;
            }
            if let Some(err) = resp.get("error") {
                return Err(BridgeError::Remote {
                    kind: err
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("Error")
                        .to_string(),
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

    pub fn resolve_model(
        &mut self,
        pattern: &str,
        default_provider: &str,
    ) -> Result<ResolvedModel, BridgeError> {
        let val = self.call(
            "providers.resolve_model",
            json!({ "pattern": pattern, "default_provider": default_provider }),
        )?;
        Ok(serde_json::from_value(val)?)
    }

    pub fn list_extension_tools(&mut self) -> Result<Vec<ToolDefinition>, BridgeError> {
        let val = self.call("extensions.list_tools", json!({}))?;
        Ok(serde_json::from_value(val)?)
    }

    pub fn execute_extension_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<Value, BridgeError> {
        self.call(
            "extensions.execute_tool",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    pub fn list_skills(&mut self, roots: &[&str]) -> Result<Vec<SkillInfo>, BridgeError> {
        let val = self.call("extensions.list_skills", json!({ "roots": roots }))?;
        Ok(serde_json::from_value(val)?)
    }

    pub fn refresh_models(
        &mut self,
        provider: Option<&str>,
    ) -> Result<Vec<ModelSpec>, BridgeError> {
        self.refresh_models_if(provider, true)
    }

    /// `force = false` leaves a catalog younger than the refresh interval untouched.
    pub fn refresh_models_if(
        &mut self,
        provider: Option<&str>,
        force: bool,
    ) -> Result<Vec<ModelSpec>, BridgeError> {
        let val = self.call(
            "providers.refresh_models",
            json!({ "provider": provider, "force": force }),
        )?;
        let models_val = val.get("models").cloned().unwrap_or_else(|| json!([]));
        Ok(serde_json::from_value(models_val)?)
    }

    /// Seconds since the last catalog pull, and whether it is past the refresh interval.
    pub fn catalog_state(&mut self) -> Result<(Option<f64>, bool), BridgeError> {
        let val = self.call("providers.catalog_state", json!({}))?;
        let age = val.get("age_seconds").and_then(Value::as_f64);
        let stale = val.get("stale").and_then(Value::as_bool).unwrap_or(true);
        Ok((age, stale))
    }

    pub fn fetch_usage(&mut self, provider: Option<&str>) -> Result<Value, BridgeError> {
        self.call("providers.usage", json!({ "provider": provider }))
    }

    pub fn default_model(&mut self, provider: &str) -> Result<String, BridgeError> {
        let val = self.call("providers.default_model", json!({ "provider": provider }))?;
        Ok(val
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }
}

impl Drop for PythonBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ProviderPort for PythonBridge {
    fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
        let val = self.call("providers.list", json!({}))?;
        Ok(serde_json::from_value(val)?)
    }

    fn list_models(&mut self, provider: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
        let val = self.call("providers.list_models", json!({ "provider": provider }))?;
        Ok(serde_json::from_value(val)?)
    }

    fn complete(&mut self, request: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
        let val = self.call("providers.complete", serde_json::to_value(request)?)?;
        Ok(serde_json::from_value(val)?)
    }

    fn web_search(&mut self, queries: &[String], num_results: usize) -> Result<String, BridgeError> {
        let val = self.call(
            "web.search",
            json!({ "queries": queries, "num_results": num_results }),
        )?;
        Ok(val
            .get("output")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    fn fetch_content(&mut self, url: &str, mode: &str) -> Result<String, BridgeError> {
        let val = self.call("web.fetch", json!({ "url": url, "mode": mode }))?;
        Ok(val
            .get("output")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    fn stream_complete(
        &mut self,
        request: CompletionRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<AssistantMessage, BridgeError> {
        let val = self.call_streaming("providers.stream", serde_json::to_value(request)?, |ev_val| {
            if let Ok(ev) = serde_json::from_value::<StreamEvent>(ev_val) {
                on_event(ev);
            }
        })?;
        Ok(serde_json::from_value(val)?)
    }
}

impl PolicyPort for PythonBridge {
    fn evaluate_tool_call(
        &mut self,
        tool_name: &str,
        command: Option<&str>,
        target_path: Option<&str>,
    ) -> Result<PolicyDecision, BridgeError> {
        let val = self.call(
            "policy.evaluate",
            json!({
                "tool_name": tool_name,
                "command": command,
                "target_path": target_path,
            }),
        )?;
        Ok(serde_json::from_value(val)?)
    }

    fn set_yolo(&mut self, enabled: bool) -> Result<(), BridgeError> {
        self.yolo_enabled = enabled;
        let _ = self.call("policy.set_yolo", json!({ "enabled": enabled }))?;
        Ok(())
    }

    fn grant_rule(
        &mut self,
        tool_name: &str,
        command: Option<&str>,
        target_path: Option<&str>,
    ) -> Result<(), BridgeError> {
        self.granted_rules.push((
            tool_name.to_string(),
            command.map(ToString::to_string),
            target_path.map(ToString::to_string),
        ));
        let _ = self.call(
            "policy.grant_rule",
            json!({
                "tool_name": tool_name,
                "command": command,
                "target_path": target_path,
            }),
        )?;
        Ok(())
    }
}

impl ExtensionPort for PythonBridge {
    fn invoke_hook(&mut self, event: &str, payload: Value) -> Result<Value, BridgeError> {
        self.call(
            "extensions.invoke_hook",
            json!({ "event": event, "payload": payload }),
        )
    }

    fn list_extension_tools(&mut self) -> Result<Vec<ToolDefinition>, BridgeError> {
        let val = self.call("extensions.list_tools", json!({}))?;
        Ok(serde_json::from_value(val)?)
    }

    fn execute_extension_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<Value, BridgeError> {
        self.call(
            "extensions.execute_tool",
            json!({ "name": name, "arguments": arguments }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn roundtrip_with_real_python_bridge() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let python_dir = manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .join("python");
        let venv_py = python_dir.join(".venv").join("Scripts").join("python.exe");
        let python_exe = if venv_py.exists() {
            venv_py.to_string_lossy().to_string()
        } else {
            "python".to_string()
        };

        let mut bridge = PythonBridge::spawn(&python_exe, &python_dir).expect("spawn bridge");
        let providers = bridge.list_providers().expect("list providers");
        assert_eq!(providers, vec!["vibe", "deepseek", "agy"]);

        let _models = bridge.list_models(None).expect("list models");

        let decision = bridge
            .evaluate_tool_call("bash", Some("rm -rf /"), None)
            .expect("evaluate policy");
        assert_eq!(decision.action, "deny");

        let (age, _stale) = bridge.catalog_state().expect("catalog state");
        let _ = age;
    }

    #[test]
    fn remote_errors_and_policy_yolo_roundtrip_over_bridge() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let python_dir = manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .join("python");
        let venv_py = python_dir.join(".venv").join("Scripts").join("python.exe");
        let python_exe = if venv_py.exists() {
            venv_py.to_string_lossy().to_string()
        } else {
            "python".to_string()
        };

        let mut bridge = PythonBridge::spawn(&python_exe, &python_dir).expect("spawn bridge");
        let err = bridge.call("unknown.method", json!({})).unwrap_err();
        assert!(matches!(err, BridgeError::Remote { .. }));

        bridge.set_yolo(true).expect("enable yolo");
        let d = bridge
            .evaluate_tool_call("write", None, Some("foo.txt"))
            .expect("evaluate under yolo");
        assert_eq!(d.action, "allow");
    }
}
