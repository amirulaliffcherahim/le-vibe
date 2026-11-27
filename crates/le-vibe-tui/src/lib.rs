pub mod commands;
pub mod copy;
pub mod diagram;
pub mod diff;
pub mod keyhint;
pub mod osc133;
pub mod editor;
pub mod export;
pub mod input;
pub mod markdown;
pub mod modal;
pub mod mode;
pub mod probe;
pub mod resync_backend;
pub mod state;
pub mod theme;
pub mod transcript;

pub use commands::{
    BUILTIN_SLASH_COMMANDS, SlashCommandItem, SlashCommandSpec, filter_slash_commands,
    filter_slash_commands_with_dynamic,
};
pub use copy::{extract_last_code_block, format_chat_html, format_chat_plain};
pub use diff::{DiffKind, DiffSegment, word_diff};
pub use keyhint::{KeyAction, key_hint, key_label, key_text};
pub use osc133::{
    OSC133_ZONE_END, OSC133_ZONE_FINAL, OSC133_ZONE_START, osc133_enabled, prompt_zone_for,
    write_zone,
};
pub use diagram::{extract_latest_xml_block, project_xml_diagram};
pub use editor::wrap_editor_lines;
pub use export::{export_session_html, export_session_markdown};
pub use input::{apply_wheel_scroll, handle_key_event, poll_tui_action};
pub use mode::{ALL_MODES, ALL_SHELLS, AgentMode, append_model_profile, build_system_prompt};
pub use probe::{ProbeFrame, key_from_spec, press, render_frame, render_frame_text};
pub use state::{
    ActiveModal, CompactionReason, FrameBuffer, ProviderUsageStats, QuestionItem, QuestionOption,
    QuestionSession, SPINNER_FRAMES, StatusKind, TerminalGuard, ToolCard, TranscriptBlock,
    TreeAction, TuiAction, TuiState, format_tokens, render_status_bar,
};
pub use theme::{ALL_THEMES, ThemeId, ThemePalette};
pub use transcript::{
    ToolHeader, draw_frame, format_duration_ms, format_read_line_range, format_tool_duration,
    tool_call_header,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffs_only_changed_lines() {
        let mut fb = FrameBuffer::new();
        let first = fb.diff(vec!["line1".into(), "line2".into()]);
        assert_eq!(first.len(), 2);

        let second = fb.diff(vec!["line1".into(), "line2-updated".into()]);
        assert_eq!(second, vec![(1, "line2-updated".to_string())]);
    }

    #[test]
    fn filters_slash_commands_live() {
        let all = filter_slash_commands("/");
        assert!(all.len() >= 10);
        let mo = filter_slash_commands("/mo");
        assert_eq!(mo.len(), 2);
        let m = filter_slash_commands("/model");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "/model");
        let u = filter_slash_commands("/us");
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].name, "/usage");
    }

    #[test]
    fn cycles_all_five_modes() {
        let mut m = AgentMode::Normal;
        let seen: Vec<&str> = (0..5)
            .map(|_| {
                let cur = m.name();
                m = m.next();
                cur
            })
            .collect();
        assert_eq!(seen, vec!["normal", "plan", "ask", "rea", "cyber"]);
        assert_eq!(m, AgentMode::Normal);
    }

    #[test]
    fn wraps_editor_lines_and_tracks_cursor() {
        let (lines, row, col) = wrap_editor_lines("hello world", 11, 5);
        assert_eq!(lines, vec!["hello", " worl", "d"]);
        assert_eq!((row, col), (2, 1));
    }

    #[test]
    fn exports_session_to_markdown_and_html() {
        let mut state = TuiState::new(
            "vibe".into(),
            "mistral-large-4-0".into(),
            ".".into(),
            Some("main".into()),
        );
        state.blocks.push(TranscriptBlock::User("hi <script>".into()));
        state.blocks.push(TranscriptBlock::Assistant("hello!".into()));

        let md = export_session_markdown(&state);
        assert!(md.contains("# le-vibe Session Export"));
        assert!(md.contains("## User"));
        assert!(md.contains("hi <script>"));
        assert!(md.contains("## Assistant"));

        let html = export_session_html(&state);
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("hi &lt;script&gt;"));
        assert!(html.contains("hello!"));
    }
}
