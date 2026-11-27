// `le-vibe tui-probe`: drive the real TUI state machine headlessly and print rendered frames as
// text, so agents (and tests) can inspect layout, scrolling, modals and input handling without a TTY.
use std::io::{self, Read, Write};

use le_vibe_tui::{
    ActiveModal, AgentMode, QuestionItem, QuestionOption, QuestionSession, ThemeId, ToolCard,
    TranscriptBlock, TuiAction, TuiState, apply_wheel_scroll, press, render_frame,
};

/// What the probe should do with one line of the script.
enum Step {
    Key(String),
    Type(String),
    Wheel(bool),
    Resize(u16, u16),
    Modal(ActiveModal),
    Busy(bool),
    Queue(String),
    Steer(String),
    Frame,
    State,
}

struct ProbeOptions {
    width: u16,
    height: u16,
    script: Option<String>,
    keys: Vec<String>,
    demo: bool,
    provider: String,
    model: String,
    theme: Option<String>,
    mode: Option<String>,
    json: bool,
}

const DEFAULT_WIDTH: u16 = 100;
const DEFAULT_HEIGHT: u16 = 30;

/// Run the probe, returning a process exit code.
pub(crate) fn run(args: &[String]) -> i32 {
    let Some(opts) = parse_options(args) else {
        eprintln!("{}", usage());
        return 2;
    };

    let mut state = TuiState::new(
        opts.provider.clone(),
        opts.model.clone(),
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string()),
        Some("main".to_string()),
    );
    state.repo_name = "tui-probe".to_string();
    state.git_badges = "probe".to_string();
    if let Some(name) = &opts.theme {
        match ThemeId::from_name(name) {
            Some(theme) => state.theme = theme,
            None => {
                eprintln!("Unknown theme: {name}");
                return 2;
            }
        }
    }
    if let Some(name) = &opts.mode {
        match AgentMode::from_name(name) {
            Some(mode) => state.mode = mode,
            None => {
                eprintln!("Unknown mode: {name}");
                return 2;
            }
        }
    }
    if opts.demo {
        seed_demo(&mut state);
    }

    let script = match load_script(&opts) {
        Ok(script) => script,
        Err(err) => {
            eprintln!("{err}");
            return 2;
        }
    };

    let (mut width, mut height) = (opts.width, opts.height);
    let mut last_was_frame = false;
    let mut step_no = 0usize;

    for raw in script.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        step_no += 1;
        let Some(step) = parse_step(line) else {
            eprintln!("Unparsed script line {step_no}: {line}");
            return 2;
        };
        match step {
            Step::Key(spec) => match press(&mut state, &spec) {
                Ok(Some(action)) => emit_action(&mut state, &action, opts.json),
                Ok(None) => {}
                Err(err) => {
                    eprintln!("Key {spec} failed: {err}");
                    return 1;
                }
            },
            Step::Type(text) => {
                for ch in text.chars() {
                    let spec = ch.to_string();
                    match press(&mut state, &spec) {
                        Ok(Some(action)) => emit_action(&mut state, &action, opts.json),
                        Ok(None) => {}
                        Err(err) => {
                            eprintln!("Typing failed: {err}");
                            return 1;
                        }
                    }
                }
            }
            Step::Wheel(up) => {
                apply_wheel_scroll(&mut state, up);
            }
            Step::Resize(w, h) => {
                width = w;
                height = h;
            }
            Step::Modal(modal) => {
                seed_modal_data(&mut state, &modal);
                state.modal = modal;
                state.modal_idx = 0;
            }
            Step::Busy(on) => {
                state.is_busy = on;
                state.status_text = on.then(|| "Worker is thinking".to_string());
            }
            Step::Queue(text) => {
                state.queued_prompts.push_back(text);
            }
            Step::Steer(text) => {
                state.steering_prompts.push_back(text);
            }
            Step::Frame => {
                print_frame(&state, width, height, opts.json);
                last_was_frame = true;
            }
            Step::State => {
                print_state(&state, opts.json);
                last_was_frame = false;
            }
        }
    }

    if !last_was_frame {
        print_frame(&state, width, height, opts.json);
    }
    0
}

fn usage() -> &'static str {
    "Usage: le-vibe tui-probe [--size 100x30] [--script <file|->] [--key <spec>]... [--demo]
                          [--theme <name>] [--mode <name>] [--provider <p>] [--model <m>]
                          [--json]

Script lines (one step per line, `#` starts a comment):
  type <text>        type text into the prompt as individual key presses
  key <spec>         one key: Enter, Esc, Tab, Up, Down, Left, Right, PageUp, PageDown,
                     Home, End, Backspace, Delete, Space, F1..F12, Ctrl+G, Alt+Up, a
  wheel up|down      one mouse wheel notch
  resize <WxH>       change the probed frame size
  modal <name>       force a modal open: model, mode, theme, term, config, login, logout,
                     usage, approval, trust, question, export, rewind, session, help,
                     changelog, debug, info, none
  busy on|off        set the busy (spinner) state
  queue <text>       add a queued follow-up prompt
  steer <text>       add a steering prompt
  frame              print the rendered frame
  state              print the UI state summary"
}

fn parse_options(args: &[String]) -> Option<ProbeOptions> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return None;
    }
    let mut opts = ProbeOptions {
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        script: None,
        keys: Vec::new(),
        demo: false,
        provider: "vibe".to_string(),
        model: "mistral-large-4-0".to_string(),
        theme: None,
        mode: None,
        json: false,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--size" => {
                let (w, h) = parse_size(args.get(i + 1)?)?;
                opts.width = w;
                opts.height = h;
                i += 2;
            }
            "--script" => {
                opts.script = Some(args.get(i + 1)?.clone());
                i += 2;
            }
            "--key" => {
                opts.keys.push(args.get(i + 1)?.clone());
                i += 2;
            }
            "--theme" => {
                opts.theme = Some(args.get(i + 1)?.clone());
                i += 2;
            }
            "--mode" => {
                opts.mode = Some(args.get(i + 1)?.clone());
                i += 2;
            }
            "--provider" => {
                opts.provider = args.get(i + 1)?.clone();
                i += 2;
            }
            "--model" => {
                opts.model = args.get(i + 1)?.clone();
                i += 2;
            }
            "--demo" => {
                opts.demo = true;
                i += 1;
            }
            "--json" => {
                opts.json = true;
                i += 1;
            }
            other => {
                eprintln!("Unknown tui-probe flag: {other}");
                return None;
            }
        }
    }
    Some(opts)
}

fn parse_size(spec: &str) -> Option<(u16, u16)> {
    let (w, h) = spec.split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn load_script(opts: &ProbeOptions) -> Result<String, String> {
    let mut script = match opts.script.as_deref() {
        Some("-") => {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|err| format!("Failed to read script from stdin: {err}"))?;
            buf
        }
        Some(path) => {
            std::fs::read_to_string(path).map_err(|err| format!("Failed to read {path}: {err}"))?
        }
        None => String::new(),
    };
    for key in &opts.keys {
        script.push_str("key ");
        script.push_str(key);
        script.push('\n');
    }
    Ok(script)
}

fn parse_step(line: &str) -> Option<Step> {
    let (verb, rest) = match line.trim().split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb, rest.trim()),
        None => (line.trim(), ""),
    };
    match verb {
        "type" => Some(Step::Type(rest.to_string())),
        "key" => Some(Step::Key(rest.to_string())),
        "wheel" => match rest {
            "up" => Some(Step::Wheel(true)),
            "down" => Some(Step::Wheel(false)),
            _ => None,
        },
        "resize" => {
            let spec = rest.replace(' ', "x");
            parse_size(&spec).map(|(w, h)| Step::Resize(w, h))
        }
        "modal" => modal_from_name(&rest.to_ascii_lowercase()).map(Step::Modal),
        "busy" => match rest {
            "on" | "true" => Some(Step::Busy(true)),
            "off" | "false" => Some(Step::Busy(false)),
            _ => None,
        },
        "queue" => Some(Step::Queue(rest.to_string())),
        "steer" => Some(Step::Steer(rest.to_string())),
        "frame" => Some(Step::Frame),
        "state" => Some(Step::State),
        _ => None,
    }
}

fn emit_action(state: &mut TuiState, action: &TuiAction, json: bool) {
    let label = action_label(action);
    let keep = match action {
        TuiAction::Submit(prompt) | TuiAction::SubmitFollowUp(prompt) => {
            state.blocks.push(TranscriptBlock::User(prompt.clone()));
            state.scroll_offset = 0;
            true
        }
        TuiAction::Interrupt => {
            state.is_busy = false;
            state.status_text = Some("Interrupted".to_string());
            true
        }
        TuiAction::Quit => {
            state.status_text = Some("Quit requested".to_string());
            true
        }
        TuiAction::OpenExternalEditor => {
            state
                .status_text
                .replace("External editor requested".to_string());
            true
        }
        TuiAction::ApprovalDecision(_) | TuiAction::QuestionAnswer(_) | TuiAction::TrustDecision(_) => {
            true
        }
        _ => false,
    };
    if json {
        let payload = serde_json::json!({ "action": label, "applied": keep });
        println!("{payload}");
    } else if keep {
        println!("[action] {label}");
    }
    let _ = io::stdout().flush();
}

fn print_frame(state: &TuiState, width: u16, height: u16, json: bool) {
    let frame = render_frame(state, width, height);
    if json {
        let payload = serde_json::json!({
            "frame": frame.text,
            "width": width,
            "height": height,
            "cursor": frame.cursor,
            "state": state_json(state),
        });
        println!("{payload}");
    } else {
        print!("{}", frame.text);
        if let Some((x, y)) = frame.cursor {
            println!("[caret] col={} row={}", x + 1, y + 1);
        }
    }
    let _ = io::stdout().flush();
}

fn print_state(state: &TuiState, json: bool) {
    if json {
        println!("{}", serde_json::json!({ "state": state_json(state) }));
    } else {
        println!("[state] {}", state_line(state));
    }
    let _ = io::stdout().flush();
}

fn state_json(state: &TuiState) -> serde_json::Value {
    serde_json::json!({
        "modal": modal_name(&state.modal),
        "input": state.input,
        "cursor": state.cursor,
        "scroll": state.scroll_offset,
        "rendered_scroll": state.rendered_scroll.get(),
        "busy": state.is_busy,
        "queued": state.queued_prompts.len(),
        "steering": state.steering_prompts.len(),
        "blocks": state.blocks.len(),
        "todos": state.todos.len(),
        "theme": state.theme.name(),
        "mode": state.mode.name(),
        "status": state.status_text,
    })
}

fn state_line(state: &TuiState) -> String {
    format!(
        "modal={} input={:?} cursor={} scroll={} rendered_scroll={} busy={} queued={} steering={} blocks={} theme={} mode={} status={:?}",
        modal_name(&state.modal),
        state.input,
        state.cursor,
        state.scroll_offset,
        state.rendered_scroll.get(),
        state.is_busy,
        state.queued_prompts.len(),
        state.steering_prompts.len(),
        state.blocks.len(),
        state.theme.name(),
        state.mode.name(),
        state.status_text,
    )
}

fn modal_name(modal: &ActiveModal) -> &'static str {
    match modal {
        ActiveModal::None => "none",
        ActiveModal::ModelPicker => "model",
        ActiveModal::ModePicker => "mode",
        ActiveModal::WorkflowPicker => "workflow",
        ActiveModal::ThemePicker => "theme",
        ActiveModal::TermPicker => "term",
        ActiveModal::ConfigModal => "config",
        ActiveModal::LoginPicker => "login",
        ActiveModal::LogoutPicker => "logout",
        ActiveModal::UsageModal => "usage",
        ActiveModal::ApprovalModal => "approval",
        ActiveModal::TrustModal => "trust",
        ActiveModal::QuestionModal => "question",
        ActiveModal::ExportPicker => "export",
        ActiveModal::RewindPicker => "rewind",
        ActiveModal::SessionPicker => "session",
        ActiveModal::HelpModal => "help",
        ActiveModal::ChangelogModal => "changelog",
        ActiveModal::DebugModal => "debug",
        ActiveModal::InfoModal => "info",
    }
}

/// One-line label for an action, used in probe traces (TuiAction carries no Debug impl).
fn action_label(action: &TuiAction) -> String {
    match action {
        TuiAction::None => "none".to_string(),
        TuiAction::Redraw => "redraw".to_string(),
        TuiAction::Submit(prompt) => format!("submit {prompt:?}"),
        TuiAction::SubmitFollowUp(prompt) => format!("submit-followup {prompt:?}"),
        TuiAction::RecallQueued => "recall-queued".to_string(),
        TuiAction::OpenExternalEditor => "open-editor".to_string(),
        TuiAction::CopyLastReply => "copy-last-reply".to_string(),
        TuiAction::TrustDecision(allow) => format!("trust {allow}"),
        TuiAction::Interrupt => "interrupt".to_string(),
        TuiAction::CycleModel => "cycle-model".to_string(),
        TuiAction::SaveScope => "save-scope".to_string(),
        TuiAction::ConfigCycleKey(key) => format!("config-cycle {key}"),
        TuiAction::ApprovalDecision(idx) => format!("approval {idx}"),
        TuiAction::QuestionAnswer(answer) => format!("answer {answer:?}"),
        TuiAction::ToggleDebug => "toggle-debug".to_string(),
        TuiAction::DebugClear => "debug-clear".to_string(),
        TuiAction::DebugCycleFilter => "debug-cycle-filter".to_string(),
        TuiAction::PasteClipboard => "paste-clipboard".to_string(),
        TuiAction::Pasted(text) => format!("pasted {} bytes", text.len()),
        TuiAction::Quit => "quit".to_string(),
    }
}

fn modal_from_name(name: &str) -> Option<ActiveModal> {
    Some(match name {
        "none" => ActiveModal::None,
        "model" => ActiveModal::ModelPicker,
        "mode" => ActiveModal::ModePicker,
        "workflow" => ActiveModal::WorkflowPicker,
        "theme" => ActiveModal::ThemePicker,
        "term" => ActiveModal::TermPicker,
        "config" => ActiveModal::ConfigModal,
        "login" => ActiveModal::LoginPicker,
        "logout" => ActiveModal::LogoutPicker,
        "usage" => ActiveModal::UsageModal,
        "approval" => ActiveModal::ApprovalModal,
        "trust" => ActiveModal::TrustModal,
        "question" => ActiveModal::QuestionModal,
        "export" => ActiveModal::ExportPicker,
        "rewind" => ActiveModal::RewindPicker,
        "session" => ActiveModal::SessionPicker,
        "help" => ActiveModal::HelpModal,
        "changelog" => ActiveModal::ChangelogModal,
        "debug" => ActiveModal::DebugModal,
        "info" => ActiveModal::InfoModal,
        _ => return None,
    })
}

/// Give a modal the data it needs so its layout can be inspected.
fn seed_modal_data(state: &mut TuiState, modal: &ActiveModal) {
    match modal {
        ActiveModal::ApprovalModal => {
            state.approval_prompt = Some((
                "edit".to_string(),
                "crates/le-vibe-tui/src/transcript.rs\n- let width = 80;\n+ let width = area.width;"
                    .to_string(),
                "requires approval: writing outside the workspace allowlist".to_string(),
            ));
            // Opened earlier than the 400ms typing grace period so keys count.
            state.approval_opened_at = Some(
                std::time::Instant::now()
                    - std::time::Duration::from_secs(5),
            );
        }
        ActiveModal::QuestionModal => {
            state.question_prompt = Some(QuestionSession {
                items: vec![QuestionItem {
                    header: "Approach".to_string(),
                    question: "Which refactor should land first?".to_string(),
                    multi_select: false,
                    options: vec![
                        QuestionOption {
                            label: "wrap tokens".to_string(),
                            description: "Rewrite the wrapper around tokens".to_string(),
                            preview: Some("fn wrap_tokens() {\n  // clean\n}".to_string()),
                        },
                        QuestionOption {
                            label: "post-pass".to_string(),
                            description: "Fix wrapping after the fact".to_string(),
                            preview: None,
                        },
                    ],
                    selected: vec![false, false],
                    custom_text: String::new(),
                    answer: None,
                }],
                active_idx: 0,
            });
        }
        ActiveModal::TrustModal => {
            state.trust_prompt_path = state.cwd.clone();
            state.trust_prompt_resources = vec![
                ".le-vibe/extensions/audit_logger.py".to_string(),
                ".le-vibe/policy.json".to_string(),
            ];
        }
        ActiveModal::InfoModal => {
            state.info_modal_title = "Probe info".to_string();
            state.info_modal_lines = vec![
                "Line one of the info modal.".to_string(),
                "Line two of the info modal.".to_string(),
            ];
        }
        ActiveModal::DebugModal => {
            state.debug_lines = vec![
                "ui tui_start provider=vibe model=mistral-large-4-0".to_string(),
                "worker stream_tick tokens=128".to_string(),
            ];
        }
        _ => {}
    }
}

/// Fill the state with representative content so a probe run shows the full surface.
fn seed_demo(state: &mut TuiState) {    state.blocks.push(TranscriptBlock::User(
        "Refactor the transcript renderer so wrapped bullets keep their hanging indent.".to_string(),
    ));
    state.blocks.push(TranscriptBlock::Thinking(
        "Wrapping splits spans; a bullet prefix must only apply to the first row.".to_string(),
    ));
    state.blocks.push(TranscriptBlock::Tool(
        ToolCard::new(
            "read",
            r#"{"path":"crates/le-vibe-tui/src/transcript.rs","offset":1,"limit":120}"#,
        )
        .with_status("done")
        .with_output("crates/le-vibe-tui/src/transcript.rs:1-120")
        .with_duration(420),
    ));
    state.blocks.push(TranscriptBlock::Assistant(
        "## Wrap changes\n\n- `wrap_lines_exact` measures each span with `UnicodeWidthStr`\n- cross-span tokens keep `(` attached to styled code such as `foo(bar)`\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n1. first numbered item that is long enough to wrap across two rows in a narrow frame\n2. second item\n"
            .to_string(),
    ));
    state.blocks.push(TranscriptBlock::Report(
        "rows  width  overflow\n  12    100         no".to_string(),
    ));
    state.todos = vec![
        (1, "Verify wrapping".to_string(), "completed".to_string()),
        (2, "Verify scrollbar".to_string(), "in_progress".to_string()),
    ];
    state.available_models = vec![
        (
            "vibe".to_string(),
            "mistral-large-4-0".to_string(),
            "Mistral Large 4".to_string(),
        ),
        (
            "agy".to_string(),
            "gemini-4-neon".to_string(),
            "Gemini 4 Neon".to_string(),
        ),
    ];
    state.model_default_thinking.insert(
        ("vibe".to_string(), "mistral-large-4-0".to_string()),
        "medium".to_string(),
    );
    state.input_tokens = 12_480;
    state.output_tokens = 1_930;
    state.cache_read_tokens = 8_200;
    state.context_window = 256_000;
    state.cost_usd = 0.042;
    state.usage_lines = vec![
        "vibe / mistral-large-4-0  12.5k in  1.9k out  $0.0420".to_string(),
        "agy / gemini-4-neon       0 in      0 out     $0.0000".to_string(),
    ];
    state.session_rows = vec![(
        "probe-session.jsonl".to_string(),
        "12 entries".to_string(),
        "refactor transcript".to_string(),
    )];
    state.rewind_rows = vec![(
        "msg_1".to_string(),
        "#1 user".to_string(),
        "refactor the transcript renderer".to_string(),
    )];
    state.history = vec![
        "explain the wrap token logic".to_string(),
        "run the tui tests".to_string(),
    ];
    state.workspace_files = vec![
        "crates/le-vibe-tui/src/transcript.rs".to_string(),
        "crates/le-vibe-tui/src/input.rs".to_string(),
    ];
    state.loaded_resources = vec!["AGENTS.md".to_string()];
    state.plugin_commands = vec![("/review".to_string(), "review the working tree".to_string())];
    state.collapsed_pastes = vec![(
        "[paste #1, 40 lines]".to_string(),
        "long pasted block".to_string(),
    )];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_script_steps() {
        assert!(
            matches!(parse_step("type hello world"), Some(Step::Type(text)) if text == "hello world")
        );
        assert!(matches!(parse_step("key Ctrl+G"), Some(Step::Key(key)) if key == "Ctrl+G"));
        assert!(matches!(parse_step("wheel up"), Some(Step::Wheel(true))));
        assert!(matches!(parse_step("resize 80x24"), Some(Step::Resize(80, 24))));
        assert!(matches!(parse_step("resize 80 24"), Some(Step::Resize(80, 24))));
        assert!(matches!(
            parse_step("modal approval"),
            Some(Step::Modal(ActiveModal::ApprovalModal))
        ));
        assert!(matches!(parse_step("busy on"), Some(Step::Busy(true))));
        assert!(matches!(parse_step("queue later"), Some(Step::Queue(text)) if text == "later"));
        assert!(matches!(parse_step("frame"), Some(Step::Frame)));
        assert!(matches!(parse_step("state"), Some(Step::State)));
        assert!(parse_step("nonsense step").is_none());
    }
}
