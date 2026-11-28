//! Output truncation, ported from pi's `core/tools/truncate.ts`.
//!
//! Two independent limits apply and the **first one hit wins**: 2000 lines or 50 KB. The result says
//! *how* it was truncated, so callers can print an actionable continuation notice (`Use offset=N`)
//! instead of a dead `...[truncated]` marker.
//!
//! Head truncation never returns a partial line: a half-written line is worse than one line less,
//! because the model cannot tell it was cut. Tail truncation may clip the final line (shell output
//! uses it and the newest bytes matter more than line integrity there).

/// Default line cap.
pub const DEFAULT_MAX_LINES: usize = 2000;
/// Default byte cap.
pub const DEFAULT_MAX_BYTES: usize = 50 * 1024;

/// Which limit stopped the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncatedBy {
    Lines,
    Bytes,
}

/// Everything a caller needs to build a continuation notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationResult {
    pub content: String,
    pub truncated: bool,
    pub truncated_by: Option<TruncatedBy>,
    /// Lines in the original content.
    pub total_lines: usize,
    /// Complete lines in `content` (0 when the first line alone exceeded the byte cap).
    pub output_lines: usize,
    /// The first line alone was larger than the byte cap, so nothing was returned.
    pub first_line_exceeds_limit: bool,
    /// Tail truncation clipped the final line.
    pub last_line_partial: bool,
}

/// Human-readable byte size (`512B`, `2.0KB`, `3.0MB`).
pub fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Split into lines the way pi counts them: a trailing newline does not add an empty line.
fn split_lines(content: &str) -> Vec<&str> {
    if content.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = content.split('\n').collect();
    if content.ends_with('\n') {
        lines.pop();
    }
    lines
}

/// The number of bytes a line occupies including its newline separator.
fn line_cost(line: &str, index: usize) -> usize {
    line.len() + usize::from(index > 0)
}

fn untouched(content: &str, lines: &[&str]) -> TruncationResult {
    TruncationResult {
        content: content.to_string(),
        truncated: false,
        truncated_by: None,
        total_lines: lines.len(),
        output_lines: lines.len(),
        first_line_exceeds_limit: false,
        last_line_partial: false,
    }
}

/// Keep the **first** `max_lines` / `max_bytes` of `content` (file reads).
pub fn truncate_head(content: &str, max_lines: usize, max_bytes: usize) -> TruncationResult {
    let lines = split_lines(content);
    let total_bytes = content.len();
    if lines.len() <= max_lines && total_bytes <= max_bytes {
        return untouched(content, &lines);
    }

    if lines.first().is_some_and(|first| first.len() > max_bytes) {
        return TruncationResult {
            content: String::new(),
            truncated: true,
            truncated_by: Some(TruncatedBy::Bytes),
            total_lines: lines.len(),
            output_lines: 0,
            first_line_exceeds_limit: true,
            last_line_partial: false,
        };
    }

    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0usize;
    let mut truncated_by = TruncatedBy::Lines;
    for (idx, line) in lines.iter().enumerate() {
        if idx >= max_lines {
            break;
        }
        let cost = line_cost(line, idx);
        if used.saturating_add(cost) > max_bytes {
            truncated_by = TruncatedBy::Bytes;
            break;
        }
        kept.push(line);
        used += cost;
    }
    if kept.len() >= max_lines && used <= max_bytes {
        truncated_by = TruncatedBy::Lines;
    }

    TruncationResult {
        content: kept.join("\n"),
        truncated: true,
        truncated_by: Some(truncated_by),
        total_lines: lines.len(),
        output_lines: kept.len(),
        first_line_exceeds_limit: false,
        last_line_partial: false,
    }
}

/// Keep the **last** `max_lines` / `max_bytes` of `content` (shell output).
pub fn truncate_tail(content: &str, max_lines: usize, max_bytes: usize) -> TruncationResult {
    let lines = split_lines(content);
    let total_bytes = content.len();
    if lines.len() <= max_lines && total_bytes <= max_bytes {
        return untouched(content, &lines);
    }

    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0usize;
    let mut truncated_by = TruncatedBy::Lines;
    let mut last_line_partial = false;
    for line in lines.iter().rev() {
        if kept.len() >= max_lines {
            break;
        }
        let cost = line.len() + 1;
        if used.saturating_add(cost) > max_bytes {
            // The newest line alone does not fit: keep its tail so the final error message survives.
            if kept.is_empty() && line.len() > max_bytes {
                let start = line.len() - max_bytes.min(line.len());
                let mut idx = start;
                while idx < line.len() && !line.is_char_boundary(idx) {
                    idx += 1;
                }
                if let Some(fragment) = line.get(idx..) {
                    kept.push(fragment);
                    last_line_partial = true;
                }
            }
            truncated_by = TruncatedBy::Bytes;
            break;
        }
        kept.push(line);
        used += cost;
    }
    if kept.len() >= max_lines && used <= max_bytes {
        truncated_by = TruncatedBy::Lines;
    }
    kept.reverse();

    TruncationResult {
        content: kept.join("\n"),
        truncated: true,
        truncated_by: Some(truncated_by),
        total_lines: lines.len(),
        output_lines: kept.len(),
        first_line_exceeds_limit: false,
        last_line_partial,
    }
}

/// `/tmp`-style spill file holding the untruncated output.
pub fn spill_path() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("lazybug-output-{}-{nanos}.log", std::process::id()))
}

/// Write `full` to a spill file, returning its path when the write succeeds.
pub fn spill_full_output(full: &str) -> Option<std::path::PathBuf> {
    let path = spill_path();
    std::fs::write(&path, full).ok().map(|_| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_cost_counts_the_separator_only_after_the_first_line() {
        assert_eq!(line_cost("abc", 0), 3);
        assert_eq!(line_cost("abc", 1), 4);
    }

    #[test]
    fn spill_file_round_trips() {
        let path = spill_full_output("full body").expect("spill");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "full body");
        let _ = std::fs::remove_file(path);
    }
}
