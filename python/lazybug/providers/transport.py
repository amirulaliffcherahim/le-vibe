from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator, Awaitable, Callable, Mapping
from datetime import UTC, datetime
from email.utils import parsedate_to_datetime
import json
import os
from pathlib import Path
import random
import re
import time
from typing import Any
import weakref

import httpx

# Reference timeouts and retry curve from mistral-vibe 3.1.2 (stream_guard.py / _retry.py).
FIRST_OUTPUT_WAIT_SECONDS = 720.0
STREAM_IDLE_SECONDS = 300.0
READ_TIMEOUT_MARGIN_SECONDS = 60.0
STALL_RETRIES = 1
INITIAL_RETRY_DELAY_S = 0.5
RETRY_BACKOFF = 1.5
MAX_RETRY_DELAY_S = 30.0
MAX_RETRY_JITTER_S = 1.0
MAX_BACKOFF_EXPONENT = 16
RETRYABLE_HTTP_STATUSES = frozenset({408, 429, 500, 502, 503, 504})

_SHARED_CLIENTS: weakref.WeakKeyDictionary[asyncio.AbstractEventLoop, httpx.AsyncClient] = (
    weakref.WeakKeyDictionary()
)
_DURATION_RE = re.compile(r"^\s*(\d+(?:\.\d+)?)s\s*$")


class IncompleteProviderStream(RuntimeError):
    """A response stream ended with an error finish reason or without a finish reason."""


class StreamStalledError(TimeoutError):
    """A model stream stayed silent past first-output or idle limits."""

    def __init__(self, kind: str, seconds: float) -> None:
        if kind == "first_output":
            msg = f"The model produced no output within {seconds:g} seconds"
        elif kind == "idle":
            msg = f"The model stopped producing output for {seconds:g} seconds and the call was stopped"
        else:
            msg = f"The model provider sent no data for {seconds:g} seconds and the call was stopped"
        super().__init__(msg)
        self.kind = kind
        self.seconds = seconds


class ProviderHTTPStatusError(RuntimeError):
    """HTTP error carrying status, parsed retry delay, and raw body."""

    def __init__(
        self,
        *,
        status_code: int,
        url: str,
        body: str,
        retry_after_s: float | None = None,
        retry_source: str | None = None,
    ) -> None:
        hint = ""
        if retry_after_s is not None:
            secs = max(1, min(60, int(retry_after_s + 0.999)))
            hint = f" [retry_after={secs}s]"
        super().__init__(f"HTTP {status_code}{hint} from {url}: {body[:500]}")
        self.status_code = status_code
        self.url = url
        self.body = body
        self.retry_after_s = retry_after_s
        self.retry_source = retry_source


def parse_google_retry_info_seconds(body: str) -> float | None:
    """Parse `google.rpc.RetryInfo.retryDelay` from a 429/503 JSON error body."""
    if not body or "RetryInfo" not in body and "retryDelay" not in body:
        return None
    try:
        parsed = json.loads(body)
    except (ValueError, TypeError):
        return None

    def _extract_delay(obj: Any) -> float | None:
        if isinstance(obj, dict):
            type_url = obj.get("@type")
            if (
                isinstance(type_url, str)
                and type_url.endswith("google.rpc.RetryInfo")
                and isinstance(obj.get("retryDelay"), str)
            ):
                m = _DURATION_RE.match(obj["retryDelay"])
                if m:
                    return max(0.0, float(m.group(1)))
            if isinstance(obj.get("retryDelay"), str):
                m = _DURATION_RE.match(obj["retryDelay"])
                if m:
                    return max(0.0, float(m.group(1)))
            for v in obj.values():
                if (res := _extract_delay(v)) is not None:
                    return res
        elif isinstance(obj, list):
            for item in obj:
                if (res := _extract_delay(item)) is not None:
                    return res
        return None

    return _extract_delay(parsed)


def parse_retry_after_header_seconds(
    headers: Mapping[str, str], *, now: datetime | None = None
) -> float | None:
    ra_ms = headers.get("retry-after-ms", "").strip()
    if ra_ms:
        try:
            return max(0.0, float(ra_ms) / 1000.0)
        except ValueError:
            pass
    ra_s = headers.get("retry-after", "").strip()
    if not ra_s:
        return None
    try:
        return max(0.0, float(ra_s))
    except ValueError:
        pass
    try:
        dt = parsedate_to_datetime(ra_s)
    except (TypeError, ValueError, OverflowError):
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=UTC)
    ref = now or datetime.now(UTC)
    return max(0.0, (dt - ref).total_seconds())


def resolve_retry_delay_seconds(
    *,
    status_code: int,
    headers: Mapping[str, str],
    body: str,
) -> tuple[float | None, str | None]:
    """Honor `google.rpc.RetryInfo.retryDelay` on 429/503 over HTTP headers."""
    if status_code in {429, 503}:
        if (body_delay := parse_google_retry_info_seconds(body)) is not None:
            return body_delay, "retry_info"
    if (hdr_delay := parse_retry_after_header_seconds(headers)) is not None:
        return hdr_delay, "header"
    return None, None


def compute_backoff_delay_s(
    retry_attempt: int,
    *,
    explicit_delay_s: float | None = None,
    jitter_s: float | None = None,
) -> float:
    if explicit_delay_s is not None:
        return min(max(0.0, explicit_delay_s), MAX_RETRY_DELAY_S)
    exp = min(max(0, retry_attempt - 1), MAX_BACKOFF_EXPONENT)
    backoff = INITIAL_RETRY_DELAY_S * (RETRY_BACKOFF**exp)
    jit = random.uniform(0.0, MAX_RETRY_JITTER_S) if jitter_s is None else jitter_s
    return min(backoff + jit, MAX_RETRY_DELAY_S)


def _get_shared_client(timeout_s: float) -> httpx.AsyncClient:
    """Reuse a keep-alive HTTP client per event loop so turns skip TCP/TLS handshakes."""
    loop = asyncio.get_running_loop()
    client = _SHARED_CLIENTS.get(loop)
    if client is None or client.is_closed:
        read_timeout = max(timeout_s, max(FIRST_OUTPUT_WAIT_SECONDS, STREAM_IDLE_SECONDS) + READ_TIMEOUT_MARGIN_SECONDS)
        client = httpx.AsyncClient(
            timeout=httpx.Timeout(timeout_s, connect=15.0, read=read_timeout, write=30.0, pool=15.0),
            limits=httpx.Limits(
                max_keepalive_connections=12,
                max_connections=24,
                keepalive_expiry=30.0,
            ),
            follow_redirects=False,
        )
        _SHARED_CLIENTS[loop] = client
    return client


async def _reset_shared_client() -> None:
    loop = asyncio.get_running_loop()
    old = _SHARED_CLIENTS.pop(loop, None)
    if old is not None and not old.is_closed:
        await old.aclose()


async def _iter_sse_lines(response: httpx.Response) -> AsyncIterator[str]:
    """Iterate SSE lines strictly on CRLF/LF/CR (matching mistral-vibe `_sse.py`).

    `httpx.Response.aiter_lines()` uses `str.splitlines()` which breaks on U+2028/U+0085
    that are legal unescaped characters inside JSON strings.
    """
    buffer = b""
    async for chunk in response.aiter_bytes():
        buffer += chunk
        held_cr = buffer.endswith(b"\r")
        if held_cr:
            buffer = buffer[:-1]
        *lines, buffer = (
            buffer.replace(b"\r\n", b"\n").replace(b"\r", b"\n").split(b"\n")
        )
        if held_cr:
            buffer += b"\r"
        for raw in lines:
            yield raw.decode("utf-8", errors="replace")
    if buffer:
        yield buffer.removesuffix(b"\r").decode("utf-8", errors="replace")


async def _iter_sse_json_events(
    response: httpx.Response,
    *,
    first_output_timeout_s: float | None = None,
    idle_timeout_s: float | None = None,
) -> AsyncIterator[dict[str, Any]]:
    """Parse SSE frames with full tolerance: multi-line `data:`, comments `:...`, keepalives, and stall guard."""
    data_lines: list[str] = []
    saw_output = False
    line_iter = _iter_sse_lines(response).__aiter__()

    async def _next_line() -> str | None:
        limit = (
            idle_timeout_s
            if saw_output
            else first_output_timeout_s
        )
        try:
            if limit is not None and limit > 0:
                return await asyncio.wait_for(line_iter.__anext__(), timeout=limit)
            return await line_iter.__anext__()
        except StopAsyncIteration:
            return None
        except TimeoutError as exc:
            kind = "idle" if saw_output else "first_output"
            raise StreamStalledError(kind, float(limit or 0.0)) from exc

    def _flush() -> tuple[bool, dict[str, Any] | None]:
        if not data_lines:
            return False, None
        raw = "\n".join(data_lines).strip()
        data_lines.clear()
        if not raw:
            return False, None
        if raw == "[DONE]":
            return True, None
        return False, json.loads(raw)

    while True:
        raw_line = await _next_line()
        if raw_line is None:
            done, obj = _flush()
            if not done and obj is not None:
                yield obj
            return
        line = raw_line.rstrip("\r")
        if line == "":
            done, obj = _flush()
            if done:
                return
            if obj is not None:
                saw_output = True
                yield obj
            continue
        if line.startswith(":"):
            # SSE comment / keepalive frame
            continue
        if line.startswith("data:"):
            val = line[5:]
            if val.startswith(" "):
                val = val[1:]
            stripped = val.strip()
            if stripped == "[DONE]" and not data_lines:
                return
            data_lines.append(val)
            # Single-line JSON convenience: if no blank line separates events (some mock/test streams),
            # flush immediately when valid JSON object completes on a single line.
            if len(data_lines) == 1 and stripped.startswith("{") and stripped.endswith("}"):
                try:
                    parsed = json.loads(stripped)
                except json.JSONDecodeError:
                    continue
                data_lines.clear()
                saw_output = True
                yield parsed
            continue
        # Ignore event:, id:, retry: fields cleanly.


RefreshHeadersCallback = Callable[[], Awaitable[Mapping[str, str] | None]]


def _wire_log_record(provider: str, model: str, http_status: int, wall_ms: int) -> None:
    """Append one compact wire-level HTTP POST record when `LAZYBUG_WIRE_LOG=1`."""
    if os.environ.get("LAZYBUG_WIRE_LOG", "").strip().lower() not in {"1", "true", "yes"}:
        return
    override = os.environ.get("LAZYBUG_WIRE_LOG_PATH", "").strip()
    path = Path(override) if override else (Path.home() / ".lazybug" / "logs" / "wire-http.jsonl")
    entry = {
        "ts": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "provider": provider,
        "model": model,
        "http_status": http_status,
        "wall_ms": wall_ms,
    }
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(entry, separators=(",", ":")) + "\n")
    except OSError:
        pass


async def stream_sse_json(
    url: str,
    *,
    headers: Mapping[str, str],
    payload: dict[str, Any],
    timeout_s: float = 180.0,
    client: httpx.AsyncClient | None = None,
    on_unauthorized: RefreshHeadersCallback | None = None,
    first_output_timeout_s: float | None = None,
    idle_timeout_s: float | None = None,
    max_retry_elapsed_s: float = 0.0,
    provider: str = "",
    model: str = "",
) -> AsyncIterator[dict[str, Any]]:
    """POST JSON and yield parsed SSE `data:` payloads until `[DONE]`.

    Supports `oauthTransport` / Mistral 401 semantics: when `on_unauthorized` is supplied,
    HTTP 401 invokes it once to force-refresh credentials and replays the request once.
    """
    active_headers: dict[str, str] = dict(headers)
    replayed_401 = False
    started = time.monotonic()
    retry_attempt = 0
    stall_retries = 0
    transport_retries = 0

    while True:
        active_client = client or _get_shared_client(timeout_s)
        yielded_any = False
        read_timeout = (
            max(timeout_s, max(first_output_timeout_s or 0.0, idle_timeout_s or 0.0) + READ_TIMEOUT_MARGIN_SECONDS)
            if (first_output_timeout_s or idle_timeout_s)
            else timeout_s
        )
        req_started = time.monotonic()
        try:
            async with active_client.stream(
                "POST",
                url,
                headers=active_headers,
                json=payload,
                timeout=httpx.Timeout(
                    timeout_s, connect=15.0, read=read_timeout, write=30.0, pool=15.0
                ),
            ) as response:
                _wire_log_record(
                    provider,
                    model,
                    response.status_code,
                    int((time.monotonic() - req_started) * 1000),
                )
                if response.status_code == 401 and on_unauthorized is not None and not replayed_401:
                    await response.aread()
                    replayed_401 = True
                    new_headers = await on_unauthorized()
                    if new_headers is not None:
                        active_headers = dict(new_headers)
                        continue
                    raise ProviderHTTPStatusError(
                        status_code=401,
                        url=url,
                        body="Unauthorized after token refresh",
                    )

                if not response.is_success:
                    body = (await response.aread()).decode("utf-8", errors="replace")
                    delay_s, delay_src = resolve_retry_delay_seconds(
                        status_code=response.status_code,
                        headers=response.headers,
                        body=body,
                    )
                    err = ProviderHTTPStatusError(
                        status_code=response.status_code,
                        url=url,
                        body=body,
                        retry_after_s=delay_s,
                        retry_source=delay_src,
                    )
                    if (
                        max_retry_elapsed_s > 0
                        and response.status_code in RETRYABLE_HTTP_STATUSES
                    ):
                        requested_wait_s = (
                            max(0.0, delay_s)
                            if delay_s is not None
                            else compute_backoff_delay_s(retry_attempt + 1)
                        )
                        if time.monotonic() - started + requested_wait_s <= max_retry_elapsed_s:
                            wait_s = min(requested_wait_s, MAX_RETRY_DELAY_S)
                            retry_attempt += 1
                            await asyncio.sleep(wait_s)
                            continue
                    raise err

                async for obj in _iter_sse_json_events(
                    response,
                    first_output_timeout_s=first_output_timeout_s,
                    idle_timeout_s=idle_timeout_s,
                ):
                    yielded_any = True
                    yield obj
            return
        except StreamStalledError:
            if not yielded_any and max_retry_elapsed_s > 0 and stall_retries < STALL_RETRIES:
                stall_retries += 1
                retry_attempt += 1
                await asyncio.sleep(compute_backoff_delay_s(retry_attempt))
                continue
            raise
        except httpx.ReadTimeout as exc:
            # Post-header read timeout maps onto StreamStalledError('read_timeout') per mistral-vibe stream_guard.
            stall_err = StreamStalledError("read_timeout", read_timeout)
            if not yielded_any and max_retry_elapsed_s > 0 and stall_retries < STALL_RETRIES:
                stall_retries += 1
                retry_attempt += 1
                await asyncio.sleep(compute_backoff_delay_s(retry_attempt))
                continue
            raise stall_err from exc
        except httpx.TransportError as exc:
            # Stale keep-alive sockets on Cloudflare/Mistral/AGY drop before first byte;
            # evict pooled client and retry once silently, or within retry budget.
            if client is None and not yielded_any and transport_retries < 1:
                transport_retries += 1
                await _reset_shared_client()
                continue
            if not yielded_any and max_retry_elapsed_s > 0:
                wait_s = compute_backoff_delay_s(retry_attempt + 1)
                if time.monotonic() - started + wait_s <= max_retry_elapsed_s:
                    retry_attempt += 1
                    await asyncio.sleep(wait_s)
                    continue
            raise RuntimeError(f"{type(exc).__name__}: {exc}") from exc


async def request_with_oauth_retry(
    client: httpx.AsyncClient,
    method: str,
    url: str,
    *,
    headers: Mapping[str, str],
    json_body: Any = None,
    params: Mapping[str, Any] | None = None,
    on_unauthorized: RefreshHeadersCallback | None = None,
) -> httpx.Response:
    """Unary HTTP request with single 401 force-refresh + replay (`oauthTransport` semantics)."""
    active_headers = dict(headers)
    kwargs: dict[str, Any] = {"headers": active_headers}
    if json_body is not None:
        kwargs["json"] = json_body
    if params is not None:
        kwargs["params"] = params
    resp = await client.request(method, url, **kwargs)
    if resp.status_code == 401 and on_unauthorized is not None:
        new_headers = await on_unauthorized()
        if new_headers is not None:
            kwargs["headers"] = dict(new_headers)
            resp = await client.request(method, url, **kwargs)
    return resp


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
