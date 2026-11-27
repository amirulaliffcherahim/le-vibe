//! Compaction policy, ported from pi's `core/compaction/compaction.ts`.
//!
//! pi triggers compaction on a **token reserve**, not a percentage of the window:
//! `contextTokens > contextWindow - reserveTokens`. A percentage rule is wrong at both ends of the
//! range — on a 1M window 85% leaves 150k of unused headroom, while on a 32k window it leaves only
//! 4.8k, which is less than one large tool result.
//!
//! The cut point keeps roughly `keepRecentTokens` of the newest entries verbatim and summarizes
//! only the head, so the model always sees the work it just did.
//!
//! Summarization also lives here instead of in the CLI: the summarizer call retries with the same
//! backoff as a normal turn, and keeping both in one crate means a single retry policy.

use le_vibe_durable::{EntryKind, SessionEntry, SessionError, SessionStore};
use le_vibe_pybridge::{BridgeError, ChatMessage, CompletionRequest, ProviderPort};
use std::sync::atomic::AtomicBool;

use crate::overflow::{estimate_tokens, is_context_overflow_text};
use crate::{is_cancelled, retry_delay_ms};

/// Instruction used by agent-driven compaction (threshold / overflow).
pub const DEFAULT_INSTRUCTION: &str = "Summarize the conversation into a concise technical \
handoff covering: (1) user goal, (2) files inspected/modified, (3) key decisions and constraints, \
(4) current state and remaining work. Keep under 600 words.";


/// Headroom kept free below the context window before compacting.
pub const DEFAULT_RESERVE_TOKENS: u64 = 16_384;

/// Approximate number of newest tokens that survive summarization verbatim.
pub const DEFAULT_KEEP_RECENT_TOKENS: u64 = 20_000;

/// `keep_recent_tokens` value meaning "summarize the whole branch".
///
/// Used by manual `/compact`, `/handover`, and branch summaries. It is *not* the same as a branch
/// that simply fits its budget (which keeps everything and summarizes nothing).
pub const SUMMARIZE_ALL: u64 = u64::MAX;

/// Whether the context is close enough to the window to compact.
///
/// Returns `false` when the window is unknown, and only triggers strictly *above* the reserve
/// boundary (matching pi's `>` comparison).
pub fn should_compact(context_tokens: u64, context_window: u64, reserve_tokens: u64) -> bool {
    if context_window == 0 {
        return false;
    }
    context_tokens > context_window.saturating_sub(reserve_tokens)
}

/// Where to cut the active branch for summarization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutPlan {
    /// Index of the first entry kept verbatim; everything before it is summarized.
    pub keep_from: usize,
    /// Entries before `keep_from`.
    pub summarized: usize,
    /// Entries from `keep_from` to the end.
    pub kept: usize,
    /// Token count of the kept tail.
    pub kept_tokens: u64,
}

/// Walk backwards from the newest entry accumulating tokens until at least
/// `keep_recent_tokens` are kept, and return that cut point.
///
/// Always keeps at least the newest entry, so a single huge message can never produce an
/// "everything is summarized" plan that would lose the current turn.
pub fn plan_cut(entry_tokens: &[u64], keep_recent_tokens: u64) -> CutPlan {
    let total = entry_tokens.len();
    if total == 0 {
        return CutPlan {
            keep_from: 0,
            summarized: 0,
            kept: 0,
            kept_tokens: 0,
        };
    }

    // The whole branch already fits the retention budget: keep every entry verbatim.
    let total_tokens: u64 = entry_tokens.iter().copied().sum();
    if total_tokens <= keep_recent_tokens {
        return CutPlan {
            keep_from: 0,
            summarized: 0,
            kept: total,
            kept_tokens: total_tokens,
        };
    }

    let mut accumulated = 0u64;
    let mut keep_from = total;
    for (idx, tokens) in entry_tokens.iter().enumerate().rev() {
        accumulated = accumulated.saturating_add(*tokens);
        keep_from = idx;
        if accumulated >= keep_recent_tokens {
            break;
        }
    }

    CutPlan {
        keep_from,
        summarized: keep_from,
        kept: total - keep_from,
        kept_tokens: accumulated,
    }
}

/// Result of one summarization pass, including the cut point it was computed for.
#[derive(Debug, Clone)]
pub struct SummaryPlan {
    pub summary: String,
    /// Index of the first entry that stays verbatim.
    pub keep_from: usize,
    pub summarized: usize,
    pub kept: usize,
    /// Prompt tokens before compaction (the number the trigger fired on).
    pub before_tokens: u64,
}

/// Everything one summarization call needs.
pub struct SummarizeOptions<'a> {
    pub provider: String,
    pub model: String,
    pub instruction: String,
    pub keep_recent_tokens: u64,
    pub max_retries: usize,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub cancel: Option<&'a AtomicBool>,
    /// Called before each backoff sleep with `(attempt, max_attempts, delay_ms)`.
    pub on_retry: &'a mut dyn FnMut(usize, usize, u64),
}

/// Render the entries that are about to be summarized.
pub fn serialize_branch(entries: &[SessionEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        match entry.kind {
            EntryKind::Message | EntryKind::CompactionSummary | EntryKind::BranchSummary => {
                out.push_str(&format!("[{}]: {}\n", entry.role, entry.content));
            }
            EntryKind::ToolCall => out.push_str(&format!(
                "[tool_call {}]: {}\n",
                entry.tool_name.as_deref().unwrap_or("tool"),
                entry.content
            )),
            EntryKind::ToolResult => {
                let preview: String = entry.content.chars().take(400).collect();
                out.push_str(&format!(
                    "[tool_result {}]: {}\n",
                    entry.tool_name.as_deref().unwrap_or("tool"),
                    preview
                ));
            }
            EntryKind::ModelChange | EntryKind::ModeChange | EntryKind::SessionMeta | EntryKind::Label => {}
        }
    }
    out
}

/// Summarize the head of `branch` with the same retry schedule as a normal turn.
///
/// Only entries before the cut point are sent to the model; the tail is retained verbatim so the
/// next request still sees the work that just happened.
pub fn summarize<B: ProviderPort>(
    bridge: &mut B,
    branch: &[SessionEntry],
    opts: &mut SummarizeOptions<'_>,
) -> Result<SummaryPlan, String> {
    if branch.is_empty() {
        return Err("Session is empty — nothing to summarize.".to_string());
    }
    let entry_tokens: Vec<u64> = branch
        .iter()
        .map(|e| estimate_tokens(&e.content) + 8)
        .collect();
    let before_tokens: u64 = entry_tokens.iter().sum();
    // `SUMMARIZE_ALL` is *not* `plan_cut`'s "everything fits, so keep it all" — conflating them
    // sent an empty transcript to the summarizer.
    let cut = if opts.keep_recent_tokens == SUMMARIZE_ALL {
        CutPlan {
            keep_from: branch.len(),
            summarized: branch.len(),
            kept: 0,
            kept_tokens: 0,
        }
    } else {
        plan_cut(&entry_tokens, opts.keep_recent_tokens)
    };
    let transcript = serialize_branch(&branch[..cut.keep_from]);

    let request = CompletionRequest {
        provider: opts.provider.clone(),
        model: opts.model.clone(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: format!("{}\n\n<transcript>\n{transcript}\n</transcript>", opts.instruction),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
        }],
        system_prompt: Some(
            "You are a precise technical summarizer. Output concise Markdown.".to_string(),
        ),
        tools: Vec::new(),
        thinking_level: "low".to_string(),
    };

    let mut attempt = 0usize;
    loop {
        if is_cancelled(opts.cancel) {
            return Err("Summarization cancelled.".to_string());
        }
        match bridge.complete(request.clone()) {
            Ok(reply) => {
                return Ok(SummaryPlan {
                    summary: reply.content,
                    keep_from: cut.keep_from,
                    summarized: cut.summarized,
                    kept: cut.kept,
                    before_tokens,
                });
            }
            Err(err) => {
                // A summarizer that overflows cannot recover by itself: the head is already
                // smaller than the branch, so the caller must fall back to a manual action.
                if is_context_overflow_text(&err.to_string(), 0) {
                    return Err(format!("Summarization request overflowed the context window: {err}"));
                }
                if attempt >= opts.max_retries || !is_retryable(&err) {
                    return Err(err.to_string());
                }
                attempt += 1;
                let delay = retry_delay_ms(opts.base_delay_ms, opts.max_delay_ms, attempt);
                (opts.on_retry)(attempt, opts.max_retries, delay);
                if crate::sleep_cancellable(opts.cancel, delay) {
                    return Err("Summarization cancelled.".to_string());
                }
            }
        }
    }
}

/// Retryable summarizer failures: transport errors and retryable HTTP statuses.
fn is_retryable(err: &BridgeError) -> bool {
    let text = err.to_string();
    [
        "HTTP 408",
        "HTTP 409",
        "HTTP 429",
        "HTTP 500",
        "HTTP 502",
        "HTTP 503",
        "HTTP 504",
        "HTTP 520",
        "HTTP 522",
        "HTTP 524",
        "Timeout",
        "timed out",
        "ConnectError",
        "Server disconnected",
        "incomplete chunked read",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

/// Summarize the whole branch with the workspace defaults (pi's retry budget).
///
/// Keeps `SummarizeOptions` boilerplate out of the CLI: every manual summary path (`/compact`,
/// `/handover`, bug reports, branch summaries) means "summarize everything".
pub fn summarize_all<B: ProviderPort>(
    bridge: &mut B,
    branch: &[SessionEntry],
    provider: &str,
    model: &str,
    instruction: &str,
    cancel: Option<&AtomicBool>,
    on_retry: &mut dyn FnMut(usize, usize, u64),
) -> Result<SummaryPlan, String> {
    let mut options = SummarizeOptions {
        provider: provider.to_string(),
        model: model.to_string(),
        instruction: instruction.to_string(),
        keep_recent_tokens: SUMMARIZE_ALL,
        max_retries: crate::DEFAULT_MAX_RETRIES,
        base_delay_ms: crate::DEFAULT_BASE_DELAY_MS,
        max_delay_ms: crate::DEFAULT_MAX_DELAY_MS,
        cancel,
        on_retry,
    };
    summarize(bridge, branch, &mut options)
}

/// Rewind-style branch summary with the workspace defaults.
pub fn branch_summary<B: ProviderPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    target: Option<&str>,
    provider: &str,
    model: &str,
    cancel: Option<&AtomicBool>,
    on_retry: &mut dyn FnMut(usize, usize, u64),
) -> Result<SessionEntry, String> {
    let mut options = SummarizeOptions {
        provider: provider.to_string(),
        model: model.to_string(),
        instruction: DEFAULT_INSTRUCTION.to_string(),
        keep_recent_tokens: SUMMARIZE_ALL,
        max_retries: crate::DEFAULT_MAX_RETRIES,
        base_delay_ms: crate::DEFAULT_BASE_DELAY_MS,
        max_delay_ms: crate::DEFAULT_MAX_DELAY_MS,
        cancel,
        on_retry,
    };
    summarize_and_branch(bridge, session, target, &mut options)
}

/// Summarize the path being abandoned, then branch to `target` recording that summary.
///
/// This is pi's `branchWithSummary`: the abandoned conversation survives only as a
/// `branch_summary` entry, so switching direction keeps the context that led there without
/// replaying it turn by turn. Used by rewind (never by `/tree` navigation, which must be instant).
pub fn summarize_and_branch<B: ProviderPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    target: Option<&str>,
    opts: &mut SummarizeOptions<'_>,
) -> Result<SessionEntry, String> {
    let abandoned = session.context_entries();
    if abandoned.is_empty() {
        // Nothing to summarize: just move the pointer, matching pi's `branch()`.
        match target {
            Some(id) => session.branch_from(id).map_err(|e| e.to_string())?,
            None => session.reset_leaf(),
        }
        return session
            .append_branch_summary("(empty branch)")
            .map_err(|e| e.to_string());
    }
    // A branch summary must cover the whole abandoned path: `keep_recent` only makes sense for
    // compaction, where the kept tail stays in context anyway.
    opts.keep_recent_tokens = SUMMARIZE_ALL;
    let plan = summarize(bridge, &abandoned, opts)?;
    session
        .branch_with_summary(target, plan.summary)
        .map_err(|e| e.to_string())
}

/// Persist a summary with pi's `firstKeptEntryId` boundary.
///
/// The retained tail is **not** copied: the boundary points at the first kept entry, so
/// [`SessionStore::context_entries`] restores it in place. Nothing is duplicated on disk.
pub fn apply_summary(
    session: &mut SessionStore,
    branch: &[SessionEntry],
    plan: &SummaryPlan,
) -> Result<(), SessionError> {
    let first_kept = branch
        .get(plan.keep_from)
        .map(|entry| entry.id.clone())
        .filter(|_| plan.kept > 0);
    session.append_compaction_with_keep(
        &plan.summary,
        first_kept,
        Some(plan.before_tokens),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_boundary_is_strict() {
        let window = 100_000;
        let reserve = 16_384;
        let boundary = window - reserve;
        assert!(!should_compact(boundary, window, reserve));
        assert!(should_compact(boundary + 1, window, reserve));
    }

    #[test]
    fn small_windows_keep_pi_headroom() {
        // 85% of 32k would be 27.2k; pi compacts at 15.6k, keeping ~16k usable.
        assert!(!should_compact(15_000, 32_000, DEFAULT_RESERVE_TOKENS));
        assert!(should_compact(16_000, 32_000, DEFAULT_RESERVE_TOKENS));
    }

    #[test]
    fn plan_cut_handles_a_single_entry() {
        let plan = plan_cut(&[999_999], DEFAULT_KEEP_RECENT_TOKENS);
        assert_eq!(plan.keep_from, 0);
        assert_eq!(plan.kept, 1);
        assert_eq!(plan.summarized, 0);
    }

    #[test]
    fn plan_cut_accumulates_until_the_target() {
        let plan = plan_cut(&[1_000, 10_000, 10_000, 10_000], 20_000);
        assert_eq!(plan.keep_from, 2);
        assert_eq!(plan.kept_tokens, 20_000);
    }
}
