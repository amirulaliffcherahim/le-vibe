from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator, Mapping
import json
from typing import Any
import weakref

import httpx

_SHARED_CLIENTS: weakref.WeakKeyDictionary[asyncio.AbstractEventLoop, httpx.AsyncClient] = (
    weakref.WeakKeyDictionary()
)


def _get_shared_client(timeout_s: float) -> httpx.AsyncClient:
    """Reuse a keep-alive HTTP client per event loop so turns skip TCP/TLS handshakes."""
    loop = asyncio.get_running_loop()
    client = _SHARED_CLIENTS.get(loop)
    if client is None or client.is_closed:
        client = httpx.AsyncClient(
            timeout=timeout_s,
            limits=httpx.Limits(
                max_keepalive_connections=12,
                max_connections=24,
                keepalive_expiry=60.0,
            ),
        )
        _SHARED_CLIENTS[loop] = client
    return client


async def stream_sse_json(
    url: str,
    *,
    headers: Mapping[str, str],
    payload: dict[str, Any],
    timeout_s: float = 120.0,
    client: httpx.AsyncClient | None = None,
) -> AsyncIterator[dict[str, Any]]:
    """POST JSON and yield parsed SSE `data:` payloads until `[DONE]`."""
    active_client = client or _get_shared_client(timeout_s)
    async with active_client.stream(
        "POST",
        url,
        headers=headers,
        json=payload,
        timeout=timeout_s,
    ) as response:
        if not response.is_success:
            body = (await response.aread()).decode("utf-8", errors="replace")
            raise RuntimeError(f"HTTP {response.status_code} from {url}: {body[:500]}")
        async for raw_line in response.aiter_lines():
            line = raw_line.strip()
            if not line or not line.startswith("data:"):
                continue
            data = line[5:].strip()
            if data == "[DONE]":
                break
            yield json.loads(data)


def calculate_cost_usd(
    *,
    input_tokens: int,
    output_tokens: int,
    cache_read_tokens: int,
    input_per_m: float,
    output_per_m: float,
    cache_read_per_m: float,
) -> float:
    return (
        (input_tokens * input_per_m)
        + (output_tokens * output_per_m)
        + (cache_read_tokens * cache_read_per_m)
    ) / 1_000_000.0
