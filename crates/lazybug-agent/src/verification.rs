//! VerificationGate: refuse a stop that claims success without proof.
//!
//! Every system prompt says "verify your changes". Long turns erode that, because the model is
//! rewarded for producing a confident summary, not for running the test. A prose rule is a
//! suggestion; this is a state machine the harness checks before it accepts a stop.
//!
//! The gate is deliberately weak in one direction: it nudges **once** per prompt and then gives
//! up. A gate that can block forever turns a forgetful model into an infinite loop, which is worse
//! than accepting an unverified answer.

/// Tools that change the working tree.
pub const MUTATION_TOOLS: &[&str] = &["edit", "write", "multiedit", "apply_patch"];

/// Tools that can prove a change works.
pub const SHELL_TOOLS: &[&str] = &["bash", "powershell", "shell", "exec"];

/// One-line harness nudge injected when an unverified edit tries to stop.
///
/// Kept terse and free of any "next time" language: the model must not learn that the nudge is
/// advisory.
pub const NUDGE_MESSAGE: &str = "[verification-gate] You edited files but have not run any command \
since the last edit. Run the smallest deterministic check (test, lint, type-check, or build) and \
report the exact command with its real output before finishing.";

/// Whether a tool call changes files.
pub fn is_mutation_tool(name: &str) -> bool {
    MUTATION_TOOLS.contains(&name)
}

/// Whether a tool call can serve as verification.
pub fn is_shell_tool(name: &str) -> bool {
    SHELL_TOOLS.contains(&name)
}

/// Whether a shell tool result represents a zero exit.
///
/// `run_shell` returns `Ok(...)` even when the command fails, putting `exit_code=N` in the body, so
/// the tool's own `is_error` flag is *not* a success signal — it would treat every red test run as
/// verification.
pub fn shell_exited_zero(output: &str) -> bool {
    output
        .lines()
        .next()
        .is_some_and(|line| line.trim() == "exit_code=0")
        || output.lines().any(|line| line.trim() == "exit_code=0")
}

/// Per-prompt verification state.
#[derive(Debug, Clone)]
pub struct VerificationGate {
    enabled: bool,
    mutated: bool,
    verified: bool,
    nudges: usize,
}

impl VerificationGate {
    /// `enabled == false` disables the gate entirely (read-only modes, or the settings opt-out).
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            mutated: false,
            verified: false,
            nudges: 0,
        }
    }

    /// Record one completed tool call.
    ///
    /// A successful mutation arms the gate and invalidates any earlier verification; a passing shell
    /// command clears the requirement. Failed mutations change nothing, because nothing changed on
    /// disk.
    pub fn observe_tool_result(&mut self, tool: &str, is_error: bool, output: &str) {
        if !self.enabled {
            return;
        }
        if is_mutation_tool(tool) {
            if !is_error {
                self.mutated = true;
                self.verified = false;
            }
            return;
        }
        if is_shell_tool(tool) && !is_error && shell_exited_zero(output) {
            self.verified = true;
        }
    }

    /// Whether the harness should refuse this stop and ask for evidence.
    ///
    /// Returns `true` at most once per gate (i.e. per prompt), which is the hard latch against
    /// nudging loops.
    pub fn should_nudge(&mut self) -> bool {
        if !self.enabled || self.nudges > 0 {
            return false;
        }
        if self.mutated && !self.verified {
            self.nudges += 1;
            return true;
        }
        false
    }

    /// One-line state summary for debug surfaces.
    pub fn report(&self) -> String {
        format!(
            "verification-gate: enabled={} mutated={} verified={} nudges={}",
            self.enabled, self.mutated, self.verified, self.nudges
        )
    }
}

/// Read-only agent modes never mutate, so the gate stays off there.
pub fn mode_is_read_only(mode_name: Option<&str>) -> bool {
    matches!(mode_name, Some("plan") | Some("ask"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_modes_are_detected() {
        assert!(mode_is_read_only(Some("plan")));
        assert!(mode_is_read_only(Some("ask")));
        assert!(!mode_is_read_only(Some("normal")));
        assert!(!mode_is_read_only(Some("rea")));
        assert!(!mode_is_read_only(Some("cyber")));
        assert!(!mode_is_read_only(None));
    }

    #[test]
    fn exit_code_zero_must_be_the_reported_code() {
        assert!(shell_exited_zero("exit_code=0"));
        assert!(!shell_exited_zero("exit_code=01"));
        assert!(!shell_exited_zero("warning: exit_code=0 is expected"));
    }

    #[test]
    fn disabled_gate_ignores_observations() {
        let mut gate = VerificationGate::new(false);
        gate.observe_tool_result("edit", false, "x");
        gate.observe_tool_result("bash", false, "exit_code=0");
        assert!(gate.report().contains("mutated=false"));
    }
}
