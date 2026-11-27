use le_vibe_tui::{ActiveModal, QuestionSession, TranscriptBlock, TuiState, draw_frame};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn render_at(state: &TuiState, width: u16, height: u16) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| draw_frame(frame, state))
        .expect("draw_frame should not panic");
}

/// Render to a plain-text grid so assertions can look for exact strings.
fn render_text(state: &TuiState, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| draw_frame(frame, state))
        .expect("draw_frame should not panic");
    let buf = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buf[(x, y)].symbol());
            }
            line.trim_end().to_string()
        })
        .collect()
}

fn base_state() -> TuiState {
    let mut state = TuiState::new(
        "vibe".into(),
        "mistral-large-4-0".into(),
        "~/le-vibe".into(),
        Some("main".into()),
    );
    state.reduced_motion = true;
    state
}

#[test]
fn renders_all_modals_at_minimum_and_desktop_sizes() {
    let mut state = TuiState::new(
        "vibe".into(),
        "mistral-large-4-0".into(),
        "C:/repo".into(),
        Some("main".into()),
    );
    state.available_models = vec![
        ("vibe".into(), "mistral-large-4-0".into(), "Mistral Large 4".into()),
        ("agy".into(), "gemini-4-neon".into(), "Gemini 4 Neon".into()),
    ];
    state.blocks.push(TranscriptBlock::User("hello".into()));
    state.blocks.push(TranscriptBlock::Thinking("thinking step".into()));
    state.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new("read", r#"{"path":"a.rs"}"#)
            .with_status("done")
            .with_output("1: fn main() {}"),
    ));
    state.blocks.push(TranscriptBlock::Assistant(
        "# Heading\n\n- bullet `code`\n\n```rust\nfn ok() {}\n```\n".into(),
    ));
    state.todos = vec![(1, "Verify TUI".into(), "in_progress".into())];
    state.approval_prompt = Some((
        "edit".into(),
        "crates/a.rs\n- old\n+ new".into(),
        "requires approval".into(),
    ));
    state.question_prompt = Some(QuestionSession::single(
        "Approach".into(),
        "Which option?".into(),
        vec![("A".into(), "First".into()), ("B".into(), "Second".into())],
    ));
    state.rewind_rows = vec![("msg_1".into(), "#1 user".into(), "hello".into())];
    state.session_rows = vec![("session-1.jsonl".into(), "1 entry".into(), "hello".into())];
    state.changelog_lines = vec!["# Changelog".into(), "- v0.1.0".into()];

    let modals = [
        ActiveModal::None,
        ActiveModal::ModelPicker,
        ActiveModal::ModePicker,
        ActiveModal::ThemePicker,
        ActiveModal::TermPicker,
        ActiveModal::ConfigModal,
        ActiveModal::LoginPicker,
        ActiveModal::LogoutPicker,
        ActiveModal::UsageModal,
        ActiveModal::ApprovalModal,
        ActiveModal::QuestionModal,
        ActiveModal::ExportPicker,
        ActiveModal::RewindPicker,
        ActiveModal::SessionPicker,
        ActiveModal::HelpModal,
        ActiveModal::ChangelogModal,
        ActiveModal::DebugModal,
        ActiveModal::InfoModal,
        ActiveModal::TrustModal,
    ];

    for modal in modals {
        state.modal = modal;
        render_at(&state, 40, 12);
        render_at(&state, 140, 40);
    }

    // Slash popup open + long wrapped input
    state.modal = ActiveModal::None;
    state.input = "/".into();
    state.cursor = 1;
    render_at(&state, 40, 12);
    render_at(&state, 140, 40);

    state.input = "a".repeat(200);
    state.cursor = state.input.len();
    render_at(&state, 40, 12);
    render_at(&state, 140, 40);
}

#[test]
fn multi_width_golden_layout_zero_overflow() {
    let mut state = TuiState::new(
        "vibe".into(),
        "mistral-large-4-0".into(),
        "~/le-vibe".into(),
        Some("main".into()),
    );
    state.reduced_motion = true;
    state.status_text = Some("Working (vibe/mistral-large-4-0)...".into());
    state.steering_prompts.push_back("Actually switch to pure Rust for this".into());
    state.queued_prompts.push_back("Then run cargo test".into());
    state.trust_prompt_path = "C:/Users/demo/repo".into();
    state.trust_prompt_resources = vec![".le-vibe/settings.json".into(), ".le-vibe/mcp.json".into()];
    state.blocks.push(TranscriptBlock::User(
        "Explain the full architecture with a very long prompt that wraps across multiple visual lines cleanly without clipping.".into(),
    ));
    state.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new("bash", r#"{"command":"npm test"}"#)
            .with_status("done")
            .with_output("T\0h\0e\0 \0o\0p\0e\0r\0a\0t\0i\0o\0n\0 \0f\0a\0i\0l\0e\0d\r\n+added line\n-removed line"),
    ));
    state.blocks.push(TranscriptBlock::Assistant(
        "Here is a long wrapped paragraph that exercises exact word wrapping across narrow, medium, and wide terminals so the final sentinel word ALWAYS_VISIBLE is never clipped below the viewport.\n\n```xml\n<mxfile><diagram><mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/><mxCell id=\"2\" value=\"Rust TUI\" vertex=\"1\" parent=\"1\"/><mxCell id=\"3\" value=\"Agent Loop\" vertex=\"1\" parent=\"1\"/><mxCell id=\"4\" value=\"events\" edge=\"1\" source=\"2\" target=\"3\" parent=\"1\"/></root></mxGraphModel></diagram></mxfile>\n```\n\nALWAYS_VISIBLE".into(),
    ));

    for (w, h) in [(60, 20), (120, 35), (180, 50)] {
        state.modal = ActiveModal::None;
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).expect("terminal");
        term.draw(|f| draw_frame(f, &state)).expect("draw");
        let buf = term.backend().buffer();
        let mut rendered = String::new();
        for y in 0..h {
            for x in 0..w {
                rendered.push_str(buf[(x, y)].symbol());
            }
            rendered.push('\n');
        }
        assert!(
            rendered.contains("ALWAYS_VISIBLE"),
            "bottom sentinel clipped at {w}x{h}:\n{rendered}"
        );
        assert!(
            rendered.contains("steer (1)"),
            "steering pill missing at {w}x{h}"
        );

        state.modal = ActiveModal::TrustModal;
        render_at(&state, w, h);
    }
}

// ---------------------------------------------------------------------------
// P0 parity (pi footer / tool cards / status strip)
// ---------------------------------------------------------------------------

/// `read` headers carry the line range and the card carries the tool duration.
#[test]
fn tool_card_shows_read_range_and_duration() {
    let mut state = base_state();
    state.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new("read", r#"{"path":"src/main.rs","offset":10,"limit":30}"#)
            .with_status("done")
            .with_output("fn main() {}")
            .with_duration(4200),
    ));
    let text = render_text(&state, 120, 30).join("\n");
    assert!(
        text.contains("read src/main.rs:10-39"),
        "read header missing line range:\n{text}"
    );
    assert!(text.contains("4.2s"), "tool duration missing:\n{text}");
}

/// Preview budget: 10 lines for generic tools, 5 for shell tools, with pi's hint wording.
#[test]
fn tool_preview_budget_matches_pi() {
    let long: String = (1..=20)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut state = base_state();
    state.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new("grep", r#"{"pattern":"TODO"}"#)
            .with_status("done")
            .with_output(&long),
    ));
    let text = render_text(&state, 120, 40).join("\n");
    assert!(text.contains("line1"), "generic preview should keep the first line:\n{text}");
    assert!(
        !text.contains("line11"),
        "generic preview must stop at 10 lines:\n{text}"
    );
    assert!(
        text.contains("... (10 more lines, ctrl+o to expand)"),
        "generic hidden-line hint wrong:\n{text}"
    );

    let mut shell = base_state();
    shell.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new("bash", r#"{"command":"cargo test"}"#)
            .with_status("done")
            .with_output(&long),
    ));
    let text = render_text(&shell, 120, 40).join("\n");
    assert!(
        text.contains("line20") && !text.contains("line15"),
        "shell preview should keep the last 5 lines (line16-20):\n{text}"
    );
    assert!(
        text.contains("... (15 earlier lines, ctrl+o to expand)"),
        "shell hidden-line hint wrong:\n{text}"
    );
}

/// Collapsed `read` on a SKILL.md collapses to a one-line `[skill]` card with no body.
#[test]
fn compact_read_skill_card_hides_body() {
    let mut state = base_state();
    state.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new(
            "read",
            r#"{"path":"/home/u/.le-vibe/skills/ui-design/SKILL.md"}"#,
        )
        .with_status("done")
        .with_output("SECRET_SKILL_BODY"),
    ));
    let text = render_text(&state, 120, 30).join("\n");
    assert!(text.contains("[skill] ui-design"), "skill card missing:\n{text}");
    assert!(
        !text.contains("SECRET_SKILL_BODY"),
        "collapsed skill read must not print the body:\n{text}"
    );
    assert!(
        text.contains("ctrl+o to expand"),
        "skill card needs an expand hint:\n{text}"
    );
}

/// Footer prints cache read/write counters and omits zero-valued stats (pi footer parity).
#[test]
fn footer_shows_cache_counters_and_omits_zero_stats() {
    let mut state = base_state();
    state.input_tokens = 2100;
    state.output_tokens = 0;
    state.cache_read_tokens = 1200;
    state.cache_write_tokens = 300;
    state.last_cache_rate = Some(93.5);
    state.cost_usd = 0.0;
    let text = render_text(&state, 160, 30).join("\n");
    assert!(text.contains("↑ 2.1k"), "input stat missing:\n{text}");
    assert!(text.contains("R 1.2k"), "cache-read counter missing:\n{text}");
    assert!(text.contains("W 300"), "cache-write counter missing:\n{text}");
    assert!(text.contains("↺ 93.5%"), "cache hit rate missing:\n{text}");
    assert!(!text.contains("↓ 0"), "zero output stat must be omitted:\n{text}");
    assert!(!text.contains("$ 0.000"), "zero cost must be omitted:\n{text}");
}

/// Status strip renders pi's distinct wording for retry / compaction / branch summary.
#[test]
fn status_strip_matches_pi_wording() {
    let mut state = base_state();
    state.is_busy = true;

    state.set_retry(2, 5, 8000);
    let text = render_text(&state, 160, 30).join("\n");
    assert!(text.contains("Retrying (2/5) in"), "retry wording wrong:\n{text}");
    assert!(text.contains("esc to cancel"), "retry cancel hint missing:\n{text}");

    state.set_compaction(le_vibe_tui::CompactionReason::Manual);
    let text = render_text(&state, 160, 30).join("\n");
    assert!(text.contains("Compacting context..."), "manual compaction wording wrong:\n{text}");

    state.set_compaction(le_vibe_tui::CompactionReason::Threshold);
    let text = render_text(&state, 160, 30).join("\n");
    assert!(text.contains("Auto-compacting..."), "threshold compaction wording wrong:\n{text}");

    state.set_compaction(le_vibe_tui::CompactionReason::Overflow);
    let text = render_text(&state, 160, 30).join("\n");
    assert!(
        text.contains("Context overflow detected, Auto-compacting..."),
        "overflow compaction wording wrong:\n{text}"
    );

    state.set_branch_summary();
    let text = render_text(&state, 160, 30).join("\n");
    assert!(text.contains("Summarizing branch..."), "branch summary wording wrong:\n{text}");
}

/// Idle must already reserve the status strip's row so nothing reflows when work starts.
#[test]
fn idle_status_reserves_height() {
    let mut idle = base_state();
    let idle_rows = render_text(&idle, 120, 30);
    let idle_model_row = idle_rows
        .iter()
        .position(|l| l.contains("(vibe) mistral-large-4-0"))
        .expect("model row missing when idle");

    idle.is_busy = true;
    idle.status_text = Some("Working (vibe/mistral-large-4-0)...".into());
    let busy_rows = render_text(&idle, 120, 30);
    let busy_model_row = busy_rows
        .iter()
        .position(|l| l.contains("(vibe) mistral-large-4-0"))
        .expect("model row missing when busy");

    assert_eq!(
        idle_model_row, busy_model_row,
        "footer shifted between idle and busy (status strip is not height-stable)"
    );
}

// ---------------------------------------------------------------------------
// P1 parity (markdown user messages / generic arg previews / edit word-diff)
// ---------------------------------------------------------------------------

/// User turns are rendered as Markdown: ordered markers survive and escaped markup stays literal.
#[test]
fn user_message_renders_markdown_with_escapes() {
    let mut state = base_state();
    state.blocks.push(TranscriptBlock::User(
        "1. first ordered item\n2. second ordered item\n\nUse `code` and \\*literal asterisks\\* here".into(),
    ));
    let text = render_text(&state, 120, 30).join("\n");
    assert!(text.contains("1. first ordered item"), "ordered marker lost:\n{text}");
    assert!(text.contains("2. second ordered item"), "second marker lost:\n{text}");
    assert!(text.contains("code"), "inline code text lost:\n{text}");
    assert!(
        text.contains("*literal asterisks*"),
        "backslash escapes must stay literal:\n{text}"
    );
    assert!(
        !text.contains("\\*"),
        "escaping backslashes must be consumed:\n{text}"
    );
}

/// Unknown/MCP tools fall back to pi's `name key=json ...` preview, and expand with a hanging indent.
#[test]
fn generic_tool_args_preview_and_expanded_indent() {
    let mut state = base_state();
    state.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new(
            "mcp__kofi__post",
            r#"{"title":"Thanks!","amount":5,"private":true}"#,
        )
        .with_status("done")
        .with_output("ok"),
    ));
    let text = render_text(&state, 160, 30).join("\n");
    // Deviation from pi: `serde_json` sorts object keys, so pairs are alphabetical rather than in
    // the model's argument order. Still deterministic and single-line, which is what matters here.
    assert!(
        text.contains(r#"mcp__kofi__post amount=5 private=true title="Thanks!""#),
        "generic collapsed arg preview wrong:\n{text}"
    );

    let mut expanded = base_state();
    expanded.expand_tools = true;
    expanded.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new(
            "mcp__kofi__post",
            r#"{"title":"Thanks!","amount":5}"#,
        )
        .with_status("done")
        .with_output("ok"),
    ));
    let rows = render_text(&expanded, 160, 30);
    let text = rows.join("\n");
    assert!(text.contains("title: Thanks!"), "expanded key/value line missing:\n{text}");
    assert!(text.contains("amount: 5"), "expanded numeric arg missing:\n{text}");
    // Expanded form puts each arg on its own line, indented two columns under the name.
    let title_row = rows
        .iter()
        .find(|l| l.contains("title: Thanks!"))
        .expect("title row");
    assert!(
        title_row.trim_start().starts_with("title:"),
        "expanded arg row must be indented under the header: {title_row:?}"
    );
}

/// `edit` cards preview the word-level diff before execution, marking only changed words.
#[test]
fn edit_card_previews_word_level_diff() {
    let mut state = base_state();
    state.blocks.push(TranscriptBlock::Tool(
        le_vibe_tui::ToolCard::new(
            "edit",
            r#"{"path":"src/lib.rs","oldText":"let count = items.len();","newText":"let count = items.iter().count();"}"#,
        )
        .with_status("running..."),
    ));
    let rows = render_text(&state, 160, 30);
    let text = rows.join("\n");
    assert!(text.contains("- let count = items.len();"), "removed line missing:\n{text}");
    assert!(
        text.contains("+ let count = items.iter().count();"),
        "added line missing:\n{text}"
    );

    // The word diff must isolate `len()` → `iter().count()` rather than reprinting whole lines.
    let segs = le_vibe_tui::word_diff("let count = items.len();", "let count = items.iter().count();");
    let removed: String = segs
        .iter()
        .filter(|s| s.kind == le_vibe_tui::DiffKind::Removed)
        .map(|s| s.text.as_str())
        .collect();
    let added: String = segs
        .iter()
        .filter(|s| s.kind == le_vibe_tui::DiffKind::Added)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(removed.trim(), "len()");
    assert_eq!(added.trim(), "iter().count()");
    // Unchanged words stay untouched so the changed span is visible against them.
    assert!(
        segs.iter().any(|s| s.kind == le_vibe_tui::DiffKind::Equal && s.text.contains("count")),
        "unchanged words must be preserved: {segs:?}"
    );
}

/// pi's `formatToolDuration`: seconds below a minute, then `Xm Ys`, then `Xh Ym Zs`.
#[test]
fn tool_duration_format_matches_pi() {
    assert_eq!(le_vibe_tui::format_tool_duration(0), "0.0s");
    assert_eq!(le_vibe_tui::format_tool_duration(4200), "4.2s");
    assert_eq!(le_vibe_tui::format_tool_duration(59_900), "59.9s");
    assert_eq!(le_vibe_tui::format_tool_duration(90_000), "1m 30s");
    assert_eq!(le_vibe_tui::format_tool_duration(7_384_000), "2h 3m 4s");
}
