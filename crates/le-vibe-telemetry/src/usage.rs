//! Per-request forensic usage logger, cache-bust classifier, and read-only `/cost` formatter.
//!
//! Appends one JSONL record per outbound LLM HTTP call to `~/.le-vibe/logs/usage-<session_id>.jsonl`
//! so retries, compactions, subagents, verification nudges, and cache busts are all inspectable
//! without ever injecting stats into model context.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrefixHashes {
    pub system_hash: String,
    pub tools_hash: String,
    pub history_prefix_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageLogRecord {
    pub ts: String,
    pub session_id: String,
    pub turn_idx: usize,
    pub req_idx: usize,
    pub source: String,
    pub provider: String,
    pub model: String,
    pub http_status: u16,
    pub stop_reason: String,
    pub wall_ms: u64,
    pub idle_since_prev_req_s: u64,
    pub in_uncached: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub out: u64,
    pub cache_hit_pct: f64,
    pub cost_usd: f64,
    pub prefix: PrefixHashes,
    pub cache_bust_reason: String,
    pub anomalies: Vec<String>,
}

/// Input captured around a single outbound LLM request.
#[derive(Debug, Clone, Default)]
pub struct UsageEventInput {
    pub turn_idx: usize,
    pub source: String,
    pub provider: String,
    pub model: String,
    pub http_status: u16,
    pub stop_reason: String,
    pub wall_ms: u64,
    pub in_uncached: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub out: u64,
    pub cost_usd: f64,
    pub system_text: String,
    pub tools_text: String,
    pub history_entry_hashes: Vec<String>,
    pub compaction_occurred: bool,
    pub extra_anomalies: Vec<String>,
    /// Optional explicit epoch seconds for deterministic testing.
    pub now_secs_override: Option<u64>,
}

#[derive(Debug, Default)]
struct LoggerState {
    req_idx: usize,
    prev_ts_secs: Option<u64>,
    prev_system_hash: Option<String>,
    prev_tools_hash: Option<String>,
    prev_history_hashes: Vec<String>,
    prev_cache_hit_pct: f64,
    prev_system_text: Option<String>,
    prev_tools_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UsageLogger {
    session_id: String,
    log_path: PathBuf,
    debug_cache: bool,
    state: Arc<Mutex<LoggerState>>,
}

impl UsageLogger {
    pub fn new(session_id: impl Into<String>, log_path: PathBuf) -> Self {
        let debug_cache = std::env::var("LE_VIBE_DEBUG_CACHE")
            .ok()
            .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"));
        Self::with_debug_cache(session_id, log_path, debug_cache)
    }

    pub fn with_debug_cache(
        session_id: impl Into<String>,
        log_path: PathBuf,
        debug_cache: bool,
    ) -> Self {
        Self {
            session_id: sanitize_id(&session_id.into()),
            log_path,
            debug_cache,
            state: Arc::new(Mutex::new(LoggerState::default())),
        }
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn record_request(&self, input: UsageEventInput) -> UsageLogRecord {
        let now_secs = input.now_secs_override.unwrap_or_else(now_epoch_secs);
        let ts = format_iso_utc(now_secs);
        let system_hash = fnv1a_hex(input.system_text.as_bytes());
        let tools_hash = fnv1a_hex(input.tools_text.as_bytes());
        let history_prefix_hash = hash_slice(&input.history_entry_hashes);
        let total_in = input.in_uncached.saturating_add(input.cache_read);
        let cache_hit_pct = if total_in > 0 {
            ((input.cache_read as f64) * 100.0 / (total_in as f64) * 10.0).round() / 10.0
        } else {
            0.0
        };

        let mut guard = self.state.lock().expect("usage logger lock");
        guard.req_idx += 1;
        let req_idx = guard.req_idx;
        let idle_since_prev_req_s = guard
            .prev_ts_secs
            .map(|prev| now_secs.saturating_sub(prev))
            .unwrap_or(0);

        let mut diff_dump: Option<(String, String, &'static str)> = None;
        let cache_bust_reason = if req_idx == 1 {
            "none".to_string()
        } else if input.compaction_occurred || input.source == "compact" {
            "compaction".to_string()
        } else if guard.prev_system_hash.as_deref() != Some(system_hash.as_str()) {
            if self.debug_cache
                && let Some(prev) = guard.prev_system_text.clone()
            {
                diff_dump = Some((prev, input.system_text.clone(), "system_prompt"));
            }
            "system_changed".to_string()
        } else if guard.prev_tools_hash.as_deref() != Some(tools_hash.as_str()) {
            if self.debug_cache
                && let Some(prev) = guard.prev_tools_text.clone()
            {
                diff_dump = Some((prev, input.tools_text.clone(), "tools"));
            }
            "tools_changed".to_string()
        } else if history_was_rewritten(&guard.prev_history_hashes, &input.history_entry_hashes) {
            "history_rewritten".to_string()
        } else if guard.prev_cache_hit_pct > 80.0
            && cache_hit_pct < 20.0
            && idle_since_prev_req_s > 280
        {
            "ttl_expired".to_string()
        } else if idle_since_prev_req_s < 60 && cache_hit_pct < 20.0 && total_in >= 1_000 {
            "provider_or_wire_miss".to_string()
        } else {
            "none".to_string()
        };

        let mut anomalies = Vec::new();
        if req_idx > 1 && input.in_uncached > 15_000 {
            anomalies.push("UNCACHED_SPIKE".to_string());
        }
        if input.source == "retry" {
            anomalies.push("RETRY_BURN".to_string());
        }
        for a in input.extra_anomalies {
            if !anomalies.contains(&a) {
                anomalies.push(a);
            }
        }

        guard.prev_ts_secs = Some(now_secs);
        guard.prev_system_hash = Some(system_hash.clone());
        guard.prev_tools_hash = Some(tools_hash.clone());
        guard.prev_history_hashes = input.history_entry_hashes;
        guard.prev_cache_hit_pct = cache_hit_pct;
        if self.debug_cache {
            guard.prev_system_text = Some(input.system_text);
            guard.prev_tools_text = Some(input.tools_text);
        }
        drop(guard);

        if let Some((before, after, label)) = diff_dump {
            self.write_cache_bust_diff(req_idx, label, &before, &after);
        }

        let record = UsageLogRecord {
            ts,
            session_id: self.session_id.clone(),
            turn_idx: input.turn_idx,
            req_idx,
            source: input.source,
            provider: input.provider,
            model: input.model,
            http_status: input.http_status,
            stop_reason: input.stop_reason,
            wall_ms: input.wall_ms,
            idle_since_prev_req_s,
            in_uncached: input.in_uncached,
            cache_read: input.cache_read,
            cache_write: input.cache_write,
            out: input.out,
            cache_hit_pct,
            cost_usd: input.cost_usd,
            prefix: PrefixHashes {
                system_hash,
                tools_hash,
                history_prefix_hash,
            },
            cache_bust_reason,
            anomalies,
        };

        self.append_jsonl(&record);
        record
    }

    fn append_jsonl(&self, record: &UsageLogRecord) {
        if let Some(parent) = self.log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            && let Ok(line) = serde_json::to_string(record)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    fn write_cache_bust_diff(&self, req_idx: usize, label: &str, before: &str, after: &str) {
        let dir = self
            .log_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let _ = fs::create_dir_all(&dir);
        let diff_path = dir.join(format!("cache-bust-{}-req{req_idx}.diff", self.session_id));
        let mut out = format!("--- {label} (before req {req_idx})\n+++ {label} (at req {req_idx})\n");
        let a_lines: Vec<&str> = before.lines().collect();
        let b_lines: Vec<&str> = after.lines().collect();
        let max_len = a_lines.len().max(b_lines.len());
        for i in 0..max_len {
            match (a_lines.get(i), b_lines.get(i)) {
                (Some(a), Some(b)) if a == b => {}
                (Some(a), Some(b)) => {
                    out.push_str(&format!("@@ line {} @@\n-{a}\n+{b}\n", i + 1));
                }
                (Some(a), None) => {
                    out.push_str(&format!("@@ line {} @@\n-{a}\n", i + 1));
                }
                (None, Some(b)) => {
                    out.push_str(&format!("@@ line {} @@\n+{b}\n", i + 1));
                }
                (None, None) => {}
            }
        }
        let _ = fs::write(diff_path, out);
    }
}

/// Deterministic 64-bit FNV-1a hex hash (zero extra crate dependencies).
pub fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn hash_slice(items: &[String]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for item in items {
        for &b in item.as_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn history_was_rewritten(prev: &[String], curr: &[String]) -> bool {
    if prev.is_empty() {
        return false;
    }
    if curr.len() < prev.len() {
        return true;
    }
    prev.iter().zip(curr.iter()).any(|(a, b)| a != b)
}

pub fn sanitize_id(raw: &str) -> String {
    let clean: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.is_empty() {
        "session".to_string()
    } else {
        clean
    }
}

pub fn default_logs_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".le-vibe").join("logs")
}

pub fn default_usage_log_path(session_id: &str) -> PathBuf {
    default_logs_dir().join(format!("usage-{}.jsonl", sanitize_id(session_id)))
}

pub fn latest_usage_log_path() -> Option<PathBuf> {
    let dir = default_logs_dir();
    let entries = fs::read_dir(dir).ok()?;
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("usage-") && n.ends_with(".jsonl"))
        })
        .max_by_key(|p| {
            p.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH)
        })
}

pub fn read_usage_log(path: &Path) -> Result<Vec<UsageLogRecord>, std::io::Error> {
    let raw = fs::read_to_string(path)?;
    let mut records = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<UsageLogRecord>(trimmed) {
            records.push(rec);
        }
    }
    Ok(records)
}

pub fn format_usage_report(records: &[UsageLogRecord]) -> String {
    if records.is_empty() {
        return "No forensic usage records found for this session.".to_string();
    }

    let mut in_uncached = 0u64;
    let mut cache_read = 0u64;
    let mut cache_write = 0u64;
    let mut out_tok = 0u64;
    let mut cost_usd = 0.0f64;

    // source -> (count, in_uncached, cache_read, out, cost_usd)
    let mut by_source: BTreeMap<&str, (usize, u64, u64, u64, f64)> = BTreeMap::new();
    let mut busts: Vec<String> = Vec::new();
    let mut anomaly_lines: Vec<String> = Vec::new();

    for r in records {
        in_uncached = in_uncached.saturating_add(r.in_uncached);
        cache_read = cache_read.saturating_add(r.cache_read);
        cache_write = cache_write.saturating_add(r.cache_write);
        out_tok = out_tok.saturating_add(r.out);
        cost_usd += r.cost_usd;

        let e = by_source.entry(r.source.as_str()).or_default();
        e.0 += 1;
        e.1 = e.1.saturating_add(r.in_uncached);
        e.2 = e.2.saturating_add(r.cache_read);
        e.3 = e.3.saturating_add(r.out);
        e.4 += r.cost_usd;

        if r.cache_bust_reason != "none" {
            busts.push(format!(
                "  req #{} (turn {}, {}): {} (in_uncached={}, cache_hit={:.1}%)",
                r.req_idx, r.turn_idx, r.source, r.cache_bust_reason, r.in_uncached, r.cache_hit_pct
            ));
        }
        if !r.anomalies.is_empty() {
            anomaly_lines.push(format!(
                "  req #{} (turn {}, {}): [{}] (in_uncached={}, wall={}ms)",
                r.req_idx,
                r.turn_idx,
                r.source,
                r.anomalies.join(", "),
                r.in_uncached,
                r.wall_ms
            ));
        }
    }

    let total_in = in_uncached.saturating_add(cache_read);
    let hit_pct = if total_in > 0 {
        (cache_read as f64) * 100.0 / (total_in as f64)
    } else {
        0.0
    };
    // Estimate cache savings assuming cached reads are ~90% cheaper than uncached input.
    let est_savings_usd = if cost_usd > 0.0 && total_in > 0 {
        let avg_rate = cost_usd / ((in_uncached as f64) + (cache_read as f64) * 0.1 + (out_tok as f64) * 3.0).max(1.0);
        (cache_read as f64) * avg_rate * 0.9
    } else {
        0.0
    };

    let mut s = String::new();
    s.push_str(&format!(
        "Session Usage Report ({}, {} requests)\n",
        records[0].session_id,
        records.len()
    ));
    s.push_str(&format!(
        "Totals: in_uncached={in_uncached}  cache_read={cache_read}  cache_write={cache_write}  out={out_tok}  cache_hit={hit_pct:.1}%  cost=${cost_usd:.4}  est_cache_savings=${est_savings_usd:.4}\n"
    ));

    s.push_str("\nBy source:\n");
    for (src, (cnt, u, c, o, cost)) in by_source {
        s.push_str(&format!(
            "  {src:<18} reqs={cnt:<3} in_uncached={u:<8} cache_read={c:<8} out={o:<6} cost=${cost:.4}\n"
        ));
    }

    if !busts.is_empty() {
        s.push_str("\nCache busts:\n");
        for b in busts {
            s.push_str(&b);
            s.push('\n');
        }
    }

    if !anomaly_lines.is_empty() {
        s.push_str("\nAnomalies:\n");
        for a in anomaly_lines {
            s.push_str(&a);
            s.push('\n');
        }
    }

    let mut ranked: Vec<&UsageLogRecord> = records.iter().collect();
    ranked.sort_by(|a, b| {
        b.in_uncached
            .cmp(&a.in_uncached)
            .then_with(|| b.cost_usd.total_cmp(&a.cost_usd))
            .then_with(|| b.wall_ms.cmp(&a.wall_ms))
    });
    s.push_str("\nTop 5 costliest requests:\n");
    for r in ranked.into_iter().take(5) {
        s.push_str(&format!(
            "  req #{:<3} {:<18} in_uncached={:<7} cache_read={:<7} out={:<5} wall={}ms cost=${:.4}\n",
            r.req_idx, r.source, r.in_uncached, r.cache_read, r.out, r.wall_ms, r.cost_usd
        ));
    }
    s
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn format_iso_utc(epoch_secs: u64) -> String {
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm, zero chrono dep).
    let days = (epoch_secs / 86_400) as i64;
    let secs_of_day = epoch_secs % 86_400;
    let h = secs_of_day / 3_600;
    let m = (secs_of_day % 3_600) / 60;
    let s = secs_of_day % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "le_vibe_telemetry_{tag}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_system_and_tools_mutation_and_writes_debug_diff() {
        let dir = temp_dir("cache_diff");
        let log_path = dir.join("usage-sess1.jsonl");
        let logger = UsageLogger::with_debug_cache("sess1", log_path.clone(), true);

        let r1 = logger.record_request(UsageEventInput {
            turn_idx: 1,
            source: "main".into(),
            provider: "vibe".into(),
            model: "deepseek-v4-1-flash-1017".into(),
            http_status: 200,
            stop_reason: "tool_calls".into(),
            wall_ms: 400,
            in_uncached: 4_000,
            cache_read: 0,
            out: 100,
            cost_usd: 0.002,
            system_text: "line1\nline2".into(),
            tools_text: r#"[{"name":"read"}]"#.into(),
            history_entry_hashes: vec!["h1".into()],
            now_secs_override: Some(1_700_000_000),
            ..Default::default()
        });
        assert_eq!(r1.cache_bust_reason, "none");

        let r2 = logger.record_request(UsageEventInput {
            turn_idx: 2,
            source: "main".into(),
            provider: "vibe".into(),
            model: "deepseek-v4-1-flash-1017".into(),
            http_status: 200,
            stop_reason: "stop".into(),
            wall_ms: 350,
            in_uncached: 4_100,
            cache_read: 0,
            out: 90,
            cost_usd: 0.002,
            system_text: "line1\nline2_changed".into(),
            tools_text: r#"[{"name":"read"}]"#.into(),
            history_entry_hashes: vec!["h1".into(), "h2".into()],
            now_secs_override: Some(1_700_000_010),
            ..Default::default()
        });
        assert_eq!(r2.cache_bust_reason, "system_changed");
        let diff_file = dir.join("cache-bust-sess1-req2.diff");
        assert!(diff_file.exists(), "LE_VIBE_DEBUG_CACHE must write diff file");
        let diff_body = fs::read_to_string(&diff_file).unwrap();
        assert!(diff_body.contains("-line2") && diff_body.contains("+line2_changed"));

        let r3 = logger.record_request(UsageEventInput {
            turn_idx: 3,
            source: "retry".into(),
            provider: "vibe".into(),
            model: "deepseek-v4-1-flash-1017".into(),
            http_status: 503,
            stop_reason: "error".into(),
            wall_ms: 120,
            in_uncached: 16_000,
            cache_read: 0,
            out: 0,
            cost_usd: 0.006,
            system_text: "line1\nline2_changed".into(),
            tools_text: r#"[{"name":"edit"},{"name":"read"}]"#.into(),
            history_entry_hashes: vec!["h1".into(), "h2".into()],
            now_secs_override: Some(1_700_000_020),
            ..Default::default()
        });
        assert_eq!(r3.cache_bust_reason, "tools_changed");
        assert!(r3.anomalies.contains(&"RETRY_BURN".to_string()));
        assert!(r3.anomalies.contains(&"UNCACHED_SPIKE".to_string()));
        assert!(dir.join("cache-bust-sess1-req3.diff").exists());

        let records = read_usage_log(&log_path).unwrap();
        assert_eq!(records.len(), 3);
        let report = format_usage_report(&records);
        assert!(report.contains("By source:"));
        assert!(report.contains("retry"));
        assert!(report.contains("system_changed"));
        assert!(report.contains("tools_changed"));
        assert!(report.contains("RETRY_BURN"));
        assert!(report.contains("Top 5 costliest requests:"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn classifies_history_rewritten_ttl_expired_and_provider_miss() {
        let dir = temp_dir("classify");
        let logger = UsageLogger::with_debug_cache("s2", dir.join("u.jsonl"), false);

        logger.record_request(UsageEventInput {
            turn_idx: 1,
            source: "main".into(),
            in_uncached: 500,
            cache_read: 4_500, // 90% hit
            system_text: "sys".into(),
            tools_text: "tools".into(),
            history_entry_hashes: vec!["a".into(), "b".into()],
            now_secs_override: Some(1_000),
            ..Default::default()
        });

        // Rewriting an earlier history entry without compaction -> history_rewritten.
        let rw = logger.record_request(UsageEventInput {
            turn_idx: 2,
            source: "main".into(),
            in_uncached: 500,
            cache_read: 4_500,
            system_text: "sys".into(),
            tools_text: "tools".into(),
            history_entry_hashes: vec!["a_mutated".into(), "b".into(), "c".into()],
            now_secs_override: Some(1_010),
            ..Default::default()
        });
        assert_eq!(rw.cache_bust_reason, "history_rewritten");

        // Long idle gap (>280s) + cache hit drop from >80% to <20% -> ttl_expired.
        let ttl = logger.record_request(UsageEventInput {
            turn_idx: 3,
            source: "main".into(),
            in_uncached: 5_000,
            cache_read: 0,
            system_text: "sys".into(),
            tools_text: "tools".into(),
            history_entry_hashes: vec!["a_mutated".into(), "b".into(), "c".into(), "d".into()],
            now_secs_override: Some(1_400),
            ..Default::default()
        });
        assert_eq!(ttl.cache_bust_reason, "ttl_expired");

        // Short idle gap (<60s) + identical prefix + <20% hit -> provider_or_wire_miss.
        let miss = logger.record_request(UsageEventInput {
            turn_idx: 4,
            source: "main".into(),
            in_uncached: 5_000,
            cache_read: 0,
            system_text: "sys".into(),
            tools_text: "tools".into(),
            history_entry_hashes: vec![
                "a_mutated".into(),
                "b".into(),
                "c".into(),
                "d".into(),
                "e".into(),
            ],
            now_secs_override: Some(1_415),
            ..Default::default()
        });
        assert_eq!(miss.cache_bust_reason, "provider_or_wire_miss");

        let _ = fs::remove_dir_all(&dir);
    }
}
