"""Python policy & permission engine for le-vibe (ported from Mistral Vibe)."""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
import fnmatch
from pathlib import Path
from typing import Literal


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
            "ls": ToolPermission.ALWAYS,
            "todo": ToolPermission.ALWAYS,
            "ask_user_question": ToolPermission.ALWAYS,
            "write": ToolPermission.ASK,
            "edit": ToolPermission.ASK,
            "bash": ToolPermission.ASK,
            "powershell": ToolPermission.ASK,
        }
        self._trusted_folders: tuple[Path, ...] = tuple(p.resolve() for p in trusted_folders)
        self.yolo: bool = yolo

    def reset(self) -> None:
        self._rules.clear()
        self._tool_permissions.clear()

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
        if command is not None and (reason := is_command_blocked(command)):
            return PolicyDecision(action="deny", reason=reason)

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
            rp = RequiredPermission(scope=PermissionScope.OUTSIDE_DIRECTORY, invocation_pattern=target_path)
            if self.covers(tool_name, rp):
                return PolicyDecision(action="allow", reason="Matched approved path rule")

        if configured is ToolPermission.ASK:
            return PolicyDecision(action="ask", reason=f"Tool {tool_name!r} requires approval")

        return PolicyDecision(action="allow", reason="Default allow")


__all__ = [
    "ApprovedRule",
    "PermissionScope",
    "PermissionStore",
    "PolicyDecision",
    "RequiredPermission",
    "ToolPermission",
    "is_command_blocked",
    "wildcard_match",
]
