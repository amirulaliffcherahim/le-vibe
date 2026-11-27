//! P1 agent-loop parity with pi: agent-owned summarization, `keep_recent` retention, threshold
//! compaction, and single-shot overflow recovery.

use le_vibe_agent::{
    AgentConfig, AgentError, AgentEvent, TelemetryRecorder, compact, run_turn_loop_cancellable,
};
use le_vibe_durable::{EntryKind, SessionStore};
use le_vibe_pybridge::{
    AssistantMessage, BridgeError, CompletionRequest, ExtensionPort, ModelSpec, PolicyDecision,
    PolicyPort, ProviderPort, Usage,
};
use le_vibe_tools::TodoStore;
use serde_json::Value;
use std::sync::atomic::AtomicBool;

// ---------------------------------------------------------------------------
// Test bridge
// ---------------------------------------------------------------------------

/// What the fake provider should do for one `complete`/`stream_complete` call.
#[derive(Clone)]
enum Step {
    /// Return a normal assistant reply with the given prompt-token usage.
    Reply { text: String, input_tokens: u64 },
    /// Return a tool call so the loop runs another turn.
    ReadCall { input_tokens: u64 },
    /// Return a `length` stop with almost no output (MiMo style overflow).
    LengthStop { output_tokens: u64 },
    /// Fail with an overflow-shaped provider error.
    OverflowError,
    /// Fail with a plain transient error.
    Transient,
}

struct ScriptedBridge {
    /// Successive replies; the last one repeats once exhausted.
    script: Vec<Step>,
    /// Requests the loop actually sent, in order.
    requests: Vec<CompletionRequest>,
}

impl ScriptedBridge {
    fn new(script: Vec<Step>) -> Self {
        Self {
            script,
            requests: Vec::new(),
        }
    }

    fn next_step(&mut self) -> Step {
        let idx = self.requests.len().saturating_sub(1);
        self.script
            .get(idx)
            .or_else(|| self.script.last())
            .cloned()
            .unwrap_or(Step::Reply {
                text: "done".into(),
                input_tokens: 10,
            })
    }
}

fn reply(text: &str, input_tokens: u64, output_tokens: u64, stop_reason: &str) -> AssistantMessage {
    AssistantMessage {
        provider: "vibe".into(),
        model: "test-model".into(),
        content: text.into(),
        reasoning_content: None,
        tool_calls: vec![],
        stop_reason: stop_reason.into(),
        usage: Usage {
            input_tokens,
            output_tokens,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd: 0.0,
        },
    }
}

impl ProviderPort for ScriptedBridge {
    fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
        Ok(vec!["vibe".into()])
    }

    fn list_models(&mut self, _provider: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
        Ok(vec![])
    }

    fn complete(&mut self, request: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
        self.requests.push(request);
        match self.next_step() {
            Step::Reply { text, input_tokens } => Ok(reply(&text, input_tokens, 5, "stop")),
            Step::ReadCall { input_tokens } => {
                let mut msg = reply("reading", input_tokens, 5, "tool_calls");
                msg.tool_calls = vec![le_vibe_pybridge::ToolCall {
                    id: "call_1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({ "path": "/nonexistent/overflow-probe" }),
                    thought_signature: None,
                }];
                Ok(msg)
            }
            Step::LengthStop { output_tokens } => Ok(reply("", 999_000, output_tokens, "length")),
            Step::OverflowError => Err(BridgeError::Remote {
                kind: "RuntimeError".into(),
                message: "prompt is too long: 210000 tokens > 200000 maximum".into(),
            }),
            Step::Transient => Err(BridgeError::Remote {
                kind: "RuntimeError".into(),
                message: "HTTP 429 too many requests".into(),
            }),
        }
    }
}

impl PolicyPort for ScriptedBridge {
    fn evaluate_tool_call(
        &mut self,
        _tool_name: &str,
        _command: Option<&str>,
        _target_path: Option<&str>,
    ) -> Result<PolicyDecision, BridgeError> {
        Ok(PolicyDecision {
            action: "allow".into(),
            reason: String::new(),
        })
    }
}

impl ExtensionPort for ScriptedBridge {
    fn invoke_hook(&mut self, _event: &str, payload: Value) -> Result<Value, BridgeError> {
        Ok(payload)
    }
}

fn config() -> AgentConfig {
    AgentConfig {
        provider: "vibe".into(),
        model: "test-model".into(),
        system_prompt: Some("sys".into()),
        thinking_level: "low".into(),
        max_turns: 4,
        max_retries: 2,
        base_delay_ms: 1,
        max_delay_ms: 2,
        allowed_tools: None,
        mode_name: None,
        codemode: "off".into(),
        steering_queue: None,
        output_store: None,
        context_window: 200_000,
        reserve_tokens: compact::DEFAULT_RESERVE_TOKENS,
    }
}

fn seed_history(session: &mut SessionStore, count: usize, body_chars: usize) {
    let body = "x".repeat(body_chars);
    for i in 0..count {
        session
            .append("user", format!("entry{i} {body}"))
            .expect("append");
        session
            .append("assistant", format!("reply{i} {body}"))
            .expect("append");
    }
}

fn run(
    bridge: &mut ScriptedBridge,
    session: &mut SessionStore,
    cfg: &AgentConfig,
    prompt: &str,
) -> (Result<AssistantMessage, AgentError>, Vec<AgentEvent>) {
    let cancel = AtomicBool::new(false);
    let telemetry = TelemetryRecorder::new();
    let mut todos = TodoStore::new();
    let mut events = Vec::new();
    let res = run_turn_loop_cancellable(
        bridge,
        session,
        &telemetry,
        &mut todos,
        None,
        Some(&cancel),
        cfg,
        prompt,
        |ev| events.push(ev.clone()),
        |_| le_vibe_agent::ApprovalChoice::AllowOnce,
        |_| String::new(),
    );
    (res, events)
}

// ---------------------------------------------------------------------------
// 1. Agent-owned summarization
// ---------------------------------------------------------------------------

#[test]
fn summarize_retries_with_the_unified_schedule_and_summarizes_only_the_head() {
    let mut session = SessionStore::in_memory();
    seed_history(&mut session, 6, 400);
    // Newest marker that must never reach the summarizer prompt.
    session.append("user", "NEWEST_SHOULD_BE_KEPT").unwrap();

    // First two calls are transient failures, third succeeds.
    let mut bridge = ScriptedBridge::new(vec![
        Step::Transient,
        Step::Transient,
        Step::Reply {
            text: "SUMMARY_TEXT".into(),
            input_tokens: 10,
        },
    ]);

    let mut delays = Vec::new();
    let branch = session.active_branch();
    let mut options = compact::SummarizeOptions {
        provider: "vibe".into(),
        model: "test-model".into(),
        instruction: "summarize".into(),
        keep_recent_tokens: 1_000,
        max_retries: 3,
        base_delay_ms: 1,
        max_delay_ms: 2,
        cancel: None,
        on_retry: &mut |_, _, delay| delays.push(delay),
    };
    let plan = compact::summarize(&mut bridge, &branch, &mut options)
        .expect("summarize should recover from transient errors");

    assert_eq!(bridge.requests.len(), 3, "two retries then success");
    assert_eq!(delays.len(), 2);
    assert_eq!(plan.summary.trim(), "SUMMARY_TEXT");
    assert!(plan.summarized > 0 && plan.kept > 0, "plan: {plan:?}");

    // The prompt covers the head only; the retained tail stays verbatim in the session.
    let sent = &bridge.requests[0].messages[0].content;
    assert!(
        !sent.contains("NEWEST_SHOULD_BE_KEPT"),
        "kept tail must not be summarized:\n{sent}"
    );
    assert!(sent.contains("entry0"), "head must be summarized:\n{sent}");
}

#[test]
fn applying_a_summary_appends_it_and_keeps_the_tail_verbatim() {
    let mut session = SessionStore::in_memory();
    seed_history(&mut session, 4, 400);
    session.append("user", "TAIL_MARKER").unwrap();

    let branch = session.active_branch();
    let plan = compact::plan_cut(
        &branch
            .iter()
            .map(|e| le_vibe_agent::estimate_tokens(&e.content))
            .collect::<Vec<_>>(),
        50,
    );
    let summary = compact::SummaryPlan {
        summary: "COMPACTED".into(),
        keep_from: plan.keep_from,
        summarized: plan.summarized,
        kept: plan.kept,
        before_tokens: 1_000,
    };
    compact::apply_summary(&mut session, &branch, &summary).expect("apply");

    let active = session.active_branch();
    assert_eq!(
        active[0].kind,
        EntryKind::CompactionSummary,
        "compaction summary must become the branch root"
    );
    assert_eq!(active[0].content, "COMPACTED");
    let tail: Vec<&str> = active[1..].iter().map(|e| e.content.as_str()).collect();
    assert!(
        tail.iter().any(|c| c.contains("TAIL_MARKER")),
        "kept tail must survive compaction: {tail:?}"
    );
    assert!(
        !tail.iter().any(|c| c.contains("entry0")),
        "summarized head must be dropped: {tail:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. Threshold compaction inside the agent
// ---------------------------------------------------------------------------

#[test]
fn agent_compacts_on_threshold_before_the_next_request() {
    let mut session = SessionStore::in_memory();
    seed_history(&mut session, 3, 200);

    let mut cfg = config();
    // Tiny window so the first reply's usage trips `should_compact` immediately.
    cfg.context_window = 1_000;
    cfg.reserve_tokens = 200;
    cfg.base_delay_ms = 1;

    let mut bridge = ScriptedBridge::new(vec![
        // Turn 0: a tool call keeps the loop going, and its usage already sits inside the reserve.
        Step::ReadCall {
            input_tokens: 900,
        },
        // Turn 1's pre-request threshold check must fire and summarize before this request.
        Step::Reply {
            text: "summary".into(),
            input_tokens: 10,
        },
        Step::Reply {
            text: "final".into(),
            input_tokens: 10,
        },
    ]);

    let (res, events) = run(&mut bridge, &mut session, &cfg, "go");
    assert!(res.is_ok(), "turn should complete: {:?}", res.err());
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction { reason, .. } if reason == "threshold")),
        "expected a threshold compaction event, got {events:?}"
    );
    assert!(
        session
            .active_branch()
            .iter()
            .any(|e| e.kind == EntryKind::CompactionSummary),
        "a compaction summary entry must be persisted"
    );
}

// ---------------------------------------------------------------------------
// 3. Overflow recovery latch
// ---------------------------------------------------------------------------

#[test]
fn recovers_from_overflow_by_compacting_once_and_retrying() {
    let mut session = SessionStore::in_memory();
    seed_history(&mut session, 3, 200);

    let mut bridge = ScriptedBridge::new(vec![
        Step::OverflowError,
        Step::Reply {
            text: "summary".into(),
            input_tokens: 10,
        },
        Step::Reply {
            text: "recovered".into(),
            input_tokens: 10,
        },
    ]);

    let (res, events) = run(&mut bridge, &mut session, &config(), "go");
    let msg = res.expect("overflow recovery should succeed");
    assert_eq!(msg.content, "recovered");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction { reason, .. } if reason == "overflow")),
        "expected an overflow compaction event, got {events:?}"
    );
}

#[test]
fn second_overflow_on_the_same_turn_hard_fails_with_pi_message() {
    let mut session = SessionStore::in_memory();
    seed_history(&mut session, 3, 200);

    // Overflow, then a *successful* compaction, then the same turn overflows again: the latch has
    // already been spent, so this must hard-fail instead of compacting twice.
    let mut bridge = ScriptedBridge::new(vec![
        Step::OverflowError,
        Step::Reply {
            text: "summary".into(),
            input_tokens: 10,
        },
        Step::OverflowError,
    ]);

    let (res, _events) = run(&mut bridge, &mut session, &config(), "go");
    let err = res.expect_err("second overflow must fail the turn");
    let text = err.to_string();
    assert!(
        text.contains("Context overflow recovery failed after one compact-and-retry attempt"),
        "pi's overflow-recovery message required, got: {text}"
    );
}

#[test]
fn recoverable_length_stop_triggers_overflow_recovery() {
    let mut session = SessionStore::in_memory();
    seed_history(&mut session, 3, 200);

    let mut cfg = config();
    cfg.context_window = 1_000_000;
    let mut bridge = ScriptedBridge::new(vec![
        Step::LengthStop { output_tokens: 0 },
        Step::Reply {
            text: "summary".into(),
            input_tokens: 10,
        },
        Step::Reply {
            text: "after length".into(),
            input_tokens: 10,
        },
    ]);

    let (res, events) = run(&mut bridge, &mut session, &cfg, "go");
    let msg = res.expect("recoverable length stop should recover");
    assert_eq!(msg.content, "after length");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction { reason, .. } if reason == "overflow")),
        "length-stop recovery must report an overflow compaction, got {events:?}"
    );
}
