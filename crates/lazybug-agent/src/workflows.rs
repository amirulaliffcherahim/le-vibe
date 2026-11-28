//! Prompt construction and reply evaluation for the CLI's slash commands.
//!
//! These used to be inline `format!` strings in `main.rs` / `worker.rs`. Moving them here keeps the
//! CLI to dispatch + state, makes the exact model-facing text reviewable in one place, and lets the
//! behaviour be pinned by tests instead of read by eye.
//!
//! Everything in this module is pure: no bridge, no session, no I/O.

// ---------------------------------------------------------------------------
// /goal — Claude-Code-style Proof Loop
// ---------------------------------------------------------------------------

/// How many Proof Loop attempts `/goal` allows before giving up.
pub const PROOF_MAX_ATTEMPTS: usize = 5;

/// Marker the model must emit when the binary stop condition is proven.
pub const PROOF_VERIFIED_MARKER: &str = "PROOF_VERIFIED:";
/// Marker the model must emit when it cannot make progress.
pub const PROOF_BLOCKED_MARKER: &str = "PROOF_BLOCKED:";

/// A Proof Loop in progress: the stop condition plus the current attempt budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofState {
    pub condition: String,
    pub attempt: usize,
    pub max_attempts: usize,
}

impl ProofState {
    pub fn new(condition: impl Into<String>, max_attempts: usize) -> Self {
        Self {
            condition: condition.into(),
            attempt: 1,
            max_attempts: max_attempts.max(1),
        }
    }

    /// Move to the next attempt, never past the budget.
    pub fn advance(&self) -> Self {
        Self {
            condition: self.condition.clone(),
            attempt: (self.attempt + 1).min(self.max_attempts),
            max_attempts: self.max_attempts,
        }
    }

    /// The first prompt sent for this goal.
    pub fn initial_prompt(&self) -> String {
        format!(
            "PROOF LOOP GOAL (attempt {}/{})\nBinary stop condition: {}\n\n\
Follow the 5-step Proof Loop (1. Act minimal diff -> 2. Run deterministic check -> \
3. Inspect raw output -> 4. Assert & fix -> 5. Terminate on evidence).\n\
When the binary stop condition is proven with real command output, end your reply with:\n\
{PROOF_VERIFIED_MARKER} <exact command and observed pass output>\n\
If genuinely blocked after retries, end with:\n\
{PROOF_BLOCKED_MARKER} <exact blocker>",
            self.attempt, self.max_attempts, self.condition
        )
    }

    /// The nudge sent when the goal is not proven yet and budget remains.
    pub fn continuation_prompt(&self) -> String {
        format!(
            "PROOF LOOP CONTINUATION (attempt {}/{})\nStop condition still not proven: {}\n\
Run the deterministic check, inspect raw stdout/stderr, fix any remaining issue, and end with \
`{PROOF_VERIFIED_MARKER} <command & pass output>` or `{PROOF_BLOCKED_MARKER} <exact blocker>`.",
            self.attempt, self.max_attempts, self.condition
        )
    }
}

/// What the model's reply says about the goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofOutcome {
    /// It emitted `PROOF_VERIFIED:` — the turn is done.
    Verified,
    /// It emitted `PROOF_BLOCKED:` — stop and report, the model says it cannot proceed.
    Blocked,
    /// No marker and the attempt budget is spent.
    CapReached,
    /// No marker and there is budget left: nudge it again.
    Continue,
    }

/// Classify one reply. Verified/Blocked win over the budget check, so the final attempt can still
/// succeed or report a blocker.
pub fn evaluate_proof_reply(reply: &str, attempt: usize, max_attempts: usize) -> ProofOutcome {
    if reply.contains(PROOF_VERIFIED_MARKER) {
        return ProofOutcome::Verified;
    }
    if reply.contains(PROOF_BLOCKED_MARKER) {
        return ProofOutcome::Blocked;
    }
    if attempt >= max_attempts {
        return ProofOutcome::CapReached;
    }
    ProofOutcome::Continue
}

// ---------------------------------------------------------------------------
// /goldfish — fresh-context EGM reviewer
// ---------------------------------------------------------------------------

/// System prompt for the zero-history reviewer (Elephant-Goldfish Model).
pub const GOLDFISH_SYSTEM_PROMPT: &str = "\
You are a FRESH-CONTEXT GOLDFISH REVIEWER (Elephant-Goldfish Model). You have ZERO prior conversation history — only the artifact below.
Critique it ruthlessly and constructively across the 5 EGM lenses:
1. **Comprehension**: ambiguous requirements, undefined terms, unstated assumptions.
2. **Structural**: missing failure modes, state transitions, concurrency/rollback edge cases.
3. **Codebase Fit**: claims or interfaces that need concrete verification against real files/symbols.
4. **Design Critic**: simpler alternatives, hidden coupling, over-engineering.
5. **Implementation Readiness**: missing binary acceptance criteria or deterministic verification commands.
End with a crisp **Verdict** (`APPROVE` / `REVISE`) and a numbered list of top concrete fixes.";

/// Maximum number of artifact characters handed to the reviewer.
pub const GOLDFISH_MAX_ARTIFACT_CHARS: usize = 24_000;

/// The user message carrying the artifact.
pub fn goldfish_review_content(source_label: &str, content: &str) -> String {
    format!("Review artifact (`{source_label}`):\n\n{content}")
}

/// Heading prefixed to the review when it is printed into the transcript.
pub fn goldfish_review_header(source_label: &str) -> String {
    format!("## Goldfish Review (Fresh Context · `{source_label}`)\n\n")
}

// ---------------------------------------------------------------------------
// Manual compaction / handover instructions
// ---------------------------------------------------------------------------

/// Instruction for `/compact` (includes the durable-memory flush contract).
pub const MANUAL_COMPACT_INSTRUCTION: &str = "\
Summarize the conversation into a concise technical handoff covering: (1) user goal, \
(2) files inspected/modified, (3) key decisions and constraints, (4) current state and remaining \
work. Keep under 600 words.\n\
If (and ONLY if) the conversation revealed durable architectural decisions, non-obvious \
constraints, or bug root causes worth keeping across future sessions, append a JSON array block \
fenced as ```memory-flush\\n[{{\"title\":\"...\",\"key\":\"...\",\"keywords\":[...],\"content\":\"...\"}}]\\n``` \
(max 3 items; omit if nothing durable).";

/// Instruction for `/handover`.
pub const HANDOVER_INSTRUCTION: &str = "\
Produce a structured session handover document covering: (1) Goal, (2) Files inspected/modified, \
(3) Decisions & constraints, (4) Open items / verification commands.\n\
If (and ONLY if) the session revealed durable decisions, constraints, or bug root causes worth \
keeping across future sessions, append a JSON array block fenced as ```memory-flush\\n[{{\"title\":\"...\",\"key\":\"...\",\"keywords\":[...],\"content\":\"...\"}}]\\n``` \
(max 3 items; omit if nothing durable).";

// ---------------------------------------------------------------------------
// /skill-save
// ---------------------------------------------------------------------------

/// Filesystem-safe slug for a skill name (`Deploy Windows` -> `deploy-windows`).
pub fn skill_slug(raw: &str) -> String {
    let slug: String = raw
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    slug.trim_matches('-').to_string()
}

/// Repo-relative path the distilled skill is written to.
pub fn skill_target(slug: &str) -> String {
    format!(".lazybug/skills/{slug}/SKILL.md")
}

/// Prompt that asks the model to distill the session into an agentskills.io skill file.
pub fn skill_distillation_prompt(slug: &str, focus: &str) -> String {
    let target = skill_target(slug);
    let extra = focus.trim();
    let focus_clause = if extra.is_empty() {
        String::new()
    } else {
        format!(" {extra}")
    };
    format!(
        "Distill the reusable workflow/procedure from this session into `{target}` using the \
agentskills.io format (YAML frontmatter with `name: {slug}` and concise `description:`, followed \
by clear step-by-step instructions, commands, and gotchas).{focus_clause}\n\
Write `{target}` directly using the `write` tool."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_reached_is_not_reported_as_blocked() {
        assert_eq!(evaluate_proof_reply("nothing", 4, 5), ProofOutcome::Continue);
        assert_eq!(evaluate_proof_reply("nothing", 5, 5), ProofOutcome::CapReached);
    }

    #[test]
    fn skill_target_is_derived_from_the_slug() {
        assert_eq!(skill_target("x"), ".lazybug/skills/x/SKILL.md");
    }

    #[test]
    fn proof_state_keeps_a_valid_budget() {
        let zero = ProofState::new("c", 0);
        assert_eq!(zero.max_attempts, 1, "budget is clamped to at least one attempt");
    }
}
