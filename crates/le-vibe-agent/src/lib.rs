use le_vibe_durable::{EntryKind, SessionStore};
use le_vibe_mcp::McpManager;
use le_vibe_pybridge::{
    AssistantMessage, BridgeError, ChatMessage, CompletionRequest, ExtensionPort, PolicyPort,
    ProviderPort, ToolCall,
};
use le_vibe_telemetry::TelemetryRecorder;
use le_vibe_tools::codemode::run_codemode_script;
use le_vibe_tools::hooks::HookRegistry;
use le_vibe_tools::{
    TodoStore, ToolError, builtin_tool_definitions, execute_builtin_tool,
    execute_builtin_tool_approved, execute_todo_tool,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("provider bridge error: {0}")]
    Bridge(#[from] BridgeError),
    #[error("session error: {0}")]
    Session(#[from] le_vibe_durable::SessionError),
    #[error("max turns exceeded ({0})")]
    MaxTurnsExceeded(usize),
    #[error("turn cancelled by user")]
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    TurnStart { turn: usize },
    Status { text: String },
    ThinkingDelta { delta: String },
    TextDelta { delta: String },
    ToolCallStart { id: String, name: String },
    ToolCallEnd { id: String, name: String, is_error: bool, output: String },
    SteeringInjected { text: String },
    TurnEnd { stop_reason: String },
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub command: Option<String>,
    pub target_path: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    AllowOnce,
    AllowSession,
    AllowYolo,
    Deny,
}

pub struct AgentConfig {
    pub provider: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub thinking_level: String,
    pub max_turns: usize,
    pub max_retries: usize,
    pub allowed_tools: Option<Vec<String>>,
    pub mode_name: Option<String>,
    pub codemode: String,
    pub steering_queue: Option<Arc<Mutex<VecDeque<String>>>>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            provider: "deepseek".to_string(),
            model: "deepseek-v4-pro".to_string(),
            system_prompt: None,
            thinking_level: "medium".to_string(),
            max_turns: 40,
            max_retries: 2,
            allowed_tools: None,
            mode_name: None,
            codemode: "off".to_string(),
            steering_queue: None,
        }
    }
}

const MODE_REMINDER_PREFIX: &str = "<system-reminder>Operational mode changed";

pub fn mode_transition_reminder(prev: Option<&str>, current: &str) -> Option<String> {
    if prev == Some(current) {
        return None;
    }
    let msg = match current {
        "plan" | "ask" => format!(
            "{MODE_REMINDER_PREFIX} to {}. You are in read-only mode. Only read, grep, find, and ls are permitted. Do not attempt file edits or shell commands.</system-reminder>",
            current.to_uppercase()
        ),
        "rea" => format!(
            "{MODE_REMINDER_PREFIX} to REA (reverse-engineering). Read, search, and shell inspection tools are enabled; file write/edit tools are disabled.</system-reminder>"
        ),
        "cyber" | "normal" if matches!(prev, Some("plan" | "ask" | "rea")) => format!(
            "{MODE_REMINDER_PREFIX} from {} to {}. You are no longer in read-only mode. Full file edit, shell, and verification tools are now permitted.</system-reminder>",
            prev.unwrap_or("plan").to_uppercase(),
            current.to_uppercase()
        ),
        _ => return None,
    };
    Some(msg)
}

pub fn sanitize_messages_for_provider(provider: &str, messages: &mut Vec<ChatMessage>) {
    // Drop stale mode-change reminders from historical replay
    messages.retain(|m| !m.content.starts_with(MODE_REMINDER_PREFIX));

    let mut normalized: Vec<ChatMessage> = Vec::with_capacity(messages.len());
    let mut i = 0;
    while i < messages.len() {
        let mut msg = messages[i].clone();
        if provider != "agy" {
            for tc in &mut msg.tool_calls {
                tc.thought_signature = None;
            }
        }
        if provider != "deepseek" {
            msg.reasoning_content = None;
        }
        if msg.role == "assistant" && !msg.tool_calls.is_empty() {
            let expected_ids: Vec<(String, String)> = msg
                .tool_calls
                .iter()
                .map(|tc| (tc.id.clone(), tc.name.clone()))
                .collect();
            normalized.push(msg);
            let mut j = i + 1;
            let mut seen_ids: Vec<String> = Vec::new();
            let mut deferred: Vec<ChatMessage> = Vec::new();
            // Scan past non-tool messages (interrupt/steering user turns recorded
            // between a tool call and its results) so tool results stay glued to
            // the assistant message that issued them. Strict providers (Mistral,
            // vibe) 400 with "Unexpected role 'tool'" if a user message separates
            // the assistant tool_calls from their results.
            while j < messages.len() {
                if messages[j].role == "tool" {
                    if let Some(id) = &messages[j].tool_call_id {
                        seen_ids.push(id.clone());
                    }
                    normalized.push(messages[j].clone());
                } else if matches!(messages[j].role.as_str(), "user" | "assistant")
                    && messages[j].tool_calls.is_empty()
                {
                    deferred.push(messages[j].clone());
                } else {
                    break;
                }
                j += 1;
            }
            for (id, name) in expected_ids {
                if !seen_ids.iter().any(|s| s == &id) {
                    normalized.push(ChatMessage {
                        role: "tool".to_string(),
                        content: "[interrupted]".to_string(),
                        reasoning_content: None,
                        tool_calls: Vec::new(),
                        tool_call_id: Some(id),
                        name: Some(name),
                    });
                }
            }
            // Re-emit deferred user/assistant messages only once every pending
            // tool result (real or [interrupted] stub) precedes them.
            normalized.extend(deferred);
            i = j;
            continue;
        }
        normalized.push(msg);
        i += 1;
    }
    // Merge consecutive plain user messages so strict providers (vibe/claude/deepseek) never 400
    let mut merged: Vec<ChatMessage> = Vec::with_capacity(normalized.len());
    for msg in normalized {
        if msg.role == "user"
            && msg.tool_calls.is_empty()
            && let Some(prev) = merged.last_mut()
            && prev.role == "user"
            && prev.tool_calls.is_empty()
        {
            if !prev.content.is_empty() && !msg.content.is_empty() {
                prev.content.push_str("\n\n");
            }
            prev.content.push_str(&msg.content);
            continue;
        }
        merged.push(msg);
    }
    *messages = merged;
}

fn is_transient_error(err: &BridgeError) -> bool {
    let text = err.to_string();
    text.contains("HTTP 429")
        || text.contains("HTTP 502")
        || text.contains("HTTP 503")
        || text.contains("HTTP 504")
        || text.contains("ReadTimeout")
        || text.contains("ConnectError")
}

const TOOL_RESULT_MAX_BYTES: usize = 24 * 1024;

pub fn cap_tool_output(text: &str) -> String {
    if text.len() <= TOOL_RESULT_MAX_BYTES {
        return text.to_string();
    }
    let mut head_end = 16 * 1024;
    while head_end > 0 && !text.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let mut tail_start = text.len().saturating_sub(6 * 1024);
    while tail_start < text.len() && !text.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let omitted = tail_start.saturating_sub(head_end);
    format!(
        "{}\n...[truncated {omitted} bytes]...\n{}",
        &text[..head_end],
        &text[tail_start..]
    )
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|c| c.load(Ordering::SeqCst))
}

pub fn run_turn_loop<B: ProviderPort + PolicyPort + ExtensionPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    config: &AgentConfig,
    user_prompt: &str,
) -> Result<AssistantMessage, AgentError> {
    run_turn_loop_streaming(bridge, session, telemetry, config, user_prompt, |_| {})
}

pub fn run_turn_loop_streaming<B: ProviderPort + PolicyPort + ExtensionPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    config: &AgentConfig,
    user_prompt: &str,
    on_event: impl FnMut(AgentEvent),
) -> Result<AssistantMessage, AgentError> {
    let mut todos = TodoStore::new();
    run_turn_loop_interactive(
        bridge,
        session,
        telemetry,
        &mut todos,
        config,
        user_prompt,
        on_event,
        |_| ApprovalChoice::AllowOnce,
        |_| "No interactive UI available.".to_string(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_turn_loop_interactive<B: ProviderPort + PolicyPort + ExtensionPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    todos: &mut TodoStore,
    config: &AgentConfig,
    user_prompt: &str,
    on_event: impl FnMut(AgentEvent),
    on_approval: impl FnMut(ApprovalRequest) -> ApprovalChoice,
    on_ask_user: impl FnMut(Value) -> String,
) -> Result<AssistantMessage, AgentError> {
    run_turn_loop_with_mcp(
        bridge,
        session,
        telemetry,
        todos,
        None,
        config,
        user_prompt,
        on_event,
        on_approval,
        on_ask_user,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_turn_loop_with_mcp<B: ProviderPort + PolicyPort + ExtensionPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    todos: &mut TodoStore,
    mcp: Option<&mut McpManager>,
    config: &AgentConfig,
    user_prompt: &str,
    on_event: impl FnMut(AgentEvent),
    on_approval: impl FnMut(ApprovalRequest) -> ApprovalChoice,
    on_ask_user: impl FnMut(Value) -> String,
) -> Result<AssistantMessage, AgentError> {
    run_turn_loop_cancellable(
        bridge,
        session,
        telemetry,
        todos,
        mcp,
        None,
        config,
        user_prompt,
        on_event,
        on_approval,
        on_ask_user,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_turn_loop_cancellable<B: ProviderPort + PolicyPort + ExtensionPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    todos: &mut TodoStore,
    mcp: Option<&mut McpManager>,
    cancel: Option<&AtomicBool>,
    config: &AgentConfig,
    user_prompt: &str,
    mut on_event: impl FnMut(AgentEvent),
    mut on_approval: impl FnMut(ApprovalRequest) -> ApprovalChoice,
    mut on_ask_user: impl FnMut(Value) -> String,
) -> Result<AssistantMessage, AgentError> {
    run_turn_loop_inner(
        bridge,
        session,
        telemetry,
        todos,
        mcp,
        cancel,
        config,
        user_prompt,
        &mut on_event,
        &mut on_approval,
        &mut on_ask_user,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_turn_loop_inner<B: ProviderPort + PolicyPort + ExtensionPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    todos: &mut TodoStore,
    mut mcp: Option<&mut McpManager>,
    cancel: Option<&AtomicBool>,
    config: &AgentConfig,
    user_prompt: &str,
    on_event: &mut dyn FnMut(AgentEvent),
    on_approval: &mut dyn FnMut(ApprovalRequest) -> ApprovalChoice,
    on_ask_user: &mut dyn FnMut(Value) -> String,
) -> Result<AssistantMessage, AgentError> {
    // Determine prior active mode from session before appending user turn
    let prev_mode = session
        .active_branch()
        .into_iter()
        .rev()
        .find(|e| e.kind == EntryKind::ModeChange)
        .map(|e| e.content);
    let mode_reminder = config
        .mode_name
        .as_deref()
        .and_then(|cur| mode_transition_reminder(prev_mode.as_deref(), cur));

    session.append("user", user_prompt)?;
    let mut messages: Vec<ChatMessage> = Vec::new();
    for entry in session.active_branch() {
        match entry.kind {
            EntryKind::Message => {
                messages.push(ChatMessage {
                    role: entry.role,
                    content: entry.content,
                    reasoning_content: None,
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                    name: None,
                });
            }
            EntryKind::CompactionSummary | EntryKind::BranchSummary => {
                messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: format!(
                        "<session_summary>\n{}\n</session_summary>",
                        entry.content
                    ),
                    reasoning_content: None,
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                    name: None,
                });
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: "Understood. I have the prior session summary and am ready to continue."
                        .to_string(),
                    reasoning_content: None,
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                    name: None,
                });
            }
            EntryKind::ToolCall => {
                let args: Value = serde_json::from_str(&entry.content).unwrap_or_else(|_| Value::Object(Default::default()));
                let tc = ToolCall {
                    id: entry.tool_call_id.unwrap_or_default(),
                    name: entry.tool_name.unwrap_or_default(),
                    arguments: args,
                    thought_signature: None,
                };
                if let Some(last) = messages.last_mut()
                    && last.role == "assistant"
                    && !last.tool_calls.is_empty()
                {
                    last.tool_calls.push(tc);
                } else {
                    messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: String::new(),
                        reasoning_content: None,
                        tool_calls: vec![tc],
                        tool_call_id: None,
                        name: None,
                    });
                }
            }
            EntryKind::ToolResult => {
                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: entry.content,
                    reasoning_content: None,
                    tool_calls: Vec::new(),
                    tool_call_id: entry.tool_call_id,
                    name: entry.tool_name,
                });
            }
            EntryKind::ModelChange | EntryKind::ModeChange | EntryKind::SessionMeta => {}
        }
    }
    sanitize_messages_for_provider(&config.provider, &mut messages);
    if let Some(reminder) = mode_reminder {
        let insert_idx = messages.len().saturating_sub(1);
        messages.insert(
            insert_idx,
            ChatMessage {
                role: "user".to_string(),
                content: reminder,
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
                name: None,
            },
        );
    }

    let ext_tools = bridge.list_extension_tools().unwrap_or_default();
    let ext_tool_names: Vec<String> = ext_tools.iter().map(|t| t.name.clone()).collect();
    let codemode_setting = config.codemode.trim().to_lowercase();
    let mut tools: Vec<_> = builtin_tool_definitions()
        .into_iter()
        .filter(|t| {
            if t.name == "codemode" {
                return codemode_setting == "on" || codemode_setting == "only";
            }
            if codemode_setting == "only"
                && !matches!(t.name.as_str(), "todo" | "ask_user_question")
            {
                return false;
            }
            config
                .allowed_tools
                .as_ref()
                .is_none_or(|allow| allow.iter().any(|a| a == &t.name))
        })
        .collect();
    if config.allowed_tools.is_none() {
        for ext in ext_tools {
            if !tools.iter().any(|t| t.name == ext.name) {
                tools.push(ext);
            }
        }
        if let Some(mgr) = mcp.as_deref() {
            for m_tool in mgr.tool_definitions() {
                if !tools.iter().any(|t| t.name == m_tool.name) {
                    tools.push(m_tool);
                }
            }
        }
    }

    let cwd_for_hooks = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let hooks = HookRegistry::load_default(&cwd_for_hooks, true);
    let mut last_partial_text = String::new();
    for turn in 0..config.max_turns {
        if is_cancelled(cancel) {
            let note = if last_partial_text.trim().is_empty() {
                "[Interrupted by user]".to_string()
            } else {
                format!("{last_partial_text}\n\n[Interrupted by user]")
            };
            let _ = session.append("assistant", &note);
            return Err(AgentError::Cancelled);
        }
        on_event(AgentEvent::TurnStart { turn });
        let span = telemetry.record_span("agent.turn");
        sanitize_messages_for_provider(&config.provider, &mut messages);

        let mut attempt = 0usize;
        let reply = loop {
            if is_cancelled(cancel) {
                telemetry.finish_span(span);
                let note = if last_partial_text.trim().is_empty() {
                    "[Interrupted by user]".to_string()
                } else {
                    format!("{last_partial_text}\n\n[Interrupted by user]")
                };
                let _ = session.append("assistant", &note);
                return Err(AgentError::Cancelled);
            }
            match bridge.stream_complete(
                CompletionRequest {
                    provider: config.provider.clone(),
                    model: config.model.clone(),
                    messages: messages.clone(),
                    system_prompt: config.system_prompt.clone(),
                    tools: tools.clone(),
                    thinking_level: config.thinking_level.clone(),
                },
                &mut |ev| match ev.kind.as_str() {
                    "thinking_delta" => on_event(AgentEvent::ThinkingDelta { delta: ev.delta }),
                    "text_delta" => {
                        last_partial_text.push_str(&ev.delta);
                        on_event(AgentEvent::TextDelta { delta: ev.delta });
                    }
                    _ => {}
                },
            ) {
                Ok(r) => break r,
                Err(BridgeError::Cancelled) => {
                    telemetry.finish_span(span);
                    let note = if last_partial_text.trim().is_empty() {
                        "[Interrupted by user]".to_string()
                    } else {
                        format!("{last_partial_text}\n\n[Interrupted by user]")
                    };
                    let _ = session.append("assistant", &note);
                    return Err(AgentError::Cancelled);
                }
                Err(err)
                    if attempt < config.max_retries
                        && is_transient_error(&err)
                        && !is_cancelled(cancel) =>
                {
                    attempt += 1;
                    let delay_secs = match attempt {
                        1 => 1,
                        2 => 3,
                        _ => 6,
                    };
                    let short_err: String = err.to_string().chars().take(60).collect();
                    on_event(AgentEvent::Status {
                        text: format!(
                            "Transient error ({short_err}) — retrying ({attempt}/{}) in {delay_secs}s...",
                            config.max_retries
                        ),
                    });
                    for _ in 0..(delay_secs * 20) {
                        if is_cancelled(cancel) {
                            telemetry.finish_span(span);
                            let note = if last_partial_text.trim().is_empty() {
                                "[Interrupted by user]".to_string()
                            } else {
                                format!("{last_partial_text}\n\n[Interrupted by user]")
                            };
                            let _ = session.append("assistant", &note);
                            return Err(AgentError::Cancelled);
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
                Err(err) => {
                    telemetry.finish_span(span);
                    let note = if last_partial_text.trim().is_empty() {
                        format!("[Turn interrupted: {err}]")
                    } else {
                        format!("{last_partial_text}\n\n[Turn interrupted: {err}]")
                    };
                    let _ = session.append("assistant", &note);
                    return Err(AgentError::Bridge(err));
                }
            }
        };
        telemetry.finish_span(span);

        if !reply.content.trim().is_empty() {
            last_partial_text = reply.content.clone();
        }

        if reply.tool_calls.is_empty() {
            session.append("assistant", &reply.content)?;
            on_event(AgentEvent::TurnEnd {
                stop_reason: reply.stop_reason.clone(),
            });
            hooks.run_stop();
            return Ok(reply);
        }

        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: reply.content.clone(),
            reasoning_content: reply.reasoning_content.clone(),
            tool_calls: reply.tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        // Fast path: execute pure read-only batches in parallel via std::thread::scope
        // (preserves exact tool_call_id ordering, policy checks, and PreToolUse/PostToolUse hooks).
        let is_parallel_readonly_batch = reply.tool_calls.len() >= 2
            && reply.tool_calls.iter().all(|tc| {
                matches!(
                    tc.name.as_str(),
                    "read" | "grep" | "find" | "ls" | "load_skill"
                )
            });

        if is_parallel_readonly_batch {
            let mut prepared: Vec<(&ToolCall, String, Option<String>)> =
                Vec::with_capacity(reply.tool_calls.len());
            for tc in &reply.tool_calls {
                if is_cancelled(cancel) {
                    let note = if last_partial_text.trim().is_empty() {
                        "[Interrupted by user]".to_string()
                    } else {
                        format!("{last_partial_text}\n\n[Interrupted by user]")
                    };
                    let _ = session.append("assistant", &note);
                    return Err(AgentError::Cancelled);
                }
                on_event(AgentEvent::ToolCallStart {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                });
                let args_json_str = tc.arguments.to_string();
                let hook_res = bridge
                    .invoke_hook(
                        "before_tool_call",
                        serde_json::json!({ "tool": tc.name, "arguments": tc.arguments }),
                    )
                    .unwrap_or(Value::Null);
                let mut blocked = if hook_res.get("blocked").and_then(Value::as_bool) == Some(true)
                {
                    Some(
                        hook_res
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("Blocked by extension hook.")
                            .to_string(),
                    )
                } else {
                    hooks.run_pre_tool(&tc.name, &args_json_str).err()
                };
                if blocked.is_none() {
                    let target_path = tc.arguments.get("path").and_then(Value::as_str);
                    match bridge.evaluate_tool_call(&tc.name, None, target_path) {
                        Ok(dec) if dec.action == "deny" => {
                            blocked = Some(format!("Policy denied: {}", dec.reason));
                        }
                        Ok(dec) if dec.action == "ask" => {
                            let choice = on_approval(ApprovalRequest {
                                tool_name: tc.name.clone(),
                                command: None,
                                target_path: target_path.map(ToString::to_string),
                                reason: dec.reason,
                            });
                            match choice {
                                ApprovalChoice::Deny => {
                                    blocked = Some("Tool call denied by user.".to_string());
                                }
                                ApprovalChoice::AllowSession => {
                                    let _ = bridge.grant_rule(&tc.name, None, target_path);
                                }
                                ApprovalChoice::AllowYolo => {
                                    let _ = bridge.set_yolo(true);
                                }
                                ApprovalChoice::AllowOnce => {}
                            }
                        }
                        Err(err) => {
                            blocked = Some(format!("Policy error: {err}"));
                        }
                        Ok(_) => {}
                    }
                }
                prepared.push((tc, args_json_str, blocked));
            }

            let results: Vec<(String, bool)> = std::thread::scope(|s| {
                let handles: Vec<_> = prepared
                    .iter()
                    .map(|(tc, _args_str, blocked)| {
                        s.spawn(move || {
                            if let Some(reason) = blocked {
                                return (reason.clone(), true);
                            }
                            match execute_builtin_tool_approved(&tc.name, &tc.arguments) {
                                Ok(out) => (out, false),
                                Err(err) => (format!("Tool error: {err}"), true),
                            }
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .unwrap_or_else(|_| ("Tool error: worker panicked".to_string(), true))
                    })
                    .collect()
            });

            for ((tc, args_json_str, _), (mut output, is_error)) in prepared.into_iter().zip(results)
            {
                if !is_error
                    && let Some(post_note) = hooks.run_post_tool(&tc.name, &args_json_str)
                {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[PostToolUse hook]\n");
                    output.push_str(&post_note);
                }
                let capped_output = cap_tool_output(&output);
                let _ = session.append_tool_result(&tc.id, &tc.name, &capped_output);
                on_event(AgentEvent::ToolCallEnd {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    is_error,
                    output,
                });
                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: capped_output,
                    reasoning_content: None,
                    tool_calls: Vec::new(),
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.name.clone()),
                });
            }
        } else {
        for tc in &reply.tool_calls {
            if is_cancelled(cancel) {
                let note = if last_partial_text.trim().is_empty() {
                    "[Interrupted by user]".to_string()
                } else {
                    format!("{last_partial_text}\n\n[Interrupted by user]")
                };
                let _ = session.append("assistant", &note);
                return Err(AgentError::Cancelled);
            }
            on_event(AgentEvent::ToolCallStart {
                id: tc.id.clone(),
                name: tc.name.clone(),
            });
            let args_json_str = tc.arguments.to_string();
            let hook_res = bridge
                .invoke_hook(
                    "before_tool_call",
                    serde_json::json!({ "tool": tc.name, "arguments": tc.arguments }),
                )
                .unwrap_or(Value::Null);
            let blocked_reason = if hook_res.get("blocked").and_then(Value::as_bool) == Some(true) {
                Some(
                    hook_res
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("Blocked by extension hook.")
                        .to_string(),
                )
            } else {
                hooks.run_pre_tool(&tc.name, &args_json_str).err()
            };
            let (mut output, is_error) = if let Some(reason) = blocked_reason {
                (reason, true)
            } else if tc.name == "codemode" {
                let script = tc
                    .arguments
                    .get("script")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let timeout_ms = tc
                    .arguments
                    .get("timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(60_000);
                if script.trim().is_empty() {
                    ("Tool error: codemode requires non-empty `script`".to_string(), true)
                } else {
                    on_event(AgentEvent::Status {
                        text: "Running codemode script...".to_string(),
                    });
                    match run_codemode_script(script, timeout_ms, |sub_tool, sub_args| {
                        let sub_args_str = sub_args.to_string();
                        hooks.run_pre_tool(sub_tool, &sub_args_str)?;
                        execute_builtin_tool(bridge, sub_tool, &sub_args)
                            .map_err(|e| e.to_string())
                    }) {
                        Ok(out) => (out, false),
                        Err(err) => (err, true),
                    }
                }
            } else if tc.name == "spawn_agent" {
                let task = tc
                    .arguments
                    .get("task")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if task.is_empty() {
                    ("Tool error: spawn_agent requires non-empty `task`".to_string(), true)
                } else {
                    on_event(AgentEvent::Status {
                        text: format!("Subagent researching: {}...", task.chars().take(48).collect::<String>()),
                    });
                    let mut child_session = SessionStore::in_memory();
                    let mut child_todos = TodoStore::new();
                    let child_cfg = AgentConfig {
                        provider: config.provider.clone(),
                        model: config.model.clone(),
                        system_prompt: Some(
                            "You are an isolated read-only research subagent. Use read, grep, find, and ls to investigate the task thoroughly, then return a concise, concrete answer with exact file paths and line numbers.".to_string(),
                        ),
                        thinking_level: config.thinking_level.clone(),
                        max_turns: 12,
                        max_retries: config.max_retries,
                        allowed_tools: Some(vec![
                            "read".to_string(),
                            "grep".to_string(),
                            "find".to_string(),
                            "ls".to_string(),
                        ]),
                        mode_name: Some("ask".to_string()),
                        codemode: "off".to_string(),
                        steering_queue: None,
                    };
                    let mut noop_ev = |_: AgentEvent| {};
                    let mut deny_all = |_: ApprovalRequest| ApprovalChoice::Deny;
                    let mut empty_q = |_: Value| String::new();
                    match run_turn_loop_inner(
                        bridge,
                        &mut child_session,
                        telemetry,
                        &mut child_todos,
                        None,
                        cancel,
                        &child_cfg,
                        task,
                        &mut noop_ev,
                        &mut deny_all,
                        &mut empty_q,
                    ) {
                        Ok(child_reply) => (child_reply.content, false),
                        Err(err) => (format!("Subagent error: {err}"), true),
                    }
                }
            } else if tc.name == "todo" {
                match execute_todo_tool(todos, &tc.arguments) {
                    Ok(out) => (out, false),
                    Err(err) => (format!("Tool error: {err}"), true),
                }
            } else if tc.name == "ask_user_question" {
                (on_ask_user(tc.arguments.clone()), false)
            } else if ext_tool_names.iter().any(|n| n == &tc.name) {
                let cmd = tc
                    .arguments
                    .get("command")
                    .and_then(Value::as_str);
                let path = tc
                    .arguments
                    .get("path")
                    .and_then(Value::as_str);
                match bridge.evaluate_tool_call(&tc.name, cmd, path) {
                    Ok(dec) if dec.action == "deny" => (format!("Policy denied: {}", dec.reason), true),
                    Ok(dec) if dec.action == "ask" => {
                        let choice = on_approval(ApprovalRequest {
                            tool_name: tc.name.clone(),
                            command: cmd.map(ToString::to_string),
                            target_path: path.map(ToString::to_string),
                            reason: dec.reason,
                        });
                        match choice {
                            ApprovalChoice::Deny => ("Tool call denied by user.".to_string(), true),
                            ApprovalChoice::AllowSession => {
                                let _ = bridge.grant_rule(&tc.name, cmd, path);
                                match bridge.execute_extension_tool(&tc.name, tc.arguments.clone()) {
                                    Ok(val) => (serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string()), false),
                                    Err(err) => (format!("Extension tool error: {err}"), true),
                                }
                            }
                            ApprovalChoice::AllowYolo => {
                                let _ = bridge.set_yolo(true);
                                match bridge.execute_extension_tool(&tc.name, tc.arguments.clone()) {
                                    Ok(val) => (serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string()), false),
                                    Err(err) => (format!("Extension tool error: {err}"), true),
                                }
                            }
                            ApprovalChoice::AllowOnce => {
                                match bridge.execute_extension_tool(&tc.name, tc.arguments.clone()) {
                                    Ok(val) => (serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string()), false),
                                    Err(err) => (format!("Extension tool error: {err}"), true),
                                }
                            }
                        }
                    }
                    Ok(_) => match bridge.execute_extension_tool(&tc.name, tc.arguments.clone()) {
                        Ok(val) => (serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string()), false),
                        Err(err) => (format!("Extension tool error: {err}"), true),
                    },
                    Err(err) => (format!("Policy error: {err}"), true),
                }
            } else if tc.name.starts_with("mcp__") {
                match bridge.evaluate_tool_call(&tc.name, None, None) {
                    Ok(dec) if dec.action == "deny" => (format!("Policy denied: {}", dec.reason), true),
                    Ok(dec) if dec.action == "ask" => {
                        let choice = on_approval(ApprovalRequest {
                            tool_name: tc.name.clone(),
                            command: None,
                            target_path: None,
                            reason: dec.reason,
                        });
                        match choice {
                            ApprovalChoice::Deny => ("Tool call denied by user.".to_string(), true),
                            ApprovalChoice::AllowSession => {
                                let _ = bridge.grant_rule(&tc.name, None, None);
                                match mcp.as_deref_mut() {
                                    Some(mgr) => match mgr.call_namespaced_tool(&tc.name, tc.arguments.clone()) {
                                        Ok(out) => (out, false),
                                        Err(err) => (format!("MCP tool error: {err}"), true),
                                    },
                                    None => ("MCP manager unavailable.".to_string(), true),
                                }
                            }
                            ApprovalChoice::AllowYolo => {
                                let _ = bridge.set_yolo(true);
                                match mcp.as_deref_mut() {
                                    Some(mgr) => match mgr.call_namespaced_tool(&tc.name, tc.arguments.clone()) {
                                        Ok(out) => (out, false),
                                        Err(err) => (format!("MCP tool error: {err}"), true),
                                    },
                                    None => ("MCP manager unavailable.".to_string(), true),
                                }
                            }
                            ApprovalChoice::AllowOnce => match mcp.as_deref_mut() {
                                Some(mgr) => match mgr.call_namespaced_tool(&tc.name, tc.arguments.clone()) {
                                    Ok(out) => (out, false),
                                    Err(err) => (format!("MCP tool error: {err}"), true),
                                },
                                None => ("MCP manager unavailable.".to_string(), true),
                            },
                        }
                    }
                    Ok(_) => match mcp.as_deref_mut() {
                        Some(mgr) => match mgr.call_namespaced_tool(&tc.name, tc.arguments.clone()) {
                            Ok(out) => (out, false),
                            Err(err) => (format!("MCP tool error: {err}"), true),
                        },
                        None => ("MCP manager unavailable.".to_string(), true),
                    },
                    Err(err) => (format!("Policy error: {err}"), true),
                }
            } else {
                match execute_builtin_tool(bridge, &tc.name, &tc.arguments) {
                    Ok(out) => (out, false),
                    Err(ToolError::PolicyAsk(reason)) => {
                        let cmd = tc
                            .arguments
                            .get("command")
                            .and_then(Value::as_str)
                            .map(ToString::to_string);
                        let path = tc
                            .arguments
                            .get("path")
                            .and_then(Value::as_str)
                            .map(ToString::to_string);
                        let choice = on_approval(ApprovalRequest {
                            tool_name: tc.name.clone(),
                            command: cmd.clone(),
                            target_path: path.clone(),
                            reason,
                        });
                        match choice {
                            ApprovalChoice::AllowOnce => {
                                match execute_builtin_tool_approved(&tc.name, &tc.arguments) {
                                    Ok(out) => (out, false),
                                    Err(err) => (format!("Tool error: {err}"), true),
                                }
                            }
                            ApprovalChoice::AllowSession => {
                                let _ = bridge.grant_rule(
                                    &tc.name,
                                    cmd.as_deref(),
                                    path.as_deref(),
                                );
                                match execute_builtin_tool_approved(&tc.name, &tc.arguments) {
                                    Ok(out) => (out, false),
                                    Err(err) => (format!("Tool error: {err}"), true),
                                }
                            }
                            ApprovalChoice::AllowYolo => {
                                let _ = bridge.set_yolo(true);
                                match execute_builtin_tool_approved(&tc.name, &tc.arguments) {
                                    Ok(out) => (out, false),
                                    Err(err) => (format!("Tool error: {err}"), true),
                                }
                            }
                            ApprovalChoice::Deny => {
                                ("Tool call denied by user.".to_string(), true)
                            }
                        }
                    }
                    Err(err) => (format!("Tool error: {err}"), true),
                }
            };
            let preview: String = output.lines().take(8).collect::<Vec<_>>().join("\n");
            let _ = bridge.invoke_hook(
                "after_tool_call",
                serde_json::json!({
                    "tool": tc.name,
                    "is_error": is_error,
                    "output": preview,
                }),
            );
            let _ = session.append_tool_call(
                &tc.id,
                &tc.name,
                serde_json::to_string(&tc.arguments).unwrap_or_else(|_| "{}".to_string()),
            );
            if !is_error
                && let Some(post_note) = hooks.run_post_tool(&tc.name, &args_json_str)
            {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str("[PostToolUse hook]\n");
                output.push_str(&post_note);
            }
            let capped_output = cap_tool_output(&output);
            let _ = session.append_tool_result(&tc.id, &tc.name, &capped_output);
            on_event(AgentEvent::ToolCallEnd {
                id: tc.id.clone(),
                name: tc.name.clone(),
                is_error,
                output: output.clone(),
            });
            messages.push(ChatMessage {
                role: "tool".to_string(),
                content: capped_output,
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: Some(tc.id.clone()),
                name: Some(tc.name.clone()),
            });
        }
        }

        // Drain steering messages ONLY after all tool_call_id results for this turn are recorded
        // (preserves OpenAI/Mistral/AGY message adjacency).
        if let Some(sq) = &config.steering_queue
            && let Ok(mut q) = sq.lock()
        {
            while let Some(steer_text) = q.pop_front() {
                if !steer_text.trim().is_empty() {
                    let _ = session.append("user", &steer_text);
                    messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: steer_text.clone(),
                        reasoning_content: None,
                        tool_calls: Vec::new(),
                        tool_call_id: None,
                        name: None,
                    });
                    on_event(AgentEvent::SteeringInjected { text: steer_text });
                }
            }
        }
    }

    let note = if last_partial_text.trim().is_empty() {
        format!("[Turn stopped: max turns exceeded ({})]", config.max_turns)
    } else {
        format!(
            "{last_partial_text}\n\n[Turn stopped: max turns exceeded ({})]",
            config.max_turns
        )
    };
    let _ = session.append("assistant", &note);
    Err(AgentError::MaxTurnsExceeded(config.max_turns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use le_vibe_pybridge::{BridgeError, ModelSpec, PolicyDecision, ToolCall, Usage};
    use serde_json::json;

    struct MockBridge {
        step: usize,
    }

    impl ProviderPort for MockBridge {
        fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
            Ok(vec!["vibe".into(), "deepseek".into(), "agy".into()])
        }
        fn list_models(&mut self, _provider: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
            Ok(vec![])
        }
        fn complete(&mut self, _request: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
            self.step += 1;
            if self.step == 1 {
                Ok(AssistantMessage {
                    provider: "deepseek".into(),
                    model: "deepseek-v4-pro".into(),
                    content: "".into(),
                    reasoning_content: Some("Need to read".into()),
                    tool_calls: vec![ToolCall {
                        id: "tc_1".into(),
                        name: "read".into(),
                        arguments: json!({"path": "nonexistent.txt"}),
                        thought_signature: None,
                    }],
                    stop_reason: "tool_calls".into(),
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cost_usd: 0.0,
                    },
                })
            } else {
                Ok(AssistantMessage {
                    provider: "deepseek".into(),
                    model: "deepseek-v4-pro".into(),
                    content: "Handled missing file.".into(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    stop_reason: "stop".into(),
                    usage: Usage {
                        input_tokens: 20,
                        output_tokens: 10,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cost_usd: 0.0,
                    },
                })
            }
        }
    }

        impl PolicyPort for MockBridge {
            fn evaluate_tool_call(
                &mut self,
                _tool_name: &str,
                _command: Option<&str>,
                _target_path: Option<&str>,
            ) -> Result<PolicyDecision, BridgeError> {
                Ok(PolicyDecision {
                    action: "allow".into(),
                    reason: "ok".into(),
                })
            }
        }
        impl ExtensionPort for MockBridge {
            fn invoke_hook(&mut self, _event: &str, payload: Value) -> Result<Value, BridgeError> {
                Ok(payload)
            }
        }

    #[test]
    fn executes_tool_call_and_emits_streaming_events() {
        let mut bridge = MockBridge { step: 0 };
        let mut session = SessionStore::in_memory();
        let telemetry = TelemetryRecorder::new();
        let cfg = AgentConfig::default();

        let mut events = Vec::new();
        let msg = run_turn_loop_streaming(&mut bridge, &mut session, &telemetry, &cfg, "check file", |ev| {
            events.push(ev);
        })
        .unwrap();
        assert_eq!(msg.content, "Handled missing file.");
        assert_eq!(session.entries().len(), 4);
        assert_eq!(telemetry.spans().len(), 2);
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolCallStart { name, .. } if name == "read")));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnEnd { .. })));
    }

    #[test]
    fn allowed_tools_filters_mutating_tools_and_deny_blocks_execution() {
        struct AskWriteBridge {
            step: usize,
            seen_tools: Vec<String>,
        }
        impl ProviderPort for AskWriteBridge {
            fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
                Ok(vec!["deepseek".into()])
            }
            fn list_models(&mut self, _p: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
                Ok(vec![])
            }
            fn complete(&mut self, req: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
                self.seen_tools = req.tools.iter().map(|t| t.name.clone()).collect();
                self.step += 1;
                if self.step == 1 {
                    Ok(AssistantMessage {
                        provider: "deepseek".into(),
                        model: "deepseek-v4-pro".into(),
                        content: String::new(),
                        reasoning_content: None,
                        tool_calls: vec![ToolCall {
                            id: "tc_deny".into(),
                            name: "write".into(),
                            arguments: json!({"path": "should_not_exist.txt", "content": "x"}),
                            thought_signature: None,
                        }],
                        stop_reason: "tool_calls".into(),
                        usage: Usage {
                            input_tokens: 1,
                            output_tokens: 1,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            cost_usd: 0.0,
                        },
                    })
                } else {
                    Ok(AssistantMessage {
                        provider: "deepseek".into(),
                        model: "deepseek-v4-pro".into(),
                        content: "Denied cleanly.".into(),
                        reasoning_content: None,
                        tool_calls: vec![],
                        stop_reason: "stop".into(),
                        usage: Usage {
                            input_tokens: 1,
                            output_tokens: 1,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            cost_usd: 0.0,
                        },
                    })
                }
            }
        }
        impl PolicyPort for AskWriteBridge {
            fn evaluate_tool_call(
                &mut self,
                _tool_name: &str,
                _command: Option<&str>,
                _target_path: Option<&str>,
            ) -> Result<PolicyDecision, BridgeError> {
                Ok(PolicyDecision {
                    action: "ask".into(),
                    reason: "requires approval".into(),
                })
            }
        }
        impl ExtensionPort for AskWriteBridge {
            fn invoke_hook(&mut self, _event: &str, payload: Value) -> Result<Value, BridgeError> {
                Ok(payload)
            }
        }

        let mut bridge = AskWriteBridge {
            step: 0,
            seen_tools: Vec::new(),
        };
        let mut session = SessionStore::in_memory();
        let telemetry = TelemetryRecorder::new();
        let mut todos = TodoStore::new();
        let cfg = AgentConfig {
            allowed_tools: Some(vec!["read".into(), "grep".into(), "ls".into()]),
            ..Default::default()
        };

        let mut tool_outputs = Vec::new();
        let msg = run_turn_loop_interactive(
            &mut bridge,
            &mut session,
            &telemetry,
            &mut todos,
            &cfg,
            "try write",
            |ev| {
                if let AgentEvent::ToolCallEnd { output, is_error, .. } = ev {
                    tool_outputs.push((output, is_error));
                }
            },
            |_| ApprovalChoice::Deny,
            |_| String::new(),
        )
        .unwrap();

        assert_eq!(msg.content, "Denied cleanly.");
        assert_eq!(bridge.seen_tools, vec!["read", "grep", "ls"]);
        assert_eq!(tool_outputs.len(), 1);
        assert!(tool_outputs[0].1);
        assert_eq!(tool_outputs[0].0, "Tool call denied by user.");
        assert!(!std::path::Path::new("should_not_exist.txt").exists());
    }

    #[test]
    fn extension_hook_blocks_and_extension_tool_executes() {
        struct ExtBridge {
            step: usize,
            after_called: bool,
        }
        impl ProviderPort for ExtBridge {
            fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
                Ok(vec!["deepseek".into()])
            }
            fn list_models(&mut self, _p: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
                Ok(vec![])
            }
            fn complete(&mut self, _req: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
                self.step += 1;
                if self.step == 1 {
                    Ok(AssistantMessage {
                        provider: "deepseek".into(),
                        model: "deepseek-v4-pro".into(),
                        content: String::new(),
                        reasoning_content: None,
                        tool_calls: vec![
                            ToolCall {
                                id: "tc_block".into(),
                                name: "read".into(),
                                arguments: json!({"path": ".env"}),
                                thought_signature: None,
                            },
                            ToolCall {
                                id: "tc_ext".into(),
                                name: "project_stats".into(),
                                arguments: json!({"path": "."}),
                                thought_signature: None,
                            },
                        ],
                        stop_reason: "tool_calls".into(),
                        usage: Usage {
                            input_tokens: 1,
                            output_tokens: 1,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            cost_usd: 0.0,
                        },
                    })
                } else {
                    Ok(AssistantMessage {
                        provider: "deepseek".into(),
                        model: "deepseek-v4-pro".into(),
                        content: "Handled extensions.".into(),
                        reasoning_content: None,
                        tool_calls: vec![],
                        stop_reason: "stop".into(),
                        usage: Usage {
                            input_tokens: 1,
                            output_tokens: 1,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            cost_usd: 0.0,
                        },
                    })
                }
            }
        }
        impl PolicyPort for ExtBridge {
            fn evaluate_tool_call(
                &mut self,
                _tool_name: &str,
                _command: Option<&str>,
                _target_path: Option<&str>,
            ) -> Result<PolicyDecision, BridgeError> {
                Ok(PolicyDecision {
                    action: "allow".into(),
                    reason: "ok".into(),
                })
            }
        }
        impl ExtensionPort for ExtBridge {
            fn invoke_hook(&mut self, event: &str, payload: Value) -> Result<Value, BridgeError> {
                if event == "before_tool_call"
                    && payload.get("tool").and_then(Value::as_str) == Some("read")
                {
                    return Ok(json!({ "blocked": true, "reason": "Blocked .env" }));
                }
                if event == "after_tool_call" {
                    self.after_called = true;
                }
                Ok(payload)
            }
            fn list_extension_tools(&mut self) -> Result<Vec<le_vibe_pybridge::ToolDefinition>, BridgeError> {
                Ok(vec![le_vibe_pybridge::ToolDefinition {
                    name: "project_stats".into(),
                    description: "stats".into(),
                    parameters: json!({"type": "object"}),
                }])
            }
            fn execute_extension_tool(&mut self, name: &str, _args: Value) -> Result<Value, BridgeError> {
                assert_eq!(name, "project_stats");
                Ok(json!({"total_files": 42}))
            }
        }

        let mut bridge = ExtBridge {
            step: 0,
            after_called: false,
        };
        let mut session = SessionStore::in_memory();
        let telemetry = TelemetryRecorder::new();
        let mut todos = TodoStore::new();
        let cfg = AgentConfig::default();

        let mut results = Vec::new();
        let msg = run_turn_loop_interactive(
            &mut bridge,
            &mut session,
            &telemetry,
            &mut todos,
            &cfg,
            "run",
            |ev| {
                if let AgentEvent::ToolCallEnd { name, is_error, output, .. } = ev {
                    results.push((name, is_error, output));
                }
            },
            |_| ApprovalChoice::AllowOnce,
            |_| String::new(),
        )
        .unwrap();

        assert_eq!(msg.content, "Handled extensions.");
        assert!(bridge.after_called);
        assert_eq!(results[0], ("read".into(), true, "Blocked .env".into()));
        assert_eq!(results[1].0, "project_stats");
        assert!(!results[1].1);
        assert!(results[1].2.contains("42"));
    }

    #[test]
    fn cancellation_persists_partial_and_returns_cancelled() {
        let cancel = AtomicBool::new(true);
        let mut bridge = MockBridge { step: 0 };
        let mut session = SessionStore::in_memory();
        let telemetry = TelemetryRecorder::new();
        let mut todos = TodoStore::new();
        let cfg = AgentConfig::default();

        let err = run_turn_loop_cancellable(
            &mut bridge,
            &mut session,
            &telemetry,
            &mut todos,
            None,
            Some(&cancel),
            &cfg,
            "abort me",
            |_| {},
            |_| ApprovalChoice::Deny,
            |_| String::new(),
        )
        .unwrap_err();
        assert!(matches!(err, AgentError::Cancelled));
        assert!(
            session
                .entries()
                .last()
                .is_some_and(|e| e.content.contains("[Interrupted by user]"))
        );
    }

    #[test]
    fn transient_503_retries_and_succeeds() {
        struct FlakyBridge {
            attempts: usize,
        }
        impl ProviderPort for FlakyBridge {
            fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
                Ok(vec!["vibe".into()])
            }
            fn list_models(&mut self, _p: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
                Ok(vec![])
            }
            fn complete(&mut self, _req: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
                self.attempts += 1;
                if self.attempts == 1 {
                    return Err(BridgeError::Remote {
                        kind: "RuntimeError".into(),
                        message: "HTTP 503 Service Unavailable".into(),
                    });
                }
                Ok(AssistantMessage {
                    provider: "vibe".into(),
                    model: "mistral-large-4-0".into(),
                    content: "Recovered!".into(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    stop_reason: "stop".into(),
                    usage: Usage {
                        input_tokens: 1,
                        output_tokens: 1,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cost_usd: 0.0,
                    },
                })
            }
        }
        impl PolicyPort for FlakyBridge {
            fn evaluate_tool_call(
                &mut self,
                _tool_name: &str,
                _command: Option<&str>,
                _target_path: Option<&str>,
            ) -> Result<PolicyDecision, BridgeError> {
                Ok(PolicyDecision {
                    action: "allow".into(),
                    reason: "ok".into(),
                })
            }
        }
        impl ExtensionPort for FlakyBridge {
            fn invoke_hook(&mut self, _event: &str, payload: Value) -> Result<Value, BridgeError> {
                Ok(payload)
            }
        }

        let mut bridge = FlakyBridge { attempts: 0 };
        let mut session = SessionStore::in_memory();
        let telemetry = TelemetryRecorder::new();
        let cfg = AgentConfig::default();
        let mut statuses = Vec::new();
        let msg = run_turn_loop_streaming(&mut bridge, &mut session, &telemetry, &cfg, "hi", |ev| {
            if let AgentEvent::Status { text } = ev {
                statuses.push(text);
            }
        })
        .unwrap();
        assert_eq!(msg.content, "Recovered!");
        assert_eq!(bridge.attempts, 2);
        assert_eq!(statuses.len(), 1);
    }

    #[test]
    fn sanitizes_cross_provider_messages_and_prunes_stale_mode_reminders() {
        let mut msgs = vec![
            ChatMessage {
                role: "user".into(),
                content: format!("{MODE_REMINDER_PREFIX} to PLAN.</system-reminder>"),
                reasoning_content: None,
                tool_calls: vec![],
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "assistant".into(),
                content: String::new(),
                reasoning_content: Some("foreign thought".into()),
                tool_calls: vec![ToolCall {
                    id: "tc_1".into(),
                    name: "read".into(),
                    arguments: json!({"path": "README.md"}),
                    thought_signature: Some("sig_agy".into()),
                }],
                tool_call_id: None,
                name: None,
            },
        ];
        sanitize_messages_for_provider("vibe", &mut msgs);
        assert_eq!(msgs.len(), 2); // assistant + synthesized [interrupted] tool result; stale reminder pruned
        assert_eq!(msgs[0].reasoning_content, None);
        assert_eq!(msgs[0].tool_calls[0].thought_signature, None);
        assert_eq!(msgs[1].role, "tool");
        assert_eq!(msgs[1].content, "[interrupted]");
        assert_eq!(msgs[1].tool_call_id.as_deref(), Some("tc_1"));
    }

    #[test]
    fn interrupted_tool_results_stay_glued_before_intervening_user_turn() {
        let mk_user = |c: &str| ChatMessage {
            role: "user".into(),
            content: c.into(),
            reasoning_content: None,
            tool_calls: vec![],
            tool_call_id: None,
            name: None,
        };
        let mk_tool = |id: &str| ChatMessage {
            role: "tool".into(),
            content: "ok".into(),
            reasoning_content: None,
            tool_calls: vec![],
            tool_call_id: Some(id.into()),
            name: Some("read".into()),
        };
        let mut msgs = vec![
            ChatMessage {
                role: "assistant".into(),
                content: String::new(),
                reasoning_content: None,
                tool_calls: vec![
                    ToolCall {
                        id: "tc_1".into(),
                        name: "read".into(),
                        arguments: json!({"path": "README.md"}),
                        thought_signature: None,
                    },
                    ToolCall {
                        id: "tc_2".into(),
                        name: "ls".into(),
                        arguments: json!({}),
                        thought_signature: None,
                    },
                ],
                tool_call_id: None,
                name: None,
            },
            mk_user("stop, do this instead"),
            mk_tool("tc_1"),
        ];
        sanitize_messages_for_provider("vibe", &mut msgs);
        // Ordering must be: assistant(tool_calls) -> tool tc_1 -> tool [interrupted] tc_2 -> user
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0].role, "assistant");
        assert_eq!(msgs[1].role, "tool");
        assert_eq!(msgs[1].tool_call_id.as_deref(), Some("tc_1"));
        assert_eq!(msgs[1].content, "ok");
        assert_eq!(msgs[2].role, "tool");
        assert_eq!(msgs[2].tool_call_id.as_deref(), Some("tc_2"));
        assert_eq!(msgs[2].content, "[interrupted]");
        assert_eq!(msgs[3].role, "user");
        assert_eq!(msgs[3].content, "stop, do this instead");
    }

    #[test]
    fn mode_transition_reminder_is_edge_triggered() {
        assert!(mode_transition_reminder(Some("plan"), "plan").is_none());
        let to_plan = mode_transition_reminder(Some("normal"), "plan").unwrap();
        assert!(to_plan.contains("read-only"));
        let to_cyber = mode_transition_reminder(Some("plan"), "cyber").unwrap();
        assert!(to_cyber.contains("no longer in read-only"));
    }

    #[test]
    fn compaction_and_handover_replay_produces_valid_role_alternation() {
        struct CaptureBridge {
            seen_messages: Vec<ChatMessage>,
        }
        impl ProviderPort for CaptureBridge {
            fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
                Ok(vec!["vibe".into()])
            }
            fn list_models(&mut self, _p: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
                Ok(vec![])
            }
            fn complete(&mut self, req: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
                self.seen_messages = req.messages;
                Ok(AssistantMessage {
                    provider: "vibe".into(),
                    model: "mistral-large-4-0".into(),
                    content: "Ready!".into(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    stop_reason: "stop".into(),
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 2,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cost_usd: 0.0,
                    },
                })
            }
        }
        impl PolicyPort for CaptureBridge {
            fn evaluate_tool_call(
                &mut self,
                _tool_name: &str,
                _command: Option<&str>,
                _target_path: Option<&str>,
            ) -> Result<PolicyDecision, BridgeError> {
                Ok(PolicyDecision {
                    action: "allow".into(),
                    reason: "ok".into(),
                })
            }
        }
        impl ExtensionPort for CaptureBridge {
            fn invoke_hook(&mut self, _event: &str, payload: Value) -> Result<Value, BridgeError> {
                Ok(payload)
            }
        }

        let mut session = SessionStore::in_memory();
        session
            .append_compaction("Goal: fix auth. Files: auth.rs.")
            .unwrap();
        let mut bridge = CaptureBridge {
            seen_messages: Vec::new(),
        };
        let telemetry = TelemetryRecorder::new();
        let cfg = AgentConfig {
            provider: "vibe".into(),
            model: "mistral-large-4-0".into(),
            ..Default::default()
        };
        let reply = run_turn_loop(&mut bridge, &mut session, &telemetry, &cfg, "continue now").unwrap();
        assert_eq!(reply.content, "Ready!");
        let roles: Vec<&str> = bridge.seen_messages.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, vec!["user", "assistant", "user"]);
        assert!(bridge.seen_messages[0].content.contains("<session_summary>"));
        assert_eq!(bridge.seen_messages[2].content, "continue now");
    }
}
