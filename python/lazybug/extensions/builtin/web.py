from __future__ import annotations

LIFECYCLE = "lazy"
DESCRIPTION = "Web search and page fetch tools (web_search, fetch_content)"


def register(host) -> None:
    from lazybug.extensions import PythonToolSpec
    from lazybug.providers.web import fetch_content, web_search

    async def _web_search(args: dict) -> str:
        raw_queries = args.get("queries")
        if isinstance(raw_queries, list) and raw_queries:
            queries = [str(q) for q in raw_queries if str(q).strip()]
        else:
            q = str(args.get("query") or "").strip()
            queries = [q] if q else []
        num = int(args.get("numResults") or 5)
        return await web_search(queries, num_results=num)

    async def _fetch_content(args: dict) -> str:
        url = str(args.get("url") or "").strip()
        mode = str(args.get("mode") or "readable").strip()
        return await fetch_content(url, mode=mode)

    host.register_tool(
        PythonToolSpec(
            name="web_search",
            description="Search the web (prefer `queries` with 2-4 varied angles). Returns titles, URLs, snippets.",
            parameters={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "queries": {"type": "array", "items": {"type": "string"}},
                    "numResults": {"type": "integer"},
                },
            },
            handler=_web_search,
        )
    )
    host.register_tool(
        PythonToolSpec(
            name="fetch_content",
            description="Fetch a page and extract readable Markdown (default), or the exact body with mode='raw'.",
            parameters={
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": {"type": "string"},
                    "mode": {"type": "string", "enum": ["readable", "raw"]},
                },
            },
            handler=_fetch_content,
        )
    )
