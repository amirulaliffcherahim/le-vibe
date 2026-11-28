"""Python extension, hook, and SKILL.md loader for lazybug."""

from __future__ import annotations

from collections.abc import Awaitable, Callable, Sequence
from dataclasses import dataclass, field
import importlib.util
from pathlib import Path
import re
import sys
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
    name = skill_md.parent.name if skill_md.name.upper() == "SKILL.MD" else skill_md.stem
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
    seen_paths: set[Path] = set()
    for root in roots:
        if not root.exists():
            continue
        for skill_md in sorted(root.rglob("*.md")):
            if skill_md.name.startswith("_") or skill_md.name.upper() == "README.MD":
                continue
            resolved = skill_md.resolve()
            if resolved in seen_paths:
                continue
            seen_paths.add(resolved)
            discovered.append(parse_skill_file(skill_md))
    return discovered


@dataclass(frozen=True, slots=True)
class PythonCommandSpec:
    name: str
    description: str
    handler: Callable[[str], str | Awaitable[str]] = field(repr=False)


class ExtensionHost:
    def __init__(self) -> None:
        self._hooks: dict[str, list[HookCallback]] = {}
        self._tools: dict[str, PythonToolSpec] = {}
        self._commands: dict[str, PythonCommandSpec] = {}
        self.load_errors: list[str] = []

    def clear(self) -> None:
        self._hooks.clear()
        self._tools.clear()
        self._commands.clear()
        self.load_errors.clear()

    def on(self, event: str, callback: HookCallback) -> None:
        self._hooks.setdefault(event, []).append(callback)

    def register_tool(self, spec: PythonToolSpec) -> None:
        self._tools[spec.name] = spec

    def register_command(
        self,
        name: str,
        description: str,
        handler: Callable[[str], str | Awaitable[str]],
    ) -> None:
        norm = name if name.startswith("/") else f"/{name}"
        self._commands[norm] = PythonCommandSpec(name=norm, description=description, handler=handler)

    def list_tools(self) -> tuple[PythonToolSpec, ...]:
        return tuple(self._tools.values())

    def list_commands(self) -> tuple[PythonCommandSpec, ...]:
        return tuple(self._commands.values())

    def hook_events(self) -> dict[str, int]:
        return {k: len(v) for k, v in sorted(self._hooks.items())}

    async def run_command(self, name: str, args: str = "") -> str:
        norm = name if name.startswith("/") else f"/{name}"
        if norm not in self._commands:
            raise KeyError(f"Unknown plugin slash command: {norm!r}")
        res = self._commands[norm].handler(args)
        if isinstance(res, Awaitable):
            res = await res
        return str(res)

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


def _detect_lifecycle_and_desc(py_file: Path) -> tuple[str, str]:
    """Inspect a Python extension file without importing it to read LIFECYCLE / DESCRIPTION."""
    try:
        head = py_file.read_text(encoding="utf-8", errors="replace")[:4096]
    except OSError:
        return "lazy", ""
    lifecycle = ""
    if m := re.search(r"""^\s*LIFECYCLE\s*=\s*["'](lazy|eager)["']""", head, re.MULTILINE):
        lifecycle = m.group(1).lower()
    if not lifecycle:
        # Tool-providing extensions default to lazy; pure hook/policy extensions stay eager.
        lifecycle = "lazy" if "register_tool" in head else "eager"
    desc = ""
    if m := re.search(r"""^\s*DESCRIPTION\s*=\s*["']([^"'\n]*)["']""", head, re.MULTILINE):
        desc = m.group(1).strip()
    return lifecycle, desc


@dataclass(frozen=True, slots=True)
class ExtensionManifest:
    name: str
    path: Path
    lifecycle: str
    description: str = ""


def _builtin_extension_dir() -> Path:
    return Path(__file__).resolve().parent / "builtin"


def discover_extension_manifests(dirs: Sequence[Path]) -> list[ExtensionManifest]:
    manifests: list[ExtensionManifest] = []
    seen: set[str] = set()
    search_dirs = [*dirs, _builtin_extension_dir()]
    for directory in search_dirs:
        if not directory.exists() or not directory.is_dir():
            continue
        for py_file in sorted(directory.glob("*.py")):
            if py_file.name.startswith("_"):
                continue
            name = py_file.stem
            if name in seen:
                continue
            seen.add(name)
            lifecycle, desc = _detect_lifecycle_and_desc(py_file)
            manifests.append(
                ExtensionManifest(name=name, path=py_file, lifecycle=lifecycle, description=desc)
            )
    return manifests


def load_extension_file(host: ExtensionHost, py_file: Path) -> str | None:
    mod_name = f"lazybug_ext_{py_file.stem}_{abs(hash(str(py_file.resolve())))}"
    try:
        spec = importlib.util.spec_from_file_location(mod_name, py_file)
        if spec is None or spec.loader is None:
            return None
        module = importlib.util.module_from_spec(spec)
        sys.modules[mod_name] = module
        spec.loader.exec_module(module)
        register_fn = getattr(module, "register", None)
        if callable(register_fn):
            register_fn(host)
            return str(py_file)
    except Exception as exc:
        host.load_errors.append(f"{py_file}: {exc}")
    return None


def load_extensions_from_dirs(
    host: ExtensionHost, dirs: Sequence[Path], *, eager_only: bool = False
) -> list[str]:
    loaded: list[str] = []
    for directory in dirs:
        if not directory.exists() or not directory.is_dir():
            continue
        for py_file in sorted(directory.glob("*.py")):
            if py_file.name.startswith("_"):
                continue
            if eager_only:
                lifecycle, _ = _detect_lifecycle_and_desc(py_file)
                if lifecycle != "eager":
                    continue
            if path_str := load_extension_file(host, py_file):
                loaded.append(path_str)
    return loaded


__all__ = [
    "ExtensionHost",
    "ExtensionManifest",
    "HookCallback",
    "PythonToolSpec",
    "SkillSpec",
    "ToolHandler",
    "discover_extension_manifests",
    "discover_skills",
    "load_extension_file",
    "load_extensions_from_dirs",
    "parse_skill_file",
]
