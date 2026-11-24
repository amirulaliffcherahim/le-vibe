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
            timeout=httpx.Timeout(timeout_s, connect=15.0, read=timeout_s, write=30.0, pool=15.0),
            limits=httpx.Limits(
                max_keepalive_connections=12,
                max_connections=24,
                keepalive_expiry=30.0,
            ),
        )
        _SHARED_CLIENTS[loop] = client
    return client


async def _reset_shared_client() -> None:
    loop = asyncio.get_running_loop()
    old = _SHARED_CLIENTS.pop(loop, None)
    if old is not None and not old.is_closed:
        await old.aclose()


async def stream_sse_json(
    url: str,
    *,
    headers: Mapping[str, str],
    payload: dict[str, Any],
    timeout_s: float = 180.0,
    client: httpx.AsyncClient | None = None,
) -> AsyncIterator[dict[str, Any]]:
    """POST JSON and yield parsed SSE `data:` payloads until `[DONE]`."""
    max_attempts = 2 if client is None else 1
    for attempt in range(max_attempts):
        active_client = client or _get_shared_client(timeout_s)
        yielded_any = False
        try:
            async with active_client.stream(
                "POST",
                url,
                headers=headers,
                json=payload,
                timeout=httpx.Timeout(
                    timeout_s, connect=15.0, read=timeout_s, write=30.0, pool=15.0
                ),
            ) as response:
                if not response.is_success:
                    body = (await response.aread()).decode("utf-8", errors="replace")
                    retry_hint = ""
                    ra_ms = response.headers.get("retry-after-ms")
                    ra_s = response.headers.get("retry-after")
                    if ra_ms:
                        try:
                            secs = max(1, min(60, int(float(ra_ms) / 1000.0 + 0.999)))
                            retry_hint = f" [retry_after={secs}s]"
                        except ValueError:
                            pass
                    elif ra_s:
                        try:
                            secs = max(1, min(60, int(float(ra_s) + 0.999)))
                            retry_hint = f" [retry_after={secs}s]"
                        except ValueError:
                            pass
                    raise RuntimeError(f"HTTP {response.status_code}{retry_hint} from {url}: {body[:500]}")
                async for raw_line in response.aiter_lines():
                    line = raw_line.strip()
                    if not line or not line.startswith("data:"):
                        continue
                    data = line[5:].strip()
                    if data == "[DONE]":
                        return
                    yielded_any = True
                    yield json.loads(data)
            return
        except httpx.TransportError as exc:
            # Stale keep-alive sockets on Cloudflare/Mistral/AGY drop before first byte;
            # evict the pooled client and retry once silently before surfacing the error.
            if client is None and not yielded_any and attempt + 1 < max_attempts:
                await _reset_shared_client()
                continue
            raise RuntimeError(f"{type(exc).__name__}: {exc}") from exc


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
