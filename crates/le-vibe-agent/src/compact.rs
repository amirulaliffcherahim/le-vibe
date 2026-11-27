//! Compaction policy, ported from pi's `core/compaction/compaction.ts`.
//!
//! pi triggers compaction on a **token reserve**, not a percentage of the window:
//! `contextTokens > contextWindow - reserveTokens`. A percentage rule is wrong at both ends of the
//! range — on a 1M window 85% leaves 150k of unused headroom, while on a 32k window it leaves only
//! 4.8k, which is less than one large tool result.
//!
//! The cut point keeps roughly `keepRecentTokens` of the newest entries verbatim and summarizes
//! only the head, so the model always sees the work it just did.

/// Headroom kept free below the context window before compacting.
pub const DEFAULT_RESERVE_TOKENS: u64 = 16_384;

/// Approximate number of newest tokens that survive summarization verbatim.
pub const DEFAULT_KEEP_RECENT_TOKENS: u64 = 20_000;

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
