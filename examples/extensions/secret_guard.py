"""Example extension: blocks read/write/edit access to sensitive credential files."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from le_vibe.extensions import ExtensionHost

_SENSITIVE_NAMES = {".env", ".env.local", ".env.production", "auth.json", "agy_oauth.json"}
_SENSITIVE_SUFFIXES = (".pem", ".key", ".p12", ".pfx")


def _is_sensitive(target: str) -> bool:
    p = Path(target)
    name = p.name.lower()
    if name in _SENSITIVE_NAMES or name.startswith(".env."):
        return True
    return name.endswith(_SENSITIVE_SUFFIXES)


def register(host: ExtensionHost) -> None:
    def guard(payload: dict[str, Any]) -> dict[str, Any]:
        tool = str(payload.get("tool") or "")
        args = payload.get("arguments") or {}
        if tool in {"read", "write", "edit"} and isinstance(args, dict):
            target = str(args.get("path") or "")
            if target and _is_sensitive(target):
                return {
                    **payload,
                    "blocked": True,
                    "reason": f"Blocked by secret_guard: access to sensitive file {target!r} is disallowed.",
                }
        return payload

    host.on("before_tool_call", guard)
