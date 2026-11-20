from __future__ import annotations

from pathlib import Path
import pytest

from le_vibe.bridge import BridgeRuntime
from le_vibe.extensions import ExtensionHost, PythonToolSpec, discover_skills
from le_vibe.policy import ApprovedRule, PermissionScope, PermissionStore, ToolPermission


def test_policy_blocks_dangerous_shell_and_enforces_trusted_folders(tmp_path: Path) -> None:
    store = PermissionStore(trusted_folders=(tmp_path,))

    denied = store.evaluate_tool_call("bash", command="rm -rf /")
    assert denied.action == "deny"

    outside = store.evaluate_tool_call("read", target_path=str(tmp_path.parent / "secret.txt"))
    assert outside.action == "ask"

    inside = store.evaluate_tool_call("read", target_path=str(tmp_path / "main.rs"))
    assert inside.action == "allow"

    store.set_tool_permission("bash", ToolPermission.ASK)
    assert store.evaluate_tool_call("bash", command="git status").action == "ask"

    store.add_rule(ApprovedRule(tool_name="bash", scope=PermissionScope.COMMAND, session_pattern="git *"))
    assert store.evaluate_tool_call("bash", command="git status").action == "allow"


@pytest.mark.asyncio
async def test_extensions_hooks_and_skills(tmp_path: Path) -> None:
    skill_dir = tmp_path / "my-skill"
    skill_dir.mkdir()
    (skill_dir / "SKILL.md").write_text(
        "---\nname: my-skill\ndescription: Test skill\n---\nAlways run tests.\n",
        encoding="utf-8",
    )
    skills = discover_skills([tmp_path])
    assert len(skills) == 1
    assert skills[0].name == "my-skill"
    assert "Always run tests." in skills[0].body

    host = ExtensionHost()
    host.on("before_turn", lambda p: {**p, "injected": True})
    host.register_tool(
        PythonToolSpec(
            name="echo",
            description="Echo text",
            parameters={"type": "object"},
            handler=lambda args: {"echo": args["text"]},
        )
    )
    out = await host.invoke_hook("before_turn", {"turn": 1})
    assert out == {"turn": 1, "injected": True}
    assert await host.execute_tool("echo", {"text": "hi"}) == {"echo": "hi"}


@pytest.mark.asyncio
async def test_extensions_async_hooks_and_plain_skill_fallback(tmp_path: Path) -> None:
    plain_dir = tmp_path / "plain-skill"
    plain_dir.mkdir()
    (plain_dir / "SKILL.md").write_text("Plain body without frontmatter.\n", encoding="utf-8")
    skills = discover_skills([tmp_path, tmp_path / "missing-dir"])
    assert len(skills) == 1
    assert skills[0].name == "plain-skill"
    assert skills[0].description == ""

    host = ExtensionHost()

    async def async_hook(payload: dict) -> dict:
        return {**payload, "step2": True}

    host.on("after_turn", lambda p: {**p, "step1": True})
    host.on("after_turn", async_hook)
    chained = await host.invoke_hook("after_turn", {})
    assert chained == {"step1": True, "step2": True}

    with pytest.raises(KeyError, match="Unknown Python extension tool"):
        await host.execute_tool("missing_tool", {})


@pytest.mark.asyncio
async def test_bridge_runtime_dispatch() -> None:
    bridge = BridgeRuntime()
    resp = await bridge.handle_request({"id": 1, "method": "providers.list"})
    assert resp["result"] == ["vibe", "deepseek", "agy"]

    policy_resp = await bridge.handle_request(
        {"id": 2, "method": "policy.evaluate", "params": {"tool_name": "bash", "command": "rm -rf /"}}
    )
    assert policy_resp["result"]["action"] == "deny"

    # resolve_model reads the live catalog, so seed one entry for the test.
    from le_vibe.providers import catalog
    from le_vibe.providers.types import ModelSpec

    catalog._DYNAMIC_MODELS["deepseek"] = (
        ModelSpec(
            id="deepseek-v4-pro",
            name="DeepSeek V4 Pro",
            provider="deepseek",
            base_url="https://api.deepseek.com",
        ),
    )
    resolved = await bridge.handle_request(
        {
            "id": 3,
            "method": "providers.resolve_model",
            "params": {"pattern": "deepseek-v4-pro:high", "default_provider": "deepseek"},
        }
    )
    assert resolved["result"] == {
        "provider": "deepseek",
        "model": "deepseek-v4-pro",
        "thinking_level": "high",
    }


@pytest.mark.asyncio
async def test_policy_yolo_and_session_grants() -> None:
    bridge = BridgeRuntime()
    ask = await bridge.handle_request(
        {"id": 10, "method": "policy.evaluate", "params": {"tool_name": "write", "target_path": "a.txt"}}
    )
    assert ask["result"]["action"] == "ask"

    await bridge.handle_request(
        {"id": 11, "method": "policy.grant_rule", "params": {"tool_name": "write", "target_path": "a.txt"}}
    )
    allowed = await bridge.handle_request(
        {"id": 12, "method": "policy.evaluate", "params": {"tool_name": "write", "target_path": "a.txt"}}
    )
    assert allowed["result"]["action"] == "allow"

    await bridge.handle_request({"id": 13, "method": "policy.set_yolo", "params": {"enabled": True}})
    yolo_bash = await bridge.handle_request(
        {"id": 14, "method": "policy.evaluate", "params": {"tool_name": "bash", "command": "ls"}}
    )
    assert yolo_bash["result"]["action"] == "allow"
    blocked = await bridge.handle_request(
        {"id": 15, "method": "policy.evaluate", "params": {"tool_name": "bash", "command": "rm -rf /"}}
    )
    assert blocked["result"]["action"] == "deny"
