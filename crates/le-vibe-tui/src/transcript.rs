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

    let header_h = if state.quiet_startup { 1 } else { 4 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),      // Sleepy Owl Header (or compact 1-line in quiet mode)
            Constraint::Min(5),                // Transcript viewport
            Constraint::Length(1),             // Dedicated working/status strip above editor
            Constraint::Length(editor_height), // Auto-wrapping Editor box (1-5 lines)
            Constraint::Length(3),             // pi-footer (identity/git, usage/model, mode+quota/runtime)
        ])
        .split(area);

    draw_header(frame, chunks[0], state);
    draw_transcript(frame, chunks[1], state);
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

fn draw_header(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    let secondary = state.theme.secondary();
    if state.quiet_startup {
        let line = Line::from(vec![
            Span::styled(" le-vibe ", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            Span::styled(format!("v{} ", state.version), Style::default().fg(Color::DarkGray)),
            Span::styled("· ", Style::default().fg(Color::DarkGray)),
            Span::styled("Shift+Tab", Style::default().fg(Color::Gray)),
            Span::styled(" mode · ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+P", Style::default().fg(Color::Gray)),
            Span::styled(" model · ", Style::default().fg(Color::DarkGray)),
            Span::styled("/", Style::default().fg(Color::Gray)),
            Span::styled(" commands", Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "  ,___,  ",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "le-vibe ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{} ", state.version),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("zZz", Style::default().fg(secondary)),
        ]),
        Line::from(vec![
            Span::styled(
                "  (-v-)  ",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("esc", Style::default().fg(Color::Gray)),
            Span::styled(" interrupt · ", Style::default().fg(Color::DarkGray)),
            Span::styled("ctrl+c", Style::default().fg(Color::Gray)),
            Span::styled(" clear/exit · ", Style::default().fg(Color::DarkGray)),
            Span::styled("ctrl+p", Style::default().fg(Color::Gray)),
            Span::styled(" cycle model · ", Style::default().fg(Color::DarkGray)),
            Span::styled("shift+tab", Style::default().fg(Color::Gray)),
            Span::styled(" mode · ", Style::default().fg(Color::DarkGray)),
            Span::styled("!", Style::default().fg(Color::Gray)),
            Span::styled(" bash · ", Style::default().fg(Color::DarkGray)),
            Span::styled("/", Style::default().fg(Color::Gray)),
            Span::styled(" commands", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" /)_(_\\  ", Style::default().fg(accent)),
            Span::styled(
                "Rust Pi harness · Unified Python provider engine (Vibe, DeepSeek, Antigravity)",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

struct CardStyle {
    bar: Color,
    title: Color,
    bg: Color,
    body: Style,
}

fn push_shaded_card(
    lines: &mut Vec<Line<'static>>,
    area_width: u16,
    title: &str,
    body: &str,
    style: CardStyle,
) {
    let total_w = (area_width as usize).saturating_sub(2).max(20);
    let inner_w = total_w.saturating_sub(4);
    let bg_style = Style::default().bg(style.bg);
    let bar_style = Style::default().fg(style.bar).bg(style.bg);

    // Top breathing room
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(" ".repeat(total_w), bg_style),
    ]));

    // Header line inside shaded card
    let header_pad = inner_w.saturating_sub(title.chars().count());
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("▎ ", bar_style),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(style.title)
                .bg(style.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(header_pad + 2), bg_style),
    ]));

    let raw_lines: Vec<&str> = if body.is_empty() {
        Vec::new()
    } else {
        body.lines().collect()
    };
    for raw in raw_lines {
        let mut remaining = raw;
        loop {
            let chunk: String = remaining.chars().take(inner_w).collect();
            let consumed = chunk.len();
            let pad = inner_w.saturating_sub(chunk.chars().count());
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled("▎ ", bar_style),
                Span::styled(chunk, style.body.bg(style.bg)),
                Span::styled(" ".repeat(pad + 2), bg_style),
            ]));
            remaining = &remaining[consumed..];
            if remaining.is_empty() {
                break;
            }
        }
    }

    // Bottom breathing room
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(" ".repeat(total_w), bg_style),
    ]));
}

fn draw_transcript(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let accent = pal.accent;
    let secondary = pal.secondary;
    let mut lines: Vec<Line<'static>> = Vec::new();

    for block in &state.blocks {
        match block {
            TranscriptBlock::User(text) => {
                lines.push(Line::from(""));
                for (idx, l) in text.lines().enumerate() {
                    let prefix = if idx == 0 { " › " } else { "   " };
                    lines.push(Line::from(vec![
                        Span::styled(
                            prefix,
                            Style::default().fg(accent).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            l.to_string(),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
            }
            TranscriptBlock::Thinking(text) => {
                if !text.is_empty() {
                    lines.push(Line::from(""));
                    if state.show_thinking {
                        push_shaded_card(
                            &mut lines,
                            area.width,
                            "thinking",
                            text,
                            CardStyle {
                                bar: secondary,
                                title: secondary,
                                bg: pal.card_thinking_bg,
                                body: Style::default()
                                    .fg(Color::Gray)
                                    .add_modifier(Modifier::ITALIC),
                            },
                        );
                    } else {
                        let line_count = text.lines().count().max(1);
                        push_shaded_card(
                            &mut lines,
                            area.width,
                            &format!("thinking ({line_count} lines · Ctrl+T to expand)"),
                            "",
                            CardStyle {
                                bar: secondary,
                                title: secondary,
                                bg: pal.card_thinking_bg,
                                body: Style::default().fg(Color::DarkGray),
                            },
                        );
                    }
                }
            }
            TranscriptBlock::Assistant(text) => {
                if !text.is_empty() {
                    lines.push(Line::from(""));
                    lines.extend(render_markdown_lines(text, &pal));
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
                let max_lines = if state.expand_tools { 40 } else { 3 };
                let preview_lines: Vec<&str> = output.trim().lines().take(max_lines).collect();
                let preview = if preview_lines.is_empty() {
                    String::new()
                } else {
                    preview_lines.join("\n")
                };
                push_shaded_card(
                    &mut lines,
                    area.width,
                    &title,
                    &preview,
                    CardStyle {
                        bar: if is_err { pal.error } else { pal.success },
                        title: if is_err { pal.error } else { pal.warning },
                        bg: if is_err { pal.card_err_bg } else { pal.card_ok_bg },
                        body: Style::default().fg(Color::Gray),
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
                let body = output.trim();
                push_shaded_card(
                    &mut lines,
                    area.width,
                    &tag,
                    body,
                    CardStyle {
                        bar: if *in_context { pal.success } else { Color::DarkGray },
                        title: if *in_context { pal.success } else { Color::Gray },
                        bg: if *in_context { pal.card_ok_bg } else { pal.card_quiet_bg },
                        body: Style::default().fg(Color::Gray),
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
    }

    let visible_height = area.height.saturating_sub(1);
    let total_lines = lines.len() as u16;
    let auto_scroll = total_lines.saturating_sub(visible_height);
    let target_scroll = auto_scroll.saturating_sub(state.scroll_offset);
    let cur = state.rendered_scroll.get();
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

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((next_scroll, 0));
    frame.render_widget(paragraph, area);
}

fn draw_status_strip(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(status) = &state.status_text {
        spans.push(Span::styled(
            format!(" {} ", state.spinner_glyph()),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(status.clone(), Style::default().fg(Color::Gray)));
    } else if !state.todos.is_empty() {
        let total = state.todos.len();
        let done = state.todos.iter().filter(|(_, _, s)| s == "completed").count();
        let active = state
            .todos
            .iter()
            .find(|(_, _, s)| s == "in_progress")
            .map(|(_, subj, _)| subj.as_str())
            .unwrap_or("pending");
        spans.push(Span::styled(
            format!(" ☑ {done}/{total} · ▸ {active}"),
            Style::default().fg(Color::Gray),
        ));
    }
    if !state.pending_attachments.is_empty() {
        let names: Vec<&str> = state
            .pending_attachments
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect();
        spans.push(Span::styled(
            format!(" [📎 {}]", names.join(", ")),
            Style::default().fg(accent),
        ));
    }
    if state.active_loops > 0 {
        spans.push(Span::styled(
            format!(" [↻ {} loop]", state.active_loops),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if state.scroll_offset > 0 {
        let scroll_pill = format!(" ↓ scrolled up ({} · End)", state.scroll_offset);
        let left_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let right_len = scroll_pill.chars().count();
        let pad = (area.width as usize).saturating_sub(left_len + right_len);
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

    let stats: [(&str, String, Color); 6] = [
        ("", format!("↑ {}", format_tokens(state.input_tokens)), c_input),
        ("", format!("↓ {}", format_tokens(state.output_tokens)), c_output),
        ("", cache_str, c_cache),
        ("", tps_str, c_tps),
        ("", format!("$ {:.3}", state.cost_usd), c_cost),
        ("", format!("◔ {ctx_pct:.1}%"), c_context),
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

