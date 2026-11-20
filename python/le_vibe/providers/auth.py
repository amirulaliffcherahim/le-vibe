from __future__ import annotations

import base64
from collections.abc import Mapping
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import secrets
import sys
import time
from urllib.parse import parse_qs, urlencode, urlsplit
import webbrowser

import httpx

from le_vibe.providers.types import AuthCredential, ProviderId

PROVIDER_ENV_KEYS: dict[ProviderId, str] = {
    "deepseek": "DEEPSEEK_API_KEY",
    "vibe": "MISTRAL_API_KEY",
    "agy": "AGY_API_KEY",
}

MISTRAL_OIDC_ISSUER = "https://auth.mistral.ai"
MISTRAL_OIDC_CLIENT_ID = "829da7e7-628f-4795-829a-8cca609c76d4"
MISTRAL_OIDC_AUDIENCE = "https://api.mistral.ai"
MISTRAL_OIDC_REDIRECT_URI = "http://127.0.0.1:8766/callback"
MISTRAL_OIDC_SCOPE = "openid offline_access email profile"
MISTRAL_CONSOLE_API_BASE = "https://console.mistral.ai/api"

AGY_OAUTH_AUTH_URL = "https://accounts.google.com/o/oauth2/v2/auth"
AGY_OAUTH_TOKEN_URL = "https://oauth2.googleapis.com/token"
AGY_OAUTH_REDIRECT_URI = "http://localhost:51121/oauth-callback"
AGY_OAUTH_SCOPES = (
    "https://www.googleapis.com/auth/aicode "
    "https://www.googleapis.com/auth/cloud-platform "
    "https://www.googleapis.com/auth/userinfo.email "
    "https://www.googleapis.com/auth/userinfo.profile "
    "https://www.googleapis.com/auth/cclog "
    "https://www.googleapis.com/auth/experimentsandconfigs"
)
AGY_USER_AGENT = (
    "antigravity/cli/1.4.2 (aidev_client; os_type=linux; arch=amd64; cl=985595000; auth_method=consumer)"
)
AGY_ENDPOINTS: tuple[str, ...] = (
    "https://daily-cloudcode-pa.googleapis.com",
    "https://daily-cloudcode-pa.sandbox.googleapis.com",
    "https://cloudcode-pa.googleapis.com",
)
AGY_DEFAULT_CLIENT_ID = os.environ.get("AGY_OAUTH_CLIENT_ID", "")
AGY_DEFAULT_CLIENT_SECRET = os.environ.get("AGY_OAUTH_CLIENT_SECRET", "")


def _load_agy_oauth_client() -> tuple[str, str]:
    if AGY_DEFAULT_CLIENT_ID and AGY_DEFAULT_CLIENT_SECRET:
        return AGY_DEFAULT_CLIENT_ID, AGY_DEFAULT_CLIENT_SECRET
    cfg_path = default_auth_file().parent / "agy_oauth.json"
    if cfg_path.exists():
        try:
            data = json.loads(cfg_path.read_text(encoding="utf-8-sig"))
            cid = str(data.get("client_id", "")).strip()
            csec = str(data.get("client_secret", "")).strip()
            if cid and csec:
                return cid, csec
        except (OSError, json.JSONDecodeError):
            pass
    raise RuntimeError(
        "Missing AGY_OAUTH_CLIENT_ID / AGY_OAUTH_CLIENT_SECRET (set env vars or ~/.le-vibe/agy_oauth.json)"
    )


class MissingAPIKeyError(RuntimeError):
    def __init__(self, env_key: str, provider: ProviderId) -> None:
        super().__init__(
            f"Missing {env_key} environment variable or saved credential for {provider} provider. "
            f"Run: le-vibe login --provider {provider}"
        )
        self.env_key = env_key
        self.provider = provider


def default_auth_file() -> Path:
    if override := os.environ.get("LE_VIBE_AUTH_FILE"):
        return Path(override)
    return Path.home() / ".le-vibe" / "auth.json"


def _read_auth_store(auth_file: Path | None = None) -> dict[str, dict[str, str | float]]:
    path = auth_file or default_auth_file()
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
        return data if isinstance(data, dict) else {}
    except (OSError, json.JSONDecodeError):
        return {}


def _write_auth_store(data: dict[str, dict[str, str | float]], auth_file: Path | None = None) -> Path:
    path = auth_file or default_auth_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    try:
        os.chmod(path, 0o600)
    except OSError:
        pass
    return path


def save_api_key(provider: ProviderId, api_key: str, *, auth_file: Path | None = None) -> Path:
    if provider not in PROVIDER_ENV_KEYS:
        raise ValueError(
            f"Unsupported provider: {provider!r}. Only 'vibe', 'deepseek', and 'agy' are allowed."
        )
    cleaned = api_key.strip()
    if not cleaned:
        raise ValueError("API key cannot be empty")
    store = _read_auth_store(auth_file)
    store[provider] = {"type": "api_key", "key": cleaned}
    return _write_auth_store(store, auth_file)


def save_oidc_tokens(
    provider: ProviderId,
    *,
    access_token: str,
    refresh_token: str | None = None,
    id_token: str | None = None,
    expires_at: float | None = None,
    token_endpoint: str | None = None,
    client_id: str | None = None,
    project_id: str | None = None,
    organization_id: str | None = None,
    workspace_id: str | None = None,
    auth_file: Path | None = None,
) -> Path:
    store = _read_auth_store(auth_file)
    entry: dict[str, str | float] = {"type": "oidc", "access_token": access_token}
    if refresh_token:
        entry["refresh_token"] = refresh_token
    if id_token:
        entry["id_token"] = id_token
    if expires_at is not None:
        entry["expires_at"] = expires_at
    if token_endpoint:
        entry["token_endpoint"] = token_endpoint
    if client_id:
        entry["client_id"] = client_id
    if project_id:
        entry["project_id"] = project_id
    if organization_id:
        entry["organization_id"] = organization_id
    if workspace_id:
        entry["workspace_id"] = workspace_id
    store[provider] = entry
    return _write_auth_store(store, auth_file)


def delete_credential(provider: ProviderId, *, auth_file: Path | None = None) -> bool:
    store = _read_auth_store(auth_file)
    if provider not in store:
        return False
    del store[provider]
    _write_auth_store(store, auth_file)
    return True


def _pkce_pair() -> tuple[str, str]:
    verifier = base64.urlsafe_b64encode(secrets.token_bytes(32)).rstrip(b"=").decode("ascii")
    digest = hashlib.sha256(verifier.encode("ascii")).digest()
    challenge = base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii")
    return verifier, challenge


def _discover_vibe_tenant(id_token: str) -> tuple[str, str]:
    """Discover (organization_id, workspace_id) from Mistral /v1/users/me/workspaces."""
    with httpx.Client(timeout=15.0) as client:
        resp = client.get(
            f"{MISTRAL_OIDC_AUDIENCE}/v1/users/me/workspaces",
            params={"limit": 100, "offset": 0},
            headers={"Authorization": f"Bearer {id_token}"},
        )
        if not resp.is_success:
            return "", ""
        workspaces = resp.json().get("workspaces") or []
        if workspaces and isinstance(workspaces[0], dict):
            org_id = str(workspaces[0].get("organization_id") or "")
            ws_id = str(workspaces[0].get("id") or "")
            return org_id, ws_id
    return "", ""


def _try_vibe_browser_api_key_sign_in(
    *,
    auth_file: Path | None = None,
    timeout_s: float = 180.0,
) -> Path | None:
    """Try Mistral Console /api/vibe/sign-in flow that exchanges PKCE for a real MISTRAL_API_KEY."""
    verifier, challenge = _pkce_pair()
    try:
        with httpx.Client(timeout=20.0) as client:
            start_resp = client.post(
                f"{MISTRAL_CONSOLE_API_BASE}/vibe/sign-in",
                json={"code_challenge": challenge, "code_challenge_method": "S256"},
            )
            if not start_resp.is_success:
                return None
            data = start_resp.json()
            process_id = str(data["process_id"])
            sign_in_url = str(data["sign_in_url"])
            poll_url = str(data["poll_url"])

            print(f"Opening browser for Mistral Vibe sign-in:\n{sign_in_url}", file=sys.stderr, flush=True)
            webbrowser.open(sign_in_url)

            deadline = time.time() + timeout_s
            exchange_token = ""
            while time.time() < deadline:
                time.sleep(3.0)
                poll_resp = client.get(poll_url)
                if not poll_resp.is_success:
                    continue
                p_data = poll_resp.json()
                status = p_data.get("status")
                if status == "completed" and p_data.get("exchange_token"):
                    exchange_token = str(p_data["exchange_token"])
                    break
                if status in {"expired", "denied", "error"}:
                    return None

            if not exchange_token:
                return None

            ex_resp = client.post(
                f"{MISTRAL_CONSOLE_API_BASE}/vibe/sign-in/{process_id}/exchange",
                json={"exchange_token": exchange_token, "code_verifier": verifier},
            )
            if not ex_resp.is_success:
                return None
            api_key = str(ex_resp.json().get("api_key") or "").strip()
            if api_key:
                return save_api_key("vibe", api_key, auth_file=auth_file)
    except Exception:
        return None
    return None


def login_vibe_oidc(
    *,
    auth_file: Path | None = None,
    timeout_s: float = 180.0,
) -> Path:
    """Sign in to Mistral Vibe: try Console API key exchange first, then OIDC PKCE with id_token + workspace discovery."""
    if path := _try_vibe_browser_api_key_sign_in(auth_file=auth_file, timeout_s=timeout_s):
        return path

    with httpx.Client(timeout=20.0) as client:
        disc_resp = client.get(f"{MISTRAL_OIDC_ISSUER}/.well-known/openid-configuration")
        disc_resp.raise_for_status()
        disc = disc_resp.json()
        auth_endpoint = disc["authorization_endpoint"]
        token_endpoint = disc["token_endpoint"]

    verifier, challenge = _pkce_pair()
    state = secrets.token_urlsafe(24)
    nonce = secrets.token_urlsafe(24)
    params = urlencode(
        {
            "response_type": "code",
            "client_id": MISTRAL_OIDC_CLIENT_ID,
            "redirect_uri": MISTRAL_OIDC_REDIRECT_URI,
            "scope": MISTRAL_OIDC_SCOPE,
            "audience": MISTRAL_OIDC_AUDIENCE,
            "state": state,
            "nonce": nonce,
            "code_challenge": challenge,
            "code_challenge_method": "S256",
        }
    )
    auth_url = f"{auth_endpoint}?{params}"

    parsed_redirect = urlsplit(MISTRAL_OIDC_REDIRECT_URI)
    host = parsed_redirect.hostname or "127.0.0.1"
    port = parsed_redirect.port or 8766
    callback_path = parsed_redirect.path or "/callback"

    captured: dict[str, str] = {}

    class _CallbackHandler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            req_url = urlsplit(self.path)
            if req_url.path != callback_path:
                self.send_response(404)
                self.end_headers()
                return
            qs = parse_qs(req_url.query)
            if err := qs.get("error"):
                captured["error"] = err[0]
            if code := qs.get("code"):
                captured["code"] = code[0]
            if got_state := qs.get("state"):
                captured["state"] = got_state[0]

            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.end_headers()
            body = (
                "<html><body><h2>le-vibe: Mistral login complete.</h2>"
                "<p>You can close this window and return to your terminal.</p></body></html>"
            )
            self.wfile.write(body.encode("utf-8"))

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            return

    server = HTTPServer((host, port), _CallbackHandler)
    server.timeout = 1.0
    print(f"Opening browser for Mistral login:\n{auth_url}", file=sys.stderr, flush=True)
    webbrowser.open(auth_url)

    deadline = time.time() + timeout_s
    try:
        while time.time() < deadline and "code" not in captured and "error" not in captured:
            server.handle_request()
    finally:
        server.server_close()

    if "error" in captured:
        raise RuntimeError(f"Mistral login failed: {captured['error']}")
    if "code" not in captured:
        raise TimeoutError("Timed out waiting for Mistral browser login callback")
    if captured.get("state") != state:
        raise RuntimeError("Mistral login state mismatch")

    with httpx.Client(timeout=20.0) as client:
        tok_resp = client.post(
            token_endpoint,
            data={
                "grant_type": "authorization_code",
                "client_id": MISTRAL_OIDC_CLIENT_ID,
                "code": captured["code"],
                "redirect_uri": MISTRAL_OIDC_REDIRECT_URI,
                "code_verifier": verifier,
            },
        )
        tok_resp.raise_for_status()
        tok_data = tok_resp.json()

    access_token = str(tok_data["access_token"])
    id_token = str(tok_data.get("id_token") or "")
    refresh_token = tok_data.get("refresh_token")
    expires_in = float(tok_data.get("expires_in", 3600))
    org_id, ws_id = _discover_vibe_tenant(id_token) if id_token else ("", "")

    return save_oidc_tokens(
        "vibe",
        access_token=access_token,
        refresh_token=str(refresh_token) if refresh_token else None,
        id_token=id_token or None,
        expires_at=time.time() + expires_in,
        token_endpoint=token_endpoint,
        client_id=MISTRAL_OIDC_CLIENT_ID,
        organization_id=org_id or None,
        workspace_id=ws_id or None,
        auth_file=auth_file,
    )


def discover_agy_project_id(access_token: str) -> str:
    """Resolve cloudaicompanionProject from Antigravity loadCodeAssist."""
    headers = {
        "Authorization": f"Bearer {access_token}",
        "Content-Type": "application/json",
        "User-Agent": AGY_USER_AGENT,
    }
    for endpoint in AGY_ENDPOINTS:
        try:
            with httpx.Client(timeout=12.0) as client:
                resp = client.post(
                    f"{endpoint}/v1internal:loadCodeAssist",
                    headers=headers,
                    json={"metadata": {"ideType": "ANTIGRAVITY"}},
                )
                if not resp.is_success:
                    continue
                data = resp.json()
                for key in (
                    "antigravityProjectId",
                    "projectId",
                    "backendProjectId",
                    "userDefinedCloudaicompanionProject",
                    "cloudaicompanionProject",
                    "project",
                ):
                    raw = data.get(key)
                    if isinstance(raw, str) and raw.strip():
                        return raw.strip()
                    if isinstance(raw, dict) and isinstance(raw.get("id"), str) and raw["id"].strip():
                        return raw["id"].strip()
        except Exception:
            continue
    return ""


def _resolve_oidc_credential(
    provider: ProviderId,
    saved: Mapping[str, str | float],
    *,
    auth_file: Path | None = None,
) -> AuthCredential | None:
    access_token = saved.get("access_token")
    if not isinstance(access_token, str) or not access_token.strip():
        return None
    expires_at = saved.get("expires_at")
    refresh_token = saved.get("refresh_token")
    token_endpoint = saved.get("token_endpoint")
    id_token = str(saved.get("id_token") or "")
    org_id = str(saved.get("organization_id") or "")
    ws_id = str(saved.get("workspace_id") or "")
    project_id = str(saved.get("project_id") or "")

    client_id = saved.get("client_id")
    effective_client_id = (
        str(client_id)
        if isinstance(client_id, str) and client_id
        else (AGY_DEFAULT_CLIENT_ID if provider == "agy" else MISTRAL_OIDC_CLIENT_ID)
    )
    needs_refresh = (
        isinstance(expires_at, (int, float))
        and time.time() >= float(expires_at) - 60.0
        and isinstance(refresh_token, str)
        and isinstance(token_endpoint, str)
    ) or (provider == "vibe" and not id_token and isinstance(refresh_token, str) and isinstance(token_endpoint, str))

    if needs_refresh:
        refresh_data: dict[str, str] = {
            "grant_type": "refresh_token",
            "client_id": effective_client_id,
            "refresh_token": str(refresh_token),
        }
        if provider == "agy":
            try:
                cid, csec = _load_agy_oauth_client()
                refresh_data["client_id"] = cid
                refresh_data["client_secret"] = csec
            except RuntimeError:
                pass
        try:
            with httpx.Client(timeout=20.0) as client:
                resp = client.post(str(token_endpoint), data=refresh_data)
                if resp.is_success:
                    data = resp.json()
                    access_token = str(data.get("access_token") or access_token)
                    refresh_token = str(data.get("refresh_token") or refresh_token)
                    id_token = str(data.get("id_token") or id_token)
                    new_exp = time.time() + float(data.get("expires_in", 3600))
                    if provider == "vibe" and id_token and not org_id:
                        org_id, ws_id = _discover_vibe_tenant(id_token)
                    if provider == "agy" and not project_id:
                        project_id = discover_agy_project_id(access_token)
                    save_oidc_tokens(
                        provider,
                        access_token=access_token,
                        refresh_token=str(refresh_token),
                        id_token=id_token or None,
                        expires_at=new_exp,
                        token_endpoint=str(token_endpoint),
                        client_id=refresh_data["client_id"],
                        project_id=project_id or None,
                        organization_id=org_id or None,
                        workspace_id=ws_id or None,
                        auth_file=auth_file,
                    )
        except Exception:
            pass

    if provider == "vibe":
        bearer = id_token or access_token.strip()
        if id_token and not org_id:
            org_id, ws_id = _discover_vibe_tenant(id_token)
        extra: dict[str, str] = {}
        if org_id:
            extra["x-external-organization-id"] = org_id
        if ws_id:
            extra["x-external-workspace-id"] = ws_id
        return AuthCredential(provider="vibe", token=bearer, source="oidc", extra_headers=extra)

    if provider == "agy" and not project_id:
        project_id = discover_agy_project_id(access_token.strip())
        if project_id:
            save_oidc_tokens(
                "agy",
                access_token=access_token.strip(),
                refresh_token=str(refresh_token) if isinstance(refresh_token, str) else None,
                expires_at=float(expires_at) if isinstance(expires_at, (int, float)) else None,
                token_endpoint=str(token_endpoint) if isinstance(token_endpoint, str) else None,
                client_id=effective_client_id,
                project_id=project_id,
                auth_file=auth_file,
            )

    return AuthCredential(
        provider=provider,
        token=access_token.strip(),
        source="oidc",
        project_id=project_id or None,
    )


def login_agy_oauth(
    *,
    auth_file: Path | None = None,
    timeout_s: float = 180.0,
) -> Path:
    """Run Google Antigravity OAuth 2.0 PKCE browser login and persist tokens."""
    client_id, client_secret = _load_agy_oauth_client()
    verifier, challenge = _pkce_pair()
    state = secrets.token_urlsafe(24)
    params = urlencode(
        {
            "response_type": "code",
            "client_id": client_id,
            "redirect_uri": AGY_OAUTH_REDIRECT_URI,
            "scope": AGY_OAUTH_SCOPES,
            "access_type": "offline",
            "prompt": "consent",
            "state": state,
            "code_challenge": challenge,
            "code_challenge_method": "S256",
        }
    )
    auth_url = f"{AGY_OAUTH_AUTH_URL}?{params}"

    parsed_redirect = urlsplit(AGY_OAUTH_REDIRECT_URI)
    host = "127.0.0.1"
    port = parsed_redirect.port or 51121
    callback_path = parsed_redirect.path or "/oauth-callback"

    captured: dict[str, str] = {}

    class _AgyCallbackHandler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            req_url = urlsplit(self.path)
            if req_url.path != callback_path:
                self.send_response(404)
                self.end_headers()
                return
            qs = parse_qs(req_url.query)
            if err := qs.get("error"):
                captured["error"] = err[0]
            if code := qs.get("code"):
                captured["code"] = code[0]
            if got_state := qs.get("state"):
                captured["state"] = got_state[0]

            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.end_headers()
            body = (
                "<html><body><h2>le-vibe: Antigravity (agy) login complete.</h2>"
                "<p>You can close this window and return to your terminal.</p></body></html>"
            )
            self.wfile.write(body.encode("utf-8"))

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            return

    server = HTTPServer((host, port), _AgyCallbackHandler)
    server.timeout = 1.0
    print(f"Opening browser for Antigravity (agy) login:\n{auth_url}", file=sys.stderr, flush=True)
    webbrowser.open(auth_url)

    deadline = time.time() + timeout_s
    try:
        while time.time() < deadline and "code" not in captured and "error" not in captured:
            server.handle_request()
    finally:
        server.server_close()

    if "error" in captured:
        raise RuntimeError(f"Antigravity login failed: {captured['error']}")
    if "code" not in captured:
        raise TimeoutError("Timed out waiting for Antigravity browser login callback")
    if captured.get("state") != state:
        raise RuntimeError("Antigravity login state mismatch")

    token_data: dict[str, str] = {
        "grant_type": "authorization_code",
        "client_id": client_id,
        "client_secret": client_secret,
        "code": captured["code"],
        "redirect_uri": AGY_OAUTH_REDIRECT_URI,
        "code_verifier": verifier,
    }

    with httpx.Client(timeout=20.0) as client:
        tok_resp = client.post(AGY_OAUTH_TOKEN_URL, data=token_data)
        tok_resp.raise_for_status()
        tok = tok_resp.json()

    access_token = str(tok["access_token"])
    refresh_token = tok.get("refresh_token")
    expires_in = float(tok.get("expires_in", 3600))
    project_id = discover_agy_project_id(access_token)

    return save_oidc_tokens(
        "agy",
        access_token=access_token,
        refresh_token=str(refresh_token) if refresh_token else None,
        expires_at=time.time() + expires_in,
        token_endpoint=AGY_OAUTH_TOKEN_URL,
        client_id=client_id,
        project_id=project_id or None,
        auth_file=auth_file,
    )


def get_agy_project_id(*, auth_file: Path | None = None) -> str:
    if env_proj := os.environ.get("AGY_PROJECT_ID"):
        return env_proj.strip()
    saved = _read_auth_store(auth_file).get("agy")
    if isinstance(saved, dict) and isinstance(saved.get("project_id"), str) and saved["project_id"]:
        return saved["project_id"]
    return ""


def _read_vibe_keyring_api_key() -> str | None:
    try:
        import keyring

        for svc in ("ai.mistral.vibe", "ai.mistral.meow", "vibe"):
            val = keyring.get_password(svc, "MISTRAL_API_KEY")
            if isinstance(val, str) and val.strip():
                return val.strip()
    except Exception:
        pass
    return None


def resolve_api_key(
    provider: ProviderId,
    *,
    explicit_key: str | None = None,
    env: Mapping[str, str] | None = None,
    auth_file: Path | None = None,
) -> AuthCredential:
    if provider not in PROVIDER_ENV_KEYS:
        raise ValueError(
            f"Unsupported provider: {provider!r}. Only 'vibe', 'deepseek', and 'agy' are allowed."
        )

    if explicit_key and explicit_key.strip():
        return AuthCredential(provider=provider, token=explicit_key.strip(), source="explicit")

    env_map = env if env is not None else os.environ
    env_key = PROVIDER_ENV_KEYS[provider]
    value = env_map.get(env_key, "").strip()
    if value:
        return AuthCredential(provider=provider, token=value, source="env")

    store = _read_auth_store(auth_file)
    saved = store.get(provider)
    if isinstance(saved, dict):
        if isinstance(saved.get("key"), str) and saved["key"].strip():
            return AuthCredential(provider=provider, token=saved["key"].strip(), source="saved")
        if saved.get("type") == "oidc":
            if cred := _resolve_oidc_credential(provider, saved, auth_file=auth_file):
                return cred

    if provider == "vibe" and env is None and auth_file is None:
        if kr_key := _read_vibe_keyring_api_key():
            return AuthCredential(provider="vibe", token=kr_key, source="keyring")

    raise MissingAPIKeyError(env_key, provider)


def has_auth(
    provider: ProviderId,
    *,
    env: Mapping[str, str] | None = None,
    auth_file: Path | None = None,
) -> bool:
    try:
        resolve_api_key(provider, env=env, auth_file=auth_file)
        return True
    except MissingAPIKeyError:
        return False


def auth_status(
    *,
    env: Mapping[str, str] | None = None,
    auth_file: Path | None = None,
) -> dict[str, dict[str, str | bool]]:
    status: dict[str, dict[str, str | bool]] = {}
    for provider in ("vibe", "deepseek", "agy"):
        try:
            cred = resolve_api_key(provider, env=env, auth_file=auth_file)
            status[provider] = {"authenticated": True, "source": cred.source}
        except MissingAPIKeyError:
            status[provider] = {"authenticated": False, "source": "none"}
    return status
