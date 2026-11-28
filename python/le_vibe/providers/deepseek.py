from __future__ import annotations

from collections.abc import AsyncIterator, Sequence
import json
from typing import Any

import httpx

from le_vibe.providers.auth import resolve_api_key
from le_vibe.providers.catalog import list_models
from le_vibe.providers.transport import calculate_cost_usd, stream_sse_json
from le_vibe.providers.types import (
    AssistantMessage,
    AuthCredential,
    ModelSpec,
    ProviderId,
    StreamEvent,
    StreamOptions,
    ToolCall,
    TranscriptContext,
    Usage,
)


def build_deepseek_payload(
    model: ModelSpec,
    context: TranscriptContext,
    options: StreamOptions,
) -> dict[str, Any]:
    messages: list[dict[str, Any]] = []
    if context.system_prompt:
        messages.append({"role": "system", "content": context.system_prompt})

    for msg in context.messages:
        entry: dict[str, Any] = {"role": msg.role, "content": msg.content}
        if msg.role == "assistant":
            if msg.tool_calls:
                entry["tool_calls"] = [
                    {
                        "id": tc.id,
                        "type": "function",
                        "function": {"name": tc.name, "arguments": json.dumps(tc.arguments)},
                    }
                    for tc in msg.tool_calls
                ]
            if model.compat.requires_reasoning_content_on_assistant_messages and msg.reasoning_content is not None:
                entry["reasoning_content"] = msg.reasoning_content
        elif msg.role == "tool" and msg.tool_call_id:
            entry["tool_call_id"] = msg.tool_call_id
        messages.append(entry)

    payload: dict[str, Any] = {
        "model": model.id,
        "messages": messages,
        "stream": True,
        "max_tokens": options.max_tokens or model.max_tokens,
    }
    if options.temperature is not None:
        payload["temperature"] = options.temperature
    if context.tools:
        payload["tools"] = [
            {
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                    "strict": model.compat.supports_strict_mode,
                },
            }
            for t in context.tools
        ]
    if model.reasoning and options.thinking_level != "off":
        mapped = model.thinking_level_map.get(options.thinking_level, options.thinking_level)
        if mapped != "none":
            payload["thinking"] = {"type": "enabled"}
            payload["reasoning_effort"] = mapped
    return payload


class DeepSeekProvider:
    id: ProviderId = "deepseek"
    name: str = "DeepSeek"

    def __init__(self, client: httpx.AsyncClient | None = None) -> None:
        self._client = client

    def list_models(self) -> Sequence[ModelSpec]:
        return list_models("deepseek")

    def resolve_auth(self, explicit_key: str | None = None) -> AuthCredential:
        return resolve_api_key("deepseek", explicit_key=explicit_key)

    async def stream(
        self,
        model: ModelSpec,
        context: TranscriptContext,
        options: StreamOptions | None = None,
    ) -> AsyncIterator[StreamEvent]:
        opts = options or StreamOptions()
        cred = self.resolve_auth(opts.api_key)
        headers = {
            "Authorization": f"Bearer {cred.token}",
            "Content-Type": "application/json",
            **opts.extra_headers,
        }
        url = f"{model.base_url.rstrip('/')}/v1/chat/completions"
        payload = build_deepseek_payload(model, context, opts)

        text_parts: list[str] = []
        reasoning_parts: list[str] = []
        tool_buffers: dict[int, dict[str, str]] = {}
        stop_reason = "stop"
        usage = Usage()

        async for chunk in stream_sse_json(
            url,
            headers=headers,
            payload=payload,
            timeout_s=opts.timeout_s,
            client=self._client,
            max_retry_elapsed_s=0.0,
            provider="deepseek",
            model=model.id,
        ):
            if raw_usage := chunk.get("usage"):
                in_tok = int(raw_usage.get("prompt_tokens", 0))
                out_tok = int(raw_usage.get("completion_tokens", 0))
                cache_read = int(
                    raw_usage.get("prompt_cache_hit_tokens")
                    or raw_usage.get("prompt_tokens_details", {}).get("cached_tokens", 0)
                    or 0
                )
                cache_write = int(
                    raw_usage.get("cache_creation_input_tokens")
                    or raw_usage.get("prompt_tokens_details", {}).get("cache_write_tokens", 0)
                    or 0
                )
                reasoning_tok = int(
                    (raw_usage.get("completion_tokens_details") or {}).get("reasoning_tokens", 0)
                    or 0
                )
                uncached_in = max(0, in_tok - cache_read)
                usage = Usage(
                    input_tokens=uncached_in,
                    output_tokens=out_tok,
                    cache_read_tokens=cache_read,
                    cache_write_tokens=cache_write,
                    reasoning_tokens=reasoning_tok,
                    cost_usd=calculate_cost_usd(
                        input_tokens=uncached_in,
                        output_tokens=out_tok,
                        cache_read_tokens=cache_read,
                        input_per_m=model.cost.input,
                        output_per_m=model.cost.output,
                        cache_read_per_m=model.cost.cache_read,
                    ),
                )

            choices = chunk.get("choices") or []
            if not choices:
                continue
            choice = choices[0]
            if finish := choice.get("finish_reason"):
                stop_reason = str(finish)
            delta = choice.get("delta") or {}

            if r_delta := delta.get("reasoning_content"):
                reasoning_parts.append(r_delta)
                yield StreamEvent(type="thinking_delta", delta=r_delta)

            if c_delta := delta.get("content"):
                text_parts.append(c_delta)
                yield StreamEvent(type="text_delta", delta=c_delta)

            for tc in delta.get("tool_calls") or []:
                idx = int(tc.get("index", 0))
                buf = tool_buffers.setdefault(idx, {"id": "", "name": "", "arguments": ""})
                if tc_id := tc.get("id"):
                    buf["id"] = tc_id
                fn = tc.get("function") or {}
                if fn_name := fn.get("name"):
                    buf["name"] += fn_name
                if fn_args := fn.get("arguments"):
                    buf["arguments"] += fn_args

        tool_calls: list[ToolCall] = []
        for idx in sorted(tool_buffers):
            buf = tool_buffers[idx]
            parsed_args = json.loads(buf["arguments"]) if buf["arguments"].strip() else {}
            call = ToolCall(id=buf["id"] or f"call_{idx}", name=buf["name"], arguments=parsed_args)
            tool_calls.append(call)
            yield StreamEvent(type="tool_call", tool_call=call)

        message = AssistantMessage(
            provider="deepseek",
            model=model.id,
            content="".join(text_parts),
            reasoning_content="".join(reasoning_parts) if reasoning_parts else None,
            tool_calls=tuple(tool_calls),
            stop_reason=stop_reason,
            usage=usage,
        )
        yield StreamEvent(type="done", message=message)

    async def complete(
        self,
        model: ModelSpec,
        context: TranscriptContext,
        options: StreamOptions | None = None,
    ) -> AssistantMessage:
        final: AssistantMessage | None = None
        async for event in self.stream(model, context, options):
            if event.type == "done" and event.message is not None:
                final = event.message
        if final is None:
            raise RuntimeError("DeepSeek stream ended without a final message")
        return final
