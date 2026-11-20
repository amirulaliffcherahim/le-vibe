from __future__ import annotations

from collections.abc import AsyncIterator, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any, Literal, Protocol

ProviderId = Literal["vibe", "deepseek", "agy"]
ThinkingLevel = Literal["off", "minimal", "low", "medium", "high", "xhigh", "max"]
Role = Literal["system", "user", "assistant", "tool"]


@dataclass(frozen=True, slots=True)
class ModelCost:
    input: float
    output: float
    cache_read: float = 0.0
    cache_write: float = 0.0


@dataclass(frozen=True, slots=True)
class ModelCompat:
    supports_store: bool = False
    supports_developer_role: bool = False
    max_tokens_field: str = "max_tokens"
    requires_reasoning_content_on_assistant_messages: bool = False
    thinking_format: str | None = None
    supports_strict_mode: bool = True
    supports_mid_convo_system_messages: bool = False


@dataclass(frozen=True, slots=True)
class ModelSpec:
    id: str
    name: str
    provider: ProviderId
    base_url: str
    reasoning: bool = False
    input_modalities: tuple[str, ...] = ("text",)
    context_window: int = 128_000
    max_tokens: int = 32_768
    cost: ModelCost = field(default_factory=lambda: ModelCost(0.0, 0.0))
    thinking_level_map: Mapping[str, str] = field(default_factory=dict)
    compat: ModelCompat = field(default_factory=ModelCompat)
    recommended: bool = False
    #: The provider reports this model as out of quota right now.
    exhausted: bool = False


@dataclass(frozen=True, slots=True)
class ToolCall:
    id: str
    name: str
    arguments: dict[str, Any]
    thought_signature: str | None = None


@dataclass(frozen=True, slots=True)
class Message:
    role: Role
    content: str = ""
    reasoning_content: str | None = None
    tool_calls: tuple[ToolCall, ...] = ()
    tool_call_id: str | None = None
    name: str | None = None


@dataclass(frozen=True, slots=True)
class ToolDefinition:
    name: str
    description: str
    parameters: dict[str, Any]


@dataclass(frozen=True, slots=True)
class TranscriptContext:
    messages: Sequence[Message]
    system_prompt: str | None = None
    tools: Sequence[ToolDefinition] = ()


@dataclass(frozen=True, slots=True)
class StreamOptions:
    api_key: str | None = None
    temperature: float | None = None
    max_tokens: int | None = None
    thinking_level: ThinkingLevel = "medium"
    timeout_s: float = 120.0
    extra_headers: Mapping[str, str] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class Usage:
    input_tokens: int = 0
    output_tokens: int = 0
    cache_read_tokens: int = 0
    cache_write_tokens: int = 0
    cost_usd: float = 0.0


@dataclass(frozen=True, slots=True)
class AssistantMessage:
    provider: ProviderId
    model: str
    content: str
    reasoning_content: str | None = None
    tool_calls: tuple[ToolCall, ...] = ()
    stop_reason: str = "stop"
    usage: Usage = field(default_factory=Usage)


@dataclass(frozen=True, slots=True)
class StreamEvent:
    type: Literal["text_delta", "thinking_delta", "tool_call", "done", "error"]
    delta: str = ""
    tool_call: ToolCall | None = None
    message: AssistantMessage | None = None
    error: str | None = None


@dataclass(frozen=True, slots=True)
class AuthCredential:
    provider: ProviderId
    token: str
    source: Literal["env", "explicit", "saved", "oidc", "keyring"] = "env"
    extra_headers: Mapping[str, str] = field(default_factory=dict)
    project_id: str | None = None


class ProviderAdapter(Protocol):
    id: ProviderId
    name: str

    def list_models(self) -> Sequence[ModelSpec]: ...

    def resolve_auth(self, explicit_key: str | None = None) -> AuthCredential: ...

    def stream(
        self,
        model: ModelSpec,
        context: TranscriptContext,
        options: StreamOptions | None = None,
    ) -> AsyncIterator[StreamEvent]: ...

    async def complete(
        self,
        model: ModelSpec,
        context: TranscriptContext,
        options: StreamOptions | None = None,
    ) -> AssistantMessage: ...
