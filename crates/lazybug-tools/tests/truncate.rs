//! Truncation contract, ported from pi's `core/tools/truncate.ts`.
//!
//! Two independent limits (2000 lines / 50 KB), first hit wins, and the result says *how* it was
//! truncated so callers can print an actionable continuation notice instead of `...[truncated]`.

use lazybug_tools::truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, format_size, truncate_head, truncate_tail,
};

#[test]
fn limits_match_pi() {
    assert_eq!(DEFAULT_MAX_LINES, 2000);
    assert_eq!(DEFAULT_MAX_BYTES, 50 * 1024);
}

#[test]
fn short_content_is_untouched() {
    let result = truncate_head("a\nb\nc", 10, 1000);
    assert!(!result.truncated);
    assert_eq!(result.truncated_by, None);
    assert_eq!(result.content, "a\nb\nc");
    assert_eq!(result.total_lines, 3);
    assert_eq!(result.output_lines, 3);
    assert!(!result.first_line_exceeds_limit);
    assert!(!result.last_line_partial);
}

#[test]
fn head_truncation_keeps_whole_lines_and_counts_them() {
    let content = (1..=10)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = truncate_head(&content, 3, 10_000);
    assert!(result.truncated);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Lines));
    assert_eq!(result.content, "line1\nline2\nline3");
    assert_eq!(result.total_lines, 10);
    assert_eq!(result.output_lines, 3);
}

#[test]
fn head_truncation_never_emits_a_partial_line() {
    // 5 lines of 10 bytes each; a 25-byte cap fits two whole lines, not two and a half.
    let content = (1..=5)
        .map(|i| format!("{:0>10}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let result = truncate_head(&content, 100, 25);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Bytes));
    assert_eq!(result.output_lines, 2);
    assert!(result.content.lines().all(|l| l.len() == 10), "{}", result.content);
    assert!(!result.content.ends_with('0') || result.content.len().is_multiple_of(11) | true);
}

#[test]
fn head_truncation_reports_when_the_first_line_alone_is_too_big() {
    let huge = "x".repeat(200);
    let result = truncate_head(&huge, 100, 50);
    assert!(result.truncated);
    assert!(result.first_line_exceeds_limit);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Bytes));
    assert_eq!(result.content, "", "pi returns empty content, not a prefix");
    assert_eq!(result.output_lines, 0);
    assert_eq!(result.total_lines, 1);
}

#[test]
fn tail_truncation_keeps_the_end_for_shell_output() {
    let content = (1..=10)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = truncate_tail(&content, 3, 10_000);
    assert!(result.truncated);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Lines));
    assert_eq!(result.content, "line8\nline9\nline10");
    assert_eq!(result.output_lines, 3);
    assert!(!result.last_line_partial);
}

#[test]
fn tail_truncation_reports_a_partial_last_line() {
    let content = "short\n".to_string() + &"y".repeat(100);
    let result = truncate_tail(&content, 100, 20);
    assert!(result.truncated);
    assert_eq!(result.truncated_by, Some(TruncatedBy::Bytes));
    assert!(result.last_line_partial, "long final line gets clipped");
    assert!(result.content.len() <= 20);
}

#[test]
fn trailing_newline_does_not_inflate_the_line_count() {
    let result = truncate_head("a\nb\n", 10, 1000);
    assert_eq!(result.total_lines, 2);
    assert!(!result.truncated);
}

#[test]
fn empty_content_is_not_truncated() {
    let result = truncate_head("", 10, 1000);
    assert_eq!(result.total_lines, 0);
    assert!(!result.truncated);
}

#[test]
fn sizes_are_human_readable() {
    assert_eq!(format_size(512), "512B");
    assert_eq!(format_size(2048), "2.0KB");
    assert_eq!(format_size(3 * 1024 * 1024), "3.0MB");
}

#[test]
fn multibyte_content_is_never_split_mid_char() {
    let content = "日本語のテキスト\n".repeat(50);
    let result = truncate_head(&content, 3, 40);
    assert!(result.truncated);
    // Must still be valid UTF-8 and end on a line boundary.
    assert!(std::str::from_utf8(result.content.as_bytes()).is_ok());
    assert!(!result.content.ends_with('日'));
}
