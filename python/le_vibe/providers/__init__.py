"""Single unified provider module supporting `vibe`, `deepseek`, and `agy`."""

from __future__ import annotations

from collections.abc import AsyncIterator

from le_vibe.providers.agy import AgyProvider
from le_vibe.providers.auth import (
    MissingAPIKeyError,
    PROVIDER_ENV_KEYS,
    auth_status,
    delete_credential,
    has_auth,
    login_agy_oauth,
    login_vibe_oidc,
    resolve_api_key,
    save_api_key,
    save_oidc_tokens,
)
from le_vibe.providers.catalog import (
    default_model_id,
    default_thinking_for_model,
    get_model,
    catalog_age_seconds,
    catalog_is_stale,
    catalog_updated_at,
    connected_providers,
    get_providers,
    list_models,
    refresh_model_catalogs,
)
from le_vibe.providers.deepseek import DeepSeekProvider
from le_vibe.providers.types import (
    AssistantMessage,
    AuthCredential,
    Message,
    ModelCompat,
    ModelCost,
    ModelSpec,
    ProviderAdapter,
    ProviderId,
    StreamEvent,
    StreamOptions,
    ThinkingLevel,
    ToolCall,
    ToolDefinition,
    TranscriptContext,
    Usage,
)
from le_vibe.providers.usage import fetch_usage
from le_vibe.providers.vibe import VibeProvider

_PROVIDERS: dict[ProviderId, ProviderAdapter] = {
    "vibe": VibeProvider(),
    "deepseek": DeepSeekProvider(),
    "agy": AgyProvider(),
}


def list_providers() -> tuple[ProviderId, ...]:
    return get_providers()


def get_provider(provider: ProviderId) -> ProviderAdapter:
    if provider not in _PROVIDERS:
        raise ValueError(
            f"Unsupported provider: {provider!r}. Only 'vibe', 'deepseek', and 'agy' are allowed."
        )
    return _PROVIDERS[provider]


def stream(
    model: ModelSpec,
    context: TranscriptContext,
    options: StreamOptions | None = None,
) -> AsyncIterator[StreamEvent]:
    return get_provider(model.provider).stream(model, context, options)


async def complete(
    model: ModelSpec,
    context: TranscriptContext,
    options: StreamOptions | None = None,
) -> AssistantMessage:
    return await get_provider(model.provider).complete(model, context, options)


__all__ = [
    "AgyProvider",
    "AssistantMessage",
    "AuthCredential",
    "default_model_id",
    "DeepSeekProvider",
    "Message",
    "MissingAPIKeyError",
    "ModelCompat",
    "ModelCost",
    "ModelSpec",
    "PROVIDER_ENV_KEYS",
    "ProviderAdapter",
    "ProviderId",
    "StreamEvent",
    "StreamOptions",
    "ThinkingLevel",
    "ToolCall",
    "ToolDefinition",
    "TranscriptContext",
    "Usage",
    "VibeProvider",
    "auth_status",
    "catalog_age_seconds",
    "catalog_is_stale",
    "catalog_updated_at",
    "connected_providers",
    "complete",
    "default_model_id",
    "default_thinking_for_model",
    "delete_credential",
    "fetch_usage",
    "get_model",
    "get_provider",
    "has_auth",
    "list_models",
    "list_providers",
    "login_agy_oauth",
    "login_vibe_oidc",
    "refresh_model_catalogs",
    "resolve_api_key",
    "save_api_key",
    "save_oidc_tokens",
    "stream",
]
