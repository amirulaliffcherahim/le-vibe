//! Reproducible before/after benchmark for le-vibe tool-output context compression.
//! Run with: `cargo test -p le-vibe-agent --test headroom_bench -- --nocapture`

use le_vibe_agent::{OutputStore, cap_tool_output, compress_tool_output};
use std::time::Instant;

fn make_json_array_with_buried_error() -> (String, &'static str) {
    let needle = "FATAL: connection pool exhausted on shard-3";
    let mut items = Vec::with_capacity(120);
    for i in 0..120 {
        if i == 67 {
            items.push(format!(
                r#"  {{"id":{i},"service":"billing-worker","region":"ap-southeast-1","status":500,"latency_ms":9812,"error":"{needle}","request_id":"req_{i:04}","payload_bytes":4096}}"#
            ));
        } else {
            items.push(format!(
                r#"  {{"id":{i},"service":"billing-worker","region":"ap-southeast-1","status":200,"latency_ms":{},"error":null,"request_id":"req_{i:04}","payload_bytes":1024}}"#,
                18 + (i % 15)
            ));
        }
    }
    (format!("[\n{}\n]", items.join(",\n")), needle)
}

fn make_noisy_build_log_with_buried_error() -> (String, &'static str) {
    let needle = "error[E0308]: mismatched types in crates/le-vibe-agent/src/lib.rs:412";
    let mut lines = Vec::with_capacity(420);
    for i in 0..420 {
        if i == 210 {
            lines.push(format!("2026-11-23T12:00:01Z ERROR {needle}"));
            lines.push("  expected `Result<String, Error>`, found `Option<String>`".to_string());
        } else if i % 3 == 0 {
            lines.push("2026-11-23T12:00:00Z INFO  [worker] heartbeat ok queue=0 mem=42MB".to_string());
        } else {
            lines.push(format!(
                "2026-11-23T12:00:00Z DEBUG [build] checking target unit #{i:03} cache=hit elapsed=1ms"
            ));
        }
    }
    (lines.join("\n"), needle)
}

fn make_source_file_read() -> (String, &'static str) {
    let needle = "    pub fn critical_exact_match(&self, key: &str) -> Option<&str> {";
    let mut lines = Vec::with_capacity(380);
    for i in 1..=380 {
        if i == 190 {
            lines.push(format!("{i:>4}: {needle}"));
        } else {
            lines.push(format!(
                "{i:>4}:     let item_{i} = process_record(state, \"value_{i}\");"
            ));
        }
    }
    (lines.join("\n"), needle)
}

fn make_massive_log_dump() -> (String, &'static str) {
    let needle = "FATAL panic at src/db/pool.rs:88: deadlock detected";
    let mut lines = Vec::with_capacity(1200);
    for i in 0..1200 {
        if i == 600 {
            lines.push(format!("2026-11-23T12:05:11Z {needle}"));
        } else {
            lines.push(format!(
                "2026-11-23T12:05:10Z INFO  [http] GET /api/v1/health 200 OK latency=2ms worker={}",
                i % 4
            ));
        }
    }
    (lines.join("\n"), needle)
}

#[test]
fn headroom_baseline_benchmark() {
    let cases = [
        ("1. JSON 120 items (err @ #67)", "mcp__api", make_json_array_with_buried_error()),
        ("2. Build log 420L (err @ #210)", "powershell", make_noisy_build_log_with_buried_error()),
        ("3. Source read 380L (exact)", "read", make_source_file_read()),
        ("4. Massive log 1200L (FATAL @ #600)", "bash", make_massive_log_dump()),
    ];

    println!("\n=== le-vibe Tool-Output Compression Benchmark (BASELINE: cap_tool_output) ===");
    println!(
        "{:<36} | {:>8} | {:>8} | {:>6} | {:>8} | {:>6} | {:>7}",
        "Scenario", "Raw B", "Sent B", "Save%", "TokSaved", "Needle", "Time us"
    );
    println!("{}", "-".repeat(98));

    let mut total_raw = 0usize;
    let mut total_sent = 0usize;
    let mut needles_kept = 0usize;

    for (label, _tool, (raw, needle)) in &cases {
        let t0 = Instant::now();
        let out = cap_tool_output(raw);
        let us = t0.elapsed().as_micros();

        let raw_b = raw.len();
        let sent_b = out.len();
        let saved_b = raw_b.saturating_sub(sent_b);
        let save_pct = if raw_b > 0 {
            (saved_b as f64 / raw_b as f64) * 100.0
        } else {
            0.0
        };
        let tok_saved = saved_b / 4;
        let kept = out.contains(needle);
        if kept {
            needles_kept += 1;
        }
        total_raw += raw_b;
        total_sent += sent_b;

        println!(
            "{:<36} | {:>8} | {:>8} | {:>5.1}% | {:>8} | {:>6} | {:>7}",
            label,
            raw_b,
            sent_b,
            save_pct,
            tok_saved,
            if kept { "YES" } else { "LOST" },
            us
        );
    }

    let total_saved = total_raw.saturating_sub(total_sent);
    let total_pct = (total_saved as f64 / total_raw as f64) * 100.0;
    println!("{}", "-".repeat(98));
    println!(
        "{:<36} | {:>8} | {:>8} | {:>5.1}% | {:>8} | {:>4}/4 |",
        "TOTAL",
        total_raw,
        total_sent,
        total_pct,
        total_saved / 4,
        needles_kept
    );

    println!("\n=== le-vibe Tool-Output Compression Benchmark (AFTER: Headroom-lite compress.rs) ===");
    println!(
        "{:<36} | {:>8} | {:>8} | {:>6} | {:>8} | {:>6} | {:>7}",
        "Scenario", "Raw B", "Sent B", "Save%", "TokSaved", "Needle", "Time us"
    );
    println!("{}", "-".repeat(98));

    let mut store = OutputStore::new();
    let mut after_raw = 0usize;
    let mut after_sent = 0usize;
    let mut after_needles = 0usize;

    for (label, tool, (raw, needle)) in &cases {
        let t0 = Instant::now();
        let out = compress_tool_output(tool, raw, &mut store);
        let us = t0.elapsed().as_micros();

        let raw_b = raw.len();
        let sent_b = out.len();
        let saved_b = raw_b.saturating_sub(sent_b);
        let save_pct = if raw_b > 0 {
            (saved_b as f64 / raw_b as f64) * 100.0
        } else {
            0.0
        };
        let tok_saved = saved_b / 4;
        let kept = out.contains(needle);
        assert!(kept, "needle must survive compression in {label}");
        if *tool == "read" {
            assert_eq!(out, *raw, "read <24KB must remain byte-exact for edit");
        }
        after_needles += 1;
        after_raw += raw_b;
        after_sent += sent_b;

        println!(
            "{:<36} | {:>8} | {:>8} | {:>5.1}% | {:>8} | {:>6} | {:>7}",
            label,
            raw_b,
            sent_b,
            save_pct,
            tok_saved,
            if kept { "YES" } else { "LOST" },
            us
        );
    }

    let after_saved = after_raw.saturating_sub(after_sent);
    let after_pct = (after_saved as f64 / after_raw as f64) * 100.0;
    println!("{}", "-".repeat(98));
    println!(
        "{:<36} | {:>8} | {:>8} | {:>5.1}% | {:>8} | {:>4}/4 |",
        "TOTAL",
        after_raw,
        after_sent,
        after_pct,
        after_saved / 4,
        after_needles
    );

    // Verify reversible CCR recovery of an omitted item (#50 from JSON array)
    let recovered = store.retrieve("out_1", Some("\"id\":50"), None, None);
    assert!(recovered.contains("\"id\":50"), "CCR retrieve_output must recover omitted items");
    assert_eq!(after_needles, 4);
    assert!(after_sent < total_sent, "Headroom-lite must send fewer bytes than baseline");
}
