//! Built-in Harness2 usage self-improver (`/optimize`, `lazybug optimize`).
//!
//! Scans recent `~/.lazybug/logs/usage-*.jsonl` logs with 0 LLM tokens, computes session-level
//! waste metrics, proposes bounded positively-phrased rules into `~/.lazybug/LEARNED_USAGE.md`,
//! and runs a `[candidate] -> [proven] / [stale]` lifecycle in `~/.lazybug/optimize-state.json`
//! so prompt rot never accumulates.

use lazybug_telemetry::usage::{default_logs_dir, read_usage_log};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RULES: usize = 8;
const MAX_WORDS: usize = 120;
const DEFAULT_SCAN_SESSIONS: usize = 20;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageMetrics {
    pub sessions_scanned: usize,
    pub total_requests: usize,
    pub avg_cache_hit_pct: f64,
    pub repeat_tool_calls_per_session: f64,
    pub retry_burn_reqs: usize,
    pub uncached_spikes_per_session: f64,
    pub budget_stops: usize,
    pub avg_reqs_per_turn: f64,
    pub repeat_sessions: usize,
    pub spike_sessions: usize,
    pub retry_sessions: usize,
    pub budget_sessions: usize,
    pub cache_busts: BTreeMap<String, usize>,
}

impl UsageMetrics {
    pub fn value_for(&self, key: &str) -> f64 {
        match key {
            "repeat_tool_calls_per_session" => self.repeat_tool_calls_per_session,
            "uncached_spikes_per_session" => self.uncached_spikes_per_session,
            "retry_burn_reqs" => self.retry_burn_reqs as f64,
            "budget_stops" => self.budget_stops as f64,
            "avg_reqs_per_turn" => self.avg_reqs_per_turn,
            "cache_miss_pct" => (100.0 - self.avg_cache_hit_pct).max(0.0),
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleStatus {
    Candidate,
    Proven,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackedRule {
    pub id: String,
    pub text: String,
    pub metric_key: String,
    pub baseline_value: f64,
    pub sessions_observed: usize,
    pub last_seen_sessions: usize,
    pub status: RuleStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OptimizeState {
    pub total_sessions_seen: usize,
    pub rules: Vec<TrackedRule>,
}

#[derive(Debug, Clone)]
pub struct OptimizePaths {
    pub logs_dir: PathBuf,
    pub learned_usage_path: PathBuf,
    pub state_path: PathBuf,
    pub history_dir: PathBuf,
}

impl OptimizePaths {
    pub fn default_user() -> Self {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".to_string());
        let base = PathBuf::from(home).join(".lazybug");
        Self {
            logs_dir: default_logs_dir(),
            learned_usage_path: base.join("LEARNED_USAGE.md"),
            state_path: base.join("optimize-state.json"),
            history_dir: base.join("history"),
        }
    }
}

pub fn scan_usage_metrics(logs_dir: &Path, limit: usize) -> UsageMetrics {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_usage = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("usage-") && n.ends_with(".jsonl"));
            if !is_usage || !path.is_file() {
                continue;
            }
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            files.push((mtime, path));
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    files.truncate(limit.max(1));

    let mut m = UsageMetrics::default();
    let mut total_in_uncached: u64 = 0;
    let mut total_cache_read: u64 = 0;
    let mut total_repeats: usize = 0;
    let mut total_spikes: usize = 0;
    let mut total_turns: usize = 0;

    for (_, path) in files {
        let Ok(records) = read_usage_log(&path) else {
            continue;
        };
        if records.is_empty() {
            continue;
        }
        m.sessions_scanned += 1;
        m.total_requests += records.len();

        let mut s_repeats = 0usize;
        let mut s_spikes = 0usize;
        let mut s_retries = 0usize;
        let mut s_budgets = 0usize;
        let mut turn_ids: HashSet<usize> = HashSet::new();

        for r in &records {
            total_in_uncached = total_in_uncached.saturating_add(r.in_uncached);
            total_cache_read = total_cache_read.saturating_add(r.cache_read);
            if r.turn_idx > 0 {
                turn_ids.insert(r.turn_idx);
            }
            if r.cache_bust_reason != "none" {
                *m.cache_busts.entry(r.cache_bust_reason.clone()).or_insert(0) += 1;
            }
            if r.source == "retry" || r.anomalies.iter().any(|a| a == "RETRY_BURN") {
                s_retries += 1;
            }
            if r.anomalies.iter().any(|a| a == "REPEAT_TOOL_CALL") {
                s_repeats += 1;
            }
            if r.anomalies.iter().any(|a| a == "UNCACHED_SPIKE") {
                s_spikes += 1;
            }
            if r.anomalies.iter().any(|a| a == "BUDGET_STOP") || r.stop_reason == "budget_limit" {
                s_budgets += 1;
            }
        }
        total_turns += turn_ids.len().max(1);
        total_repeats += s_repeats;
        total_spikes += s_spikes;
        m.retry_burn_reqs += s_retries;
        m.budget_stops += s_budgets;
        if s_repeats > 0 {
            m.repeat_sessions += 1;
        }
        if s_spikes > 0 {
            m.spike_sessions += 1;
        }
        if s_retries > 0 {
            m.retry_sessions += 1;
        }
        if s_budgets > 0 {
            m.budget_sessions += 1;
        }
    }

    let total_in = total_in_uncached.saturating_add(total_cache_read);
    if total_in > 0 {
        m.avg_cache_hit_pct = (total_cache_read as f64) * 100.0 / (total_in as f64);
    }
    if m.sessions_scanned > 0 {
        let denom = m.sessions_scanned as f64;
        m.repeat_tool_calls_per_session = total_repeats as f64 / denom;
        m.uncached_spikes_per_session = total_spikes as f64 / denom;
    }
    if total_turns > 0 {
        m.avg_reqs_per_turn = m.total_requests as f64 / total_turns as f64;
    }
    m
}

struct RuleTemplate {
    id: &'static str,
    metric_key: &'static str,
    text: &'static str,
}

const TEMPLATES: &[RuleTemplate] = &[
    RuleTemplate {
        id: "repeat_tool_call",
        metric_key: "repeat_tool_calls_per_session",
        text: "Reuse the previous tool output already in context and pick a different action when a command or read repeats.",
    },
    RuleTemplate {
        id: "uncached_spike",
        metric_key: "uncached_spikes_per_session",
        text: "Keep file reads and shell outputs tightly scoped with offset/limit or grep filters so context stays compact.",
    },
    RuleTemplate {
        id: "budget_stop",
        metric_key: "budget_stops",
        text: "Batch independent reads in one turn and commit to one minimal edit path so tasks finish well inside the step budget.",
    },
    RuleTemplate {
        id: "high_reqs_per_turn",
        metric_key: "avg_reqs_per_turn",
        text: "Combine related lookups into parallel tool calls per turn instead of stepping one small inspection at a time.",
    },
    RuleTemplate {
        id: "cache_bust_history",
        metric_key: "cache_miss_pct",
        text: "Keep earlier turns append-only and avoid switching modes mid-task so the prefix cache stays warm.",
    },
];

pub fn propose_candidate_rules(metrics: &UsageMetrics) -> Vec<TrackedRule> {
    let mut out = Vec::new();
    if metrics.repeat_sessions >= 2 && metrics.repeat_tool_calls_per_session >= 0.5 {
        out.push(make_candidate(&TEMPLATES[0], metrics));
    }
    if metrics.spike_sessions >= 2 && metrics.uncached_spikes_per_session >= 0.5 {
        out.push(make_candidate(&TEMPLATES[1], metrics));
    }
    if metrics.budget_sessions >= 2 && metrics.budget_stops >= 2 {
        out.push(make_candidate(&TEMPLATES[2], metrics));
    }
    if metrics.sessions_scanned >= 2 && metrics.avg_reqs_per_turn >= 3.5 {
        out.push(make_candidate(&TEMPLATES[3], metrics));
    }
    if metrics.sessions_scanned >= 2
        && (metrics.cache_busts.get("history_rewritten").copied().unwrap_or(0) >= 2
            || metrics.cache_busts.get("system_changed").copied().unwrap_or(0) >= 2)
    {
        out.push(make_candidate(&TEMPLATES[4], metrics));
    }
    out
}

fn make_candidate(tpl: &RuleTemplate, metrics: &UsageMetrics) -> TrackedRule {
    TrackedRule {
        id: tpl.id.to_string(),
        text: tpl.text.to_string(),
        metric_key: tpl.metric_key.to_string(),
        baseline_value: metrics.value_for(tpl.metric_key),
        sessions_observed: 0,
        last_seen_sessions: metrics.sessions_scanned,
        status: RuleStatus::Candidate,
    }
}

/// Update Harness2 lifecycle (`candidate` -> `proven` / `stale`) using the current metrics.
pub fn advance_lifecycle(
    state: &mut OptimizeState,
    metrics: &UsageMetrics,
    session_delta: usize,
) -> (Vec<String>, Vec<String>) {
    let mut promoted = Vec::new();
    let mut evicted = Vec::new();

    for rule in &mut state.rules {
        if rule.status != RuleStatus::Candidate {
            continue;
        }
        rule.sessions_observed = rule.sessions_observed.saturating_add(session_delta);
        let current = metrics.value_for(&rule.metric_key);
        let improved_15pct = if rule.baseline_value > 0.0 {
            current <= rule.baseline_value * 0.85
        } else {
            current == 0.0
        };
        if rule.sessions_observed >= 5 && improved_15pct {
            rule.status = RuleStatus::Proven;
            promoted.push(rule.id.clone());
        } else if rule.sessions_observed >= 10 && !improved_15pct {
            rule.status = RuleStatus::Stale;
            evicted.push(rule.id.clone());
        }
    }

    // Remove evicted stale rules from active list so prompt rot never accumulates,
    // keeping a small record only if needed? Spec says auto-evict as stale.
    state.rules.retain(|r| r.status != RuleStatus::Stale);

    for cand in propose_candidate_rules(metrics) {
        if !state.rules.iter().any(|r| r.id == cand.id) {
            state.rules.push(cand);
        }
    }
    state.total_sessions_seen = state.total_sessions_seen.saturating_add(session_delta);
    (promoted, evicted)
}

pub fn config_recommendations(metrics: &UsageMetrics) -> Vec<String> {
    let mut recs = Vec::new();
    if metrics.retry_sessions >= 2 || metrics.retry_burn_reqs >= 3 {
        recs.push(format!(
            "High retry burn ({} retries across {} session(s)): consider lowering `retry_max` or checking provider rate limits.",
            metrics.retry_burn_reqs, metrics.retry_sessions
        ));
    }
    if metrics.budget_sessions >= 2 {
        recs.push(
            "Multiple `BUDGET_STOP` events: tighten `max_prompt_requests` / `max_prompt_uncached_tokens` or lower `thinking` in `settings.json`."
                .to_string(),
        );
    }
    if metrics.sessions_scanned >= 2 && metrics.avg_cache_hit_pct < 60.0 {
        recs.push(format!(
            "Low average cache hit ({:.1}%): pin `tools` in `settings.json` and avoid `LAZYBUG_NO_EXTENSIONS=0` bloat.",
            metrics.avg_cache_hit_pct
        ));
    }
    recs
}

/// Enforce `<= 8` bullets, `<= 120` words, positive phrasing, and deduplication.
pub fn render_learned_usage_markdown(rules: &[TrackedRule]) -> String {
    let active: Vec<&TrackedRule> = rules
        .iter()
        .filter(|r| r.status != RuleStatus::Stale)
        .collect();
    if active.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    let mut word_count = 0usize;
    let mut seen = HashSet::new();
    for rule in active {
        if lines.len() >= MAX_RULES {
            break;
        }
        let clean = sanitize_positive_rule(&rule.text);
        if clean.is_empty() || !seen.insert(clean.to_ascii_lowercase()) {
            continue;
        }
        let tag = match rule.status {
            RuleStatus::Proven => "proven",
            RuleStatus::Candidate => "candidate",
            RuleStatus::Stale => continue,
        };
        let bullet = format!("- [{tag}] {clean}");
        let w = bullet.split_whitespace().count();
        if word_count + w > MAX_WORDS {
            break;
        }
        word_count += w;
        lines.push(bullet);
    }
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}

fn sanitize_positive_rule(text: &str) -> String {
    let trimmed = text.trim();
    // Guardrail: reject negative phrasing if any ever slips in.
    for banned in ["Don't", "Do not", "Never", "Avoid"] {
        if trimmed.contains(banned) {
            return String::new();
        }
    }
    trimmed.to_string()
}

pub fn load_state(path: &Path) -> OptimizeState {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_state(path: &Path, state: &OptimizeState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create_dir_all failed: {e}"))?;
    }
    let body = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(path, body + "\n").map_err(|e| format!("write state failed: {e}"))
}

pub fn run_optimize_dry_run(paths: &OptimizePaths) -> String {
    let metrics = scan_usage_metrics(&paths.logs_dir, DEFAULT_SCAN_SESSIONS);
    let mut state = load_state(&paths.state_path);
    let (promoted, evicted) = advance_lifecycle(&mut state, &metrics, 0);
    let rendered = render_learned_usage_markdown(&state.rules);
    let recs = config_recommendations(&metrics);
    format_optimize_output(&metrics, &rendered, &recs, &promoted, &evicted, false)
}

pub fn run_optimize_apply(paths: &OptimizePaths) -> Result<String, String> {
    let metrics = scan_usage_metrics(&paths.logs_dir, DEFAULT_SCAN_SESSIONS);
    let mut state = load_state(&paths.state_path);
    let delta = metrics.sessions_scanned.max(1);
    let (promoted, evicted) = advance_lifecycle(&mut state, &metrics, delta);
    let rendered = render_learned_usage_markdown(&state.rules);

    if paths.learned_usage_path.is_file() {
        snapshot_learned_usage(&paths.learned_usage_path, &paths.history_dir)?;
    }
    if let Some(parent) = paths.learned_usage_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create_dir_all failed: {e}"))?;
    }
    fs::write(&paths.learned_usage_path, &rendered)
        .map_err(|e| format!("write LEARNED_USAGE.md failed: {e}"))?;
    save_state(&paths.state_path, &state)?;

    let recs = config_recommendations(&metrics);
    Ok(format_optimize_output(
        &metrics, &rendered, &recs, &promoted, &evicted, true,
    ))
}

pub fn run_optimize_rollback(paths: &OptimizePaths) -> Result<String, String> {
    let mut snapshots: Vec<PathBuf> = fs::read_dir(&paths.history_dir)
        .map_err(|_| "No optimize history snapshots found.".to_string())?
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("learned-usage-") && n.ends_with(".md"))
        })
        .collect();
    snapshots.sort();
    let latest = snapshots
        .pop()
        .ok_or_else(|| "No learned-usage snapshot found to roll back to.".to_string())?;
    let bytes = fs::read(&latest).map_err(|e| format!("read snapshot failed: {e}"))?;
    if let Some(parent) = paths.learned_usage_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create_dir_all failed: {e}"))?;
    }
    fs::write(&paths.learned_usage_path, bytes)
        .map_err(|e| format!("restore LEARNED_USAGE.md failed: {e}"))?;
    Ok(format!(
        "Rolled back {} from snapshot {}.",
        paths.learned_usage_path.display(),
        latest.display()
    ))
}

fn snapshot_learned_usage(src: &Path, history_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(history_dir).map_err(|e| format!("create history dir failed: {e}"))?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dst = history_dir.join(format!("learned-usage-{ts}.md"));
    fs::copy(src, &dst).map_err(|e| format!("snapshot copy failed: {e}"))?;
    Ok(dst)
}

fn format_optimize_output(
    m: &UsageMetrics,
    rendered_rules: &str,
    recs: &[String],
    promoted: &[String],
    evicted: &[String],
    applied: bool,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Harness2 Usage Optimizer ({} sessions, {} requests)\n",
        m.sessions_scanned, m.total_requests
    ));
    out.push_str(&format!(
        "Metrics: cache_hit={:.1}%  repeats/session={:.2}  spikes/session={:.2}  retries={}  budget_stops={}  reqs/turn={:.2}\n",
        m.avg_cache_hit_pct,
        m.repeat_tool_calls_per_session,
        m.uncached_spikes_per_session,
        m.retry_burn_reqs,
        m.budget_stops,
        m.avg_reqs_per_turn,
    ));
    if !promoted.is_empty() {
        out.push_str(&format!("Promoted to [proven]: {}\n", promoted.join(", ")));
    }
    if !evicted.is_empty() {
        out.push_str(&format!("Evicted as [stale]: {}\n", evicted.join(", ")));
    }
    if rendered_rules.trim().is_empty() {
        out.push_str("\nPrompt rules: (none proposed)\n");
    } else {
        out.push_str(&format!(
            "\nPrompt rules ({}):\n{}",
            if applied { "written to LEARNED_USAGE.md" } else { "dry-run preview" },
            rendered_rules
        ));
    }
    if !recs.is_empty() {
        out.push_str("\nConfig recommendations (read-only):\n");
        for r in recs {
            out.push_str(&format!("  - {r}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazybug_telemetry::usage::{PrefixHashes, UsageLogRecord};

    fn temp_paths(tag: &str) -> OptimizePaths {
        let root = std::env::temp_dir().join(format!("lazybug_opt_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        OptimizePaths {
            logs_dir: root.join("logs"),
            learned_usage_path: root.join("LEARNED_USAGE.md"),
            state_path: root.join("optimize-state.json"),
            history_dir: root.join("history"),
        }
    }

    fn write_usage_session(logs_dir: &Path, name: &str, recs: Vec<UsageLogRecord>) {
        fs::create_dir_all(logs_dir).unwrap();
        let mut body = String::new();
        for r in recs {
            body.push_str(&serde_json::to_string(&r).unwrap());
            body.push('\n');
        }
        fs::write(logs_dir.join(name), body).unwrap();
    }

    fn rec(turn_idx: usize, in_uncached: u64, cache_read: u64, anomalies: &[&str]) -> UsageLogRecord {
        UsageLogRecord {
            ts: "2026-11-29T00:00:00Z".into(),
            session_id: "s".into(),
            turn_idx,
            req_idx: turn_idx,
            source: "main".into(),
            provider: "vibe".into(),
            model: "deepseek-v4-1-flash-1017".into(),
            http_status: 200,
            stop_reason: "stop".into(),
            wall_ms: 100,
            idle_since_prev_req_s: 5,
            in_uncached,
            cache_read,
            cache_write: 0,
            out: 50,
            reasoning_tokens: 0,
            cache_hit_pct: 80.0,
            cost_usd: 0.001,
            prefix: PrefixHashes::default(),
            cache_bust_reason: "none".into(),
            anomalies: anomalies.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn recurring_anomalies_propose_bounded_positive_rules() {
        let paths = temp_paths("propose");
        for i in 1..=3 {
            write_usage_session(
                &paths.logs_dir,
                &format!("usage-s{i}.jsonl"),
                vec![
                    rec(1, 20_000, 1_000, &["UNCACHED_SPIKE", "REPEAT_TOOL_CALL"]),
                    rec(2, 500, 5_000, &["BUDGET_STOP"]),
                ],
            );
        }
        let metrics = scan_usage_metrics(&paths.logs_dir, 20);
        let rules = propose_candidate_rules(&metrics);
        assert!(rules.len() >= 3);
        let md = render_learned_usage_markdown(&rules);
        let bullet_count = md.lines().filter(|l| l.starts_with("- ")).count();
        let word_count = md.split_whitespace().count();
        assert!(bullet_count <= MAX_RULES, "bullets={bullet_count}");
        assert!(word_count <= MAX_WORDS, "words={word_count}");
        for banned in ["Don't", "Do not", "Never", "Avoid"] {
            assert!(!md.contains(banned), "found banned {banned}");
        }
        let _ = fs::remove_dir_all(paths.learned_usage_path.parent().unwrap());
    }

    #[test]
    fn candidates_promote_on_improvement_and_evict_as_stale_without_improvement() {
        let mut state = OptimizeState {
            total_sessions_seen: 2,
            rules: vec![
                TrackedRule {
                    id: "repeat_tool_call".into(),
                    text: "Reuse prior output.".into(),
                    metric_key: "repeat_tool_calls_per_session".into(),
                    baseline_value: 2.0,
                    sessions_observed: 4,
                    last_seen_sessions: 2,
                    status: RuleStatus::Candidate,
                },
                TrackedRule {
                    id: "uncached_spike".into(),
                    text: "Keep reads scoped.".into(),
                    metric_key: "uncached_spikes_per_session".into(),
                    baseline_value: 2.0,
                    sessions_observed: 9,
                    last_seen_sessions: 2,
                    status: RuleStatus::Candidate,
                },
            ],
        };
        let improved = UsageMetrics {
            sessions_scanned: 1,
            repeat_tool_calls_per_session: 1.0, // -50% (>= 15% improvement) -> promote at 5
            uncached_spikes_per_session: 2.0,   // 0% improvement at 10 -> evict as stale
            ..Default::default()
        };
        let (promoted, evicted) = advance_lifecycle(&mut state, &improved, 1);
        assert_eq!(promoted, vec!["repeat_tool_call"]);
        assert_eq!(evicted, vec!["uncached_spike"]);
        assert_eq!(state.rules.len(), 1);
        assert_eq!(state.rules[0].status, RuleStatus::Proven);
    }

    #[test]
    fn apply_snapshots_prior_learned_usage_and_rollback_restores_byte_for_byte() {
        let paths = temp_paths("rollback");
        let root = paths.learned_usage_path.parent().unwrap().to_path_buf();
        let append_sys = root.join("APPEND_SYSTEM.md");
        let sys_md = root.join("SYSTEM.md");
        fs::write(&append_sys, "USER_APPEND_UNTOUCHED\n").unwrap();
        fs::write(&sys_md, "USER_SYSTEM_UNTOUCHED\n").unwrap();
        let original_learned = "- [proven] Prior custom learned rule.\n";
        fs::write(&paths.learned_usage_path, original_learned).unwrap();

        for i in 1..=2 {
            write_usage_session(
                &paths.logs_dir,
                &format!("usage-r{i}.jsonl"),
                vec![rec(1, 18_000, 1_000, &["REPEAT_TOOL_CALL"])],
            );
        }

        let out = run_optimize_apply(&paths).expect("apply ok");
        assert!(out.contains("written to LEARNED_USAGE.md"));
        let new_body = fs::read_to_string(&paths.learned_usage_path).unwrap();
        assert_ne!(new_body, original_learned);

        let rb = run_optimize_rollback(&paths).expect("rollback ok");
        assert!(rb.contains("Rolled back"));
        assert_eq!(
            fs::read_to_string(&paths.learned_usage_path).unwrap(),
            original_learned
        );
        // Never touches user-owned SYSTEM.md or APPEND_SYSTEM.md.
        assert_eq!(fs::read_to_string(&append_sys).unwrap(), "USER_APPEND_UNTOUCHED\n");
        assert_eq!(fs::read_to_string(&sys_md).unwrap(), "USER_SYSTEM_UNTOUCHED\n");

        let _ = fs::remove_dir_all(&root);
    }
}
