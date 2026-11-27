use le_vibe_agent::{AgentConfig, AgentEvent, ApprovalChoice, run_turn_loop_cancellable};
use le_vibe_durable::SessionStore;
use le_vibe_mcp::McpManager;
use le_vibe_pybridge::{ProviderPort, PythonBridge};
use le_vibe_telemetry::TelemetryRecorder;
use le_vibe_tools::TodoStore;
use le_vibe_tui::{AgentMode, build_system_prompt};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

use crate::settings::{cwd_string, default_mcp_paths, self_repo_path};

pub(crate) fn run_streaming_prompt(
    bridge: &mut PythonBridge,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    cfg: &AgentConfig,
    prompt: &str,
    json_mode: bool,
) {
    let mut todos = TodoStore::new();
    let mut mcp = McpManager::from_config_files(&default_mcp_paths());
    let res = run_turn_loop_cancellable(
        bridge,
        session,
        telemetry,
        &mut todos,
        Some(&mut mcp),
        None,
        cfg,
        prompt,
        |ev| {
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
        },
        |_| ApprovalChoice::AllowOnce,
        |_| "No interactive UI available.".to_string(),
    );
    if !json_mode {
        println!();
    }
    match res {
        Ok(reply) => {
            if json_mode {
                println!(
                    "{}",
                    json!({
                        "type": "result",
                        "stop_reason": reply.stop_reason,
                        "usage": reply.usage
                    })
                );
            }
        }
        Err(err) => {
            eprintln!("Error: {err}");
            if json_mode {
                std::process::exit(1);
            }
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
    let mut todos = TodoStore::new();
    let mut mcp = McpManager::from_config_files(&default_mcp_paths());
    let output_store = std::sync::Arc::new(std::sync::Mutex::new(le_vibe_agent::OutputStore::new()));
    let mut cached_model_note: Option<(String, Option<String>)> = None;
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
                let mut sys = build_system_prompt(
                    AgentMode::Normal,
                    "powershell",
                    &cwd_string(),
                    &self_repo_path(),
                    &[],
                    "",
                );
                let model_key = format!("{provider}/{model}");
                let note = match &cached_model_note {
                    Some((k, n)) if k == &model_key => n.clone(),
                    _ => {
                        let loaded = le_vibe_tools::memory::MemoryRagStore::open_default()
                            .read_by_key(&format!("model:{model_key}"));
                        cached_model_note = Some((model_key.clone(), loaded.clone()));
                        loaded
                    }
                };
                if let Some(n) = &note {
                    le_vibe_tui::append_model_profile(&mut sys, &model_key, n);
                }
                let cfg = AgentConfig {
                    provider: provider.clone(),
                    model: model.clone(),
                    system_prompt: Some(sys),
                    thinking_level: "medium".to_string(),
                    max_turns: 40,
                    max_retries: le_vibe_agent::DEFAULT_MAX_RETRIES,
                    base_delay_ms: le_vibe_agent::DEFAULT_BASE_DELAY_MS,
                    max_delay_ms: le_vibe_agent::DEFAULT_MAX_DELAY_MS,
                    allowed_tools: None,
                    mode_name: Some("normal".to_string()),
                    codemode: "off".to_string(),
                    steering_queue: None,
                    output_store: Some(std::sync::Arc::clone(&output_store)),
                };
                match run_turn_loop_cancellable(
                    bridge,
                    session,
                    telemetry,
                    &mut todos,
                    Some(&mut mcp),
                    None,
                    &cfg,
                    text,
                    |ev| {
                        println!("{}", json!({"id": id, "event": ev}));
                    },
                    |_| ApprovalChoice::AllowOnce,
                    |_| "No interactive UI available.".to_string(),
                ) {
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

