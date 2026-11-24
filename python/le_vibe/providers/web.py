"""Zero-dependency (httpx + stdlib) web_search and fetch_content for le-vibe."""

from __future__ import annotations

import asyncio
from html.parser import HTMLParser
import os
import re
from typing import Any
from urllib.parse import parse_qs, unquote, urlparse

import httpx

_USER_AGENT = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
)


class _DdgHtmlParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.results: list[dict[str, str]] = []
        self._in_title = False
        self._in_snippet = False
        self._cur_title: list[str] = []
        self._cur_url: str = ""
        self._cur_snippet: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attr_map = dict(attrs)
        cls = attr_map.get("class") or ""
        if tag == "a" and "result__a" in cls:
            self._in_title = True
            self._cur_title = []
            href = attr_map.get("href") or ""
            if "uddg=" in href:
                qs = parse_qs(urlparse(href).query)
                href = unquote(qs.get("uddg", [href])[0])
            self._cur_url = href
        elif tag in ("a", "div", "span") and "result__snippet" in cls:
            self._in_snippet = True
            self._cur_snippet = []

    def handle_endtag(self, tag: str) -> None:
        if tag == "a" and self._in_title:
            self._in_title = False
            title = " ".join("".join(self._cur_title).split())
            if title and self._cur_url:
                self.results.append({"title": title, "url": self._cur_url, "snippet": ""})
        elif self._in_snippet and tag in ("a", "div", "span"):
            self._in_snippet = False
            snippet = " ".join("".join(self._cur_snippet).split())
            if self.results and snippet and not self.results[-1]["snippet"]:
                self.results[-1]["snippet"] = snippet

    def handle_data(self, data: str) -> None:
        if self._in_title:
            self._cur_title.append(data)
        elif self._in_snippet:
            self._cur_snippet.append(data)


class _ReadableHtmlParser(HTMLParser):
    _SKIP_TAGS = frozenset({"script", "style", "noscript", "svg", "nav", "footer", "iframe"})
    _BLOCK_TAGS = frozenset({"p", "div", "li", "br", "tr", "section", "article", "pre", "blockquote"})

    def __init__(self) -> None:
        super().__init__()
        self._skip_depth = 0
        self._in_title = False
        self._heading_level = 0
        self.title_parts: list[str] = []
        self.chunks: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        t = tag.lower()
        if t in self._SKIP_TAGS:
            self._skip_depth += 1
            return
        if self._skip_depth > 0:
            return
        if t == "title":
            self._in_title = True
        elif t in ("h1", "h2", "h3", "h4"):
            self._heading_level = int(t[1])
            self.chunks.append(f"\n\n{'#' * self._heading_level} ")
        elif t == "li":
            self.chunks.append("\n- ")
        elif t in self._BLOCK_TAGS:
            self.chunks.append("\n")

    def handle_endtag(self, tag: str) -> None:
        t = tag.lower()
        if t in self._SKIP_TAGS:
            self._skip_depth = max(0, self._skip_depth - 1)
            return
        if self._skip_depth > 0:
            return
        if t == "title":
            self._in_title = False
        elif t in ("h1", "h2", "h3", "h4"):
            self._heading_level = 0
            self.chunks.append("\n")
        elif t in ("p", "pre", "blockquote"):
            self.chunks.append("\n")

    def handle_data(self, data: str) -> None:
        if self._skip_depth > 0:
            return
        if self._in_title:
            self.title_parts.append(data)
        else:
            self.chunks.append(data)

    def render(self) -> str:
        title = " ".join("".join(self.title_parts).split())
        raw = "".join(self.chunks)
        lines = [" ".join(line.split()) for line in raw.splitlines()]
        compact = re.sub(r"\n{3,}", "\n\n", "\n".join(lines)).strip()
        if title:
            return f"# {title}\n\n{compact}" if compact else f"# {title}"
        return compact


async def _search_brave(client: httpx.AsyncClient, query: str, num: int, api_key: str) -> list[dict[str, str]]:
    resp = await client.get(
        "https://api.search.brave.com/res/v1/web/search",
        params={"q": query, "count": num},
        headers={"Accept": "application/json", "X-Subscription-Token": api_key},
    )
    if not resp.is_success:
        return []
    items = (resp.json().get("web") or {}).get("results") or []
    return [
        {
            "title": str(it.get("title") or ""),
            "url": str(it.get("url") or ""),
            "snippet": str(it.get("description") or ""),
        }
        for it in items[:num]
        if it.get("url")
    ]


async def _search_ddg(client: httpx.AsyncClient, query: str, num: int) -> list[dict[str, str]]:
    resp = await client.post(
        "https://html.duckduckgo.com/html/",
        data={"q": query},
        headers={"User-Agent": _USER_AGENT, "Content-Type": "application/x-www-form-urlencoded"},
    )
    if not resp.is_success:
        return []
    parser = _DdgHtmlParser()
    parser.feed(resp.text)
    return parser.results[:num]


async def web_search(queries: list[str], *, num_results: int = 5) -> str:
    clean_queries = [q.strip() for q in queries if isinstance(q, str) and q.strip()][:4]
    if not clean_queries:
        return "web_search error: provide `query` or non-empty `queries`."
    num = max(1, min(10, int(num_results)))
    brave_key = os.environ.get("BRAVE_API_KEY", "").strip()

    async with httpx.AsyncClient(timeout=20.0, follow_redirects=True) as client:
        async def _run_one(q: str) -> tuple[str, list[dict[str, str]]]:
            try:
                if brave_key:
                    res = await _search_brave(client, q, num, brave_key)
                    if res:
                        return q, res
                return q, await _search_ddg(client, q, num)
            except Exception as exc:  # noqa: BLE001
                return q, [{"title": f"Search error: {exc}", "url": "", "snippet": ""}]

        batches = await asyncio.gather(*(_run_one(q) for q in clean_queries))

    sections: list[str] = []
    for q, rows in batches:
        lines = [f"## Search: {q}"]
        if not rows:
            lines.append("(no results)")
        else:
            for idx, r in enumerate(rows, 1):
                lines.append(f"{idx}. {r['title']}")
                if r["url"]:
                    lines.append(f"   {r['url']}")
                if r["snippet"]:
                    lines.append(f"   {r['snippet']}")
        sections.append("\n".join(lines))
    return "\n\n".join(sections)


async def fetch_content(url: str, *, mode: str = "readable", max_chars: int = 24_000) -> str:
    parsed = urlparse(url.strip())
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        return f"fetch_content error: URL must use http:// or https:// (got {url!r})."

    async with httpx.AsyncClient(timeout=25.0, follow_redirects=True) as client:
        resp = await client.get(url.strip(), headers={"User-Agent": _USER_AGENT})
        if not resp.is_success:
            return f"fetch_content error: HTTP {resp.status_code} from {url}"
        ctype = (resp.headers.get("content-type") or "").lower()
        body = resp.text

    if mode == "raw" or ("html" not in ctype and "<html" not in body[:500].lower()):
        out = body
    else:
        parser = _ReadableHtmlParser()
        parser.feed(body)
        out = parser.render()

    if len(out) > max_chars:
        out = out[:max_chars] + f"\n\n...[truncated at {max_chars} chars]"
    return f"URL: {resp.url}\n\n{out}"
