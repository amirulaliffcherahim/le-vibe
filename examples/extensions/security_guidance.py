"""Extension: security_guidance — blocks high-risk code/shell patterns before execution."""

from __future__ import annotations

import re
from typing import Any

from lazybug.extensions import ExtensionHost

_CODE_PATTERNS: tuple[tuple[re.Pattern[str], str], ...] = (
    (re.compile(r"\bpickle\.loads?\s*\("), "Unsafe deserialization via pickle.load(s)"),
    (re.compile(r"subprocess\.[A-Za-z_]+\([^)]*shell\s*=\s*True"), "Command injection risk: subprocess with shell=True"),
    (re.compile(r"\bdangerouslySetInnerHTML\b"), "XSS risk: dangerouslySetInnerHTML"),
    (re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"), "Hardcoded private key material"),
    (re.compile(r"\b(?:sk-[A-Za-z0-9]{24,}|ghp_[A-Za-z0-9]{30,}|AKIA[0-9A-Z]{16})\b"), "Hardcoded API key or token"),
)

_SHELL_PATTERNS: tuple[tuple[re.Pattern[str], str], ...] = (
    (re.compile(r"curl\s+[^|\n]+\|\s*(?:ba)?sh\b", re.IGNORECASE), "Piped remote script execution (curl | sh)"),
    (re.compile(r"wget\s+[^|\n]+\|\s*(?:ba)?sh\b", re.IGNORECASE), "Piped remote script execution (wget | sh)"),
    (re.compile(r"\b(?:iex|Invoke-Expression)\s*\(?\s*(?:irm|Invoke-RestMethod)", re.IGNORECASE), "Remote PowerShell execution (irm | iex)"),
)


def register(host: ExtensionHost) -> None:
    def scan(payload: dict[str, Any]) -> dict[str, Any]:
        tool = str(payload.get("tool") or "")
        args = payload.get("arguments") or {}
        if not isinstance(args, dict):
            return payload

        if tool in {"write", "edit"}:
            body = str(args.get("content") or args.get("newText") or args.get("new_text") or "")
            for rx, reason in _CODE_PATTERNS:
                if rx.search(body):
                    return {
                        **payload,
                        "blocked": True,
                        "reason": f"Blocked by security_guidance: {reason}",
                    }
        elif tool in {"bash", "powershell"}:
            cmd = str(args.get("command") or "")
            for rx, reason in _SHELL_PATTERNS:
                if rx.search(cmd):
                    return {
                        **payload,
                        "blocked": True,
                        "reason": f"Blocked by security_guidance: {reason}",
                    }
        return payload

    host.on("before_tool_call", scan)
