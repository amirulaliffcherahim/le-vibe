from __future__ import annotations

from collections.abc import AsyncIterator, Sequence
import time
import re
from typing import Any
import uuid

import httpx

from le_vibe.providers.auth import AGY_USER_AGENT, get_agy_project_id, resolve_api_key
from le_vibe.providers.catalog import list_models
from le_vibe.providers.transport import stream_sse_json
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


_AGY_ATTACHED_IMAGE_RE = re.compile(
    r'\[attached_image name="[^"]*" mime="([^"]+)" data="data:[^;]+;base64,([^"]+)"\]'
)


def resolve_agy_runtime_model(model: ModelSpec, thinking_level: str) -> str:
    """Map a base model id plus a thinking level onto the backend runtime id."""
    return (
        model.thinking_level_map.get(thinking_level)
        or model.thinking_level_map.get("medium")
        or model.thinking_level_map.get("off")
        or model.id
    )


def build_agy_payload(
    model: ModelSpec,
    context: TranscriptContext,
    options: StreamOptions,
    *,
    project_id: str = "",
) -> dict[str, Any]:
    runtime_model = resolve_agy_runtime_model(model, options.thinking_level)
    contents: list[dict[str, Any]] = []
    for msg in context.messages:
        if msg.role == "tool":
            resp_part = {
                "functionResponse": {
                    "name": msg.name or "tool",
                    "response": {"output": msg.content},
                }
            }
            if contents and contents[-1]["role"] == "user" and all(
                "functionResponse" in p for p in contents[-1]["parts"]
            ):
                contents[-1]["parts"].append(resp_part)
            else:
                contents.append({"role": "user", "parts": [resp_part]})
            continue

        role = "model" if msg.role == "assistant" else "user"
        parts: list[dict[str, Any]] = []
        if msg.content:
            if role == "user" and "[attached_image " in msg.content:
                for m in _AGY_ATTACHED_IMAGE_RE.finditer(msg.content):
                    parts.append({"inlineData": {"mimeType": m.group(1), "data": m.group(2)}})
                cleaned = _AGY_ATTACHED_IMAGE_RE.sub("", msg.content).strip()
                if cleaned:
                    parts.append({"text": cleaned})
            else:
                parts.append({"text": msg.content})
        for tc in msg.tool_calls:
            fc_part: dict[str, Any] = {"functionCall": {"name": tc.name, "args": tc.arguments}}
            if tc.thought_signature:
                fc_part["thoughtSignature"] = tc.thought_signature
            elif runtime_model.startswith("gemini-"):
                fc_part["thoughtSignature"] = "skip_thought_signature_validator"
            parts.append(fc_part)
        contents.append({"role": role, "parts": parts or [{"text": ""}]})

    if not contents:
        contents.append({"role": "user", "parts": [{"text": "Apply the active system instructions."}]})

    is_claude = runtime_model.startswith("claude-")
    is_non_gemini = (
        is_claude
        or runtime_model.startswith("gpt-oss-")
        or not runtime_model.startswith("gemini-")
    )
    traj_id = str(uuid.uuid4())
    agent_id = str(uuid.uuid4())
    step = max(1, len(contents))
    last_step_index = str(max(0, step - 1))

    request: dict[str, Any] = {
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": options.max_tokens or model.max_tokens,
        },
        "sessionId": str(int(time.time() * 1000)),
        "labels": {
            "last_step_index": last_step_index,
            "request_id": f"{traj_id}-0",
            "trajectory_id": traj_id,
            "used_claude": "true" if is_claude else "false",
            "used_claude_conservative": "true" if is_claude else "false",
            "used_non_gemini_model": "true" if is_non_gemini else "false",
        },
    }
    if options.temperature is not None:
        request["generationConfig"]["temperature"] = options.temperature
    if context.system_prompt:
        request["systemInstruction"] = {"role": "user", "parts": [{"text": context.system_prompt}]}
    if context.tools:
        use_legacy_params = is_claude or runtime_model.startswith("gpt-oss-")
        request["tools"] = [
            {
                "functionDeclarations": [
                    {
                        "name": t.name,
                        "description": t.description,
                        **(
                            {"parameters": t.parameters}
                            if use_legacy_params
                            else {"parametersJsonSchema": t.parameters}
                        ),
                    }
                    for t in context.tools
                ]
            }
        ]

    return {
        "project": project_id,
        "model": runtime_model,
        "requestType": "AGENT",
        "userAgent": "ANTIGRAVITY",
        "requestId": f"agent/{agent_id}/{int(time.time() * 1000)}/{traj_id}/{step}",
        "request": request,
    }


class AgyProvider:
    id: ProviderId = "agy"
    name: str = "Antigravity (agy)"

    def __init__(self, client: httpx.AsyncClient | None = None) -> None:
        self._client = client

    def list_models(self) -> Sequence[ModelSpec]:
        return list_models("agy")

    def resolve_auth(self, explicit_key: str | None = None) -> AuthCredential:
        return resolve_api_key("agy", explicit_key=explicit_key)

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
            "User-Agent": AGY_USER_AGENT,
            **cred.extra_headers,
            **opts.extra_headers,
        }
        project_id = cred.project_id or get_agy_project_id()
        url = f"{model.base_url.rstrip('/')}/v1internal:streamGenerateContent?alt=sse"
        payload = build_agy_payload(model, context, opts, project_id=project_id)

        text_parts: list[str] = []
        reasoning_parts: list[str] = []
        tool_calls: list[ToolCall] = []
        stop_reason = "stop"
        usage = Usage()

        async for chunk in stream_sse_json(
            url, headers=headers, payload=payload, timeout_s=opts.timeout_s, client=self._client
        ):
            inner = chunk.get("response") or chunk
            if meta := inner.get("usageMetadata"):
                usage = Usage(
                    input_tokens=int(meta.get("promptTokenCount", 0)),
                    output_tokens=int(meta.get("candidatesTokenCount", 0)),
                    cache_read_tokens=int(meta.get("cachedContentTokenCount", 0)),
                )

            candidates = inner.get("candidates") or []
            if not candidates:
                continue
            cand = candidates[0]
            if finish := cand.get("finishReason"):
                stop_reason = str(finish).lower()
            parts = (cand.get("content") or {}).get("parts") or []
            for part in parts:
                if part.get("thought") and (txt := part.get("text")):
                    reasoning_parts.append(txt)
                    yield StreamEvent(type="thinking_delta", delta=txt)
                elif txt := part.get("text"):
                    text_parts.append(txt)
                    yield StreamEvent(type="text_delta", delta=txt)
                elif fc := part.get("functionCall"):
                    sig = part.get("thoughtSignature")
                    call = ToolCall(
                        id=f"agy_call_{len(tool_calls) + 1}",
                        name=str(fc.get("name", "")),
                        arguments=dict(fc.get("args") or {}),
                        thought_signature=str(sig) if isinstance(sig, str) and sig else None,
                    )
                    tool_calls.append(call)
                    yield StreamEvent(type="tool_call", tool_call=call)

        message = AssistantMessage(
            provider="agy",
            model=model.id,
            content="".join(text_parts),
            reasoning_content="".join(reasoning_parts) if reasoning_parts else None,
            tool_calls=tuple(tool_calls),
            stop_reason="tool_calls" if tool_calls else stop_reason,
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
            raise RuntimeError("Agy stream ended without a final message")
        return final
