use le_vibe_agent::{AgentConfig, AgentEvent, ApprovalChoice, OutputStore, run_turn_loop_cancellable};
use le_vibe_durable::{EntryKind, SessionStore};
use le_vibe_mcp::McpManager;
use le_vibe_pybridge::{ChatMessage, CompletionRequest, PolicyPort, ProviderPort, PythonBridge};
use le_vibe_telemetry::TelemetryRecorder;
use le_vibe_tools::TodoStore;
use le_vibe_tui::{
    AgentMode, QuestionItem, QuestionOption, QuestionSession, TuiState, append_model_profile,
    build_system_prompt, format_tokens,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::debug_log::EventLogger;
use crate::features::export_bug_report_pdf;
use crate::settings::{
    current_git_branch, cwd_string, default_mcp_paths, default_model_for, default_skill_dirs,
    format_cwd, project_sessions_dir, run_shell_command, scan_workspace_files, self_repo_path,
    sessions_dir, summarize_git_status,
};

pub(crate) fn humanize_age(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h", seconds / 3_600)
    } else {
        format!("{}d", seconds / 86_400)
    }
}


#[allow(dead_code)]
pub(crate) fn refresh_state(bridge: &mut PythonBridge, state: &mut TuiState, live_network: bool) {
    state.cwd = format_cwd();
    state.git_branch = current_git_branch();
    let models_res = if live_network {
        bridge
            .refresh_models(None)
            .or_else(|_| bridge.list_models(None))
    } else {
        bridge.list_models(None)
    };
    if let Ok(models) = models_res {
        // A provider that is not connected reports no models, so the picker follows the account.
        if let Some(found) = models
            .iter()
            .find(|m| m.provider == state.provider && m.id == state.model)
        {
            state.context_window = found.context_window;
        } else if let Ok(preferred) = bridge.default_model(&state.provider).map(|m| {
            models
                .iter()
                .find(|candidate| candidate.provider == state.provider && candidate.id == m)
                .or_else(|| models.iter().find(|candidate| candidate.provider == state.provider))
                .map(|candidate| (candidate.id.clone(), candidate.context_window))
        }) && let Some((id, ctx)) = preferred
        {
            state.model = id;
            state.context_window = ctx;
        }
        state.available_models = models
            .into_iter()
            .map(|m| {
                let name = if m.exhausted {
                    format!("{} · out of quota", m.name)
                } else {
                    m.name
                };
                (m.provider, m.id, name)
            })
            .collect();
    }
    if let Ok(status) = bridge.auth_status()
        && let Some(obj) = status.as_object()
    {
        for (p, info) in obj {
            let authed = info
                .get("authenticated")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let src = info
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .to_string();
            state.provider_auth.insert(p.clone(), (authed, src));
        }
    }
}

/// ISO-8601 UTC timestamp ("2026-12-01T00:00:00Z") to a Unix epoch second.
pub(crate) fn parse_iso_utc(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let num = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Days since the Unix epoch for a proleptic Gregorian date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// "4d 3h" / "2h 15m" / "12m" until `reset_at`, empty when the time is unknown or past.
pub(crate) fn resets_in(reset_at: Option<&str>) -> String {
    let Some(raw) = reset_at else {
        return String::new();
    };
    let Some(target) = parse_iso_utc(raw) else {
        return String::new();
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let left = target - now;
    if left <= 0 {
        return "resets now".to_string();
    }
    let (days, hours, minutes) = (left / 86_400, (left % 86_400) / 3_600, (left % 3_600) / 60);
    if days > 0 {
        format!("resets {days}d {hours}h")
    } else if hours > 0 {
        format!("resets {hours}h {minutes}m")
    } else {
        format!("resets {minutes}m")
    }
}

/// Name the window a reset time implies: a reset hours away is the rolling five-hour
/// window, one days away is the weekly window, and an unknown reset stays unlabelled.
pub(crate) fn window_label(reset_at: Option<&str>) -> &'static str {
    let Some(raw) = reset_at else {
        return "Limit";
    };
    let Some(target) = parse_iso_utc(raw) else {
        return "Limit";
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    match (target - now) / 3_600 {
        left if left <= 6 => "Five Hour Limit",
        _ => "Weekly Limit",
    }
}

/// A filled bar for the fraction still left, e.g. `[#####################-]`.
pub(crate) fn quota_bar(remaining: f64, width: usize) -> String {
    let filled = ((remaining.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    format!(
        "[{}{}]",
        "#".repeat(filled),
        "-".repeat(width.saturating_sub(filled))
    )
}

pub(crate) fn usage_report(state: &TuiState, live: &Value) -> String {
    let mut lines: Vec<String> = Vec::new();
    let str_of = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or("").to_string();

    // Antigravity: one bar per quota window the account actually has.
    if let Some(agy) = live.get("agy") {
        lines.push(format!(
            "{} ({})",
            str_of(agy.get("plan")),
            str_of(agy.get("tier_id"))
        ));
        if let Some(groups) = agy.get("groups").and_then(Value::as_array) {
            for group in groups {
                let label = str_of(group.get("label"));
                lines.push(String::new());
                lines.push(label);
                if let Some(buckets) = group.get("buckets").and_then(Value::as_array) {
                    for bucket in buckets {
                        let b_name = str_of(bucket.get("label"));
                        let remaining = bucket
                            .get("remaining")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0);
                        let reset = resets_in(bucket.get("reset_at").and_then(Value::as_str));
                        lines.push(format!(
                            "  {} {b_name}: {:.1}% left{}",
                            quota_bar(remaining, 22),
                            remaining * 100.0,
                            if reset.is_empty() {
                                String::new()
                            } else {
                                format!(" · {reset}")
                            }
                        ));
                    }
                } else {
                    let remaining = group
                        .get("remaining")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    let reset = resets_in(group.get("reset_at").and_then(Value::as_str));
                    lines.push(format!(
                        "  {} {} Remaining: {:.1}% left{}",
                        quota_bar(remaining, 22),
                        window_label(group.get("reset_at").and_then(Value::as_str)),
                        remaining * 100.0,
                        if reset.is_empty() {
                            String::new()
                        } else {
                            format!(" · {reset}")
                        }
                    ));
                }
            }
        } else if !agy.get("authenticated").and_then(Value::as_bool).unwrap_or(true) {
            lines.push("  not logged in — /login agy".to_string());
        }
        lines.push(String::new());
    }

    // Mistral: plan plus the monthly Vibe Code budget.
    if let Some(vibe) = live.get("vibe") {
        lines.push(format!("Mistral Vibe ({})", str_of(vibe.get("plan"))));
        match vibe.get("budget_used_pct").and_then(Value::as_f64) {
            Some(used) => {
                let total = vibe
                    .get("budget_total")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let reset = resets_in(vibe.get("budget_reset_at").and_then(Value::as_str));
                lines.push(format!(
                    "  {} Vibe Budget: {:.1}% used · ${:.2} / ${:.0}{}",
                    quota_bar(1.0 - used / 100.0, 22),
                    used,
                    total * used / 100.0,
                    total,
                    if reset.is_empty() {
                        String::new()
                    } else {
                        format!(" · {reset}")
                    }
                ));
            }
            None => lines.push("  budget unavailable".to_string()),
        }
        if let Some(api_used) = vibe.get("api_budget_used_pct").and_then(Value::as_f64) {
            let api_total = vibe
                .get("api_budget_total")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let api_reset = resets_in(vibe.get("api_budget_reset_at").and_then(Value::as_str));
            lines.push(format!(
                "  {} API Budget:  {:.1}% used · ${:.2} / ${:.0}{}",
                quota_bar(1.0 - api_used / 100.0, 22),
                api_used,
                api_total * api_used / 100.0,
                api_total,
                if api_reset.is_empty() {
                    String::new()
                } else {
                    format!(" · {api_reset}")
                }
            ));
        }
        let payg = vibe
            .get("payg_enabled")
            .and_then(Value::as_bool)
            .map(|on| if on { "on" } else { "off" })
            .unwrap_or("unknown");
        lines.push(format!("  Pay-as-you-go: {payg}"));
        lines.push(String::new());
    }

    // DeepSeek: account balance.
    if let Some(deepseek) = live.get("deepseek") {
        lines.push("DeepSeek".to_string());
        match deepseek.get("balances").and_then(Value::as_array) {
            Some(balances) if !balances.is_empty() => {
                for balance in balances {
                    lines.push(format!(
                        "  {} {} balance (granted {} · topped up {})",
                        str_of(balance.get("currency")),
                        str_of(balance.get("total")),
                        str_of(balance.get("granted")),
                        str_of(balance.get("topped_up"))
                    ));
                }
            }
            _ => lines.push(format!(
                "  balance unavailable — {}",
                str_of(deepseek.get("error"))
            )),
        }
        lines.push(String::new());
    }

    lines.push(format!(
        "Session: ↑{} ↓{} R{} · ${:.4}",
        format_tokens(state.input_tokens),
        format_tokens(state.output_tokens),
        format_tokens(state.cache_read_tokens),
        state.cost_usd
    ));
    lines.join("\n")
}

pub(crate) enum WorkerCmd {
    RefreshCatalog {
        live: bool,
        announce: bool,
        provider: String,
        model: String,
    },
    CheckStaleAndRefresh {
        provider: String,
        model: String,
    },
    FetchUsage,
    RunPrompt {
        provider: String,
        model: String,
        thinking_level: String,
        mode: AgentMode,
        shell: String,
        max_turns: usize,
        retry_max: usize,
        prompt: String,
    },
    ResolveModel {
        pattern: String,
        default_provider: String,
    },
    #[allow(dead_code)]
    SwitchProvider {
        provider: String,
    },
    Login {
        target: String,
        api_key: Option<String>,
    },
    Logout {
        target: String,
    },
    RunShell {
        command: String,
        shell: String,
        include_in_context: bool,
    },
    SetYolo(bool),
    SetCodemode(String),
    Compact {
        provider: String,
        model: String,
        instructions: Option<String>,
    },
    Handover {
        provider: String,
        model: String,
        goal: Option<String>,
    },
    ListRewindCheckpoints,
    ListTreeNodes,
    RewindTo(String),
    ListSessions,
    ResumeSession(String),
    NewSession,
    SetSessionName(Option<String>),
    CloneSession,
    SessionInfo,
    ContextInfo {
        mode: AgentMode,
        shell: String,
        context_window: u64,
        last_input_tokens: u64,
    },
    PolicyStatus,
    PluginsStatus,
    ReloadPlugins,
    ReloadProjectResources,
    RunPluginCommand {
        name: String,
        args: String,
    },
    GenerateBugReport {
        provider: String,
        model: String,
        repo: String,
    },
    Skills,
    McpStatus,
    RecordModelChange(String),
    RecordModeChange(String),
    Shutdown,
}

pub(crate) enum WorkerMsg {
    CatalogSynced {
        models: Vec<(String, String, String)>,
        default_thinking: Vec<(String, String, String)>,
        auth: Vec<(String, bool, String)>,
        cwd: String,
        branch: Option<String>,
        resolved_model: Option<(String, u64)>,
        notice: Option<String>,
    },
    UsageLoaded(Value),
    ModelResolved {
        provider: String,
        model: String,
        thinking_level: String,
        context_window: u64,
    },
    ProviderSwitched {
        provider: String,
        model: String,
        thinking_level: String,
        context_window: u64,
    },
    Agent(AgentEvent),
    ApprovalNeeded {
        tool: String,
        detail: String,
        reason: String,
    },
    QuestionNeeded(QuestionSession),
    ShellResult {
        command: String,
        output: String,
        in_context: bool,
        git_badges: String,
    },
    TurnDone {
        provider: String,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cost_usd: f64,
        tps: Option<f64>,
        git_badges: String,
    },
    TurnError(String),
    Notice(String),
    InfoModal {
        title: String,
        lines: Vec<String>,
    },
    RewindLoaded(Vec<(String, String, String)>),
    SessionsLoaded(Vec<(String, String, String)>),
    WorkspaceFilesLoaded(Vec<String>),
    PluginCommandsLoaded(Vec<(String, String)>),
    SessionReplaced {
        blocks: Vec<(String, String)>,
        notice: String,
        prefill_input: Option<String>,
        reset_usage: bool,
    },
    TodosUpdated(Vec<(u64, String, String)>),
    Idle,
}

fn run_llm_summary(
    bridge: &mut PythonBridge,
    session: &SessionStore,
    provider: &str,
    model: &str,
    instruction: &str,
) -> Result<String, String> {
    let branch = session.active_branch();
    if branch.is_empty() {
        return Err("Session is empty — nothing to summarize.".to_string());
    }
    let mut transcript = String::new();
    for entry in &branch {
        let line = match entry.kind {
            EntryKind::Message | EntryKind::CompactionSummary | EntryKind::BranchSummary => {
                format!("[{}]: {}\n", entry.role, entry.content)
            }
            EntryKind::ToolCall => format!(
                "[tool_call {}]: {}\n",
                entry.tool_name.as_deref().unwrap_or("tool"),
                entry.content
            ),
            EntryKind::ToolResult => {
                let preview: String = entry.content.chars().take(400).collect();
                format!(
                    "[tool_result {}]: {}\n",
                    entry.tool_name.as_deref().unwrap_or("tool"),
                    preview
                )
            }
            EntryKind::ModelChange | EntryKind::ModeChange | EntryKind::SessionMeta => {
                continue;
            }
        };
        transcript.push_str(&line);
    }
    let req = CompletionRequest {
        provider: provider.to_string(),
        model: model.to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: format!("{instruction}\n\n<transcript>\n{transcript}\n</transcript>"),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
        }],
        system_prompt: Some("You are a precise technical summarizer. Output concise Markdown.".to_string()),
        tools: Vec::new(),
        thinking_level: "low".to_string(),
    };
    let reply = bridge.complete(req).map_err(|e| e.to_string())?;
    Ok(reply.content)
}

fn extract_and_flush_memories(summary_raw: &str) -> (String, usize) {
    let marker = "```memory-flush";
    let Some(start) = summary_raw.find(marker) else {
        return (summary_raw.to_string(), 0);
    };
    let after = &summary_raw[start + marker.len()..];
    let Some(end_rel) = after.find("```") else {
        return (summary_raw.to_string(), 0);
    };
    let json_block = after[..end_rel].trim();
    let cleaned = format!(
        "{}{}",
        summary_raw[..start].trim_end(),
        after[end_rel + 3..].trim_start()
    )
    .trim()
    .to_string();

    let mut saved = 0usize;
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json_block) {
        let store = le_vibe_tools::memory::MemoryRagStore::open_default();
        for item in items.into_iter().take(5) {
            let Some(content) = item.get("content").and_then(Value::as_str) else {
                continue;
            };
            if content.trim().is_empty() {
                continue;
            }
            let title = item.get("title").and_then(Value::as_str);
            let key = item
                .get("key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            let keywords: Vec<String> = item
                .get("keywords")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if store
                .write_memory("project", "long_term", title, content, keywords, key)
                .is_ok()
            {
                saved += 1;
            }
        }
    }
    (if cleaned.is_empty() { summary_raw.to_string() } else { cleaned }, saved)
}

fn session_blocks(session: &SessionStore) -> Vec<(String, String)> {
    session
        .active_branch()
        .into_iter()
        .filter_map(|e| match e.kind {
            EntryKind::Message => Some((e.role, e.content)),
            EntryKind::CompactionSummary | EntryKind::BranchSummary => {
                Some(("system".to_string(), e.content))
            }
            _ => None,
        })
        .collect()
}

pub(crate) struct SyncPayload {
    models: Vec<(String, String, String)>,
    default_thinking: Vec<(String, String, String)>,
    auth: Vec<(String, bool, String)>,
    resolved_model: Option<(String, u64)>,
}

pub(crate) fn collect_sync_payload(
    bridge: &mut PythonBridge,
    provider: &str,
    model: &str,
    live_network: bool,
) -> SyncPayload {
    let models_res = if live_network {
        bridge
            .refresh_models(None)
            .or_else(|_| bridge.list_models(None))
    } else {
        bridge.list_models(None)
    };
    let mut resolved_model = None;
    let mut avail = Vec::new();
    let mut default_thinking = Vec::new();
    if let Ok(models) = models_res {
        if let Some(found) = models
            .iter()
            .find(|m| m.provider == provider && m.id == model)
        {
            resolved_model = Some((found.id.clone(), found.context_window));
        } else if let Ok(pref) = bridge.default_model(provider)
            && let Some(cand) = models
                .iter()
                .find(|c| c.provider == provider && c.id == pref)
                .or_else(|| models.iter().find(|c| c.provider == provider))
        {
            resolved_model = Some((cand.id.clone(), cand.context_window));
        }
        for m in models {
            default_thinking.push((m.provider.clone(), m.id.clone(), m.default_thinking));
            let name = if m.exhausted {
                format!("{} · out of quota", m.name)
            } else {
                m.name
            };
            avail.push((m.provider, m.id, name));
        }
    }
    let mut auth_vec = Vec::new();
    if let Ok(status) = bridge.auth_status()
        && let Some(obj) = status.as_object()
    {
        for (p, info) in obj {
            let authed = info
                .get("authenticated")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let src = info
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .to_string();
            auth_vec.push((p.clone(), authed, src));
        }
    }
    SyncPayload {
        models: avail,
        default_thinking,
        auth: auth_vec,
        resolved_model,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_tui_worker(
    mut bridge: PythonBridge,
    mut session: SessionStore,
    telemetry: TelemetryRecorder,
    cancel: Arc<AtomicBool>,
    steering_queue: Arc<Mutex<VecDeque<String>>>,
    project_trusted: bool,
    logger: Arc<EventLogger>,
    system_prompt_extra: Option<String>,
) -> (
    mpsc::Sender<WorkerCmd>,
    mpsc::Receiver<WorkerMsg>,
    mpsc::Sender<ApprovalChoice>,
    mpsc::Sender<String>,
) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCmd>();
    let (msg_tx, msg_rx) = mpsc::channel::<WorkerMsg>();
    let (approval_tx, approval_rx) = mpsc::channel::<ApprovalChoice>();
    let (question_tx, question_rx) = mpsc::channel::<String>();

    std::thread::spawn(move || {
        bridge.set_cancel_token(Arc::clone(&cancel));
        let mut todos = TodoStore::new();
        let output_store = Arc::new(Mutex::new(OutputStore::new()));
        let mut codemode_setting = "off".to_string();
        // Freeze `<memory_toc>` and `<model_profile>` per session/model so mid-session
        // `memory_write` never busts the provider KV-cache prefix.
        let mut session_memory_toc = le_vibe_tools::memory::MemoryRagStore::open_default().build_toc(25);
        let mut session_model_note: Option<(String, Option<String>)> = None;
        let mcp_started = Instant::now();
        let mcp_paths: Vec<std::path::PathBuf> = default_mcp_paths()
            .into_iter()
            .filter(|p| project_trusted || p.is_absolute())
            .collect();
        let mut mcp = McpManager::from_config_files(&mcp_paths);
        logger.log(
            "worker",
            "mcp_init",
            Some(mcp_started.elapsed().as_millis() as u64),
            format!("servers={}", mcp.status_summary().len()),
        );
        let ws_files = scan_workspace_files(std::path::Path::new("."), 1500);
        let _ = msg_tx.send(WorkerMsg::WorkspaceFilesLoaded(ws_files));
        if let Ok(cmds) = bridge.list_extension_commands() {
            let _ = msg_tx.send(WorkerMsg::PluginCommandsLoaded(cmds));
        }
        if !session.active_branch().is_empty() {
            let blocks = session_blocks(&session);
            let count = session.active_branch().len();
            let path_label = session
                .path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "session".to_string());
            let _ = msg_tx.send(WorkerMsg::SessionReplaced {
                blocks,
                notice: format!("Restored session {path_label} ({count} entries)"),
                prefill_input: None,
                reset_usage: false,
            });
        }
        while let Ok(cmd) = cmd_rx.recv() {
            let cmd_started = Instant::now();
            match cmd {
                WorkerCmd::Shutdown => {
                    logger.log("worker", "shutdown", None, "worker stopping");
                    break;
                }
                WorkerCmd::SetYolo(enabled) => {
                    let _ = bridge.set_yolo(enabled);
                }
                WorkerCmd::SetCodemode(mode_val) => {
                    codemode_setting = mode_val;
                }
                WorkerCmd::RefreshCatalog {
                    live,
                    announce,
                    provider,
                    model,
                } => {
                    let SyncPayload {
                        models,
                        default_thinking,
                        auth,
                        resolved_model,
                    } = collect_sync_payload(&mut bridge, &provider, &model, live);
                    let cwd = format_cwd();
                    let branch = current_git_branch();
                    let notice = if announce {
                        let _ = bridge.reload_policy();
                        let _ = bridge.reload_extensions();
                        session_memory_toc =
                            le_vibe_tools::memory::MemoryRagStore::open_default().build_toc(25);
                        session_model_note = None;
                        if let Ok(cmds) = bridge.list_extension_commands() {
                            let _ = msg_tx.send(WorkerMsg::PluginCommandsLoaded(cmds));
                        }
                        mcp = McpManager::from_config_files(&mcp_paths);
                        let age = bridge
                            .catalog_state()
                            .ok()
                            .and_then(|(age, _)| age)
                            .map(humanize_age)
                            .unwrap_or_else(|| "just now".to_string());
                        let skill_dirs = default_skill_dirs();
                        let skill_refs: Vec<&str> = skill_dirs.iter().map(String::as_str).collect();
                        let skill_count = bridge
                            .list_skills(&skill_refs)
                            .map(|s| s.len())
                            .unwrap_or(0);
                        Some(format!(
                            "Reloaded live catalogs: {} models across 3 providers (updated {age}), {skill_count} skills, cwd={cwd}",
                            models.len()
                        ))
                    } else {
                        None
                    };
                    let _ = msg_tx.send(WorkerMsg::CatalogSynced {
                        models,
                        default_thinking,
                        auth,
                        cwd,
                        branch,
                        resolved_model,
                        notice,
                    });
                    logger.log(
                        "worker",
                        "refresh_catalog",
                        Some(cmd_started.elapsed().as_millis() as u64),
                        format!("live={live}"),
                    );
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::CheckStaleAndRefresh { provider, model } => {
                    if let Ok((_age, true)) = bridge.catalog_state() {
                        let SyncPayload {
                            models,
                            default_thinking,
                            auth,
                            resolved_model,
                        } = collect_sync_payload(&mut bridge, &provider, &model, true);
                        let _ = msg_tx.send(WorkerMsg::CatalogSynced {
                            models,
                            default_thinking,
                            auth,
                            cwd: format_cwd(),
                            branch: current_git_branch(),
                            resolved_model,
                            notice: None,
                        });
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::FetchUsage => {
                    let live = bridge.fetch_usage(None).unwrap_or(Value::Null);
                    logger.log(
                        "worker",
                        "fetch_usage",
                        Some(cmd_started.elapsed().as_millis() as u64),
                        "fetched live quota",
                    );
                    let _ = msg_tx.send(WorkerMsg::UsageLoaded(live));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::ResolveModel {
                    pattern,
                    default_provider,
                } => {
                    match bridge.resolve_model(&pattern, &default_provider) {
                        Ok(resolved) => {
                            let ctx = bridge
                                .list_models(Some(&resolved.provider))
                                .ok()
                                .and_then(|ms| {
                                    ms.into_iter()
                                        .find(|m| m.id == resolved.model)
                                        .map(|m| m.context_window)
                                })
                                .unwrap_or(256_000);
                            let _ = msg_tx.send(WorkerMsg::ModelResolved {
                                provider: resolved.provider,
                                model: resolved.model,
                                thinking_level: resolved.thinking_level,
                                context_window: ctx,
                            });
                        }
                        Err(e) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Model not found: {e}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::SwitchProvider { provider } => {
                    let model = default_model_for(&mut bridge, &provider);
                    let found = bridge
                        .list_models(Some(&provider))
                        .ok()
                        .and_then(|ms| ms.into_iter().find(|m| m.id == model));
                    let ctx = found.as_ref().map(|m| m.context_window).unwrap_or(256_000);
                    let def_think = found
                        .map(|m| m.default_thinking)
                        .unwrap_or_else(|| "medium".to_string());
                    let _ = msg_tx.send(WorkerMsg::ProviderSwitched {
                        provider,
                        model,
                        thinking_level: def_think,
                        context_window: ctx,
                    });
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::Login { target, api_key } => {
                    let res = if let Some(key) = api_key {
                        bridge.save_api_key(&target, &key)
                    } else if target == "agy" {
                        bridge.login_agy_oauth()
                    } else if target == "vibe" {
                        bridge.login_vibe_oidc()
                    } else {
                        Err(le_vibe_pybridge::BridgeError::Remote {
                            kind: "Auth".into(),
                            message: "For deepseek, pass your API key: /login deepseek <DEEPSEEK_API_KEY>".into(),
                        })
                    };
                    match res {
                        Ok(path) => {
                            let SyncPayload {
                                models,
                                default_thinking,
                                auth,
                                resolved_model,
                            } = collect_sync_payload(&mut bridge, &target, "", true);
                            let _ = msg_tx.send(WorkerMsg::CatalogSynced {
                                models,
                                default_thinking,
                                auth,
                                cwd: format_cwd(),
                                branch: current_git_branch(),
                                resolved_model,
                                notice: Some(format!("Logged in to {target} (saved to {path})")),
                            });
                        }
                        Err(e) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Login failed: {e}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::Logout { target } => {
                    match bridge.logout(&target) {
                        Ok(true) => {
                            let SyncPayload {
                                models,
                                default_thinking,
                                auth,
                                resolved_model,
                            } = collect_sync_payload(&mut bridge, &target, "", true);
                            let _ = msg_tx.send(WorkerMsg::CatalogSynced {
                                models,
                                default_thinking,
                                auth,
                                cwd: format_cwd(),
                                branch: current_git_branch(),
                                resolved_model,
                                notice: Some(format!("Removed saved credentials for {target}.")),
                            });
                        }
                        Ok(false) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!(
                                "No saved credentials found for {target}."
                            )));
                        }
                        Err(e) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Logout failed: {e}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::RunShell {
                    command,
                    shell,
                    include_in_context,
                } => {
                    let shell_out = run_shell_command(&shell, &command);
                    let text = match shell_out {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            let code = out.status.code().unwrap_or(-1);
                            let mut combined = String::new();
                            if !stdout.trim().is_empty() {
                                combined.push_str(stdout.trim_end());
                            }
                            if !stderr.trim().is_empty() {
                                if !combined.is_empty() {
                                    combined.push('\n');
                                }
                                combined.push_str(stderr.trim_end());
                            }
                            if combined.is_empty() {
                                format!("exit_code={code}")
                            } else {
                                format!("{combined}\n[exit_code={code}]")
                            }
                        }
                        Err(e) => format!("Failed to run command: {e}"),
                    };
                    if include_in_context {
                        let _ = session.append(
                            "user",
                            format!("[shell: ! {command}]\n{text}"),
                        );
                    }
                    let _ = msg_tx.send(WorkerMsg::ShellResult {
                        command,
                        output: text,
                        in_context: include_in_context,
                        git_badges: summarize_git_status(),
                    });
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::Compact {
                    provider,
                    model,
                    instructions,
                } => {
                    let count = session.active_branch().len();
                    let extra = instructions
                        .as_deref()
                        .map(|i| format!(" Focus especially on: {i}."))
                        .unwrap_or_default();
                    let prompt = format!(
                        "Summarize the conversation into a concise technical handoff covering: (1) user goal, (2) files inspected/modified, (3) key decisions and constraints, (4) current state and remaining work. Keep under 600 words.{extra}\nIf (and ONLY if) the conversation revealed durable architectural decisions, non-obvious constraints, or bug root causes worth keeping across future sessions, append a JSON array block fenced as ```memory-flush\\n[{{\"title\":\"...\",\"key\":\"...\",\"keywords\":[...],\"content\":\"...\"}}]\\n``` (max 3 items; omit if nothing durable)."
                    );
                    match run_llm_summary(&mut bridge, &session, &provider, &model, &prompt) {
                        Ok(raw_summary) => {
                            let (summary, flushed) = extract_and_flush_memories(&raw_summary);
                            let _ = session.append_compaction(&summary);
                            session_memory_toc =
                                le_vibe_tools::memory::MemoryRagStore::open_default().build_toc(25);
                            session_model_note = None;
                            let flush_note = if flushed > 0 {
                                format!(" · saved {flushed} durable memory entry(s) to MEMORY.md")
                            } else {
                                String::new()
                            };
                            let _ = msg_tx.send(WorkerMsg::Notice(format!(
                                "Compacted {count} entries via {provider}/{model}{flush_note}:\n{summary}"
                            )));
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Compaction failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::Handover {
                    provider,
                    model,
                    goal,
                } => {
                    let extra = goal
                        .as_deref()
                        .map(|g| format!(" Target next goal: {g}."))
                        .unwrap_or_default();
                    let open_todos: Vec<String> = todos
                        .items()
                        .iter()
                        .filter(|t| t.status != "deleted" && t.status != "completed")
                        .map(|t| format!("- [{}] {}", t.status, t.subject))
                        .collect();
                    let todo_ctx = if open_todos.is_empty() {
                        String::new()
                    } else {
                        format!("\nOpen todos to carry over:\n{}", open_todos.join("\n"))
                    };
                    let prompt = format!(
                        "Produce a structured session handover document covering: (1) Goal, (2) Files inspected/modified, (3) Decisions & constraints, (4) Open items / verification commands.{extra}{todo_ctx}\nIf (and ONLY if) the session revealed durable decisions, constraints, or bug root causes worth keeping across future sessions, append a JSON array block fenced as ```memory-flush\\n[{{\"title\":\"...\",\"key\":\"...\",\"keywords\":[...],\"content\":\"...\"}}]\\n``` (max 3 items; omit if nothing durable)."
                    );
                    match run_llm_summary(&mut bridge, &session, &provider, &model, &prompt) {
                        Ok(raw_summary) => {
                            let (summary, flushed) = extract_and_flush_memories(&raw_summary);
                            let ts = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            let path = project_sessions_dir().join(format!("session-{ts}.jsonl"));
                            if let Ok(mut fresh) = SessionStore::open(&path) {
                                let _ = fresh.append_compaction(&summary);
                                session = fresh;
                                session_memory_toc =
                                    le_vibe_tools::memory::MemoryRagStore::open_default().build_toc(25);
                                session_model_note = None;
                                let flush_note = if flushed > 0 {
                                    format!(" · saved {flushed} durable memory entry(s)")
                                } else {
                                    String::new()
                                };
                                let _ = msg_tx.send(WorkerMsg::SessionReplaced {
                                    blocks: session_blocks(&session),
                                    notice: format!(
                                        "Handover complete -> new session {} (context reset, summary loaded{flush_note})",
                                        path.display()
                                    ),
                                    prefill_input: None,
                                    reset_usage: true,
                                });
                            }
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Handover failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::ListRewindCheckpoints => {
                    let rows: Vec<(String, String, String)> = session
                        .entries()
                        .iter()
                        .filter(|e| e.kind == EntryKind::Message && e.role == "user")
                        .enumerate()
                        .map(|(i, e)| {
                            let preview: String = e.content.chars().take(68).collect();
                            (
                                e.id.clone(),
                                format!("#{} user", i + 1),
                                preview.replace('\n', " "),
                            )
                        })
                        .collect();
                    let _ = msg_tx.send(WorkerMsg::RewindLoaded(rows));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::ListTreeNodes => {
                    let entries = session.entries();
                    let active_leaf = session.leaf_id().map(str::to_string);
                    let mut children: std::collections::HashMap<Option<String>, Vec<usize>> =
                        std::collections::HashMap::new();
                    for (idx, e) in entries.iter().enumerate() {
                        if matches!(e.kind, EntryKind::Message | EntryKind::CompactionSummary)
                            && matches!(e.role.as_str(), "user" | "assistant" | "system")
                        {
                            children.entry(e.parent_id.clone()).or_default().push(idx);
                        }
                    }
                    // Also include roots whose parent_id wasn't a message (e.g. tool_result or session_meta)
                    let entry_by_id: std::collections::HashMap<&str, usize> = entries
                        .iter()
                        .enumerate()
                        .map(|(i, e)| (e.id.as_str(), i))
                        .collect();
                    let mut rows: Vec<(String, String, String)> = Vec::new();
                    let mut visited = std::collections::HashSet::new();
                    let mut stack: Vec<(usize, usize)> = Vec::new();
                    for (idx, e) in entries.iter().enumerate() {
                        if !matches!(e.kind, EntryKind::Message | EntryKind::CompactionSummary)
                            || !matches!(e.role.as_str(), "user" | "assistant" | "system")
                        {
                            continue;
                        }
                        let parent_is_msg = e
                            .parent_id
                            .as_deref()
                            .and_then(|pid| entry_by_id.get(pid))
                            .is_some_and(|&pi| {
                                matches!(
                                    entries[pi].kind,
                                    EntryKind::Message | EntryKind::CompactionSummary
                                ) && matches!(
                                    entries[pi].role.as_str(),
                                    "user" | "assistant" | "system"
                                )
                            });
                        if !parent_is_msg {
                            stack.push((idx, 0));
                        }
                    }
                    stack.reverse();
                    while let Some((idx, depth)) = stack.pop() {
                        if !visited.insert(idx) {
                            continue;
                        }
                        let e = &entries[idx];
                        let is_leaf = active_leaf.as_deref() == Some(e.id.as_str());
                        let indent = "  ".repeat(depth.min(6));
                        let branch_glyph = if depth == 0 { "" } else { "└─ " };
                        let star = if is_leaf { "★ " } else { "" };
                        let role_tag = if e.kind == EntryKind::CompactionSummary {
                            "summary"
                        } else {
                            e.role.as_str()
                        };
                        let preview: String = e
                            .content
                            .replace('\n', " ")
                            .chars()
                            .take(64)
                            .collect();
                        rows.push((
                            e.id.clone(),
                            format!("{indent}{branch_glyph}{star}{role_tag}"),
                            preview,
                        ));
                        // Walk through any intermediate tool_result children to find descendant messages
                        let mut next_children: Vec<usize> = Vec::new();
                        let mut frontier = vec![e.id.clone()];
                        while let Some(pid) = frontier.pop() {
                            for (ci, ce) in entries.iter().enumerate() {
                                if ce.parent_id.as_deref() == Some(pid.as_str()) {
                                    if matches!(
                                        ce.kind,
                                        EntryKind::Message | EntryKind::CompactionSummary
                                    ) && matches!(
                                        ce.role.as_str(),
                                        "user" | "assistant" | "system"
                                    ) {
                                        next_children.push(ci);
                                    } else {
                                        frontier.push(ce.id.clone());
                                    }
                                }
                            }
                        }
                        let next_depth = if next_children.len() > 1 { depth + 1 } else { depth };
                        for ci in next_children.into_iter().rev() {
                            stack.push((ci, next_depth));
                        }
                    }
                    let _ = children; // silence unused if empty
                    let _ = msg_tx.send(WorkerMsg::RewindLoaded(rows));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::RewindTo(id) => {
                    let target = session.entries().iter().find(|e| e.id == id).cloned();
                    match target {
                        Some(entry) => {
                            let is_user = entry.kind == EntryKind::Message && entry.role == "user";
                            let prefill = if is_user {
                                Some(entry.content.clone())
                            } else {
                                None
                            };
                            let branch_res = if is_user {
                                if let Some(parent_id) = entry.parent_id.as_deref() {
                                    session.branch_from(parent_id)
                                } else {
                                    session.reset_leaf();
                                    Ok(())
                                }
                            } else {
                                session.branch_from(&entry.id)
                            };
                            match branch_res {
                                Ok(()) => {
                                    let note = if is_user {
                                        format!(
                                            "Switched branch before {id}. Prompt loaded into editor — edit or press Enter to branch."
                                        )
                                    } else {
                                        format!("Switched active branch to node {id}.")
                                    };
                                    let _ = msg_tx.send(WorkerMsg::SessionReplaced {
                                        blocks: session_blocks(&session),
                                        notice: note,
                                        prefill_input: prefill,
                                        reset_usage: false,
                                    });
                                }
                                Err(err) => {
                                    let _ = msg_tx.send(WorkerMsg::Notice(format!("Branch switch failed: {err}")));
                                }
                            }
                        }
                        None => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Checkpoint {id} not found.")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::ListSessions => {
                    let mut list = SessionStore::list_sessions(&project_sessions_dir());
                    if list.is_empty() {
                        list = SessionStore::list_sessions(&sessions_dir());
                    }
                    let rows = list
                        .into_iter()
                        .map(|s| {
                            let label = s
                                .name
                                .unwrap_or_else(|| format!("{} entries", s.entry_count));
                            (s.path.to_string_lossy().to_string(), label, s.preview)
                        })
                        .collect();
                    let _ = msg_tx.send(WorkerMsg::SessionsLoaded(rows));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::ResumeSession(path_str) => {
                    match SessionStore::open(&path_str) {
                        Ok(loaded) => {
                            session = loaded;
                            let _ = msg_tx.send(WorkerMsg::SessionReplaced {
                                blocks: session_blocks(&session),
                                notice: format!("Resumed session {path_str}"),
                                prefill_input: None,
                                reset_usage: true,
                            });
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Resume failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::NewSession => {
                    let ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let path = project_sessions_dir().join(format!("session-{ts}.jsonl"));
                    match SessionStore::open(&path) {
                        Ok(fresh) => {
                            session = fresh;
                            session_memory_toc =
                                le_vibe_tools::memory::MemoryRagStore::open_default().build_toc(25);
                            session_model_note = None;
                            let _ = msg_tx.send(WorkerMsg::SessionReplaced {
                                blocks: Vec::new(),
                                notice: format!("Started new session {}", path.display()),
                                prefill_input: None,
                                reset_usage: true,
                            });
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("New session failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::SetSessionName(name) => {
                    if let Some(n) = name {
                        let _ = session.set_name(&n);
                        let _ = msg_tx.send(WorkerMsg::Notice(format!("Session named: {n}")));
                    } else {
                        let cur = session.name().unwrap_or("(unnamed)");
                        let _ = msg_tx.send(WorkerMsg::Notice(format!("Session name: {cur}")));
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::CloneSession => {
                    let ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let path = project_sessions_dir().join(format!("session-{ts}.jsonl"));
                    match session.clone_branch(&path) {
                        Ok(cloned) => {
                            session = cloned;
                            let _ = msg_tx.send(WorkerMsg::SessionReplaced {
                                blocks: session_blocks(&session),
                                notice: format!("Cloned active branch -> {}", path.display()),
                                prefill_input: None,
                                reset_usage: false,
                            });
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Clone failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::PolicyStatus => {
                    match bridge.policy_status() {
                        Ok(val) => {
                            let yolo = val.get("yolo").and_then(Value::as_bool).unwrap_or(false);
                            let mut lines = vec![format!(
                                "Active Policy (yolo={})",
                                if yolo { "on" } else { "off" }
                            )];
                            let files: Vec<&str> = val
                                .get("loaded_files")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().filter_map(Value::as_str).collect())
                                .unwrap_or_default();
                            lines.push(format!(
                                "Loaded files: {}",
                                if files.is_empty() {
                                    "(default built-in policy)".to_string()
                                } else {
                                    files.join(", ")
                                }
                            ));
                            if let Some(folders) = val.get("trusted_folders").and_then(Value::as_array)
                                && !folders.is_empty()
                            {
                                let list: Vec<&str> = folders.iter().filter_map(Value::as_str).collect();
                                lines.push(format!("Trusted folders: {}", list.join(", ")));
                            }
                            if let Some(perms) = val.get("tool_permissions").and_then(Value::as_object) {
                                let pairs: Vec<String> = perms
                                    .iter()
                                    .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("?")))
                                    .collect();
                                lines.push(format!("Tool permissions: {}", pairs.join(" · ")));
                            }
                            if let Some(blocked) = val.get("blocked_commands").and_then(Value::as_array)
                                && !blocked.is_empty()
                            {
                                let list: Vec<&str> = blocked.iter().filter_map(Value::as_str).collect();
                                lines.push(format!("Blocked commands: {}", list.join(", ")));
                            }
                            if let Some(rules) = val.get("session_rules").and_then(Value::as_array)
                                && !rules.is_empty()
                            {
                                lines.push(format!("Approved rules: {} rule(s)", rules.len()));
                            }
                            let _ = msg_tx.send(WorkerMsg::InfoModal {
                                title: "Active Policies (/policy)".to_string(),
                                lines,
                            });
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Policy query failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::PluginsStatus => {
                    match bridge.extensions_status() {
                        Ok(val) => {
                            let loaded: Vec<&str> = val
                                .get("loaded")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().filter_map(Value::as_str).collect())
                                .unwrap_or_default();
                            let tools: Vec<&str> = val
                                .get("tools")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().filter_map(Value::as_str).collect())
                                .unwrap_or_default();
                            let cmds: Vec<String> = val
                                .get("commands")
                                .and_then(Value::as_array)
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|c| {
                                            let n = c.get("name")?.as_str()?;
                                            let d = c.get("description").and_then(Value::as_str).unwrap_or("");
                                            Some(format!("{n} ({d})"))
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let errs: Vec<&str> = val
                                .get("errors")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().filter_map(Value::as_str).collect())
                                .unwrap_or_default();
                            let mut lines = vec![format!(
                                "Plugins / Extensions (~/.le-vibe/extensions & .le-vibe/extensions): {} loaded",
                                loaded.len()
                            )];
                            for p in loaded {
                                lines.push(format!("  • {p}"));
                            }
                            if !tools.is_empty() {
                                lines.push(format!("Tools: {}", tools.join(", ")));
                            }
                            if !cmds.is_empty() {
                                lines.push(format!("Slash commands: {}", cmds.join(", ")));
                            }
                            if !errs.is_empty() {
                                lines.push(format!("Load errors: {}", errs.join(" | ")));
                            }
                            let _ = msg_tx.send(WorkerMsg::InfoModal {
                                title: "Extensions & Plugins (/plugins)".to_string(),
                                lines,
                            });
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Plugins status failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::ReloadPlugins => {
                    match bridge.reload_extensions() {
                        Ok(val) => {
                            let count = val
                                .get("loaded")
                                .and_then(Value::as_array)
                                .map(|a| a.len())
                                .unwrap_or(0);
                            if let Ok(cmds) = bridge.list_extension_commands() {
                                let _ = msg_tx.send(WorkerMsg::PluginCommandsLoaded(cmds));
                            }
                            let _ = msg_tx.send(WorkerMsg::Notice(format!(
                                "Reloaded {count} plugin/extension(s)."
                            )));
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Reload plugins failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::ReloadProjectResources => {
                    let mcp_paths = default_mcp_paths();
                    mcp = McpManager::from_config_files(&mcp_paths);
                    let _ = bridge.reload_extensions();
                    if let Ok(cmds) = bridge.list_extension_commands() {
                        let _ = msg_tx.send(WorkerMsg::PluginCommandsLoaded(cmds));
                    }
                    let _ = msg_tx.send(WorkerMsg::Notice(format!(
                        "Project trusted — loaded project MCP ({} server(s)) & extensions.",
                        mcp.status_summary().len()
                    )));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::RunPluginCommand { name, args } => {
                    match bridge.run_extension_command(&name, &args) {
                        Ok(out) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(out));
                        }
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Plugin command {name} failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::GenerateBugReport {
                    provider,
                    model,
                    repo,
                } => {
                    let prompt = "Generate an industry-standard Software Defect & Fix Report in Markdown from the conversation transcript. Include these exact sections: # Title, ## Metadata (Severity, Priority, Environment, Component), ## Summary, ## Steps to Reproduce, ## Expected vs Actual Behavior, ## Root Cause Analysis, ## Fix Applied, ## Verification & Test Evidence, ## Follow-up Recommendations.";
                    match run_llm_summary(&mut bridge, &session, &provider, &model, prompt) {
                        Ok(md) => match export_bug_report_pdf(&repo, &format!("{provider}/{model}"), &md) {
                            Ok((path, true)) => {
                                let _ = msg_tx.send(WorkerMsg::Notice(format!(
                                    "Generated bug report PDF: {}",
                                    path.display()
                                )));
                            }
                            Ok((path, false)) => {
                                let _ = msg_tx.send(WorkerMsg::Notice(format!(
                                    "No headless browser found for PDF; saved printable HTML report: {}",
                                    path.display()
                                )));
                            }
                            Err(err) => {
                                let _ = msg_tx.send(WorkerMsg::Notice(format!("Bug report export failed: {err}")));
                            }
                        },
                        Err(err) => {
                            let _ = msg_tx.send(WorkerMsg::Notice(format!("Bug report generation failed: {err}")));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::SessionInfo => {
                    let path = session
                        .path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "(in-memory)".to_string());
                    let name = session.name().unwrap_or("(unnamed)");
                    let total = session.entries().len();
                    let active = session.active_branch().len();
                    let _ = msg_tx.send(WorkerMsg::Notice(format!(
                        "Session: {name}\nFile: {path}\nEntries: {active} active / {total} total\nLeaf: {}",
                        session.leaf_id().unwrap_or("-")
                    )));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::ContextInfo {
                    mode,
                    shell,
                    context_window,
                    last_input_tokens,
                } => {
                    let skill_dirs = default_skill_dirs();
                    let skill_refs: Vec<&str> = skill_dirs.iter().map(String::as_str).collect();
                    let skills: Vec<(String, String, String)> = bridge
                        .list_skills(&skill_refs)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|s| (s.name, s.description, s.path))
                        .collect();
                    let sys = build_system_prompt(
                        mode,
                        &shell,
                        &cwd_string(),
                        &self_repo_path(),
                        &skills,
                        &session_memory_toc,
                    );
                    let sys_tok = sys.len() / 4;
                    let branch_bytes: usize = session.active_branch().iter().map(|e| e.content.len()).sum();
                    let msg_tok = branch_bytes / 4;
                    let pct = if context_window > 0 {
                        (last_input_tokens as f64 / context_window as f64) * 100.0
                    } else {
                        0.0
                    };
                    let headroom_line = output_store
                        .lock()
                        .ok()
                        .map(|s| {
                            format!(
                                "\n  Headroom compression: {} output(s) crushed, ~{} tok saved ({} retrieve(s))",
                                s.stats.crushed_count,
                                format_tokens(s.stats.est_tokens_saved() as u64),
                                s.stats.retrieve_hits
                            )
                        })
                        .unwrap_or_default();
                    let _ = msg_tx.send(WorkerMsg::Notice(format!(
                        "Context Breakdown:\n  System + AGENTS.md + skills: ~{sys_tok} tok\n  Active branch messages: ~{msg_tok} tok\n  Last reported input: {last_input_tokens} / {context_window} ({pct:.1}%)\n  Auto-compact threshold: 85%{headroom_line}"
                    )));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::Skills => {
                    let skill_dirs = default_skill_dirs();
                    let skill_refs: Vec<&str> = skill_dirs.iter().map(String::as_str).collect();
                    let lines = match bridge.list_skills(&skill_refs) {
                        Ok(skills) if skills.is_empty() => {
                            vec!["No SKILL.md files discovered in project or global skill directories.".to_string()]
                        }
                        Ok(skills) => skills
                            .into_iter()
                            .map(|s| format!("• {} — {} ({})", s.name, s.description, s.path))
                            .collect(),
                        Err(err) => vec![format!("Failed to list skills: {err}")],
                    };
                    let _ = msg_tx.send(WorkerMsg::InfoModal {
                        title: "Discovered Skills (/skills)".to_string(),
                        lines,
                    });
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::McpStatus => {
                    let summary = mcp.status_summary();
                    let msg = if summary.is_empty() {
                        "No MCP servers configured (~/.le-vibe/mcp.json or .le-vibe/mcp.json).".to_string()
                    } else {
                        summary
                            .into_iter()
                            .map(|(name, status)| format!("{name}: {status}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let _ = msg_tx.send(WorkerMsg::Notice(msg));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::RecordModelChange(spec) => {
                    let _ = session.append_model_change(spec);
                }
                WorkerCmd::RecordModeChange(mode) => {
                    let _ = session.append_mode_change(mode);
                }
                WorkerCmd::RunPrompt {
                    provider,
                    model,
                    thinking_level,
                    mode,
                    shell,
                    max_turns,
                    retry_max,
                    prompt,
                } => {
                    cancel.store(false, Ordering::SeqCst);
                    let skill_dirs = default_skill_dirs();
                    let skill_refs: Vec<&str> = skill_dirs.iter().map(String::as_str).collect();
                    let discovered_skills: Vec<(String, String, String)> = bridge
                        .list_skills(&skill_refs)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|s| (s.name, s.description, s.path))
                        .collect();
                    let mut sys = build_system_prompt(
                        mode,
                        &shell,
                        &cwd_string(),
                        &self_repo_path(),
                        &discovered_skills,
                        &session_memory_toc,
                    );
                    let model_key = format!("{provider}/{model}");
                    let cached_note = match &session_model_note {
                        Some((k, note)) if k == &model_key => note.clone(),
                        _ => {
                            let loaded = le_vibe_tools::memory::MemoryRagStore::open_default()
                                .read_by_key(&format!("model:{model_key}"));
                            session_model_note = Some((model_key.clone(), loaded.clone()));
                            loaded
                        }
                    };
                    if let Some(note) = &cached_note {
                        append_model_profile(&mut sys, &model_key, note);
                    }
                    if let Some(extra) = &system_prompt_extra
                        && !extra.trim().is_empty()
                    {
                        sys.push_str(&format!("\n\n<cli_instructions>\n{}\n</cli_instructions>", extra.trim()));
                    }
                    let cfg = AgentConfig {
                        provider: provider.clone(),
                        model,
                        system_prompt: Some(sys),
                        thinking_level,
                        max_turns,
                        max_retries: retry_max,
                        allowed_tools: mode.allowed_tools(),
                        mode_name: Some(mode.name().to_string()),
                        codemode: codemode_setting.clone(),
                        steering_queue: Some(Arc::clone(&steering_queue)),
                        output_store: Some(Arc::clone(&output_store)),
                    };
                    let mut stream_ms: u128 = 0;
                    let mut stream_start: Option<Instant> = None;
                    let mut last_delta_at: Option<Instant> = None;
                    let res = run_turn_loop_cancellable(
                        &mut bridge,
                        &mut session,
                        &telemetry,
                        &mut todos,
                        Some(&mut mcp),
                        Some(&cancel),
                        &cfg,
                        &prompt,
                        |ev| {
                            match &ev {
                                AgentEvent::TextDelta { .. } | AgentEvent::ThinkingDelta { .. } => {
                                    let now = Instant::now();
                                    stream_start.get_or_insert(now);
                                    last_delta_at = Some(now);
                                }
                                AgentEvent::ToolCallStart { .. } | AgentEvent::ToolCallEnd { .. } => {
                                    if let (Some(s), Some(e)) = (stream_start.take(), last_delta_at.take()) {
                                        stream_ms += e.duration_since(s).as_millis();
                                    }
                                }
                                _ => {}
                            }
                            let _ = msg_tx.send(WorkerMsg::Agent(ev));
                        },
                        |req| {
                            let detail = req
                                .command
                                .or(req.target_path)
                                .unwrap_or_else(|| "(no target)".to_string());
                            let _ = msg_tx.send(WorkerMsg::ApprovalNeeded {
                                tool: req.tool_name,
                                detail,
                                reason: req.reason,
                            });
                            approval_rx.recv().unwrap_or(ApprovalChoice::Deny)
                        },
                        |args| {
                            let items: Vec<QuestionItem> = args
                                .get("questions")
                                .and_then(Value::as_array)
                                .map(|arr| {
                                    arr.iter()
                                        .map(|q| {
                                            let header = q
                                                .get("header")
                                                .and_then(Value::as_str)
                                                .unwrap_or("Question")
                                                .to_string();
                                            let question = q
                                                .get("question")
                                                .and_then(Value::as_str)
                                                .unwrap_or("Please choose an option:")
                                                .to_string();
                                            let multi_select = q
                                                .get("multiSelect")
                                                .and_then(Value::as_bool)
                                                .unwrap_or(false);
                                            let options: Vec<QuestionOption> = q
                                                .get("options")
                                                .and_then(Value::as_array)
                                                .map(|opts| {
                                                    opts.iter()
                                                        .filter_map(|o| {
                                                            let label =
                                                                o.get("label")?.as_str()?.to_string();
                                                            let description = o
                                                                .get("description")
                                                                .and_then(Value::as_str)
                                                                .unwrap_or("")
                                                                .to_string();
                                                            let preview = o
                                                                .get("preview")
                                                                .and_then(Value::as_str)
                                                                .map(ToString::to_string);
                                                            Some(QuestionOption {
                                                                label,
                                                                description,
                                                                preview,
                                                            })
                                                        })
                                                        .collect()
                                                })
                                                .unwrap_or_default();
                                            let len = options.len();
                                            QuestionItem {
                                                header,
                                                question,
                                                multi_select,
                                                options,
                                                selected: vec![false; len],
                                                custom_text: String::new(),
                                                answer: None,
                                            }
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let session = if items.is_empty() {
                                QuestionSession::single(
                                    "Question".to_string(),
                                    "Please respond:".to_string(),
                                    Vec::new(),
                                )
                            } else {
                                QuestionSession {
                                    items,
                                    active_idx: 0,
                                }
                            };
                            let _ = msg_tx.send(WorkerMsg::QuestionNeeded(session));
                            question_rx
                                .recv()
                                .unwrap_or_else(|_| "User cancelled.".to_string())
                        },
                    );
                    if let (Some(s), Some(e)) = (stream_start.take(), last_delta_at.take()) {
                        stream_ms += e.duration_since(s).as_millis();
                    }
                    let tps = if stream_ms >= 250 {
                        Some(res.as_ref().map(|r| r.usage.output_tokens as f64).unwrap_or(0.0) / (stream_ms as f64 / 1000.0))
                    } else {
                        None
                    };
                    let todo_snapshot: Vec<(u64, String, String)> = todos
                        .items()
                        .iter()
                        .filter(|t| t.status != "deleted")
                        .map(|t| (t.id, t.subject.clone(), t.status.clone()))
                        .collect();
                    let _ = msg_tx.send(WorkerMsg::TodosUpdated(todo_snapshot));
                    match res {
                        Ok(reply) => {
                            logger.log(
                                "agent",
                                "turn_done",
                                Some(cmd_started.elapsed().as_millis() as u64),
                                format!(
                                    "{provider} in={} out={} stop={}",
                                    reply.usage.input_tokens,
                                    reply.usage.output_tokens,
                                    reply.stop_reason
                                ),
                            );
                            let _ = msg_tx.send(WorkerMsg::TurnDone {
                                provider,
                                input_tokens: reply.usage.input_tokens,
                                output_tokens: reply.usage.output_tokens,
                                cache_read_tokens: reply.usage.cache_read_tokens,
                                cost_usd: reply.usage.cost_usd,
                                tps,
                                git_badges: summarize_git_status(),
                            });
                        }
                        Err(le_vibe_agent::AgentError::Cancelled) => {
                            logger.log(
                                "agent",
                                "turn_cancelled",
                                Some(cmd_started.elapsed().as_millis() as u64),
                                "interrupted by Esc",
                            );
                            let _ = msg_tx.send(WorkerMsg::Notice("Interrupted by user.".to_string()));
                        }
                        Err(err) => {
                            logger.log(
                                "error",
                                "turn_error",
                                Some(cmd_started.elapsed().as_millis() as u64),
                                err.to_string(),
                            );
                            let _ = msg_tx.send(WorkerMsg::TurnError(err.to_string()));
                        }
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
            }
        }
    });

    (cmd_tx, msg_rx, approval_tx, question_tx)
}

pub(crate) fn mini_bar(remaining: f64) -> String {
    let filled = ((remaining.clamp(0.0, 1.0) * 6.0).round() as usize).min(6);
    format!("[{}{}]", "#".repeat(filled), "-".repeat(6 - filled))
}

pub(crate) fn agy_5h_remaining(agy: &Value, active_model: &str) -> Option<(f64, &'static str)> {
    let want_group = if active_model.starts_with("claude") || active_model.starts_with("gpt") {
        "claude"
    } else if active_model.contains("argon") {
        "argon"
    } else {
        "gemini"
    };

    if let Some(groups) = agy.get("groups").and_then(Value::as_array) {
        let mut fallback_5h: Option<f64> = None;
        for grp in groups {
            let label = grp
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            let is_target_group = label.contains(want_group);
            if let Some(buckets) = grp.get("buckets").and_then(Value::as_array) {
                for b in buckets {
                    let window = b.get("window").and_then(Value::as_str).unwrap_or("");
                    let b_label = b
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_lowercase();
                    let is_5h = window == "5h" || b_label.contains("five hour");
                    if let Some(rem) = b.get("remaining").and_then(Value::as_f64) {
                        if is_target_group && is_5h {
                            return Some((rem, "5h"));
                        }
                        if is_5h {
                            fallback_5h = Some(fallback_5h.map_or(rem, |cur| cur.min(rem)));
                        }
                    }
                }
            }
        }
        if let Some(rem) = fallback_5h {
            return Some((rem, "5h"));
        }
    }
    agy.get("quota_remaining")
        .and_then(Value::as_f64)
        .map(|rem| (rem, "7d"))
}

pub(crate) fn update_quota_badges(state: &mut TuiState, live: &Value) {
    if let Some(agy) = live.get("agy")
        && let Some((rem, _win)) = agy_5h_remaining(agy, &state.model)
    {
        state.provider_quota_badge.insert(
            "agy".to_string(),
            format!("agy {} {:.1}%", mini_bar(rem), rem * 100.0),
        );
    }
    if let Some(vibe) = live.get("vibe")
        && let Some(used) = vibe.get("budget_used_pct").and_then(Value::as_f64)
    {
        let rem = (1.0 - used / 100.0).clamp(0.0, 1.0);
        state.provider_quota_badge.insert(
            "vibe".to_string(),
            format!("vibe {} {:.1}%", mini_bar(rem), rem * 100.0),
        );
    }
    if let Some(ds) = live.get("deepseek")
        && let Some(b) = ds
            .get("balances")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
    {
        let cur = b.get("currency").and_then(Value::as_str).unwrap_or("USD");
        let tot = b.get("total").and_then(Value::as_str).unwrap_or("0");
        state
            .provider_quota_badge
            .insert("deepseek".to_string(), format!("deepseek {cur} {tot}"));
    }
}

