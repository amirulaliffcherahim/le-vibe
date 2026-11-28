//! Native Rust context compression & reversible CCR store (Headroom-lite).
//!
//! Tool-aware `ContentRouter` compresses large shell/MCP/JSON/log outputs once at append time
//! (preserving KV-cache prefix stability) while keeping `read` exact up to 24 KB and storing
//! raw originals in a bounded FIFO `OutputStore` recoverable via `retrieve_output`.

use serde_json::Value;
use std::collections::{HashMap, VecDeque};

use le_vibe_pybridge::ChatMessage;

const SHELL_COMPRESS_THRESHOLD: usize = 6 * 1024;
const READ_COMPRESS_THRESHOLD: usize = 24 * 1024;
/// Tool results newer than this stay verbatim: the active working set must not churn.
pub const AGE_KEEP_RECENT_TOOL_RESULTS: usize = 6;
/// Nothing smaller than this is worth eliding - a stub would not save a full line.
pub const AGE_MIN_ELIDE_CHARS: usize = 2_000;
/// Bytes of the original kept in the stub: the command header or first error line.
const AGE_HEAD_CHARS: usize = 240;
const RETRIEVE_MAX_BYTES: usize = 4_000;
const STORE_MAX_ENTRIES: usize = 64;
const STORE_MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompressionStats {
    pub raw_bytes: usize,
    pub compressed_bytes: usize,
    pub crushed_count: usize,
    pub retrieve_hits: usize,
}

impl CompressionStats {
    pub fn bytes_saved(&self) -> usize {
        self.raw_bytes.saturating_sub(self.compressed_bytes)
    }

    pub fn est_tokens_saved(&self) -> usize {
        self.bytes_saved() / 4
    }
}

/// Bounded in-memory CCR (Cache-Content-Retrieve) store for one session.
#[derive(Debug, Default)]
pub struct OutputStore {
    next_id: usize,
    total_bytes: usize,
    order: VecDeque<String>,
    entries: HashMap<String, String>,
    pub stats: CompressionStats,
}

impl OutputStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store raw output and return its short handle (`out_1`, `out_2`, ...).
    pub fn put(&mut self, raw: String) -> String {
        self.next_id += 1;
        let id = format!("out_{}", self.next_id);
        let len = raw.len();
        while (self.entries.len() >= STORE_MAX_ENTRIES
            || self.total_bytes.saturating_add(len) > STORE_MAX_BYTES)
            && !self.order.is_empty()
        {
            if let Some(oldest) = self.order.pop_front()
                && let Some(removed) = self.entries.remove(&oldest)
            {
                self.total_bytes = self.total_bytes.saturating_sub(removed.len());
            }
        }
        self.total_bytes = self.total_bytes.saturating_add(len);
        self.order.push_back(id.clone());
        self.entries.insert(id.clone(), raw);
        id
    }

    /// Retrieve lines from a stored raw output by optional query or 1-based line offset/limit.
    /// Hard-capped at `RETRIEVE_MAX_BYTES` so `retrieve_output` never triggers re-compression.
    pub fn retrieve(
        &mut self,
        id: &str,
        query: Option<&str>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> String {
        let Some(raw) = self.entries.get(id) else {
            return format!(
                "Output ref '{id}' is not in the current session cache (evicted or from a resumed session). Re-run the source tool to inspect it."
            );
        };
        self.stats.retrieve_hits += 1;
        let lines: Vec<&str> = raw.lines().collect();
        let total_lines = lines.len();

        if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
            let q_lower = q.to_ascii_lowercase();
            let mut include = vec![false; total_lines];
            let mut match_count = 0usize;
            for (idx, line) in lines.iter().enumerate() {
                if line.to_ascii_lowercase().contains(&q_lower) {
                    match_count += 1;
                    let start = idx.saturating_sub(2);
                    let end = (idx + 2).min(total_lines.saturating_sub(1));
                    for flag in &mut include[start..=end] {
                        *flag = true;
                    }
                }
            }
            if match_count == 0 {
                return format!("No lines matching '{q}' in {id} ({total_lines} lines).");
            }
            let mut out = format!("Matches for '{q}' in {id} ({match_count} hits / {total_lines} lines):\n");
            let mut prev_included = true;
            for (idx, line) in lines.iter().enumerate() {
                if !include[idx] {
                    prev_included = false;
                    continue;
                }
                if !prev_included && !out.is_empty() {
                    out.push_str("  ...\n");
                }
                prev_included = true;
                let row = format!("{:>5}: {line}\n", idx + 1);
                if out.len() + row.len() > RETRIEVE_MAX_BYTES {
                    out.push_str("... [capped at 4KB; refine `query` or pass `offset`/`limit`]");
                    break;
                }
                out.push_str(&row);
            }
            return out.trim_end().to_string();
        }

        let start_line = offset.unwrap_or(1).max(1);
        let max_lines = limit.unwrap_or(80).clamp(1, 200);
        let start_idx = start_line.saturating_sub(1).min(total_lines);
        let end_idx = (start_idx + max_lines).min(total_lines);
        let mut out = format!(
            "{id} lines {}-{} of {total_lines}:\n",
            start_idx + 1,
            end_idx
        );
        for (idx, line) in lines[start_idx..end_idx].iter().enumerate() {
            let row = format!("{:>5}: {line}\n", start_idx + idx + 1);
            if out.len() + row.len() > RETRIEVE_MAX_BYTES {
                out.push_str("... [capped at 4KB; pass a smaller `limit` or higher `offset`]");
                break;
            }
            out.push_str(&row);
        }
        out.trim_end().to_string()
    }
}

/// Tool-aware entry point: compresses large tool outputs once at append time and registers raw
/// text in `store` so the model can recover exact lines via `retrieve_output`.
/// Elide the bodies of *stale* tool results before a request.
///
/// Every step resends the whole transcript, so one 24 KB read kept forever costs ~6k tokens on
/// every later step. The newest [`AGE_KEEP_RECENT_TOOL_RESULTS`] results are the working set and
/// stay verbatim; older bulky ones shrink to a stub that keeps the first line (the command,
/// exit code or headline error) so the model still knows what happened.
///
/// This only shapes the outgoing request - the session DAG keeps the full text.
pub fn age_stale_tool_results(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let tool_indexes: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "tool")
        .map(|(i, _)| i)
        .collect();
    if tool_indexes.len() <= AGE_KEEP_RECENT_TOOL_RESULTS {
        return messages.to_vec();
    }
    let stale_until = tool_indexes.len() - AGE_KEEP_RECENT_TOOL_RESULTS;
    let stale: std::collections::HashSet<usize> = tool_indexes[..stale_until].iter().copied().collect();

    messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            if !stale.contains(&i) || msg.content.len() < AGE_MIN_ELIDE_CHARS {
                return msg.clone();
            }
            let mut aged = msg.clone();
            aged.content = age_stub(msg);
            aged
        })
        .collect()
}

fn age_stub(msg: &ChatMessage) -> String {
    let head: String = msg.content.chars().take(AGE_HEAD_CHARS).collect();
    let head = head.trim_end();
    let tool = msg.name.as_deref().unwrap_or("tool");
    let lines = msg.content.lines().count();
    let note = format!(
        "\n[aged out {tool} result: {lines} lines elided to keep context small - re-run the tool \
if the body is still needed]"
    );
    if head.len() < msg.content.len() {
        format!("{head}{note}")
    } else {
        note.trim_start().to_string()
    }
}

pub fn compress_tool_output(tool_name: &str, raw: &str, store: &mut OutputStore) -> String {
    if tool_name == "retrieve_output" {
        return raw.to_string();
    }

    // Never line-dedup or aggressively crush `read` so `edit(oldText)` always sees exact text.
    if tool_name == "read" {
        if raw.len() <= READ_COMPRESS_THRESHOLD {
            return raw.to_string();
        }
        let id = store.put(raw.to_string());
        let compressed = compress_head_tail_preserve_lines(raw, 16 * 1024, 6 * 1024, &id, true);
        record_stats(store, raw.len(), compressed.len());
        return compressed;
    }

    if raw.len() <= SHELL_COMPRESS_THRESHOLD {
        return raw.to_string();
    }

    // Route 1: Valid JSON array or object wrapping a large array -> SmartCrusher
    let trimmed = raw.trim();
    if (trimmed.starts_with('[') || trimmed.starts_with('{'))
        && let Ok(val) = serde_json::from_str::<Value>(trimmed)
        && let Some(crushed_body) = crush_json(&val)
        && crushed_body.len() + 180 < raw.len()
    {
        let id = store.put(raw.to_string());
        let saved_tok = raw.len().saturating_sub(crushed_body.len()) / 4;
        let out = format!(
            "{crushed_body}\n\n[compressed {} -> {} (~{saved_tok} tok saved), ref={id}; call retrieve_output(id=\"{id}\", query=..., offset=..., limit=...)]",
            fmt_kb(raw.len()),
            fmt_kb(crushed_body.len())
        );
        record_stats(store, raw.len(), out.len());
        return out;
    }

    // Route 2: Multi-line logs / build / test output -> LogCrusher
    if raw.lines().count() >= 40
        && let Some(crushed_body) = crush_logs(raw)
        && crushed_body.len() + 180 < raw.len()
    {
        let id = store.put(raw.to_string());
        let saved_tok = raw.len().saturating_sub(crushed_body.len()) / 4;
        let out = format!(
            "{crushed_body}\n\n[compressed {} -> {} (~{saved_tok} tok saved), ref={id}; call retrieve_output(id=\"{id}\", query=..., offset=..., limit=...)]",
            fmt_kb(raw.len()),
            fmt_kb(crushed_body.len())
        );
        record_stats(store, raw.len(), out.len());
        return out;
    }

    // Route 3: Large unstructured text fallback (>24KB) -> head + tail + error lines
    if raw.len() <= READ_COMPRESS_THRESHOLD {
        return raw.to_string();
    }
    let id = store.put(raw.to_string());
    let compressed = compress_head_tail_preserve_lines(raw, 12 * 1024, 6 * 1024, &id, false);
    record_stats(store, raw.len(), compressed.len());
    compressed
}

fn record_stats(store: &mut OutputStore, raw_len: usize, out_len: usize) {
    store.stats.raw_bytes = store.stats.raw_bytes.saturating_add(raw_len);
    store.stats.compressed_bytes = store.stats.compressed_bytes.saturating_add(out_len);
    store.stats.crushed_count += 1;
}

fn fmt_kb(bytes: usize) -> String {
    if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

fn is_anomalous_json_item(v: &Value) -> bool {
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                let kl = k.to_ascii_lowercase();
                if matches!(kl.as_str(), "error" | "err" | "exception" | "fatal" | "stack" | "traceback")
                    && !val.is_null()
                    && val.as_str().map(|s| !s.is_empty()).unwrap_or(true)
                    && val.as_bool() != Some(false)
                {
                    return true;
                }
                if (kl == "status" || kl == "status_code" || kl == "code")
                    && val.as_i64().is_some_and(|n| n >= 400)
                {
                    return true;
                }
                if (kl == "ok" || kl == "success" || kl == "passed") && val.as_bool() == Some(false) {
                    return true;
                }
                if (kl == "level" || kl == "severity")
                    && val.as_str().is_some_and(|s| {
                        let sl = s.to_ascii_lowercase();
                        sl.contains("err") || sl.contains("fatal") || sl.contains("crit") || sl.contains("panic")
                    })
                {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

fn crush_json_array(arr: &[Value]) -> Option<String> {
    if arr.len() <= 8 {
        return None;
    }
    let total = arr.len();
    let mut kept_indices: Vec<usize> = Vec::new();
    for i in 0..3.min(total) {
        kept_indices.push(i);
    }
    let mut anomalies = 0usize;
    for (i, item) in arr.iter().enumerate().skip(3).take(total.saturating_sub(5)) {
        if is_anomalous_json_item(item) && anomalies < 12 {
            kept_indices.push(i);
            anomalies += 1;
        }
    }
    for i in total.saturating_sub(2)..total {
        if !kept_indices.contains(&i) {
            kept_indices.push(i);
        }
    }
    kept_indices.sort_unstable();
    kept_indices.dedup();

    let schema = arr.first().and_then(Value::as_object).map(|m| {
        let keys: Vec<&str> = m.keys().map(String::as_str).collect();
        keys.join(", ")
    });
    let mut lines = Vec::new();
    lines.push(format!(
        "JSON array ({total} items, showing {} incl. {anomalies} error/anomaly item(s){}):",
        kept_indices.len(),
        schema
            .as_deref()
            .map(|s| format!("; keys=[{s}]"))
            .unwrap_or_default()
    ));
    let mut prev: Option<usize> = None;
    for idx in kept_indices {
        if let Some(p) = prev
            && idx > p + 1
        {
            lines.push(format!("  ... ({} items omitted) ...", idx - p - 1));
        }
        let compact = serde_json::to_string(&arr[idx]).unwrap_or_default();
        lines.push(format!("  [{idx}] {compact}"));
        prev = Some(idx);
    }
    Some(lines.join("\n"))
}

fn crush_json(val: &Value) -> Option<String> {
    match val {
        Value::Array(arr) => crush_json_array(arr),
        Value::Object(map) => {
            let (big_key, big_arr) = map
                .iter()
                .filter_map(|(k, v)| v.as_array().map(|a| (k, a)))
                .max_by_key(|(_, a)| a.len())?;
            let crushed = crush_json_array(big_arr)?;
            let mut meta = Vec::new();
            for (k, v) in map {
                if k == big_key {
                    continue;
                }
                let s = serde_json::to_string(v).unwrap_or_default();
                let preview: String = s.chars().take(120).collect();
                meta.push(format!("  {k}: {preview}"));
            }
            if meta.is_empty() {
                Some(format!("field `{big_key}` -> {crushed}"))
            } else {
                Some(format!(
                    "JSON object metadata:\n{}\nfield `{big_key}` -> {crushed}",
                    meta.join("\n")
                ))
            }
        }
        _ => None,
    }
}

fn is_important_log_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("fatal")
        || lower.contains("panic")
        || lower.contains("error[")
        || lower.contains(" error ")
        || lower.starts_with("error:")
        || lower.contains("exception")
        || lower.contains("traceback")
        || lower.contains("failed")
        || lower.contains("caused by:")
        || lower.contains("assertionerror")
        || lower.contains("segfault")
}

/// Normalize volatile timestamps/numbers so repetitive `INFO`/`DEBUG` lines collapse together.
fn log_signature(line: &str) -> String {
    let mut out = String::with_capacity(line.len().min(96));
    let mut prev_digit = false;
    for ch in line.chars().take(120) {
        if ch.is_ascii_digit() {
            if !prev_digit {
                out.push('#');
                prev_digit = true;
            }
        } else {
            prev_digit = false;
            out.push(ch);
        }
    }
    out
}

fn crush_logs(raw: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let total = lines.len();
    if total < 40 {
        return None;
    }

    let mut keep = vec![false; total];
    let head = 8.min(total);
    let tail_start = total.saturating_sub(8);
    for flag in &mut keep[..head] {
        *flag = true;
    }
    for flag in &mut keep[tail_start..] {
        *flag = true;
    }

    let mut important_hits = 0usize;
    for (idx, line) in lines.iter().enumerate() {
        if is_important_log_line(line) {
            important_hits += 1;
            let s = idx.saturating_sub(2);
            let e = (idx + 2).min(total - 1);
            for flag in &mut keep[s..=e] {
                *flag = true;
            }
        }
    }

    let mut out = Vec::new();
    let mut i = 0usize;
    while i < total {
        if keep[i] {
            // Collapse exact consecutive repeats even in kept sections.
            let mut run = 1usize;
            while i + run < total && keep[i + run] && lines[i + run] == lines[i] {
                run += 1;
            }
            if run > 2 {
                out.push(format!("{} [repeated x{run}]", lines[i]));
            } else {
                for j in 0..run {
                    out.push(lines[i + j].to_string());
                }
            }
            i += run;
        } else {
            let start = i;
            let mut sig_counts: HashMap<String, usize> = HashMap::new();
            while i < total && !keep[i] {
                *sig_counts.entry(log_signature(lines[i])).or_insert(0) += 1;
                i += 1;
            }
            let omitted = i - start;
            let unique = sig_counts.len();
            out.push(format!(
                "... [{omitted} lines omitted (L{}-L{}, {unique} unique pattern(s))] ...",
                start + 1,
                i
            ));
        }
    }

    let header = format!(
        "Log summary ({total} lines, {important_hits} error/warn hit(s) preserved):"
    );
    Some(format!("{header}\n{}", out.join("\n")))
}

fn floor_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn compress_head_tail_preserve_lines(
    raw: &str,
    head_bytes: usize,
    tail_bytes: usize,
    ref_id: &str,
    is_read_tool: bool,
) -> String {
    let head_end = floor_char_boundary(raw, head_bytes);
    let tail_start = ceil_char_boundary(raw, raw.len().saturating_sub(tail_bytes));
    let omitted_bytes = tail_start.saturating_sub(head_end);
    let hint = if is_read_tool {
        format!(
            "use read(offset, limit) or retrieve_output(id=\"{ref_id}\", query=..., offset=..., limit=...)"
        )
    } else {
        format!("call retrieve_output(id=\"{ref_id}\", query=..., offset=..., limit=...)")
    };
    format!(
        "{}\n\n... [{omitted_bytes} bytes omitted, ref={ref_id}; {hint}] ...\n\n{}",
        &raw[..head_end],
        &raw[tail_start..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_msg(name: &str, body: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".to_string(),
            content: body.to_string(),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: Some(format!("call_{name}")),
            name: Some(name.to_string()),
        }
    }

    #[test]
    fn aging_keeps_the_recent_working_set_verbatim() {
        let body = "x".repeat(8_000);
        let messages: Vec<ChatMessage> = (0..AGE_KEEP_RECENT_TOOL_RESULTS)
            .map(|i| tool_msg(&format!("read{i}"), &body))
            .collect();
        let aged = age_stale_tool_results(&messages);
        assert_eq!(aged, messages);
    }

    #[test]
    fn aging_elides_old_bodies_but_keeps_the_head_and_a_hint() {
        let old_body = format!("exit_code=0\n{}", "log line\n".repeat(2_000));
        let recent_body = "y".repeat(8_000);
        let mut messages = vec![tool_msg("powershell", &old_body)];
        for i in 0..AGE_KEEP_RECENT_TOOL_RESULTS {
            messages.push(tool_msg(&format!("read{i}"), &recent_body));
        }

        let aged = age_stale_tool_results(&messages);

        assert!(aged[0].content.len() < 400, "stale body must shrink");
        assert!(aged[0].content.starts_with("exit_code=0"), "head is preserved");
        assert!(aged[0].content.contains("re-run the tool"));
        assert!(aged[0].content.contains("powershell"));
        // Identity and ordering are untouched so tool_call pairing still holds.
        assert_eq!(aged[0].tool_call_id, messages[0].tool_call_id);
        assert_eq!(aged[1..], messages[1..]);
    }

    #[test]
    fn aging_leaves_small_results_and_user_turns_alone() {
        let small = tool_msg("ls", "src\ncrates");
        let user = ChatMessage {
            role: "user".to_string(),
            content: "z".repeat(9_000),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
        };
        let mut messages = vec![small.clone(), user.clone()];
        for i in 0..AGE_KEEP_RECENT_TOOL_RESULTS {
            messages.push(tool_msg(&format!("read{i}"), &"y".repeat(4_000)));
        }

        let aged = age_stale_tool_results(&messages);
        assert_eq!(aged[0], small);
        assert_eq!(aged[1], user);
    }

    #[test]
    fn smart_crusher_preserves_buried_json_error_and_allows_retrieve() {
        let mut items = Vec::new();
        for i in 0..140 {
            if i == 55 {
                items.push(format!(r#"{{"id":{i},"status":500,"error":"FATAL: shard-3 down"}}"#));
            } else {
                items.push(format!(r#"{{"id":{i},"status":200,"error":null,"payload":"xxxxxxxxxx"}}"#));
            }
        }
        let raw = format!("[\n{}\n]", items.join(",\n"));
        let mut store = OutputStore::new();
        let out = compress_tool_output("mcp__api", &raw, &mut store);
        assert!(out.len() < raw.len() / 2);
        assert!(out.contains("FATAL: shard-3 down"));
        assert!(out.contains("ref=out_1"));

        let retrieved = store.retrieve("out_1", Some("\"id\":42"), None, None);
        assert!(retrieved.contains("\"id\":42"));
        assert_eq!(store.stats.retrieve_hits, 1);
    }

    #[test]
    fn log_crusher_preserves_fatal_and_collapses_noise() {
        let mut lines = Vec::new();
        for i in 0..400 {
            if i == 200 {
                lines.push("2026-11-23T12:00:00Z FATAL panic at pool.rs:88".to_string());
            } else {
                lines.push(format!("2026-11-23T12:00:00Z INFO heartbeat #{i}"));
            }
        }
        let raw = lines.join("\n");
        let mut store = OutputStore::new();
        let out = compress_tool_output("bash", &raw, &mut store);
        assert!(out.len() < raw.len() / 3);
        assert!(out.contains("FATAL panic at pool.rs:88"));
        assert!(out.contains("ref=out_1"));
    }

    #[test]
    fn read_tool_is_untouched_below_24kb() {
        let raw = "fn main() {}\n".repeat(1200); // ~15.6 KB
        let mut store = OutputStore::new();
        let out = compress_tool_output("read", &raw, &mut store);
        assert_eq!(out, raw);
        assert_eq!(store.stats.crushed_count, 0);
    }

    #[test]
    fn missing_ref_returns_clean_resumed_session_fallback() {
        let mut store = OutputStore::new();
        let msg = store.retrieve("out_99", None, None, None);
        assert!(msg.contains("not in the current session cache"));
    }
}
