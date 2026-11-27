//! Context-overflow classification, ported from pi's `ai/src/utils/overflow.ts`.
//!
//! Three failure shapes mean "the prompt did not fit", and each needs a different recovery:
//!
//! 1. **Error overflow** — the provider rejected the request (HTTP 400/413 or a body message).
//!    Almost every backend phrases this differently, hence the pattern table.
//! 2. **Silent overflow** — z.ai style: the request succeeds but the prompt tokens already exceed
//!    the window. Only detectable by comparing usage against the window.
//! 3. **Length-stop overflow** — Xiaomi MiMo style: input is truncated to fill the window exactly,
//!    so the model returns `length` with (almost) no output.
//!
//! Overflow must never be retried as a transient error: retrying resends the same oversized
//! prompt. It is recovered by compacting and retrying once, which the caller does.

/// Error text shapes that mean the prompt exceeded the context window or request byte limit.
///
/// Comments name the provider each pattern was observed on, so a future pattern can be added
/// without guesswork.
/// Lowercase only: matching is case-insensitive, as pi's patterns use the `/i` flag.
const OVERFLOW_PATTERNS: &[&str] = &[
    "prompt is too long",                          // Anthropic
    "prompt exceeds max length",                   // z.ai (CN endpoint)
    "request_too_large",                           // Anthropic byte-size overflow (413)
    "input is too long for requested model",       // Amazon Bedrock
    "exceeds the context window",                  // OpenAI
    "exceeds the model's maximum context length",  // OpenAI-compatible proxies
    "exceeds the models maximum context length",   // ditto, apostrophe stripped
    "context window exceeded",                     // LiteLLM pre-call check
    "input token count",                           // Google Gemini
    "exceeds the maximum number of tokens",        // Google Gemini
    "maximum prompt length is",                    // xAI Grok
    "reduce the length of the messages",           // Groq
    "maximum context length is",                   // OpenRouter
    "exceeds the maximum allowed input length",    // OpenRouter / Poolside
    "is longer than the model",                    // Together AI
    "exceeds the available context size",          // llama.cpp server
    "greater than the context length",             // LM Studio
    "context window exceeds limit",                // MiniMax
    "exceeded model token limit",                  // Kimi for Coding
    "too large for model with",                    // Mistral
    "configured context size is",                  // DS4 server
    "model_context_window_exceeded",               // z.ai non-standard finish reason
    "exceeded max context length",                 // Ollama
    "range of input length should be",             // DashScope / Qwen
    "context_length_exceeded",                     // generic OpenAI-shaped fallback
    "context length exceeded",                     // generic fallback
    "too many tokens",                             // generic fallback
    "token limit exceeded",                        // generic fallback
    "prompt too long",                             // z.ai short form
];

/// Messages matching these are **not** overflow even when they also match an overflow pattern.
///
/// Bedrock formats throttling as "ThrottlingException: Too many tokens, please wait…", which
/// would otherwise be misread as overflow and trigger a pointless compaction.
const NON_OVERFLOW_PATTERNS: &[&str] = &[
    // Lowercase only: matching is case-insensitive, as pi's patterns use the `/i` flag.
    "throttling error:",
    "throttlingexception",
    "service unavailable:",
    "rate limit",
    "too many requests",
];

fn contains_any(lowercased_haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| lowercased_haystack.contains(n))
}

/// Bytes-per-token heuristic used when a provider reports no usage. Deliberately conservative
/// (over-estimates), matching pi's `estimateTokens`.
const CHARS_PER_TOKEN: u64 = 4;

/// Rough token estimate for a message body (chars/4).
pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(CHARS_PER_TOKEN)
}

/// Whether an error message means the prompt did not fit.
///
/// `context_window` enables the silent-overflow check; pass `0` when the window is unknown.
pub fn is_context_overflow_text(text: &str, context_window: u64) -> bool {
    let _ = context_window;
    let lower = text.to_ascii_lowercase();
    if contains_any(&lower, NON_OVERFLOW_PATTERNS) {
        return false;
    }
    if contains_any(&lower, OVERFLOW_PATTERNS) {
        return true;
    }
    // HTTP 413 is request-size overflow regardless of body text. Match the status at the start
    // of the message (raw SDK form) or pi's `formatProviderError` prefix, not a 413 quoted in a
    // human sentence.
    if lower.contains("api error (413):") {
        return true;
    }
    // Cerebras returns a bodiless 400/413.
    let trimmed = lower.trim_start();
    trimmed.starts_with("400") || trimmed.starts_with("413")
}

/// Silent overflow: the request succeeded, but the prompt already overflowed the window.
pub fn is_silent_overflow(stop_reason: &str, input_tokens: u64, _output_tokens: u64, context_window: u64) -> bool {
    stop_reason == "stop" && context_window > 0 && input_tokens > context_window
}

/// Length-stop overflow: output stopped well short of the requested cap, so the input filled the
/// window and the server had no room left to generate.
pub fn is_recoverable_length(stop_reason: &str, output_tokens: u64, desired_max_output: u64) -> bool {
    stop_reason == "length" && desired_max_output > 0 && output_tokens < desired_max_output
}

/// Full pi-equivalent check: error text first, then the two usage-based shapes.
pub fn is_context_overflow(
    stop_reason: &str,
    error_text: Option<&str>,
    input_tokens: u64,
    output_tokens: u64,
    context_window: u64,
) -> bool {
    if let Some(text) = error_text
        && is_context_overflow_text(text, context_window)
    {
        return true;
    }
    is_silent_overflow(stop_reason, input_tokens, output_tokens, context_window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_tokens_conservatively() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("日本語"), 1);
    }

    #[test]
    fn non_overflow_exclusion_beats_overflow_pattern() {
        assert!(!is_context_overflow_text(
            "ThrottlingException: Too many tokens, please wait",
            0
        ));
        assert!(is_context_overflow_text("too many tokens", 0));
    }

    #[test]
    fn http_413_is_overflow_only_in_status_position() {
        assert!(is_context_overflow_text("413 Payload Too Large", 0));
        assert!(is_context_overflow_text("anthropic API error (413): nope", 0));
        // A 413 mentioned inside prose must not trip the status match.
        assert!(!is_context_overflow_text("wrote 413 bytes to disk", 0));
    }

    #[test]
    fn combined_check_prefers_error_text() {
        assert!(is_context_overflow(
            "error",
            Some("prompt is too long"),
            10,
            10,
            1_000_000
        ));
        assert!(is_context_overflow("stop", None, 1_100_000, 10, 1_000_000));
        assert!(!is_context_overflow("stop", None, 10, 10, 1_000_000));
    }
}
