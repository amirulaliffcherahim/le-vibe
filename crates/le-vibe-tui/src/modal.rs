use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph, Wrap};

use crate::commands::SlashCommandItem;
use crate::mode::{ALL_MODES, ALL_SHELLS};
use crate::state::{ActiveModal, TuiState};
use crate::theme::{ALL_THEMES, ThemeId};

fn truncate_ellipsis(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = s.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

pub(crate) fn draw_slash_popup(
    frame: &mut Frame<'_>,
    editor_area: Rect,
    matches: &[SlashCommandItem],
    selected_idx: usize,
    theme: ThemeId,
) {
    let pal = theme.palette();
    let accent = pal.accent;
    let max_h = editor_area.y.clamp(3, 16);
    let height = (matches.len() as u16 + 2).min(max_h);
    let y = editor_area.y.saturating_sub(height);
    let popup_area = Rect {
        x: editor_area.x,
        y,
        width: editor_area.width,
        height,
    };
    let inner_w = popup_area.width.saturating_sub(2) as usize;
    let visible_rows = popup_area.height.saturating_sub(2).max(1) as usize;
    let start = if selected_idx >= visible_rows {
        selected_idx + 1 - visible_rows
    } else {
        0
    };
    let end = (start + visible_rows).min(matches.len());

    let max_cmd_len = matches
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(14)
        .min((inner_w / 2).max(16))
        + 2;

    frame.render_widget(Clear, popup_area);
    let items: Vec<ListItem<'_>> = matches[start..end]
        .iter()
        .enumerate()
        .map(|(offset, cmd)| {
            let idx = start + offset;
            let is_sel = idx == selected_idx;
            let pointer = if is_sel { " ▸ " } else { "   " };
            let name_style = if is_sel {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let desc_style = if is_sel {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let cmd_chars = cmd.name.chars().count();
            let pad_len = max_cmd_len.saturating_sub(cmd_chars);
            let used = 3 + cmd_chars + pad_len;
            let desc_budget = inner_w.saturating_sub(used);
            let desc = truncate_ellipsis(&cmd.description, desc_budget);
            ListItem::new(Line::from(vec![
                Span::styled(pointer, name_style),
                Span::styled(cmd.name.clone(), name_style),
                Span::raw(" ".repeat(pad_len)),
                Span::styled(desc, desc_style),
            ]))
        })
        .collect();

    let title = truncate_ellipsis(
        " Commands (↑/↓ · Tab complete · Enter run) ",
        inner_w,
    );
    let list = List::new(items)
        .style(Style::default().bg(pal.modal_bg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.muted))
                .style(Style::default().bg(pal.modal_bg))
                .title(Span::styled(title, Style::default().fg(accent))),
        );
    frame.render_widget(list, popup_area);
}

pub(crate) fn draw_at_file_popup(
    frame: &mut Frame<'_>,
    editor_area: Rect,
    files: &[String],
    selected_idx: usize,
    theme: ThemeId,
) {
    let pal = theme.palette();
    let accent = pal.accent;
    let max_h = editor_area.y.clamp(3, 14);
    let height = (files.len() as u16 + 2).min(max_h);
    let y = editor_area.y.saturating_sub(height);
    let popup_area = Rect {
        x: editor_area.x,
        y,
        width: editor_area.width,
        height,
    };
    let inner_w = popup_area.width.saturating_sub(2) as usize;
    let visible_rows = popup_area.height.saturating_sub(2).max(1) as usize;
    let sel = selected_idx.min(files.len().saturating_sub(1));
    let start = if sel >= visible_rows {
        sel + 1 - visible_rows
    } else {
        0
    };
    let end = (start + visible_rows).min(files.len());

    frame.render_widget(Clear, popup_area);
    let items: Vec<ListItem<'_>> = files[start..end]
        .iter()
        .enumerate()
        .map(|(offset, path)| {
            let is_sel = start + offset == sel;
            let ptr = if is_sel { " ▸ " } else { "   " };
            let style = if is_sel {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let fitted = truncate_ellipsis(path, inner_w.saturating_sub(3));
            ListItem::new(Line::from(vec![
                Span::styled(ptr, style),
                Span::styled(fitted, style),
            ]))
        })
        .collect();
    let title = truncate_ellipsis(" Files (@path · ↑/↓ · Tab/Enter insert) ", inner_w);
    let list = List::new(items)
        .style(Style::default().bg(pal.modal_bg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.muted))
                .style(Style::default().bg(pal.modal_bg))
                .title(Span::styled(title, Style::default().fg(accent))),
        );
    frame.render_widget(list, popup_area);
}

pub(crate) fn draw_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    if state.modal == ActiveModal::UsageModal {
        draw_usage_modal(frame, area, state);
        return;
    }
    if state.modal == ActiveModal::TrustModal {
        draw_trust_modal(frame, area, state);
        return;
    }
    if state.modal == ActiveModal::ApprovalModal {
        draw_approval_modal(frame, area, state);
        return;
    }
    if state.modal == ActiveModal::QuestionModal {
        draw_question_modal(frame, area, state);
        return;
    }
    if state.modal == ActiveModal::HelpModal
        || state.modal == ActiveModal::ChangelogModal
        || state.modal == ActiveModal::DebugModal
        || state.modal == ActiveModal::InfoModal
    {
        draw_text_modal(frame, area, state);
        return;
    }
    let (title, rows): (String, Vec<(String, String, String)>) = match state.modal {
        ActiveModal::None
        | ActiveModal::UsageModal
        | ActiveModal::ApprovalModal
        | ActiveModal::QuestionModal
        | ActiveModal::HelpModal
        | ActiveModal::ChangelogModal
        | ActiveModal::DebugModal
        | ActiveModal::InfoModal
        | ActiveModal::TrustModal => return,
        ActiveModal::ModelPicker => {
            let base = if state.scope_view {
                " Scoped Models · Tab: all · Space: pin · Ctrl+S: save "
            } else {
                " Models · Tab: scoped · Space: pin · Ctrl+S: save · Type to filter "
            };
            let title = if state.modal_filter.is_empty() {
                base.to_string()
            } else {
                format!(" Models [filter: {}] · Backspace · Enter · Esc ", state.modal_filter)
            };
            let rows = state
                .picker_rows()
                .iter()
                .map(|(p, id, name)| {
                    let marker = if state.is_scoped(p, id) { "●" } else { "○" };
                    (format!("{marker} ({p})"), id.clone(), name.clone())
                })
                .collect();
            (title, rows)
        }
        ActiveModal::ThemePicker => (
            " Select Theme (↑/↓ Enter Esc) ".to_string(),
            ALL_THEMES
                .iter()
                .map(|t| (t.name().to_string(), String::new(), t.description().to_string()))
                .collect(),
        ),
        ActiveModal::ExportPicker => (
            " Export Session (↑/↓ Enter Esc · or /export [md|html] [path]) ".to_string(),
            vec![
                (
                    "md".into(),
                    ".md".into(),
                    "Export full session as clean Markdown".into(),
                ),
                (
                    "html".into(),
                    ".html".into(),
                    "Export full session as self-contained styled HTML".into(),
                ),
            ],
        ),
        ActiveModal::ModePicker => (
            " Select Mode (↑/↓ Enter Esc · Shift+Tab cycles · Alt+W workflows) ".to_string(),
            ALL_MODES
                .iter()
                .map(|m| (m.name().to_string(), String::new(), m.description().to_string()))
                .collect(),
        ),
        ActiveModal::WorkflowPicker => (
            format!(
                " {} Workflows — Pick a workflow to initiate (↑/↓ Enter Esc) ",
                state.mode.name().to_uppercase()
            ),
            state
                .mode
                .workflows()
                .iter()
                .map(|w| (w.label.to_string(), String::new(), w.description.to_string()))
                .collect(),
        ),
        ActiveModal::TermPicker => (
            " Select Terminal Shell (↑/↓ Enter Esc · or /term <exe>) ".to_string(),
            ALL_SHELLS
                .iter()
                .map(|(name, desc)| {
                    let active = if *name == state.shell { "●" } else { " " };
                    (format!("{active} {name}"), String::new(), (*desc).to_string())
                })
                .collect(),
        ),
        ActiveModal::ConfigModal => {
            let scope = state.config_scope_label();
            let title = if state.modal_filter.is_empty() {
                format!(
                    " Settings [save: {scope}] · Tab: scope · Enter/Space: toggle · Type to filter · Esc "
                )
            } else {
                format!(
                    " Settings [save: {scope} · filter: {}] · Tab: scope · Enter · Esc ",
                    state.modal_filter
                )
            };
            let rows: Vec<(String, String, String)> = state
                .config_rows()
                .into_iter()
                .map(|(_, label, val, desc)| (label.to_string(), val, desc.to_string()))
                .collect();
            (title, rows)
        }
        ActiveModal::LoginPicker | ActiveModal::LogoutPicker => {
            let header = if state.modal == ActiveModal::LoginPicker {
                " Auth Status & Login — Press Enter to Login (↑/↓ Enter Esc) "
            } else {
                " Logout Provider — Press Enter to Remove Credentials (↑/↓ Enter Esc) "
            };
            let list = ["vibe", "deepseek", "agy"]
                .iter()
                .map(|p| {
                    let (authed, src) = state
                        .provider_auth
                        .get(*p)
                        .cloned()
                        .unwrap_or((false, "none".into()));
                    let badge = if authed {
                        format!("✓ authenticated ({src})")
                    } else {
                        "✗ not logged in".to_string()
                    };
                    let flow = match *p {
                        "vibe" => "Mistral OIDC browser login",
                        "agy" => "Google Antigravity OAuth browser login",
                        _ => "DEEPSEEK_API_KEY / saved key",
                    };
                    ((*p).to_string(), badge, flow.to_string())
                })
                .collect();
            (header.to_string(), list)
        }
        ActiveModal::RewindPicker => (
            " Rewind Checkpoint — Pick user input to rewind to (↑/↓ Enter Esc) ".to_string(),
            if state.rewind_rows.is_empty() {
                vec![("(empty)".into(), String::new(), "No user checkpoints yet.".into())]
            } else {
                state.rewind_rows.clone()
            },
        ),
        ActiveModal::SessionPicker => {
            let title = if state.modal_filter.is_empty() {
                " Saved Sessions — Enter to resume · Type to filter · Esc ".to_string()
            } else {
                format!(
                    " Saved Sessions [filter: {}] · Backspace · Enter · Esc ",
                    state.modal_filter
                )
            };
            let rows = state.filtered_session_rows();
            let display = if rows.is_empty() {
                vec![("(none)".into(), String::new(), "No matching saved sessions.".into())]
            } else {
                rows.iter()
                    .map(|(path, label, preview)| {
                        let name = std::path::Path::new(path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(path)
                            .to_string();
                        (name, label.clone(), preview.clone())
                    })
                    .collect()
            };
            (title, display)
        }
    };

    let height = (rows.len() as u16 + 4)
        .min(area.height.saturating_sub(4))
        .max(5);
    // Responsive width: 75% of terminal width, clamped between 64 and 116 columns.
    let target_w = ((area.width as u32 * 75) / 100) as u16;
    let max_w = area.width.saturating_sub(4).max(20);
    let width = target_w.clamp(64.min(max_w), max_w.min(116));
    let popup_area = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let inner_w = popup_area.width.saturating_sub(2) as usize;
    let visible_rows = popup_area.height.saturating_sub(2).max(1) as usize;
    let selected = state.modal_idx.min(rows.len().saturating_sub(1));
    let start = if selected >= visible_rows {
        selected + 1 - visible_rows
    } else {
        0
    };
    let end = (start + visible_rows).min(rows.len());

    let col1_w = rows
        .iter()
        .map(|(a, _, _)| a.chars().count())
        .max()
        .unwrap_or(8)
        .min((inner_w / 3).max(10))
        + 2;
    let has_col2 = rows.iter().any(|(_, b, _)| !b.is_empty());
    let col2_w = if has_col2 {
        rows.iter()
            .map(|(_, b, _)| b.chars().count())
            .max()
            .unwrap_or(0)
            .min((inner_w / 3).max(10))
            + 2
    } else {
        0
    };

    frame.render_widget(Clear, popup_area);
    let items: Vec<ListItem<'_>> = rows[start..end]
        .iter()
        .enumerate()
        .map(|(offset, (c1, c2, c3))| {
            let idx = start + offset;
            let is_sel = idx == selected;
            let pointer = if is_sel { " ▸ " } else { "   " };
            let s1 = if is_sel {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            let s2 = if is_sel {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let s3 = if is_sel {
                Style::default().fg(Color::Gray)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let c1_fit = truncate_ellipsis(c1, col1_w.saturating_sub(1));
            let pad1 = " ".repeat(col1_w.saturating_sub(c1_fit.chars().count()));
            let c2_fit = if has_col2 {
                truncate_ellipsis(c2, col2_w.saturating_sub(1))
            } else {
                String::new()
            };
            let pad2 = if has_col2 {
                " ".repeat(col2_w.saturating_sub(c2_fit.chars().count()))
            } else {
                String::new()
            };
            let used = 3 + col1_w + col2_w;
            let c3_fit = truncate_ellipsis(c3, inner_w.saturating_sub(used));
            ListItem::new(Line::from(vec![
                Span::styled(pointer, s1),
                Span::styled(c1_fit, s1),
                Span::raw(pad1),
                Span::styled(c2_fit, s2),
                Span::raw(pad2),
                Span::styled(c3_fit, s3),
            ]))
        })
        .collect();

    let fitted_title = truncate_ellipsis(&title, inner_w);
    let pal = state.theme.palette();
    let list = List::new(items)
        .style(Style::default().bg(pal.modal_bg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pal.accent))
                .style(Style::default().bg(pal.modal_bg))
                .title(Span::styled(
                    fitted_title,
                    Style::default().fg(pal.complementary).add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(list, popup_area);
}


fn draw_approval_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let Some((tool, detail, reason)) = &state.approval_prompt else {
        return;
    };
    let width = ((area.width as u32 * 76) / 100) as u16;
    let width = width.clamp(50.min(area.width.saturating_sub(2)), area.width.saturating_sub(2).min(104));
    let detail_lines = detail.lines().count().max(1) as u16;
    let height = (9 + detail_lines.min(12)).min(area.height.saturating_sub(4));
    let popup = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let options = [
        "1. Allow once",
        "2. Allow for this session",
        "3. Enable YOLO (auto-approve all)",
        "4. Deny",
    ];
    let mut lines = vec![
        Line::from(vec![
            Span::styled(" Tool: ", Style::default().fg(Color::DarkGray)),
            Span::styled(tool.clone(), Style::default().fg(pal.warning).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  ({reason})"), Style::default().fg(Color::DarkGray)),
        ]),
    ];
    for (idx, dline) in detail.lines().take(12).enumerate() {
        let style = if dline.starts_with('+') {
            Style::default().fg(pal.success)
        } else if dline.starts_with('-') {
            Style::default().fg(pal.error)
        } else {
            Style::default().fg(Color::White)
        };
        let prefix = if idx == 0 { " Target: " } else { "         " };
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::DarkGray)),
            Span::styled(dline.to_string(), style),
        ]));
    }
    lines.push(Line::from(""));
    for (idx, opt) in options.iter().enumerate() {
        let sel = idx == state.modal_idx;
        let ptr = if sel { " ▸ " } else { "   " };
        let style = if sel {
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        lines.push(Line::from(Span::styled(format!("{ptr}{opt}"), style)));
    }
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(pal.modal_bg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(1))
                .border_style(Style::default().fg(pal.warning))
                .style(Style::default().bg(pal.modal_bg))
                .title(Span::styled(
                    " Permission Required (↑/↓ Enter · 1-4 · Esc deny) ",
                    Style::default().fg(pal.warning).add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(paragraph, popup);
}

fn draw_question_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let Some(session) = &state.question_prompt else {
        return;
    };
    let Some(item) = session.active() else {
        return;
    };
    let has_preview = item.options.iter().any(|o| o.preview.is_some());
    let pct = if has_preview { 88 } else { 78 };
    let width = ((area.width as u32 * pct) / 100) as u16;
    let width = width.clamp(
        52.min(area.width.saturating_sub(2)),
        area.width.saturating_sub(2).min(124),
    );
    let inner_w = width.saturating_sub(4).max(1) as usize;
    let q_lines = (item.question.chars().count() + 2).div_ceil(inner_w).max(1);
    let opt_lines: usize = item
        .options
        .iter()
        .map(|o| 1 + usize::from(!o.description.is_empty()))
        .sum();
    let tab_rows = usize::from(session.items.len() > 1) * 2;
    let height = ((q_lines + opt_lines + tab_rows + 8) as u16)
        .max(if has_preview { 16 } else { 10 })
        .min(area.height.saturating_sub(4));
    let popup = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let title = if session.items.len() > 1 {
        format!(
            " {} ({}/{}) · Tab/←/→ switch · ↑/↓ · {}Enter · Esc ",
            item.header,
            session.active_idx + 1,
            session.items.len(),
            if item.multi_select { "Space toggle · " } else { "" }
        )
    } else {
        format!(
            " {} · ↑/↓ · {}Enter · Esc ",
            item.header,
            if item.multi_select { "Space toggle · " } else { "" }
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::horizontal(1))
        .border_style(Style::default().fg(pal.accent))
        .style(Style::default().bg(pal.modal_bg))
        .title(Span::styled(
            truncate_ellipsis(&title, width.saturating_sub(2) as usize),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        ));
    let inner_area = block.inner(popup);
    frame.render_widget(block, popup);

    let mut left_lines: Vec<Line<'_>> = Vec::new();
    if session.items.len() > 1 {
        let mut tabs: Vec<Span<'_>> = Vec::new();
        for (i, q) in session.items.iter().enumerate() {
            let active = i == session.active_idx;
            let done = q.answer.is_some();
            let mark = if done { "✓ " } else { "" };
            let st = if active {
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.card_quiet_bg)
                    .add_modifier(Modifier::BOLD)
            } else if done {
                Style::default().fg(pal.success)
            } else {
                Style::default().fg(pal.muted)
            };
            tabs.push(Span::styled(format!(" [{mark}{}] ", q.header), st));
            tabs.push(Span::raw(" "));
        }
        left_lines.push(Line::from(tabs));
        left_lines.push(Line::from(""));
    }

    left_lines.push(Line::from(Span::styled(
        item.question.clone(),
        Style::default().fg(pal.fg).add_modifier(Modifier::BOLD),
    )));
    left_lines.push(Line::from(""));

    for (idx, opt) in item.options.iter().enumerate() {
        let sel = idx == state.modal_idx;
        let ptr = if sel { "▸ " } else { "  " };
        let check = if item.multi_select {
            if item.selected.get(idx).copied().unwrap_or(false) {
                "[x] "
            } else {
                "[ ] "
            }
        } else {
            ""
        };
        let l_style = if sel {
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(pal.fg)
        };
        left_lines.push(Line::from(Span::styled(
            format!("{ptr}{check}{}. {}", idx + 1, opt.label),
            l_style,
        )));
        if !opt.description.is_empty() {
            left_lines.push(Line::from(Span::styled(
                format!("     {}", opt.description),
                Style::default().fg(pal.muted),
            )));
        }
    }

    let custom_idx = item.options.len();
    let custom_sel = state.modal_idx == custom_idx;
    let custom_ptr = if custom_sel { "▸ " } else { "  " };
    let custom_label = if item.custom_text.is_empty() {
        if custom_sel {
            "Type something. ▌".to_string()
        } else {
            "Type something.".to_string()
        }
    } else if custom_sel {
        format!("{}▌", item.custom_text)
    } else {
        item.custom_text.clone()
    };
    left_lines.push(Line::from(vec![
        Span::styled(
            format!("{custom_ptr}{}. ", custom_idx + 1),
            Style::default().fg(if custom_sel { pal.accent } else { pal.muted }),
        ),
        Span::styled(
            custom_label,
            Style::default().fg(if item.custom_text.is_empty() && !custom_sel {
                pal.muted
            } else {
                pal.fg
            }),
        ),
    ]));

    // Pi rpiv-ask-user-question UX: expand to full width while typing on "Type something."
    if has_preview && !custom_sel && inner_area.width >= 72 {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
            .split(inner_area);
        frame.render_widget(
            Paragraph::new(left_lines)
                .wrap(Wrap { trim: false })
                .style(Style::default().bg(pal.modal_bg)),
            panes[0],
        );
        let preview_text = item
            .options
            .get(state.modal_idx)
            .and_then(|o| o.preview.as_deref())
            .unwrap_or("(select an option to preview)");
        let preview_lines: Vec<Line<'_>> = preview_text
            .lines()
            .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(pal.fg))))
            .collect();
        let preview_box = Paragraph::new(preview_lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(pal.card_quiet_bg))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .padding(Padding::horizontal(1))
                    .border_style(Style::default().fg(pal.muted))
                    .style(Style::default().bg(pal.card_quiet_bg))
                    .title(Span::styled(" Preview ", Style::default().fg(pal.secondary))),
            );
        frame.render_widget(preview_box, panes[1]);
    } else {
        frame.render_widget(
            Paragraph::new(left_lines)
                .wrap(Wrap { trim: false })
                .style(Style::default().bg(pal.modal_bg)),
            inner_area,
        );
    }
}

fn draw_usage_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    let target_w = ((area.width as u32 * 82) / 100) as u16;
    let max_w = area.width.saturating_sub(2).max(20);
    let width = target_w.clamp(48.min(max_w), max_w.min(116));
    let inner_w = width.saturating_sub(2).max(1) as usize;
    let wrapped_count: usize = if state.usage_lines.is_empty() {
        1
    } else {
        state
            .usage_lines
            .iter()
            .map(|l| l.chars().count().max(1).div_ceil(inner_w))
            .sum()
    };
    let content_h = (wrapped_count as u16 + 4).max(10);
    let height = content_h.min(area.height.saturating_sub(4)).max(6);
    let popup_area = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, popup_area);
    let title = if state.usage_loading {
        format!(
            " {} Usage & Quotas — refreshing... (↑/↓ scroll · r refresh · Esc close) ",
            state.spinner_glyph()
        )
    } else {
        " Usage & Quotas (↑/↓ scroll · r refresh · Esc/Enter close) ".to_string()
    };
    let title = truncate_ellipsis(&title, inner_w);

    let mut lines: Vec<Line<'_>> = Vec::new();
    if state.usage_lines.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                " {} Fetching live plan quota, budget & balances...",
                state.spinner_glyph()
            ),
            Style::default().fg(accent),
        )));
    } else {
        for raw in &state.usage_lines {
            if raw.is_empty() {
                lines.push(Line::from(""));
            } else if !raw.starts_with(' ') {
                lines.push(Line::from(Span::styled(
                    format!(" {raw}"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    raw.clone(),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
    }

    let pal = state.theme.palette();
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.usage_scroll, 0))
        .style(Style::default().bg(pal.modal_bg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .padding(Padding::new(2, 2, 1, 1))
                .border_style(Style::default().fg(accent))
                .style(Style::default().bg(pal.modal_bg))
                .title(Span::styled(
                    title,
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(paragraph, popup_area);
}

fn draw_text_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    let max_w = area.width.saturating_sub(2).max(20);
    let width = (((area.width as u32 * 82) / 100) as u16).clamp(48.min(max_w), max_w.min(116));
    let height = area.height.saturating_sub(4).clamp(8, 28);
    let popup_area = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup_area);

    let pal = state.theme.palette();
    let inner_modal_w = width.saturating_sub(3) as usize;
    let (title, lines): (String, Vec<Line<'_>>) = if state.modal == ActiveModal::InfoModal {
        (
            format!(" {} (↑/↓ scroll · Esc/Enter close) ", state.info_modal_title),
            state
                .info_modal_lines
                .iter()
                .map(|s| {
                    let is_diagram = s.contains('┌') || s.contains('└') || s.contains('│') || s.contains("─▶");
                    let display = if is_diagram {
                        truncate_ellipsis(s, inner_modal_w)
                    } else {
                        s.clone()
                    };
                    let style = if s.ends_with(':') || s.starts_with("## ") || s.starts_with("Active ") || s.starts_with("Draw.io Diagram") {
                        Style::default().fg(pal.complementary).add_modifier(Modifier::BOLD)
                    } else if s.contains("─▶") || s.contains("├─") || s.contains("└─") {
                        Style::default().fg(pal.secondary)
                    } else if s.contains('┌') || s.contains('└') || s.contains('│') {
                        Style::default().fg(pal.accent)
                    } else if s.trim_start().starts_with("• ") || s.trim_start().starts_with("- ") {
                        Style::default().fg(pal.fg).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(pal.muted)
                    };
                    Line::from(Span::styled(display, style.bg(pal.modal_bg)))
                })
                .collect(),
        )
    } else if state.modal == ActiveModal::DebugModal {
        (
            " Debugger & Event Log (f filter · c clear · ↑/↓ scroll · F12/Esc close) ".to_string(),
            state
                .debug_lines
                .iter()
                .map(|s| {
                    let style = if s.contains("[error]") {
                        Style::default().fg(pal.error)
                    } else if s.contains("[perf]") {
                        Style::default().fg(pal.warning)
                    } else if s.contains("[tool]") {
                        Style::default().fg(pal.success)
                    } else if s.contains("[worker]") || s.contains("[agent]") {
                        Style::default().fg(pal.accent)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    Line::from(Span::styled(s.clone(), style))
                })
                .collect(),
        )
    } else if state.modal == ActiveModal::HelpModal {
        let items = [
            "Keybindings:",
            "  Shift+Tab       Cycle mode (normal / plan / ask / rea / cyber)",
            "  Ctrl+P          Cycle scoped models",
            "  Ctrl+B          Toggle right-side todo pane",
            "  Ctrl+T / Ctrl+O Toggle thinking / expand tool output",
            "  Ctrl+X          Copy last reply to clipboard",
            "  Ctrl+V / Ctrl+G Paste image or text / open external editor",
            "  Ctrl+L          Clear transcript viewport (keeps context)",
            "  Shift+Enter     Insert newline (or Alt+Enter / Ctrl+J)",
            "  Esc             Interrupt active turn (or clear editor)",
            "  !cmd / !!cmd    Run shell command (in-context / local)",
            "",
            "Models & Settings:",
            "  /model  /mode  /thinking  /theme  /config  /term  /yolo  /policy",
            "",
            "Session & Copy:",
            "  /new  /clear  /resume  /rename  /rewind  /tree  /compact  /handover",
            "  /copy [chat|code]  /export  /report  /search",
            "",
            "Learning, Tools & Extensions:",
            "  /learn  /suggestion  /context  /usage  /memory  /todos",
            "  /skills  /mcp  /plugins  /reload  /trust  /auth  /login  /quit",
        ];
        (
            " Help & Hotkeys (↑/↓ scroll · Esc/Enter close) ".to_string(),
            items
                .iter()
                .map(|s| {
                    let st = if s.ends_with(':') {
                        Style::default()
                            .fg(pal.complementary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(pal.fg)
                    };
                    Line::from(Span::styled(*s, st))
                })
                .collect(),
        )
    } else {
        (
            " le-vibe Changelog (↑/↓ scroll · Esc/Enter close) ".to_string(),
            state
                .changelog_lines
                .iter()
                .map(|s| Line::from(Span::styled(s.clone(), Style::default().fg(Color::Gray))))
                .collect(),
        )
    };

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.help_scroll, 0))
        .style(Style::default().bg(pal.modal_bg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .padding(Padding::new(2, 2, 1, 1))
                .border_style(Style::default().fg(accent))
                .style(Style::default().bg(pal.modal_bg))
                .title(Span::styled(
                    truncate_ellipsis(&title, width.saturating_sub(2) as usize),
                    Style::default().fg(pal.complementary).add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(paragraph, popup_area);
}

fn draw_trust_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let width = ((area.width as f32 * 0.78) as u16)
        .max(44)
        .min(area.width.saturating_sub(2));
    let height = (11 + state.trust_prompt_resources.len().min(6) as u16)
        .min(area.height.saturating_sub(2));
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let popup_area = Rect { x, y, width, height };
    frame.render_widget(Clear, popup_area);

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            format!(" Folder: {}", state.trust_prompt_path),
            Style::default().fg(pal.fg).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " Protected project resources detected (not yet loaded):",
            Style::default().fg(pal.muted),
        )),
    ];
    for res in state.trust_prompt_resources.iter().take(6) {
        lines.push(Line::from(Span::styled(
            format!("   • {res}"),
            Style::default().fg(pal.complementary),
        )));
    }
    lines.push(Line::from(""));
    let choices = [
        ("1. Trust this folder", "Load project settings, MCP, extensions & prompts (saved to ~/.le-vibe/trust.json)"),
        ("2. Keep untrusted", "Skip project resources for this session (user-level settings still apply)"),
    ];
    for (idx, (label, desc)) in choices.iter().enumerate() {
        let selected = state.modal_idx == idx;
        let marker = if selected { " ▸ " } else { "   " };
        let style = if selected {
            Style::default()
                .fg(pal.accent)
                .bg(pal.card_quiet_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(pal.fg)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker}{label}  "), style),
            Span::styled(*desc, Style::default().fg(pal.muted)),
        ]));
    }
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(pal.modal_bg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(1))
                .border_style(Style::default().fg(pal.warning))
                .style(Style::default().bg(pal.modal_bg))
                .title(Span::styled(
                    " Project Trust Check (y/1 Trust · n/Esc Skip) ",
                    Style::default().fg(pal.warning).add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(paragraph, popup_area);
}

