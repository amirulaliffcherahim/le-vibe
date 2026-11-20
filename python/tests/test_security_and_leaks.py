from __future__ import annotations

import json
from pathlib import Path
import pytest

from le_vibe.bridge import BridgeRuntime
from le_vibe.policy import PermissionStore
from le_vibe.providers import (
    auth_status,
    list_models,
    save_api_key,
    save_oidc_tokens,
)


def test_auth_status_and_catalog_never_leak_secrets(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    auth_file = tmp_path / "auth.json"
    monkeypatch.setenv("LE_VIBE_AUTH_FILE", str(auth_file))

    secret_ds = "sk-deepseek-super-secret-token-999"
    secret_vibe = "mistral-super-secret-key-888"
    secret_agy_access = "ya29.super-secret-agy-access-777"
    secret_agy_refresh = "1//super-secret-agy-refresh-666"

    save_api_key("deepseek", secret_ds, auth_file=auth_file)
    save_api_key("vibe", secret_vibe, auth_file=auth_file)
    save_oidc_tokens(
        "agy",
        access_token=secret_agy_access,
        refresh_token=secret_agy_refresh,
        expires_at=9_999_999_999.0,
        auth_file=auth_file,
    )

    status_json = json.dumps(auth_status(auth_file=auth_file))
    models_json = json.dumps([m.id for m in list_models()])

    for secret in (secret_ds, secret_vibe, secret_agy_access, secret_agy_refresh):
        assert secret not in status_json
        assert secret not in models_json


@pytest.mark.asyncio
async def test_bridge_errors_and_responses_do_not_leak_tokens(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    auth_file = tmp_path / "auth.json"
    monkeypatch.setenv("LE_VIBE_AUTH_FILE", str(auth_file))
    secret = "sk-leak-check-1234567890"
    save_api_key("deepseek", secret, auth_file=auth_file)

    bridge = BridgeRuntime()
    resp_auth = await bridge.handle_request({"id": 1, "method": "providers.auth_status"})
    assert secret not in json.dumps(resp_auth)

    resp_err = await bridge.handle_request(
        {
            "id": 2,
            "method": "providers.complete",
            "params": {"provider": "deepseek", "model": "nonexistent-model", "messages": []},
        }
    )
    assert "error" in resp_err
    assert secret not in json.dumps(resp_err)


def test_policy_yolo_still_blocks_dangerous_commands_and_isolates_grants(tmp_path: Path) -> None:
    store = PermissionStore(trusted_folders=(tmp_path,), yolo=True)

    # Dangerous commands must stay blocked even in YOLO mode.
    for cmd in ("rm -rf /", "rm -rf /*", "mkfs.ext4 /dev/sda", "dd if=/dev/zero of=/dev/sda"):
        decision = store.evaluate_tool_call("bash", command=cmd)
        assert decision.action == "deny", f"Expected deny for {cmd}"

    # Granting one file does not leak to another file or another tool.
    store.set_yolo(False)
    f1 = str(tmp_path.parent / "outside_a.txt")
    f2 = str(tmp_path.parent / "outside_b.txt")
    store.grant_session_rule("write", target_path=f1)
    assert store.evaluate_tool_call("write", target_path=f1).action == "allow"
    assert store.evaluate_tool_call("write", target_path=f2).action == "ask"
    assert store.evaluate_tool_call("edit", target_path=f1).action == "ask"
