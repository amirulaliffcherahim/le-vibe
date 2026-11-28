"""Line-delimited JSON-RPC + streaming NDJSON bridge for the Rust le-vibe harness."""

from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable
from dataclasses import asdict
import json
import os
from pathlib import Path
import sys
from typing import Any

from le_vibe.extensions import (
    ExtensionHost,
    ExtensionManifest,
    discover_extension_manifests,
    discover_skills,
    load_extension_file,
    load_extensions_from_dirs,
)
from le_vibe.policy import PermissionStore
from le_vibe.providers import (
    Message,
    StreamOptions,
    ToolCall,
    ToolDefinition,
    TranscriptContext,
    auth_status,
    complete,
    default_model_id,
    default_thinking_for_model,
    delete_credential,
    fetch_usage,
    get_model,
    has_auth,
    list_models,
    list_providers,
    login_agy_oauth,
    login_vibe_oidc,
    catalog_age_seconds,
    catalog_is_stale,
    catalog_updated_at,
    refresh_model_catalogs,
    save_api_key,
    stream,
)

THINKING_LEVELS = frozenset({"off", "minimal", "low", "medium", "high", "xhigh", "max"})


def resolve_model_ref(pattern: str, *, default_provider: str = "vibe") -> dict[str, str]:
    text = pattern.strip()
    explicit_thinking: str | None = None
    if ":" in text:
        base, suffix = text.rsplit(":", 1)
        if suffix in THINKING_LEVELS:
            text = base
            explicit_thinking = suffix

    if "/" in text:
        provider, model_id = text.split("/", 1)
        spec = get_model(provider, model_id)  # type: ignore[arg-type]
        return {
            "provider": spec.provider,
            "model": spec.id,
            "thinking_level": explicit_thinking or default_thinking_for_model(spec),
        }

    for candidate in list_models(default_provider):  # type: ignore[arg-type]
        if candidate.id == text:
            return {
                "provider": candidate.provider,
                "model": candidate.id,
                "thinking_level": explicit_thinking or default_thinking_for_model(candidate),
            }
    for candidate in list_models():
        if candidate.id == text:
            return {
                "provider": candidate.provider,
                "model": candidate.id,
                "thinking_level": explicit_thinking or default_thinking_for_model(candidate),
            }
    raise KeyError(f"Model {pattern!r} not found")


def _serialize_model(model: Any) -> dict[str, Any]:
    data = asdict(model)
    data["default_thinking"] = default_thinking_for_model(model)
    return data


def _clean_utf8(s: str | None) -> str | None:
    if s is None:
        return None
    return s.encode("utf-8", errors="replace").decode("utf-8")


def _parse_completion_params(params: dict[str, Any]):
    model = get_model(params["provider"], params["model"])
    messages = [
        Message(
            role=m["role"],
            content=_clean_utf8(m.get("content", "")) or "",
            reasoning_content=_clean_utf8(m.get("reasoning_content")),
            tool_calls=tuple(
                ToolCall(id=tc["id"], name=tc["name"], arguments=tc.get("arguments", {}))
                for tc in m.get("tool_calls", [])
            ),
            tool_call_id=m.get("tool_call_id"),
            name=m.get("name"),
        )
        for m in params.get("messages", [])
    ]
    tools = [
        ToolDefinition(
            name=t["name"],
            description=t.get("description", ""),
            parameters=t.get("parameters", {}),
        )
        for t in params.get("tools", [])
    ]
    ctx = TranscriptContext(
        messages=messages,
        system_prompt=_clean_utf8(params.get("system_prompt")),
        tools=tools,
    )
    opts = StreamOptions(
        api_key=params.get("api_key"),
        temperature=params.get("temperature"),
        max_tokens=params.get("max_tokens"),
        thinking_level=params.get("thinking_level", "medium"),
    )
    return model, ctx, opts


def _default_policy_paths() -> list[Path]:
    return [Path.home() / ".le-vibe" / "policy.json", Path.cwd() / ".le-vibe" / "policy.json"]


def _default_extension_dirs() -> list[Path]:
    return [
        Path.home() / ".le-vibe" / "extension",
        Path.home() / ".agents" / "extension",
        Path.cwd() / ".le-vibe" / "extension",
        Path.cwd() / ".agents" / "extension",
        Path.home() / ".le-vibe" / "extensions",
        Path.cwd() / ".le-vibe" / "extensions",
    ]


def _load_configured_extensions(
    host: ExtensionHost, dirs: Sequence[Path] | None = None
) -> list[str]:
    if os.environ.get("LE_VIBE_NO_EXTENSIONS") == "1":
        return []
    ext_dirs = list(dirs) if dirs is not None else _default_extension_dirs()
    # Only eager extensions (e.g. policy/guard hooks) load at startup; tool-providing
    # extensions stay lazy until activated via `extensions.load` / `/ext load <name>`.
    loaded = load_extensions_from_dirs(host, ext_dirs, eager_only=True)
    extra_raw = os.environ.get("LE_VIBE_EXTRA_EXTENSIONS", "").strip()
    if extra_raw:
        for item in extra_raw.split(os.pathsep):
            p = Path(item.strip())
            if not p.exists():
                continue
            if p.is_dir():
                loaded.extend(load_extensions_from_dirs(host, [p], eager_only=True))
            elif p.is_file() and p.suffix == ".py":
                if path_str := load_extension_file(host, p):
                    loaded.append(path_str)
    return loaded


class BridgeRuntime:
    def __init__(self, *, extension_dirs: Sequence[Path] | None = None) -> None:
        self.policy = PermissionStore.load_from_files(_default_policy_paths())
        self.extensions = ExtensionHost()
        self._extension_dirs = (
            list(extension_dirs) if extension_dirs is not None else _default_extension_dirs()
        )
        self._manifests: list[ExtensionManifest] = (
            []
            if os.environ.get("LE_VIBE_NO_EXTENSIONS") == "1"
            else discover_extension_manifests(self._extension_dirs)
        )
        self.loaded_extensions = _load_configured_extensions(
            self.extensions, self._extension_dirs
        )

    def reload_policy(self) -> None:
        yolo = self.policy.yolo
        self.policy = PermissionStore.load_from_files(_default_policy_paths(), yolo=yolo)

    def reload_extensions(self) -> list[str]:
        self.extensions.clear()
        self._manifests = (
            []
            if os.environ.get("LE_VIBE_NO_EXTENSIONS") == "1"
            else discover_extension_manifests(self._extension_dirs)
        )
        self.loaded_extensions = _load_configured_extensions(
            self.extensions, self._extension_dirs
        )
        return self.loaded_extensions

    def load_extension(self, name: str) -> dict[str, Any]:
        needle = name.strip()
        if not needle:
            raise ValueError("Extension name is required")
        manifest = next((m for m in self._manifests if m.name == needle), None)
        if manifest is None:
            # Re-scan in case an extension was added since startup.
            self._manifests = discover_extension_manifests(self._extension_dirs)
            manifest = next((m for m in self._manifests if m.name == needle), None)
        if manifest is None:
            avail = ", ".join(m.name for m in self._manifests) or "(none)"
            raise KeyError(f"Unknown extension {needle!r}. Available: {avail}")
        path_str = str(manifest.path)
        already = path_str in self.loaded_extensions
        if not already:
            if loaded_path := load_extension_file(self.extensions, manifest.path):
                self.loaded_extensions.append(loaded_path)
        return {
            "loaded": manifest.name,
            "already_loaded": already,
            "tools": [t.name for t in self.extensions.list_tools()],
        }

    async def handle_request(
        self,
        req: dict[str, Any],
        *,
        emit_stream: Callable[[dict[str, Any]], Awaitable[None] | None] | None = None,
    ) -> dict[str, Any]:
        req_id = req.get("id")
        method = req.get("method", "")
        params = req.get("params") or {}

        try:
            if method == "providers.stream":
                model, ctx, opts = _parse_completion_params(params)
                final_msg = None
                async for ev in stream(model, ctx, opts):
                    frame = {"id": req_id, "stream": True, "event": asdict(ev)}
                    if emit_stream is not None:
                        res = emit_stream(frame)
                        if isinstance(res, Awaitable):
                            await res
                    if ev.type == "done" and ev.message is not None:
                        final_msg = ev.message
                if final_msg is None:
                    raise RuntimeError("Stream ended without final message")
                return {"id": req_id, "result": asdict(final_msg)}

            result = await self._dispatch(method, params)
            return {"id": req_id, "result": result}
        except Exception as exc:  # noqa: BLE001
            return {"id": req_id, "error": {"message": str(exc), "type": type(exc).__name__}}

    async def _dispatch(self, method: str, params: dict[str, Any]) -> Any:
        if method == "providers.list":
            return list(list_providers())

        if method == "providers.list_models":
            provider = params.get("provider")
            if params.get("refresh"):
                await refresh_model_catalogs(provider)
            return [_serialize_model(m) for m in list_models(provider)]

        if method == "providers.refresh_models":
            provider = params.get("provider")
            counts = await refresh_model_catalogs(
                provider, force=bool(params.get("force", True))
            )
            return {
                "counts": counts,
                "models": [_serialize_model(m) for m in list_models(provider)],
                "updated_at": catalog_updated_at(),
                "age_seconds": catalog_age_seconds(),
                "stale": catalog_is_stale(),
            }

        if method == "providers.catalog_state":
            counts = {p: len(list_models(p)) for p in list_providers()}
            return {
                "updated_at": catalog_updated_at(),
                "age_seconds": catalog_age_seconds(),
                "stale": catalog_is_stale(),
                "counts": counts,
            }

        if method == "providers.resolve_model":
            return resolve_model_ref(
                params["pattern"],
                default_provider=params.get("default_provider", "vibe"),
            )

        if method == "providers.default_model":
            return {"model": default_model_id(params["provider"])}

        if method == "providers.has_auth":
            return {"authenticated": has_auth(params["provider"])}

        if method == "providers.auth_status":
            return auth_status()

        if method == "providers.save_api_key":
            path = save_api_key(params["provider"], params["api_key"])
            return {"saved": True, "path": str(path)}

        if method == "providers.login_vibe_oidc":
            path = await asyncio.to_thread(login_vibe_oidc)
            return {"saved": True, "path": str(path)}

        if method == "providers.login_agy_oauth":
            path = await asyncio.to_thread(login_agy_oauth)
            return {"saved": True, "path": str(path)}

        if method == "providers.logout":
            return {"removed": delete_credential(params["provider"])}

        if method == "providers.complete":
            model, ctx, opts = _parse_completion_params(params)
            msg = await complete(model, ctx, opts)
            return asdict(msg)

        if method == "providers.usage":
            return await fetch_usage(params.get("provider"))

        if method == "policy.evaluate":
            decision = self.policy.evaluate_tool_call(
                params["tool_name"],
                command=params.get("command"),
                target_path=params.get("target_path"),
            )
            return asdict(decision)

        if method == "policy.set_yolo":
            enabled = bool(params.get("enabled", False))
            self.policy.set_yolo(enabled)
            return {"yolo": self.policy.yolo}

        if method == "policy.grant_rule":
            self.policy.grant_session_rule(
                params["tool_name"],
                command=params.get("command"),
                target_path=params.get("target_path"),
            )
            return {"granted": True}

        if method == "extensions.invoke_hook":
            return await self.extensions.invoke_hook(params["event"], params.get("payload", {}))

        if method == "extensions.list_tools":
            return [
                {"name": t.name, "description": t.description, "parameters": t.parameters}
                for t in self.extensions.list_tools()
            ]

        if method == "extensions.execute_tool":
            return await self.extensions.execute_tool(params["name"], params.get("arguments", {}))

        if method == "extensions.list_skills":
            roots = [Path(r) for r in params.get("roots", [])]
            return [
                {"name": s.name, "description": s.description, "path": str(s.path)}
                for s in discover_skills(roots)
            ]

        if method == "extensions.load":
            return self.load_extension(str(params.get("name", "")))

        if method == "extensions.reload":
            loaded = self.reload_extensions()
            return {
                "loaded": loaded,
                "errors": list(self.extensions.load_errors),
                "commands": [
                    {"name": c.name, "description": c.description}
                    for c in self.extensions.list_commands()
                ],
            }

        if method == "extensions.status":
            return {
                "loaded": list(self.loaded_extensions),
                "available": [
                    {
                        "name": m.name,
                        "lifecycle": m.lifecycle,
                        "description": m.description,
                        "loaded": str(m.path) in self.loaded_extensions,
                    }
                    for m in self._manifests
                ],
                "tools": [t.name for t in self.extensions.list_tools()],
                "hooks": self.extensions.hook_events(),
                "commands": [
                    {"name": c.name, "description": c.description}
                    for c in self.extensions.list_commands()
                ],
                "errors": list(self.extensions.load_errors),
            }

        if method == "extensions.list_commands":
            return [
                {"name": c.name, "description": c.description}
                for c in self.extensions.list_commands()
            ]

        if method == "extensions.run_command":
            out = await self.extensions.run_command(params["name"], str(params.get("args", "")))
            return {"output": out}

        if method == "policy.reload":
            self.reload_policy()
            return {"reloaded": True}

        if method == "policy.status":
            return self.policy.summary()

        if method == "web.search":
            from le_vibe.providers.web import web_search

            raw_queries = params.get("queries")
            if isinstance(raw_queries, list) and raw_queries:
                queries = [str(q) for q in raw_queries]
            else:
                q = str(params.get("query") or "").strip()
                queries = [q] if q else []
            num = int(params.get("num_results") or params.get("numResults") or 5)
            return {"output": await web_search(queries, num_results=num)}

        if method == "web.fetch":
            from le_vibe.providers.web import fetch_content

            url = str(params.get("url") or "")
            mode = str(params.get("mode") or "readable")
            return {"output": await fetch_content(url, mode=mode)}

        raise ValueError(f"Unknown bridge method: {method!r}")


async def serve_stdio() -> None:
    if hasattr(sys.stdin, "reconfigure"):
        sys.stdin.reconfigure(encoding="utf-8", errors="replace")
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    runtime = BridgeRuntime()

    def _emit(frame: dict[str, Any]) -> None:
        sys.stdout.write(json.dumps(frame, ensure_ascii=False) + "\n")
        sys.stdout.flush()

    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        req = json.loads(line)
        resp = await runtime.handle_request(req, emit_stream=_emit)
        _emit(resp)


if __name__ == "__main__":
    try:
        asyncio.run(serve_stdio())
    except (KeyboardInterrupt, BrokenPipeError, EOFError):
        sys.exit(0)
