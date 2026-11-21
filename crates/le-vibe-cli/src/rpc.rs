use le_vibe_agent::{AgentConfig, AgentEvent, run_turn_loop_streaming};
use le_vibe_durable::SessionStore;
use le_vibe_pybridge::{ProviderPort, PythonBridge};
use le_vibe_telemetry::TelemetryRecorder;
use le_vibe_tui::{AgentMode, build_system_prompt};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

use crate::settings::{cwd_string, self_repo_path};

pub(crate) fn run_streaming_prompt(
    bridge: &mut PythonBridge,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    cfg: &AgentConfig,
    prompt: &str,
    json_mode: bool,
) {
    let res = run_turn_loop_streaming(bridge, session, telemetry, cfg, prompt, |ev| {
        if json_mode {
            println!("{}", serde_json::to_string(&ev).unwrap());
            return;
        }
        match ev {
            AgentEvent::TextDelta { delta } => {
                print!("{delta}");
                let _ = io::stdout().flush();
            }
            AgentEvent::ToolCallStart { name, .. } => {
                eprintln!("\n[tool:{name}]");
            }
            _ => {}
        }
    });
    if !json_mode {
        println!();
    }
    if let Err(err) = res {
        eprintln!("Error: {err}");
        if json_mode {
            std::process::exit(1);
        }
    }
}


pub(crate) fn run_rpc_mode(
    bridge: &mut PythonBridge,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    provider: &mut String,
    model: &mut String,
) {
    let stdin = io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(trimmed) else {
            println!("{}", json!({"error": "invalid json"}));
            continue;
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

        let resp = match method {
            "list_models" => match bridge.list_models(None) {
                Ok(models) => json!({"id": id, "result": models}),
                Err(e) => json!({"id": id, "error": e.to_string()}),
            },
            "auth_status" => match bridge.auth_status() {
                Ok(status) => json!({"id": id, "result": status}),
                Err(e) => json!({"id": id, "error": e.to_string()}),
            },
            "session_tree" => json!({"id": id, "result": session.active_branch()}),
            "set_model" => {
                if let Some(p) = params.get("provider").and_then(Value::as_str) {
                    *provider = p.to_string();
                }
                if let Some(m) = params.get("model").and_then(Value::as_str) {
                    *model = m.to_string();
                }
                json!({"id": id, "result": {"provider": provider, "model": model}})
            }
            "prompt" => {
                let text = params.get("text").and_then(Value::as_str).unwrap_or("");
                let sys = build_system_prompt(
                    AgentMode::Normal,
                    "powershell",
                    &cwd_string(),
                    &self_repo_path(),
                    &[],
                    "",
                );
                let cfg = AgentConfig {
                    provider: provider.clone(),
                    model: model.clone(),
                    system_prompt: Some(sys),
                    thinking_level: "medium".to_string(),
                    max_turns: 40,
                    max_retries: 2,
                    allowed_tools: None,
                    mode_name: Some("normal".to_string()),
                    codemode: "off".to_string(),
                    steering_queue: None,
                };
                match run_turn_loop_streaming(bridge, session, telemetry, &cfg, text, |ev| {
                    println!("{}", json!({"id": id, "event": ev}));
                }) {
                    Ok(reply) => json!({"id": id, "result": reply}),
                    Err(e) => json!({"id": id, "error": e.to_string()}),
                }
            }
            _ => json!({"id": id, "error": format!("unknown method: {method}")}),
        };
        println!("{resp}");
        let _ = io::stdout().flush();
    }
}

