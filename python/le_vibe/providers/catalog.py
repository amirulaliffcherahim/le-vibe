from __future__ import annotations

import asyncio
from collections.abc import Mapping
from dataclasses import asdict, dataclass
import fnmatch
import json
import os
from pathlib import Path
import time
from typing import Any

import httpx

from le_vibe.providers.auth import (
    AGY_ENDPOINTS,
    VIBE_USER_AGENT,
    MissingAPIKeyError,
    _agy_endpoint_candidates,
    _agy_headers,
    default_auth_file,
    get_agy_project_id,
    resolve_api_key,
)
from le_vibe.providers.transport import request_with_oauth_retry
from le_vibe.providers.types import ModelCompat, ModelCost, ModelSpec, ProviderId

DEEPSEEK_BASE_URL = "https://api.deepseek.com"
VIBE_BASE_URL = "https://api.mistral.ai/v1"
AGY_BASE_URL = "https://daily-cloudcode-pa.googleapis.com"

_DEEPSEEK_COMPAT = ModelCompat(
    supports_store=False,
    supports_developer_role=False,
    max_tokens_field="max_tokens",
    requires_reasoning_content_on_assistant_messages=True,
    thinking_format="deepseek",
    supports_strict_mode=True,
)

_VIBE_COMPAT = ModelCompat(
    supports_store=False,
    supports_developer_role=False,
    max_tokens_field="max_tokens",
    requires_reasoning_content_on_assistant_messages=False,
    thinking_format="mistral",
    supports_strict_mode=True,
    supports_mid_convo_system_messages=True,
)

# No model list is hardcoded: every entry comes from the provider's own catalog
# endpoint (or the last successful fetch cached on disk). A provider that is not
# connected contributes nothing, so the picker tracks the account's real entitlements.

_DYNAMIC_MODELS: dict[ProviderId, tuple[ModelSpec, ...]] = {}
# The model each provider says it prefers right now (Antigravity's defaultAgentModelId,
# Mistral's `recommended` flag, DeepSeek's pro id). Refreshed with the catalog.
_PROVIDER_DEFAULT: dict[ProviderId, str] = {}
#: When the catalogs were last pulled from the providers (Unix seconds).
_LAST_REFRESH: float = 0.0
_CACHE_LOADED = False

#: How long a catalog is trusted before the next automatic refresh.
CATALOG_REFRESH_INTERVAL_S = float(
    os.environ.get("LE_VIBE_CATALOG_REFRESH_S", str(30 * 60))
)


def catalog_updated_at() -> float:
    return _LAST_REFRESH


def catalog_age_seconds() -> float | None:
    """Seconds since the last successful pull, or None when nothing has been pulled."""
    return time.time() - _LAST_REFRESH if _LAST_REFRESH else None


def catalog_is_stale(interval_s: float | None = None) -> bool:
    age = catalog_age_seconds()
    if age is None:
        return True
    return age >= (CATALOG_REFRESH_INTERVAL_S if interval_s is None else interval_s)


def catalog_cache_file() -> Path:
    if override := os.environ.get("LE_VIBE_CATALOG_CACHE"):
        return Path(override)
    return default_auth_file().parent / "models-cache.json"


def _spec_from_dict(item: dict[str, Any]) -> ModelSpec | None:
    try:
        cost_raw = item.get("cost") or {}
        compat_raw = item.get("compat") or {}
        return ModelSpec(
            id=str(item["id"]),
            name=str(item.get("name") or item["id"]),
            provider=item["provider"],
            base_url=str(item["base_url"]),
            reasoning=bool(item.get("reasoning", True)),
            input_modalities=tuple(item.get("input_modalities") or ("text",)),
            context_window=int(item.get("context_window") or 128_000),
            max_tokens=int(item.get("max_tokens") or 32_768),
            cost=ModelCost(
                input=float(cost_raw.get("input", 0.0)),
                output=float(cost_raw.get("output", 0.0)),
                cache_read=float(cost_raw.get("cache_read", 0.0)),
                cache_write=float(cost_raw.get("cache_write", 0.0)),
            ),
            thinking_level_map=dict(item.get("thinking_level_map") or {}),
            compat=ModelCompat(**compat_raw) if isinstance(compat_raw, dict) else ModelCompat(),
        )
    except Exception:
        return None


def _load_cached_catalogs() -> None:
    global _CACHE_LOADED
    if _CACHE_LOADED:
        return
    _CACHE_LOADED = True
    path = catalog_cache_file()
    if not path.exists():
        return
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
        providers_map = data.get("providers") if isinstance(data, dict) else None
        if not isinstance(providers_map, dict):
            return
        updated_at = data.get("updated_at") if isinstance(data, dict) else None
        if isinstance(updated_at, (int, float)) and float(updated_at) > 0:
            global _LAST_REFRESH
            _LAST_REFRESH = float(updated_at)
        cached_defaults = data.get("defaults") if isinstance(data, dict) else None
        if isinstance(cached_defaults, dict):
            for prov, mid in cached_defaults.items():
                if isinstance(mid, str) and mid:
                    _PROVIDER_DEFAULT[prov] = mid
        for prov in ("vibe", "deepseek", "agy"):
            entries = providers_map.get(prov)
            if isinstance(entries, list):
                parsed = [s for e in entries if isinstance(e, dict) and (s := _spec_from_dict(e)) is not None]
                if parsed:
                    _DYNAMIC_MODELS[prov] = tuple(parsed)  # type: ignore[index]
    except Exception:
        pass


def _save_cached_catalogs() -> None:
    path = catalog_cache_file()
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "updated_at": time.time(),
            "defaults": dict(_PROVIDER_DEFAULT),
            "providers": {
                prov: [asdict(m) for m in models] for prov, models in _DYNAMIC_MODELS.items()
            },
        }
        path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    except Exception:
        pass


def _merge_models(baseline: tuple[ModelSpec, ...], discovered: list[ModelSpec]) -> tuple[ModelSpec, ...]:
    by_id: dict[str, ModelSpec] = {m.id: m for m in discovered}
    for base in baseline:
        by_id.setdefault(base.id, base)
    return tuple(by_id.values())


# Thinking variants a runtime id suffix can map onto, longest suffix first.
_AGY_THINKING_SUFFIXES: tuple[tuple[str, str | None], ...] = (
    ("tiered-xhigh", "xhigh"),
    ("extra-low", "low"),
    ("extra-high", "xhigh"),
    ("thinking", "high"),
    ("minimal", "minimal"),
    ("tiered", None),
    ("xhigh", "xhigh"),
    ("medium", "medium"),
    ("high", "high"),
    ("low", "low"),
)

_AGY_DISPLAY_LEVELS: tuple[tuple[str, str], ...] = (
    ("extra low", "low"),
    ("extra high", "xhigh"),
    ("xhigh", "xhigh"),
    ("thinking", "high"),
    ("minimal", "minimal"),
    ("medium", "medium"),
    ("high", "high"),
    ("low", "low"),
)

# Runtime ids whose public family differs from their id prefix.
_AGY_RUNTIME_ALIASES: dict[str, tuple[str, str]] = {
    "gemini-3-flash-agent": ("gemini-3-flash", "high"),
    "gemini-pro-agent": ("gemini-2.5-pro", "high"),
}


class _AgyGroup:
    __slots__ = (
        "public_id",
        "variants",
        "unsuffixed",
        "display",
        "ctx",
        "max_out",
        "thinking",
        "recommended",
        "exhausted",
        "endpoint",
    )

    def __init__(self, public_id: str) -> None:
        self.public_id = public_id
        self.variants: dict[str, str] = {}
        self.unsuffixed: str | None = None
        self.display: str | None = None
        self.ctx = 1_048_576
        self.max_out = 65_536
        self.thinking = True
        self.recommended = False
        self.exhausted = False
        self.endpoint = ""


def _humanize_model_id(model_id: str) -> str:
    """`gemini-4-hydrogen-sonar` -> `Gemini 4 Hydrogen Sonar`."""
    words: list[str] = []
    for part in model_id.replace("_", "-").split("-"):
        if not part:
            continue
        if part.isdigit() or len(part) <= 2:
            words.append(part.upper() if part.isalpha() else part)
        else:
            words.append(part.capitalize())
    return " ".join(words)


def _agy_strip_level(display: str) -> str:
    text = display.strip()
    if text.endswith(")") and "(" in text:
        head, _, tail = text.rpartition("(")
        if tail.rstrip(")").strip().lower() in {
            "low",
            "medium",
            "high",
            "thinking",
            "minimal",
            "extra low",
            "extra high",
            "xhigh",
        }:
            return head.strip()
    return text


def _agy_level_from_display(display: str) -> str | None:
    text = display.strip().lower()
    for name, level in _AGY_DISPLAY_LEVELS:
        if f"({name})" in text:
            return level
    return None


def _agy_public_id(runtime_id: str) -> tuple[str, str | None]:
    """Base family id plus the thinking level this runtime id encodes."""
    if alias := _AGY_RUNTIME_ALIASES.get(runtime_id):
        return alias[0], alias[1]
    lower = runtime_id.lower()
    for suffix, level in _AGY_THINKING_SUFFIXES:
        if lower.endswith(f"-{suffix}"):
            return runtime_id[: -(len(suffix) + 1)], level
    return runtime_id, None


def _agy_group_models(
    models_map: dict[str, Any],
    endpoint: str,
    *,
    default_runtime: str = "",
    endpoint_of: dict[str, str] | None = None,
) -> list[ModelSpec]:
    """Collapse thinking variants into one base model whose thinking_level_map routes them."""
    groups: dict[str, _AgyGroup] = {}
    for runtime_id, info in models_map.items():
        if not isinstance(runtime_id, str) or not runtime_id.strip():
            continue
        lower_id = runtime_id.lower()
        if (
            lower_id.startswith(("chat_", "tab_", "embed", "tts", "whisper"))
            or any(tok in lower_id for tok in ("image", "embedding", "moderation", "audio"))
            or lower_id.endswith("-disabled")
        ):
            continue
        info_dict = info if isinstance(info, dict) else {}
        if info_dict.get("isInternal"):
            continue

        public_id, suffix_level = _agy_public_id(runtime_id)
        group = groups.setdefault(public_id, _AgyGroup(public_id))
        display = str(info_dict.get("displayName") or info_dict.get("label") or "")
        level = _agy_level_from_display(display) or suffix_level
        if level:
            group.variants[level] = runtime_id
        else:
            group.unsuffixed = runtime_id
        if display:
            group.display = _agy_strip_level(display)
        elif group.display is None:
            group.display = _humanize_model_id(public_id)
        group.ctx = int(info_dict.get("maxTokens") or group.ctx)
        group.max_out = int(info_dict.get("maxOutputTokens") or group.max_out)
        group.thinking = bool(info_dict.get("supportsThinking", group.thinking))
        if info_dict.get("recommended") is True:
            group.recommended = True
        quota = info_dict.get("quotaInfo")
        if isinstance(quota, dict):
            fraction = quota.get("remainingFraction")
            if isinstance(fraction, (int, float)) and float(fraction) <= 0.0:
                group.exhausted = True
        if not group.endpoint:
            group.endpoint = (endpoint_of or {}).get(runtime_id, endpoint)

    out: list[ModelSpec] = []
    # The account's own default agent model wins when it maps onto a base model.
    preferred_id = _agy_public_id(default_runtime)[0] if default_runtime else ""
    if preferred_id and preferred_id in groups:
        _PROVIDER_DEFAULT["agy"] = preferred_id
    for public_id, group in groups.items():
        mapping = dict(group.variants)
        if group.unsuffixed:
            mapping.setdefault("off", group.unsuffixed)
            mapping.setdefault("medium", group.unsuffixed)
        if not mapping:
            continue
        out.append(
            ModelSpec(
                id=public_id,
                name=group.display or public_id,
                provider="agy",
                base_url=group.endpoint or endpoint,
                reasoning=group.thinking,
                input_modalities=("text", "image"),
                context_window=group.ctx,
                max_tokens=group.max_out,
                cost=ModelCost(input=0.0, output=0.0),
                thinking_level_map=mapping,
                recommended=group.recommended,
                exhausted=group.exhausted,
            )
        )
    return sorted(out, key=lambda m: m.id)


async def _fetch_vibe_catalog(client: httpx.AsyncClient) -> list[ModelSpec]:
    try:
        cred = resolve_api_key("vibe")
    except (MissingAPIKeyError, RuntimeError):
        return []
    headers = {
        "Authorization": f"Bearer {cred.token}",
        "User-Agent": VIBE_USER_AGENT,
        **cred.extra_headers,
    }

    async def _refresh_vibe() -> dict[str, str] | None:
        try:
            refreshed = resolve_api_key("vibe", force_refresh=True)
        except (MissingAPIKeyError, RuntimeError):
            return None
        return {
            "Authorization": f"Bearer {refreshed.token}",
            "User-Agent": VIBE_USER_AGENT,
            **refreshed.extra_headers,
        }

    resp = await request_with_oauth_retry(
        client,
        "GET",
        f"{VIBE_BASE_URL}/model-catalog?surface=vibe-code",
        headers=headers,
        on_unauthorized=_refresh_vibe,
    )
    if not resp.is_success:
        return []
    models_raw = resp.json().get("models") or []
    out: list[ModelSpec] = []
    seen_labels: set[str] = set()
    for item in models_raw:
        if not isinstance(item, dict) or not item.get("id"):
            continue
        mid = str(item["id"])
        label = str(item.get("label") or mid)
        # Two catalog rows can share a label (a paid and a free variant of one model).
        if label in seen_labels:
            continue
        seen_labels.add(label)
        ctx_win = int(item.get("max_context_length") or 256_000)
        params = item.get("parameters") or {}
        caps = item.get("capabilities") or {}
        reasoning_levels = params.get("reasoning") or []
        mapping: dict[str, str] = {}
        for level in reasoning_levels:
            mapped = {"none": "off"}.get(str(level), str(level))
            mapping[mapped] = mapped
        modalities = ("text", "image") if caps.get("vision") else ("text",)
        out.append(
            ModelSpec(
                id=mid,
                name=label,
                provider="vibe",
                base_url=VIBE_BASE_URL,
                reasoning=bool(reasoning_levels),
                input_modalities=modalities,
                context_window=ctx_win,
                max_tokens=64_000,
                cost=ModelCost(input=0.40, output=2.00, cache_read=0.04),
                compat=_VIBE_COMPAT,
                thinking_level_map=mapping,
                recommended=bool(item.get("recommended")),
            )
        )
    for model in out:
        if model.recommended:
            _PROVIDER_DEFAULT["vibe"] = model.id
            break
    return out


async def _fetch_agy_catalog(client: httpx.AsyncClient) -> list[ModelSpec]:
    try:
        cred = resolve_api_key("agy")
    except (MissingAPIKeyError, RuntimeError):
        return []
    project_id = cred.project_id or get_agy_project_id()
    headers = _agy_headers(cred.token, project_id=project_id)

    async def _refresh_agy() -> dict[str, str] | None:
        try:
            refreshed = resolve_api_key("agy", force_refresh=True)
        except (MissingAPIKeyError, RuntimeError):
            return None
        return _agy_headers(refreshed.token, project_id=refreshed.project_id or project_id)

    # The endpoints do not always serve the same set, so merge all of them and keep the
    # first endpoint that names each model.
    merged: dict[str, Any] = {}
    endpoint_of: dict[str, str] = {}
    default_runtime = ""
    for endpoint in _agy_endpoint_candidates():
        try:
            resp = await request_with_oauth_retry(
                client,
                "POST",
                f"{endpoint}/v1internal:fetchAvailableModels",
                headers=headers,
                json_body={"project": project_id},
                on_unauthorized=_refresh_agy,
            )
            if not resp.is_success:
                continue
            payload = resp.json()
            models_map = payload.get("models") or {}
            if not isinstance(models_map, dict):
                continue
            for model_id, info in models_map.items():
                if model_id not in merged:
                    merged[model_id] = info
                    endpoint_of[model_id] = endpoint
            default_runtime = default_runtime or str(payload.get("defaultAgentModelId") or "")
        except Exception:
            continue
    if not merged:
        return []
    primary = endpoint_of.get(default_runtime) or endpoint_of[next(iter(endpoint_of))]
    return _agy_group_models(
        merged, primary, default_runtime=default_runtime, endpoint_of=endpoint_of
    )


async def _fetch_deepseek_catalog(client: httpx.AsyncClient) -> list[ModelSpec]:
    try:
        cred = resolve_api_key("deepseek")
    except MissingAPIKeyError:
        return []
    headers = {"Authorization": f"Bearer {cred.token}"}
    resp = await client.get(f"{DEEPSEEK_BASE_URL}/v1/models", headers=headers)
    if not resp.is_success:
        return []
    data = resp.json().get("data") or []
    out: list[ModelSpec] = []
    for item in data:
        if not isinstance(item, dict) or not item.get("id"):
            continue
        mid = str(item["id"])
        out.append(
            ModelSpec(
                id=mid,
                name=f"DeepSeek ({mid})",
                provider="deepseek",
                base_url=DEEPSEEK_BASE_URL,
                reasoning=True,
                input_modalities=("text",),
                context_window=128_000,
                max_tokens=64_000,
                cost=ModelCost(input=0.27, output=1.10, cache_read=0.07),
                compat=_DEEPSEEK_COMPAT,
                recommended="pro" in mid.lower(),
            )
        )
    for model in out:
        if model.recommended:
            _PROVIDER_DEFAULT["deepseek"] = model.id
            break
    return out


async def refresh_model_catalogs(
    provider: ProviderId | None = None,
    *,
    timeout_s: float = 10.0,
    force: bool = True,
) -> dict[str, int]:
    """Pull live model catalogs from authenticated providers and persist to ~/.le-vibe/models-cache.json.

    With ``force=False`` a catalog younger than ``CATALOG_REFRESH_INTERVAL_S`` is left alone,
    so callers can ask on every UI tick without hammering the providers.
    """
    global _LAST_REFRESH
    _load_cached_catalogs()
    if not force and provider is None and not catalog_is_stale():
        return {p: len(list_models(p)) for p in PROVIDER_IDS}
    targets: tuple[ProviderId, ...] = (provider,) if provider else ("vibe", "deepseek", "agy")
    counts: dict[str, int] = {}

    async with httpx.AsyncClient(timeout=timeout_s) as client:
        tasks: dict[ProviderId, Any] = {}
        for prov in targets:
            if prov == "vibe":
                tasks[prov] = _fetch_vibe_catalog(client)
            elif prov == "agy":
                tasks[prov] = _fetch_agy_catalog(client)
            elif prov == "deepseek":
                tasks[prov] = _fetch_deepseek_catalog(client)

        results = await asyncio.gather(*tasks.values(), return_exceptions=True)
        for prov, res in zip(tasks.keys(), results, strict=False):
            if isinstance(res, list) and res:
                merged = tuple(res)
                _DYNAMIC_MODELS[prov] = merged
                counts[prov] = len(merged)
            else:
                # An unreachable or unconnected provider keeps whatever the cache holds.
                counts[prov] = len(_DYNAMIC_MODELS.get(prov, ()))

    _LAST_REFRESH = time.time()
    _save_cached_catalogs()
    return counts


@dataclass(frozen=True, slots=True)
class ModelReasoningCapability:
    """Supported `reasoning_effort` values and nearest-level fallback for a model family."""

    pattern: str
    supported: tuple[str, ...]
    fallback: Mapping[str, str]


# Per-model reasoning capabilities (matched against lowercase model.id).
# Keeps clamping in the catalog layer instead of hardcoding provider-wide rules.
MODEL_REASONING_CAPABILITIES: tuple[ModelReasoningCapability, ...] = (
    ModelReasoningCapability(
        pattern="deepseek-v4-1-flash*",
        supported=("none", "low", "high", "max"),
        fallback={"minimal": "low", "medium": "high", "xhigh": "high"},
    ),
    ModelReasoningCapability(
        pattern="mistral-large-4*",
        supported=("low", "medium", "high"),
        fallback={"none": "low", "minimal": "low", "xhigh": "high", "max": "high"},
    ),
    ModelReasoningCapability(
        pattern="mistral-small-2603",
        supported=("none", "high"),
        fallback={
            "minimal": "high",
            "low": "high",
            "medium": "high",
            "xhigh": "high",
            "max": "high",
        },
    ),
    ModelReasoningCapability(
        pattern="mistral-medium-3-5",
        supported=("none", "high"),
        fallback={
            "minimal": "high",
            "low": "high",
            "medium": "high",
            "xhigh": "high",
            "max": "high",
        },
    ),
    ModelReasoningCapability(
        pattern="zai-glm-5-3*",
        supported=("low", "high", "max"),
        fallback={"none": "low", "minimal": "low", "medium": "high", "xhigh": "max"},
    ),
)

_EFFORT_LADDER: tuple[str, ...] = ("none", "minimal", "low", "medium", "high", "xhigh", "max")


def _nearest_supported_effort(level: str, supported: tuple[str, ...]) -> str:
    if level in supported:
        return level
    if level in _EFFORT_LADDER:
        target = _EFFORT_LADDER.index(level)
        ranked = sorted(
            (s for s in supported if s in _EFFORT_LADDER),
            key=lambda s: (abs(_EFFORT_LADDER.index(s) - target), -_EFFORT_LADDER.index(s)),
        )
        if ranked:
            return ranked[0]
    return "high" if "high" in supported else supported[0]


def resolve_reasoning_effort(model: ModelSpec, requested: str) -> str:
    """Clamp a requested thinking level to a valid `reasoning_effort` for `model`."""
    raw = (requested or "medium").strip().lower()
    normalized = "none" if raw in {"off", "none"} else raw
    mid = model.id.lower()

    for cap in MODEL_REASONING_CAPABILITIES:
        if fnmatch.fnmatchcase(mid, cap.pattern):
            if normalized in cap.supported:
                return normalized
            if normalized in cap.fallback:
                return cap.fallback[normalized]
            return _nearest_supported_effort(normalized, cap.supported)

    if model.thinking_level_map:
        if raw in model.thinking_level_map:
            mapped = model.thinking_level_map[raw]
            return "none" if mapped in {"off", "none"} else mapped
        if normalized in model.thinking_level_map:
            mapped = model.thinking_level_map[normalized]
            return "none" if mapped in {"off", "none"} else mapped
        supported = tuple(
            dict.fromkeys("none" if v in {"off", "none"} else v for v in model.thinking_level_map.values())
        )
        if supported:
            return _nearest_supported_effort(normalized, supported)

    return normalized


def default_thinking_for_model(model: ModelSpec) -> str:
    """Default every model to `medium`, falling back to `high` when `medium` is absent."""
    if not model.reasoning:
        return "off"
    levels = model.thinking_level_map
    if not levels or "medium" in levels:
        return "medium"
    if "high" in levels:
        return "high"
    for candidate in ("xhigh", "low", "minimal", "max"):
        if candidate in levels:
            return candidate
    return "medium"


def default_model_id(provider: ProviderId) -> str:
    """The model to start with: whatever the provider recommends, else its first entry.

    Derived from the live catalog, so a new release becomes the default without a code change.
    """
    models = list_models(provider)
    chosen = _PROVIDER_DEFAULT.get(provider)
    if chosen and any(m.id == chosen for m in models):
        return chosen
    for model in models:
        if model.recommended:
            return model.id
    return models[0].id if models else ""


PROVIDER_IDS: tuple[ProviderId, ...] = ("vibe", "deepseek", "agy")


def get_providers() -> tuple[ProviderId, ...]:
    return PROVIDER_IDS


def connected_providers() -> tuple[ProviderId, ...]:
    """Providers that returned at least one model, i.e. are signed in and reachable."""
    return tuple(p for p in PROVIDER_IDS if list_models(p))


def list_models(provider: ProviderId | None = None) -> tuple[ModelSpec, ...]:
    """Every model the connected account can actually use right now.

    Empty for a provider that is not connected, so callers can treat an empty list
    as "not available" instead of showing a stale entry.
    """
    _load_cached_catalogs()
    if provider is not None:
        if provider not in PROVIDER_IDS:
            raise ValueError(
                f"Unsupported provider: {provider!r}. Only 'vibe', 'deepseek', and 'agy' are allowed."
            )
        return _DYNAMIC_MODELS.get(provider, ())
    return (
        *list_models("vibe"),
        *list_models("deepseek"),
        *list_models("agy"),
    )


def get_model(provider: ProviderId, model_id: str) -> ModelSpec:
    for model in list_models(provider):
        if model.id == model_id:
            return model
    raise KeyError(f"Model {model_id!r} not found for provider {provider!r}")
