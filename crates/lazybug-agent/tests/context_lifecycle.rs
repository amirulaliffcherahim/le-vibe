//! P0 context-lifecycle parity with pi: overflow classification, compaction policy, retry policy.
//!
//! These are the pure decision functions that the turn loop will call in P1. Keeping them pure
//! means the rules are testable without a provider, a session file, or a TUI.

use lazybug_agent::{compact, overflow, retry_delay_ms, AgentConfig};

// ---------------------------------------------------------------------------
// 1. Overflow classification (pi: ai/src/utils/overflow.ts)
// ---------------------------------------------------------------------------

#[test]
fn detects_error_based_context_overflow_across_providers() {
    // Real provider phrasing, one per backend we can actually reach through the bridge.
    let cases = [
        "prompt is too long: 213462 tokens > 200000 maximum", // Anthropic
        r#"413 {"error":{"type":"request_too_large","message":"Request exceeds the maximum size"}}"#,
        "Your input exceeds the context window of this model", // OpenAI
        "This model's maximum context length is 131072 tokens", // OpenAI-compatible
        "The input token count (1196265) exceeds the maximum number of tokens allowed (1048575)", // Google
        "This model's maximum prompt length is 131072 but the request contains 537812 tokens", // xAI
        "Please reduce the length of the messages or completion", // Groq
        "Prompt contains 300000 tokens, too large for model with 131072 maximum context length",
        "Prompt too long", // z.ai
        "token limit exceeded",
    ];
    for text in cases {
        assert!(
            overflow::is_context_overflow_text(text, 0),
            "should be classified as overflow: {text:?}"
        );
    }
}

#[test]
fn rate_limit_and_server_errors_are_not_overflow() {
    // Bedrock formats throttling as "Too many tokens", which matches an overflow pattern:
    // the non-overflow exclusion list must win.
    let cases = [
        "ThrottlingException: Too many tokens, please wait before trying again.",
        "Throttling error: Too many tokens",
        "Service unavailable: too many tokens",
        "429 rate limit exceeded",
        "Too many requests",
    ];
    for text in cases {
        assert!(
            !overflow::is_context_overflow_text(text, 0),
            "must NOT be classified as overflow: {text:?}"
        );
    }
}

#[test]
fn detects_silent_overflow_from_usage_when_provider_says_stop() {
    // z.ai style: the request succeeds but prompt tokens already exceed the window.
    assert!(overflow::is_silent_overflow("stop", 1_050_000, 100, 1_000_000));
    // Under the window, nothing to do.
    assert!(!overflow::is_silent_overflow("stop", 900_000, 100, 1_000_000));
    // Only a successful stop counts; an error is handled by the message patterns.
    assert!(!overflow::is_silent_overflow("error", 1_050_000, 100, 1_000_000));
    // No known window means no silent check.
    assert!(!overflow::is_silent_overflow("stop", 1_050_000, 100, 0));
}

#[test]
fn recoverable_length_means_output_stopped_short_of_the_requested_cap() {
    // MiMo style: input filled the window, so the model got "length" with output far below max.
    assert!(overflow::is_recoverable_length("length", 0, 8192));
    assert!(overflow::is_recoverable_length("length", 500, 8192));
    // Output hit the requested cap: genuinely long answer, not an overflow.
    assert!(!overflow::is_recoverable_length("length", 8192, 8192));
    // Not a length stop at all.
    assert!(!overflow::is_recoverable_length("stop", 0, 8192));
    // No requested cap means nothing to compare against.
    assert!(!overflow::is_recoverable_length("length", 0, 0));
}

// ---------------------------------------------------------------------------
// 2. Compaction policy (pi: core/compaction/compaction.ts)
// ---------------------------------------------------------------------------

#[test]
fn compaction_triggers_on_window_minus_reserve_not_a_percentage() {
    let window = 200_000u64;
    let reserve = 16_384u64;

    assert_eq!(compact::DEFAULT_RESERVE_TOKENS, 16_384);
    assert_eq!(compact::DEFAULT_KEEP_RECENT_TOKENS, 20_000);

    // pi's rule: compact when contextTokens > contextWindow - reserveTokens.
    assert!(!compact::should_compact(180_000, window, reserve));
    assert!(compact::should_compact(183_617, window, reserve));
    // A small window leaves proportionally far more headroom than an 85% rule would.
    assert!(!compact::should_compact(15_000, 32_000, reserve));
    assert!(compact::should_compact(15_617, 32_000, reserve));
    // Unknown window never triggers.
    assert!(!compact::should_compact(999_999, 0, reserve));
    // Disabled reserve of 0 means "compact only once over the window".
    assert!(!compact::should_compact(200_000, window, 0));
}

#[test]
fn plan_cut_keeps_the_recent_tail_verbatim() {
    // 10 entries, 5k tokens each = 50k. Keeping >= 20k recent means the last 4 entries survive.
    let tokens: Vec<u64> = vec![5_000; 10];
    let plan = compact::plan_cut(&tokens, compact::DEFAULT_KEEP_RECENT_TOKENS);

    assert_eq!(plan.keep_from, 6, "keep the last 4 entries");
    assert_eq!(plan.summarized, 6);
    assert_eq!(plan.kept, 4);
    assert_eq!(plan.kept_tokens, 20_000);
}

#[test]
fn plan_cut_edge_cases() {
    // Everything fits: nothing to summarize, keep everything.
    let plan = compact::plan_cut(&[1_000, 1_000], 20_000);
    assert_eq!(plan.keep_from, 0);
    assert_eq!(plan.summarized, 0);

    // Empty branch: no work, and no panic.
    let empty = compact::plan_cut(&[], 20_000);
    assert_eq!(empty.keep_from, 0);
    assert_eq!(empty.kept, 0);

    // A single huge entry still gets kept (never summarize literally everything).
    let plan = compact::plan_cut(&[500_000, 500_000], 20_000);
    assert_eq!(plan.keep_from, 1);
    assert_eq!(plan.kept, 1);
    assert_eq!(plan.summarized, 1);
}

// ---------------------------------------------------------------------------
// 3. Retry policy (pi: ai/src/utils/retry.ts + settings-defaults.ts)
// ---------------------------------------------------------------------------

#[test]
fn retry_defaults_match_pi() {
    let cfg = AgentConfig::default();
    assert_eq!(cfg.max_retries, 3);
    assert_eq!(cfg.base_delay_ms, 2_000);
    assert_eq!(cfg.max_delay_ms, 60_000);
}

#[test]
fn retry_delay_is_exponential_from_the_base_and_capped() {
    // min(base * 2^(attempt-1), max)
    assert_eq!(retry_delay_ms(2_000, 60_000, 1), 2_000);
    assert_eq!(retry_delay_ms(2_000, 60_000, 2), 4_000);
    assert_eq!(retry_delay_ms(2_000, 60_000, 3), 8_000);
    assert_eq!(retry_delay_ms(2_000, 60_000, 4), 16_000);
    // Capped, and attempt 0/-1 can never panic or go negative.
    assert_eq!(retry_delay_ms(2_000, 60_000, 10), 60_000);
    assert_eq!(retry_delay_ms(2_000, 60_000, 0), 2_000);
    // Huge attempts saturate instead of overflowing.
    assert_eq!(retry_delay_ms(2_000, 60_000, 200), 60_000);
}
