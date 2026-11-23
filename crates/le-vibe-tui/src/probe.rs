// Headless TUI probe: renders a frame to plain text and turns key specs into input events, so an
// agent or test can drive the real input state machine without a terminal (no TTY, no raw mode).
use std::io;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::input::handle_key_event;
use crate::state::{TuiAction, TuiState};
use crate::transcript::draw_frame;

/// Lower bounds keep the rendered layout readable when a script asks for a tiny frame.
const MIN_WIDTH: u16 = 20;
const MIN_HEIGHT: u16 = 6;

/// A rendered probe frame: the visible text plus where the terminal caret would sit.
pub struct ProbeFrame {
    pub text: String,
    /// Caret as `(column, row)` when the frame asked for a visible cursor.
    pub cursor: Option<(u16, u16)>,
}

/// Render one frame of `state` and return it as plain text, one terminal row per line.
pub fn render_frame_text(state: &TuiState, width: u16, height: u16) -> String {
    render_frame(state, width, height).text
}

/// Render one frame including the caret position the real terminal would place.
pub fn render_frame(state: &TuiState, width: u16, height: u16) -> ProbeFrame {
    let width = width.max(MIN_WIDTH);
    let height = height.max(MIN_HEIGHT);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend never fails to build");
    terminal
        .draw(|frame| draw_frame(frame, state))
        .expect("draw_frame must not fail on the test backend");

    let backend = terminal.backend();
    let buffer = backend.buffer();
    let mut text = String::with_capacity(usize::from(width) * usize::from(height) * 2);
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = buffer.cell((x, y)) {
                text.push_str(cell.symbol());
            }
        }
        while text.ends_with(' ') {
            text.pop();
        }
        text.push('\n');
    }
    let pos = backend.cursor_position();
    ProbeFrame {
        text,
        cursor: backend.cursor_visible().then_some((pos.x, pos.y)),
    }
}

/// Parse a key spec (`Enter`, `Ctrl+G`, `Alt+Up`, `Tab`, `f12`, or a single character) into a press.
pub fn key_from_spec(spec: &str) -> Option<KeyEvent> {
    // A single space is a real key press, so it must not be trimmed away.
    if spec == " " {
        return Some(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    }
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    let mut modifiers = KeyModifiers::NONE;
    let mut tail = spec;
    while let Some((head, rest)) = tail.split_once('+') {
        match head.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "c" => modifiers |= KeyModifiers::CONTROL,
            "alt" | "meta" | "m" => modifiers |= KeyModifiers::ALT,
            "shift" | "s" => modifiers |= KeyModifiers::SHIFT,
            _ => return None,
        }
        tail = rest;
    }

    let code = match tail.to_ascii_lowercase().as_str() {
        "enter" | "return" | "cr" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        "null" => KeyCode::Null,
        other => {
            if let Some(num) = other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
                && (1..=12).contains(&num)
            {
                KeyCode::F(num)
            } else {
                let mut chars = tail.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                KeyCode::Char(c)
            }
        }
    };

    Some(KeyEvent::new(code, modifiers))
}

/// Feed one key spec through the live input state machine, returning the action it produced.
pub fn press(state: &mut TuiState, spec: &str) -> io::Result<Option<TuiAction>> {
    match key_from_spec(spec) {
        Some(key) => handle_key_event(state, key).map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TranscriptBlock;

    fn state() -> TuiState {
        TuiState::new(
            "vibe".into(),
            "mistral-large-4-0".into(),
            "/tmp/probe".into(),
            Some("main".into()),
        )
    }

    #[test]
    fn parses_key_specs_with_modifiers_and_names() {
        let enter = key_from_spec("Enter").expect("enter parses");
        assert_eq!(enter.code, KeyCode::Enter);
        assert_eq!(enter.modifiers, KeyModifiers::NONE);

        let ctrl_g = key_from_spec("Ctrl+G").expect("ctrl+g parses");
        assert_eq!(ctrl_g.code, KeyCode::Char('G'));
        assert!(ctrl_g.modifiers.contains(KeyModifiers::CONTROL));

        let alt_up = key_from_spec("Alt+Up").expect("alt+up parses");
        assert_eq!(alt_up.code, KeyCode::Up);
        assert!(alt_up.modifiers.contains(KeyModifiers::ALT));

        assert_eq!(key_from_spec("f12").expect("f12 parses").code, KeyCode::F(12));
        assert_eq!(key_from_spec("a").expect("char parses").code, KeyCode::Char('a'));
        assert!(key_from_spec("").is_none());
        assert!(key_from_spec("hyper+x").is_none());
        assert!(key_from_spec("notakey").is_none());
    }

    #[test]
    fn renders_typed_input_and_caret_position() {
        let mut state = state();
        press(&mut state, "h").expect("typing works");
        press(&mut state, "i").expect("typing works");
        assert_eq!(state.input, "hi");

        let frame = render_frame(&state, 60, 20);
        assert!(frame.text.contains(" hi"), "frame should show typed input");
        let (col, row) = frame.cursor.expect("editor asks for a visible caret");
        assert_eq!(row + 1, 16, "caret sits on the editor row");
        assert_eq!(col + 1, 4, "caret follows the typed text");
    }

    #[test]
    fn wheel_scroll_keeps_offset_within_content() {
        let mut state = state();
        for i in 0..40 {
            state.blocks.push(TranscriptBlock::User(format!("line {i}")));
        }
        crate::input::apply_wheel_scroll(&mut state, true);
        assert_eq!(state.scroll_offset, 3);
        render_frame(&state, 80, 24);
        assert!(state.rendered_scroll.get() > 0, "render reports the clamped scroll");
        for _ in 0..200 {
            crate::input::apply_wheel_scroll(&mut state, false);
        }
        assert_eq!(state.scroll_offset, 0);
    }
}
