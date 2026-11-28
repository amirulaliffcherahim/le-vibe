mod cli;
mod debug_log;
mod features;
mod learn;
mod rpc;
mod settings;
#[cfg(feature = "dev-debug")]
mod tui_probe;
mod update;
mod worker;

use le_vibe_agent::{AgentConfig, AgentEvent, ApprovalChoice};
use le_vibe_durable::SessionStore;
use le_vibe_pybridge::{PolicyPort, ProviderPort, PythonBridge};
use le_vibe_telemetry::TelemetryRecorder;
use le_vibe_tui::{
    ALL_SHELLS, ALL_THEMES, ActiveModal, AgentMode, CompactionReason, TerminalGuard, ThemeId,
    ToolCard, TranscriptBlock, TuiAction, TuiState, build_system_prompt, draw_frame,
    export_session_html, export_session_markdown, extract_last_code_block, extract_latest_xml_block,
    format_chat_html, format_chat_plain, format_tokens, poll_tui_action, project_xml_diagram,
    render_status_bar,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::cli::{CliAction, build_initial_prompt, help_text, parse_cli_args};
use crate::debug_log::{DEBUG_ENABLED, EventLogger};
use crate::features::{
    LoopScheduler, PastedItem, build_optimization_digest, classify_pasted_text,
    detect_project_resources, expand_prompt_template, format_hotkeys_lines, grab_clipboard_item,
    load_prompt_templates, lookup_project_trust, parse_loop_spec, run_external_editor,
    save_project_trust, search_past_sessions,
};
use crate::rpc::{run_rpc_mode, run_streaming_prompt};
use crate::settings::{
    arg_value, current_git_branch, current_repo_name, cwd_string, default_mcp_paths,
    default_model_for, format_cwd, load_history, load_layered_settings, load_scope,
    open_session_with_opts, pinned_tools, project_settings_file, resolve_initial_model_offline,
    resolve_python_dir, resolve_python_exe, save_history, save_scope, save_setting,
    save_setting_for_scope, scan_workspace_files, self_repo_path, sessions_dir, settings_file,
    summarize_git_status, trust_cwd_in_policy,
};
use crate::worker::{WorkerCmd, WorkerMsg, spawn_tui_worker, update_quota_badges, usage_report};

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


fn main() {
    if let Ok(exe) = std::env::current_exe() {
        update::cleanup_stale_old_binaries(&exe);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = parse_cli_args(&args);

    match action {
        CliAction::Help => {
            println!("{}", help_text());
            return;
        }
        CliAction::Version => {
            println!("le-vibe {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        CliAction::Subcommand(ref sub, ref sub_args) if sub == "config" => {
            match sub_args.first().map(String::as_str) {
                Some("path") => {
                    println!("user:    {}", settings_file().display());
                    println!("project: {}", project_settings_file().display());
                }
                Some("set") if sub_args.len() >= 3 => {
                    let key = &sub_args[1];
                    let val = &sub_args[2];
                    let scope = if sub_args.iter().any(|a| a == "--project") { 1 } else { 0 };
                    let label = save_setting_for_scope(scope, key, val);
                    println!("Saved {key} = {val} ({label})");
                }
                _ => {
                    let (vals, origins) = load_layered_settings();
                    println!("user:    {}", settings_file().display());
                    println!("project: {}", project_settings_file().display());
                    if vals.is_empty() {
                        println!("(no overrides set; using defaults)");
                    } else {
                        for (k, v) in vals {
                            let orig = origins.get(&k).map(String::as_str).unwrap_or("default");
                            println!("  {k} = {v} [{orig}]");
                        }
                    }
                }
            }
            return;
        }
        CliAction::Subcommand(ref sub, ref sub_args)
            if matches!(
                sub.as_str(),
                "usage" | "install" | "remove" | "list" | "update" | "doctor"
            ) =>
        {
            if sub == "usage" {
                let target = if let Some(arg) = sub_args.first() {
                    let p = std::path::PathBuf::from(arg);
                    if p.is_file() {
                        Some(p)
                    } else {
                        Some(le_vibe_telemetry::usage::default_usage_log_path(arg))
                    }
                } else {
                    le_vibe_telemetry::usage::latest_usage_log_path()
                };
                match target {
                    Some(path) if path.is_file() => {
                        match le_vibe_telemetry::usage::read_usage_log(&path) {
                            Ok(recs) => println!(
                                "{}",
                                le_vibe_telemetry::usage::format_usage_report(&recs)
                            ),
                            Err(err) => eprintln!("Failed to read {}: {err}", path.display()),
                        }
                    }
                    Some(path) => eprintln!("Usage log not found: {}", path.display()),
                    None => println!("No forensic usage logs found in ~/.le-vibe/logs/."),
                }
                return;
            }
            let home = std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let ext_dir = home.join(".le-vibe").join("extensions");
            match sub.as_str() {
                "list" => {
                    let mut found = Vec::new();
                    for dir in [&ext_dir, &std::path::PathBuf::from(".le-vibe").join("extensions")] {
                        if let Ok(entries) = fs::read_dir(dir) {
                            for e in entries.flatten() {
                                let p = e.path();
                                if p.extension().and_then(|s| s.to_str()) == Some("py") {
                                    found.push(p.display().to_string());
                                }
                            }
                        }
                    }
                    if found.is_empty() {
                        println!("No extensions installed in {}", ext_dir.display());
                    } else {
                        for f in found {
                            println!("  • {f}");
                        }
                    }
                }
                "install" => {
                    let Some(src) = sub_args.first() else {
                        eprintln!("Usage: le-vibe install <path/to/ext.py | git-url>");
                        std::process::exit(1);
                    };
                    let _ = fs::create_dir_all(&ext_dir);
                    let src_path = std::path::Path::new(src);
                    if src_path.is_file() {
                        let name = src_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("extension.py");
                        let dst = ext_dir.join(name);
                        if let Err(err) = fs::copy(src_path, &dst) {
                            eprintln!("Install failed: {err}");
                            std::process::exit(1);
                        }
                        println!("Installed extension to {}", dst.display());
                    } else if src.starts_with("http://")
                        || src.starts_with("https://")
                        || src.starts_with("git@")
                    {
                        let status = std::process::Command::new("git")
                            .args(["clone", "--depth", "1", src])
                            .current_dir(&ext_dir)
                            .status();
                        if !status.is_ok_and(|s| s.success()) {
                            eprintln!("git clone failed for {src}");
                            std::process::exit(1);
                        }
                        println!("Cloned extension repository into {}", ext_dir.display());
                    } else {
                        eprintln!("Source not found: {src}");
                        std::process::exit(1);
                    }
                }
                "remove" => {
                    let Some(name) = sub_args.first() else {
                        eprintln!("Usage: le-vibe remove <extension-name>");
                        std::process::exit(1);
                    };
                    let candidates = [
                        ext_dir.join(name),
                        ext_dir.join(format!("{name}.py")),
                    ];
                    let mut removed = false;
                    for p in candidates {
                        if (p.is_file() && fs::remove_file(&p).is_ok())
                            || (p.is_dir() && fs::remove_dir_all(&p).is_ok())
                        {
                            println!("Removed {}", p.display());
                            removed = true;
                        }
                    }
                    if !removed {
                        eprintln!("Extension not found: {name}");
                        std::process::exit(1);
                    }
                }
                "doctor" => {
                    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                    println!("le-vibe doctor (v{})", env!("CARGO_PKG_VERSION"));
                    println!("  python_dir : {}", resolve_python_dir().display());
                    println!("  python_exe : {}", resolve_python_exe(&resolve_python_dir()));
                    println!("  settings   : {} (exists={})", settings_file().display(), settings_file().is_file());
                    let proj_res = detect_project_resources(&cwd);
                    let trust = lookup_project_trust(&cwd);
                    println!(
                        "  trust      : {:?} ({} protected project resource(s))",
                        trust,
                        proj_res.len()
                    );
                    let mcp_paths = default_mcp_paths();
                    let mcp = le_vibe_mcp::McpManager::from_config_files(&mcp_paths);
                    println!("  mcp        : {} server(s) configured", mcp.status_summary().len());
                    let tpls = load_prompt_templates(&cwd, trust.unwrap_or(true));
                    println!("  prompts    : {} template(s) loaded", tpls.len());
                    let py_dir = resolve_python_dir();
                    let py_exe = resolve_python_exe(&py_dir);
                    if let Ok(mut bridge) = PythonBridge::spawn(&py_exe, &py_dir)
                        && let Ok(auth) = bridge.auth_status()
                    {
                        println!("  auth       : {}", serde_json::to_string(&auth).unwrap_or_default());
                    }
                    return;
                }
                "update" => {
                    update::run_update(sub_args);
                }
                _ => {}
            }
            return;
        }
        CliAction::Subcommand(ref sub, ref sub_args) if sub == "mcp" => {
            if sub_args.first().map(String::as_str) == Some("list") || sub_args.is_empty() {
                let mgr = le_vibe_mcp::McpManager::from_config_files(&default_mcp_paths());
                let summary = mgr.status_summary();
                if summary.is_empty() {
                    println!("No MCP servers configured (~/.le-vibe/mcp.json or .le-vibe/mcp.json).");
                } else {
                    for (name, status) in summary {
                        println!("{name}: {status}");
                    }
                }
            } else {
                eprintln!("Usage: le-vibe mcp list");
                std::process::exit(1);
            }
            return;
        }
        #[cfg(feature = "dev-debug")]
        CliAction::Subcommand(ref sub, ref sub_args) if sub == "tui-probe" => {
            std::process::exit(tui_probe::run(sub_args));
        }
        CliAction::ExportOffline { input, output } => {
            let session = SessionStore::open(&input).expect("failed to open session file");
            let mut state = TuiState::new(
                "session".into(),
                "export".into(),
                format_cwd(),
                current_git_branch(),
            );
            for entry in session.active_branch() {
                match entry.role.as_str() {
                    "user" => state.blocks.push(TranscriptBlock::User(entry.content)),
                    "assistant" => state.blocks.push(TranscriptBlock::Assistant(entry.content)),
                    _ => state.blocks.push(TranscriptBlock::System(entry.content)),
                }
            }
            let out_path = output.unwrap_or_else(|| format!("{input}.html"));
            let content = if out_path.ends_with(".md") {
                export_session_markdown(&state)
            } else {
                export_session_html(&state)
            };
            fs::write(&out_path, content).expect("failed to write export");
            println!("Exported {out_path}");
            return;
        }
        _ => {}
    }

    // Propagate --no-extensions and -e/--extension to the Python bridge via process env
    if let CliAction::Run(ref opts) = action {
        if opts.no_extensions {
            // SAFETY: single-threaded startup before any worker threads are spawned.
            unsafe {
                std::env::set_var("LE_VIBE_NO_EXTENSIONS", "1");
            }
        }
        if !opts.extra_extensions.is_empty() {
            let sep = if cfg!(windows) { ";" } else { ":" };
            let joined = opts.extra_extensions.join(sep);
            // SAFETY: single-threaded startup before any worker threads are spawned.
            unsafe {
                std::env::set_var("LE_VIBE_EXTRA_EXTENSIONS", joined);
            }
        }
    }

    let python_dir = resolve_python_dir();
    let python_exe = resolve_python_exe(&python_dir);

    match action {
        CliAction::Subcommand(sub, sub_args) => {
            let mut bridge = PythonBridge::spawn(&python_exe, &python_dir)
                .expect("failed to start le-vibe Python bridge");
            match sub.as_str() {
                "login" => handle_login(&mut bridge, &sub_args),
                "auth" => {
                    let status = bridge.auth_status().expect("auth status");
                    println!("{}", serde_json::to_string_pretty(&status).unwrap());
                }
                _ => {}
            }
        }
        CliAction::ListModels(query) => {
            let mut bridge = PythonBridge::spawn(&python_exe, &python_dir)
                .expect("failed to start le-vibe Python bridge");
            let models = bridge
                .refresh_models(None)
                .or_else(|_| bridge.list_models(None))
                .expect("list models");
            let q = query.as_deref().unwrap_or("").to_lowercase();
            for m in models {
                if q.is_empty()
                    || m.id.to_lowercase().contains(&q)
                    || m.name.to_lowercase().contains(&q)
                    || m.provider.to_lowercase().contains(&q)
                {
                    println!("{}/{} ({})", m.provider, m.id, m.name);
                }
            }
        }
        CliAction::Run(opts) => {
            let json_mode = opts.mode.as_deref() == Some("json");
            let rpc_mode = opts.mode.as_deref() == Some("rpc");
            let non_tty = !std::io::IsTerminal::is_terminal(&std::io::stdout());
            let initial_prompt = build_initial_prompt(&opts);
            let is_non_interactive = rpc_mode
                || ((opts.print_mode || json_mode || non_tty)
                    && (opts.print_mode || json_mode || initial_prompt.is_some()));

            let mut session = open_session_with_opts(&opts);
            let telemetry = TelemetryRecorder::new();

            if is_non_interactive {
                let mut bridge = PythonBridge::spawn(&python_exe, &python_dir)
                    .expect("failed to start le-vibe Python bridge");
                let mut provider = opts.provider.clone().unwrap_or_else(|| "vibe".to_string());
                let mut model = opts
                    .model
                    .clone()
                    .unwrap_or_else(|| default_model_for(&mut bridge, &provider));
                if opts.yolo {
                    let _ = bridge.set_yolo(true);
                }

                if rpc_mode {
                    run_rpc_mode(&mut bridge, &mut session, &telemetry, &mut provider, &mut model);
                    return;
                }

                let prompt = initial_prompt.unwrap_or_default();
                if !json_mode {
                    eprintln!("{}", render_status_bar(&provider, &model));
                }
                let agent_mode = opts
                    .agent_mode
                    .as_deref()
                    .and_then(AgentMode::from_name)
                    .unwrap_or(AgentMode::Normal);
                let allowed_tools = opts
                    .allowed_tools
                    .or_else(pinned_tools)
                    .or_else(|| agent_mode.allowed_tools());
                let mut sys = build_system_prompt(
                    agent_mode,
                    "powershell",
                    &cwd_string(),
                    &self_repo_path(),
                    &[],
                    "",
                );
                if let Some(extra) = &opts.system_prompt_extra
                    && !extra.trim().is_empty()
                {
                    sys.push_str(&format!("\n\n<cli_instructions>\n{}\n</cli_instructions>", extra.trim()));
                }
                let model_key = format!("{provider}/{model}");
                if let Some(note) = le_vibe_tools::memory::MemoryRagStore::open_default()
                    .read_by_key(&format!("model:{model_key}"))
                {
                    le_vibe_tui::append_model_profile(&mut sys, &model_key, &note);
                }
                let layered_p = load_layered_settings().0;
                let cfg = AgentConfig {
                    provider,
                    model,
                    system_prompt: Some(sys),
                    thinking_level: opts.thinking.unwrap_or_else(|| "medium".to_string()),
                    max_turns: 40,
                    max_retries: le_vibe_agent::DEFAULT_MAX_RETRIES,
                    base_delay_ms: le_vibe_agent::DEFAULT_BASE_DELAY_MS,
                    max_delay_ms: le_vibe_agent::DEFAULT_MAX_DELAY_MS,
                    // `-p` has no model registry, so the threshold trigger stays off;
                    // overflow recovery still works from provider error text.
                    context_window: 0,
                    reserve_tokens: le_vibe_agent::DEFAULT_RESERVE_TOKENS,
                    verify_edits: true,
                    allowed_tools,
                    mode_name: Some(agent_mode.name().to_string()),
                    codemode: layered_p
                        .get("codemode")
                        .cloned()
                        .unwrap_or_else(|| "off".to_string()),
                    steering_queue: None,
                    output_store: Some(Arc::new(Mutex::new(le_vibe_agent::OutputStore::new()))),
                    max_prompt_requests: layered_p
                        .get("max_prompt_requests")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0),
                    max_prompt_uncached_tokens: layered_p
                        .get("max_prompt_uncached_tokens")
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0),
                    ..AgentConfig::default()
                };
                run_streaming_prompt(&mut bridge, &mut session, &telemetry, &cfg, &prompt, json_mode);
                return;
            }

            // Interactive TUI: resolve initial model offline (0 Python, 0 network) so frame 1
            // renders and accepts keystrokes immediately while PythonBridge warms in the worker.
            let layered_boot = load_layered_settings().0;
            let (provider, model) = resolve_initial_model_offline(
                opts.provider.as_deref(),
                opts.model.as_deref(),
                &layered_boot,
            );
            run_interactive_tui(
                python_exe,
                python_dir,
                session,
                telemetry,
                provider,
                model,
                opts.yolo,
                opts.offline,
                opts.system_prompt_extra,
                opts.trust_override,
            );
        }
        CliAction::Help | CliAction::Version | CliAction::ExportOffline { .. } => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn run_interactive_tui(
    python_exe: String,
    python_dir: std::path::PathBuf,
    session: SessionStore,
    telemetry: TelemetryRecorder,
    initial_provider: String,
    initial_model: String,
    initial_yolo: bool,
    offline: bool,
    system_prompt_extra: Option<String>,
    trust_override: Option<bool>,
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
    state.history = load_history();
    let (layered, origins) = load_layered_settings();
    state.config_origins = origins;
    let parse_bool = |k: &str, default: bool| -> bool {
        layered
            .get(k)
            .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "on" | "yes"))
            .unwrap_or(default)
    };
    state.modes = crate::settings::discover_modes();
    if let Some(saved_shell) = layered.get("terminal") {
        state.shell = saved_shell.clone();
    }
    if let Some(saved_theme) = layered.get("theme").and_then(|s| ThemeId::from_name(s)) {
        state.theme = saved_theme;
    }
    if let Some(saved_mode) = layered.get("mode") {
        state.set_active_mode(saved_mode);
    }
    if let Some(saved_thinking) = layered.get("thinking") {
        state.thinking_level = saved_thinking.clone();
    }
    if !initial_yolo {
        state.yolo = parse_bool("yolo", false);
    }
    if let Some(cm) = layered.get("codemode")
        && matches!(cm.as_str(), "off" | "on" | "only")
    {
        state.codemode = cm.clone();
    }
    state.show_thinking = parse_bool("show_thinking", true);
    state.expand_tools = parse_bool("expand_tools", false);
    state.reduced_motion = parse_bool("reduced_motion", state.reduced_motion);
    state.quiet_startup = parse_bool("quiet_startup", false);
    state.bell_on_done = parse_bool("bell_on_done", false);
    state.auto_compact = parse_bool("auto_compact", true);
    state.verify_edits = parse_bool("verify_edits", true);
    if let Some(mt) = layered.get("max_turns").and_then(|s| s.parse::<usize>().ok()) {
        state.max_turns = mt.clamp(5, 200);
    }
    if let Some(rm) = layered.get("retry_max").and_then(|s| s.parse::<usize>().ok()) {
        state.retry_max = rm.min(5);
    }
    state.confirm_on_exit = parse_bool("confirm_on_exit", false);
    state.double_esc_rewind = parse_bool("double_esc_rewind", true);
    state.status_text = None;
    state.is_busy = false;

    let cwd_path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let detected_project_resources = detect_project_resources(&cwd_path);
    let saved_trust = lookup_project_trust(&cwd_path);
    let project_trusted = match trust_override {
        Some(v) => v,
        None if detected_project_resources.is_empty() => true,
        None => saved_trust.unwrap_or(false),
    };
    let needs_trust_modal =
        trust_override.is_none() && !detected_project_resources.is_empty() && saved_trust.is_none();

    let mut prompt_templates = load_prompt_templates(&cwd_path, project_trusted);
    for tpl in &prompt_templates {
        state
            .plugin_commands
            .push((tpl.command.clone(), tpl.description.clone()));
    }
    for ctx_name in ["AGENTS.override.md", "AGENTS.md", "CLAUDE.md"] {
        if cwd_path.join(ctx_name).is_file() {
            state.loaded_resources.push(ctx_name.to_string());
            break;
        }
    }
    if cwd_path.join(".le-vibe/APPEND_SYSTEM.md").is_file() {
        state.loaded_resources.push("APPEND_SYSTEM.md".to_string());
    }
    if !prompt_templates.is_empty() {
        state
            .loaded_resources
            .push(format!("{} prompt template(s)", prompt_templates.len()));
    }
    if needs_trust_modal {
        state.trust_prompt_path = cwd_path.display().to_string();
        state.trust_prompt_resources = detected_project_resources;
        state.modal = ActiveModal::TrustModal;
        state.modal_idx = 0;
    }

    let logger = EventLogger::new_default();
    logger.log(
        "ui",
        "tui_start",
        None,
        format!("provider={} model={} trusted={}", state.provider, state.model, project_trusted),
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let steering_queue: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let (cmd_tx, msg_rx, approval_tx, question_tx) = spawn_tui_worker(
        python_exe,
        python_dir,
        state.yolo,
        session,
        telemetry,
        Arc::clone(&cancel),
        Arc::clone(&steering_queue),
        project_trusted,
        Arc::clone(&logger),
        system_prompt_extra,
    );
    if state.codemode != "off" {
        let _ = cmd_tx.send(WorkerCmd::SetCodemode(state.codemode.clone()));
    }
    let _ = cmd_tx.send(WorkerCmd::RefreshCatalog {
        live: false,
        announce: false,
        provider: state.provider.clone(),
        model: state.model.clone(),
    });
    if !offline {
        let _ = cmd_tx.send(WorkerCmd::FetchUsage);
    }

    let mut guard = TerminalGuard::enter().expect("failed to initialize TUI terminal");
    let mut last_catalog_check = Instant::now();
    let mut last_spinner_tick = Instant::now();
    let mut last_live_usage: Option<Value> = None;
    let mut last_recorded_mode = state.active_mode_name().to_string();
    let mut turn_tools_count: usize = 0;
    let mut loop_scheduler = LoopScheduler::default();
    let mut last_quit_press: Option<Instant> = None;
    let mut last_esc_empty: Option<Instant> = None;
    let mut needs_redraw = true;
    let mut input_dirty = true;
    let mut last_draw_at = Instant::now();
    let mut active_prompt = false;

    loop {
        if state.active_mode_name() != last_recorded_mode {
            last_recorded_mode = state.active_mode_name().to_string();
            needs_redraw = true;
            let _ = cmd_tx.send(WorkerCmd::RecordModeChange(last_recorded_mode.clone()));
        }
        loop {
            let msg = match msg_rx.try_recv() {
                Ok(m) => m,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if state.is_busy || active_prompt {
                        state.is_busy = false;
                        active_prompt = false;
                        state.queued_prompts.clear();
                        state.status_text = None;
                        state.blocks.push(TranscriptBlock::System(
                            "Worker disconnected unexpectedly. Restart le-vibe to continue."
                                .to_string(),
                        ));
                        needs_redraw = true;
                    }
                    break;
                }
            };
            needs_redraw = true;
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
                    AgentEvent::Status { text } => {
                        state.status_text = Some(text);
                    }
                    AgentEvent::Retry {
                        attempt,
                        max_attempts,
                        delay_ms,
                    } => {
                        state.set_retry(attempt, max_attempts, delay_ms);
                    }
                    AgentEvent::TurnStart { .. } => state.clear_status_kind(),
                    AgentEvent::Compaction {
                        reason,
                        summarized,
                        kept,
                        before_tokens,
                    } => {
                        // The agent owns the trigger; the UI only maps the reason it reported.
                        let kind = match reason.as_str() {
                            "overflow" => CompactionReason::Overflow,
                            "threshold" => CompactionReason::Threshold,
                            _ => CompactionReason::Manual,
                        };
                        state.set_compaction(kind);
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Compacted {summarized} entries (kept {kept} recent) at {} tokens.",
                            format_tokens(before_tokens)
                        )));
                    }
                    AgentEvent::ThinkingDelta { delta } => state.push_stream_delta(true, &delta),
                    AgentEvent::TextDelta { delta } => state.push_stream_delta(false, &delta),
                    AgentEvent::SteeringInjected { text } => {
                        if state.steering_prompts.front().is_some_and(|s| s == &text) {
                            state.steering_prompts.pop_front();
                        }
                        state.blocks.push(TranscriptBlock::User(text));
                    }
                    AgentEvent::ToolCallStart {
                        name, args_json, ..
                    } => {
                        turn_tools_count += 1;
                        logger.log("tool", "start", None, name.clone());
                        state.set_working();
                        state.status_text = Some(format!("Running {name}…"));
                        state.blocks.push(TranscriptBlock::Tool(
                            ToolCard::new(&name, &args_json).with_status("running..."),
                        ));
                    }
                    AgentEvent::ToolCallEnd {
                        name,
                        is_error,
                        output,
                        duration_ms,
                        ..
                    } => {
                        let label = if is_error { "failed" } else { "done" };
                        let log_msg = if is_error {
                            let first_line: String = output
                                .lines()
                                .map(str::trim)
                                .find(|l| !l.is_empty())
                                .unwrap_or("")
                                .chars()
                                .take(160)
                                .collect();
                            if first_line.is_empty() {
                                format!("{name} -> {label} ({} bytes)", output.len())
                            } else {
                                format!("{name} -> {label}: {first_line}")
                            }
                        } else {
                            format!("{name} -> {label} ({} bytes)", output.len())
                        };
                        logger.log(
                            if is_error { "error" } else { "tool" },
                            "end",
                            None,
                            log_msg,
                        );
                        // Keep the full output: the renderer owns the preview budget (pi parity).
                        if let Some(TranscriptBlock::Tool(card)) =
                            state.blocks.iter_mut().rev().find(|b| {
                                matches!(
                                    b,
                                    TranscriptBlock::Tool(c)
                                    if c.name == name && c.status == "running..."
                                )
                            })
                        {
                            card.status = label.to_string();
                            card.output = output.clone();
                            card.duration_ms = Some(duration_ms);
                        }
                        if state.is_busy
                            && !state.blocks.iter().any(|b| {
                                matches!(b, TranscriptBlock::Tool(c) if c.is_running())
                            })
                        {
                            state.set_working();
                            state.status_text = Some("Thinking...".to_string());
                        }
                    }
                    _ => {}
                },
                WorkerMsg::ApprovalNeeded {
                    tool,
                    detail,
                    reason,
                } => {
                    if state.bell_on_done {
                        let _ = write!(io::stdout(), "\x07");
                        let _ = io::stdout().flush();
                    }
                    state.approval_prompt = Some((tool, detail, reason));
                    state.approval_opened_at = Some(Instant::now());
                    state.modal = ActiveModal::ApprovalModal;
                    state.modal_idx = 0;
                }
                WorkerMsg::QuestionNeeded(session) => {
                    state.question_prompt = Some(session);
                    state.modal = ActiveModal::QuestionModal;
                    state.modal_idx = 0;
                }
                WorkerMsg::ShellResult {
                    command,
                    output,
                    in_context,
                    git_badges,
                } => {
                    active_prompt = false;
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
                    context_tokens,
                    output_tokens,
                    cache_read_tokens,
                    cache_write_tokens,
                    reasoning_tokens,
                    cost_usd,
                    tps,
                    git_badges,
                } => {
                    let turn_ms = state
                        .active_started
                        .take()
                        .map(|started| {
                            let ms = started.elapsed().as_millis() as u64;
                            state.active_ms = state.active_ms.saturating_add(ms);
                            ms
                        })
                        .unwrap_or(0);
                    state.last_tps = tps;
                    state.git_badges = git_badges;
                    state.clear_status_kind();
                    if context_tokens > 0 {
                        state.context_tokens = context_tokens;
                    }
                    state.reasoning_tokens = state.reasoning_tokens.saturating_add(reasoning_tokens);
                    state.record_usage(
                        &provider,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        cost_usd,
                    );
                    if turn_tools_count > 0 {
                        let tps_str = tps.map(|v| format!(" · {v:.1} t/s")).unwrap_or_default();
                        state.blocks.push(TranscriptBlock::System(format!(
                            "✓ {} tool(s) · {} out{tps_str} · {:.1}s",
                            turn_tools_count,
                            format_tokens(output_tokens),
                            turn_ms as f64 / 1000.0
                        )));
                        turn_tools_count = 0;
                    }
                    if state.bell_on_done {
                        let _ = write!(io::stdout(), "\x07");
                        let _ = io::stdout().flush();
                    }
                    if state.mode == AgentMode::Plan {
                        state.blocks.push(TranscriptBlock::System(
                            "Plan ready — run `/goldfish` (fresh EGM review), `Alt+W` for EGM stages, or `Shift+Tab` to Execute.".to_string(),
                        ));
                    }
                    active_prompt = false;
                    // Claude-Code-style `/goal` Proof Loop evaluator: the decision lives in
                    // `le_vibe_agent::workflows`, the CLI only renders it.
                    if let Some((cond, attempt, max_att)) = state.active_goal.clone() {
                        let last_reply = state
                            .blocks
                            .iter()
                            .rev()
                            .find_map(|b| match b {
                                TranscriptBlock::Assistant(t) => Some(t.as_str()),
                                _ => None,
                            })
                            .unwrap_or("");
                        match le_vibe_agent::evaluate_proof_reply(last_reply, attempt, max_att) {
                            le_vibe_agent::ProofOutcome::Verified => {
                                state.active_goal = None;
                                state.blocks.push(TranscriptBlock::System(format!(
                                    "\u{2713} Proof Loop verified on attempt {attempt}/{max_att}: {cond}"
                                )));
                            }
                            le_vibe_agent::ProofOutcome::Blocked
                            | le_vibe_agent::ProofOutcome::CapReached => {
                                state.active_goal = None;
                                state.blocks.push(TranscriptBlock::System(format!(
                                    "Proof Loop ended at attempt {attempt}/{max_att} (not verified)."
                                )));
                            }
                            le_vibe_agent::ProofOutcome::Continue => {
                                let next = le_vibe_agent::ProofState {
                                    condition: cond.clone(),
                                    attempt: attempt + 1,
                                    max_attempts: max_att,
                                };
                                state.active_goal = Some((cond, next.attempt, max_att));
                                let cont = next.continuation_prompt();
                                state.is_busy = true;
                                active_prompt = true;
                                cancel.store(false, Ordering::SeqCst);
                                state.active_started = Some(Instant::now());
                                state.status_text =
                                    Some(format!("Proof Loop ({}/{max_att})\u{2026}", next.attempt));
                                let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                                    provider: state.provider.clone(),
                                    model: state.model.clone(),
                                    thinking_level: state.thinking_level.clone(),
                                    mode: state.mode,
                                    mode_id: state.active_mode_name().to_string(),
                                    shell: state.shell.clone(),
                                    max_turns: state.max_turns,
                                    retry_max: state.retry_max,
                                    context_window: state.agent_context_window(),
                                    verify_edits: state.verify_edits,
                                    prompt: cont,
                                });
                            }
                        }
                    }
                }
                WorkerMsg::TurnError(err) => {
                    active_prompt = false;
                    if let Some(started) = state.active_started.take() {
                        state.active_ms = state
                            .active_ms
                            .saturating_add(started.elapsed().as_millis() as u64);
                    }
                    let clean_err = err
                        .trim_start_matches("provider bridge error: ")
                        .trim_start_matches("python error (RuntimeError): ")
                        .trim_start_matches("python error: ")
                        .to_string();
                    state.blocks.push(TranscriptBlock::Tool(
                        ToolCard::new("error", "")
                            .with_status("failed")
                            .with_output(&clean_err),
                    ));
                }
                WorkerMsg::Notice(text) => {
                    if text.starts_with("Interrupted") {
                        active_prompt = false;
                    }
                    if text.starts_with("Compacted session") {
                        state.context_tokens = 0;
                        state.clear_status_kind();
                    }
                    state.blocks.push(TranscriptBlock::System(text));
                }
                WorkerMsg::InfoModal { title, lines } => {
                    state.info_modal_title = title;
                    state.info_modal_lines = lines;
                    state.help_scroll = 0;
                    state.modal = ActiveModal::InfoModal;
                }
                WorkerMsg::RewindLoaded(rows) => {
                    state.rewind_rows = rows;
                    state.modal_idx = state.rewind_rows.len().saturating_sub(1);
                    state.modal = ActiveModal::RewindPicker;
                }
                WorkerMsg::TreeLoaded { rows, action } => {
                    state.tree_rows = rows;
                    state.tree_action = action;
                    state.modal_idx = 0;
                    state.modal = ActiveModal::TreePicker;
                }
                WorkerMsg::SessionsLoaded(rows) => {
                    state.session_rows = rows;
                    state.modal_filter.clear();
                    state.modal_idx = 0;
                    state.modal = ActiveModal::SessionPicker;
                }
                WorkerMsg::WorkspaceFilesLoaded(files) => {
                    state.workspace_files = files;
                }
                WorkerMsg::PluginCommandsLoaded(cmds) => {
                    state.plugin_commands = cmds;
                    for tpl in &prompt_templates {
                        if !state.plugin_commands.iter().any(|(c, _)| c == &tpl.command) {
                            state
                                .plugin_commands
                                .push((tpl.command.clone(), tpl.description.clone()));
                        }
                    }
                }
                WorkerMsg::SessionReplaced {
                    blocks,
                    notice,
                    prefill_input,
                    reset_usage,
                } => {
                    if reset_usage {
                        state.input_tokens = 0;
                        state.context_tokens = 0;
                        state.output_tokens = 0;
                        state.cache_read_tokens = 0;
                        state.cost_usd = 0.0;
                    }
                    state.blocks.clear();
                    for (role, content) in blocks {
                        match role.as_str() {
                            "user" => state.blocks.push(TranscriptBlock::User(content)),
                            "assistant" => state.blocks.push(TranscriptBlock::Assistant(content)),
                            _ => state.blocks.push(TranscriptBlock::System(content)),
                        }
                    }
                    state.blocks.push(TranscriptBlock::System(notice));
                    state.scroll_offset = 0;
                    if let Some(text) = prefill_input {
                        state.input = text;
                        state.cursor = state.input.len();
                    }
                }
                WorkerMsg::TodosUpdated(items) => {
                    state.todos = items;
                }
                WorkerMsg::Idle => {
                    if active_prompt {
                        // Ignore Idle emitted by non-prompt background/info commands while a prompt is active.
                        continue;
                    }
                    if let Some(next_prompt) = state.queued_prompts.pop_front() {
                        cancel.store(false, Ordering::SeqCst);
                        state.is_busy = true;
                        active_prompt = true;
                        state.scroll_offset = 0;
                        state.active_started = Some(Instant::now());
                        if let Some(rest) = next_prompt.strip_prefix("!!") {
                            let cmd = rest.trim().to_string();
                            state.status_text = Some(format!("Running `{cmd}`..."));
                            let _ = cmd_tx.send(WorkerCmd::RunShell {
                                command: cmd,
                                include_in_context: false,
                                shell: state.shell.clone(),
                            });
                        } else if let Some(rest) = next_prompt.strip_prefix('!') {
                            let cmd = rest.trim().to_string();
                            state.status_text = Some(format!("Running `{cmd}`..."));
                            let _ = cmd_tx.send(WorkerCmd::RunShell {
                                command: cmd,
                                include_in_context: true,
                                shell: state.shell.clone(),
                            });
                        } else {
                            state.status_text = Some("Thinking...".to_string());
                            state.blocks.push(TranscriptBlock::User(next_prompt.clone()));
                            let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                                provider: state.provider.clone(),
                                model: state.model.clone(),
                                thinking_level: state.thinking_level.clone(),
                                mode: state.mode,
                                mode_id: state.active_mode_name().to_string(),
                                shell: state.shell.clone(),
                                max_turns: state.max_turns,
                                retry_max: state.retry_max,
                                context_window: state.agent_context_window(),
                                    verify_edits: state.verify_edits,
                                prompt: next_prompt,
                            });
                        }
                    } else {
                        state.is_busy = false;
                        state.status_text = None;
                    }
                }
            }
        }

        if (state.is_busy || state.usage_loading)
            && last_spinner_tick.elapsed() >= Duration::from_millis(80)
        {
            last_spinner_tick = Instant::now();
            state.advance_spinner();
            needs_redraw = true;
        }

        if !state.is_busy
            && let Some(due_prompt) = loop_scheduler.pop_due()
        {
            needs_redraw = true;
            state.blocks.push(TranscriptBlock::User(format!("[loop] {due_prompt}")));
            state.is_busy = true;
            active_prompt = true;
            cancel.store(false, Ordering::SeqCst);
            state.active_started = Some(Instant::now());
            state.status_text = Some("Running scheduled loop...".to_string());
            let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                provider: state.provider.clone(),
                model: state.model.clone(),
                thinking_level: state.thinking_level.clone(),
                mode: state.mode,
                mode_id: state.active_mode_name().to_string(),
                shell: state.shell.clone(),
                max_turns: state.max_turns,
                retry_max: state.retry_max,
                context_window: state.agent_context_window(),
                                    verify_edits: state.verify_edits,
                prompt: due_prompt,
            });
        }

        if !offline && last_catalog_check.elapsed() >= Duration::from_secs(60) && !state.is_busy {
            last_catalog_check = Instant::now();
            let _ = cmd_tx.send(WorkerCmd::CheckStaleAndRefresh {
                provider: state.provider.clone(),
                model: state.model.clone(),
            });
        }

        let can_draw_now = !state.is_busy
            || input_dirty
            || last_draw_at.elapsed() >= Duration::from_millis(24);
        if needs_redraw && can_draw_now {
            input_dirty = false;
            last_draw_at = Instant::now();
            if DEBUG_ENABLED && state.modal == ActiveModal::DebugModal {
                let summary = format!(
                    "State: busy={} mode={} model={}/{} blocks={} queue={} last_draw={}ms",
                    state.is_busy,
                    state.mode.name(),
                    state.provider,
                    state.model,
                    state.blocks.len(),
                    state.queued_prompts.len(),
                    state.last_draw_ms,
                );
                state.debug_lines = logger.formatted_lines(&state.debug_filter, &summary);
            }
            let draw_start = Instant::now();
            let before_scroll = state.rendered_scroll.get();
            // Synchronized update (DEC mode 2026) prevents mid-frame cursor flicker during animations.
            let _ = write!(guard.terminal.backend_mut(), "\x1b[?2026h");
            let draw_failed = guard.terminal.draw(|f| draw_frame(f, &state)).is_err();
            let _ = write!(guard.terminal.backend_mut(), "\x1b[?2026l");
            let _ = guard.terminal.backend_mut().flush();
            if draw_failed {
                break;
            }
            let draw_ms = draw_start.elapsed().as_millis() as u64;
            state.last_draw_ms = draw_ms;
            if draw_ms > 16 {
                logger.log("perf", "slow_frame", Some(draw_ms), format!("blocks={}", state.blocks.len()));
            }
            // Keep animating while smooth scroll is still easing.
            needs_redraw = state.rendered_scroll.get() != before_scroll;
        }

        // Wait up to 4ms while streaming so token bursts & keys react immediately; 16ms when idle.
        let poll_wait = if state.is_busy {
            Duration::from_millis(4)
        } else {
            Duration::from_millis(16)
        };
        let mut actions = Vec::new();
        let first = poll_tui_action(&mut state, poll_wait).unwrap_or(TuiAction::None);
        if !matches!(first, TuiAction::None) {
            needs_redraw = true;
            input_dirty = true;
            actions.push(first);
            for _ in 0..15 {
                let next = poll_tui_action(&mut state, Duration::ZERO).unwrap_or(TuiAction::None);
                if matches!(next, TuiAction::None) {
                    break;
                }
                actions.push(next);
            }
        }
        let mut should_quit = false;
        for action in actions {
            match action {
            TuiAction::None | TuiAction::Redraw => {}
            TuiAction::Quit => {
                if state.confirm_on_exit
                    && last_quit_press
                        .is_none_or(|t| t.elapsed() > Duration::from_millis(1500))
                {
                    last_quit_press = Some(Instant::now());
                    state.status_text = Some("Press Ctrl+C again to quit".to_string());
                    continue;
                }
                save_history(&state.history);
                if state.scope_dirty {
                    save_scope(&state.scoped_models);
                }
                let _ = cmd_tx.send(WorkerCmd::Shutdown);
                should_quit = true;
                break;
            }
            TuiAction::Interrupt => {
                if !state.is_busy {
                    if state.scroll_offset > 0 {
                        state.scroll_offset = 0;
                        continue;
                    }
                    if !state.pending_attachments.is_empty() {
                        state.pending_attachments.clear();
                        state.status_text = Some("Cleared pending attachment(s).".to_string());
                        continue;
                    }
                    if state.input.is_empty() && state.double_esc_rewind {
                        if last_esc_empty.is_some_and(|t| t.elapsed() <= Duration::from_millis(700)) {
                            last_esc_empty = None;
                            state.status_text = None;
                            let _ = cmd_tx.send(WorkerCmd::ListRewindCheckpoints);
                        } else {
                            last_esc_empty = Some(Instant::now());
                            state.status_text = Some("Press Esc again to /rewind".to_string());
                        }
                    }
                    continue;
                }
                logger.log("ui", "interrupt", None, "Esc pressed while busy");
                cancel.store(true, Ordering::SeqCst);
                state.usage_loading = false;
                let mut recovered: Vec<String> = Vec::new();
                if let Ok(mut sq) = steering_queue.lock() {
                    recovered.extend(sq.drain(..));
                }
                state.steering_prompts.clear();
                recovered.extend(state.queued_prompts.drain(..));
                if !recovered.is_empty() {
                    let count = recovered.len();
                    let joined = recovered.join("\n");
                    if state.input.is_empty() {
                        state.input = joined;
                    } else {
                        state.input = format!("{}\n{}", joined, state.input);
                    }
                    state.cursor = state.input.len();
                    state.status_text = Some(format!(
                        "Interrupting (returned {count} queued prompt(s) to editor)..."
                    ));
                } else {
                    state.status_text = Some("Interrupting...".to_string());
                }
            }
            TuiAction::RecallQueued => {
                let mut recovered: Vec<String> = Vec::new();
                if let Ok(mut sq) = steering_queue.lock() {
                    recovered.extend(sq.drain(..));
                }
                state.steering_prompts.clear();
                recovered.extend(state.queued_prompts.drain(..));
                if !recovered.is_empty() {
                    let count = recovered.len();
                    let joined = recovered.join("\n");
                    if state.input.is_empty() {
                        state.input = joined;
                    } else {
                        state.input = format!("{}\n{}", joined, state.input);
                    }
                    state.cursor = state.input.len();
                    state.status_text = Some(format!(
                        "Returned {count} queued prompt(s) to editor."
                    ));
                }
            }
            TuiAction::OpenExternalEditor => {
                let res = guard.suspend_for(|| run_external_editor(&state.input));
                match res {
                    Ok(edited) => {
                        state.input = edited.trim_end_matches('\n').to_string();
                        state.cursor = state.input.len();
                    }
                    Err(e) => {
                        state.blocks.push(TranscriptBlock::System(format!(
                            "External editor error: {e}"
                        )));
                    }
                }
            }
            TuiAction::CopyLastReply => {
                let last_msg = state.blocks.iter().rev().find_map(|b| match b {
                    TranscriptBlock::Assistant(txt) if !txt.trim().is_empty() => Some(txt.clone()),
                    _ => None,
                });
                if let Some(txt) = last_msg {
                    if copy_to_clipboard(&txt) {
                        state.status_text = Some(format!(
                            "Copied last assistant reply ({} chars) to clipboard.",
                            txt.len()
                        ));
                    } else {
                        state.status_text = Some("Clipboard copy failed.".to_string());
                    }
                } else {
                    state.status_text = Some("No assistant reply to copy yet.".to_string());
                }
            }
            TuiAction::TrustDecision(allow) => {
                save_project_trust(&cwd_path, allow);
                if allow {
                    prompt_templates = load_prompt_templates(&cwd_path, true);
                    for tpl in &prompt_templates {
                        if !state.plugin_commands.iter().any(|(c, _)| c == &tpl.command) {
                            state
                                .plugin_commands
                                .push((tpl.command.clone(), tpl.description.clone()));
                        }
                    }
                    let _ = cmd_tx.send(WorkerCmd::ReloadProjectResources);
                } else {
                    state.blocks.push(TranscriptBlock::System(
                        "Project marked untrusted — skipped project .le-vibe resources.".to_string(),
                    ));
                }
            }
            TuiAction::SubmitFollowUp(text) => {
                state.queued_prompts.push_back(text);
            }
            TuiAction::ToggleDebug => {
                if DEBUG_ENABLED {
                    if state.modal == ActiveModal::DebugModal {
                        state.modal = ActiveModal::None;
                        state.help_scroll = 0;
                    } else {
                        state.help_scroll = 0;
                        state.modal = ActiveModal::DebugModal;
                        logger.log("ui", "debug_open", None, "opened debugger overlay");
                    }
                }
            }
            TuiAction::DebugClear => {
                if DEBUG_ENABLED {
                    logger.clear();
                    state.help_scroll = 0;
                }
            }
            TuiAction::DebugCycleFilter => {
                if DEBUG_ENABLED {
                    const FILTERS: &[&str] = &["all", "error", "perf", "worker", "agent", "tool", "ui"];
                    let cur = FILTERS
                        .iter()
                        .position(|f| *f == state.debug_filter)
                        .unwrap_or(0);
                    state.debug_filter = FILTERS[(cur + 1) % FILTERS.len()].to_string();
                    state.help_scroll = 0;
                }
            }
            TuiAction::PasteClipboard => {
                if let Some(item) = grab_clipboard_item() {
                    apply_pasted_item(&mut state, item);
                }
            }
            TuiAction::Pasted(raw) => {
                apply_pasted_item(&mut state, classify_pasted_text(&raw));
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
                    let _ = cmd_tx.send(WorkerCmd::RecordModelChange(format!(
                        "{}/{}:{}",
                        state.provider, state.model, state.thinking_level
                    )));
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
                        save_setting("yolo", "true");
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
            TuiAction::ConfigCycleKey(key) => {
                let scope = state.config_save_scope;
                let next_val: Option<String> = match key.as_str() {
                    "theme" => {
                        let cur = ALL_THEMES.iter().position(|t| *t == state.theme).unwrap_or(0);
                        state.theme = ALL_THEMES[(cur + 1) % ALL_THEMES.len()];
                        Some(state.theme.name().to_string())
                    }
                    "terminal" => {
                        let shells = ["powershell", "pwsh", "bash", "cmd", "zsh", "sh"];
                        let cur = shells.iter().position(|s| *s == state.shell).unwrap_or(0);
                        state.shell = shells[(cur + 1) % shells.len()].to_string();
                        Some(state.shell.clone())
                    }
                    "mode" => {
                        state.mode = state.mode.next();
                        Some(state.mode.name().to_string())
                    }
                    "thinking" => {
                        let levels = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
                        let cur = levels.iter().position(|l| *l == state.thinking_level).unwrap_or(3);
                        state.thinking_level = levels[(cur + 1) % levels.len()].to_string();
                        Some(state.thinking_level.clone())
                    }
                    "yolo" => {
                        state.yolo = !state.yolo;
                        let _ = cmd_tx.send(WorkerCmd::SetYolo(state.yolo));
                        Some(state.yolo.to_string())
                    }
                    "codemode" => {
                        let modes = ["off", "on", "only"];
                        let cur = modes.iter().position(|m| *m == state.codemode).unwrap_or(0);
                        state.codemode = modes[(cur + 1) % modes.len()].to_string();
                        let _ = cmd_tx.send(WorkerCmd::SetCodemode(state.codemode.clone()));
                        Some(state.codemode.clone())
                    }
                    "show_thinking" => {
                        state.show_thinking = !state.show_thinking;
                        Some(state.show_thinking.to_string())
                    }
                    "expand_tools" => {
                        state.expand_tools = !state.expand_tools;
                        Some(state.expand_tools.to_string())
                    }
                    "reduced_motion" => {
                        state.reduced_motion = !state.reduced_motion;
                        Some(state.reduced_motion.to_string())
                    }
                    "quiet_startup" => {
                        state.quiet_startup = !state.quiet_startup;
                        Some(state.quiet_startup.to_string())
                    }
                    "bell_on_done" => {
                        state.bell_on_done = !state.bell_on_done;
                        Some(state.bell_on_done.to_string())
                    }
                    "auto_compact" => {
                        state.auto_compact = !state.auto_compact;
                        Some(state.auto_compact.to_string())
                    }
                    "verify_edits" => {
                        state.verify_edits = !state.verify_edits;
                        Some(state.verify_edits.to_string())
                    }
                    "max_turns" => {
                        let opts = [20usize, 40, 80];
                        let cur = opts.iter().position(|v| *v == state.max_turns).unwrap_or(1);
                        state.max_turns = opts[(cur + 1) % opts.len()];
                        Some(state.max_turns.to_string())
                    }
                    "retry_max" => {
                        let opts = [0usize, 2, 3];
                        let cur = opts.iter().position(|v| *v == state.retry_max).unwrap_or(1);
                        state.retry_max = opts[(cur + 1) % opts.len()];
                        Some(state.retry_max.to_string())
                    }
                    "double_esc_rewind" => {
                        state.double_esc_rewind = !state.double_esc_rewind;
                        Some(state.double_esc_rewind.to_string())
                    }
                    "confirm_on_exit" => {
                        state.confirm_on_exit = !state.confirm_on_exit;
                        Some(state.confirm_on_exit.to_string())
                    }
                    _ => None,
                };
                if let Some(val) = next_val {
                    let origin = save_setting_for_scope(scope, &key, &val);
                    state.config_origins.insert(key, origin.to_string());
                }
            }
            TuiAction::Submit(input) => {
                if let Some(rest) = input.strip_prefix("/followup") {
                    let msg = rest.trim().to_string();
                    if msg.is_empty() {
                        state.blocks.push(TranscriptBlock::System(
                            "Usage: /followup <prompt> (or press Alt+Enter / Ctrl+Enter while busy)".to_string(),
                        ));
                    } else if state.is_busy {
                        state.queued_prompts.push_back(msg);
                    } else {
                        state.queued_prompts.push_back(msg);
                        let _ = cmd_tx.send(WorkerCmd::McpStatus);
                    }
                    continue;
                }
                if state.is_busy && !input.starts_with('/') {
                    if input.starts_with('!') {
                        state.queued_prompts.push_back(input);
                    } else {
                        if let Ok(mut sq) = steering_queue.lock() {
                            sq.push_back(input.clone());
                        }
                        state.steering_prompts.push_back(input);
                    }
                    continue;
                }
                if state.is_busy
                    && matches!(
                        input.split_whitespace().next().unwrap_or(""),
                        "/compact" | "/handover" | "/clear" | "/new" | "/rewind" | "/tree" | "/resume"
                    )
                {
                    state.blocks.push(TranscriptBlock::System(
                        "Turn in progress — wait for it to finish or press Esc first.".to_string(),
                    ));
                    continue;
                }
                if input == "/quit" {
                    save_history(&state.history);
                    if state.scope_dirty {
                        save_scope(&state.scoped_models);
                    }
                    let _ = cmd_tx.send(WorkerCmd::Shutdown);
                    should_quit = true;
                    break;
                }
                if input == "/clear" {
                    state.blocks.clear();
                    continue;
                }
                if input == "/policy" {
                    let _ = cmd_tx.send(WorkerCmd::PolicyStatus);
                    continue;
                }
                if input == "/usage" {
                    state.modal = ActiveModal::UsageModal;
                    state.usage_loading = true;
                    state.usage_scroll = 0;
                    let _ = cmd_tx.send(WorkerCmd::FetchUsage);
                    continue;
                }
                if input == "/cost" {
                    let log_path = le_vibe_telemetry::usage::latest_usage_log_path();
                    let report = match log_path
                        .and_then(|p| le_vibe_telemetry::usage::read_usage_log(&p).ok())
                    {
                        Some(recs) => le_vibe_telemetry::usage::format_usage_report(&recs),
                        None => "No forensic usage records found for this session yet.".to_string(),
                    };
                    state.blocks.push(TranscriptBlock::System(report));
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/ext load ") {
                    let name = arg.trim();
                    if !name.is_empty() {
                        let _ = cmd_tx.send(WorkerCmd::LoadExtension(name.to_string()));
                    }
                    continue;
                }
                if input == "/reload" {
                    let (layered, origins) = load_layered_settings();
                    state.config_origins = origins;
                    state.modes = crate::settings::discover_modes();
                    if let Some(t) = layered.get("theme").and_then(|s| ThemeId::from_name(s)) {
                        state.theme = t;
                    }
                    if let Some(sh) = layered.get("terminal") {
                        state.shell = sh.clone();
                    }
                    if let Some(m) = layered.get("mode") {
                        state.set_active_mode(m);
                    }
                    let _ = cmd_tx.send(WorkerCmd::ReloadProjectResources);
                    state.workspace_files = scan_workspace_files(std::path::Path::new("."), 1500);
                    state.is_busy = true;
                    state.status_text = Some("Reloading catalogs, settings, policy, plugins & MCP...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::RefreshCatalog {
                        live: true,
                        announce: true,
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                    });
                    continue;
                }
                if input == "/todos" || input == "/todo" {
                    state.show_todo_pane = !state.show_todo_pane;
                    continue;
                }
                if input == "/yolo" {
                    state.yolo = !state.yolo;
                    save_setting("yolo", &state.yolo.to_string());
                    let _ = cmd_tx.send(WorkerCmd::SetYolo(state.yolo));
                    state.blocks.push(TranscriptBlock::System(if state.yolo {
                        "YOLO mode enabled (all non-blocked tools auto-approved).".to_string()
                    } else {
                        "YOLO mode disabled (ask mode restored).".to_string()
                    }));
                    continue;
                }
                if input == "/workflow" || input == "/workflows" {
                    state.modal = ActiveModal::WorkflowPicker;
                    state.modal_idx = 0;
                    continue;
                }
                if input == "/egm" {
                    state.mode = AgentMode::Plan;
                    let _ = cmd_tx.send(WorkerCmd::RecordModeChange("plan".to_string()));
                    state.modal = ActiveModal::WorkflowPicker;
                    state.modal_idx = 0;
                    continue;
                }
                if input == "/goldfish" || input.starts_with("/goldfish ") {
                    let target = input.strip_prefix("/goldfish").unwrap_or("").trim().to_string();
                    let fallback_text = state
                        .blocks
                        .iter()
                        .rev()
                        .find_map(|b| match b {
                            TranscriptBlock::Assistant(t) if !t.trim().is_empty() => Some(t.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    state.is_busy = true;
                    state.active_started = Some(Instant::now());
                    state.status_text = Some("Goldfish reviewing (fresh zero-history context)…".to_string());
                    let _ = cmd_tx.send(WorkerCmd::GoldfishReview {
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                        thinking_level: state.thinking_level.clone(),
                        target,
                        fallback_text,
                    });
                    continue;
                }
                if input == "/goal" || input.starts_with("/goal ") {
                    let arg = input.strip_prefix("/goal").unwrap_or("").trim();
                    if arg.is_empty() {
                        if let Some((ref cond, att, max_att)) = state.active_goal {
                            state.blocks.push(TranscriptBlock::System(format!(
                                "Active Proof Loop goal ({att}/{max_att}): {cond}\nUse `/goal clear` to stop."
                            )));
                        } else {
                            state.blocks.push(TranscriptBlock::System(
                                "Usage: `/goal <binary stop condition>` (e.g. `/goal cargo test --workspace passes with 0 warnings`) or `/goal clear`.".to_string(),
                            ));
                        }
                        continue;
                    }
                    if arg.eq_ignore_ascii_case("clear") || arg.eq_ignore_ascii_case("stop") {
                        state.active_goal = None;
                        state.blocks.push(TranscriptBlock::System("Proof Loop goal cleared.".to_string()));
                        continue;
                    }
                    state.mode = AgentMode::Normal;
                    state.active_goal = Some((
                        arg.to_string(),
                        1,
                        le_vibe_agent::workflows::PROOF_MAX_ATTEMPTS,
                    ));
                    let goal_prompt =
                        le_vibe_agent::ProofState::new(arg, le_vibe_agent::workflows::PROOF_MAX_ATTEMPTS)
                            .initial_prompt();
                    state.blocks.push(TranscriptBlock::User(format!("/goal {arg}")));
                    state.is_busy = true;
                    active_prompt = true;
                    cancel.store(false, Ordering::SeqCst);
                    state.scroll_offset = 0;
                    state.active_started = Some(Instant::now());
                    state.status_text = Some("Proof Loop (1/5)…".to_string());
                    let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                        thinking_level: state.thinking_level.clone(),
                        mode: state.mode,
                        mode_id: state.active_mode_name().to_string(),
                        shell: state.shell.clone(),
                        max_turns: state.max_turns,
                        retry_max: state.retry_max,
                        context_window: state.agent_context_window(),
                                    verify_edits: state.verify_edits,
                        prompt: goal_prompt,
                    });
                    continue;
                }
                if input == "/mode" {
                    state.modal = ActiveModal::ModePicker;
                    let cur_name = state.active_mode_name().to_string();
                    state.modal_idx = state
                        .primary_modes()
                        .iter()
                        .position(|m| m.id.eq_ignore_ascii_case(&cur_name))
                        .unwrap_or(0);
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/mode ") {
                    if state.set_active_mode(arg) {
                        let active_name = state.active_mode_name().to_string();
                        let desc = state
                            .modes
                            .iter()
                            .find(|m| m.id.eq_ignore_ascii_case(&active_name))
                            .map(|m| m.description.clone())
                            .unwrap_or_default();
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Switched mode to [{active_name}] — {desc}"
                        )));
                    } else {
                        let names: Vec<&str> = state
                            .primary_modes()
                            .into_iter()
                            .map(|m| m.id.as_str())
                            .collect();
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Unknown mode {:?}. Choose: {}.",
                            arg.trim(),
                            names.join(", ")
                        )));
                    }
                    continue;
                }
                if input == "/config" {
                    state.modal_filter.clear();
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
                    state.is_busy = true;
                    state.set_compaction(CompactionReason::Manual);
                    let _ = cmd_tx.send(WorkerCmd::Compact {
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                        instructions: None,
                    });
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/compact ") {
                    state.is_busy = true;
                    state.set_compaction(CompactionReason::Manual);
                    let _ = cmd_tx.send(WorkerCmd::Compact {
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                        instructions: Some(arg.trim().to_string()),
                    });
                    continue;
                }
                if input == "/hotkeys" {
                    state.info_modal_title = "Active Keybindings (/hotkeys)".to_string();
                    state.info_modal_lines = format_hotkeys_lines();
                    state.help_scroll = 0;
                    state.modal = ActiveModal::InfoModal;
                    continue;
                }
                if let Some(rest) = input.strip_prefix("/search") {
                    let q = rest.trim();
                    if q.is_empty() {
                        state.blocks.push(TranscriptBlock::System(
                            "Usage: /search <query> — BM25 search across saved sessions.".to_string(),
                        ));
                    } else {
                        let dirs = vec![sessions_dir(), crate::settings::project_sessions_dir()];
                        state.info_modal_title = format!("Session Search: {q}");
                        state.info_modal_lines = search_past_sessions(&dirs, q, 18);
                        state.help_scroll = 0;
                        state.modal = ActiveModal::InfoModal;
                    }
                    continue;
                }
                if input == "/tree" {
                    let _ = cmd_tx.send(WorkerCmd::ListTreeNodes {
                        action: le_vibe_tui::TreeAction::Switch,
                    });
                    continue;
                }
                if input == "/fork" || input == "/clone" {
                    let _ = cmd_tx.send(WorkerCmd::ListTreeNodes {
                        action: le_vibe_tui::TreeAction::Fork,
                    });
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/fork ") {
                    let target = arg.trim();
                    if !target.is_empty() {
                        let _ = cmd_tx.send(WorkerCmd::ForkTo(target.to_string()));
                    }
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/tree-switch ") {
                    // Tree navigation is instant: move the branch, no summarizer call.
                    let _ = cmd_tx.send(WorkerCmd::SwitchBranch(arg.trim().to_string()));
                    continue;
                }
                if input == "/rewind" {
                    let _ = cmd_tx.send(WorkerCmd::ListRewindCheckpoints);
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/rewind ") {
                    let id = arg.trim();
                    if id != "(empty)" && !id.is_empty() {
                        let _ = cmd_tx.send(WorkerCmd::RewindTo {
                            id: id.to_string(),
                            provider: state.provider.clone(),
                            model: state.model.clone(),
                        });
                    }
                    continue;
                }
                if input == "/new" {
                    state.input_tokens = 0;
                    state.context_tokens = 0;
                    state.output_tokens = 0;
                    state.cache_read_tokens = 0;
                    state.cost_usd = 0.0;
                    let _ = cmd_tx.send(WorkerCmd::NewSession);
                    continue;
                }
                if input == "/resume" {
                    let _ = cmd_tx.send(WorkerCmd::ListSessions);
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/resume ") {
                    let p = arg.trim();
                    if p != "(none)" && !p.is_empty() {
                        let _ = cmd_tx.send(WorkerCmd::ResumeSession(p.to_string()));
                    }
                    continue;
                }
                if input == "/rename" {
                    let _ = cmd_tx.send(WorkerCmd::SetSessionName(None));
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/rename ") {
                    let _ = cmd_tx.send(WorkerCmd::SetSessionName(Some(arg.trim().to_string())));
                    continue;
                }
                if input == "/status" {
                    let _ = cmd_tx.send(WorkerCmd::SessionInfo);
                    continue;
                }
                if input == "/context" {
                    let _ = cmd_tx.send(WorkerCmd::ContextInfo {
                        mode: state.mode,
                        shell: state.shell.clone(),
                        context_window: state.context_window,
                        last_input_tokens: state.input_tokens,
                    });
                    continue;
                }
                if input == "/handover" {
                    state.is_busy = true;
                    state.status_text = Some("Generating session handover...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::Handover {
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                        goal: None,
                    });
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/handover ") {
                    state.is_busy = true;
                    state.status_text = Some("Generating session handover...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::Handover {
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                        goal: Some(arg.trim().to_string()),
                    });
                    continue;
                }
                if input == "/report" {
                    state.is_busy = true;
                    state.status_text = Some("Generating bug report PDF...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::GenerateBugReport {
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                        repo: state.repo_name.clone(),
                    });
                    continue;
                }
                if input == "/learn" {
                    let rag = le_vibe_tools::memory::MemoryRagStore::open_default();
                    let report = learn::mine_sessions(&sessions_dir(), &rag, 20);
                    state
                        .blocks
                        .push(TranscriptBlock::Report(learn::format_learn_report(&report)));
                    continue;
                }
                if input == "/suggestion" {
                    let home = std::env::var_os("USERPROFILE")
                        .or_else(|| std::env::var_os("HOME"))
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let logs_dir = home.join(".le-vibe").join("logs");
                    let prompt = build_optimization_digest(
                        &sessions_dir(),
                        &logs_dir,
                        &self_repo_path(),
                    );
                    state.blocks.push(TranscriptBlock::User("/suggestion".to_string()));
                    state.is_busy = true;
                    active_prompt = true;
                    cancel.store(false, Ordering::SeqCst);
                    state.active_started = Some(Instant::now());
                    state.status_text = Some("Analyzing previous sessions for optimizations...".to_string());
                    let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                        provider: state.provider.clone(),
                        model: state.model.clone(),
                        thinking_level: state.thinking_level.clone(),
                        mode: state.mode,
                        mode_id: state.active_mode_name().to_string(),
                        shell: state.shell.clone(),
                        max_turns: state.max_turns,
                        retry_max: state.retry_max,
                        context_window: state.agent_context_window(),
                                    verify_edits: state.verify_edits,
                        prompt,
                    });
                    continue;
                }
                if input == "/diagram" {
                    let latest_xml = state.blocks.iter().rev().find_map(|b| match b {
                        TranscriptBlock::Assistant(txt) => extract_latest_xml_block(txt),
                        _ => None,
                    });
                    if let Some(xml) = latest_xml {
                        state.info_modal_title = "XML Diagram Projection (/diagram)".to_string();
                        state.info_modal_lines = project_xml_diagram(&xml);
                        state.help_scroll = 0;
                        state.modal = ActiveModal::InfoModal;
                    } else {
                        state.blocks.push(TranscriptBlock::System(
                            "No XML/draw.io diagram found in transcript. Pass a file path: /diagram <file.xml|file.drawio>".to_string(),
                        ));
                    }
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/diagram ") {
                    let path = arg.trim().trim_matches('"');
                    match fs::read_to_string(path) {
                        Ok(xml) => {
                            state.info_modal_title = format!("Diagram: {path}");
                            state.info_modal_lines = project_xml_diagram(&xml);
                            state.help_scroll = 0;
                            state.modal = ActiveModal::InfoModal;
                        }
                        Err(err) => {
                            state.blocks.push(TranscriptBlock::System(format!(
                                "Failed to read diagram file {path:?}: {err}"
                            )));
                        }
                    }
                    continue;
                }
                if input == "/memory" {
                    let store = le_vibe_tools::memory::MemoryRagStore::open_default();
                    state.blocks.push(TranscriptBlock::System(store.stats_summary()));
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/memory ") {
                    let q = arg.trim();
                    let store = le_vibe_tools::memory::MemoryRagStore::open_default();
                    if q.eq_ignore_ascii_case("index") {
                        match store.index_repo_docs(std::path::Path::new("."), 300) {
                            Ok(n) => state.blocks.push(TranscriptBlock::System(format!(
                                "Indexed {n} repo doc/signature chunks into Rust RAG v2 store."
                            ))),
                            Err(err) => state.blocks.push(TranscriptBlock::System(format!(
                                "RAG indexing failed: {err}"
                            ))),
                        }
                    } else {
                        let (scope, query) = if let Some(rest) = q.strip_prefix("repo ") {
                            ("repo", rest.trim())
                        } else {
                            ("memory", q)
                        };
                        let hits = store.search(query, 5, scope);
                        if hits.is_empty() {
                            state.blocks.push(TranscriptBlock::System(format!(
                                "No relevant {scope} matches for {query:?}."
                            )));
                        } else {
                            let body = hits
                                .into_iter()
                                .enumerate()
                                .map(|(i, h)| {
                                    let preview: String = h.body.chars().take(180).collect();
                                    format!(
                                        "{}. [{}] [{}] (cos={:.2}, bm25={:.2}) {}\n   {}",
                                        i + 1,
                                        h.id,
                                        h.source,
                                        h.cosine,
                                        h.bm25,
                                        h.title,
                                        preview.replace('\n', " ")
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            state.blocks.push(TranscriptBlock::System(body));
                        }
                    }
                    continue;
                }
                if input == "/loop" || input == "/loop list" {
                    state.blocks.push(TranscriptBlock::System(loop_scheduler.list_summary()));
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/loop ") {
                    let trimmed = arg.trim();
                    if let Some(stop_target) = trimmed.strip_prefix("stop") {
                        let removed = loop_scheduler.stop(stop_target.trim());
                        state.active_loops = loop_scheduler.len();
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Stopped {removed} scheduled loop(s)."
                        )));
                    } else if let Some((interval, prompt)) = parse_loop_spec(trimmed) {
                        let id = loop_scheduler.add(interval, prompt.clone());
                        state.active_loops = loop_scheduler.len();
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Scheduled loop #{id} every {}s: {prompt} (stop with /loop stop {id})",
                            interval.as_secs()
                        )));
                    } else {
                        state.blocks.push(TranscriptBlock::System(
                            "Usage: /loop <interval> <prompt> (e.g. /loop 2m cargo test) | /loop list | /loop stop [id|all]"
                                .to_string(),
                        ));
                    }
                    continue;
                }
                if input == "/plugins" {
                    let _ = cmd_tx.send(WorkerCmd::PluginsStatus);
                    continue;
                }
                if input == "/reload-plugins" {
                    let _ = cmd_tx.send(WorkerCmd::ReloadPlugins);
                    continue;
                }
                if let Some(rest) = input.strip_prefix("/skill-save") {
                    let trimmed = rest.trim();
                    let mut parts = trimmed.splitn(2, char::is_whitespace);
                    let raw_name = parts.next().unwrap_or("").trim();
                    let extra = parts.next().unwrap_or("").trim();
                    let slug = le_vibe_agent::workflows::skill_slug(raw_name);
                    if slug.is_empty() {
                        state.blocks.push(TranscriptBlock::System(
                            "Usage: /skill-save <skill-name> [optional focus]".to_string(),
                        ));
                    } else {
                        let distill_prompt =
                            le_vibe_agent::workflows::skill_distillation_prompt(&slug, extra);
                        state.blocks.push(TranscriptBlock::User(format!("/skill-save {slug}")));
                        state.is_busy = true;
                        active_prompt = true;
                        cancel.store(false, Ordering::SeqCst);
                        state.scroll_offset = 0;
                        state.active_started = Some(Instant::now());
                        state.status_text = Some(format!("Distilling skill {slug}..."));
                        let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                            provider: state.provider.clone(),
                            model: state.model.clone(),
                            thinking_level: state.thinking_level.clone(),
                            mode: state.mode,
                            mode_id: state.active_mode_name().to_string(),
                            shell: state.shell.clone(),
                            max_turns: state.max_turns,
                            retry_max: state.retry_max,
                            context_window: state.agent_context_window(),
                                    verify_edits: state.verify_edits,
                            prompt: distill_prompt,
                        });
                    }
                    continue;
                }
                if input == "/skills" {
                    let _ = cmd_tx.send(WorkerCmd::Skills);
                    continue;
                }
                if input == "/mcp" {
                    let _ = cmd_tx.send(WorkerCmd::McpStatus);
                    continue;
                }
                if input == "/export" {
                    state.modal = ActiveModal::ExportPicker;
                    state.modal_idx = 0;
                    continue;
                }
                if let Some(arg) = input.strip_prefix("/export ") {
                    let parts: Vec<&str> = arg.split_whitespace().collect();
                    let first = parts.first().copied().unwrap_or("md");
                    let (fmt, explicit_path) = if first.eq_ignore_ascii_case("html")
                        || first.eq_ignore_ascii_case("htm")
                    {
                        ("html", parts.get(1).copied())
                    } else if first.eq_ignore_ascii_case("md")
                        || first.eq_ignore_ascii_case("markdown")
                    {
                        ("md", parts.get(1).copied())
                    } else if first.ends_with(".html") || first.ends_with(".htm") {
                        ("html", Some(first))
                    } else {
                        ("md", Some(first))
                    };
                    let ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let out_path = explicit_path
                        .map(ToString::to_string)
                        .unwrap_or_else(|| format!("le-vibe-session-{ts}.{fmt}"));
                    let content = if fmt == "html" {
                        export_session_html(&state)
                    } else {
                        export_session_markdown(&state)
                    };
                    match fs::write(&out_path, content) {
                        Ok(()) => state.blocks.push(TranscriptBlock::System(format!(
                            "Exported session ({fmt}) to {out_path}"
                        ))),
                        Err(err) => state.blocks.push(TranscriptBlock::System(format!(
                            "Export failed ({out_path}): {err}"
                        ))),
                    }
                    continue;
                }
                if input == "/copy" || input.starts_with("/copy ") {
                    let arg = input.strip_prefix("/copy").unwrap_or("").trim().to_ascii_lowercase();
                    let last_msg = state.blocks.iter().rev().find_map(|b| match b {
                        TranscriptBlock::Assistant(txt) if !txt.trim().is_empty() => Some(txt.clone()),
                        _ => None,
                    });
                    if let Some(txt) = last_msg {
                        let (label, copied, char_len) = match arg.as_str() {
                            "chat" | "plain" | "whatsapp" | "teams" => {
                                let plain = format_chat_plain(&txt);
                                let html = format_chat_html(&txt);
                                let ok = copy_to_clipboard_dual(&plain, html.as_deref());
                                ("chat-friendly", ok, plain.len())
                            }
                            "code" => {
                                if let Some(code) = extract_last_code_block(&txt) {
                                    let ok = copy_to_clipboard(&code);
                                    ("code block", ok, code.len())
                                } else {
                                    state.blocks.push(TranscriptBlock::System(
                                        "No fenced code block in the last assistant response.".to_string(),
                                    ));
                                    continue;
                                }
                            }
                            _ => {
                                let ok = copy_to_clipboard(&txt);
                                ("Markdown", ok, txt.len())
                            }
                        };
                        if copied {
                            state.status_text = Some(format!(
                                "Copied ({label}, {char_len} chars) to clipboard."
                            ));
                            state.blocks.push(TranscriptBlock::System(format!(
                                "Copied ({label}, {char_len} chars) to clipboard."
                            )));
                        } else {
                            state.blocks.push(TranscriptBlock::System(
                                "Clipboard copy failed (Set-Clipboard / pbcopy / xclip unavailable).".to_string(),
                            ));
                        }
                    } else {
                        state.blocks.push(TranscriptBlock::System(
                            "No assistant response to copy.".to_string(),
                        ));
                    }
                    continue;
                }
                if input == "/help" {
                    state.help_scroll = 0;
                    state.modal = ActiveModal::HelpModal;
                    continue;
                }
                if input == "/trust" {
                    match trust_cwd_in_policy() {
                        Ok(p) => {
                            state.blocks.push(TranscriptBlock::System(format!(
                                "Trusted cwd in {}; reloading policy...",
                                p.display()
                            )));
                            let _ = cmd_tx.send(WorkerCmd::RefreshCatalog {
                                live: false,
                                announce: true,
                                provider: state.provider.clone(),
                                model: state.model.clone(),
                            });
                        }
                        Err(err) => state.blocks.push(TranscriptBlock::System(format!(
                            "Failed to update policy.json: {err}"
                        ))),
                    }
                    continue;
                }
                if input == "/changelog" {
                    let path = std::path::PathBuf::from(self_repo_path()).join("change-log.md");
                    state.changelog_lines = fs::read_to_string(&path)
                        .map(|s| s.lines().map(ToString::to_string).collect())
                        .unwrap_or_else(|_| vec!["change-log.md not found.".to_string()]);
                    state.help_scroll = 0;
                    state.modal = ActiveModal::ChangelogModal;
                    continue;
                }
                if input == "/model" {
                    state.scope_view = true;
                    state.modal_filter.clear();
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
                        active_prompt = true;
                        cancel.store(false, Ordering::SeqCst);
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
                        active_prompt = true;
                        cancel.store(false, Ordering::SeqCst);
                        state.status_text = Some(format!("Running ! {cmd}..."));
                        let _ = cmd_tx.send(WorkerCmd::RunShell {
                            command: cmd,
                            shell: state.shell.clone(),
                            include_in_context: true,
                        });
                    }
                    continue;
                }

                // Dispatch prompt templates and dynamic plugin/extension slash commands; reject unknown /commands locally
                if input.starts_with('/') {
                    let mut parts = input.splitn(2, char::is_whitespace);
                    let cmd_name = parts.next().unwrap_or("");
                    let cmd_args = parts.next().unwrap_or("").trim().to_string();
                    if let Some(tpl) = prompt_templates.iter().find(|t| t.command == cmd_name) {
                        let expanded = expand_prompt_template(&tpl.body, &cmd_args);
                        state.blocks.push(TranscriptBlock::User(expanded.clone()));
                        state.is_busy = true;
                        active_prompt = true;
                        cancel.store(false, Ordering::SeqCst);
                        state.scroll_offset = 0;
                        state.active_started = Some(Instant::now());
                        state.status_text = Some(format!("Working ({}/{})...", state.provider, state.model));
                        let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                            provider: state.provider.clone(),
                            model: state.model.clone(),
                            thinking_level: state.thinking_level.clone(),
                            mode: state.mode,
                            mode_id: state.active_mode_name().to_string(),
                            shell: state.shell.clone(),
                            max_turns: state.max_turns,
                            retry_max: state.retry_max,
                            context_window: state.agent_context_window(),
                                    verify_edits: state.verify_edits,
                            prompt: expanded,
                        });
                    } else if state
                        .plugin_commands
                        .iter()
                        .any(|(name, _)| name == cmd_name)
                    {
                        let _ = cmd_tx.send(WorkerCmd::RunPluginCommand {
                            name: cmd_name.to_string(),
                            args: cmd_args,
                        });
                    } else {
                        state.blocks.push(TranscriptBlock::System(format!(
                            "Unknown command {cmd_name} — type /help for available commands."
                        )));
                    }
                    continue;
                }

                let mut prompt_text = input;
                if !state.collapsed_pastes.is_empty() {
                    for (pill, full) in state.collapsed_pastes.drain(..) {
                        if prompt_text.contains(&pill) {
                            prompt_text = prompt_text.replace(&pill, &full);
                        }
                    }
                }
                if !state.pending_attachments.is_empty() {
                    let mut att_blocks = Vec::new();
                    for (name, mime, b64) in state.pending_attachments.drain(..) {
                        att_blocks.push(format!(
                            "[attached_image name=\"{name}\" mime=\"{mime}\" data=\"data:{mime};base64,{b64}\"]"
                        ));
                    }
                    prompt_text = format!("{}\n\n{prompt_text}", att_blocks.join("\n"));
                }
                let input = prompt_text;

                // Threshold compaction is owned by `le-vibe-agent` (pi's reserve rule); the CLI
                // only passes the window and lets the agent decide.
                state.blocks.push(TranscriptBlock::User(input.clone()));
                state.is_busy = true;
                active_prompt = true;
                cancel.store(false, Ordering::SeqCst);
                state.scroll_offset = 0;
                state.active_started = Some(Instant::now());
                state.status_text = Some(format!("Working ({}/{})...", state.provider, state.model));
                let _ = cmd_tx.send(WorkerCmd::RunPrompt {
                    provider: state.provider.clone(),
                    model: state.model.clone(),
                    thinking_level: state.thinking_level.clone(),
                    mode: state.mode,
                    mode_id: state.active_mode_name().to_string(),
                    shell: state.shell.clone(),
                    max_turns: state.max_turns,
                    retry_max: state.retry_max,
                    context_window: state.agent_context_window(),
                                    verify_edits: state.verify_edits,
                    prompt: input,
                });
            }
        }
        }
        if should_quit {
            break;
        }
    }
}


/// Insert a pasted image, file path, or text block into the editor using Pi-style `[...]` pills
/// so multi-line blocks, files, and images never flood the input box or auto-submit.
fn apply_pasted_item(state: &mut TuiState, item: PastedItem) {
    match item {
        PastedItem::Image { name, mime, base64 } => {
            state.pending_attachments.push((name.clone(), mime, base64));
            let idx = state.pending_attachments.len();
            let pill = format!("[image #{idx}: {name}] ");
            state.input.insert_str(state.cursor, &pill);
            state.cursor += pill.len();
            state.selected_slash_idx = 0;
        }
        PastedItem::ImageTooLarge(msg) => {
            state
                .blocks
                .push(TranscriptBlock::System(format!("Image skipped: {msg}")));
        }
        PastedItem::FilePath(path) => {
            let pill = format!("[file: {path}]");
            let ins = format!("{pill} ");
            state.collapsed_pastes.push((pill, format!("@{path}")));
            state.input.insert_str(state.cursor, &ins);
            state.cursor += ins.len();
            state.selected_slash_idx = 0;
        }
        PastedItem::PlainText(txt) => {
            let norm = txt.replace("\r\n", "\n").replace('\r', "\n");
            let line_count = norm.lines().count();
            if line_count >= 4 || norm.chars().count() > 300 {
                let idx = state.collapsed_pastes.len() + 1;
                let pill = format!("[paste #{idx} +{line_count} lines]");
                let ins = format!("{pill} ");
                state.collapsed_pastes.push((pill, norm));
                state.input.insert_str(state.cursor, &ins);
                state.cursor += ins.len();
            } else {
                state.input.insert_str(state.cursor, &norm);
                state.cursor += norm.len();
            }
            state.selected_slash_idx = 0;
        }
    }
}

fn copy_to_clipboard(text: &str) -> bool {
    copy_to_clipboard_dual(text, None)
}

fn copy_to_clipboard_dual(plain: &str, html: Option<&str>) -> bool {
    if cfg!(target_os = "windows")
        && let Some(html_frag) = html
    {
        // Dual-MIME Windows clipboard: Teams/Notion paste real HTML tables while WhatsApp gets clean plain text.
        let header_stub = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
        let prefix = "<html><body><!--StartFragment-->";
        let suffix = "<!--EndFragment--></body></html>";
        let start_html = header_stub.len();
        let start_frag = start_html + prefix.len();
        let end_frag = start_frag + html_frag.len();
        let end_html = end_frag + suffix.len();
        let cf_html = format!(
            "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_frag:010}\r\nEndFragment:{end_frag:010}\r\n{prefix}{html_frag}{suffix}"
        );
        let payload = format!("{plain}\0\0{cf_html}");
        let script = "[Console]::InputEncoding = [System.Text.Encoding]::UTF8; $raw = [Console]::In.ReadToEnd(); $parts = $raw -split \"`0`0\", 2; Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.DataObject; $d.SetText($parts[0], [System.Windows.Forms.TextDataFormat]::UnicodeText); if ($parts.Length -gt 1) { $d.SetData([System.Windows.Forms.DataFormats]::Html, $parts[1]) }; [System.Windows.Forms.Clipboard]::SetDataObject($d, $true)";
        if let Ok(mut child) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(payload.as_bytes());
            }
            if child.wait().is_ok_and(|s| s.success()) {
                return true;
            }
        }
    }

    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "windows") {
        &[(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "[Console]::InputEncoding = [System.Text.Encoding]::UTF8; $t = [Console]::In.ReadToEnd(); Set-Clipboard -Value $t",
            ],
        )]
    } else if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else {
        &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])]
    };
    for (bin, args) in candidates {
        if let Ok(mut child) = std::process::Command::new(bin)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(plain.as_bytes());
            }
            if child.wait().is_ok_and(|s| s.success()) {
                return true;
            }
        }
    }
    // OSC 52 universal terminal clipboard fallback (works over SSH/tmux without local tools).
    let b64 = encode_base64(plain.as_bytes());
    let mut out = io::stdout();
    write!(out, "\x1b]52;c;{b64}\x07").is_ok() && out.flush().is_ok()
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(TABLE[(b0 >> 2) & 0x3F] as char);
        out.push(TABLE[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((b1 & 0x0F) << 2) | (b2 >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[b2 & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::worker::{parse_iso_utc, quota_bar, resets_in};

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
