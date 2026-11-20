use le_vibe_durable::SessionStore;
use le_vibe_pybridge::{
    AssistantMessage, ChatMessage, CompletionRequest, PolicyPort, ProviderPort,
};
use le_vibe_telemetry::TelemetryRecorder;
use le_vibe_tools::{
    TodoStore, ToolError, builtin_tool_definitions, execute_builtin_tool,
    execute_builtin_tool_approved, execute_todo_tool,
};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("provider bridge error: {0}")]
    Bridge(#[from] le_vibe_pybridge::BridgeError),
    #[error("session error: {0}")]
    Session(#[from] le_vibe_durable::SessionError),
    #[error("max turns exceeded ({0})")]
    MaxTurnsExceeded(usize),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    TurnStart { turn: usize },
    ThinkingDelta { delta: String },
    TextDelta { delta: String },
    ToolCallStart { id: String, name: String },
    ToolCallEnd { id: String, name: String, is_error: bool, output: String },
    TurnEnd { stop_reason: String },
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub command: Option<String>,
    pub target_path: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    AllowOnce,
    AllowSession,
    AllowYolo,
    Deny,
}

pub struct AgentConfig {
    pub provider: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub thinking_level: String,
    pub max_turns: usize,
    pub allowed_tools: Option<Vec<String>>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            provider: "deepseek".to_string(),
            model: "deepseek-v4-pro".to_string(),
            system_prompt: None,
            thinking_level: "medium".to_string(),
            max_turns: 8,
            allowed_tools: None,
        }
    }
}

pub fn run_turn_loop<B: ProviderPort + PolicyPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    config: &AgentConfig,
    user_prompt: &str,
) -> Result<AssistantMessage, AgentError> {
    run_turn_loop_streaming(bridge, session, telemetry, config, user_prompt, |_| {})
}

pub fn run_turn_loop_streaming<B: ProviderPort + PolicyPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    config: &AgentConfig,
    user_prompt: &str,
    on_event: impl FnMut(AgentEvent),
) -> Result<AssistantMessage, AgentError> {
    let mut todos = TodoStore::new();
    run_turn_loop_interactive(
        bridge,
        session,
        telemetry,
        &mut todos,
        config,
        user_prompt,
        on_event,
        |_| ApprovalChoice::AllowOnce,
        |_| "No interactive UI available.".to_string(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_turn_loop_interactive<B: ProviderPort + PolicyPort>(
    bridge: &mut B,
    session: &mut SessionStore,
    telemetry: &TelemetryRecorder,
    todos: &mut TodoStore,
    config: &AgentConfig,
    user_prompt: &str,
    mut on_event: impl FnMut(AgentEvent),
    mut on_approval: impl FnMut(ApprovalRequest) -> ApprovalChoice,
    mut on_ask_user: impl FnMut(Value) -> String,
) -> Result<AssistantMessage, AgentError> {
    session.append("user", user_prompt)?;
    let mut messages: Vec<ChatMessage> = session
        .active_branch()
        .into_iter()
        .map(|entry| ChatMessage {
            role: entry.role,
            content: entry.content,
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
        })
        .collect();

    let tools: Vec<_> = builtin_tool_definitions()
        .into_iter()
        .filter(|t| {
            config
                .allowed_tools
                .as_ref()
                .is_none_or(|allow| allow.iter().any(|a| a == &t.name))
        })
        .collect();

    for turn in 0..config.max_turns {
        on_event(AgentEvent::TurnStart { turn });
        let span = telemetry.record_span("agent.turn");
        let reply = bridge.stream_complete(
            CompletionRequest {
                provider: config.provider.clone(),
                model: config.model.clone(),
                messages: messages.clone(),
                system_prompt: config.system_prompt.clone(),
                tools: tools.clone(),
                thinking_level: config.thinking_level.clone(),
            },
            &mut |ev| match ev.kind.as_str() {
                "thinking_delta" => on_event(AgentEvent::ThinkingDelta { delta: ev.delta }),
                "text_delta" => on_event(AgentEvent::TextDelta { delta: ev.delta }),
                _ => {}
            },
        )?;
        telemetry.finish_span(span);

        if reply.tool_calls.is_empty() {
            session.append("assistant", &reply.content)?;
            on_event(AgentEvent::TurnEnd {
                stop_reason: reply.stop_reason.clone(),
            });
            return Ok(reply);
        }

        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: reply.content.clone(),
            reasoning_content: reply.reasoning_content.clone(),
            tool_calls: reply.tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        for tc in &reply.tool_calls {
            on_event(AgentEvent::ToolCallStart {
                id: tc.id.clone(),
                name: tc.name.clone(),
            });
            let (output, is_error) = if tc.name == "todo" {
                match execute_todo_tool(todos, &tc.arguments) {
                    Ok(out) => (out, false),
                    Err(err) => (format!("Tool error: {err}"), true),
                }
            } else if tc.name == "ask_user_question" {
                (on_ask_user(tc.arguments.clone()), false)
            } else {
                match execute_builtin_tool(bridge, &tc.name, &tc.arguments) {
                    Ok(out) => (out, false),
                    Err(ToolError::PolicyAsk(reason)) => {
                        let cmd = tc
                            .arguments
                            .get("command")
                            .and_then(Value::as_str)
                            .map(ToString::to_string);
                        let path = tc
                            .arguments
                            .get("path")
                            .and_then(Value::as_str)
                            .map(ToString::to_string);
                        let choice = on_approval(ApprovalRequest {
                            tool_name: tc.name.clone(),
                            command: cmd.clone(),
                            target_path: path.clone(),
                            reason,
                        });
                        match choice {
                            ApprovalChoice::AllowOnce => {
                                match execute_builtin_tool_approved(&tc.name, &tc.arguments) {
                                    Ok(out) => (out, false),
                                    Err(err) => (format!("Tool error: {err}"), true),
                                }
                            }
                            ApprovalChoice::AllowSession => {
                                let _ = bridge.grant_rule(
                                    &tc.name,
                                    cmd.as_deref(),
                                    path.as_deref(),
                                );
                                match execute_builtin_tool_approved(&tc.name, &tc.arguments) {
                                    Ok(out) => (out, false),
                                    Err(err) => (format!("Tool error: {err}"), true),
                                }
                            }
                            ApprovalChoice::AllowYolo => {
                                let _ = bridge.set_yolo(true);
                                match execute_builtin_tool_approved(&tc.name, &tc.arguments) {
                                    Ok(out) => (out, false),
                                    Err(err) => (format!("Tool error: {err}"), true),
                                }
                            }
                            ApprovalChoice::Deny => {
                                ("Tool call denied by user.".to_string(), true)
                            }
                        }
                    }
                    Err(err) => (format!("Tool error: {err}"), true),
                }
            };
            on_event(AgentEvent::ToolCallEnd {
                id: tc.id.clone(),
                name: tc.name.clone(),
                is_error,
                output: output.clone(),
            });
            messages.push(ChatMessage {
                role: "tool".to_string(),
                content: output,
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: Some(tc.id.clone()),
                name: Some(tc.name.clone()),
            });
        }
    }

    Err(AgentError::MaxTurnsExceeded(config.max_turns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use le_vibe_pybridge::{BridgeError, ModelSpec, PolicyDecision, ToolCall, Usage};
    use serde_json::json;

    struct MockBridge {
        step: usize,
    }

    impl ProviderPort for MockBridge {
        fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
            Ok(vec!["vibe".into(), "deepseek".into(), "agy".into()])
        }
        fn list_models(&mut self, _provider: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
            Ok(vec![])
        }
        fn complete(&mut self, _request: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
            self.step += 1;
            if self.step == 1 {
                Ok(AssistantMessage {
                    provider: "deepseek".into(),
                    model: "deepseek-v4-pro".into(),
                    content: "".into(),
                    reasoning_content: Some("Need to read".into()),
                    tool_calls: vec![ToolCall {
                        id: "tc_1".into(),
                        name: "read".into(),
                        arguments: json!({"path": "nonexistent.txt"}),
                        thought_signature: None,
                    }],
                    stop_reason: "tool_calls".into(),
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cost_usd: 0.0,
                    },
                })
            } else {
                Ok(AssistantMessage {
                    provider: "deepseek".into(),
                    model: "deepseek-v4-pro".into(),
                    content: "Handled missing file.".into(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    stop_reason: "stop".into(),
                    usage: Usage {
                        input_tokens: 20,
                        output_tokens: 10,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cost_usd: 0.0,
                    },
                })
            }
        }
    }

    impl PolicyPort for MockBridge {
        fn evaluate_tool_call(
            &mut self,
            _tool_name: &str,
            _command: Option<&str>,
            _target_path: Option<&str>,
        ) -> Result<PolicyDecision, BridgeError> {
            Ok(PolicyDecision {
                action: "allow".into(),
                reason: "ok".into(),
            })
        }
    }

    #[test]
    fn executes_tool_call_and_emits_streaming_events() {
        let mut bridge = MockBridge { step: 0 };
        let mut session = SessionStore::in_memory();
        let telemetry = TelemetryRecorder::new();
        let cfg = AgentConfig::default();

        let mut events = Vec::new();
        let msg = run_turn_loop_streaming(&mut bridge, &mut session, &telemetry, &cfg, "check file", |ev| {
            events.push(ev);
        })
        .unwrap();
        assert_eq!(msg.content, "Handled missing file.");
        assert_eq!(session.entries().len(), 2);
        assert_eq!(telemetry.spans().len(), 2);
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolCallStart { name, .. } if name == "read")));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnEnd { .. })));
    }

    #[test]
    fn allowed_tools_filters_mutating_tools_and_deny_blocks_execution() {
        struct AskWriteBridge {
            step: usize,
            seen_tools: Vec<String>,
        }
        impl ProviderPort for AskWriteBridge {
            fn list_providers(&mut self) -> Result<Vec<String>, BridgeError> {
                Ok(vec!["deepseek".into()])
            }
            fn list_models(&mut self, _p: Option<&str>) -> Result<Vec<ModelSpec>, BridgeError> {
                Ok(vec![])
            }
            fn complete(&mut self, req: CompletionRequest) -> Result<AssistantMessage, BridgeError> {
                self.seen_tools = req.tools.iter().map(|t| t.name.clone()).collect();
                self.step += 1;
                if self.step == 1 {
                    Ok(AssistantMessage {
                        provider: "deepseek".into(),
                        model: "deepseek-v4-pro".into(),
                        content: String::new(),
                        reasoning_content: None,
                        tool_calls: vec![ToolCall {
                            id: "tc_deny".into(),
                            name: "write".into(),
                            arguments: json!({"path": "should_not_exist.txt", "content": "x"}),
                            thought_signature: None,
                        }],
                        stop_reason: "tool_calls".into(),
                        usage: Usage {
                            input_tokens: 1,
                            output_tokens: 1,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            cost_usd: 0.0,
                        },
                    })
                } else {
                    Ok(AssistantMessage {
                        provider: "deepseek".into(),
                        model: "deepseek-v4-pro".into(),
                        content: "Denied cleanly.".into(),
                        reasoning_content: None,
                        tool_calls: vec![],
                        stop_reason: "stop".into(),
                        usage: Usage {
                            input_tokens: 1,
                            output_tokens: 1,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            cost_usd: 0.0,
                        },
                    })
                }
            }
        }
        impl PolicyPort for AskWriteBridge {
            fn evaluate_tool_call(
                &mut self,
                _tool_name: &str,
                _command: Option<&str>,
                _target_path: Option<&str>,
            ) -> Result<PolicyDecision, BridgeError> {
                Ok(PolicyDecision {
                    action: "ask".into(),
                    reason: "requires approval".into(),
                })
            }
        }

        let mut bridge = AskWriteBridge {
            step: 0,
            seen_tools: Vec::new(),
        };
        let mut session = SessionStore::in_memory();
        let telemetry = TelemetryRecorder::new();
        let mut todos = TodoStore::new();
        let cfg = AgentConfig {
            allowed_tools: Some(vec!["read".into(), "grep".into(), "ls".into()]),
            ..Default::default()
        };

        let mut tool_outputs = Vec::new();
        let msg = run_turn_loop_interactive(
            &mut bridge,
            &mut session,
            &telemetry,
            &mut todos,
            &cfg,
            "try write",
            |ev| {
                if let AgentEvent::ToolCallEnd { output, is_error, .. } = ev {
                    tool_outputs.push((output, is_error));
                }
            },
            |_| ApprovalChoice::Deny,
            |_| String::new(),
        )
        .unwrap();

        assert_eq!(msg.content, "Denied cleanly.");
        assert_eq!(bridge.seen_tools, vec!["read", "grep", "ls"]);
        assert_eq!(tool_outputs.len(), 1);
        assert!(tool_outputs[0].1);
        assert_eq!(tool_outputs[0].0, "Tool call denied by user.");
        assert!(!std::path::Path::new("should_not_exist.txt").exists());
    }
}
