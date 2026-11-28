// Chat-safe clipboard formatters: turn raw Markdown (the data model, not screen cells) into
// plain text for WhatsApp/Teams, optional HTML for Dual-MIME paste, or the last code block.

use ratatui::text::Span;

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn is_table_sep(line: &str) -> bool {
    let t = line.trim();
    t.contains('|')
        && t.contains('-')
        && t.chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn parse_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

fn neutralize_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            continue;
        }
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next();
            continue;
        }
        if c == '_' && chars.peek() == Some(&'_') {
            chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

fn flatten_table_plain(rows: &[Vec<String>]) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }
    let headers = &rows[0];
    let data = &rows[1..];
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return Vec::new();
    }

    // 1. Two-column key-value table -> simple `Key: Value` list (drop headers).
    if cols == 2 && !data.is_empty() {
        return data
            .iter()
            .filter_map(|row| {
                let k = neutralize_inline(row.first().map(String::as_str).unwrap_or(""));
                let v = neutralize_inline(row.get(1).map(String::as_str).unwrap_or(""));
                if k.is_empty() && v.is_empty() {
                    None
                } else if v.is_empty() {
                    Some(k)
                } else {
                    Some(format!("{k}: {v}"))
                }
            })
            .collect();
    }

    // 2. Narrow 3+ column numeric/compact table: keep monospace grid if total row width <= 34 chars.
    let mut widths = vec![1usize; cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(Span::raw(neutralize_inline(cell)).width());
        }
    }
    let grid_width: usize = widths.iter().sum::<usize>() + cols.saturating_sub(1) * 2;
    if grid_width <= 34 && cols >= 3 {
        let mut out = vec!["```".to_string()];
        for row in rows {
            let mut line = String::new();
            for (i, w) in widths.iter().enumerate() {
                let cell = neutralize_inline(row.get(i).map(String::as_str).unwrap_or(""));
                let pad = w.saturating_sub(Span::raw(&cell).width());
                line.push_str(&cell);
                if i + 1 < cols {
                    line.push_str(&" ".repeat(pad + 2));
                }
            }
            out.push(line.trim_end().to_string());
        }
        out.push("```".to_string());
        return out;
    }

    // 3. Multi-column (3+ cols) -> mobile/chat-safe card blocks.
    let mut out = Vec::new();
    for (r_idx, row) in data.iter().enumerate() {
        if r_idx > 0 {
            out.push(String::new());
        }
        let title = neutralize_inline(row.first().map(String::as_str).unwrap_or("Item"));
        out.push(title);
        for c_idx in 1..cols {
            let header = neutralize_inline(headers.get(c_idx).map(String::as_str).unwrap_or("Field"));
            let val = neutralize_inline(row.get(c_idx).map(String::as_str).unwrap_or(""));
            if !val.is_empty() {
                out.push(format!("- {header}: {val}"));
            }
        }
    }
    out
}

/// Convert raw Markdown into chat-safe plain text (for WhatsApp, Teams, Slack).
pub fn format_chat_plain(markdown: &str) -> String {
    let clean = strip_ansi(markdown);
    let lines: Vec<&str> = clean.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut para_buf: Vec<String> = Vec::new();
    let mut in_fence = false;
    let mut i = 0usize;

    let flush_para = |buf: &mut Vec<String>, out: &mut Vec<String>| {
        if !buf.is_empty() {
            out.push(buf.join(" "));
            buf.clear();
        }
    };

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();

        if trimmed.starts_with("```") {
            flush_para(&mut para_buf, &mut out);
            in_fence = !in_fence;
            out.push("```".to_string());
            i += 1;
            continue;
        }
        if in_fence {
            out.push(raw.trim_end().to_string());
            i += 1;
            continue;
        }

        if trimmed.is_empty() {
            flush_para(&mut para_buf, &mut out);
            if out.last().is_some_and(|l| !l.is_empty()) {
                out.push(String::new());
            }
            i += 1;
            continue;
        }

        if trimmed.contains('|') && i + 1 < lines.len() && is_table_sep(lines[i + 1]) {
            flush_para(&mut para_buf, &mut out);
            let mut rows = vec![parse_row(trimmed)];
            i += 2;
            while i < lines.len() {
                let r = lines[i].trim();
                if r.is_empty() || !r.contains('|') {
                    break;
                }
                rows.push(parse_row(r));
                i += 1;
            }
            out.extend(flatten_table_plain(&rows));
            continue;
        }

        if let Some(h) = trimmed
            .strip_prefix("### ")
            .or_else(|| trimmed.strip_prefix("## "))
            .or_else(|| trimmed.strip_prefix("# "))
        {
            flush_para(&mut para_buf, &mut out);
            let heading = neutralize_inline(h.trim()).trim_end_matches(':').to_string();
            out.push(format!("{heading}:"));
            i += 1;
            continue;
        }

        if let Some(item) = trimmed.strip_prefix("* ").or_else(|| trimmed.strip_prefix("- ")) {
            flush_para(&mut para_buf, &mut out);
            out.push(format!("- {}", neutralize_inline(item.trim())));
            i += 1;
            continue;
        }

        para_buf.push(neutralize_inline(trimmed));
        i += 1;
    }
    flush_para(&mut para_buf, &mut out);
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out.join("\n")
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Convert Markdown containing tables into clean HTML for Dual-MIME clipboard paste (Teams/Notion).
pub fn format_chat_html(markdown: &str) -> Option<String> {
    let clean = strip_ansi(markdown);
    let lines: Vec<&str> = clean.lines().collect();
    let mut has_table = false;
    let mut html = String::from("<div>");
    let mut in_fence = false;
    let mut i = 0usize;

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            html.push_str(if in_fence { "<pre><code>" } else { "</code></pre>" });
            i += 1;
            continue;
        }
        if in_fence {
            html.push_str(&escape_html(raw));
            html.push('\n');
            i += 1;
            continue;
        }
        if trimmed.contains('|') && i + 1 < lines.len() && is_table_sep(lines[i + 1]) {
            has_table = true;
            let header = parse_row(trimmed);
            html.push_str("<table border=\"1\" cellspacing=\"0\" cellpadding=\"4\"><thead><tr>");
            for cell in header {
                html.push_str(&format!("<th>{}</th>", escape_html(&neutralize_inline(&cell))));
            }
            html.push_str("</tr></thead><tbody>");
            i += 2;
            while i < lines.len() {
                let r = lines[i].trim();
                if r.is_empty() || !r.contains('|') {
                    break;
                }
                html.push_str("<tr>");
                for cell in parse_row(r) {
                    html.push_str(&format!("<td>{}</td>", escape_html(&neutralize_inline(&cell))));
                }
                html.push_str("</tr>");
                i += 1;
            }
            html.push_str("</tbody></table>");
            continue;
        }
        if let Some(h) = trimmed
            .strip_prefix("### ")
            .or_else(|| trimmed.strip_prefix("## "))
            .or_else(|| trimmed.strip_prefix("# "))
        {
            html.push_str(&format!("<p><strong>{}</strong></p>", escape_html(&neutralize_inline(h))));
        } else if let Some(item) = trimmed.strip_prefix("* ").or_else(|| trimmed.strip_prefix("- ")) {
            html.push_str(&format!("<p>• {}</p>", escape_html(&neutralize_inline(item))));
        } else if !trimmed.is_empty() {
            html.push_str(&format!("<p>{}</p>", escape_html(&neutralize_inline(trimmed))));
        }
        i += 1;
    }
    html.push_str("</div>");
    has_table.then_some(html)
}

/// Extract the last fenced code block from Markdown without the ``` fences.
pub fn extract_last_code_block(markdown: &str) -> Option<String> {
    let mut in_fence = false;
    let mut current: Vec<&str> = Vec::new();
    let mut last: Option<String> = None;

    for raw in markdown.lines() {
        if raw.trim_start().starts_with("```") {
            if in_fence {
                last = Some(current.join("\n"));
                current.clear();
                in_fence = false;
            } else {
                in_fence = true;
                current.clear();
            }
            continue;
        }
        if in_fence {
            current.push(raw);
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_two_col_and_multi_col_tables_for_chat() {
        let two_col = "## Stack\n\n| Key | Value |\n| --- | --- |\n| Runtime | Node.js 20 |\n| DB | `Postgres` |";
        assert_eq!(
            format_chat_plain(two_col),
            "Stack:\n\nRuntime: Node.js 20\nDB: Postgres"
        );

        let multi = "| Service | Role | Status |\n| --- | --- | --- |\n| idx-fn | Backend Function | Deployed |\n| idx-ext | Browser Extension | Ready |";
        assert_eq!(
            format_chat_plain(multi),
            "idx-fn\n- Role: Backend Function\n- Status: Deployed\n\nidx-ext\n- Role: Browser Extension\n- Status: Ready"
        );
    }

    #[test]
    fn extracts_last_code_block_without_fences() {
        let md = "First:\n```sh\necho 1\n```\nSecond:\n```python\nprint('ok')\n```";
        assert_eq!(extract_last_code_block(md).as_deref(), Some("print('ok')"));
    }
}
