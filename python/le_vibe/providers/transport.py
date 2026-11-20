from __future__ import annotations

from collections.abc import AsyncIterator, Mapping
import json
from typing import Any

import httpx


async def stream_sse_json(
    url: str,
    *,
    headers: Mapping[str, str],
    payload: dict[str, Any],
    timeout_s: float = 120.0,
    client: httpx.AsyncClient | None = None,
) -> AsyncIterator[dict[str, Any]]:
    """POST JSON and yield parsed SSE `data:` payloads until `[DONE]`."""
    owns_client = client is None
    active_client = client or httpx.AsyncClient(timeout=timeout_s)
    try:
        async with active_client.stream("POST", url, headers=headers, json=payload) as response:
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
    finally:
        if owns_client:
            await active_client.aclose()


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
