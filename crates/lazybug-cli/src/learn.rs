//! Zero-LLM session miner (`/learn`): analyzes recent session JSONL files for repeated tool
//! friction, per-model error rates, user interrupts, and Headroom compression efficiency, then
//! deduplicates candidate lessons against `MemoryRagStore`. Read-only by design.

use lazybug_tools::memory::MemoryRagStore;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Default)]
pub(crate) struct ModelStats {
    pub user_turns: usize,
    pub tool_results: usize,
    pub tool_errors: usize,
    pub harness_misuse: usize,
    pub interrupts: usize,
    pub max_turn_hits: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct LearnCandidate {
    pub key: String,
    pub scope: &'static str,
    pub title: String,
    pub body: String,
    pub evidence: String,
    pub already_known: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LearnReport {
    pub sessions_scanned: usize,
    pub entries_scanned: usize,
    pub total_user_turns: usize,
    pub total_tool_results: usize,
    pub total_tool_errors: usize,
    pub total_interrupts: usize,
    pub total_max_turn_hits: usize,
    pub compressed_outputs: usize,
    pub est_tokens_saved: usize,
    pub retrieve_calls: usize,
    pub models: BTreeMap<String, ModelStats>,
    pub tool_friction: BTreeMap<(String, String), usize>,
    pub top_bloated_tools: BTreeMap<String, usize>,
    pub candidates: Vec<LearnCandidate>,
    /// Rules already present in MEMORY.md whose friction still recurs (>=3x): refine or retire.
    pub stale_rules: Vec<(String, usize)>,
}

/// Normalize a model spec like `vibe/mistral-large-4-0:medium` -> `vibe/mistral-large-4-0`.
fn normalize_model_spec(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    trimmed.split(':').next().unwrap_or(trimmed).to_string()
}

/// Extract `[sub_tool=<name>]` when a codemode script surfaces a sub-tool error.
fn extract_sub_tool(content: &str) -> Option<&str> {
    let start = content.find("[sub_tool=")?;
    let rest = &content[start + 10..];
    let end = rest.find(']')?;
    let name = rest[..end].trim();
    (!name.is_empty()).then_some(name)
}

/// Classify a tool error string into a stable friction bucket. Returns `None` for normal
/// compiler/test iteration noise so `/learn` focuses on repeatable harness/model mistakes.
fn classify_tool_friction(tool: &str, content: &str) -> Option<&'static str> {
    let lower = content.to_ascii_lowercase();
    if lower.contains("read ") && lower.contains("before editing") {
        return Some("edit_without_read");
    }
    if lower.contains("old_text") || lower.contains("oldtext") || lower.contains("not found in") {
        return Some("edit_oldtext_mismatch");
    }
    if lower.contains("policy denied:") {
        return Some("policy_denied");
    }
    if lower.contains("unknown tool") {
        return Some("unknown_tool_name");
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return Some("tool_timeout");
    }
    if tool == "codemode" && (lower.contains("traceback") || lower.contains("error")) {
        return Some("codemode_script_error");
    }
    if lower.starts_with("tool error:") {
        return Some("tool_arg_error");
    }
    None
}

/// Parse `(~1234 tok saved)` from a Headroom-lite compressed marker if present.
fn extract_saved_tokens(content: &str) -> Option<usize> {
    let start = content.find("(~")?;
    let rest = &content[start + 2..];
    let end = rest.find(" tok saved)")?;
    rest[..end].trim().parse::<usize>().ok()
}

fn collect_recent_sessions(sessions_root: &Path, max_sessions: usize) -> Vec<PathBuf> {
    let mut files: Vec<(u64, PathBuf)> = Vec::new();
    let mut scan_dir = |dir: &Path| {
        if let Ok(entries) = fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                    let mtime = e
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    files.push((mtime, p));
                }
            }
        }
    };
    scan_dir(sessions_root);
    if let Ok(dirs) = fs::read_dir(sessions_root) {
        for d in dirs.flatten() {
            if d.path().is_dir() {
                scan_dir(&d.path());
            }
        }
    }
    files.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    files.truncate(max_sessions);
    files.into_iter().map(|(_, p)| p).collect()
}

/// Mine up to `max_sessions` recent session files and deduplicate candidate lessons against `rag`.
pub(crate) fn mine_sessions(
    sessions_root: &Path,
    rag: &MemoryRagStore,
    max_sessions: usize,
) -> LearnReport {
    let mut report = LearnReport::default();
    let session_files = collect_recent_sessions(sessions_root, max_sessions);
    report.sessions_scanned = session_files.len();

    for path in &session_files {
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };
        let mut active_model = "unknown".to_string();

        for line in raw.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            report.entries_scanned += 1;
            let kind = v.get("kind").and_then(|x| x.as_str()).unwrap_or("message");
            let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("");
            let content = v.get("content").and_then(|x| x.as_str()).unwrap_or("");

            if kind == "model_change" {
                active_model = normalize_model_spec(content);
                continue;
            }

            let mstats = report.models.entry(active_model.clone()).or_default();

            if kind == "message" && role == "user" {
                report.total_user_turns += 1;
                mstats.user_turns += 1;
            }

            if content.contains("[Interrupted by user]") || content.contains("[Turn interrupted:") {
                report.total_interrupts += 1;
                mstats.interrupts += 1;
            }

            if content.contains("max turns exceeded") || content.contains("Reached turn budget") {
                report.total_max_turn_hits += 1;
                mstats.max_turn_hits += 1;
            }

            if kind == "tool_result" {
                report.total_tool_results += 1;
                mstats.tool_results += 1;
                let tool = v
                    .get("tool_name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown");

                if tool == "retrieve_output" {
                    report.retrieve_calls += 1;
                }
                if let Some(saved) = extract_saved_tokens(content) {
                    report.compressed_outputs += 1;
                    report.est_tokens_saved += saved;
                }
                if content.len() >= 8 * 1024 {
                    *report.top_bloated_tools.entry(tool.to_string()).or_insert(0) += content.len();
                }

                let is_err = content.starts_with("Tool error:")
                    || content.starts_with("Policy denied:")
                    || content.contains("Traceback (most recent call last)");
                if is_err {
                    report.total_tool_errors += 1;
                    mstats.tool_errors += 1;
                    let effective_tool = if tool == "codemode" {
                        extract_sub_tool(content).unwrap_or(tool)
                    } else {
                        tool
                    };
                    if let Some(bucket) = classify_tool_friction(effective_tool, content) {
                        mstats.harness_misuse += 1;
                        *report
                            .tool_friction
                            .entry((effective_tool.to_string(), bucket.to_string()))
                            .or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // Build candidate lessons from repeated patterns (count >= 2) and dedup against MemoryRagStore.
    let mut raw_candidates: Vec<(String, String, String, usize)> = Vec::new();
    for ((tool, bucket), count) in &report.tool_friction {
        if *count < 2 {
            continue;
        }
        let (title, body) = match bucket.as_str() {
            "edit_without_read" => (
                "Always read target file before edit".to_string(),
                format!(
                    "Observed {count} `edit` denials due to require_read_before_edit. Call `read` on target path before `edit`."
                ),
            ),
            "edit_oldtext_mismatch" => (
                "Keep edit oldText exact and compact".to_string(),
                format!(
                    "Observed {count} `edit` oldText mismatches. Re-read exact lines with offset/limit before replacing."
                ),
            ),
            "codemode_script_error" => (
                "Validate codemode scripts cleanly".to_string(),
                format!(
                    "Observed {count} codemode script errors. Keep scripts short, check key names, and filter before printing."
                ),
            ),
            "policy_denied" => (
                format!("Policy friction on `{tool}`"),
                format!(
                    "Observed {count} policy denials on `{tool}`. Review `~/.lazybug/policy.json` allowlist or adjust tool usage."
                ),
            ),
            "tool_timeout" => (
                format!("Cap slow `{tool}` commands"),
                format!(
                    "Observed {count} timeouts on `{tool}`. Scope searches/commands and cap output early."
                ),
            ),
            _ => (
                format!("Repeated `{tool}` friction ({bucket})"),
                format!(
                    "Observed {count} repeated `{bucket}` errors on `{tool}` across recent sessions."
                ),
            ),
        };
        raw_candidates.push((format!("learn:{tool}:{bucket}"), title, body, *count));
    }

    for (model, stats) in &report.models {
        if model == "unknown" || stats.tool_results < 5 {
            continue;
        }
        let err_pct = (stats.harness_misuse as f64 / stats.tool_results as f64) * 100.0;
        if stats.harness_misuse >= 3 && err_pct >= 15.0 {
            raw_candidates.push((
                format!("model:{model}"),
                format!("Model profile hint for {model}"),
                format!(
                    "{model} hit {}/{:} harness-misuse tool errors ({err_pct:.0}%). Prefer targeted reads and small surgical tool calls.",
                    stats.harness_misuse, stats.tool_results
                ),
                stats.harness_misuse,
            ));
        }
    }

    raw_candidates.sort_by_key(|(_, _, _, c)| std::cmp::Reverse(*c));
    for (key, title, body, count) in raw_candidates.into_iter().take(8) {
        let hits = rag.search(&format!("{title} {body}"), 3, "memory");
        let already_known = hits.iter().any(|h| {
            h.title.eq_ignore_ascii_case(&title)
                || h.body.contains(&format!("[key:{key}]"))
                || h.score >= 0.72
        });
        // Harness2 split: policy/repo-specific friction is "domain"; general tool/model habits are "invariant".
        let scope = if key.contains("policy_denied") || key.contains("tool_timeout") {
            "domain"
        } else {
            "invariant"
        };
        let evidence = format!("{count}x across {} session(s)", report.sessions_scanned);
        if already_known && count >= 3 {
            report.stale_rules.push((key.clone(), count));
        }
        report.candidates.push(LearnCandidate {
            key,
            scope,
            title,
            body,
            evidence,
            already_known,
        });
    }

    report
}

pub(crate) fn format_learn_report(r: &LearnReport) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Learn Report (0 LLM tokens · {} session(s), {} entries)",
        r.sessions_scanned, r.entries_scanned
    ));
    lines.push(format!(
        "  Turns: {} user · {} tool results ({} errors) · {} interrupts · {} turn-budget wraps",
        r.total_user_turns,
        r.total_tool_results,
        r.total_tool_errors,
        r.total_interrupts,
        r.total_max_turn_hits
    ));
    lines.push(format!(
        "  Headroom: {} output(s) crushed, ~{} tok saved ({} retrieve_output calls)",
        r.compressed_outputs, r.est_tokens_saved, r.retrieve_calls
    ));

    if !r.models.is_empty() {
        lines.push("Per-Model Breakdown:".to_string());
        for (model, st) in &r.models {
            if st.user_turns == 0 && st.tool_results == 0 {
                continue;
            }
            let err_rate = if st.tool_results > 0 {
                (st.tool_errors as f64 / st.tool_results as f64) * 100.0
            } else {
                0.0
            };
            lines.push(format!(
                "  {model}: {} turns, {} tools ({} err / {err_rate:.0}%), {} misuse, {} interrupts",
                st.user_turns, st.tool_results, st.tool_errors, st.harness_misuse, st.interrupts
            ));
        }
    }

    if !r.tool_friction.is_empty() {
        lines.push("Repeated Tool Friction:".to_string());
        for ((tool, bucket), count) in &r.tool_friction {
            lines.push(format!("  {tool} / {bucket}: {count}x"));
        }
    }

    if !r.top_bloated_tools.is_empty() {
        let mut bloated: Vec<(&String, &usize)> = r.top_bloated_tools.iter().collect();
        bloated.sort_by_key(|(_, b)| std::cmp::Reverse(**b));
        let summary: Vec<String> = bloated
            .into_iter()
            .take(5)
            .map(|(t, b)| format!("{t}={}KB", b / 1024))
            .collect();
        lines.push(format!("Largest Tool Outputs (>=8KB): {}", summary.join(", ")));
    }

    if r.candidates.is_empty() {
        lines.push("Candidate Lessons: none (sessions look clean).".to_string());
    } else {
        let new_cnt = r.candidates.iter().filter(|c| !c.already_known).count();
        lines.push(format!(
            "Candidate Lessons ({new_cnt} new / {} total, deduped vs MEMORY.md):",
            r.candidates.len()
        ));
        for c in &r.candidates {
            let tag = if c.already_known { "[known]" } else { "[new]  " };
            lines.push(format!(
                "  {tag} [{}] [{}] {} (evidence: {}) — {}",
                c.scope, c.key, c.title, c.evidence, c.body
            ));
        }
        if !r.stale_rules.is_empty() {
            lines.push("Stale / Ineffective Known Rules (still recurring >=3x — refine or retire):".to_string());
            for (key, count) in &r.stale_rules {
                lines.push(format!("  [stale] {key} ({count}x despite existing rule)"));
            }
        }
        lines.push("  Run /suggestion to curate and apply selected improvements.".to_string());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn mines_per_model_friction_headroom_stats_and_dedups_rag() {
        let tmp = std::env::temp_dir().join(format!(
            "lazybug_learn_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let sess_dir = tmp.join("sessions");
        let mem_dir = tmp.join("memory");
        fs::create_dir_all(&sess_dir).unwrap();
        fs::create_dir_all(&mem_dir).unwrap();

        let rag = MemoryRagStore::open(&mem_dir, &mem_dir);
        rag.write_memory(
            "global",
            "long_term",
            Some("Always read target file before edit"),
            "Call read before edit to satisfy require_read_before_edit.",
            vec!["edit".into(), "read".into()],
            Some("learn:edit:edit_without_read".to_string()),
        )
        .unwrap();

        // Build 20 synthetic sessions (2,000 entries total) for deterministic benchmark + dedup verification.
        let t_build = Instant::now();
        for s in 0..20 {
            let model = if s % 2 == 0 {
                "vibe/mistral-large-4-0:medium"
            } else {
                "agy/gemini-4-neon:high"
            };
            let mut lines = Vec::with_capacity(100);
            lines.push(format!(
                r#"{{"id":"e0","parent_id":null,"kind":"model_change","role":"system","content":"{model}"}}"#
            ));
            for i in 1..100 {
                let row = match i % 10 {
                    1 => r#"{"id":"u","parent_id":null,"kind":"message","role":"user","content":"fix bug"}"#,
                    2 => r#"{"id":"t1","parent_id":null,"kind":"tool_result","role":"tool","tool_name":"edit","content":"Policy denied: Read 'a.rs' first before editing (require_read_before_edit policy)"}"#,
                    3 if s % 2 == 0 => r#"{"id":"t2","parent_id":null,"kind":"tool_result","role":"tool","tool_name":"codemode","content":"Tool error: Traceback (most recent call last) NameError"}"#,
                    4 => r#"{"id":"t3","parent_id":null,"kind":"tool_result","role":"tool","tool_name":"bash","content":"ok\n[compressed 40.0KB -> 2.0KB (~9500 tok saved), ref=out_1]"}"#,
                    5 => r#"{"id":"t4","parent_id":null,"kind":"tool_result","role":"tool","tool_name":"retrieve_output","content":"1: line"}"#,
                    _ => r#"{"id":"t5","parent_id":null,"kind":"tool_result","role":"tool","tool_name":"read","content":"fn main() {}"}"#,
                };
                lines.push(row.to_string());
            }
            fs::write(sess_dir.join(format!("s_{s:02}.jsonl")), lines.join("\n")).unwrap();
        }
        let _ = t_build;

        let t0 = Instant::now();
        let report = mine_sessions(&sess_dir, &rag, 20);
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

        println!(
            "\n=== PR B /learn Miner Benchmark ===\n  sessions: {}\n  entries : {}\n  time_ms : {:.2} ms (0 LLM tokens)\n  models  : {}\n  saved   : ~{} tok across {} crushed outputs ({} retrieves)\n  candidates: {} total ({} deduped as [known])",
            report.sessions_scanned,
            report.entries_scanned,
            elapsed_ms,
            report.models.len(),
            report.est_tokens_saved,
            report.compressed_outputs,
            report.retrieve_calls,
            report.candidates.len(),
            report.candidates.iter().filter(|c| c.already_known).count()
        );

        assert_eq!(report.sessions_scanned, 20);
        assert_eq!(report.entries_scanned, 2000);
        assert!(report.models.contains_key("vibe/mistral-large-4-0"));
        assert!(report.models.contains_key("agy/gemini-4-neon"));
        assert_eq!(report.compressed_outputs, 200);
        assert_eq!(report.est_tokens_saved, 200 * 9500);
        assert!(
            report
                .candidates
                .iter()
                .any(|c| c.key == "learn:edit:edit_without_read" && c.already_known && c.scope == "invariant"),
            "existing RAG lesson must be marked already_known"
        );
        assert!(
            report
                .stale_rules
                .iter()
                .any(|(k, cnt)| k == "learn:edit:edit_without_read" && *cnt >= 3),
            "recurring known lesson must be flagged as stale for refinement/retirement"
        );
        assert!(
            report
                .candidates
                .iter()
                .any(|c| c.key == "learn:codemode:codemode_script_error" && !c.already_known),
            "new codemode friction must surface as [new]"
        );

        let rendered = format_learn_report(&report);
        assert!(rendered.contains("Per-Model Breakdown:"));
        assert!(rendered.contains("Stale / Ineffective Known Rules"));
        assert!(rendered.contains("evidence:"));
        assert!(rendered.contains("vibe/mistral-large-4-0"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pr_c_model_profile_kv_cache_prefix_and_suggestion_digest_benchmark() {
        let tmp = std::env::temp_dir().join(format!(
            "lazybug_prc_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let sess_dir = tmp.join("sessions");
        let logs_dir = tmp.join("logs");
        let mem_dir = tmp.join("memory");
        fs::create_dir_all(&sess_dir).unwrap();
        fs::create_dir_all(&logs_dir).unwrap();
        fs::create_dir_all(&mem_dir).unwrap();

        let rag = MemoryRagStore::open(&mem_dir, &mem_dir);
        let long_note = "Prefer codemode for multi-file reads; keep edit oldText exact. ".repeat(15);
        rag.write_memory(
            "global",
            "long_term",
            Some("Model profile: vibe/mistral-large-4-0"),
            &long_note,
            vec!["model".into(), "vibe".into(), "mistral".into()],
            Some("model:vibe/mistral-large-4-0".to_string()),
        )
        .unwrap();

        let t0 = Instant::now();
        let fetched = rag
            .read_by_key("model:vibe/mistral-large-4-0")
            .expect("exact model key should resolve");
        let lookup_us = t0.elapsed().as_micros();

        let base_prompt = lazybug_tui::build_system_prompt(
            lazybug_tui::AgentMode::Normal,
            "powershell",
            "C:/repo",
            "C:/repo",
            &[],
            "",
        );
        let mut adjusted_prompt = base_prompt.clone();
        lazybug_tui::append_model_profile(
            &mut adjusted_prompt,
            "vibe/mistral-large-4-0",
            &fetched,
        );

        // KV-cache invariant: static prefix must be 100% byte-identical before <model_profile>.
        assert!(adjusted_prompt.starts_with(&base_prompt));
        let added_bytes = adjusted_prompt.len() - base_prompt.len();
        assert!(
            added_bytes <= 490,
            "model_profile block must stay capped near 400 bytes, got {added_bytes}"
        );
        assert!(adjusted_prompt.contains("<model_profile model=\"vibe/mistral-large-4-0\">"));

        // Verify /suggestion digest integrates <learn_report> + <rag_memory> + Model profile tier.
        let t1 = Instant::now();
        let digest = crate::features::build_optimization_digest(&sess_dir, &logs_dir, "C:/repo");
        let digest_ms = t1.elapsed().as_secs_f64() * 1000.0;
        assert!(digest.contains("<learn_report>"));
        assert!(digest.contains("<rag_memory>"));
        assert!(digest.contains("key=\"model:<provider>/<model>\""));

        // Verify codemode sub-tool attribution extracts `[sub_tool=edit]` properly.
        assert_eq!(
            extract_sub_tool("RuntimeError: [sub_tool=edit] Read 'a.rs' first before editing"),
            Some("edit")
        );
        assert_eq!(
            classify_tool_friction(
                "edit",
                "RuntimeError: [sub_tool=edit] Read 'a.rs' first before editing"
            ),
            Some("edit_without_read")
        );

        // End-to-end round-trip: real compress_tool_output -> SessionStore -> mine_sessions.
        let mut store = lazybug_agent::OutputStore::new();
        let noisy_log = (0..300)
            .map(|i| {
                if i == 150 {
                    "2026-11-23T12:00:00Z FATAL crash at main.rs:1".to_string()
                } else {
                    format!("2026-11-23T12:00:00Z INFO heartbeat #{i}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let crushed = lazybug_agent::compress_tool_output("bash", &noisy_log, &mut store);
        let rt_path = sess_dir.join("roundtrip.jsonl");
        let mut sess = lazybug_durable::SessionStore::open(&rt_path).unwrap();
        sess.append_model_change("vibe/mistral-large-4-0:medium")
            .unwrap();
        sess.append_tool_result("tc_1", "bash", &crushed).unwrap();
        let rt_report = mine_sessions(&sess_dir, &rag, 5);
        assert_eq!(rt_report.compressed_outputs, 1);
        assert!(rt_report.est_tokens_saved > 1000);

        println!(
            "\n=== PR C Interconnected /suggestion & Model-Profile Benchmark ===\n  model_profile lookup : {} us\n  KV-cache prefix match: 100% byte-identical ({} B static prefix preserved)\n  model_profile size   : {} B (capped <=400 B body)\n  /suggestion digest   : {:.2} ms ({} chars, includes <learn_report> + <rag_memory>)",
            lookup_us,
            base_prompt.len(),
            added_bytes,
            digest_ms,
            digest.len()
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}
