//! Prompt/evaluation builders extracted out of `le-vibe-cli` (Thin-CLI sweep P0).
//!
//! These are pure functions: no bridge, no state, no I/O. Behaviour is identical to what the CLI
//! did inline, so the tests pin the exact strings the model sees.

use le_vibe_agent::workflows::{
    GOLDFISH_SYSTEM_PROMPT, MANUAL_COMPACT_INSTRUCTION, PROOF_MAX_ATTEMPTS, ProofOutcome,
    ProofState, evaluate_proof_reply, goldfish_review_content, goldfish_review_header,
    skill_distillation_prompt, skill_slug,
};

// ---------------------------------------------------------------------------
// /goal Proof Loop
// ---------------------------------------------------------------------------

#[test]
fn initial_proof_prompt_carries_the_condition_and_attempt_budget() {
    let prompt = ProofState::new("cargo test --workspace passes", PROOF_MAX_ATTEMPTS).initial_prompt();
    assert_eq!(PROOF_MAX_ATTEMPTS, 5);
    assert!(prompt.contains("PROOF LOOP GOAL (attempt 1/5)"), "{prompt}");
    assert!(prompt.contains("Binary stop condition: cargo test --workspace passes"));
    assert!(prompt.contains("PROOF_VERIFIED:"), "must name the success marker");
    assert!(prompt.contains("PROOF_BLOCKED:"), "must name the failure marker");
    assert!(prompt.contains("5-step Proof Loop"));
}

#[test]
fn continuation_prompt_repeats_the_condition_with_the_next_attempt() {
    let state = ProofState::new("all tests green", PROOF_MAX_ATTEMPTS).advance();
    let prompt = state.continuation_prompt();
    assert!(prompt.contains("PROOF LOOP CONTINUATION (attempt 2/5)"), "{prompt}");
    assert!(prompt.contains("Stop condition still not proven: all tests green"));
}

#[test]
fn proof_reply_evaluation_distinguishes_all_four_outcomes() {
    assert_eq!(
        evaluate_proof_reply("done\nPROOF_VERIFIED: cargo test -> ok", 1, 5),
        ProofOutcome::Verified
    );
    assert_eq!(
        evaluate_proof_reply("PROOF_BLOCKED: missing fixture", 1, 5),
        ProofOutcome::Blocked
    );
    // No marker and budget remains: keep going.
    assert_eq!(
        evaluate_proof_reply("still working on it", 1, 5),
        ProofOutcome::Continue
    );
    // No marker and the budget is spent: stop, but this is not a "blocked" claim.
    assert_eq!(
        evaluate_proof_reply("still working on it", 5, 5),
        ProofOutcome::CapReached
    );
    // Verified wins even on the last attempt.
    assert_eq!(
        evaluate_proof_reply("PROOF_VERIFIED: green", 5, 5),
        ProofOutcome::Verified
    );
    // Blocked wins over the cap check.
    assert_eq!(
        evaluate_proof_reply("PROOF_BLOCKED: nope", 5, 5),
        ProofOutcome::Blocked
    );
}

#[test]
fn proof_state_advances_without_exceeding_the_budget() {
    let mut state = ProofState::new("cond", 3);
    assert_eq!(state.attempt, 1);
    state = state.advance();
    assert_eq!(state.attempt, 2);
    state = state.advance();
    assert_eq!(state.attempt, 3);
    // Advancing past the cap stays at the cap instead of running away.
    assert_eq!(state.advance().attempt, 3);
}

// ---------------------------------------------------------------------------
// /goldfish
// ---------------------------------------------------------------------------

#[test]
fn goldfish_prompt_keeps_the_five_lenses_and_fresh_context_wording() {
    let sys = GOLDFISH_SYSTEM_PROMPT;
    assert!(sys.contains("FRESH-CONTEXT GOLDFISH REVIEWER"));
    assert!(sys.contains("ZERO prior conversation history"));
    for lens in [
        "Comprehension",
        "Structural",
        "Codebase Fit",
        "Design Critic",
        "Implementation Readiness",
    ] {
        assert!(sys.contains(lens), "missing lens {lens}");
    }
    assert!(sys.contains("APPROVE") && sys.contains("REVISE"));
}

#[test]
fn goldfish_content_names_the_artifact_source() {
    let content = goldfish_review_content("docs/spec.md", "BODY");
    assert!(content.contains("`docs/spec.md`"), "{content}");
    assert!(content.contains("BODY"));
    let header = goldfish_review_header("docs/spec.md");
    assert!(header.starts_with("## Goldfish Review (Fresh Context"));
    assert!(header.contains("`docs/spec.md`"));
}

// ---------------------------------------------------------------------------
// /skill-save
// ---------------------------------------------------------------------------

#[test]
fn skill_slug_is_filesystem_safe() {
    assert_eq!(skill_slug("Deploy Windows"), "deploy-windows");
    assert_eq!(skill_slug("  a/b/c  "), "a-b-c");
    assert_eq!(skill_slug("---"), "");
    assert_eq!(skill_slug(""), "");
    assert_eq!(skill_slug("MiXeD_case_OK"), "mixed_case_ok");
}

#[test]
fn skill_distillation_prompt_targets_the_skill_path() {
    let prompt = skill_distillation_prompt("deploy-windows", "focus on rollback");
    assert!(prompt.contains(".le-vibe/skills/deploy-windows/SKILL.md"), "{prompt}");
    assert!(prompt.contains("name: deploy-windows"));
    assert!(prompt.contains("agentskills.io"));
    assert!(prompt.contains("focus on rollback"));
    assert!(prompt.contains("`write` tool"));
    // No focus is allowed and must not leave a dangling space-only clause.
    let bare = skill_distillation_prompt("x", "");
    assert!(!bare.contains("  "), "double space leaked: {bare}");
}

// ---------------------------------------------------------------------------
// /compact instruction (moved string, guarded by a test so it can't drift)
// ---------------------------------------------------------------------------

#[test]
fn manual_compact_instruction_keeps_the_memory_flush_contract() {
    assert!(MANUAL_COMPACT_INSTRUCTION.contains("concise technical handoff"));
    assert!(MANUAL_COMPACT_INSTRUCTION.contains("user goal"));
    assert!(MANUAL_COMPACT_INSTRUCTION.contains("memory-flush"));
    assert!(MANUAL_COMPACT_INSTRUCTION.contains("max 3 items"));
}
