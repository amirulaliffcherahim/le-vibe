"""Python policy & permission engine for le-vibe (ported from Mistral Vibe)."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from enum import StrEnum
import fnmatch
import json
from pathlib import Path
from typing import Any, Literal


class PermissionScope(StrEnum):
    TOOL = "tool"
    OUTSIDE_DIRECTORY = "outside_directory"
    COMMAND = "command"


class ToolPermission(StrEnum):
    ALWAYS = "always"
    ASK = "ask"
    NEVER = "never"


@dataclass(frozen=True, slots=True)
class ApprovedRule:
    tool_name: str
    scope: PermissionScope
    session_pattern: str


@dataclass(frozen=True, slots=True)
class RequiredPermission:
    scope: PermissionScope
    invocation_pattern: str
    literal: bool = False


@dataclass(frozen=True, slots=True)
class PolicyDecision:
    action: Literal["allow", "ask", "deny"]
    reason: str = ""


def wildcard_match(text: str, pattern: str) -> bool:
    if fnmatch.fnmatch(text, pattern):
        return True
    if pattern.endswith(" *") and fnmatch.fnmatch(text, pattern[:-2]):
        return True
    return False


_DANGEROUS_SHELL_PATTERNS: tuple[str, ...] = (
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd if=",
    ":(){:|:&};:",
    "shutdown",
    "reboot",
)


def is_command_blocked(command: str) -> str | None:
    lowered = command.strip().lower()
    for pat in _DANGEROUS_SHELL_PATTERNS:
        if pat in lowered:
            return f"Blocked dangerous shell pattern: {pat}"
    return None


class PermissionStore:
    def __init__(self, *, trusted_folders: tuple[Path, ...] = (), yolo: bool = False) -> None:
        self._rules: list[ApprovedRule] = []
        self._tool_permissions: dict[str, ToolPermission] = {
            "read": ToolPermission.ALWAYS,
            "grep": ToolPermission.ALWAYS,
            "find": ToolPermission.ALWAYS,
            "ls": ToolPermission.ALWAYS,
            "todo": ToolPermission.ALWAYS,
            "ask_user_question": ToolPermission.ALWAYS,
            "memory_search": ToolPermission.ALWAYS,
            "memory_read": ToolPermission.ALWAYS,
            "memory_write": ToolPermission.ALWAYS,
            "memory_delete": ToolPermission.ALWAYS,
            "write": ToolPermission.ASK,
            "edit": ToolPermission.ASK,
            "bash": ToolPermission.ASK,
            "powershell": ToolPermission.ASK,
        }
        self._trusted_folders: tuple[Path, ...] = tuple(p.resolve() for p in trusted_folders)
        self._blocked_commands: list[str] = []
        self._allowed_paths: list[str] = []
        self.loaded_files: list[str] = []
        self.yolo: bool = yolo

    def reset(self) -> None:
        self._rules.clear()
        self._tool_permissions.clear()
        self._blocked_commands.clear()
        self._allowed_paths.clear()
        self.loaded_files.clear()

    def apply_policy(self, data: dict[str, Any], *, base_dir: Path | None = None) -> None:
        if folders := data.get("trusted_folders"):
            if isinstance(folders, list):
                resolved: list[Path] = []
                for f in folders:
                    p = Path(str(f)).expanduser()
                    if not p.is_absolute() and base_dir is not None:
                        p = base_dir / p
                    resolved.append(p.resolve())
                self._trusted_folders = tuple(resolved)

        if perms := data.get("tool_permissions"):
            if isinstance(perms, dict):
                for tool_name, level in perms.items():
                    try:
                        self._tool_permissions[str(tool_name)] = ToolPermission(str(level).lower())
                    except ValueError:
                        continue

        if cmds := data.get("allowed_commands"):
            if isinstance(cmds, list):
                for pat in cmds:
                    for shell_tool in ("bash", "powershell"):
                        self.add_rule(
                            ApprovedRule(
                                tool_name=shell_tool,
                                scope=PermissionScope.COMMAND,
                                session_pattern=str(pat),
                            )
                        )

        if paths := data.get("allowed_paths"):
            if isinstance(paths, list):
                self._allowed_paths.extend(str(p) for p in paths)

        if blocked := data.get("blocked_commands"):
            if isinstance(blocked, list):
                self._blocked_commands.extend(str(b).strip().lower() for b in blocked if str(b).strip())

    @classmethod
    def load_from_files(cls, paths: Sequence[Path], *, yolo: bool = False) -> PermissionStore:
        store = cls(yolo=yolo)
        for path in paths:
            if path.exists() and path.is_file():
                data = load_policy_file(path)
                store.apply_policy(data)
                store.loaded_files.append(str(path))
        return store

    def summary(self) -> dict[str, Any]:
        return {
            "yolo": self.yolo,
            "loaded_files": list(self.loaded_files),
            "trusted_folders": [str(p) for p in self._trusted_folders],
            "tool_permissions": {k: v.value for k, v in sorted(self._tool_permissions.items())},
            "allowed_paths": list(self._allowed_paths),
            "blocked_commands": list(self._blocked_commands),
            "session_rules": [
                {"tool": r.tool_name, "scope": r.scope.value, "pattern": r.session_pattern}
                for r in self._rules
            ],
        }

    def set_yolo(self, enabled: bool) -> None:
        self.yolo = enabled

    def add_rule(self, rule: ApprovedRule) -> None:
        self._rules.append(rule)

    def grant_session_rule(
        self,
        tool_name: str,
        *,
        command: str | None = None,
        target_path: str | None = None,
    ) -> None:
        if command:
            self.add_rule(
                ApprovedRule(
                    tool_name=tool_name,
                    scope=PermissionScope.COMMAND,
                    session_pattern=command,
                )
            )
        elif target_path:
            self.add_rule(
                ApprovedRule(
                    tool_name=tool_name,
                    scope=PermissionScope.OUTSIDE_DIRECTORY,
                    session_pattern=target_path,
                )
            )
        else:
            self.set_tool_permission(tool_name, ToolPermission.ALWAYS)

    def set_tool_permission(self, tool_name: str, permission: ToolPermission) -> None:
        self._tool_permissions[tool_name] = permission

    def get_tool_permission(self, tool_name: str) -> ToolPermission | None:
        return self._tool_permissions.get(tool_name)

    def is_path_trusted(self, path: str | Path) -> bool:
        if not self._trusted_folders:
            return True
        resolved = Path(path).resolve()
        return any(resolved == root or root in resolved.parents for root in self._trusted_folders)

    def covers(self, tool_name: str, rp: RequiredPermission) -> bool:
        def matches(rule: ApprovedRule) -> bool:
            if rp.literal:
                return rule.session_pattern == rp.invocation_pattern
            return wildcard_match(rp.invocation_pattern, rule.session_pattern)

        return any(
            rule.tool_name == tool_name and rule.scope == rp.scope and matches(rule)
            for rule in self._rules
        )

    def evaluate_tool_call(
        self,
        tool_name: str,
        *,
        command: str | None = None,
        target_path: str | None = None,
    ) -> PolicyDecision:
        if command is not None:
            if reason := is_command_blocked(command):
                return PolicyDecision(action="deny", reason=reason)
            lowered = command.strip().lower()
            for pat in self._blocked_commands:
                if pat in lowered:
                    return PolicyDecision(action="deny", reason=f"Blocked by policy rule: {pat}")

        if self.yolo:
            return PolicyDecision(action="allow", reason="Auto-approved (yolo)")

        if target_path is not None and not self.is_path_trusted(target_path):
            rp = RequiredPermission(scope=PermissionScope.OUTSIDE_DIRECTORY, invocation_pattern=target_path)
            if not self.covers(tool_name, rp):
                return PolicyDecision(
                    action="ask",
                    reason=f"Path {target_path!r} is outside trusted folders",
                )

        configured = self.get_tool_permission(tool_name)
        if configured is ToolPermission.NEVER:
            return PolicyDecision(action="deny", reason=f"Tool {tool_name!r} is disabled by policy")
        if configured is ToolPermission.ALWAYS:
            return PolicyDecision(action="allow", reason="Allowed by tool policy")

        if command is not None:
            rp = RequiredPermission(scope=PermissionScope.COMMAND, invocation_pattern=command)
            if self.covers(tool_name, rp):
                return PolicyDecision(action="allow", reason="Matched approved command rule")

        if target_path is not None:
            if any(wildcard_match(target_path, pat) for pat in self._allowed_paths):
                return PolicyDecision(action="allow", reason="Matched policy allowed_paths rule")
            rp = RequiredPermission(scope=PermissionScope.OUTSIDE_DIRECTORY, invocation_pattern=target_path)
            if self.covers(tool_name, rp):
                return PolicyDecision(action="allow", reason="Matched approved path rule")

        if configured is ToolPermission.ASK:
            return PolicyDecision(action="ask", reason=f"Tool {tool_name!r} requires approval")

        return PolicyDecision(action="allow", reason="Default allow")


def load_policy_file(path: Path) -> dict[str, Any]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ValueError(f"Policy file {path} must contain a JSON object")
    return raw


__all__ = [
    "ApprovedRule",
    "PermissionScope",
    "PermissionStore",
    "PolicyDecision",
    "RequiredPermission",
    "ToolPermission",
    "is_command_blocked",
    "load_policy_file",
    "wildcard_match",
]
