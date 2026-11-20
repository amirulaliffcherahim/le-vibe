from __future__ import annotations

import asyncio
from pathlib import Path

from le_vibe.extensions import ExtensionHost, load_extensions_from_dirs
from le_vibe.policy import PermissionStore, ToolPermission

REPO_ROOT = Path(__file__).resolve().parents[2]


def test_example_policies_load_and_enforce_rules() -> None:
    strict = PermissionStore.load_from_files([REPO_ROOT / "examples" / "policies" / "strict.json"])
    assert strict.get_tool_permission("bash") is ToolPermission.NEVER
    assert strict.evaluate_tool_call("bash", command="echo hi").action == "deny"
    assert strict.evaluate_tool_call("read", target_path="README.md").action == "allow"

    ci = PermissionStore.load_from_files([REPO_ROOT / "examples" / "policies" / "ci_readonly.json"])
    assert ci.evaluate_tool_call("write", target_path="x.txt").action == "deny"
    assert ci.evaluate_tool_call("read", target_path="x.txt").action == "allow"

    balanced = PermissionStore.load_from_files([REPO_ROOT / "examples" / "policies" / "balanced.json"])
    assert balanced.evaluate_tool_call("bash", command="git status").action == "allow"
    assert balanced.evaluate_tool_call("bash", command="cargo test --workspace").action == "allow"
    assert balanced.evaluate_tool_call("bash", command="git push --force").action == "deny"
    # Hardcoded dangerous pattern cannot be bypassed even with yolo
    balanced.set_yolo(True)
    assert balanced.evaluate_tool_call("bash", command="rm -rf /").action == "deny"


def test_example_extensions_load_and_execute() -> None:
    host = ExtensionHost()
    loaded = load_extensions_from_dirs(host, [REPO_ROOT / "examples" / "extensions"])
    assert len(loaded) == 3
    assert not host.load_errors

    tool_names = {t.name for t in host.list_tools()}
    assert "project_stats" in tool_names

    # secret_guard blocks .env
    res_env = asyncio.run(
        host.invoke_hook("before_tool_call", {"tool": "read", "arguments": {"path": ".env"}})
    )
    assert res_env.get("blocked") is True

    # secret_guard allows normal files
    res_ok = asyncio.run(
        host.invoke_hook("before_tool_call", {"tool": "read", "arguments": {"path": "README.md"}})
    )
    assert not res_ok.get("blocked")

    # project_stats counts files in examples/
    stats = asyncio.run(host.execute_tool("project_stats", {"path": str(REPO_ROOT / "examples")}))
    assert stats["total_files"] >= 7
    assert stats["total_lines"] > 0
