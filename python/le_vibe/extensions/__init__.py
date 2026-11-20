"""Python extension, hook, and SKILL.md loader for le-vibe."""

from __future__ import annotations

from collections.abc import Awaitable, Callable, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml

HookCallback = Callable[[dict[str, Any]], dict[str, Any] | Awaitable[dict[str, Any]] | None]
ToolHandler = Callable[[dict[str, Any]], Any | Awaitable[Any]]


@dataclass(frozen=True, slots=True)
class SkillSpec:
    name: str
    description: str
    path: Path
    body: str


@dataclass(frozen=True, slots=True)
class PythonToolSpec:
    name: str
    description: str
    parameters: dict[str, Any]
    handler: ToolHandler = field(repr=False)


def parse_skill_file(skill_md: Path) -> SkillSpec:
    raw = skill_md.read_text(encoding="utf-8")
    name = skill_md.parent.name
    description = ""
    body = raw

    if raw.startswith("---"):
        parts = raw.split("---", 2)
        if len(parts) == 3:
            meta = yaml.safe_load(parts[1]) or {}
            if isinstance(meta, dict):
                name = str(meta.get("name") or name)
                description = str(meta.get("description") or "")
            body = parts[2].lstrip("\r\n")

    return SkillSpec(name=name, description=description, path=skill_md, body=body)


def discover_skills(roots: Sequence[Path]) -> list[SkillSpec]:
    discovered: list[SkillSpec] = []
    for root in roots:
        if not root.exists():
            continue
        for skill_md in sorted(root.rglob("SKILL.md")):
            discovered.append(parse_skill_file(skill_md))
    return discovered


class ExtensionHost:
    def __init__(self) -> None:
        self._hooks: dict[str, list[HookCallback]] = {}
        self._tools: dict[str, PythonToolSpec] = {}

    def on(self, event: str, callback: HookCallback) -> None:
        self._hooks.setdefault(event, []).append(callback)

    def register_tool(self, spec: PythonToolSpec) -> None:
        self._tools[spec.name] = spec

    def list_tools(self) -> tuple[PythonToolSpec, ...]:
        return tuple(self._tools.values())

    async def invoke_hook(self, event: str, payload: dict[str, Any]) -> dict[str, Any]:
        current = dict(payload)
        for cb in self._hooks.get(event, []):
            result = cb(current)
            if isinstance(result, Awaitable):
                result = await result
            if isinstance(result, dict):
                current = result
        return current

    async def execute_tool(self, name: str, arguments: dict[str, Any]) -> Any:
        if name not in self._tools:
            raise KeyError(f"Unknown Python extension tool: {name!r}")
        result = self._tools[name].handler(arguments)
        if isinstance(result, Awaitable):
            return await result
        return result


__all__ = [
    "ExtensionHost",
    "HookCallback",
    "PythonToolSpec",
    "SkillSpec",
    "ToolHandler",
    "discover_skills",
    "parse_skill_file",
]
