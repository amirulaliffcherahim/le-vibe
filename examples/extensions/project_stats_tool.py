"""Example extension: registers a custom `project_stats` tool counting files and lines."""

from __future__ import annotations

from collections import Counter
from pathlib import Path
from typing import Any

from lazybug.extensions import ExtensionHost, PythonToolSpec

_SKIP_DIRS = {".git", "target", "node_modules", ".venv", "__pycache__", "dist", "build"}


def _project_stats(args: dict[str, Any]) -> dict[str, Any]:
    root = Path(str(args.get("path") or ".")).resolve()
    if not root.exists():
        return {"error": f"Path does not exist: {root}"}

    counts: Counter[str] = Counter()
    lines_by_ext: Counter[str] = Counter()
    total_files = 0
    total_lines = 0

    for item in root.rglob("*"):
        if any(part in _SKIP_DIRS for part in item.parts):
            continue
        if not item.is_file():
            continue
        ext = item.suffix.lower() or "<no-ext>"
        counts[ext] += 1
        total_files += 1
        try:
            n_lines = sum(1 for _ in item.open("r", encoding="utf-8", errors="ignore"))
        except OSError:
            n_lines = 0
        lines_by_ext[ext] += n_lines
        total_lines += n_lines

    top = [
        {"ext": ext, "files": counts[ext], "lines": lines_by_ext[ext]}
        for ext, _ in counts.most_common(12)
    ]
    return {"root": str(root), "total_files": total_files, "total_lines": total_lines, "by_extension": top}


def register(host: ExtensionHost) -> None:
    host.register_tool(
        PythonToolSpec(
            name="project_stats",
            description="Count files and lines grouped by file extension under a directory",
            parameters={
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory to inspect (default: '.')"}
                },
            },
            handler=_project_stats,
        )
    )
