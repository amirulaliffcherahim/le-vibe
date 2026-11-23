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
    state.blocks.push(TranscriptBlock::Tool {
        name: "read".into(),
        status: "done".into(),
        output: "1: fn main() {}".into(),
    });
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
    state.blocks.push(TranscriptBlock::Tool {
        name: "bash".into(),
        status: "done".into(),
        output: "T\0h\0e\0 \0o\0p\0e\0r\0a\0t\0i\0o\0n\0 \0f\0a\0i\0l\0e\0d\r\n+added line\n-removed line".into(),
    });
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
