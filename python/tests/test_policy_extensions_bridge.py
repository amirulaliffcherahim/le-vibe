from __future__ import annotations

from pathlib import Path
import pytest

from lazybug.bridge import BridgeRuntime
from lazybug.extensions import ExtensionHost, PythonToolSpec, discover_skills
from lazybug.policy import ApprovedRule, PermissionScope, PermissionStore, ToolPermission


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
    from lazybug.providers import catalog
    from lazybug.providers.types import ModelSpec

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


@pytest.mark.asyncio
async def test_lazy_extensions_skip_startup_and_load_on_demand(tmp_path: Path) -> None:
    ext_dir = tmp_path / "extensions"
    ext_dir.mkdir()
    marker = tmp_path / "lazy_loaded.txt"
    (ext_dir / "lazy_ext.py").write_text(
        f"""
from pathlib import Path
LIFECYCLE = "lazy"
DESCRIPTION = "Lazy test tool"
Path({str(marker)!r}).write_text("imported", encoding="utf-8")

def register(host):
    from lazybug.extensions import PythonToolSpec
    host.register_tool(
        PythonToolSpec(
            name="lazy_ping",
            description="Ping",
            parameters={{"type": "object"}},
            handler=lambda _a: "pong",
        )
    )
""",
        encoding="utf-8",
    )

    bridge = BridgeRuntime(extension_dirs=[ext_dir])
    listed = await bridge.handle_request({"id": 20, "method": "extensions.list_tools"})
    assert listed["result"] == []
    assert not marker.exists(), "lazy extension must not be imported at startup"

    loaded = await bridge.handle_request(
        {"id": 21, "method": "extensions.load", "params": {"name": "lazy_ext"}}
    )
    assert loaded["result"]["loaded"] == "lazy_ext"
    assert marker.exists()

    listed_after = await bridge.handle_request({"id": 22, "method": "extensions.list_tools"})
    assert [t["name"] for t in listed_after["result"]] == ["lazy_ping"]

    # Bundled first-party lazy extensions (`web`, `planning`) are discoverable & loadable on demand.
    web_loaded = await bridge.handle_request(
        {"id": 23, "method": "extensions.load", "params": {"name": "web"}}
    )
    assert web_loaded["result"]["loaded"] == "web"
    listed_web = await bridge.handle_request({"id": 24, "method": "extensions.list_tools"})
    names_web = [t["name"] for t in listed_web["result"]]
    assert names_web == ["lazy_ping", "web_search", "fetch_content"]


def test_discover_skills_supports_flat_and_nested_markdown_layouts(tmp_path: Path) -> None:
    root = tmp_path / "skill"
    root.mkdir()
    (root / "quick.md").write_text("---\ndescription: Flat skill\n---\nBody1.", encoding="utf-8")
    nested = root / "complex-skill"
    nested.mkdir()
    (nested / "multi-skill.md").write_text(
        "---\ndescription: Nested skill\n---\nBody2.", encoding="utf-8"
    )
    (root / "README.md").write_text("Ignored readme", encoding="utf-8")

    skills = discover_skills([root])
    names = [s.name for s in skills]
    assert names == ["complex-skill" if False else "multi-skill", "quick"] or set(names) == {
        "multi-skill",
        "quick",
    }

