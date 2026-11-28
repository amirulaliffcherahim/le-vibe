from __future__ import annotations

from collections.abc import AsyncIterator, Sequence
import hashlib
import os
import re
import time
from typing import Any
import uuid

import httpx

from le_vibe.providers.auth import (
    _agy_endpoint_candidates,
    _agy_headers,
    get_agy_project_id,
    resolve_api_key,
)
from le_vibe.providers.catalog import list_models
from le_vibe.providers.transport import (
    FIRST_OUTPUT_WAIT_SECONDS,
    STREAM_IDLE_SECONDS,
    StreamStalledError,
    stream_sse_json,
)
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
    is_claude = runtime_model.startswith("claude-")
    contents: list[dict[str, Any]] = []
    for msg in context.messages:
        if msg.role == "tool":
            fr_body: dict[str, Any] = {
                "name": msg.name or "tool",
                "response": {"output": msg.content},
            }
            if is_claude and msg.tool_call_id:
                fr_body["id"] = msg.tool_call_id
            resp_part = {"functionResponse": fr_body}
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
            fc_body: dict[str, Any] = {"name": tc.name, "args": tc.arguments}
            if is_claude and tc.id:
                fc_body["id"] = tc.id
            fc_part: dict[str, Any] = {"functionCall": fc_body}
            if tc.thought_signature:
                fc_part["thoughtSignature"] = tc.thought_signature
            elif runtime_model.startswith("gemini-"):
                fc_part["thoughtSignature"] = "skip_thought_signature_validator"
            parts.append(fc_part)
        contents.append({"role": role, "parts": parts or [{"text": ""}]})

    if not contents:
        contents.append({"role": "user", "parts": [{"text": "Apply the active system instructions."}]})

    is_non_gemini = (
        is_claude
        or runtime_model.startswith("gpt-oss-")
        or not runtime_model.startswith("gemini-")
    )
    # Derive a stable sessionId / trajectory_id / agent_id from the conversation's initial seed
    # so Google's KV-cache router pins every turn of the session to the same warm cache shard.
    first_user_text = next(
        (m.content[:256] for m in context.messages if m.role == "user" and m.content),
        "",
    )
    seed = f"{project_id}|{runtime_model}|{(context.system_prompt or '')[:256]}|{first_user_text}"
    digest = hashlib.sha256(seed.encode("utf-8", errors="ignore")).digest()
    traj_id = str(uuid.UUID(bytes=digest[:16]))
    agent_id = str(uuid.UUID(bytes=digest[16:32]))
    session_id = str(int.from_bytes(digest[:8], "big") & 0x7FFF_FFFF_FFFF_FFFF)
    step = max(1, len(contents))
    last_step_index = str(max(0, step - 1))

    request: dict[str, Any] = {
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": options.max_tokens or model.max_tokens,
        },
        "sessionId": session_id,
        "labels": {
            "last_step_index": last_step_index,
            "request_id": f"{traj_id}-{step}",
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
                            {"parameters": _sanitize_legacy_schema(t.parameters)}
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


_LEGACY_UNSUPPORTED_SCHEMA_KEYS = frozenset(
    {
        "$schema",
        "$id",
        "$defs",
        "additionalProperties",
        "minItems",
        "maxItems",
        "maxLength",
        "minLength",
        "exclusiveMinimum",
        "exclusiveMaximum",
    }
)


def _sanitize_legacy_schema(obj: Any) -> Any:
    """Strip JSON-Schema keywords and flatten list `type` fields rejected by Google's legacy OpenAPI `parameters` proto."""
    if isinstance(obj, dict):
        out: dict[str, Any] = {}
        for k, v in obj.items():
            if k in _LEGACY_UNSUPPORTED_SCHEMA_KEYS:
                continue
            if k == "type" and isinstance(v, list):
                non_null = [t for t in v if isinstance(t, str) and t != "null"]
                out["type"] = non_null[0] if non_null else "string"
                continue
            out[k] = _sanitize_legacy_schema(v)
        return out
    if isinstance(obj, list):
        return [_sanitize_legacy_schema(v) for v in obj]
    return obj


class AgyProvider:
    id: ProviderId = "agy"
    name: str = "Antigravity (agy)"

    def __init__(self, client: httpx.AsyncClient | None = None) -> None:
        self._client = client

    def list_models(self) -> Sequence[ModelSpec]:
        return list_models("agy")

    def resolve_auth(self, explicit_key: str | None = None) -> AuthCredential:
        return resolve_api_key("agy", explicit_key=explicit_key)

    def _build_stream_headers(
        self,
        cred: AuthCredential,
        *,
        project_id: str,
        endpoint: str,
        request_id: str,
        extra_headers: dict[str, str] | Any,
    ) -> dict[str, str]:
        is_aicode = "aicode.googleapis.com" in endpoint
        headers = _agy_headers(
            cred.token,
            project_id=project_id,
            request_id=request_id if is_aicode else "",
        )
        headers.update(cred.extra_headers)
        headers.update(extra_headers)
        return headers

    async def stream(
        self,
        model: ModelSpec,
        context: TranscriptContext,
        options: StreamOptions | None = None,
    ) -> AsyncIterator[StreamEvent]:
        opts = options or StreamOptions()
        cred = self.resolve_auth(opts.api_key)
        project_id = cred.project_id or get_agy_project_id()
        env_override = (
            os.environ.get("AGY_ENDPOINT_URL", "").strip()
            or os.environ.get("CLOUD_CODE_URL", "").strip()
            or os.environ.get("AICODE_ENDPOINT_URL", "").strip()
        )
        endpoint = env_override or model.base_url or _agy_endpoint_candidates()[0]
        url = f"{endpoint.rstrip('/')}/v1internal:streamGenerateContent?alt=sse"
        payload = build_agy_payload(model, context, opts, project_id=project_id)
        headers = self._build_stream_headers(
            cred,
            project_id=project_id,
            endpoint=endpoint,
            request_id=str(payload.get("requestId") or ""),
            extra_headers=opts.extra_headers,
        )

        async def _on_unauthorized() -> dict[str, str] | None:
            refreshed = resolve_api_key("agy", force_refresh=True)
            return self._build_stream_headers(
                refreshed,
                project_id=refreshed.project_id or project_id,
                endpoint=endpoint,
                request_id=str(payload.get("requestId") or ""),
                extra_headers=opts.extra_headers,
            )

        text_parts: list[str] = []
        reasoning_parts: list[str] = []
        tool_calls: list[ToolCall] = []
        stop_reason = "stop"
        usage = Usage()

        continuation_attempts = 0
        stream_payload = payload
        while True:
            yielded_any = False
            try:
                async for chunk in stream_sse_json(
                    url,
                    headers=headers,
                    payload=stream_payload,
                    timeout_s=opts.timeout_s,
                    client=self._client,
                    on_unauthorized=_on_unauthorized,
                    first_output_timeout_s=FIRST_OUTPUT_WAIT_SECONDS,
                    idle_timeout_s=STREAM_IDLE_SECONDS,
                    max_retry_elapsed_s=opts.timeout_s,
                ):
                    yielded_any = True
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
                            # Google returns the id for Claude-backed calls; keep it.
                            raw_id = fc.get("id")
                            call_id = (
                                str(raw_id)
                                if isinstance(raw_id, str) and raw_id.strip()
                                else f"agy_call_{len(tool_calls) + 1}"
                            )
                            call = ToolCall(
                                id=call_id,
                                name=str(fc.get("name", "")),
                                arguments=dict(fc.get("args") or {}),
                                thought_signature=str(sig) if isinstance(sig, str) and sig else None,
                            )
                            tool_calls.append(call)
                            yield StreamEvent(type="tool_call", tool_call=call)
                break
            except (StreamStalledError, RuntimeError) as exc:
                if not yielded_any or continuation_attempts >= 2:
                    raise
                continuation_attempts += 1
                # Resume with the partial assistant turn instead of restarting it.
                contents = list(payload["request"]["contents"])
                partial = "".join(text_parts)
                if partial:
                    contents.append({"role": "model", "parts": [{"text": partial}]})
                    contents.append(
                        {
                            "role": "user",
                            "parts": [{"text": "Continue exactly where you left off. Do not repeat text."}],
                        }
                    )
                stream_payload = {
                    **payload,
                    "request": {
                        **payload["request"],
                        "contents": contents,
                        "labels": {
                            **payload["request"].get("labels", {}),
                            "continuation": "true",
                        },
                    },
                }

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
