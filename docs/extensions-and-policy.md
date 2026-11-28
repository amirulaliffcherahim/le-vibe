# Python Extensions & Policies

## Policies (`python/lazybug/policy/__init__.py`)

`PermissionStore` evaluates every tool call in order:
1. Hardcoded `_DANGEROUS_SHELL_PATTERNS` (`rm -rf /`, `mkfs`, `dd if=`, etc.) — **always denied**, even in `yolo` mode.
2. Policy `blocked_commands` — **always denied**.
3. `yolo` mode — auto-allows remaining calls.
4. `trusted_folders` — paths outside trusted roots prompt `ask` unless matched by `allowed_paths` or session grants.
5. `tool_permissions` (`always` / `ask` / `never`) and `allowed_commands` wildcard rules.

Files loaded automatically on startup (and `/reload`):
- `~/.lazybug/policy.json` (global)
- `<cwd>/.lazybug/policy.json` (project override)

See `examples/policies/` (`balanced.json`, `strict.json`, `ci_readonly.json`).

## Extensions (`python/lazybug/extensions/__init__.py`)

`load_extensions_from_dirs` scans `~/.lazybug/extensions/*.py` and `<cwd>/.lazybug/extensions/*.py` (skipping `_*.py`) and calls `register(host: ExtensionHost)`.

- **Hooks**: `before_tool_call` (can return `{"blocked": True, "reason": "..."}`) and `after_tool_call`.
- **Custom tools**: `host.register_tool(PythonToolSpec(...))` — merged into `CompletionRequest.tools` and routed through `PolicyPort` before execution.

See `examples/extensions/` (`audit_logger.py`, `project_stats_tool.py`, `secret_guard.py`) and `examples/example.md`.
