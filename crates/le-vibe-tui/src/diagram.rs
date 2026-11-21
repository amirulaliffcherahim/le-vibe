//! Pure-Rust XML / Draw.io (`mxfile`) / SVG diagram projector for the le-vibe TUI.
//! Converts draw.io `<mxCell>` graphs into Unicode box-and-arrow diagrams and generic
//! XML/SVG documents into clean structured box/tree projections.

use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone)]
struct GraphNode {
    id: String,
    label: String,
}

#[derive(Debug, Clone)]
struct GraphEdge {
    source: String,
    target: String,
    label: String,
}

fn unescape_xml(s: &str) -> String {
    let mut out = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#xa;", " ")
        .replace("&#10;", " ");
    // Strip simple HTML tags often embedded inside draw.io `value="..."` (e.g., <b>, <br>, <div>)
    let mut plain = String::with_capacity(out.len());
    let mut in_tag = false;
    for ch in out.chars() {
        match ch {
            '<' => {
                in_tag = true;
                plain.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => plain.push(ch),
            _ => {}
        }
    }
    out = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    out
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let double = format!("{name}=\"");
    if let Some(pos) = tag.find(&double) {
        let rest = &tag[pos + double.len()..];
        if let Some(end) = rest.find('"') {
            return Some(unescape_xml(&rest[..end]));
        }
    }
    let single = format!("{name}='");
    if let Some(pos) = tag.find(&single) {
        let rest = &tag[pos + single.len()..];
        if let Some(end) = rest.find('\'') {
            return Some(unescape_xml(&rest[..end]));
        }
    }
    None
}

fn render_box(label: &str, max_inner: usize) -> [String; 3] {
    let trimmed: String = if label.chars().count() > max_inner {
        let mut s: String = label.chars().take(max_inner.saturating_sub(1)).collect();
        s.push('…');
        s
    } else {
        label.to_string()
    };
    let w = trimmed.chars().count().max(4);
    let pad = w - trimmed.chars().count();
    let left_pad = pad / 2;
    let right_pad = pad - left_pad;
    let top = format!("┌{}┐", "─".repeat(w + 2));
    let mid = format!(
        "│ {}{}{} │",
        " ".repeat(left_pad),
        trimmed,
        " ".repeat(right_pad)
    );
    let bot = format!("└{}┘", "─".repeat(w + 2));
    [top, mid, bot]
}

fn project_drawio_mxgraph(xml: &str) -> Option<Vec<String>> {
    let mut nodes: BTreeMap<String, GraphNode> = BTreeMap::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    let mut rest = xml;
    while let Some(start) = rest.find("<mxCell") {
        let after = &rest[start..];
        let Some(end) = after.find('>') else {
            break;
        };
        let tag = &after[..=end];
        rest = &after[end + 1..];

        let id = extract_attr(tag, "id").unwrap_or_default();
        let value = extract_attr(tag, "value").unwrap_or_default();
        let is_vertex = extract_attr(tag, "vertex").as_deref() == Some("1");
        let is_edge = extract_attr(tag, "edge").as_deref() == Some("1");

        if is_edge {
            if let (Some(src), Some(tgt)) = (extract_attr(tag, "source"), extract_attr(tag, "target"))
            {
                edges.push(GraphEdge {
                    source: src,
                    target: tgt,
                    label: value,
                });
            }
        } else if is_vertex && !id.is_empty() {
            let label = if value.is_empty() { id.clone() } else { value };
            nodes.insert(id.clone(), GraphNode { id, label });
        }
    }

    if nodes.is_empty() && edges.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "Draw.io Diagram ({} nodes, {} edges)",
        nodes.len(),
        edges.len()
    ));
    lines.push(String::new());

    // Group edges by source so fan-outs render as a clean connected tree instead of repeating parents.
    let mut order: Vec<String> = Vec::new();
    let mut by_source: BTreeMap<String, Vec<&GraphEdge>> = BTreeMap::new();
    for edge in &edges {
        if !by_source.contains_key(&edge.source) {
            order.push(edge.source.clone());
        }
        by_source.entry(edge.source.clone()).or_default().push(edge);
    }

    let mut shown_nodes: HashSet<String> = HashSet::new();
    for src_id in order {
        let Some(out_edges) = by_source.get(&src_id) else {
            continue;
        };
        let src_label = nodes
            .get(&src_id)
            .map(|n| n.label.as_str())
            .unwrap_or(&src_id);
        shown_nodes.insert(src_id.clone());

        if out_edges.len() == 1 {
            let edge = out_edges[0];
            let tgt_label = nodes
                .get(&edge.target)
                .map(|n| n.label.as_str())
                .unwrap_or(&edge.target);
            shown_nodes.insert(edge.target.clone());
            let b1 = render_box(src_label, 34);
            let b2 = render_box(tgt_label, 34);
            let arrow = if edge.label.is_empty() {
                " ───▶ ".to_string()
            } else {
                let short: String = edge.label.chars().take(22).collect();
                format!(" ─[{short}]─▶ ")
            };
            let spacer = " ".repeat(arrow.chars().count());
            lines.push(format!("{}{spacer}{}", b1[0], b2[0]));
            lines.push(format!("{}{arrow}{}", b1[1], b2[1]));
            lines.push(format!("{}{spacer}{}", b1[2], b2[2]));
        } else {
            // Fan-out: render parent box once, then branch targets cleanly below it
            let b1 = render_box(src_label, 36);
            lines.push(b1[0].clone());
            lines.push(b1[1].clone());
            lines.push(b1[2].clone());
            for (i, edge) in out_edges.iter().enumerate() {
                let tgt_label = nodes
                    .get(&edge.target)
                    .map(|n| n.label.as_str())
                    .unwrap_or(&edge.target);
                shown_nodes.insert(edge.target.clone());
                let branch = if i + 1 == out_edges.len() { "  └─" } else { "  ├─" };
                let label_part = if edge.label.is_empty() {
                    "▶ ".to_string()
                } else {
                    let short: String = edge.label.chars().take(22).collect();
                    format!("[{short}]─▶ ")
                };
                lines.push(format!("{branch}{label_part}[ {tgt_label} ]"));
            }
            lines.push(String::new());
        }
    }

    let orphans: Vec<&GraphNode> = nodes
        .values()
        .filter(|n| !shown_nodes.contains(&n.id))
        .collect();
    if !orphans.is_empty() {
        if !edges.is_empty() {
            lines.push(String::new());
        }
        for chunk in orphans.chunks(3) {
            let boxes: Vec<[String; 3]> = chunk.iter().map(|n| render_box(&n.label, 22)).collect();
            for row in 0..3 {
                let joined = boxes
                    .iter()
                    .map(|b| b[row].as_str())
                    .collect::<Vec<_>>()
                    .join("  ");
                lines.push(joined);
            }
        }
    }

    Some(lines)
}

fn project_xml_tree(xml: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut depth: usize = 0;
    let mut chars = xml.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '<' {
            continue;
        }
        let mut tag = String::new();
        for c in chars.by_ref() {
            if c == '>' {
                break;
            }
            tag.push(c);
        }
        let trimmed = tag.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('?')
            || trimmed.starts_with('!')
        {
            continue;
        }
        if trimmed.starts_with('/') {
            depth = depth.saturating_sub(1);
            continue;
        }
        let self_closing = trimmed.ends_with('/');
        let clean = trimmed.trim_end_matches('/').trim();
        let tag_name = clean
            .split_whitespace()
            .next()
            .unwrap_or("node");

        // Grab key identifying attributes for concise projection
        let mut attrs = Vec::new();
        for key in ["id", "name", "title", "label", "type", "value", "href", "src"] {
            if let Some(val) = extract_attr(clean, key)
                && !val.is_empty()
            {
                let short: String = val.chars().take(32).collect();
                attrs.push(format!("{key}={short}"));
            }
        }

        // Capture immediate text content before next `<`
        let mut text_buf = String::new();
        while let Some(&next_c) = chars.peek() {
            if next_c == '<' {
                break;
            }
            if let Some(c) = chars.next() {
                text_buf.push(c);
            }
        }
        let text = unescape_xml(&text_buf);

        let indent = if depth == 0 {
            String::new()
        } else {
            format!("{}├─ ", "│  ".repeat(depth.saturating_sub(1)))
        };
        let attr_str = if attrs.is_empty() {
            String::new()
        } else {
            format!(" [{}]", attrs.join(", "))
        };
        let text_str = if text.is_empty() {
            String::new()
        } else {
            let short: String = text.chars().take(56).collect();
            format!(": {short}")
        };

        lines.push(format!("{indent}<{tag_name}>{attr_str}{text_str}"));
        if lines.len() >= 120 {
            lines.push("... [diagram projection truncated at 120 nodes]".to_string());
            break;
        }
        if !self_closing {
            depth = (depth + 1).min(12);
        }
    }

    if lines.is_empty() {
        vec!["(No XML elements found to project.)".to_string()]
    } else {
        lines
    }
}

/// Projects XML (`mxfile`/draw.io, SVG, or structured XML) into Unicode TUI lines.
pub fn project_xml_diagram(xml: &str) -> Vec<String> {
    if (xml.contains("<mxCell") || xml.contains("<mxGraphModel") || xml.contains("<mxfile"))
        && let Some(drawio) = project_drawio_mxgraph(xml)
    {
        return drawio;
    }
    project_xml_tree(xml)
}

/// Extracts the last fenced `xml` / `drawio` / `svg` block (or raw `<mxfile>...</mxfile>`) from text.
pub fn extract_latest_xml_block(text: &str) -> Option<String> {
    let mut found: Option<String> = None;
    let mut in_xml = false;
    let mut buf = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(lang) = trimmed.strip_prefix("```") {
            let l = lang.trim().to_lowercase();
            if !in_xml && matches!(l.as_str(), "xml" | "drawio" | "svg" | "mxfile") {
                in_xml = true;
                buf.clear();
            } else if in_xml {
                in_xml = false;
                if !buf.is_empty() {
                    found = Some(buf.join("\n"));
                }
            }
            continue;
        }
        if in_xml {
            buf.push(line);
        }
    }
    if found.is_some() {
        return found;
    }
    if let Some(start) = text.find("<mxfile") {
        let end = text[start..]
            .find("</mxfile>")
            .map(|e| start + e + "</mxfile>".len())
            .unwrap_or(text.len());
        return Some(text[start..end].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_drawio_mxfile_into_boxes_and_arrows() {
        let xml = r#"
<mxfile>
  <diagram name="Arch">
    <mxGraphModel>
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <mxCell id="a" value="Rust TUI" vertex="1" parent="1"/>
        <mxCell id="b" value="Python Bridge" vertex="1" parent="1"/>
        <mxCell id="e1" value="NDJSON" edge="1" source="a" target="b" parent="1"/>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
"#;
        let out = project_xml_diagram(xml);
        let joined = out.join("\n");
        assert!(joined.contains("Draw.io Diagram (2 nodes, 1 edges)"));
        assert!(joined.contains("Rust TUI"));
        assert!(joined.contains("Python Bridge"));
        assert!(joined.contains("─[NDJSON]─▶"));
    }

    #[test]
    fn projects_generic_xml_as_structured_tree() {
        let xml = r#"<system name="le-vibe"><layer id="tui"><widget label="Editor">input</widget></layer></system>"#;
        let out = project_xml_diagram(xml);
        let joined = out.join("\n");
        assert!(joined.contains("<system> [name=le-vibe]"));
        assert!(joined.contains("├─ <layer> [id=tui]"));
        assert!(joined.contains("<widget> [label=Editor]: input"));
    }
}
