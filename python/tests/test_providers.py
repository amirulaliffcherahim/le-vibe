from __future__ import annotations

from pathlib import Path
import json
import pytest
import httpx

from le_vibe.providers import (
    Message,
    MissingAPIKeyError,
    StreamOptions,
    ToolCall,
    ToolDefinition,
    TranscriptContext,
    complete,
    get_model,
    get_provider,
    has_auth,
    default_model_id,
    list_models,
    list_providers,
    resolve_api_key,
)
from le_vibe.providers.agy import AgyProvider
from le_vibe.providers.deepseek import DeepSeekProvider, build_deepseek_payload
from le_vibe.providers.vibe import VibeProvider, build_vibe_payload


def test_only_vibe_deepseek_and_agy_providers_allowed() -> None:
    assert list_providers() == ("vibe", "deepseek", "agy")
    with pytest.raises(ValueError, match="Only 'vibe', 'deepseek', and 'agy' are allowed"):
        get_provider("anthropic")  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="Only 'vibe', 'deepseek', and 'agy' are allowed"):
        list_models("openai")  # type: ignore[arg-type]


def test_no_model_list_is_hardcoded() -> None:
    """With no catalog fetched and no cache, every provider reports zero models."""
    assert list_models("vibe") == ()
    assert list_models("deepseek") == ()
    assert list_models("agy") == ()
    assert list_models() == ()
    assert default_model_id("vibe") == ""


def _deepseek_test_model() -> "ModelSpec":
    from le_vibe.providers.types import ModelCompat, ModelCost, ModelSpec

    return ModelSpec(
        id="deepseek-v4-pro",
        name="DeepSeek V4 Pro",
        provider="deepseek",
        base_url="https://api.deepseek.com",
        reasoning=True,
        input_modalities=("text",),
        context_window=1_000_000,
        max_tokens=384_000,
        cost=ModelCost(input=1.32, output=3.96, cache_read=0.044),
        compat=ModelCompat(
            thinking_format="deepseek",
            requires_reasoning_content_on_assistant_messages=True,
            supports_mid_convo_system_messages=True,
        ),
    )


def _vibe_test_model() -> "ModelSpec":
    from le_vibe.providers.types import ModelCompat, ModelCost, ModelSpec

    return ModelSpec(
        id="mistral-medium-latest",
        name="Mistral Medium 3.5",
        provider="vibe",
        base_url="https://api.mistral.ai/v1",
        reasoning=True,
        input_modalities=("text", "image"),
        context_window=262_144,
        max_tokens=64_000,
        cost=ModelCost(input=0.4, output=2.0, cache_read=0.04),
        compat=ModelCompat(thinking_format="mistral"),
    )


def test_auth_resolution() -> None:
    cred = resolve_api_key("deepseek", env={"DEEPSEEK_API_KEY": "sk-ds-123"})
    assert cred.token == "sk-ds-123"
    assert cred.source == "env"

    assert has_auth("vibe", env={"MISTRAL_API_KEY": "sk-vibe-456"}, auth_file=Path("/nonexistent/auth.json")) is True
    assert has_auth("vibe", env={}, auth_file=Path("/nonexistent/auth.json")) is False

    with pytest.raises(MissingAPIKeyError):
        resolve_api_key("deepseek", env={}, auth_file=Path("/nonexistent/auth.json"))


def test_saved_auth_file_persistence(tmp_path) -> None:
    from le_vibe.providers import auth_status, delete_credential, save_api_key

    auth_file = tmp_path / "auth.json"
    save_api_key("vibe", "sk-vibe-saved", auth_file=auth_file)
    cred = resolve_api_key("vibe", env={}, auth_file=auth_file)
    assert cred.token == "sk-vibe-saved"
    assert cred.source == "saved"
    assert auth_status(env={}, auth_file=auth_file)["vibe"]["authenticated"] is True
    assert delete_credential("vibe", auth_file=auth_file) is True
    assert has_auth("vibe", env={}, auth_file=auth_file) is False


def test_oidc_token_persistence(tmp_path) -> None:
    import time
    from le_vibe.providers import save_oidc_tokens

    auth_file = tmp_path / "auth.json"
    save_oidc_tokens(
        "vibe",
        access_token="oidc-access-123",
        refresh_token="oidc-refresh-456",
        expires_at=time.time() + 3600,
        auth_file=auth_file,
    )
    cred = resolve_api_key("vibe", env={}, auth_file=auth_file)
    assert cred.token == "oidc-access-123"
    assert cred.source == "oidc"


def test_deepseek_payload_preserves_reasoning_content_for_tool_calls() -> None:
    model = _deepseek_test_model()
    ctx = TranscriptContext(
        system_prompt="Be concise.",
        messages=[
            Message(role="user", content="Read foo.txt"),
            Message(
                role="assistant",
                content="",
                reasoning_content="Need to read foo.txt first",
                tool_calls=(ToolCall(id="call_1", name="read", arguments={"path": "foo.txt"}),),
            ),
            Message(role="tool", content="hello", tool_call_id="call_1"),
        ],
        tools=[ToolDefinition(name="read", description="Read file", parameters={"type": "object"})],
    )
    payload = build_deepseek_payload(model, ctx, StreamOptions(thinking_level="high"))
    assistant_msg = payload["messages"][2]
    assert assistant_msg["reasoning_content"] == "Need to read foo.txt first"
    assert assistant_msg["tool_calls"][0]["function"]["name"] == "read"
    assert payload["thinking"] == {"type": "enabled"}


def test_vibe_payload_mapping() -> None:
    model = _vibe_test_model()
    ctx = TranscriptContext(messages=[Message(role="user", content="hi")])
    payload = build_vibe_payload(model, ctx, StreamOptions(thinking_level="medium"))
    assert payload["model"] == "mistral-medium-latest"
    assert payload["reasoning_effort"] == "medium"


@pytest.mark.asyncio
async def test_deepseek_stream_and_complete_with_mock_sse() -> None:
    sse_lines = "\n".join(
        [
            'data: {"choices":[{"delta":{"reasoning_content":"Thinking..."}}]}',
            'data: {"choices":[{"delta":{"content":"Hello "}}]}',
            'data: {"choices":[{"delta":{"content":"world","tool_calls":[{"index":0,"id":"tc_1","function":{"name":"read","arguments":"{\\"path\\":\\"a.txt\\"}"}}]}}]}',
            'data: {"choices":[{"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1000,"completion_tokens":100,"prompt_cache_hit_tokens":500}}',
            "data: [DONE]",
        ]
    )

    transport = httpx.MockTransport(lambda request: httpx.Response(200, text=sse_lines))
    async with httpx.AsyncClient(transport=transport) as client:
        provider = DeepSeekProvider(client=client)
        model = _deepseek_test_model()
        msg = await provider.complete(
            model,
            TranscriptContext(messages=[Message(role="user", content="hi")]),
            StreamOptions(api_key="test-key"),
        )

    assert msg.provider == "deepseek"
    assert msg.content == "Hello world"
    assert msg.reasoning_content == "Thinking..."
    assert msg.stop_reason == "tool_calls"
    assert len(msg.tool_calls) == 1
    assert msg.tool_calls[0].arguments == {"path": "a.txt"}
    assert msg.usage.cache_read_tokens == 500
    assert msg.usage.cost_usd > 0


@pytest.mark.asyncio
async def test_vibe_stream_with_mock_sse() -> None:
    sse_lines = "\n".join(
        [
            'data: {"choices":[{"delta":{"content":[{"type":"thinking","thinking":[{"text":"Plan "}]},{"type":"text","text":"Done"}]}}]}',
            'data: {"choices":[{"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":20}}',
            "data: [DONE]",
        ]
    )
    transport = httpx.MockTransport(lambda request: httpx.Response(200, text=sse_lines))
    async with httpx.AsyncClient(transport=transport) as client:
        provider = VibeProvider(client=client)
        model = _vibe_test_model()
        msg = await provider.complete(
            model,
            TranscriptContext(messages=[Message(role="user", content="hi")]),
            StreamOptions(api_key="test-key"),
        )

    assert msg.provider == "vibe"
    assert msg.content == "Done"
    assert msg.reasoning_content == "Plan "
    assert msg.stop_reason == "stop"


@pytest.mark.asyncio
async def test_agy_stream_with_mock_sse() -> None:
    sse_lines = "\n".join(
        [
            'data: {"response":{"candidates":[{"content":{"parts":[{"thought":true,"text":"Think "},{"text":"Answer"}]}}]}}',
            'data: {"response":{"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":50,"candidatesTokenCount":10}}}',
            "data: [DONE]",
        ]
    )
    transport = httpx.MockTransport(lambda request: httpx.Response(200, text=sse_lines))
    model = _grouped_flash_model()
    async with httpx.AsyncClient(transport=transport) as client:
        provider = AgyProvider(client=client)
        msg = await provider.complete(
            model,
            TranscriptContext(messages=[Message(role="user", content="hi")]),
            StreamOptions(api_key="test-token"),
        )

    assert msg.provider == "agy"
    assert msg.content == "Answer"
    assert msg.reasoning_content == "Think "
    assert msg.usage.input_tokens == 50
    assert msg.usage.output_tokens == 10


def test_agy_thinking_variants_collapse_into_base_models() -> None:
    from le_vibe.providers.catalog import _agy_group_models

    raw = {
        "gemini-3.8-flash-low": {"displayName": "Gemini 3.8 Flash (Low)", "supportsThinking": True},
        "gemini-3.8-flash-medium": {"displayName": "Gemini 3.8 Flash (Medium)"},
        "gemini-3.8-flash-high": {"displayName": "Gemini 3.8 Flash (High)"},
        "gemini-3.8-flash-tiered": {},
        "claude-opus-5-5-low": {"displayName": "Claude Opus 5.5 (Low)"},
        "claude-opus-5-5-high": {"displayName": "Claude Opus 5.5 (High)"},
        "gemini-3.1-flash-image": {"displayName": "Gemini 3.1 Flash Image"},
        "tab_flash_lite_preview": {},
    }
    models = _agy_group_models(raw, "https://example.invalid", default_runtime="gemini-3.8-flash-high")
    ids = [m.id for m in models]
    assert ids == ["claude-opus-5-5", "gemini-3.8-flash"]
    flash = next(m for m in models if m.id == "gemini-3.8-flash")
    assert flash.thinking_level_map["low"] == "gemini-3.8-flash-low"
    assert flash.thinking_level_map["medium"] == "gemini-3.8-flash-medium"
    assert flash.thinking_level_map["high"] == "gemini-3.8-flash-high"
    assert flash.thinking_level_map["off"] == "gemini-3.8-flash-tiered"


def _grouped_flash_model():
    from le_vibe.providers.catalog import _agy_group_models

    raw = {
        "gemini-3.8-flash-low": {"displayName": "Gemini 3.8 Flash (Low)"},
        "gemini-3.8-flash-medium": {"displayName": "Gemini 3.8 Flash (Medium)"},
        "gemini-3.8-flash-high": {"displayName": "Gemini 3.8 Flash (High)"},
        "gemini-3.8-flash-tiered": {},
    }
    models = _agy_group_models(raw, "https://example.invalid")
    return next(m for m in models if m.id == "gemini-3.8-flash")


def test_agy_payload_routes_the_thinking_level() -> None:
    from le_vibe.providers.agy import build_agy_payload

    model = _grouped_flash_model()
    ctx = TranscriptContext(messages=[Message(role="user", content="hi")])
    for level, runtime in (
        ("off", "gemini-3.8-flash-tiered"),
        ("low", "gemini-3.8-flash-low"),
        ("medium", "gemini-3.8-flash-medium"),
        ("high", "gemini-3.8-flash-high"),
    ):
        payload = build_agy_payload(model, ctx, StreamOptions(thinking_level=level), project_id="p")
        assert payload["model"] == runtime


def test_default_model_id_prefers_what_the_provider_recommends() -> None:
    from le_vibe.providers import catalog, default_model_id

    catalog._DYNAMIC_MODELS["agy"] = (
        _mini_spec("agy", "gemini-old", recommended=False),
        _mini_spec("agy", "gemini-new", recommended=True),
    )
    catalog._PROVIDER_DEFAULT.pop("agy", None)
    assert default_model_id("agy") == "gemini-new"

    catalog._PROVIDER_DEFAULT["agy"] = "gemini-old"
    assert default_model_id("agy") == "gemini-old"


def _mini_spec(provider: str, model_id: str, *, recommended: bool = False):
    from le_vibe.providers.types import ModelSpec

    return ModelSpec(
        id=model_id,
        name=model_id,
        provider=provider,  # type: ignore[arg-type]
        base_url="https://example.invalid",
        recommended=recommended,
    )


def test_distinct_base_ids_are_kept_even_when_display_names_collide() -> None:
    from le_vibe.providers.catalog import _agy_group_models

    raw = {
        "gemini-2.5-flash": {"displayName": "Gemini 3.5 Flash Lite"},
        "gemini-3.5-flash-lite": {"displayName": "Gemini 3.5 Flash Lite"},
        "gemini-3.8-flash-low": {"displayName": "Gemini 3.8 Flash (Low)"},
        "gemini-3.8-flash-high": {"displayName": "Gemini 3.8 Flash (High)"},
    }
    ids = [m.id for m in _agy_group_models(raw, "https://example.invalid")]
    # Thinking suffixes collapse, distinct ids with the same label do not.
    assert ids == ["gemini-2.5-flash", "gemini-3.5-flash-lite", "gemini-3.8-flash"]


def test_ids_without_a_display_name_are_humanized() -> None:
    from le_vibe.providers.catalog import _agy_group_models

    models = _agy_group_models(
        {"gemini-4-hydrogen-sonar": {}, "gemini-4-neon-lite": {}},
        "https://example.invalid",
    )
    names = {m.id: m.name for m in models}
    assert names["gemini-4-hydrogen-sonar"] == "Gemini 4 Hydrogen Sonar"
    assert names["gemini-4-neon-lite"] == "Gemini 4 Neon Lite"


def test_catalog_reports_freshness_and_can_skip_a_fresh_refresh() -> None:
    import asyncio

    from le_vibe.providers import catalog

    catalog._LAST_REFRESH = 0.0
    assert catalog.catalog_age_seconds() is None
    assert catalog.catalog_is_stale()

    counts = asyncio.run(catalog.refresh_model_catalogs())
    assert sum(counts.values()) >= 0
    assert catalog.catalog_age_seconds() is not None
    assert not catalog.catalog_is_stale()
    # A fresh catalog is not re-pulled, so a UI tick cannot hammer the providers.
    assert asyncio.run(catalog.refresh_model_catalogs(force=False)) == counts


def test_exhausted_models_carry_the_flag() -> None:
    from le_vibe.providers.catalog import _agy_group_models

    raw = {
        "gemini-4-argon-high": {
            "displayName": "Gemini 4 Argon (High)",
            "quotaInfo": {"remainingFraction": 0.0, "resetTime": "2026-12-01T00:00:00Z"},
        },
        "gemini-4-argon-low": {
            "displayName": "Gemini 4 Argon (Low)",
            "quotaInfo": {"remainingFraction": 0.0},
        },
    }
    models = _agy_group_models(raw, "https://example.invalid")
    assert [m.exhausted for m in models] == [True]


def test_each_model_keeps_the_endpoint_that_advertised_it() -> None:
    from le_vibe.providers.catalog import _agy_group_models

    raw = {"gemini-3.8-flash-high": {}, "claude-opus-5-5-high": {}}
    models = _agy_group_models(
        raw,
        "https://primary.invalid",
        endpoint_of={
            "gemini-3.8-flash-high": "https://primary.invalid",
            "claude-opus-5-5-high": "https://sandbox.invalid",
        },
    )
    urls = {m.id: m.base_url for m in models}
    assert urls["gemini-3.8-flash"] == "https://primary.invalid"
    assert urls["claude-opus-5-5"] == "https://sandbox.invalid"


def test_default_thinking_prefers_medium_or_high_when_no_medium() -> None:
    from le_vibe.providers import default_thinking_for_model
    from le_vibe.providers.types import ModelSpec

    with_med = ModelSpec(
        id="m1",
        name="M1",
        provider="vibe",
        base_url="https://example.invalid",
        reasoning=True,
        thinking_level_map={"low": "low", "medium": "medium", "high": "high"},
    )
    assert default_thinking_for_model(with_med) == "medium"

    only_high = ModelSpec(
        id="m2",
        name="M2",
        provider="vibe",
        base_url="https://example.invalid",
        reasoning=True,
        thinking_level_map={"off": "none", "high": "high"},
    )
    assert default_thinking_for_model(only_high) == "high"
