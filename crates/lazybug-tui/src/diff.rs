//! Self-contained word-level diff for `edit` / `write` tool cards.
//!
//! Mirrors pi's `renderIntraLineDiff`: common indentation is kept out of the comparison so a
//! leading insertion cannot swallow it, and only the words that actually changed are marked, so
//! the renderer can invert exactly those spans instead of reprinting whole lines.
//!
//! No external diff crate: the tokenizer plus an LCS table is enough for one line pair, and the
//! table size is capped so a pathological edit can never stall a frame.

/// How one run of text relates to the other side of the diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// Identical on both sides.
    Equal,
    /// Present only in the old text.
    Removed,
    /// Present only in the new text.
    Added,
}

/// A contiguous run of text with its diff classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSegment {
    pub kind: DiffKind,
    pub text: String,
}

/// Token budget above which the diff degrades to a whole-line replace.
const MAX_TOKENS: usize = 400;

fn common_indent_len(old: &str, new: &str) -> usize {
    let mut len = 0;
    for (a, b) in old.chars().zip(new.chars()) {
        if a != b || !a.is_whitespace() {
            break;
        }
        len += a.len_utf8();
    }
    len
}

/// Split into words, whitespace runs and single punctuation characters.
///
/// A compact `name(...)` group with no internal whitespace stays one token so `len()` → `iter()`
/// reads as one changed word instead of three.
fn tokenize(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte.is_ascii_whitespace() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(&text[start..i]);
        } else if byte.is_ascii_alphanumeric() || byte == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'(' {
                let mut j = i + 1;
                let mut compact = true;
                while j < bytes.len() && bytes[j] != b')' {
                    if bytes[j].is_ascii_whitespace() || j - i > 24 {
                        compact = false;
                        break;
                    }
                    j += 1;
                }
                if compact && j < bytes.len() {
                    i = j + 1;
                }
            }
            out.push(&text[start..i]);
        } else {
            let ch = text[i..].chars().next().unwrap_or(' ');
            let width = ch.len_utf8();
            out.push(&text[i..i + width]);
            i += width;
        }
    }
    out
}

fn push_segment(out: &mut Vec<DiffSegment>, kind: DiffKind, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut()
        && last.kind == kind
    {
        last.text.push_str(text);
        return;
    }
    out.push(DiffSegment {
        kind,
        text: text.to_string(),
    });
}

/// Word-level diff between one old line and its replacement.
///
/// Returns merged runs in reading order: `Equal` text present on both sides, `Removed` runs only
/// in `old`, and `Added` runs only in `new`.
pub fn word_diff(old: &str, new: &str) -> Vec<DiffSegment> {
    let mut out: Vec<DiffSegment> = Vec::new();
    let indent_bytes = common_indent_len(old, new);
    let (indent, old_rest) = old.split_at(indent_bytes);
    let new_rest = &new[indent_bytes..];
    push_segment(&mut out, DiffKind::Equal, indent);

    let old_tokens = tokenize(old_rest);
    let new_tokens = tokenize(new_rest);

    if old_tokens.len() > MAX_TOKENS || new_tokens.len() > MAX_TOKENS {
        push_segment(&mut out, DiffKind::Removed, old_rest);
        push_segment(&mut out, DiffKind::Added, new_rest);
        return out;
    }

    // Suffix LCS table: `table[i][j]` = LCS length of old_tokens[i..] and new_tokens[j..].
    let rows = old_tokens.len() + 1;
    let cols = new_tokens.len() + 1;
    let mut table = vec![0u32; rows * cols];
    for i in (0..old_tokens.len()).rev() {
        for j in (0..new_tokens.len()).rev() {
            table[i * cols + j] = if old_tokens[i] == new_tokens[j] {
                table[(i + 1) * cols + j + 1] + 1
            } else {
                table[(i + 1) * cols + j].max(table[i * cols + j + 1])
            };
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    while i < old_tokens.len() && j < new_tokens.len() {
        if old_tokens[i] == new_tokens[j] {
            push_segment(&mut out, DiffKind::Equal, old_tokens[i]);
            i += 1;
            j += 1;
        } else if table[(i + 1) * cols + j] >= table[i * cols + j + 1] {
            push_segment(&mut out, DiffKind::Removed, old_tokens[i]);
            i += 1;
        } else {
            push_segment(&mut out, DiffKind::Added, new_tokens[j]);
            j += 1;
        }
    }
    while i < old_tokens.len() {
        push_segment(&mut out, DiffKind::Removed, old_tokens[i]);
        i += 1;
    }
    while j < new_tokens.len() {
        push_segment(&mut out, DiffKind::Added, new_tokens[j]);
        j += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(segments: &[DiffSegment], kind: DiffKind) -> String {
        segments
            .iter()
            .filter(|s| s.kind == kind)
            .map(|s| s.text.as_str())
            .collect()
    }

    #[test]
    fn isolates_the_changed_words() {
        let segs = word_diff("let count = items.len();", "let count = items.iter().count();");
        assert_eq!(joined(&segs, DiffKind::Removed).trim(), "len()");
        assert_eq!(joined(&segs, DiffKind::Added).trim(), "iter().count()");
        assert_eq!(joined(&segs, DiffKind::Equal), "let count = items.;");
    }

    #[test]
    fn keeps_common_indentation_out_of_the_diff() {
        let segs = word_diff("    let a = 1;", "    let a = 2;");
        // Adjacent equal runs merge, so the indent leads the first segment.
        assert_eq!(segs[0].kind, DiffKind::Equal);
        assert!(
            segs[0].text.starts_with("    let a = "),
            "common indentation must stay out of the diff: {:?}",
            segs[0]
        );
        assert_eq!(joined(&segs, DiffKind::Removed).trim(), "1");
        assert_eq!(joined(&segs, DiffKind::Added).trim(), "2");
    }

    #[test]
    fn identical_lines_are_a_single_equal_run() {
        let segs = word_diff("same line", "same line");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].kind, DiffKind::Equal);
    }

    #[test]
    fn non_ascii_text_does_not_panic_or_split_mid_char() {
        let segs = word_diff("let s = \"日本語\";", "let s = \"日本語です\";");
        assert!(!joined(&segs, DiffKind::Added).is_empty());
        // Reassembling both sides must reproduce the inputs exactly.
        let removed: String = segs
            .iter()
            .filter(|s| s.kind != DiffKind::Added)
            .map(|s| s.text.as_str())
            .collect();
        let added: String = segs
            .iter()
            .filter(|s| s.kind != DiffKind::Removed)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(removed, "let s = \"日本語\";");
        assert_eq!(added, "let s = \"日本語です\";");
    }
}
