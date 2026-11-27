use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::diagram::project_xml_diagram;
use crate::theme::ThemePalette;

/// Index of `pat` that is not escaped by a preceding backslash (`\`` stays literal text).
fn find_unescaped(hay: &str, pat: &str) -> Option<usize> {
    let mut escaped = false;
    let mut search_from = 0usize;
    for (idx, ch) in hay.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if idx < search_from {
            continue;
        }
        if hay[idx..].starts_with(pat) {
            return Some(idx);
        }
        search_from = idx;
    }
    None
}

/// Drop escaping backslashes before Markdown punctuation (`\*` -> `*`).
fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(next) if "`*_\\[]()#-.!|>{}+".contains(next) => out.push(next),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn parse_inline_spans(line: &str, base: Style, code_style: Style) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        if let Some(tick) = find_unescaped(rest, "`") {
            if tick > 0 {
                spans.extend(parse_bold_italic(&rest[..tick], base));
            }
            let after = &rest[tick + 1..];
            if let Some(end_tick) = find_unescaped(after, "`") {
                spans.push(Span::styled(after[..end_tick].to_string(), code_style));
                rest = &after[end_tick + 1..];
            } else {
                spans.push(Span::styled(strip_escapes(&rest[tick..]), base));
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
        if let Some(start) = find_unescaped(rest, "**") {
            if start > 0 {
                spans.push(Span::styled(strip_escapes(&rest[..start]), base));
            }
            let after = &rest[start + 2..];
            if let Some(end) = find_unescaped(after, "**") {
                spans.push(Span::styled(
                    strip_escapes(&after[..end]),
                    base.add_modifier(Modifier::BOLD),
                ));
                rest = &after[end + 2..];
            } else {
                spans.push(Span::styled(strip_escapes(&rest[start..]), base));
                break;
            }
        } else {
            spans.push(Span::styled(strip_escapes(rest), base));
            break;
        }
    }
    spans
}

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

fn is_table_separator(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('|') || !t.contains('-') {
        return false;
    }
    t.chars()
        .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn parse_table_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

fn render_table(rows: &[Vec<String>], pal: &ThemePalette, max_width: usize) -> Vec<Line<'static>> {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return Vec::new();
    }
    let mut widths = vec![1usize; cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(Span::raw(cell).width());
        }
    }
    // Cap column widths proportionally if the table exceeds the viewport width.
    let gaps = cols.saturating_sub(1) * 2;
    let budget = max_width.saturating_sub(2 + gaps).max(cols * 3);
    let total: usize = widths.iter().sum();
    if total > budget {
        for w in &mut widths {
            *w = ((*w * budget) / total).max(3);
        }
    }

    let mut out = Vec::new();
    for (r_idx, row) in rows.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
        for (c_idx, w) in widths.iter().enumerate() {
            let cell = row.get(c_idx).map(String::as_str).unwrap_or("");
            let (fit, rest) = split_at_width(cell, *w);
            let display = if !rest.is_empty() && *w >= 2 {
                let (head, _) = split_at_width(cell, w - 1);
                format!("{head}…")
            } else {
                fit.to_string()
            };
            let pad = w.saturating_sub(Span::raw(&display).width());
            let style = if r_idx == 0 {
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(pal.fg)
            };
            spans.push(Span::styled(display, style));
            if c_idx + 1 < cols {
                spans.push(Span::raw(" ".repeat(pad + 2)));
            }
        }
        out.push(Line::from(spans));
        if r_idx == 0 {
            let mut rule_spans: Vec<Span<'static>> = vec![Span::raw(" ")];
            for (c_idx, w) in widths.iter().enumerate() {
                rule_spans.push(Span::styled("─".repeat(*w), Style::default().fg(pal.muted)));
                if c_idx + 1 < cols {
                    rule_spans.push(Span::raw("  "));
                }
            }
            out.push(Line::from(rule_spans));
        }
    }
    out
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
        let (chunk, rest) = split_at_width(remaining, inner_w);
        let pad = area_width.saturating_sub(1 + Span::raw(chunk).width());
        out.push(
            Line::from(vec![
                Span::styled(" ", bg_style),
                Span::styled(chunk.to_string(), style.bg(bg)),
                Span::styled(" ".repeat(pad), bg_style),
            ])
            .style(bg_style),
        );
        remaining = rest;
    }
}

pub fn render_markdown_lines(
    text: &str,
    pal: &ThemePalette,
    area_width: u16,
    expand_raw_xml: bool,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    // Reserve 2 columns for the scrollbar margin so padded code cards never re-wrap a trailing space.
    let width = (area_width as usize).saturating_sub(2).max(20);
    let base = Style::default().fg(pal.fg);
    let code_inline = Style::default().fg(pal.complementary).bg(pal.card_quiet_bg);
    let code_block_style = Style::default().fg(pal.fg);
    let mut in_fence = false;
    let mut fence_is_xml = false;
    let mut xml_buf: Vec<String> = Vec::new();
    let mut code_buf: Vec<String> = Vec::new();
    let mut fence_lang = String::new();
    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0usize;

    while i < raw_lines.len() {
        let raw = raw_lines[i];
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
            i += 1;
            continue;
        }

        if in_fence {
            if fence_is_xml {
                xml_buf.push(raw.to_string());
            } else {
                code_buf.push(raw.to_string());
            }
            i += 1;
            continue;
        }

        if trimmed.contains('|')
            && i + 1 < raw_lines.len()
            && is_table_separator(raw_lines[i + 1])
        {
            let mut table_rows = vec![parse_table_row(trimmed)];
            i += 2;
            while i < raw_lines.len() {
                let row_line = raw_lines[i].trim();
                if row_line.is_empty() || !row_line.contains('|') {
                    break;
                }
                table_rows.push(parse_table_row(row_line));
                i += 1;
            }
            out.extend(render_table(&table_rows, pal, width));
            continue;
        }

        if trimmed == "---" || trimmed == "***" {
            out.push(Line::from(Span::styled(
                " ────────────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
            i += 1;
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
            i += 1;
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
            i += 1;
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
            i += 1;
            continue;
        }

        if let Some(q) = trimmed.strip_prefix("> ") {
            let mut spans = vec![Span::styled(" │ ", Style::default().fg(pal.secondary))];
            spans.extend(parse_inline_spans(q, Style::default().fg(Color::Gray), code_inline));
            out.push(Line::from(spans));
            i += 1;
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
            i += 1;
            continue;
        }

        let mut spans = vec![Span::raw(" ")];
        spans.extend(parse_inline_spans(raw, base, code_inline));
        out.push(Line::from(spans));
        i += 1;
    }
    if out.is_empty() {
        out.push(Line::from(""));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn renders_gfm_table_as_clean_aligned_columns() {
        let pal = ThemeId::Pi.palette();
        let md = "| Tool | Status |\n| --- | --- |\n| kofi | ok |\n| stripe | needs key |";
        let lines = render_markdown_lines(md, &pal, 60, false);
        let text: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(text.len(), 4);
        assert_eq!(text[0], " Tool    Status");
        assert_eq!(text[1], " ──────  ─────────");
        assert_eq!(text[2], " kofi    ok");
        assert_eq!(text[3], " stripe  needs key");
    }
}
