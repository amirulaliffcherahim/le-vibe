use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::diagram::project_xml_diagram;
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

fn push_full_width_line(
    out: &mut Vec<Line<'static>>,
    area_width: usize,
    text: &str,
    style: Style,
    bg: Color,
) {
    let inner_w = area_width.saturating_sub(2).max(16);
    let bg_style = Style::default().bg(bg);
    let clean: String = text
        .chars()
        .filter(|c| *c != '\0' && *c != '\r' && !c.is_control())
        .collect();
    if clean.is_empty() {
        out.push(Line::from(Span::styled(" ".repeat(area_width), bg_style)).style(bg_style));
        return;
    }
    let mut remaining = clean.as_str();
    while !remaining.is_empty() {
        let chunk: String = remaining.chars().take(inner_w).collect();
        let consumed = chunk.len();
        let pad = area_width.saturating_sub(1 + chunk.chars().count());
        out.push(
            Line::from(vec![
                Span::styled(" ", bg_style),
                Span::styled(chunk, style.bg(bg)),
                Span::styled(" ".repeat(pad), bg_style),
            ])
            .style(bg_style),
        );
        remaining = &remaining[consumed..];
    }
}

pub fn render_markdown_lines(
    text: &str,
    pal: &ThemePalette,
    area_width: u16,
    expand_raw_xml: bool,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    let width = (area_width as usize).max(24);
    let base = Style::default().fg(pal.fg);
    let code_inline = Style::default().fg(pal.complementary).bg(pal.card_quiet_bg);
    let code_block_style = Style::default().fg(pal.fg);
    let mut in_fence = false;
    let mut fence_is_xml = false;
    let mut xml_buf: Vec<String> = Vec::new();
    let mut code_buf: Vec<String> = Vec::new();
    let mut fence_lang = String::new();

    for raw in text.lines() {
        let trimmed = raw.trim_start();
        if let Some(lang) = trimmed.strip_prefix("```") {
            in_fence = !in_fence;
            if in_fence {
                let l = lang.trim().to_lowercase();
                fence_is_xml = matches!(l.as_str(), "xml" | "drawio" | "svg" | "mxfile");
                fence_lang = if l.is_empty() { "code".to_string() } else { l };
                xml_buf.clear();
                code_buf.clear();
            } else if fence_is_xml && !xml_buf.is_empty() {
                let bg = pal.card_quiet_bg;
                let projected = project_xml_diagram(&xml_buf.join("\n"));
                out.push(Line::from(""));
                push_full_width_line(&mut out, width, "", Style::default(), bg);
                push_full_width_line(
                    &mut out,
                    width,
                    "Diagram Projection (/diagram for full view)",
                    Style::default().fg(pal.complementary).add_modifier(Modifier::BOLD),
                    bg,
                );
                for pl in projected.into_iter().take(32) {
                    let dl_style = if pl.contains("─▶") || pl.contains("├─") || pl.contains("└─") {
                        Style::default().fg(pal.secondary)
                    } else if pl.contains('┌') || pl.contains('└') || pl.contains('│') {
                        Style::default().fg(pal.accent)
                    } else {
                        Style::default().fg(pal.muted)
                    };
                    push_full_width_line(&mut out, width, &pl, dl_style, bg);
                }
                if expand_raw_xml {
                    push_full_width_line(&mut out, width, "", Style::default(), bg);
                    for xl in xml_buf.iter().take(60) {
                        push_full_width_line(&mut out, width, xl, Style::default().fg(Color::DarkGray), bg);
                    }
                } else {
                    push_full_width_line(
                        &mut out,
                        width,
                        &format!("... ({} lines raw XML, ctrl+o to expand or /diagram)", xml_buf.len()),
                        Style::default().fg(Color::DarkGray),
                        bg,
                    );
                }
                push_full_width_line(&mut out, width, "", Style::default(), bg);
                fence_is_xml = false;
                xml_buf.clear();
            } else {
                let bg = pal.card_quiet_bg;
                out.push(Line::from(""));
                push_full_width_line(&mut out, width, "", Style::default(), bg);
                push_full_width_line(
                    &mut out,
                    width,
                    &fence_lang,
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                    bg,
                );
                for cl in &code_buf {
                    push_full_width_line(&mut out, width, cl, code_block_style, bg);
                }
                push_full_width_line(&mut out, width, "", Style::default(), bg);
                code_buf.clear();
            }
            continue;
        }

        if in_fence {
            if fence_is_xml {
                xml_buf.push(raw.to_string());
            } else {
                code_buf.push(raw.to_string());
            }
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
                Span::styled("• ", Style::default().fg(pal.complementary)),
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
