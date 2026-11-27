use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::commands::filter_slash_commands_with_dynamic;
use crate::editor::{draw_editor, wrap_editor_lines};
use crate::markdown::render_markdown_lines;
use crate::modal::{draw_at_file_popup, draw_modal, draw_slash_popup};
use crate::mode::AgentMode;
use crate::state::{ActiveModal, StatusKind, TranscriptBlock, TuiState, format_tokens};
use crate::theme::ThemePalette;

pub fn draw_frame(frame: &mut Frame<'_>, state: &TuiState) {
    let area = frame.area();
    if area.width < 40 || area.height < 12 {
        let msg = Paragraph::new(format!(
            "Terminal too small ({}x{}). Minimum is 40x12.",
            area.width, area.height
        ))
        .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    let editor_max_w = area.width.saturating_sub(2).max(1) as usize;
    let (editor_wrapped, _, _) = wrap_editor_lines(&state.input, state.cursor, editor_max_w);
    let editor_height = (editor_wrapped.len() as u16).clamp(1, 5) + 2;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),                // Full-height scrollable transcript (startup banner scrolls naturally like Pi)
            Constraint::Length(1),             // Breathing room above working/status strip (Pi Loader style)
            Constraint::Length(1),             // Dedicated working/status strip above editor
            Constraint::Length(editor_height), // Auto-wrapping Editor box (1-5 lines)
            Constraint::Length(3),             // pi-footer (identity/git, usage/model, mode+quota/runtime)
        ])
        .split(area);

    if state.show_todo_pane && !state.todos.is_empty() && area.width >= 88 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(52), Constraint::Length(32)])
            .split(chunks[0]);
        draw_transcript(frame, cols[0], state);
        draw_todo_pane(frame, cols[1], state);
    } else {
        draw_transcript(frame, chunks[0], state);
    }
    draw_status_strip(frame, chunks[2], state);
    draw_editor(frame, chunks[3], state);
    draw_footer(frame, chunks[4], state);

    let matches = filter_slash_commands_with_dynamic(&state.input, &state.plugin_commands);
    if !matches.is_empty() && state.modal == ActiveModal::None {
        draw_slash_popup(frame, chunks[3], &matches, state.selected_slash_idx, state.theme);
    } else if state.modal == ActiveModal::None {
        let file_matches = state.at_file_matches();
        if !file_matches.is_empty() {
            draw_at_file_popup(
                frame,
                chunks[3],
                &file_matches,
                state.selected_slash_idx,
                state.theme,
            );
        }
    }
    if state.modal != ActiveModal::None {
        draw_modal(frame, area, state);
    }
}

fn push_startup_banner(lines: &mut Vec<Line<'static>>, state: &TuiState) {
    let accent = state.theme.accent();
    let secondary = state.theme.secondary();
    if state.quiet_startup {
        lines.push(Line::from(vec![
            Span::styled(" le-vibe ", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            Span::styled(format!("v{} ", state.version), Style::default().fg(Color::DarkGray)),
            Span::styled("· ", Style::default().fg(Color::DarkGray)),
            Span::styled("Shift+Tab", Style::default().fg(Color::Gray)),
            Span::styled(" mode · ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+P", Style::default().fg(Color::Gray)),
            Span::styled(" model · ", Style::default().fg(Color::DarkGray)),
            Span::styled("/", Style::default().fg(Color::Gray)),
            Span::styled(" commands", Style::default().fg(Color::DarkGray)),
        ]));
        return;
    }
    lines.push(Line::from(vec![
        Span::styled("  ,___,  ", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        Span::styled("le-vibe ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("v{} ", state.version), Style::default().fg(Color::DarkGray)),
        Span::styled("zZz", Style::default().fg(secondary)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  (-v-)  ", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        Span::styled("esc", Style::default().fg(Color::Gray)),
        Span::styled(" interrupt · ", Style::default().fg(Color::DarkGray)),
        Span::styled("ctrl+c", Style::default().fg(Color::Gray)),
        Span::styled(" clear/exit · ", Style::default().fg(Color::DarkGray)),
        Span::styled("ctrl+p", Style::default().fg(Color::Gray)),
        Span::styled(" model · ", Style::default().fg(Color::DarkGray)),
        Span::styled("shift+tab", Style::default().fg(Color::Gray)),
        Span::styled(" mode · ", Style::default().fg(Color::DarkGray)),
        Span::styled("/", Style::default().fg(Color::Gray)),
        Span::styled(" commands", Style::default().fg(Color::DarkGray)),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" /)_(_\\  ", Style::default().fg(accent)),
        Span::styled(
            "Rust Pi harness · Unified Python provider engine (Vibe, DeepSeek, Antigravity)",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    if !state.loaded_resources.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("         "),
            Span::styled("loaded: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.loaded_resources.join(" · "),
                Style::default().fg(state.theme.palette().muted),
            ),
        ]));
    }
}

struct SurfaceStyle {
    title: Color,
    bg: Color,
    body: Style,
    diff_ok: Color,
    diff_err: Color,
}

/// Compute hanging indent width for wrapped continuation lines (leading spaces + list/quote marker),
/// capped at `max_width / 3` so narrow terminals never starve.
fn hanging_indent_for_line(line: &Line<'_>, max_width: usize) -> usize {
    let mut plain = String::new();
    for s in &line.spans {
        plain.push_str(s.content.as_ref());
        if plain.len() >= 48 {
            break;
        }
    }
    let bytes = plain.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    let rest = &plain[i..];
    let marker_cols = if rest.starts_with("• ")
        || rest.starts_with("- ")
        || rest.starts_with("* ")
        || rest.starts_with("+ ")
        || rest.starts_with("│ ")
        || rest.starts_with("> ")
    {
        2
    } else {
        // Ordered list prefix like "1. " or "12) "
        let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0
            && digits <= 4
            && rest.len() > digits + 1
            && matches!(rest.as_bytes()[digits], b'.' | b')')
            && rest.as_bytes()[digits + 1] == b' '
        {
            digits + 2
        } else {
            0
        }
    };
    let indent = i + marker_cols;
    indent.min(max_width / 3)
}

struct WrapToken {
    is_ws: bool,
    parts: Vec<(String, Style)>,
    width: usize,
}

/// Word-wrap any line wider than `max_width` while preserving span styles, keeping cross-span
/// words atomic (e.g. `(` + inline code), and applying hanging indentation on continuation lines.
fn wrap_lines_exact(lines: Vec<Line<'static>>, max_width: usize) -> Vec<Line<'static>> {
    let w = max_width.max(8);
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        if line.width() <= w {
            out.push(line);
            continue;
        }
        let line_style = line.style;
        let hanging = hanging_indent_for_line(&line, w);

        // Flatten into atomic tokens across span boundaries: each token is either pure whitespace
        // or a contiguous non-whitespace word (possibly spanning multiple styles, e.g. `(` + `code`).
        let mut tokens: Vec<WrapToken> = Vec::new();
        for span in line.spans {
            let st = span.style;
            let text = span.content.into_owned();
            let mut idx = 0;
            let bytes = text.as_bytes();
            while idx < text.len() {
                let is_ws = bytes[idx].is_ascii_whitespace();
                let start = idx;
                while idx < text.len() && bytes[idx].is_ascii_whitespace() == is_ws {
                    idx += 1;
                    while idx < text.len() && !text.is_char_boundary(idx) {
                        idx += 1;
                    }
                }
                let seg = text[start..idx].to_string();
                let seg_w = Span::raw(&seg).width();
                if let Some(last) = tokens.last_mut()
                    && last.is_ws == is_ws
                {
                    last.parts.push((seg, st));
                    last.width += seg_w;
                } else {
                    tokens.push(WrapToken {
                        is_ws,
                        parts: vec![(seg, st)],
                        width: seg_w,
                    });
                }
            }
        }

        let mut cur_spans: Vec<Span<'static>> = Vec::new();
        let mut cur_w = 0usize;
        let mut is_continuation = false;

        let start_continuation = |cur_spans: &mut Vec<Span<'static>>,
                                  cur_w: &mut usize,
                                  is_continuation: &mut bool,
                                  out: &mut Vec<Line<'static>>| {
            out.push(Line::from(std::mem::take(cur_spans)).style(line_style));
            *cur_w = 0;
            *is_continuation = true;
            if hanging > 0 {
                cur_spans.push(Span::styled(" ".repeat(hanging), line_style));
                *cur_w = hanging;
            }
        };

        for WrapToken {
            is_ws,
            parts,
            width: tok_w,
        } in tokens
        {
            if is_ws {
                // Drop leading whitespace on wrapped continuation lines
                if is_continuation && cur_w <= hanging {
                    continue;
                }
                if cur_w + tok_w <= w {
                    for (seg, st) in parts {
                        cur_spans.push(Span::styled(seg, st));
                    }
                    cur_w += tok_w;
                } else if cur_w > 0 {
                    start_continuation(&mut cur_spans, &mut cur_w, &mut is_continuation, &mut out);
                }
                continue;
            }

            if cur_w + tok_w <= w {
                for (seg, st) in parts {
                    cur_spans.push(Span::styled(seg, st));
                }
                cur_w += tok_w;
                continue;
            }

            // Word doesn't fit on current line: wrap before it if current line already has content
            let min_prefix = if is_continuation { hanging } else { 0 };
            if cur_w > min_prefix {
                start_continuation(&mut cur_spans, &mut cur_w, &mut is_continuation, &mut out);
                if cur_w + tok_w <= w {
                    for (seg, st) in parts {
                        cur_spans.push(Span::styled(seg, st));
                    }
                    cur_w += tok_w;
                    continue;
                }
            }

            // Single word wider than available line width: hard-split cleanly by display width
            for (seg, st) in parts {
                let mut rest = seg.as_str();
                while !rest.is_empty() {
                    let avail = w.saturating_sub(cur_w).max(1);
                    let mut take_bytes = 0usize;
                    let mut taken_w = 0usize;
                    for (bi, ch) in rest.char_indices() {
                        let cw = Span::raw(ch.to_string()).width().max(1);
                        if taken_w + cw > avail && taken_w > 0 {
                            break;
                        }
                        taken_w += cw;
                        take_bytes = bi + ch.len_utf8();
                    }
                    if take_bytes == 0 {
                        let ch = rest.chars().next().unwrap();
                        take_bytes = ch.len_utf8();
                        taken_w = Span::raw(ch.to_string()).width().max(1);
                    }
                    cur_spans.push(Span::styled(rest[..take_bytes].to_string(), st));
                    cur_w += taken_w;
                    rest = &rest[take_bytes..];
                    if cur_w >= w && !rest.is_empty() {
                        start_continuation(
                            &mut cur_spans,
                            &mut cur_w,
                            &mut is_continuation,
                            &mut out,
                        );
                    }
                }
            }
        }
        if !cur_spans.is_empty() {
            out.push(Line::from(cur_spans).style(line_style));
        }
    }
    out
}

fn sanitize_display_line(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\t' => out.push_str("    "),
            '\0' | '\r' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Push one wrapped, full-width-padded surface row, keeping per-span styling.
fn surface_push_row(
    lines: &mut Vec<Line<'static>>,
    total_w: usize,
    inner_w: usize,
    bg_style: Style,
    spans: Vec<Span<'static>>,
) {
    if spans.iter().all(|s| s.content.is_empty()) {
        lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
        return;
    }
    let plain: String = spans.iter().map(|s| s.content.as_ref()).collect();
    let clean = sanitize_display_line(&plain);
    if clean.is_empty() {
        lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
        return;
    }
    // Fast path: single unstyled chunk.
    if spans.len() == 1 {
        let mut remaining = clean.as_str();
        while !remaining.is_empty() {
            let (chunk, rest) = split_at_width(remaining, inner_w);
            let pad = total_w.saturating_sub(1 + Span::raw(chunk).width());
            lines.push(
                Line::from(vec![
                    Span::styled(" ", bg_style),
                    Span::styled(chunk.to_string(), spans[0].style.bg(bg_style.bg.unwrap_or_default())),
                    Span::styled(" ".repeat(pad), bg_style),
                ])
                .style(bg_style),
            );
            remaining = rest;
        }
        return;
    }
    if Span::raw(plain.clone()).width() <= inner_w {
        let pad = total_w.saturating_sub(1 + Span::raw(clean.clone()).width());
        let mut row: Vec<Span<'static>> = Vec::with_capacity(spans.len() + 2);
        row.push(Span::styled(" ", bg_style));
        row.extend(spans);
        row.push(Span::styled(" ".repeat(pad), bg_style));
        lines.push(Line::from(row).style(bg_style));
        return;
    }
    // Overflowing styled row: split by display width, preserving span order.
    let mut row: Vec<Span<'static>> = vec![Span::styled(" ", bg_style)];
    let mut used = 0usize;
    for span in spans {
        let mut rest = span.content.to_string();
        while !rest.is_empty() {
            let avail = inner_w.saturating_sub(used).max(1);
            let (chunk, tail) = {
                let (c, t) = split_at_width(&rest, avail);
                (c.to_string(), t.to_string())
            };
            if chunk.is_empty() {
                break;
            }
            used += Span::raw(chunk.clone()).width();
            row.push(Span::styled(chunk, span.style));
            rest = tail;
            if used >= inner_w {
                let pad = total_w.saturating_sub(1 + used);
                row.push(Span::styled(" ".repeat(pad), bg_style));
                lines.push(Line::from(std::mem::replace(
                    &mut row,
                    vec![Span::styled(" ", bg_style)],
                ))
                .style(bg_style));
                used = 0;
            }
        }
    }
    if row.len() > 1 {
        let pad = total_w.saturating_sub(1 + used);
        row.push(Span::styled(" ".repeat(pad), bg_style));
        lines.push(Line::from(row).style(bg_style));
    }
}

/// Pi-style full-width borderless tinted surface (zero `▎` noise, zero jagged right edges).
fn push_pi_surface(
    lines: &mut Vec<Line<'static>>,
    area_width: u16,
    title: &str,
    body: &str,
    hidden_count: usize,
    hidden_is_earlier: bool,
    style: SurfaceStyle,
) {
    let body_lines = text_body_rows(body, &style);
    push_pi_surface_lines(
        lines,
        area_width,
        title,
        SurfaceBody {
            header_rows: &[],
            rows: &body_lines,
            hidden_count,
            hidden_is_earlier,
        },
        style,
    );
}

/// Body content for a tool surface: optional expanded-arg header rows, styled body rows and the
/// collapsed-line counter used for the `ctrl+o to expand` hint.
struct SurfaceBody<'a> {
    header_rows: &'a [String],
    rows: &'a [Line<'static>],
    hidden_count: usize,
    hidden_is_earlier: bool,
}

/// Colour `+`/`-` prefixed output lines the way pi colours a diff body.
fn text_body_rows(body: &str, style: &SurfaceStyle) -> Vec<Line<'static>> {
    body.lines()
        .map(|raw_line| {
            let clean = sanitize_display_line(raw_line);
            let line_style = if clean.starts_with('+') && !clean.starts_with("+++") {
                Style::default().fg(style.diff_ok).bg(style.bg)
            } else if clean.starts_with('-') && !clean.starts_with("---") {
                Style::default().fg(style.diff_err).bg(style.bg)
            } else {
                style.body.bg(style.bg)
            };
            Line::from(Span::styled(clean, line_style))
        })
        .collect()
}

fn is_edit_tool(name: &str) -> bool {
    matches!(name, "edit" | "write" | "multiedit" | "apply_patch")
}

/// Build pi's two-row (`-` then `+`) word-diff preview for an `edit`/`write` tool call.
fn edit_diff_rows(name: &str, args_json: &str, style: &SurfaceStyle) -> Vec<Line<'static>> {
    if !is_edit_tool(name) {
        return Vec::new();
    }
    let Ok(args) = serde_json::from_str::<serde_json::Value>(args_json) else {
        return Vec::new();
    };
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let (Some(old), Some(new)) = (
        args.get("oldText").and_then(serde_json::Value::as_str),
        args.get("newText").and_then(serde_json::Value::as_str),
    ) {
        pairs.push((old.to_string(), new.to_string()));
    }
    if let Some(edits) = args.get("edits").and_then(serde_json::Value::as_array) {
        for e in edits {
            if let (Some(old), Some(new)) = (
                e.get("oldText").and_then(serde_json::Value::as_str),
                e.get("newText").and_then(serde_json::Value::as_str),
            ) {
                pairs.push((old.to_string(), new.to_string()));
            }
        }
    }
    if pairs.is_empty() {
        return Vec::new();
    }

    let mut rows: Vec<Line<'static>> = Vec::new();
    for (old, new) in pairs.iter().take(4) {
        let segments = crate::diff::word_diff(old, new);
        for kind in [crate::diff::DiffKind::Removed, crate::diff::DiffKind::Added] {
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                if kind == crate::diff::DiffKind::Removed {
                    "- "
                } else {
                    "+ "
                },
                Style::default().fg(if kind == crate::diff::DiffKind::Removed {
                    style.diff_err
                } else {
                    style.diff_ok
                }),
            )];
            let mut changed = false;
            for seg in &segments {
                let highlight = seg.kind == kind;
                let visible = seg.kind == kind || seg.kind == crate::diff::DiffKind::Equal;
                if !visible {
                    continue;
                }
                if highlight {
                    changed = true;
                }
                let mut s = Style::default().fg(style.body.fg.unwrap_or(style.title));
                if highlight {
                    s = s.add_modifier(Modifier::REVERSED | Modifier::BOLD);
                }
                spans.push(Span::styled(seg.text.clone(), s));
            }
            if changed {
                rows.push(Line::from(spans));
            }
        }
    }
    rows
}

/// Same surface, but with pre-styled body rows and optional extra header rows (e.g. expanded args).
fn push_pi_surface_lines(
    lines: &mut Vec<Line<'static>>,
    area_width: u16,
    title: &str,
    body: SurfaceBody<'_>,
    style: SurfaceStyle,
) {
    // Pad to the wrap width (callers reserve 2 columns for the scrollbar) so a surface line never
    // re-wraps into a phantom trailing line.
    let total_w = (area_width as usize).saturating_sub(2).max(20);
    let inner_w = total_w.saturating_sub(2).max(16);
    let bg_style = Style::default().bg(style.bg);

    // Top 1-line breathing room
    lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));

    // Header line (bold white/tinted, full-width padded)
    let clean_title = sanitize_display_line(title);
    let (title_fit, _) = split_at_width(&clean_title, inner_w);
    let header_pad = total_w.saturating_sub(1 + Span::raw(title_fit).width());
    lines.push(
        Line::from(vec![
            Span::styled(" ", bg_style),
            Span::styled(
                title_fit.to_string(),
                Style::default()
                    .fg(style.title)
                    .bg(style.bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ".repeat(header_pad), bg_style),
        ])
        .style(bg_style),
    );

    // Extra header rows (expanded generic args): two-space hanging indent, muted.
    for row in body.header_rows {
        surface_push_row(
            lines,
            total_w,
            inner_w,
            bg_style,
            vec![Span::styled(
                format!("  {row}"),
                Style::default().fg(style.body.fg.unwrap_or(style.title)).bg(style.bg),
            )],
        );
    }

    if body.hidden_count > 0 {
        let hint = format!(
            "... ({} {} lines, {} to expand)",
            body.hidden_count,
            if body.hidden_is_earlier {
                "earlier"
            } else {
                "more"
            },
            crate::keyhint::key_text(crate::keyhint::KeyAction::ExpandTools)
        );
        let (hint_fit, _) = split_at_width(&hint, inner_w);
        let pad = total_w.saturating_sub(1 + Span::raw(hint_fit).width());
        lines.push(
            Line::from(vec![
                Span::styled(" ", bg_style),
                Span::styled(hint_fit.to_string(), Style::default().fg(Color::DarkGray).bg(style.bg)),
                Span::styled(" ".repeat(pad), bg_style),
            ])
            .style(bg_style),
        );
    }

    if !body.rows.is_empty() {
        lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
        for row in body.rows {
            surface_push_row(
                lines,
                total_w,
                inner_w,
                bg_style,
                row.spans
                    .iter()
                    .map(|s| Span::styled(s.content.to_string(), s.style.bg(style.bg)))
                    .collect(),
            );
        }
    }

    // Bottom 1-line breathing room
    lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
}

/// Pi-style user turn: full-width tinted surface, one column of inner padding, no prefix glyph.
fn push_user_surface(
    lines: &mut Vec<Line<'static>>,
    area_width: u16,
    text: &str,
    bg: Color,
    pal: &ThemePalette,
) {
    let total_w = (area_width as usize).saturating_sub(2).max(20);
    let inner_w = total_w.saturating_sub(2).max(8);
    let bg_style = Style::default().bg(bg);

    lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
    // pi renders user turns as Markdown (ordered markers and backslash escapes preserved), then
    // tints the whole block with `userMessageBg`.
    let markdown = render_markdown_lines(text, pal, inner_w as u16, false);
    for row in wrap_lines_exact(markdown, inner_w) {
        if row.spans.iter().all(|s| s.content.trim().is_empty()) {
            lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
            continue;
        }
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(row.spans.len() + 2);
        spans.push(Span::styled(" ", bg_style));
        spans.extend(
            row.spans
                .into_iter()
                .map(|s| Span::styled(s.content.to_string(), s.style.bg(bg))),
        );
        let used: usize = spans.iter().map(|s| Span::raw(s.content.to_string()).width()).sum();
        spans.push(Span::styled(" ".repeat(total_w.saturating_sub(used)), bg_style));
        lines.push(Line::from(spans).style(bg_style));
    }
    lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
}

/// Split `text` so the prefix fits within `max_width` terminal columns (making progress on wide chars).
fn split_at_width(text: &str, max_width: usize) -> (&str, &str) {
    let mut used = 0usize;
    let mut split_idx = 0usize;
    for (idx, ch) in text.char_indices() {
        let mut buf = [0u8; 4];
        let w = Span::raw(ch.encode_utf8(&mut buf) as &str).width();
        if split_idx > 0 && used + w > max_width {
            return text.split_at(idx);
        }
        used += w;
        split_idx = idx + ch.len_utf8();
        if used >= max_width {
            return text.split_at(split_idx);
        }
    }
    (text, "")
}

/// Sentinel neighbour kind for the first block.
pub(crate) const PREV_NONE: u8 = u8::MAX;
const KIND_THINKING: u8 = 1;
const KIND_ASSISTANT: u8 = 2;

/// Coarse block identity, used to decide spacer placement between blocks.
fn block_kind(block: &TranscriptBlock) -> u8 {
    match block {
        TranscriptBlock::User(_) => 0,
        TranscriptBlock::Thinking(_) => KIND_THINKING,
        TranscriptBlock::Assistant(_) => KIND_ASSISTANT,
        TranscriptBlock::Tool(_) => 3,
        TranscriptBlock::Script { .. } => 4,
        TranscriptBlock::System(_) => 5,
        TranscriptBlock::Report(_) => 6,
    }
}

fn hash_block(block: &TranscriptBlock) -> u64 {    let mut h = DefaultHasher::new();
    match block {
        TranscriptBlock::User(t) => {
            0u8.hash(&mut h);
            t.hash(&mut h);
        }
        TranscriptBlock::Thinking(t) => {
            1u8.hash(&mut h);
            t.hash(&mut h);
        }
        TranscriptBlock::Assistant(t) => {
            2u8.hash(&mut h);
            t.hash(&mut h);
        }
        TranscriptBlock::Tool(card) => {
            3u8.hash(&mut h);
            card.name.hash(&mut h);
            card.args_json.hash(&mut h);
            card.status.hash(&mut h);
            card.output.hash(&mut h);
            card.duration_ms.hash(&mut h);
        }
        TranscriptBlock::Script { command, output, in_context } => {
            4u8.hash(&mut h);
            command.hash(&mut h);
            output.hash(&mut h);
            in_context.hash(&mut h);
        }
        TranscriptBlock::System(t) => {
            5u8.hash(&mut h);
            t.hash(&mut h);
        }
        TranscriptBlock::Report(t) => {
            6u8.hash(&mut h);
            t.hash(&mut h);
        }
    }
    h.finish()
}

fn render_block_wrapped(
    block: &TranscriptBlock,
    area_width: u16,
    wrap_width: usize,
    state: &TuiState,
    prev_kind: Option<u8>,
) -> Vec<Line<'static>> {
    let pal = state.theme.palette();
    let accent = pal.accent;
    let mut lines: Vec<Line<'static>> = Vec::new();
    // pi adds spacers lazily, right before the next *visible group*: an interleaved
    // thinking → text → thinking run reads as one block, so no blank line inside it.
    let after_thinking = prev_kind == Some(KIND_THINKING);
    let after_text = prev_kind == Some(KIND_ASSISTANT);
    match block {
        TranscriptBlock::User(text) => {
            lines.push(Line::from(""));
            push_user_surface(
                &mut lines,
                area_width,
                text,
                state.theme.user_surface(),
                &pal,
            );
        }
        TranscriptBlock::Thinking(text) => {
            if !text.is_empty() {
                if !after_text && !after_thinking {
                    lines.push(Line::from(""));
                }
                let line_count = text.lines().count().max(1);
                if state.show_thinking {
                    let think_style = Style::default()
                        .fg(pal.muted)
                        .add_modifier(Modifier::ITALIC);
                    let raw_lines: Vec<Line<'static>> = text
                        .lines()
                        .map(|l| Line::from(vec![Span::raw(" "), Span::styled(l.to_string(), think_style)]))
                        .collect();
                    lines.extend(wrap_lines_exact(raw_lines, wrap_width));
                } else {
                    let unit = if line_count == 1 { "line" } else { "lines" };
                    let hint = crate::keyhint::key_text(crate::keyhint::KeyAction::ToggleThinking);
                    let label = if state.is_busy {
                        format!(" Thinking… ({line_count} {unit} · {hint})")
                    } else {
                        format!(" Thought ({line_count} {unit} · {hint})")
                    };
                    lines.push(Line::from(Span::styled(
                        label,
                        Style::default()
                            .fg(pal.muted)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
            }
        }
        TranscriptBlock::Assistant(text) => {
            if !text.is_empty() {
                if !after_thinking {
                    lines.push(Line::from(""));
                }
                // Align assistant prose to the same 1-column left rhythm as user & tool cards.
                let pad_w = area_width.saturating_sub(1).max(20);
                let md_lines = render_markdown_lines(text, &pal, pad_w, state.expand_tools);
                for line in md_lines {
                    let mut spans = Vec::with_capacity(line.spans.len() + 1);
                    spans.push(Span::raw(" "));
                    spans.extend(line.spans);
                    lines.push(Line::from(spans).style(line.style));
                }
            }
        }
        TranscriptBlock::Tool(card) => {
            lines.push(Line::from(""));
            let is_err = card.is_error();
            let is_running = card.is_running();
            let header = tool_call_header(&card.name, &card.args_json, state.expand_tools);
            let mut title = header.title;
            if !card.status.is_empty() {
                title.push_str(" · ");
                title.push_str(&card.status);
            }
            if let Some(ms) = card.duration_ms {
                title.push_str(&format!(" ({})", format_tool_duration(ms)));
            }

            // pi hides the body of collapsed compact `read` cards (`[skill]`, resource, docs).
            let (preview, hidden) = if header.suppress_body && !state.expand_tools {
                (String::new(), 0)
            } else {
                let all_out: Vec<&str> = card.output.trim().lines().collect();
                // pi: 10 preview lines by default, 5 for shell tools (keep the tail for shells).
                let (max_lines, keep_tail) = if state.expand_tools {
                    (40, is_shell_tool(&card.name))
                } else if is_shell_tool(&card.name) {
                    (5, true)
                } else {
                    (10, false)
                };
                let hidden = all_out.len().saturating_sub(max_lines);
                let slice = if hidden > 0 && keep_tail {
                    all_out[all_out.len() - max_lines..].join("\n")
                } else if hidden > 0 {
                    all_out[..max_lines].join("\n")
                } else {
                    all_out.join("\n")
                };
                (slice, hidden)
            };
            let tail_slice = is_shell_tool(&card.name);
            let style = SurfaceStyle {
                title: if is_err { pal.error } else { pal.fg },
                bg: if is_err {
                    pal.card_err_bg
                } else if is_running {
                    pal.card_quiet_bg
                } else {
                    pal.card_ok_bg
                },
                body: Style::default().fg(pal.muted),
                diff_ok: pal.success,
                diff_err: pal.error,
            };
            // pi previews edits before they run: word-level diff with the changed spans inverted.
            let diff_rows = edit_diff_rows(&card.name, &card.args_json, &style);
            let has_diff = !diff_rows.is_empty();
            let mut body = if diff_rows.is_empty() {
                text_body_rows(&preview, &style)
            } else {
                diff_rows
            };
            if has_diff && state.expand_tools && !preview.is_empty() {
                body.push(Line::from(""));
                for raw in preview.lines() {
                    body.push(Line::from(Span::styled(
                        sanitize_display_line(raw),
                        style.body.bg(style.bg),
                    )));
                }
            }
            push_pi_surface_lines(
                &mut lines,
                area_width,
                &title,
                SurfaceBody {
                    header_rows: &header.args_lines,
                    rows: &body,
                    hidden_count: hidden,
                    hidden_is_earlier: tail_slice,
                },
                style,
            );
        }
        TranscriptBlock::Script {
            command,
            output,
            in_context,
        } => {
            lines.push(Line::from(""));
            let tag = if *in_context {
                format!("$ {command}")
            } else {
                format!("$ {command} (no context)")
            };
            let all_out: Vec<&str> = output.trim().lines().collect();
            let max_lines = if state.expand_tools { 40 } else { 8 };
            let hidden = all_out.len().saturating_sub(max_lines);
            let preview = if hidden > 0 {
                all_out[all_out.len() - max_lines..].join("\n")
            } else {
                all_out.join("\n")
            };
            push_pi_surface(
                &mut lines,
                area_width,
                &tag,
                &preview,
                hidden,
                true,
                SurfaceStyle {
                    title: Color::White,
                    bg: pal.card_quiet_bg,
                    body: Style::default().fg(Color::Gray),
                    diff_ok: pal.success,
                    diff_err: pal.error,
                },
            );
        }
        TranscriptBlock::System(text) => {
            lines.push(Line::from(""));
            for l in text.lines() {
                lines.push(Line::from(vec![
                    Span::styled("   • ", Style::default().fg(accent)),
                    Span::styled(l.to_string(), Style::default().fg(Color::Gray)),
                ]));
            }
        }
        TranscriptBlock::Report(text) => {
            lines.push(Line::from(""));
            for l in text.lines() {
                if l.is_empty() {
                    lines.push(Line::from(""));
                } else if l.starts_with(' ') {
                    // Detail lines (bars, budget) stay indented and dim.
                    lines.push(Line::from(Span::styled(
                        l.to_string(),
                        Style::default().fg(Color::Gray),
                    )));
                } else {
                    lines.push(Line::from(Span::styled(
                        l.to_string(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    )));
                }
            }
        }
    }
    wrap_lines_exact(lines, wrap_width)
}

fn draw_transcript(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let wrap_width = (area.width as usize).saturating_sub(2).max(8);
    let mut banner_lines: Vec<Line<'static>> = Vec::new();
    push_startup_banner(&mut banner_lines, state);
    let mut visual_lines = wrap_lines_exact(banner_lines, wrap_width);

    let mut cache = state.transcript_cache.borrow_mut();
    if cache.width != area.width
        || cache.theme != Some(state.theme)
        || cache.show_thinking != state.show_thinking
        || cache.expand_tools != state.expand_tools
    {
        cache.width = area.width;
        cache.theme = Some(state.theme);
        cache.show_thinking = state.show_thinking;
        cache.expand_tools = state.expand_tools;
        cache.entries.clear();
    }
    if cache.entries.len() > state.blocks.len() {
        cache.entries.truncate(state.blocks.len());
    }
    for (idx, block) in state.blocks.iter().enumerate() {
        // Spacer placement depends on the previous block (interleaved thinking/text runs are one
        // visual group), so the neighbour kind is part of the cache key.
        let prev_kind = if idx == 0 {
            PREV_NONE
        } else {
            block_kind(&state.blocks[idx - 1])
        };
        let h = hash_block(block) ^ (u64::from(prev_kind) << 56);
        let hit = cache.entries.get(idx).is_some_and(|(ch, _)| *ch == h);
        if !hit {
            let rendered =
                render_block_wrapped(block, area.width, wrap_width, state, Some(prev_kind));
            if idx < cache.entries.len() {
                cache.entries[idx] = (h, rendered);
            } else {
                cache.entries.push((h, rendered));
            }
        }
        visual_lines.extend(cache.entries[idx].1.iter().cloned());
    }
    drop(cache);
    let total_lines = visual_lines.len().min(u16::MAX as usize) as u16;
    let visible_height = area.height.max(1);
    let auto_scroll = total_lines.saturating_sub(visible_height);
    let clamped_offset = state.scroll_offset.min(auto_scroll);
    let target_scroll = auto_scroll.saturating_sub(clamped_offset);
    let cur = state.rendered_scroll.get().min(auto_scroll);
    let next_scroll = if state.reduced_motion || cur == target_scroll {
        target_scroll
    } else if cur < target_scroll {
        let step = ((target_scroll - cur) / 3).max(1);
        (cur + step).min(target_scroll)
    } else {
        let step = ((cur - target_scroll) / 3).max(1);
        cur.saturating_sub(step).max(target_scroll)
    };
    state.rendered_scroll.set(next_scroll);

    let paragraph = Paragraph::new(visual_lines).scroll((next_scroll, 0));
    frame.render_widget(paragraph, area);

    // Draw only the thumb while scrolled up so normal copy-paste at bottom has no right-edge `│` noise.
    if state.scroll_offset > 0 && total_lines > visible_height && area.width >= 8 {
        let track_h = area.height as usize;
        let thumb_h = ((visible_height as usize * track_h) / total_lines as usize)
            .clamp(1, track_h);
        let max_scroll = auto_scroll.max(1) as usize;
        let thumb_top = ((next_scroll as usize) * track_h.saturating_sub(thumb_h)) / max_scroll;
        let x = area.x + area.width - 1;
        let pal = state.theme.palette();
        let buf = frame.buffer_mut();
        for row in thumb_top..(thumb_top + thumb_h) {
            let y = area.y + row as u16;
            let cell = &mut buf[(x, y)];
            cell.set_symbol("┃");
            cell.set_fg(pal.accent);
        }
    }
}

fn draw_todo_pane(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let total = state.todos.len();
    let done = state.todos.iter().filter(|(_, _, s)| s == "completed").count();
    let title = format!(" Tasks ({done}/{total}) ");

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (_, subj, status) in &state.todos {
        let (glyph, g_col) = match status.as_str() {
            "completed" => ("✓", pal.success),
            "in_progress" => ("▸", pal.accent),
            _ => ("○", pal.muted),
        };
        let text_style = if status == "completed" {
            Style::default().fg(pal.muted).add_modifier(Modifier::CROSSED_OUT)
        } else if status == "in_progress" {
            Style::default().fg(pal.fg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(pal.fg)
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {glyph} "), Style::default().fg(g_col)),
            Span::styled(subj.clone(), text_style),
        ]));
    }

    let pane = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(pal.modal_bg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(1))
                .border_style(Style::default().fg(pal.muted))
                .style(Style::default().bg(pal.modal_bg))
                .title(Span::styled(
                    title,
                    Style::default().fg(pal.muted).add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(pane, area);
}

fn draw_status_strip(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let accent = pal.accent;
    let mut spans: Vec<Span<'static>> = Vec::new();
    let scroll_pill = if state.scroll_offset > 0 {
        format!(" ↓ scrolled up ({} · End)", state.scroll_offset)
    } else {
        String::new()
    };
    let right_reserved = scroll_pill.chars().count();
    let max_left = (area.width as usize).saturating_sub(right_reserved + 1);
    let mut used = 0usize;

    let push_clamped = |spans: &mut Vec<Span<'static>>, used: &mut usize, text: String, style: Style| {
        if *used >= max_left {
            return;
        }
        let avail = max_left.saturating_sub(*used);
        let len = text.chars().count();
        if len <= avail {
            *used += len;
            spans.push(Span::styled(text, style));
        } else if avail > 3 {
            let mut s: String = text.chars().take(avail - 1).collect();
            s.push('…');
            *used += avail;
            spans.push(Span::styled(s, style));
        }
    };

    let effective_status = state.status_label();
    if let Some(status) = effective_status {
        let glyph = if state.is_busy || state.usage_loading {
            format!(" {} ", state.spinner_glyph())
        } else {
            " • ".to_string()
        };
        // pi colours only retry indicators with the warning tone; the rest stay accent.
        let tone = if state.status_kind.is_warning() {
            pal.warning
        } else {
            accent
        };
        push_clamped(
            &mut spans,
            &mut used,
            glyph,
            Style::default().fg(tone).add_modifier(Modifier::BOLD),
        );
        push_clamped(&mut spans, &mut used, status.to_string(), Style::default().fg(pal.fg));
        if state.is_busy {
            if let Some(started) = state.active_started {
                let secs = started.elapsed().as_secs_f64();
                push_clamped(
                    &mut spans,
                    &mut used,
                    format!(" · {secs:.1}s"),
                    Style::default().fg(pal.muted),
                );
            }
            // Retry / compaction / branch-summary labels already carry their own cancel hint.
            let kind_has_hint = !matches!(
                state.status_kind,
                StatusKind::Idle | StatusKind::Working
            );
            if !kind_has_hint
                && state.steering_prompts.is_empty()
                && state.queued_prompts.is_empty()
            {
                push_clamped(
                    &mut spans,
                    &mut used,
                    format!(
                        "  {} · {}",
                        crate::keyhint::key_hint(
                            crate::keyhint::KeyAction::Interrupt,
                            "interrupt"
                        ),
                        crate::keyhint::key_hint(
                            crate::keyhint::KeyAction::QueueFollowUp,
                            "queue"
                        )
                    ),
                    Style::default().fg(Color::DarkGray),
                );
            }
        }
    } else if !state.todos.is_empty() {
        let total = state.todos.len();
        let done = state.todos.iter().filter(|(_, _, s)| s == "completed").count();
        let active = state
            .todos
            .iter()
            .find(|(_, _, s)| s == "in_progress")
            .map(|(_, subj, _)| subj.as_str())
            .unwrap_or("pending");
        push_clamped(
            &mut spans,
            &mut used,
            format!(" ☑ {done}/{total} · ▸ {active}"),
            Style::default().fg(pal.muted),
        );
    }
    if let Some((ref cond, attempt, max_att)) = state.active_goal {
        let short_cond: String = cond.chars().take(32).collect();
        push_clamped(
            &mut spans,
            &mut used,
            format!("  ◎ goal {attempt}/{max_att}: {short_cond}"),
            Style::default().fg(pal.success).add_modifier(Modifier::BOLD),
        );
    }
    if let Some(first_s) = state.steering_prompts.front() {
        let one_line: String = first_s.lines().next().unwrap_or("").chars().take(36).collect();
        push_clamped(
            &mut spans,
            &mut used,
            format!("  ⚡ steer ({}): \"{one_line}\"", state.steering_prompts.len()),
            Style::default().fg(pal.secondary),
        );
    }
    if let Some(first_q) = state.queued_prompts.front() {
        let one_line: String = first_q.lines().next().unwrap_or("").chars().take(36).collect();
        push_clamped(
            &mut spans,
            &mut used,
            format!("  ↳ follow-up ({}): \"{one_line}\"", state.queued_prompts.len()),
            Style::default().fg(pal.complementary),
        );
    }
    if !state.pending_attachments.is_empty() {
        let names: Vec<&str> = state
            .pending_attachments
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect();
        push_clamped(
            &mut spans,
            &mut used,
            format!(" [📎 {}]", names.join(", ")),
            Style::default().fg(accent),
        );
    }
    if state.active_loops > 0 {
        push_clamped(
            &mut spans,
            &mut used,
            format!(" [↻ {} loop]", state.active_loops),
            Style::default().fg(pal.muted),
        );
    }
    if !scroll_pill.is_empty() {
        let pad = (area.width as usize).saturating_sub(used + right_reserved);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(scroll_pill, Style::default().fg(accent)));
    }
    if !spans.is_empty() {
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}


/// Tool duration exactly as pi renders it: `4.2s`, then `1m 30s`, then `2h 3m 4s`.
pub fn format_tool_duration(ms: u64) -> String {
    if ms < 60_000 {
        return format!("{:.1}s", ms as f64 / 1000.0);
    }
    let total_secs = ms / 1000;
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    if minutes < 60 {
        return format!("{minutes}m {seconds}s");
    }
    format!(
        "{}h {}m {seconds}s",
        minutes / 60,
        minutes % 60
    )
}

fn is_shell_tool(name: &str) -> bool {
    matches!(name, "bash" | "powershell" | "shell" | "exec")
}

/// pi's `COLLAPSED_ARGS_CHARS`: how much of the `key=json` preview fits on the title line.
const COLLAPSED_ARGS_CHARS: usize = 100;

/// A rendered tool header plus whether its body should stay collapsed.
pub struct ToolHeader {
    pub title: String,
    /// Extra header rows (pi's expanded `key: value` arg list).
    pub args_lines: Vec<String>,
    /// pi collapses `SKILL.md` / resource reads to a single header line.
    pub suppress_body: bool,
}

fn arg_str<'a>(args: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| {
        args.get(*k)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
    })
}

fn arg_u64(args: &serde_json::Value, key: &str) -> Option<u64> {
    let v = args.get(key)?;
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn file_basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn file_parent(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(idx) => file_basename(&trimmed[..idx]),
        None => "",
    }
}

/// pi's `formatReadLineRange`: `:10-39` for offset+limit, `:10` for offset only.
pub fn format_read_line_range(offset: Option<u64>, limit: Option<u64>) -> String {
    let Some(start) = offset else {
        return limit.map(|l| format!(":1-{l}")).unwrap_or_default();
    };
    match limit {
        Some(count) if count > 0 => format!(":{start}-{}", start + count - 1),
        _ => format!(":{start}"),
    }
}

/// Build the tool card header from the raw model arguments.
///
/// Mirrors pi's renderers: `read <path>:<range>`, shell tools show the command, and unknown
/// tools fall back to `name key=json ...`. Collapsed `read`s of skills/resources/docs collapse.
pub fn tool_call_header(name: &str, args_json: &str, expanded: bool) -> ToolHeader {
    let plain = |title: String| ToolHeader {
        title,
        args_lines: Vec::new(),
        suppress_body: false,
    };
    let args: serde_json::Value =
        serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);
    if args.is_null() {
        return plain(name.to_string());
    }

    match name {
        "read" => {
            let Some(path) = arg_str(&args, &["path", "file", "file_path"]) else {
                return plain(name.to_string());
            };
            let range = format_read_line_range(arg_u64(&args, "offset"), arg_u64(&args, "limit"));
            let base = file_basename(path);
            let hint = format!(
                " ({} to expand)",
                crate::keyhint::key_text(crate::keyhint::KeyAction::ExpandTools)
            );
            let hint = hint.as_str();
            if !expanded {
                if base.eq_ignore_ascii_case("SKILL.md") {
                    return ToolHeader {
                        title: format!("[skill] {}{range}{hint}", file_parent(path)),
                        args_lines: Vec::new(),
                        suppress_body: true,
                    };
                }
                if matches!(
                    base.to_ascii_uppercase().as_str(),
                    "AGENTS.MD" | "AGENTS.OVERRIDE.MD" | "CLAUDE.MD"
                ) {
                    return ToolHeader {
                        title: format!("read resource {path}{range}{hint}"),
                        args_lines: Vec::new(),
                        suppress_body: true,
                    };
                }
            }
            plain(format!("read {path}{range}"))
        }
        n if is_shell_tool(n) => {
            let Some(command) = arg_str(&args, &["command", "cmd"]) else {
                return plain(format!("{n} ..."));
            };
            let line = command.lines().next().unwrap_or("").trim();
            let shown: String = line.chars().take(120).collect();
            let timeout = arg_u64(&args, "timeout")
                .map(|t| format!(" (timeout {t}s)"))
                .unwrap_or_default();
            plain(format!("{n} {shown}{timeout}"))
        }
        "edit" | "write" => arg_str(&args, &["path", "file", "file_path"])
            .map(|p| plain(format!("{name} {p}")))
            .unwrap_or_else(|| plain(name.to_string())),
        "grep" | "find" => {
            let pattern = arg_str(&args, &["pattern", "query"]).unwrap_or("");
            let path = arg_str(&args, &["path"]).map(|p| format!(" {p}")).unwrap_or_default();
            plain(format!("{name} {pattern}{path}"))
        }
        "ls" => arg_str(&args, &["path"])
            .map(|p| plain(format!("ls {p}")))
            .unwrap_or_else(|| plain("ls".to_string())),
        "web_search" => {
            let query = arg_str(&args, &["query"]).or_else(|| {
                args.get("queries")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(serde_json::Value::as_str)
            });
            plain(format!("web_search {}", query.unwrap_or("")))
        }
        "fetch_content" => plain(format!(
            "fetch_content {}",
            arg_str(&args, &["url"]).unwrap_or("")
        )),
        "codemode" => {
            let calls: Vec<&str> = arg_str(&args, &["code"])
                .unwrap_or("")
                .split("tools.")
                .skip(1)
                .filter_map(|s| s.split('(').next())
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 24)
                .take(3)
                .collect();
            if calls.is_empty() {
                plain("codemode".to_string())
            } else {
                plain(format!("codemode {}", calls.join(" + ")))
            }
        }
        _ => {
            // pi's generic fallback: `title key=JSON key=JSON`, truncated when collapsed.
            let Some(obj) = args.as_object() else {
                return plain(name.to_string());
            };
            if obj.is_empty() {
                return plain(name.to_string());
            }
            if expanded {
                // pi: expanded shows one `key: value` row per argument, strings unquoted.
                let args_lines: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| {
                        let text = match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        format!("{k}: {}", text.replace('\n', "\n    "))
                    })
                    .collect();
                ToolHeader {
                    title: name.to_string(),
                    args_lines,
                    suppress_body: false,
                }
            } else {
                // pi: collapsed is `title key=JSON key=JSON`, cut to COLLAPSED_ARGS_CHARS.
                let pairs: String = obj
                    .iter()
                    .map(|(k, v)| {
                        let json = serde_json::to_string(v).unwrap_or_else(|_| v.to_string());
                        format!("{k}={json}")
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let shown: String = if pairs.chars().count() > COLLAPSED_ARGS_CHARS {
                    let mut s: String = pairs.chars().take(COLLAPSED_ARGS_CHARS - 3).collect();
                    s.push_str("...");
                    s
                } else {
                    pairs
                };
                plain(format!("{name} {shown}"))
            }
        }
    }
}

pub fn format_duration_ms(milliseconds: u64) -> String {
    let total = milliseconds / 1_000;
    let hours = total / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn thinking_color(level: &str) -> Color {
    match level {
        "minimal" => Color::Indexed(46),
        "low" => Color::Indexed(82),
        "medium" => Color::Indexed(118),
        "high" => Color::Indexed(220),
        "xhigh" => Color::Indexed(208),
        "max" => Color::Indexed(196),
        _ => Color::DarkGray,
    }
}

pub(crate) fn draw_footer(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let c_branch = pal.branch;
    let c_git_status = pal.git_status;
    let c_input = pal.input;
    let c_output = pal.output;
    let c_cache = pal.cache;
    let c_tps = pal.tps;
    let c_cost = pal.cost;
    let c_context = pal.context;

    // Line 1: repo · branch [git badges] (or dir name when not in git)
    let mut line1_spans: Vec<Span<'_>> = vec![Span::raw(" ")];
    let repo_label = if state.repo_name.is_empty() {
        state.cwd.clone()
    } else {
        state.repo_name.clone()
    };
    if let Some(branch) = &state.git_branch {
        line1_spans.push(Span::styled(repo_label, Style::default().fg(Color::DarkGray)));
        line1_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
        line1_spans.push(Span::styled(branch.clone(), Style::default().fg(c_branch)));
    } else {
        line1_spans.push(Span::styled(repo_label, Style::default().fg(c_branch)));
    }
    if !state.git_badges.is_empty() {
        line1_spans.push(Span::raw(" "));
        line1_spans.push(Span::styled(
            state.git_badges.clone(),
            Style::default().fg(c_git_status),
        ));
    }

    // Line 2: ↑ in · ↓ out · ↺ cache% · ⚡ t/s · $ cost · ◔ ctx%   <pad>   (provider) model • thinking
    let cache_str = match state.last_cache_rate {
        Some(rate) => format!("↺ {rate:.1}%"),
        None => "↺ —".to_string(),
    };
    let tps_str = match state.last_tps {
        Some(tps) => format!("⚡ {tps:.1} t/s"),
        None => "⚡ —".to_string(),
    };
    // After compaction `context_tokens` resets to 0 while cumulative `input_tokens > 0`:
    // mirror Pi's footer (`?/1.0M (auto)`) until the next LLM turn reports exact prompt size.
    let (ctx_pct, ctx_text) = if state.context_window == 0 {
        (0.0, "◔ ?".to_string())
    } else if state.context_tokens == 0 && state.input_tokens > 0 {
        (
            0.0,
            format!("◔ ?/{} (auto)", format_tokens(state.context_window)),
        )
    } else {
        let pct = (state.context_tokens as f64 / state.context_window as f64) * 100.0;
        (
            pct,
            format!(
                "◔ {pct:.1}%/{} (auto)",
                format_tokens(state.context_window)
            ),
        )
    };
    let ctx_color = if ctx_pct > 90.0 {
        pal.error
    } else if ctx_pct > 70.0 {
        pal.warning
    } else {
        c_context
    };

    // pi omits every zero-valued counter instead of printing padding zeros.
    let mut stats: Vec<(String, Color)> = Vec::new();
    if state.input_tokens > 0 {
        stats.push((format!("↑ {}", format_tokens(state.input_tokens)), c_input));
    }
    if state.output_tokens > 0 {
        stats.push((format!("↓ {}", format_tokens(state.output_tokens)), c_output));
    }
    if state.cache_read_tokens > 0 {
        stats.push((
            format!("R {}", format_tokens(state.cache_read_tokens)),
            c_cache,
        ));
    }
    if state.cache_write_tokens > 0 {
        stats.push((
            format!("W {}", format_tokens(state.cache_write_tokens)),
            c_cache,
        ));
    }
    if state.last_cache_rate.is_some() {
        stats.push((cache_str, c_cache));
    }
    if state.last_tps.is_some() {
        stats.push((tps_str, c_tps));
    }
    if state.cost_usd > 0.0 {
        stats.push((format!("$ {:.3}", state.cost_usd), c_cost));
    }
    stats.push((ctx_text, ctx_color));

    let right_model = format!("({}) {} ", state.provider, state.model);
    let right_sep = "• ";
    let right_think = format!("{} ", state.thinking_level);
    let right_len =
        right_model.chars().count() + right_sep.chars().count() + right_think.chars().count();
    let max_left = (area.width as usize).saturating_sub(right_len + 2);

    let mut line2_spans: Vec<Span<'_>> = vec![Span::raw(" ")];
    let mut left_len = 1usize;
    let mut added = 0usize;
    for (text, col) in &stats {
        let sep_len = if added > 0 { 3 } else { 0 };
        let item_len = text.chars().count();
        if left_len + sep_len + item_len > max_left {
            continue;
        }
        if added > 0 {
            line2_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
            left_len += 3;
        }
        left_len += item_len;
        line2_spans.push(Span::styled(text.clone(), Style::default().fg(*col)));
        added += 1;
    }

    let pad2 = (area.width as usize).saturating_sub(left_len + right_len);
    line2_spans.push(Span::raw(" ".repeat(pad2.max(1))));
    line2_spans.push(Span::styled(right_model, Style::default().fg(Color::White)));
    line2_spans.push(Span::styled(right_sep, Style::default().fg(Color::DarkGray)));
    line2_spans.push(Span::styled(
        right_think,
        Style::default().fg(thinking_color(&state.thinking_level)),
    ));

    // Line 3: left = ▸ MODE · provider mini quota bar, right = ◷ active runtime timer
    let mode_color = match state.mode {
        AgentMode::Normal => pal.accent,
        AgentMode::Plan => pal.warning,
        AgentMode::Ask => pal.tps,
        AgentMode::Rea => pal.secondary,
        AgentMode::Cyber => pal.error,
    };
    let mode_pill = format!(" ▸ {}", state.mode.name());
    let quota_badge = state
        .provider_quota_badge
        .get(&state.provider)
        .cloned()
        .unwrap_or_default();
    let runtime = format!("◷ {} ", format_duration_ms(state.total_active_ms()));

    let mut left3_len = mode_pill.chars().count();
    let mut line3_spans = vec![Span::styled(
        mode_pill,
        Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
    )];
    if !quota_badge.is_empty() {
        let sep = " · ";
        left3_len += sep.chars().count() + quota_badge.chars().count();
        line3_spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));
        line3_spans.push(Span::styled(quota_badge, Style::default().fg(Color::Gray)));
    }
    let perm_label = if state.yolo { "yolo" } else { "ask" };
    let perm_color = if state.yolo {
        pal.warning
    } else {
        Color::DarkGray
    };
    let right3_len = perm_label.chars().count() + 3 + runtime.chars().count();
    let pad3 = (area.width as usize).saturating_sub(left3_len + right3_len);
    line3_spans.push(Span::raw(" ".repeat(pad3)));
    line3_spans.push(Span::styled(
        perm_label,
        if state.yolo {
            Style::default().fg(perm_color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(perm_color)
        },
    ));
    line3_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
    line3_spans.push(Span::styled(runtime, Style::default().fg(Color::DarkGray)));
    let line3 = Line::from(line3_spans);

    let lines = vec![Line::from(line1_spans), Line::from(line2_spans), line3];
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod wrap_tests {
    use super::*;

    fn line_text(l: &Line<'_>) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn preserves_hanging_indent_on_wrapped_bullets() {
        let input = vec![Line::from(vec![
            Span::raw("   • "),
            Span::raw("dcpoc (Workplace Hub), Portfolio_Website (:3000), ocr-pipeline"),
        ])];
        let wrapped = wrap_lines_exact(input, 36);
        assert!(wrapped.len() >= 2);
        assert!(line_text(&wrapped[0]).starts_with("   • dcpoc"));
        assert!(
            line_text(&wrapped[1]).starts_with("     "),
            "continuation should align under bullet text, got {:?}",
            line_text(&wrapped[1])
        );
    }

    #[test]
    fn keeps_cross_span_open_paren_attached_to_styled_code() {
        let input = vec![Line::from(vec![
            Span::raw("   • mail-inbound ("),
            Span::styled("server.mjs", Style::default().fg(Color::Cyan)),
            Span::raw(" :8825)"),
        ])];
        // Width 20 fits "   • mail-inbound (" (19 chars) but not "(server.mjs", so "(" must wrap with "server.mjs".
        let wrapped = wrap_lines_exact(input, 20);
        assert!(wrapped.len() >= 2);
        assert!(
            !line_text(&wrapped[0]).trim_end().ends_with('('),
            "should not strand '(' at end of line: {:?}",
            line_text(&wrapped[0])
        );
        assert!(
            line_text(&wrapped[1]).contains("(server.mjs"),
            "continuation should keep '(server.mjs' together: {:?}",
            line_text(&wrapped[1])
        );
    }

    #[test]
    fn transcript_cache_hits_on_unchanged_blocks_and_invalidates_on_change() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut state = TuiState::new("vibe".into(), "devstral".into(), ".".into(), None);
        state.blocks.push(TranscriptBlock::User("hello".into()));
        state.blocks.push(TranscriptBlock::Assistant("world".into()));

        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw_frame(f, &state)).unwrap();
        assert_eq!(state.transcript_cache.borrow().entries.len(), 2);
        let first_h0 = state.transcript_cache.borrow().entries[0].0;
        let first_h1 = state.transcript_cache.borrow().entries[1].0;

        // Second draw at same width/theme/toggles keeps cached hashes intact
        term.draw(|f| draw_frame(f, &state)).unwrap();
        assert_eq!(state.transcript_cache.borrow().entries[0].0, first_h0);
        assert_eq!(state.transcript_cache.borrow().entries[1].0, first_h1);

        // Mutating last block updates only that block's hash
        if let Some(TranscriptBlock::Assistant(s)) = state.blocks.last_mut() {
            s.push_str(" more");
        }
        term.draw(|f| draw_frame(f, &state)).unwrap();
        assert_eq!(state.transcript_cache.borrow().entries[0].0, first_h0);
        assert_ne!(state.transcript_cache.borrow().entries[1].0, first_h1);

        // Width change clears and repopulates at new width
        let mut term_wide = Terminal::new(TestBackend::new(100, 24)).unwrap();
        term_wide.draw(|f| draw_frame(f, &state)).unwrap();
        assert_eq!(state.transcript_cache.borrow().width, 100);
        assert_eq!(state.transcript_cache.borrow().entries.len(), 2);
    }
}

