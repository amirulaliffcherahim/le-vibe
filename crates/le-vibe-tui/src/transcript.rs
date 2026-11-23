use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::commands::filter_slash_commands_with_dynamic;
use crate::editor::{draw_editor, wrap_editor_lines};
use crate::markdown::render_markdown_lines;
use crate::modal::{draw_at_file_popup, draw_modal, draw_slash_popup};
use crate::mode::AgentMode;
use crate::state::{ActiveModal, TranscriptBlock, TuiState, format_tokens};

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

    draw_transcript(frame, chunks[0], state);
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

/// Pi-style full-width borderless tinted surface (zero `▎` noise, zero jagged right edges).
fn push_pi_surface(
    lines: &mut Vec<Line<'static>>,
    area_width: u16,
    title: &str,
    body: &str,
    hidden_count: usize,
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

    if hidden_count > 0 {
        let hint = format!("... ({hidden_count} earlier lines, ctrl+o to expand)");
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

    if !body.is_empty() {
        lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
        for raw_line in body.lines() {
            let clean = sanitize_display_line(raw_line);
            let line_style = if clean.starts_with('+') && !clean.starts_with("+++") {
                Style::default().fg(style.diff_ok).bg(style.bg)
            } else if clean.starts_with('-') && !clean.starts_with("---") {
                Style::default().fg(style.diff_err).bg(style.bg)
            } else {
                style.body.bg(style.bg)
            };
            if clean.is_empty() {
                lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
                continue;
            }
            let mut remaining = clean.as_str();
            while !remaining.is_empty() {
                let (chunk, rest) = split_at_width(remaining, inner_w);
                let pad = total_w.saturating_sub(1 + Span::raw(chunk).width());
                lines.push(
                    Line::from(vec![
                        Span::styled(" ", bg_style),
                        Span::styled(chunk.to_string(), line_style),
                        Span::styled(" ".repeat(pad), bg_style),
                    ])
                    .style(bg_style),
                );
                remaining = rest;
            }
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
    fg: Color,
) {
    let total_w = (area_width as usize).saturating_sub(2).max(20);
    let inner_w = total_w.saturating_sub(2).max(8);
    let bg_style = Style::default().bg(bg);
    let body_style = Style::default().fg(fg).bg(bg);

    lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
    for raw in text.lines() {
        let clean = sanitize_display_line(raw);
        if clean.is_empty() {
            lines.push(Line::from(Span::styled(" ".repeat(total_w), bg_style)).style(bg_style));
            continue;
        }
        let mut remaining = clean.as_str();
        while !remaining.is_empty() {
            let (chunk, rest) = split_at_width(remaining, inner_w);
            let pad = total_w.saturating_sub(1 + Span::raw(chunk).width());
            lines.push(
                Line::from(vec![
                    Span::styled(" ", bg_style),
                    Span::styled(chunk.to_string(), body_style),
                    Span::styled(" ".repeat(pad), bg_style),
                ])
                .style(bg_style),
            );
            remaining = rest;
        }
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
        TranscriptBlock::Tool { name, status, output } => {
            3u8.hash(&mut h);
            name.hash(&mut h);
            status.hash(&mut h);
            output.hash(&mut h);
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
) -> Vec<Line<'static>> {
    let pal = state.theme.palette();
    let accent = pal.accent;
    let mut lines: Vec<Line<'static>> = Vec::new();
    match block {
        TranscriptBlock::User(text) => {
            lines.push(Line::from(""));
            push_user_surface(
                &mut lines,
                area_width,
                text,
                state.theme.user_surface(),
                pal.fg,
            );
        }
        TranscriptBlock::Thinking(text) => {
            if !text.is_empty() {
                lines.push(Line::from(""));
                let line_count = text.lines().count().max(1);
                if state.show_thinking {
                    for l in text.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(" ", Style::default()),
                            Span::styled(
                                l.to_string(),
                                Style::default()
                                    .fg(pal.muted)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                        ]));
                    }
                } else {
                    let unit = if line_count == 1 { "line" } else { "lines" };
                    lines.push(Line::from(Span::styled(
                        format!(" Thinking... ({line_count} {unit}, ctrl+t to expand)"),
                        Style::default()
                            .fg(pal.muted)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
            }
        }
        TranscriptBlock::Assistant(text) => {
            if !text.is_empty() {
                lines.push(Line::from(""));
                lines.extend(render_markdown_lines(text, &pal, area_width, state.expand_tools));
            }
        }
        TranscriptBlock::Tool {
            name,
            status,
            output,
        } => {
            lines.push(Line::from(""));
            let is_err = status == "failed";
            let title = format!("{name} · {status}");
            let all_out: Vec<&str> = output.trim().lines().collect();
            let max_lines = if state.expand_tools { 40 } else { 5 };
            let hidden = all_out.len().saturating_sub(max_lines);
            let preview = if hidden > 0 {
                all_out[all_out.len() - max_lines..].join("\n")
            } else {
                all_out.join("\n")
            };
            push_pi_surface(
                &mut lines,
                area_width,
                &title,
                &preview,
                hidden,
                SurfaceStyle {
                    title: if is_err { pal.error } else { pal.fg },
                    bg: if is_err {
                        pal.card_err_bg
                    } else if status == "running" || status == "pending" {
                        pal.card_quiet_bg
                    } else {
                        pal.card_ok_bg
                    },
                    body: Style::default().fg(pal.muted),
                    diff_ok: pal.success,
                    diff_err: pal.error,
                },
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
        let h = hash_block(block);
        let hit = cache.entries.get(idx).is_some_and(|(ch, _)| *ch == h);
        if !hit {
            let rendered = render_block_wrapped(block, area.width, wrap_width, state);
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

    let effective_status = state
        .status_text
        .as_deref()
        .or_else(|| state.is_busy.then_some("Working..."));
    if let Some(status) = effective_status {
        let glyph = if state.is_busy || state.usage_loading {
            format!(" {} ", state.spinner_glyph())
        } else {
            " • ".to_string()
        };
        push_clamped(
            &mut spans,
            &mut used,
            glyph,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        );
        push_clamped(&mut spans, &mut used, status.to_string(), Style::default().fg(pal.fg));
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
    let ctx_pct = if state.context_window > 0 {
        (state.input_tokens as f64 / state.context_window as f64) * 100.0
    } else {
        0.0
    };
    // Pi colors context pressure at the same thresholds and shows the window size.
    let ctx_text = if state.context_window > 0 {
        format!("◔ {ctx_pct:.1}%/{} (auto)", format_tokens(state.context_window))
    } else {
        "◔ ?".to_string()
    };
    let ctx_color = if ctx_pct > 90.0 {
        pal.error
    } else if ctx_pct > 70.0 {
        pal.warning
    } else {
        c_context
    };

    let stats: [(&str, String, Color); 6] = [
        ("", format!("↑ {}", format_tokens(state.input_tokens)), c_input),
        ("", format!("↓ {}", format_tokens(state.output_tokens)), c_output),
        ("", cache_str, c_cache),
        ("", tps_str, c_tps),
        ("", format!("$ {:.3}", state.cost_usd), c_cost),
        ("", ctx_text, ctx_color),
    ];

    let right_model = format!("({}) {} ", state.provider, state.model);
    let right_sep = "• ";
    let right_think = format!("{} ", state.thinking_level);
    let right_len =
        right_model.chars().count() + right_sep.chars().count() + right_think.chars().count();
    let max_left = (area.width as usize).saturating_sub(right_len + 2);

    let mut line2_spans: Vec<Span<'_>> = vec![Span::raw(" ")];
    let mut left_len = 1usize;
    let mut added = 0usize;
    for (_, text, col) in &stats {
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

