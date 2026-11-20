use le_vibe_agent::{
    AgentConfig, AgentEvent, ApprovalChoice, run_turn_loop_interactive, run_turn_loop_streaming,
};
use le_vibe_durable::SessionStore;
use le_vibe_pybridge::{PolicyPort, ProviderPort, PythonBridge};
use le_vibe_telemetry::TelemetryRecorder;
use le_vibe_tools::TodoStore;
use le_vibe_tui::{
    ALL_SHELLS, ALL_THEMES, ActiveModal, AgentMode, TerminalGuard, ThemeId, TranscriptBlock,
    TuiAction, TuiState, draw_frame, format_tokens, poll_tui_action, render_status_bar,
};
use serde_json::{Value, json};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn resolve_python_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LE_VIBE_PYTHON_DIR") {
        return PathBuf::from(dir);
    }
    let cwd_python = PathBuf::from("python");
    if cwd_python.exists() {
        return cwd_python;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("python"))
        .unwrap_or(cwd_python)
}

fn resolve_python_exe(python_dir: &Path) -> String {
    let venv_win = python_dir.join(".venv").join("Scripts").join("python.exe");
    if venv_win.exists() {
        return venv_win.to_string_lossy().to_string();
    }
    let venv_unix = python_dir.join(".venv").join("bin").join("python");
    if venv_unix.exists() {
        return venv_unix.to_string_lossy().to_string();
    }
    "python".to_string()
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

/// The model a provider starts on: whatever its live catalog recommends.
/// Nothing is hardcoded, so a provider that is not connected yields no model.
fn default_model_for(bridge: &mut PythonBridge, provider: &str) -> String {
    bridge.default_model(provider).unwrap_or_default()
}

fn provider_arg(args: &[String]) -> String {
    arg_value(args, "--provider").unwrap_or_else(|| "vibe".to_string())
}

fn prompt_line(label: &str) -> String {
    print!("{label}");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);
    buf.trim().to_string()
}

fn handle_login(bridge: &mut PythonBridge, args: &[String]) {
    let provider = arg_value(args, "--provider").unwrap_or_else(|| {
        let chosen = prompt_line("Provider (vibe / deepseek / agy) [vibe]: ");
        if chosen.is_empty() {
            "vibe".to_string()
        } else {
            chosen
        }
    });

    if provider == "vibe" && arg_value(args, "--api-key").is_none() {
        match bridge.login_vibe_oidc() {
            Ok(path) => {
                println!("Logged in to Mistral (Vibe) via OIDC. Saved to {path}");
                return;
            }
            Err(err) => {
                eprintln!("Mistral OIDC login failed: {err}");
                std::process::exit(1);
            }
        }
    }

    if provider == "agy" && arg_value(args, "--api-key").is_none() {
        match bridge.login_agy_oauth() {
            Ok(path) => {
                println!("Logged in to Antigravity (agy) via Google OAuth. Saved to {path}");
                return;
            }
            Err(err) => {
                eprintln!("Antigravity (agy) OAuth login failed: {err}");
                std::process::exit(1);
            }
        }
    }

    if provider == "deepseek" {
        println!("Get your DeepSeek API key from: https://platform.deepseek.com/api_keys");
    }

    let api_key = arg_value(args, "--api-key")
        .unwrap_or_else(|| prompt_line(&format!("Enter API key for {provider}: ")));

    match bridge.save_api_key(&provider, &api_key) {
        Ok(path) => println!("Saved {provider} API key to {path}"),
        Err(err) => {
            eprintln!("Login failed: {err}");
            std::process::exit(1);
        }
    }
}

fn run_streaming_prompt(
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let python_dir = resolve_python_dir();
    let python_exe = resolve_python_exe(&python_dir);
    let mut bridge = PythonBridge::spawn(&python_exe, &python_dir)
        .expect("failed to start le-vibe Python bridge");

    match args.first().map(String::as_str) {
        Some("login") => {
            handle_login(&mut bridge, &args[1..]);
            return;
        }
        Some("logout") => {
            let provider = arg_value(&args[1..], "--provider").unwrap_or_else(|| "vibe".to_string());
            match bridge.logout(&provider) {
                Ok(true) => println!("Removed saved credentials for {provider}"),
                Ok(false) => println!("No saved credentials found for {provider}"),
                Err(err) => {
                    eprintln!("Logout failed: {err}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some("auth") => {
            let status = bridge.auth_status().expect("auth status");
            println!("{}", serde_json::to_string_pretty(&status).unwrap());
            return;
        }
        Some("usage") => {
            let mut state = TuiState::new(
                provider_arg(&args[1..]),
                String::new(),
                format_cwd(),
                current_git_branch(),
            );
            refresh_state(&mut bridge, &mut state, false);
            let live = bridge.fetch_usage(None).unwrap_or(Value::Null);
            println!("{}", usage_report(&state, &live));
            return;
        }
        _ => {}
    }

    if args.iter().any(|a| a == "--list-models") {
        let models = bridge
            .refresh_models(None)
            .or_else(|_| bridge.list_models(None))
            .expect("list models");
        for m in models {
            println!("{}/{} ({})", m.provider, m.id, m.name);
        }
        return;
    }

    let mut provider = arg_value(&args, "--provider").unwrap_or_else(|| "vibe".to_string());
    let mut model = arg_value(&args, "--model").unwrap_or_else(|| default_model_for(&mut bridge, &provider));
    let mode = arg_value(&args, "--mode");
    let json_mode = mode.as_deref() == Some("json");
    let rpc_mode = mode.as_deref() == Some("rpc");
    let mut session = open_session(&args);
    let telemetry = TelemetryRecorder::new();

    if rpc_mode {
        run_rpc_mode(&mut bridge, &mut session, &telemetry, &mut provider, &mut model);
        return;
    }

    if let Some(prompt) = arg_value(&args, "--print") {
        if !json_mode {
            eprintln!("{}", render_status_bar(&provider, &model));
        }
        let cfg = AgentConfig {
            provider,
            model,
            system_prompt: Some("You are le-vibe, a concise coding assistant.".to_string()),
            thinking_level: "medium".to_string(),
            max_turns: 8,
            allowed_tools: None,
        };
        run_streaming_prompt(&mut bridge, &mut session, &telemetry, &cfg, &prompt, json_mode);
        return;
    }

    let yolo = args.iter().any(|a| a == "--yolo" || a == "--auto-approve");
    if yolo {
        let _ = bridge.set_yolo(true);
    }

    run_interactive_tui(bridge, session, telemetry, provider, model, yolo);
}

/// "45s" / "3m" / "2h" / "5d" for a catalog age in seconds.
fn humanize_age(seconds: f64) -> String {
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

fn current_git_branch() -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if b.is_empty() { None } else { Some(b) }
}

fn current_repo_name() -> String {
    if let Ok(out) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        && out.status.success()
    {
        let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Some(name) = Path::new(&root).file_name().and_then(|s| s.to_str())
            && !name.is_empty()
        {
            return name.to_string();
        }
    }
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(ToString::to_string))
        .unwrap_or_else(|| ".".to_string())
}

fn summarize_git_status() -> String {
    let Ok(out) = Command::new("git")
        .args(["status", "--porcelain=v2", "--branch", "--untracked-files=normal"])
        .output()
    else {
        return String::new();
    };
    if !out.status.success() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (mut staged, mut unstaged, mut untracked, mut conflicts, mut ahead, mut behind) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);

    for line in text.lines() {
        if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let mut parts = ab.split_whitespace();
            if let (Some(a), Some(b)) = (parts.next(), parts.next()) {
                ahead = a.trim_start_matches('+').parse().unwrap_or(0);
                behind = b.trim_start_matches('-').parse().unwrap_or(0);
            }
        } else if line.starts_with("? ") {
            untracked += 1;
        } else if line.starts_with("u ") {
            conflicts += 1;
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            let bytes = line.as_bytes();
            if bytes.len() >= 4 {
                if bytes[2] != b'.' {
                    staged += 1;
                }
                if bytes[3] != b'.' {
                    unstaged += 1;
                }
            }
        }
    }

    let mut badges = Vec::new();
    if conflicts > 0 {
        badges.push(format!("!{conflicts}"));
    }
    if staged > 0 {
        badges.push(format!("+{staged}"));
    }
    if unstaged > 0 {
        badges.push(format!("~{unstaged}"));
    }
    if untracked > 0 {
        badges.push(format!("?{untracked}"));
    }
    if ahead > 0 {
        badges.push(format!("↑{ahead}"));
    }
    if behind > 0 {
        badges.push(format!("↓{behind}"));
    }
    if badges.is_empty() {
        String::new()
    } else {
        format!("[{}]", badges.join(" "))
    }
}

fn format_cwd() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    if let Some(h) = home
        && let Ok(rel) = cwd.strip_prefix(&h)
    {
        let rel_str = rel.to_string_lossy();
        return if rel_str.is_empty() {
            "~".to_string()
        } else {
            format!("~\\{rel_str}")
        };
    }
    cwd.to_string_lossy().to_string()
}

fn refresh_state(bridge: &mut PythonBridge, state: &mut TuiState, live_network: bool) {
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
fn parse_iso_utc(text: &str) -> Option<i64> {
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
fn resets_in(reset_at: Option<&str>) -> String {
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
fn window_label(reset_at: Option<&str>) -> &'static str {
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
fn quota_bar(remaining: f64, width: usize) -> String {
    let filled = ((remaining.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    format!(
        "[{}{}]",
        "#".repeat(filled),
        "-".repeat(width.saturating_sub(filled))
    )
}

fn usage_report(state: &TuiState, live: &Value) -> String {
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

enum WorkerCmd {
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
        prompt: String,
    },
    ResolveModel {
        pattern: String,
        default_provider: String,
    },
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
    Compact,
    Tree,
    Skills,
    Shutdown,
}

enum WorkerMsg {
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
    QuestionNeeded {
        header: String,
        question: String,
        options: Vec<(String, String)>,
    },
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
    Idle,
}

struct SyncPayload {
    models: Vec<(String, String, String)>,
    default_thinking: Vec<(String, String, String)>,
    auth: Vec<(String, bool, String)>,
    resolved_model: Option<(String, u64)>,
}

fn collect_sync_payload(
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

fn spawn_tui_worker(
    mut bridge: PythonBridge,
    mut session: SessionStore,
    telemetry: TelemetryRecorder,
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
        let mut todos = TodoStore::new();
        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                WorkerCmd::Shutdown => break,
                WorkerCmd::SetYolo(enabled) => {
                    let _ = bridge.set_yolo(enabled);
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
                        let age = bridge
                            .catalog_state()
                            .ok()
                            .and_then(|(age, _)| age)
                            .map(humanize_age)
                            .unwrap_or_else(|| "just now".to_string());
                        let skill_count = bridge
                            .list_skills(&[".le-vibe/skills", ".pi/skills"])
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
                WorkerCmd::Compact => {
                    let count = session.active_branch().len();
                    let _ = session.append_compaction(format!("Compacted {count} prior messages."));
                    let _ = msg_tx.send(WorkerMsg::Notice(format!(
                        "Compacted session context ({count} prior entries)."
                    )));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::Tree => {
                    let mut out = Vec::new();
                    for entry in session.active_branch() {
                        out.push(format!(
                            "[{}] (parent={:?}, role={}): {}",
                            entry.id,
                            entry.parent_id,
                            entry.role,
                            entry.content.chars().take(80).collect::<String>()
                        ));
                    }
                    if out.is_empty() {
                        out.push("Session tree is empty.".to_string());
                    }
                    let _ = msg_tx.send(WorkerMsg::Notice(out.join("\n")));
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::Skills => {
                    if let Ok(skills) = bridge.list_skills(&[".le-vibe/skills", ".pi/skills"]) {
                        let msg = if skills.is_empty() {
                            "No SKILL.md files discovered.".to_string()
                        } else {
                            skills
                                .into_iter()
                                .map(|s| format!("{} — {} ({})", s.name, s.description, s.path))
                                .collect::<Vec<_>>()
                                .join("\n")
                        };
                        let _ = msg_tx.send(WorkerMsg::Notice(msg));
                    }
                    let _ = msg_tx.send(WorkerMsg::Idle);
                }
                WorkerCmd::RunPrompt {
                    provider,
                    model,
                    thinking_level,
                    mode,
                    shell,
                    prompt,
                } => {
                    let self_repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| ".".to_string());
                    let sys = format!(
                        "{}\nEnvironment: os={}, active_shell={shell}. Only write commands and tool calls compatible with `{shell}` on `{}`.\nSelf-reference: le-vibe source repository is at `{self_repo}` (Rust harness in `crates/*`, Python provider/policy/extension engine in `python/le_vibe/*`, docs in `goal.md`, `plan.md`, `README.md`, `change-log.md`). To self-modify & verify: run `cargo test --workspace` and `pytest python/tests`, and reinstall with `cargo install --path crates/le-vibe-cli`.",
                        mode.system_prompt(),
                        std::env::consts::OS,
                        std::env::consts::OS,
                    );
                    let cfg = AgentConfig {
                        provider: provider.clone(),
                        model,
                        system_prompt: Some(sys),
                        thinking_level,
                        max_turns: 8,
                        allowed_tools: mode.allowed_tools(),
                    };
                    let mut first_delta_at: Option<Instant> = None;
                    let mut last_delta_at: Option<Instant> = None;
                    let res = run_turn_loop_interactive(
                        &mut bridge,
                        &mut session,
                        &telemetry,
                        &mut todos,
                        &cfg,
                        &prompt,
                        |ev| {
                            if matches!(
                                ev,
                                AgentEvent::TextDelta { .. } | AgentEvent::ThinkingDelta { .. }
                            ) {
                                let now = Instant::now();
                                first_delta_at.get_or_insert(now);
                                last_delta_at = Some(now);
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
                            let q_arr = args.get("questions").and_then(Value::as_array);
                            let first = q_arr.and_then(|a| a.first());
                            let header = first
                                .and_then(|q| q.get("header"))
                                .and_then(Value::as_str)
                                .unwrap_or("Question")
                                .to_string();
                            let question = first
                                .and_then(|q| q.get("question"))
                                .and_then(Value::as_str)
                                .unwrap_or("Please choose an option:")
                                .to_string();
                            let options = first
                                .and_then(|q| q.get("options"))
                                .and_then(Value::as_array)
                                .map(|opts| {
                                    opts.iter()
                                        .filter_map(|o| {
                                            let l = o.get("label")?.as_str()?.to_string();
                                            let d = o
                                                .get("description")
                                                .and_then(Value::as_str)
                                                .unwrap_or("")
                                                .to_string();
                                            Some((l, d))
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let _ = msg_tx.send(WorkerMsg::QuestionNeeded {
                                header,
                                question,
                                options,
                            });
                            question_rx
                                .recv()
                                .unwrap_or_else(|_| "User cancelled.".to_string())
                        },
                    );
                    let tps = match (first_delta_at, last_delta_at) {
                        (Some(start), Some(end)) => {
                            let ms = end.duration_since(start).as_millis() as f64;
                            if ms >= 250.0 {
                                Some(res.as_ref().map(|r| r.usage.output_tokens as f64).unwrap_or(0.0) / (ms / 1000.0))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    match res {
                        Ok(reply) => {
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
                        Err(err) => {
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

fn mini_bar(remaining: f64) -> String {
    let filled = ((remaining.clamp(0.0, 1.0) * 6.0).round() as usize).min(6);
    format!("[{}{}]", "#".repeat(filled), "-".repeat(6 - filled))
}

fn agy_5h_remaining(agy: &Value, active_model: &str) -> Option<(f64, &'static str)> {
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

fn update_quota_badges(state: &mut TuiState, live: &Value) {
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

fn run_interactive_tui(
    bridge: PythonBridge,
    session: SessionStore,
    telemetry: TelemetryRecorder,
    initial_provider: String,
    initial_model: String,
    initial_yolo: bool,
) {
    let mut state = TuiState::new(
        initial_provider,
        initial_model,
        format_cwd(),
        current_git_branch(),
    );
    state.yolo = initial_yolo;
    state.repo_name = current_repo_name();
    state.git_badges = summarize_git_status();
    state.scoped_models = load_scope();
    if let Some(saved_shell) = load_setting("terminal") {
        state.shell = saved_shell;
    }
    if let Some(saved_theme) = load_setting("theme").and_then(|s| ThemeId::from_name(&s)) {
        state.theme = saved_theme;
    }
    state.status_text = Some("Syncing live model catalogs...".to_string());
    state.is_busy = true;

    let (cmd_tx, msg_rx, approval_tx, question_tx) =
        spawn_tui_worker(bridge, session, telemetry);
    let _ = cmd_tx.send(WorkerCmd::RefreshCatalog {
        live: true,
        announce: false,
        provider: state.provider.clone(),
        model: state.model.clone(),
    });
    let _ = cmd_tx.send(WorkerCmd::FetchUsage);

    let mut guard = TerminalGuard::enter().expect("failed to initialize TUI terminal");
    let mut last_catalog_check = Instant::now();
    let mut last_live_usage: Option<Value> = None;

    loop {
        while let Ok(msg) = msg_rx.try_recv() {
            match msg {
                WorkerMsg::CatalogSynced {
                    models,
                    default_thinking,
                    auth,
                    cwd,
                    branch,
                    resolved_model,
                    notice,
                } => {
                    state.cwd = cwd;
                    state.git_branch = branch;
                    state.repo_name = current_repo_name();
                    state.git_badges = summarize_git_status();
                    state.available_models = models;
                    for (p, id, dt) in default_thinking {
                        state.model_default_thinking.insert((p, id), dt);
                    }
                    for (p, authed, src) in auth {
                        state.provider_auth.insert(p, (authed, src));
                    }
                    if let Some((id, ctx)) = resolved_model {
                        state.model = id;
                        state.context_window = ctx;
                        state.apply_default_thinking();
                    }
                    if state.available_models.is_empty() {
                        state.blocks.push(TranscriptBlock::System(
                            "No models available. Run /login to connect vibe, deepseek, or agy."
                                .to_string(),
                        ));
                    }
                    if let Some(n) = notice {
                        state.blocks.push(TranscriptBlock::System(n));
                    }
                }
                WorkerMsg::UsageLoaded(live) => {
                    update_quota_badges(&mut state, &live);
                    let report = usage_report(&state, &live);
                    last_live_usage = Some(live);
                    state.usage_lines = report.lines().map(ToString::to_string).collect();
                    state.usage_loading = false;
                }
                WorkerMsg::ModelResolved {
                    provider,
                    model,
                    thinking_level,
                    context_window,
                } => {
                    state.provider = provider;
                    state.model = model;
                    state.thinking_level = thinking_level;
                    state.context_window = context_window;
                    if let Some(live) = &last_live_usage {
                        update_quota_badges(&mut state, live);
                    }
                    state.blocks.push(TranscriptBlock::System(format!(
                        "Switched model to ({}) {} • {}",
                        state.provider, state.model, state.thinking_level
                    )));
                }
                WorkerMsg::ProviderSwitched {
                    provider,
                    model,
                    thinking_level,
                    context_window,
                } => {
                    state.provider = provider;
                    state.model = model;
                    state.thinking_level = thinking_level;
                    state.context_window = context_window;
                    state.blocks.push(TranscriptBlock::System(format!(
                        "Switched provider to ({}) {} • {}",
                        state.provider, state.model, state.thinking_level
                    )));
                }
                WorkerMsg::Agent(ev) => match ev {
                    AgentEvent::ThinkingDelta { delta } => {
                        if let Some(TranscriptBlock::Thinking(buf)) = state.blocks.last_mut() {
                            buf.push_str(&delta);
                        } else {
                            state.blocks.push(TranscriptBlock::Thinking(delta));
                        }
                    }
                    AgentEvent::TextDelta { delta } => {
                        if let Some(TranscriptBlock::Assistant(buf)) = state.blocks.last_mut() {
                            buf.push_str(&delta);
                        } else {
                            state.blocks.push(TranscriptBlock::Assistant(delta));
                        }
                    }
                    AgentEvent::ToolCallStart { name, .. } => {
                        state.status_text = Some(format!("Running tool {name}..."));
                        state.blocks.push(TranscriptBlock::Tool {
                            name,
                            status: "running...".to_string(),
                            output: String::new(),
                        });
                    }
                    AgentEvent::ToolCallEnd {
                        name,
                        is_error,
                        output,
                        ..
                    } => {
                        let label = if is_error { "failed" } else { "done" };
                        let preview: String = output.lines().take(8).collect::<Vec<_>>().join("\n");
                        if let Some(TranscriptBlock::Tool {
                            name: n,
                            status: st,
                            output: out,
                        }) = state.blocks.last_mut()
                            && *n == name
                        {
                            *st = label.to_string();
                            *out = preview;
                        }
                    }
                    _ => {}
                },
                WorkerMsg::ApprovalNeeded {
                    tool,
                    detail,
                    reason,
                } => {
                    state.approval_prompt = Some((tool, detail, reason));
                    state.modal = ActiveModal::ApprovalModal;
                    state.modal_idx = 0;
                }
                WorkerMsg::QuestionNeeded {
                    header,
                    question,
                    options,
                } => {
                    state.question_prompt = Some((header, question, options));
                    state.question_custom.clear();
                    state.modal = ActiveModal::QuestionModal;
                    state.modal_idx = 0;
                }
                WorkerMsg::ShellResult {
                    command,
                    output,
                    in_context,
                    git_badges,
                } => {
                    state.git_badges = git_badges;
                    let preview: String = output.lines().take(20).collect::<Vec<_>>().join("\n");
                    state.blocks.push(TranscriptBlock::Script {
                        command,
                        output: preview,
                        in_context,
                    });
                },
                WorkerMsg::TurnDone {
                    provider,
                    input_tokens,
                    output_tokens,
                    cache_read_tokens,
                    cost_usd,
                    tps,
                    git_badges,
                } => {
                    if let Some(started) = state.active_started.take() {
                        state.active_ms = state
                            .active_ms
                            .saturating_add(started.elapsed().as_millis() as u64);
                    }
                    state.last_tps = tps;
                    state.git_badges = git_badges;
                    state.record_usage(
                        &provider,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cost_usd,
                    );
                }
                WorkerMsg::TurnError(err) => {
                    if let Some(started) = state.active_started.take() {
                        state.active_ms = state
                            .active_ms
                            .saturating_add(started.elapsed().as_millis() as u64);
                    }
                    state
                        .blocks
                        .push(TranscriptBlock::System(format!("Error: {err}")));
                }
                WorkerMsg::Notice(text) => {
                    state.blocks.push(TranscriptBlock::System(text));
                }
                WorkerMsg::Idle => {
                    state.is_busy = false;
                    state.status_text = None;
                }
            }
        }

        if state.is_busy || state.usage_loading {
            state.advance_spinner();
        }

        if last_catalog_check.elapsed() >= Duration::from_secs(60) && !state.is_busy {
            last_catalog_check = Instant::now();
            let _ = cmd_tx.send(WorkerCmd::CheckStaleAndRefresh {
                provider: state.provider.clone(),
                model: state.model.clone(),
            });
        }

        guard.terminal.draw(|f| draw_frame(f, &state)).unwrap();

        match poll_tui_action(&mut state, Duration::from_millis(33)).unwrap_or(TuiAction::None) {
            TuiAction::None => {}
            TuiAction::Quit => {
                if state.scope_dirty {
                    save_scope(&state.scoped_models);
                }
                let _ = cmd_tx.send(WorkerCmd::Shutdown);
                break;
            }
            TuiAction::CycleModel => {
                let pool = state.cycle_pool();
                if !pool.is_empty() {
                    let cur = pool
                        .iter()
                        .position(|(p, id)| *p == state.provider && *id == state.model)
                        .unwrap_or(0);
                    let (provider, model) = pool[(cur + 1) % pool.len()].clone();
                    state.provider = provider;
                    state.model = model;
                    state.apply_default_thinking();
                    if let Some(live) = &last_live_usage {
                        update_quota_badges(&mut state, live);
                    }
                }
            }
            TuiAction::ApprovalDecision(idx) => {
                let choice = match idx {
                    0 => ApprovalChoice::AllowOnce,
                    1 => ApprovalChoice::AllowSession,
                    2 => {
                        state.yolo = true;
                        ApprovalChoice::AllowYolo
                    }
                    _ => ApprovalChoice::Deny,
                };
                let _ = approval_tx.send(choice);
            }
            TuiAction::QuestionAnswer(answer) => {
                let _ = question_tx.send(answer);
            }
            TuiAction::SaveScope => {
                save_scope(&state.scoped_models);
                state.scope_dirty = false;
                state.modal = ActiveModal::None;
                state.blocks.push(TranscriptBlock::System(format!(
                    "Saved {} scoped model(s) for Ctrl+P cycling.",
                    state.scoped_models.len()
                )));
            }
            TuiAction::ConfigCycle(idx) => match idx {
                0 => {
                    let cur = ALL_THEMES
                        .iter()
                        .position(|t| *t == state.theme)
                        .unwrap_or(0);
                    state.theme = ALL_THEMES[(cur + 1) % ALL_THEMES.len()];
                    save_setting("theme", state.theme.name());
                }
                1 => {
                    let shells = ["powershell", "pwsh", "bash", "cmd", "zsh", "sh"];
                    let cur = shells
                        .iter()
                        .position(|s| *s == state.shell)
                        .unwrap_or(0);
                    state.shell = shells[(cur + 1) % shells.len()].to_string();
                    save_setting("terminal", &state.shell);
                }
                2 => {
                    state.mode = state.mode.next();
                }
                3 => {
                    state.yolo = !state.yolo;
                    let _ = cmd_tx.send(WorkerCmd::SetYolo(state.yolo));
                }
                4 => {
                    let levels = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
                    let cur = levels
                        .iter()
                        .position(|l| *l == state.thinking_level)
                        .unwrap_or(3);
                    state.thinking_level = levels[(cur + 1) % levels.len()].to_string();
                }
                5 => {
                    state.reduced_motion = !state.reduced_motion;
                }
                _ => {}
            },
            TuiAction::Submit(input) => {
                if input == "/exit" || input == "/quit" {
                    if state.scope_dirty {
                        save_scope(&state.scoped_models);
                    }
                    let _ = cmd_tx.send(WorkerCmd::Shutdown);
                    break;
                }
                if input == "/clear" {
                    state.blocks.clear();
                    continue;
                }
                if input == "/usage" {
                    state.modal = ActiveModal::UsageModal;
                    state.usage_loading = true;
                    state.usage_scroll = 0;
                    let _ = cmd_tx.send(WorkerCmd::FetchUsage);
                    continue;
                }
                if input == "/reload" {
                    state.is_busy = true;
                    state.status_text = Some("Pulling live model catalogs...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::RefreshCatalog {
                        live: true,
                        announce: true,
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                    });
                    continue;
                }
                if input == "/yolo" {
                    state.yolo = !state.yolo;
                    let _ = cmd_tx.send(WorkerCmd::SetYolo(state.yolo));
                    state.blocks.push(TranscriptBlock::System(if state.yolo {
                        "YOLO mode enabled (all non-blocked tools auto-approved).".to_string()
                    } else {
                        "YOLO mode disabled (ask mode restored).".to_string()
                    }));
                    continue;
                }
                if input == "/mode" {
                    state.modal = ActiveModal::ModePicker;
                    state.modal_idx = match state.mode {
                        AgentMode::Normal => 0,
                        AgentMode::Plan => 1,
                        AgentMode::Execute => 2,
                        AgentMode::Ask => 3,
                        AgentMode::Rea => 4,
                    };
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/mode ") {
                    if let Some(m) = AgentMode::from_name(arg) {
                        state.mode = m;
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Switched mode to [{}] — {}",
                            m.name(),
                            m.description()
                        )));
                    } else {
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Unknown mode {:?}. Choose: normal, plan, execute, ask, rea.",
                            arg.trim()
                        )));
                    }
                    continue;
                }
                if input == "/config" {
                    state.modal = ActiveModal::ConfigModal;
                    state.modal_idx = 0;
                    continue;
                }
                if input == "/term" {
                    state.modal = ActiveModal::TermPicker;
                    state.modal_idx = ALL_SHELLS
                        .iter()
                        .position(|(name, _)| *name == state.shell)
                        .unwrap_or(0);
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/term ") {
                    let next_shell = arg.trim().to_string();
                    if !next_shell.is_empty() {
                        state.shell = next_shell.clone();
                        save_setting("terminal", &next_shell);
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Terminal shell set to `{next_shell}` (saved to ~/.le-vibe/settings.json)."
                        )));
                    }
                    continue;
                }
                if input == "/theme" {
                    state.modal = ActiveModal::ThemePicker;
                    state.modal_idx = 0;
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/theme ") {
                    if let Some(t) = ThemeId::from_name(arg) {
                        state.theme = t;
                        save_setting("theme", t.name());
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Switched theme to {} ({})",
                            t.name(),
                            t.description()
                        )));
                    } else {
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Unknown theme {:?}. Use /theme to pick from modal.",
                            arg.trim()
                        )));
                    }
                    continue;
                }
                if input == "/compact" {
                    let _ = cmd_tx.send(WorkerCmd::Compact);
                    continue;
                }
                if input == "/tree" {
                    let _ = cmd_tx.send(WorkerCmd::Tree);
                    continue;
                }
                if input == "/skills" {
                    let _ = cmd_tx.send(WorkerCmd::Skills);
                    continue;
                }
                if input == "/scoped-models" {
                    state.scope_view = true;
                    state.modal = ActiveModal::ModelPicker;
                    state.modal_idx = 0;
                    continue;
                }
                if input == "/model" {
                    state.scope_view = true;
                    state.modal = ActiveModal::ModelPicker;
                    state.modal_idx = state
                        .picker_rows()
                        .iter()
                        .position(|(p, id, _)| *p == state.provider && *id == state.model)
                        .unwrap_or(0);
                    let _ = cmd_tx.send(WorkerCmd::RefreshCatalog {
                        live: true,
                        announce: false,
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                    });
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/model ") {
                    state.is_busy = true;
                    state.status_text = Some("Resolving model...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::ResolveModel {
                        pattern: arg.trim().to_string(),
                        default_provider: state.provider.clone(),
                    });
                    continue;
                }
                if input == "/provider" {
                    state.modal = ActiveModal::ProviderPicker;
                    state.modal_idx = match state.provider.as_str() {
                        "deepseek" => 1,
                        "agy" => 2,
                        _ => 0,
                    };
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/provider ") {
                    state.is_busy = true;
                    state.status_text = Some("Switching provider...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::SwitchProvider {
                        provider: arg.trim().to_string(),
                    });
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/thinking ") {
                    state.thinking_level = arg.trim().to_string();
                    state.blocks.push(TranscriptBlock::System(format!(
                        "Set thinking level to {}",
                        state.thinking_level
                    )));
                    continue;
                }
                if input == "/login" || input == "/auth" {
                    state.modal = ActiveModal::LoginPicker;
                    state.modal_idx = 0;
                    continue;
                }
                if let Some(arg) = input
                    .strip_prefix("/login ")
                    .or_else(|| input.strip_prefix("/auth "))
                {
                    let mut parts = arg.split_whitespace();
                    let target = parts.next().unwrap_or(&state.provider).to_string();
                    let key_rest: String = parts.collect::<Vec<_>>().join(" ");
                    state.is_busy = true;
                    state.status_text = Some(format!("Waiting for {target} login..."));
                    let _ = cmd_tx.send(WorkerCmd::Login {
                        target,
                        api_key: if key_rest.is_empty() {
                            None
                        } else {
                            Some(key_rest)
                        },
                    });
                    continue;
                }
                if input == "/logout" {
                    state.modal = ActiveModal::LogoutPicker;
                    state.modal_idx = 0;
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/logout ") {
                    state.is_busy = true;
                    state.status_text = Some("Removing credentials...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::Logout {
                        target: arg.trim().to_string(),
                    });
                    continue;
                }

                if let Some(cmd) = input.strip_prefix("!!") {
                    let cmd = cmd.trim().to_string();
                    if !cmd.is_empty() {
                        state.is_busy = true;
                        state.status_text = Some(format!("Running ! {cmd} (no context)..."));
                        let _ = cmd_tx.send(WorkerCmd::RunShell {
                            command: cmd,
                            shell: state.shell.clone(),
                            include_in_context: false,
                        });
                    }
                    continue;
                }
                if let Some(cmd) = input.strip_prefix('!') {
                    let cmd = cmd.trim().to_string();
                    if !cmd.is_empty() {
                        state.is_busy = true;
                        state.status_text = Some(format!("Running ! {cmd}..."));
                        let _ = cmd_tx.send(WorkerCmd::RunShell {
                            command: cmd,
                            shell: state.shell.clone(),
                            include_in_context: true,
                        });
                    }
                    continue;
                }

                state.blocks.push(TranscriptBlock::User(input.clone()));
                state.is_busy = true;
                state.active_started = Some(Instant::now());
                state.status_text = Some(format!("Working ({}/{})...", state.provider, state.model));
                let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                    provider: state.provider.clone(),
                    model: state.model.clone(),
                    thinking_level: state.thinking_level.clone(),
                    mode: state.mode,
                    shell: state.shell.clone(),
                    prompt: input,
                });
            }
        }
    }
}

fn settings_file() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".le-vibe").join("settings.json")
}

fn load_setting(key: &str) -> Option<String> {
    let raw = fs::read_to_string(settings_file()).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    val.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn save_setting(key: &str, value: &str) {
    let path = settings_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut obj = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Map<String, Value>>(&s).ok())
        .unwrap_or_default();
    obj.insert(key.to_string(), json!(value));
    let _ = fs::write(
        path,
        serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default() + "\n",
    );
}

fn run_shell_command(shell: &str, command: &str) -> io::Result<std::process::Output> {
    let lower = shell.to_lowercase();
    if lower == "powershell" || lower == "pwsh" || lower.ends_with("powershell.exe") || lower.ends_with("pwsh.exe") {
        Command::new(shell)
            .args(["-NoProfile", "-Command", command])
            .output()
    } else if lower == "cmd" || lower.ends_with("cmd.exe") {
        Command::new(shell).args(["/C", command]).output()
    } else {
        Command::new(shell).args(["-c", command]).output()
    }
}

fn scope_file() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".le-vibe").join("scoped-models.json")
}

fn load_scope() -> Vec<(String, String)> {
    let Ok(raw) = fs::read_to_string(scope_file()) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    value
        .get("models")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let provider = row.get("provider")?.as_str()?.to_string();
                    let id = row.get("id")?.as_str()?.to_string();
                    Some((provider, id))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn save_scope(scope: &[(String, String)]) {
    let path = scope_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let rows: Vec<Value> = scope
        .iter()
        .map(|(provider, id)| json!({ "provider": provider, "id": id }))
        .collect();
    let _ = fs::write(
        path,
        serde_json::to_string_pretty(&json!({ "models": rows })).unwrap_or_default() + "
",
    );
}

fn sessions_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".le-vibe").join("sessions")
}

fn open_session(args: &[String]) -> SessionStore {
    if let Some(path) = arg_value(args, "--session") {
        return SessionStore::open(path).expect("failed to open session file");
    }
    let dir = sessions_dir();
    if let Some(id) = arg_value(args, "--resume") {
        let p = dir.join(format!("{id}.jsonl"));
        return SessionStore::open(p).expect("failed to resume session");
    }
    if args.iter().any(|a| a == "--continue")
        && let Ok(entries) = fs::read_dir(&dir)
    {
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
            .collect();
        files.sort();
        if let Some(latest) = files.pop() {
            return SessionStore::open(latest).expect("failed to continue session");
        }
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let p = dir.join(format!("session-{ts}.jsonl"));
    SessionStore::open(p).unwrap_or_else(|_| SessionStore::in_memory())
}

fn run_rpc_mode(
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
                let cfg = AgentConfig {
                    provider: provider.clone(),
                    model: model.clone(),
                    system_prompt: Some("You are le-vibe, a concise coding assistant.".to_string()),
                    thinking_level: "medium".to_string(),
                    max_turns: 8,
                    allowed_tools: None,
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

#[cfg(test)]
mod tests {
    use super::{parse_iso_utc, quota_bar, resets_in};

    #[test]
    fn parses_iso_utc_timestamps() {
        // 2026-12-01T00:00:00Z
        assert_eq!(parse_iso_utc("2026-12-01T00:00:00Z"), Some(1_796_083_200));
        assert_eq!(parse_iso_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_utc("not-a-date"), None);
    }

    #[test]
    fn draws_a_bar_proportional_to_what_is_left() {
        assert_eq!(quota_bar(1.0, 8), "[########]");
        assert_eq!(quota_bar(0.0, 8), "[--------]");
        assert_eq!(quota_bar(0.5, 8), "[####----]");
    }

    #[test]
    fn counts_down_to_a_reset() {
        assert_eq!(resets_in(None), "");
        // A time in the past reads as due now rather than a negative countdown.
        assert_eq!(resets_in(Some("1970-01-01T00:00:00Z")), "resets now");
    }
}
