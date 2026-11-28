use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::{ActiveModal, TuiState};

pub(crate) fn prev_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub(crate) fn next_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    if i >= s.len() {
        return s.len();
    }
    i += 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub fn wrap_editor_lines(input: &str, cursor_byte: usize, max_width: usize) -> (Vec<String>, usize, usize) {
    let width = max_width.max(1);
    let target = cursor_byte.min(input.len());
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_chars = 0usize;
    let mut cursor_pos: Option<(usize, usize)> = None;

    for (byte_idx, ch) in input.char_indices() {
        if byte_idx == target && cursor_pos.is_none() {
            cursor_pos = Some((lines.len(), cur_chars));
        }
        if ch == '\n' {
            lines.push(std::mem::take(&mut cur));
            cur_chars = 0;
            continue;
        }
        if cur_chars >= width {
            lines.push(std::mem::take(&mut cur));
            cur_chars = 0;
        }
        cur.push(ch);
        cur_chars += 1;
    }
    if cursor_pos.is_none() {
        if cur_chars >= width && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur_chars = 0;
        }
        cursor_pos = Some((lines.len(), cur_chars));
    }
    lines.push(cur);
    let (row, col) = cursor_pos.unwrap_or((0, 0));
    (lines, row, col)
}

pub(crate) fn draw_editor(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(accent));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let max_width = inner.width.saturating_sub(2).max(1) as usize;
    let (wrapped, cursor_row, cursor_col) =
        wrap_editor_lines(&state.input, state.cursor, max_width);
    let visible_rows = inner.height.max(1) as usize;
    let start_row = if cursor_row >= visible_rows {
        cursor_row + 1 - visible_rows
    } else {
        0
    };
    let end_row = (start_row + visible_rows).min(wrapped.len());

    let display_lines: Vec<Line<'_>> = if state.input.is_empty() && !state.is_busy {
        let pal = state.theme.palette();
        vec![Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(crate::keyhint::placeholder(), Style::default().fg(pal.muted)),
        ])]
    } else {
        wrapped[start_row..end_row]
            .iter()
            .map(|l| {
                Line::from(vec![
                    Span::styled(" ", Style::default()),
                    Span::styled(l.clone(), Style::default().fg(Color::White)),
                ])
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(display_lines), inner);

    if state.modal == ActiveModal::None && (!state.is_busy || !state.input.is_empty()) {
        let cursor_x = (inner.x + 1 + cursor_col as u16).min(inner.x + inner.width.saturating_sub(1));
        let cursor_y = inner.y + (cursor_row.saturating_sub(start_row)) as u16;
        if cursor_y < inner.y + inner.height {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

