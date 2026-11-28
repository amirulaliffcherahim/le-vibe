//! VerificationGate: a Rust `if` instead of a prose rule.
//!
//! Prose instructions ("always verify your changes") decay over a long turn. The gate makes the
//! harness refuse to accept a stop when the model edited files and never proved the result, without
//! ever being able to loop: one nudge per prompt, then it gives up and lets the model stop.

use le_vibe_agent::verification::{
    MUTATION_TOOLS, NUDGE_MESSAGE, VerificationGate, is_mutation_tool, shell_exited_zero,
};

// ---------------------------------------------------------------------------
// Tool classification
// ---------------------------------------------------------------------------

#[test]
fn mutation_and_shell_tools_are_recognised() {
    for tool in MUTATION_TOOLS {
        assert!(is_mutation_tool(tool), "{tool} must count as a mutation");
    }
    assert!(!is_mutation_tool("read"));
    assert!(!is_mutation_tool("grep"));
    assert!(!is_mutation_tool("bash"));
}

#[test]
fn shell_success_is_read_from_the_exit_code_not_the_tool_status() {
    // `run_shell` returns Ok with `exit_code=N` in the body, so a failing command is a *successful*
    // tool call. The gate must look at the exit code, otherwise every failing test would "verify".
    assert!(shell_exited_zero("exit_code=0\nall tests passed"));
    assert!(shell_exited_zero("exit_code=0"));
    assert!(!shell_exited_zero("exit_code=1\n2 failed"));
    assert!(!shell_exited_zero("exit_code=101\npanicked"));
    assert!(!shell_exited_zero("no exit code here"));
    assert!(!shell_exited_zero(""));
}

// ---------------------------------------------------------------------------
// The five required behaviours
// ---------------------------------------------------------------------------

#[test]
fn mutate_then_stop_is_nudged_once_then_allowed() {
    let mut gate = VerificationGate::new(true);
    gate.observe_tool_result("edit", false, "updated src/lib.rs");

    assert!(gate.should_nudge(), "first stop after an unverified edit");
    assert!(!gate.should_nudge(), "the latch must prevent a second nudge");
    assert!(!gate.should_nudge(), "and it must stay latched");
}

#[test]
fn mutate_then_passing_command_is_allowed_immediately() {
    let mut gate = VerificationGate::new(true);
    gate.observe_tool_result("edit", false, "updated");
    gate.observe_tool_result("bash", false, "exit_code=0\nok");

    assert!(!gate.should_nudge(), "a passing command after the edit verifies it");
}

#[test]
fn a_second_mutation_invalidates_a_passing_verification() {
    let mut gate = VerificationGate::new(true);
    gate.observe_tool_result("edit", false, "first edit");
    gate.observe_tool_result("bash", false, "exit_code=0");
    gate.observe_tool_result("write", false, "second file");

    assert!(
        gate.should_nudge(),
        "the earlier green run no longer covers the newest mutation"
    );
}

#[test]
fn failed_mutations_do_not_arm_the_gate() {
    let mut gate = VerificationGate::new(true);
    gate.observe_tool_result("edit", true, "Tool error: old_string not found");

    assert!(!gate.should_nudge(), "a rejected edit changed nothing");
}

#[test]
fn disabled_gate_and_read_only_mode_never_nudge() {
    let mut disabled = VerificationGate::new(false);
    disabled.observe_tool_result("edit", false, "updated");
    assert!(!disabled.should_nudge());

    // Read-only modes pass `enabled = false` too, which is how Plan/Ask stay untouched.
    let mut read_only = VerificationGate::new(false);
    read_only.observe_tool_result("write", false, "n/a");
    read_only.observe_tool_result("bash", false, "exit_code=1");
    assert!(!read_only.should_nudge());
}

// ---------------------------------------------------------------------------
// Edge cases worth pinning
// ---------------------------------------------------------------------------

#[test]
fn failing_command_does_not_verify() {
    let mut gate = VerificationGate::new(true);
    gate.observe_tool_result("edit", false, "updated");
    gate.observe_tool_result("bash", false, "exit_code=1\n1 failed");

    assert!(gate.should_nudge(), "a red test run is not verification");
}

#[test]
fn verification_before_any_mutation_does_not_arm_the_gate() {
    let mut gate = VerificationGate::new(true);
    gate.observe_tool_result("bash", false, "exit_code=0");

    assert!(!gate.should_nudge(), "nothing was edited");
}

#[test]
fn nudge_message_names_the_requirement_and_forbids_guessing() {
    assert!(NUDGE_MESSAGE.contains("verification"));
    assert!(
        NUDGE_MESSAGE.to_lowercase().contains("run"),
        "it must tell the model what to do: {NUDGE_MESSAGE}"
    );
    assert!(
        !NUDGE_MESSAGE.contains('\n'),
        "keep it one line so it reads as a terse harness message"
    );
    // It must not promise a retry budget, or the model learns it can ignore it.
    assert!(!NUDGE_MESSAGE.to_lowercase().contains("again"));
}

#[test]
fn state_is_reported_for_diagnostics() {
    let mut gate = VerificationGate::new(true);
    gate.observe_tool_result("edit", false, "updated");
    gate.observe_tool_result("bash", false, "exit_code=0");
    let report = gate.report();
    assert!(report.contains("mutated"), "{report}");
    assert!(report.contains("verified"), "{report}");
    assert!(report.contains("nudges=0"), "{report}");
}

// ---------------------------------------------------------------------------
// Loop-level integration (real turn loop, scripted provider, real tools)
// ---------------------------------------------------------------------------

mod loop_level {
    use le_vibe_agent::{
        AgentConfig, AgentEvent, ApprovalChoice, TelemetryRecorder, NUDGE_MESSAGE,
        run_turn_loop_cancellable,
    };
    use le_vibe_durable::SessionStore;
    use le_vibe_pybridge::{
        AssistantMessage, BridgeError, CompletionRequest, ExtensionPort, ModelSpec, PolicyDecision,
        PolicyPort, ProviderPort, ToolCall, Usage,
    };
    use le_vibe_tools::TodoStore;
    use serde_json::Value;
    use std::sync::atomic::AtomicBool;

    #[derive(Clone)]
    enum Step {
        Write,
        ShellOk,
        Reply(&'static str),
    }

    struct Bridge {
        script: Vec<Step>,
        sent: Vec<CompletionRequest>,
    }

    impl Bridge {
        fn new(script: Vec<Step>) -> Self {
            Self {
                script,
                sent: Vec::new(),
            }
        }
    }

    fn message(content: &str, stop: &str, tool_calls: Vec<ToolCall>) -> AssistantMessage {
        AssistantMessage {
            provider: "vibe".into(),
            model: "m".into(),
            content: content.into(),
            reasoning_content: None,
            tool_calls,
            stop_reason: stop.into(),
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                reasoning_tokens: 0,
                cost_usd: 0.0,
            },
        }
    }

    impl ProviderPort for Bridge {
        fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
            Ok(vec![])
        }
        fn list_models(&mut self, _p: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
            Ok(vec![])
        }
        fn complete(&mut self, request: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
            let idx = self.sent.len();
            self.sent.push(request);
            match self.script.get(idx).cloned().unwrap_or(Step::Reply("no-more-script")) {
                Step::Write => Ok(message(
                    "writing",
                    "tool_calls",
                    vec![ToolCall {
                        id: "call_w".into(),
                        name: "write".into(),
                        arguments: serde_json::json!({
                            "path": std::env::temp_dir()
                                .join("le_vibe_gate_probe.txt")
                                .display()
                                .to_string(),
                            "content": "gate probe"
                        }),
                        thought_signature: None,
                    }],
                )),
                Step::ShellOk => Ok(message(
                    "checking",
                    "tool_calls",
                    vec![ToolCall {
                        id: "call_s".into(),
                        name: "powershell".into(),
                        arguments: serde_json::json!({ "command": "exit 0" }),
                        thought_signature: None,
                    }],
                )),
                Step::Reply(text) => Ok(message(text, "stop", vec![])),
            }
        }
    }

    impl PolicyPort for Bridge {
        fn evaluate_tool_call(
            &mut self,
            _tool: &str,
            _command: Option<&str>,
            _path: Option<&str>,
        ) -> Result<PolicyDecision, BridgeError> {
            Ok(PolicyDecision {
                action: "allow".into(),
                reason: String::new(),
            })
        }
    }

    impl ExtensionPort for Bridge {
        fn invoke_hook(&mut self, _event: &str, payload: Value) -> Result<Value, BridgeError> {
            Ok(payload)
        }
    }

    fn cfg(verify_edits: bool, mode: &str) -> AgentConfig {
        AgentConfig {
            provider: "vibe".into(),
            model: "m".into(),
            system_prompt: Some("sys".into()),
            thinking_level: "low".into(),
            max_turns: 8,
            max_retries: 0,
            base_delay_ms: 1,
            max_delay_ms: 2,
            allowed_tools: None,
            mode_name: Some(mode.to_string()),
            codemode: "off".into(),
            steering_queue: None,
            output_store: None,
            context_window: 0,
            reserve_tokens: 0,
            verify_edits,
            ..AgentConfig::default()
        }
    }

    fn run(bridge: &mut Bridge, cfg: &AgentConfig) -> (String, Vec<AgentEvent>) {
        let mut session = SessionStore::in_memory();
        let cancel = AtomicBool::new(false);
        let telemetry = TelemetryRecorder::new();
        let mut todos = TodoStore::new();
        let mut events = Vec::new();
        let reply = run_turn_loop_cancellable(
            bridge,
            &mut session,
            &telemetry,
            &mut todos,
            None,
            Some(&cancel),
            cfg,
            "go",
            |ev| events.push(ev),
            |_| ApprovalChoice::AllowOnce,
            |_| String::new(),
        )
        .expect("turn loop");
        (reply.content, events)
    }

    fn nudged(bridge: &Bridge) -> bool {
        bridge
            .sent
            .iter()
            .any(|r| r.messages.iter().any(|m| m.content == NUDGE_MESSAGE))
    }

    #[test]
    fn unverified_edit_is_nudged_once_then_the_model_may_stop() {
        let mut bridge = Bridge::new(vec![
            Step::Write,
            Step::Reply("first answer"),
            Step::Reply("second answer"),
            Step::Reply("third answer"),
        ]);
        let (content, events) = run(&mut bridge, &cfg(true, "normal"));

        assert_eq!(content, "second answer", "the latched gate must let it stop");
        assert!(nudged(&bridge), "the first unverified stop must be refused");
        // Exactly one nudge: the nudge appears in exactly one request.
        let hits = bridge
            .sent
            .iter()
            .filter(|r| r.messages.iter().any(|m| m.content == NUDGE_MESSAGE))
            .count();
        assert_eq!(hits, 1, "the latch must prevent a second nudge");
        assert!(
            bridge.sent.len() == 3,
            "expected write, nudge-retry, stop; got {} requests",
            bridge.sent.len()
        );
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::Status { text } if text.contains("Verification gate"))),
            "the nudge should be visible in the status strip"
        );
        let _ = std::fs::remove_file(
            std::env::temp_dir().join("le_vibe_gate_probe.txt"),
        );
    }

    #[test]
    fn verified_edit_stops_without_a_nudge() {
        let mut bridge = Bridge::new(vec![Step::Write, Step::ShellOk, Step::Reply("done")]);
        let (content, _events) = run(&mut bridge, &cfg(true, "normal"));

        assert_eq!(content, "done");
        assert!(!nudged(&bridge), "a passing command verifies the edit");
        let _ = std::fs::remove_file(std::env::temp_dir().join("le_vibe_gate_probe.txt"));
    }

    #[test]
    fn gate_off_or_read_only_mode_stops_immediately() {
        let mut off = Bridge::new(vec![Step::Write, Step::Reply("done")]);
        let (content, _events) = run(&mut off, &cfg(false, "normal"));
        assert_eq!(content, "done");
        assert!(!nudged(&off), "disabled gate must never nudge");

        let mut plan = Bridge::new(vec![Step::Reply("plan only")]);
        let (content, _events) = run(&mut plan, &cfg(true, "plan"));
        assert_eq!(content, "plan only");
        assert!(!nudged(&plan), "read-only modes never arm the gate");
    }
}
