from __future__ import annotations

from collections.abc import AsyncIterator, Sequence
import json
import re
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


_ATTACHED_IMAGE_RE = re.compile(
    r'\[attached_image name="[^"]*" mime="([^"]+)" data="(data:[^"]+)"\]'
)


def _split_multimodal_content(content: str) -> str | list[dict[str, Any]]:
    matches = list(_ATTACHED_IMAGE_RE.finditer(content))
    if not matches:
        return content
    parts: list[dict[str, Any]] = []
    for m in matches:
        parts.append({"type": "image_url", "image_url": {"url": m.group(2)}})
    cleaned = _ATTACHED_IMAGE_RE.sub("", content).strip()
    if cleaned:
        parts.append({"type": "text", "text": cleaned})
    return parts


def build_vibe_payload(
    model: ModelSpec,
    context: TranscriptContext,
    options: StreamOptions,
) -> dict[str, Any]:
    messages: list[dict[str, Any]] = []
    if context.system_prompt:
        messages.append({"role": "system", "content": context.system_prompt})

    for msg in context.messages:
        content: Any = _split_multimodal_content(msg.content) if msg.role == "user" else msg.content
        entry: dict[str, Any] = {"role": msg.role, "content": content}
        if msg.role == "assistant" and msg.tool_calls:
            entry["tool_calls"] = [
                {
                    "id": tc.id,
                    "type": "function",
                    "function": {"name": tc.name, "arguments": json.dumps(tc.arguments)},
                }
                for tc in msg.tool_calls
            ]
        elif msg.role == "tool":
            if msg.tool_call_id:
                entry["tool_call_id"] = msg.tool_call_id
            if msg.name:
                entry["name"] = msg.name
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
                },
            }
            for t in context.tools
        ]
    if model.reasoning and options.thinking_level != "off":
        mapped = model.thinking_level_map.get(options.thinking_level, options.thinking_level)
        if mapped != "none":
            payload["reasoning_effort"] = mapped
    return payload


class VibeProvider:
    id: ProviderId = "vibe"
    name: str = "Mistral Vibe"

    def __init__(self, client: httpx.AsyncClient | None = None) -> None:
        self._client = client

    def list_models(self) -> Sequence[ModelSpec]:
        return list_models("vibe")

    def resolve_auth(self, explicit_key: str | None = None) -> AuthCredential:
        return resolve_api_key("vibe", explicit_key=explicit_key)

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
            "User-Agent": "le-vibe/0.1.0",
            **cred.extra_headers,
            **opts.extra_headers,
        }
        url = f"{model.base_url.rstrip('/')}/chat/completions"
        payload = build_vibe_payload(model, context, opts)

        text_parts: list[str] = []
        reasoning_parts: list[str] = []
        tool_buffers: dict[int, dict[str, str]] = {}
        stop_reason = "stop"
        usage = Usage()

        async for chunk in stream_sse_json(
            url, headers=headers, payload=payload, timeout_s=opts.timeout_s, client=self._client
        ):
            if raw_usage := chunk.get("usage"):
                in_tok = int(raw_usage.get("prompt_tokens", 0))
                out_tok = int(raw_usage.get("completion_tokens", 0))
                cache_read = int(raw_usage.get("prompt_tokens_details", {}).get("cached_tokens", 0) or 0)
                usage = Usage(
                    input_tokens=in_tok,
                    output_tokens=out_tok,
                    cache_read_tokens=cache_read,
                    cost_usd=calculate_cost_usd(
                        input_tokens=max(0, in_tok - cache_read),
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

            if reasoning := delta.get("reasoning_content"):
                if isinstance(reasoning, str) and reasoning:
                    reasoning_parts.append(reasoning)
                    yield StreamEvent(type="thinking_delta", delta=reasoning)

            content = delta.get("content")
            if isinstance(content, str) and content:
                text_parts.append(content)
                yield StreamEvent(type="text_delta", delta=content)
            elif isinstance(content, list):
                for block in content:
                    if isinstance(block, dict):
                        if block.get("type") == "thinking" and (thinking := block.get("thinking")):
                            for t_part in thinking:
                                if isinstance(t_part, dict) and (txt := t_part.get("text")):
                                    reasoning_parts.append(txt)
                                    yield StreamEvent(type="thinking_delta", delta=txt)
                        elif block.get("type") == "text" and (txt := block.get("text")):
                            text_parts.append(txt)
                            yield StreamEvent(type="text_delta", delta=txt)

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
            raw_args = buf["arguments"].strip()
            if raw_args:
                try:
                    parsed_args = json.loads(raw_args)
                    if not isinstance(parsed_args, dict):
                        parsed_args = {"value": parsed_args}
                except json.JSONDecodeError:
                    parsed_args = {"_raw_arguments": raw_args}
            else:
                parsed_args = {}
            call = ToolCall(id=buf["id"] or f"call_{idx}", name=buf["name"], arguments=parsed_args)
            tool_calls.append(call)
            yield StreamEvent(type="tool_call", tool_call=call)

        message = AssistantMessage(
            provider="vibe",
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
            raise RuntimeError("Vibe stream ended without a final message")
        return final
