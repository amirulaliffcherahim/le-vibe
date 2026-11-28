"""Example extension: append timestamped tool call audit records to ~/.lazybug/tool-audit.jsonl."""

from __future__ import annotations

import json
from pathlib import Path
import time
from typing import Any

from lazybug.extensions import ExtensionHost


def _audit_path() -> Path:
    return Path.home() / ".lazybug" / "tool-audit.jsonl"


def _write_record(record: dict[str, Any]) -> None:
    path = _audit_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(record, ensure_ascii=False) + "\n")


def register(host: ExtensionHost) -> None:
    def on_before(payload: dict[str, Any]) -> dict[str, Any]:
        _write_record(
            {
                "ts": int(time.time()),
                "event": "before_tool_call",
                "tool": payload.get("tool"),
                "arguments": payload.get("arguments"),
            }
        )
        return payload

    def on_after(payload: dict[str, Any]) -> dict[str, Any]:
        _write_record(
            {
                "ts": int(time.time()),
                "event": "after_tool_call",
                "tool": payload.get("tool"),
                "is_error": bool(payload.get("is_error", False)),
            }
        )
        return payload

    host.on("before_tool_call", on_before)
    host.on("after_tool_call", on_after)
