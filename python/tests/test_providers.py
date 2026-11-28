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


def test_agy_auto_discovers_future_model_families_without_allowlist() -> None:
    from le_vibe.providers.catalog import _agy_group_models

    raw = {
        "helium-1-pro-low": {"displayName": "Helium 1 Pro (Low)", "supportsThinking": True},
        "helium-1-pro-high": {"displayName": "Helium 1 Pro (High)", "supportsThinking": True},
        "glm-5.4": {"displayName": "GLM 5.4"},
        "tab_autocomplete_v2": {"displayName": "Tab"},
        "imagen-4-image-gen": {"displayName": "Image"},
        "internal-canary": {"displayName": "Canary", "isInternal": True},
    }
    models = _agy_group_models(raw, "https://example.invalid")
    ids = [m.id for m in models]
    assert "helium-1-pro" in ids
    assert "glm-5.4" in ids
    assert "tab_autocomplete_v2" not in ids
    assert "imagen-4-image-gen" not in ids
    assert "internal-canary" not in ids


def test_agy_keeps_stable_session_and_trajectory_ids_across_turns() -> None:
    from le_vibe.providers.agy import build_agy_payload
    from le_vibe.providers.types import Message, ModelSpec, StreamOptions, TranscriptContext

    model = ModelSpec(
        id="gemini-4-neon",
        name="Gemini 4 Neon",
        provider="agy",
        base_url="https://example.invalid",
    )
    ctx1 = TranscriptContext(
        system_prompt="sys prompt",
        messages=[Message(role="user", content="hello")],
    )
    ctx2 = TranscriptContext(
        system_prompt="sys prompt",
        messages=[
            Message(role="user", content="hello"),
            Message(role="assistant", content="hi"),
            Message(role="user", content="next step"),
        ],
    )
    p1 = build_agy_payload(model, ctx1, StreamOptions(), project_id="proj-1")
    p2 = build_agy_payload(model, ctx2, StreamOptions(), project_id="proj-1")
    assert p1["request"]["sessionId"] == p2["request"]["sessionId"]
    assert p1["request"]["labels"]["trajectory_id"] == p2["request"]["labels"]["trajectory_id"]
    assert p1["request"]["labels"]["request_id"] != p2["request"]["labels"]["request_id"]


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


def test_agy_oauth_oob_detection_and_code_extraction() -> None:
    from le_vibe.providers.auth import _extract_agy_auth_code, _should_use_agy_oob_auth

    assert _should_use_agy_oob_auth({}) is False
    assert _should_use_agy_oob_auth({"SSH_CONNECTION": "1.2.3.4 1234 5.6.7.8 22"}) is True
    assert _should_use_agy_oob_auth({"ANTIGRAVITY_REMOTE_OOB_AUTH": "1"}) is True
    assert _should_use_agy_oob_auth({"SSH_TTY": "/dev/pts/0", "ANTIGRAVITY_REMOTE_OOB_AUTH": "false"}) is False

    assert _extract_agy_auth_code("raw-code-123", "st-1") == "raw-code-123"
    assert (
        _extract_agy_auth_code(
            "https://antigravity.google/oauth-callback?code=cb-456&state=st-1",
            "st-1",
        )
        == "cb-456"
    )
    with pytest.raises(RuntimeError, match="state mismatch"):
        _extract_agy_auth_code(
            "https://antigravity.google/oauth-callback?code=cb-456&state=wrong",
            "st-1",
        )
    with pytest.raises(RuntimeError, match="access_denied"):
        _extract_agy_auth_code(
            "https://antigravity.google/oauth-callback?error=access_denied&error_description=nope",
            "st-1",
        )


def test_agy_oauth_loopback_login_uses_ephemeral_port_and_redirects(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import threading
    from urllib.parse import parse_qs, urlsplit
    from le_vibe.providers import auth

    auth_file = tmp_path / "auth.json"
    monkeypatch.setenv("AGY_OAUTH_CLIENT_ID", "test-cid")
    monkeypatch.setenv("AGY_OAUTH_CLIENT_SECRET", "test-csec")
    monkeypatch.setattr(auth, "discover_agy_project_id", lambda tok: "proj-123")

    seen_redirect_uri: list[str] = []
    callback_status: list[tuple[int, str]] = []
    real_httpx_client = httpx.Client

    def fake_open(url: str) -> bool:
        qs = parse_qs(urlsplit(url).query)
        redirect_uri = qs["redirect_uri"][0]
        state = qs["state"][0]
        seen_redirect_uri.append(redirect_uri)

        def _hit() -> None:
            with real_httpx_client(follow_redirects=False) as c:
                r = c.get(f"{redirect_uri}?code=code-ok&state={state}")
                callback_status.append((r.status_code, r.headers.get("location", "")))

        threading.Thread(target=_hit, daemon=True).start()
        return True

    monkeypatch.setattr(auth.webbrowser, "open", fake_open)

    exchanged: list[dict[str, str]] = []

    class _FakeClient:
        def __init__(self, *args: object, **kwargs: object) -> None:
            pass

        def __enter__(self) -> "_FakeClient":
            return self

        def __exit__(self, *args: object) -> None:
            return None

        def post(self, url: str, data: dict[str, str]) -> httpx.Response:
            exchanged.append(dict(data))
            return httpx.Response(
                200,
                request=httpx.Request("POST", url),
                json={
                    "access_token": "ya29.fresh",
                    "refresh_token": "1//ref",
                    "id_token": "id-jwt-1",
                    "expires_in": 3600,
                },
            )

    monkeypatch.setattr(auth.httpx, "Client", _FakeClient)

    out_path = auth.login_agy_oauth(auth_file=auth_file, timeout_s=5.0, oob=False)
    assert out_path == auth_file
    assert seen_redirect_uri[0].startswith("http://localhost:")
    assert seen_redirect_uri[0].endswith("/auth/callback")
    assert callback_status == [(302, auth.AGY_OAUTH_SUCCESS_URL)]
    assert exchanged[0]["redirect_uri"] == seen_redirect_uri[0]
    assert exchanged[0]["code"] == "code-ok"

    stored = json.loads(auth_file.read_text(encoding="utf-8"))["agy"]
    assert stored["access_token"] == "ya29.fresh"
    assert stored["id_token"] == "id-jwt-1"
    assert stored["project_id"] == "proj-123"


def test_agy_refresh_evicts_on_invalid_grant_and_keeps_on_network_error(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from le_vibe.providers import auth, save_oidc_tokens

    auth_file = tmp_path / "auth.json"
    monkeypatch.setenv("AGY_OAUTH_CLIENT_ID", "test-cid")
    monkeypatch.setenv("AGY_OAUTH_CLIENT_SECRET", "test-csec")

    # 1) Transient network error keeps stored token intact.
    save_oidc_tokens(
        "agy",
        access_token="ya29.stale",
        refresh_token="1//ref",
        expires_at=1.0,
        token_endpoint=auth.AGY_OAUTH_TOKEN_URL,
        client_id="test-cid",
        project_id="proj-1",
        auth_file=auth_file,
    )

    class _NetErrClient:
        def __init__(self, *args: object, **kwargs: object) -> None:
            pass

        def __enter__(self) -> "_NetErrClient":
            return self

        def __exit__(self, *args: object) -> None:
            return None

        def post(self, url: str, data: dict[str, str]) -> httpx.Response:
            raise httpx.ConnectError("offline")

    monkeypatch.setattr(auth.httpx, "Client", _NetErrClient)
    cred = auth.resolve_api_key("agy", env={}, auth_file=auth_file)
    assert cred.token == "ya29.stale"
    assert "agy" in json.loads(auth_file.read_text(encoding="utf-8"))

    # 2) Definitive invalid_grant evicts stored credential.
    class _InvalidGrantClient:
        def __init__(self, *args: object, **kwargs: object) -> None:
            pass

        def __enter__(self) -> "_InvalidGrantClient":
            return self

        def __exit__(self, *args: object) -> None:
            return None

        def post(self, url: str, data: dict[str, str]) -> httpx.Response:
            return httpx.Response(
                400,
                request=httpx.Request("POST", url),
                json={"error": "invalid_grant"},
            )

    monkeypatch.setattr(auth.httpx, "Client", _InvalidGrantClient)
    with pytest.raises(MissingAPIKeyError):
        auth.resolve_api_key("agy", env={}, auth_file=auth_file)
    assert "agy" not in json.loads(auth_file.read_text(encoding="utf-8"))
