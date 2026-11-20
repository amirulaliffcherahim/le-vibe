use le_vibe_tui::{ActiveModal, TranscriptBlock, TuiState, draw_frame};
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
    state.question_prompt = Some((
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
