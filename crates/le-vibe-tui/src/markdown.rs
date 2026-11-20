use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::ThemePalette;

fn parse_inline_spans(line: &str, base: Style, code_style: Style) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        if let Some(tick) = rest.find('`') {
            if tick > 0 {
                spans.extend(parse_bold_italic(&rest[..tick], base));
            }
            let after = &rest[tick + 1..];
            if let Some(end_tick) = after.find('`') {
                spans.push(Span::styled(after[..end_tick].to_string(), code_style));
                rest = &after[end_tick + 1..];
            } else {
                spans.push(Span::styled(rest[tick..].to_string(), base));
                break;
            }
        } else {
            spans.extend(parse_bold_italic(rest, base));
            break;
        }
    }
    if spans.is_empty() {
        spans.push(Span::raw(""));
    }
    spans
}

fn parse_bold_italic(text: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(start) = rest.find("**") {
            if start > 0 {
                spans.push(Span::styled(rest[..start].to_string(), base));
            }
            let after = &rest[start + 2..];
            if let Some(end) = after.find("**") {
                spans.push(Span::styled(
                    after[..end].to_string(),
                    base.add_modifier(Modifier::BOLD),
                ));
                rest = &after[end + 2..];
            } else {
                spans.push(Span::styled(rest[start..].to_string(), base));
                break;
            }
        } else {
            spans.push(Span::styled(rest.to_string(), base));
            break;
        }
    }
    spans
}

pub fn render_markdown_lines(text: &str, pal: &ThemePalette) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    let base = Style::default().fg(Color::White);
    let code_inline = Style::default().fg(pal.accent).bg(pal.card_quiet_bg);
    let code_block_style = Style::default().fg(Color::Gray).bg(pal.card_quiet_bg);
    let mut in_fence = false;

    for raw in text.lines() {
        let trimmed = raw.trim_start();
        if let Some(lang) = trimmed.strip_prefix("```") {
            in_fence = !in_fence;
            if in_fence {
                let label = if lang.trim().is_empty() {
                    "code".to_string()
                } else {
                    lang.trim().to_string()
                };
                out.push(Line::from(vec![
                    Span::styled(" ▎ ", Style::default().fg(pal.accent).bg(pal.card_quiet_bg)),
                    Span::styled(
                        label,
                        Style::default()
                            .fg(Color::DarkGray)
                            .bg(pal.card_quiet_bg)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
            continue;
        }

        if in_fence {
            out.push(Line::from(vec![
                Span::styled(" ▎ ", Style::default().fg(pal.accent).bg(pal.card_quiet_bg)),
                Span::styled(raw.to_string(), code_block_style),
            ]));
            continue;
        }

        if trimmed == "---" || trimmed == "***" {
            out.push(Line::from(Span::styled(
                " ────────────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        if let Some(h) = trimmed.strip_prefix("### ") {
            out.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    h.to_string(),
                    Style::default().fg(pal.secondary).add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }
        if let Some(h) = trimmed.strip_prefix("## ") {
            out.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    h.to_string(),
                    Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }
        if let Some(h) = trimmed.strip_prefix("# ") {
            out.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    h.to_string(),
                    Style::default()
                        .fg(pal.accent)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
            ]));
            continue;
        }

        if let Some(q) = trimmed.strip_prefix("> ") {
            let mut spans = vec![Span::styled(" │ ", Style::default().fg(pal.secondary))];
            spans.extend(parse_inline_spans(q, Style::default().fg(Color::Gray), code_inline));
            out.push(Line::from(spans));
            continue;
        }

        if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            let indent = raw.len().saturating_sub(trimmed.len());
            let mut spans = vec![
                Span::raw(" ".repeat(indent + 1)),
                Span::styled("• ", Style::default().fg(pal.accent)),
            ];
            spans.extend(parse_inline_spans(item, base, code_inline));
            out.push(Line::from(spans));
            continue;
        }

        let mut spans = vec![Span::raw(" ")];
        spans.extend(parse_inline_spans(raw, base, code_inline));
        out.push(Line::from(spans));
    }
    if out.is_empty() {
        out.push(Line::from(""));
    }
    out
}
