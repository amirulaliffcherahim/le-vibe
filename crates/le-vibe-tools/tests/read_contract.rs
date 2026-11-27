//! `read` contract parity with pi's `core/tools/read.ts`: actionable continuation notices, an
//! error (not an empty string) for an out-of-range offset, and a bash hint when one line is huge.

use le_vibe_tools::execute_builtin_tool_approved;
use serde_json::json;
use std::io::Write;

fn fixture(name: &str, content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("le_vibe_read_contract");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    path
}

fn read_tool(path: &std::path::Path, extra: serde_json::Value) -> Result<String, String> {
    let mut args = json!({ "path": path.display().to_string() });
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            args[k] = v.clone();
        }
    }
    execute_builtin_tool_approved("read", &args).map_err(|e| e.to_string())
}

fn numbered(count: usize) -> String {
    (1..=count)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn small_file_reads_completely_without_a_notice() {
    let path = fixture("small.txt", "alpha\nbeta");
    let out = read_tool(&path, json!({})).expect("read");
    assert!(out.contains("1: alpha"), "{out}");
    assert!(out.contains("2: beta"), "{out}");
    assert!(!out.contains("Use offset="), "no notice needed for a whole file");
}

#[test]
fn offset_beyond_end_of_file_is_an_error() {
    let path = fixture("short.txt", "only\nthree\nlines");
    let err = read_tool(&path, json!({ "offset": 99 })).expect_err("offset past EOF must fail");
    assert!(err.contains("beyond end of file"), "{err}");
    assert!(err.contains("3"), "the error must state the real line count: {err}");
}

#[test]
fn limit_records_produce_an_actionable_continuation_notice() {
    let path = fixture("many.txt", &numbered(10));
    let out = read_tool(&path, json!({ "limit": 3 })).expect("read");
    assert!(out.contains("1: line1"), "{out}");
    assert!(
        out.contains("[Showing lines 1-3 of 10. Use offset=4 to continue.]"),
        "pi-style continuation notice expected, got:\n{out}"
    );
}

#[test]
fn offset_plus_limit_records_report_the_next_offset() {
    let path = fixture("many2.txt", &numbered(10));
    let out = read_tool(&path, json!({ "offset": 5, "limit": 2 })).expect("read");
    assert!(out.contains("5: line5"), "{out}");
    assert!(
        out.contains("[Showing lines 5-6 of 10. Use offset=7 to continue.]"),
        "got:\n{out}"
    );
}

#[test]
fn a_single_oversized_line_points_at_bash() {
    let path = fixture("huge.txt", &"z".repeat(60 * 1024));
    let out = read_tool(&path, json!({})).expect("read");
    assert!(
        out.contains("Use bash"),
        "pi tells the model to use bash for one huge line, got:\n{out}"
    );
    assert!(out.contains("sed -n"), "{out}");
}

#[test]
fn byte_truncation_names_the_limit_in_the_notice() {
    let path = fixture("bytes.txt", &numbered(4000));
    let out = read_tool(&path, json!({})).expect("read");
    assert!(out.contains("Showing lines"), "{out}");
    assert!(out.contains("Use offset="), "{out}");
    assert!(out.contains("2000") || out.contains("50.0KB"), "limit named: {out}");
}
