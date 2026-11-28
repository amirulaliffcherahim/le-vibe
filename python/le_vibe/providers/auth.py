from __future__ import annotations

import base64
import bisect
from collections.abc import Mapping
import hashlib
import hmac
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import ssl
import struct
import sys
import threading
import time
from typing import Any
from urllib.parse import parse_qs, urlencode, urlsplit
import webbrowser

import httpx

from le_vibe.providers.types import AuthCredential, ProviderId

# Manual API keys are supported for DeepSeek only. `vibe` and `agy` are
# OAuth-only: their reference clients never accept a pasted API key.
PROVIDER_ENV_KEYS: dict[ProviderId, str] = {"deepseek": "DEEPSEEK_API_KEY"}

# ---------------------------------------------------------------------------
# Mistral / Vibe (reference: mistral-vibe 3.1.2 setup/auth)
# ---------------------------------------------------------------------------

MISTRAL_OIDC_ISSUER = "https://auth.mistral.ai"
MISTRAL_OIDC_CLIENT_ID = "829da7e7-628f-4795-829a-8cca609c76d4"
MISTRAL_OIDC_AUDIENCE = "https://api.mistral.ai"
MISTRAL_OIDC_REDIRECT_URI = "http://127.0.0.1:8766/callback"
MISTRAL_OIDC_SCOPE = "openid offline_access email profile"
MISTRAL_CONSOLE_API_BASE = "https://console.mistral.ai/api"
MISTRAL_API_BASE = "https://api.mistral.ai"
VIBE_USER_AGENT = "mistral-client-python/Mistral-Vibe/3.1.2"

_VIBE_OIDC_KEYRING_SERVICE = "ai.mistral.vibe.oidc"
_VIBE_OIDC_KEYRING_ENTRY = "cli-oidc:v1:tokens"
_VIBE_API_KEY_KEYRING_SERVICE = "ai.mistral.vibe"
_VIBE_ENV_FILE = Path.home() / ".vibe" / ".env"
_VIBE_TOKEN_LEEWAY_S = 30.0
_WORKSPACE_PAGE_SIZE = 100

# ---------------------------------------------------------------------------
# Antigravity / agy (reference: agy-cli 1.5.18)
# ---------------------------------------------------------------------------

AGY_OAUTH_AUTH_URL = "https://accounts.google.com/o/oauth2/auth"
AGY_OAUTH_TOKEN_URL = "https://oauth2.googleapis.com/token"
AGY_OAUTH_CALLBACK_PATH = "/auth/callback"
AGY_OAUTH_OOB_REDIRECT_URI = "https://antigravity.google/oauth-callback"
AGY_OAUTH_SUCCESS_URL = "https://antigravity.google/auth-success?app=cli"
AGY_OAUTH_SCOPES = (
    "https://www.googleapis.com/auth/cloud-platform "
    "https://www.googleapis.com/auth/userinfo.email "
    "https://www.googleapis.com/auth/userinfo.profile "
    "https://www.googleapis.com/auth/cclog "
    "https://www.googleapis.com/auth/experimentsandconfigs "
    "https://www.googleapis.com/auth/aicode "
    "openid"
)
AGY_USERINFO_URL = "https://www.googleapis.com/oauth2/v2/userinfo"
AGY_TOKENINFO_URL = "https://www.googleapis.com/oauth2/v3/tokeninfo"
AGY_DEFAULT_PROJECT = "aicode-consumers"
AGY_CLI_VERSION = "1.5.18"
AGY_CLI_CHANGELIST = "1030252320"
AGY_ENDPOINTS: tuple[str, ...] = (
    "https://daily-cloudcode-pa.googleapis.com",
    "https://cloudcode-pa.googleapis.com",
    "https://aicode.googleapis.com",
)
AGY_DEFAULT_CLIENT_ID = os.environ.get("AGY_OAUTH_CLIENT_ID", "").strip()
AGY_DEFAULT_CLIENT_SECRET = os.environ.get("AGY_OAUTH_CLIENT_SECRET", "").strip()
AGY_TOKEN_EXPIRY_LEEWAY_S = 30.0
_AGY_FATAL_OAUTH_ERRORS = frozenset({"invalid_grant", "invalid_client", "unauthorized_client"})
_AGY_REQUIRED_SCOPES = frozenset(AGY_OAUTH_SCOPES.split()) - {"openid"}

_AGY_TOKEN_FILE = Path.home() / ".gemini" / "jetski-standalone-oauth-token"
_AGY_KEYRING_SERVICE = "gemini"
_AGY_KEYRING_ACCOUNT = "antigravity"
_AGY_KEYRING_SLOTS: tuple[tuple[str, str], ...] = (
    ("gemini:antigravity", "antigravity"),
    ("gemini", "antigravity"),
    ("jetski-standalone-oauth-token", "oauth-token"),
)
_OIDC_TOKEN_ENDPOINT_CACHE: dict[str, str] = {}


def _agy_os_type() -> str:
    if sys.platform.startswith("win"):
        return "windows"
    if sys.platform == "darwin":
        return "darwin"
    return "linux"


def _agy_arch() -> str:
    import platform

    machine = platform.machine().lower()
    return {
        "x86_64": "amd64",
        "amd64": "amd64",
        "aarch64": "arm64",
        "arm64": "arm64",
        "i386": "386",
        "i686": "386",
    }.get(machine, machine or "amd64")


def agy_user_agent(auth_method: str = "consumer") -> str:
    return (
        f"antigravity/cli/{AGY_CLI_VERSION} (aidev_client; os_type={_agy_os_type()}; "
        f"arch={_agy_arch()}; cl={AGY_CLI_CHANGELIST}; auth_method={auth_method})"
    )


# Kept for callers that imported the old constant.
AGY_USER_AGENT = agy_user_agent()


_AGY_OAUTH_CLIENT_CACHE: tuple[str, str] | None = None
_AGY_CID_RE = re.compile(rb"\d{10,13}-[a-z0-9]{20,40}\.apps\.googleusercontent\.com")
_AGY_CSEC_RE = re.compile(rb"GOCSPX-[A-Za-z0-9_-]{28}")


def _find_agy_binary() -> Path | None:
    which = shutil.which("agy")
    if which:
        p = Path(which)
        if p.is_file():
            return p
    candidates: list[Path] = []
    if sys.platform.startswith("win"):
        local_app = os.environ.get("LOCALAPPDATA", "").strip()
        if local_app:
            candidates.append(Path(local_app) / "agy" / "bin" / "agy.exe")
        candidates.append(Path.home() / "AppData" / "Local" / "agy" / "bin" / "agy.exe")
    else:
        candidates.extend(
            [
                Path.home() / ".local" / "bin" / "agy",
                Path("/usr/local/bin/agy"),
                Path("/opt/homebrew/bin/agy"),
            ]
        )
    for cand in candidates:
        if cand.is_file():
            return cand
    return None


def _extract_agy_oauth_from_pe(data: bytes) -> tuple[str, str] | None:
    if len(data) < 0x100 or data[:2] != b"MZ":
        return None
    try:
        pe_off = struct.unpack_from("<I", data, 0x3C)[0]
        if data[pe_off : pe_off + 4] != b"PE\x00\x00":
            return None
        num_sec = struct.unpack_from("<H", data, pe_off + 6)[0]
        opt_size = struct.unpack_from("<H", data, pe_off + 20)[0]
    except struct.error:
        return None
    sec_off = pe_off + 24 + opt_size
    secs: list[tuple[str, int, int, int, int]] = []
    text_sec: tuple[int, int, int] | None = None
    for i in range(num_sec):
        o = sec_off + i * 40
        if o + 40 > len(data):
            break
        name = data[o : o + 8].rstrip(b"\x00").decode("ascii", "ignore")
        vsize, vaddr, rsize, rptr = struct.unpack_from("<IIII", data, o + 8)
        secs.append((name, vaddr, vsize, rptr, rsize))
        if name == ".text":
            text_sec = (vaddr, rptr, rsize)
    if text_sec is None:
        return None

    def _foff_to_rva(foff: int) -> int | None:
        for _, vaddr, _, rptr, rsize in secs:
            if rptr <= foff < rptr + rsize:
                return vaddr + (foff - rptr)
        return None

    tvaddr, trptr, trsize = text_sec
    text = data[trptr : trptr + trsize]

    cids: list[tuple[str, int]] = []
    for m in _AGY_CID_RE.finditer(data):
        rva = _foff_to_rva(m.start())
        if rva is not None:
            cids.append((m.group(0).decode("ascii"), rva))
    csecs: list[tuple[str, int]] = []
    for m in _AGY_CSEC_RE.finditer(data):
        rva = _foff_to_rva(m.start())
        if rva is not None:
            csecs.append((m.group(0).decode("ascii"), rva))
    if not cids or not csecs:
        return None

    cid_rva_set = {rva for _, rva in cids}
    csec_rva_set = {rva for _, rva in csecs}
    cid_refs: dict[int, list[int]] = {rva: [] for rva in cid_rva_set}
    csec_refs: dict[int, list[int]] = {rva: [] for rva in csec_rva_set}

    # Single linear scan over .text for x86_64 RIP-relative LEA (48/4c 8d [mod=00,rm=101] disp32).
    idx = 1
    limit = len(text) - 5
    while True:
        pos = text.find(b"\x8d", idx, limit)
        if pos == -1:
            break
        idx = pos + 1
        if text[pos - 1] in (0x48, 0x4C) and (text[pos + 1] & 0xC7) == 0x05:
            disp = struct.unpack_from("<i", text, pos + 2)[0]
            target = tvaddr + pos + 6 + disp
            if target in cid_refs:
                cid_refs[target].append(tvaddr + pos - 1)
            elif target in csec_refs:
                csec_refs[target].append(tvaddr + pos - 1)

    # Pair each client_id with the client_secret referenced right next to it in .text.
    pairs: list[tuple[str, str, list[int]]] = []
    for cid_str, cid_rva in cids:
        crefs = cid_refs.get(cid_rva, [])
        if not crefs:
            continue
        best_csec: str | None = None
        best_hits = 0
        for csec_str, csec_rva in csecs:
            srefs = csec_refs.get(csec_rva, [])
            hits = sum(1 for cr in crefs if any(abs(cr - sr) <= 64 for sr in srefs))
            if hits > best_hits:
                best_hits = hits
                best_csec = csec_str
        if best_csec is not None:
            pairs.append((cid_str, best_csec, crefs))
    if not pairs:
        return None
    if len(pairs) == 1:
        return pairs[0][0], pairs[0][1]

    # Disambiguate consumer vs business vs MCP via Go's pclntab (`newBrowserLogin`).
    pcln = data.find(b"\xf1\xff\xff\xff\x00\x00")
    if pcln != -1 and pcln + 72 <= len(data):
        try:
            nfunc, _, _, funcname_off, _, _, _, pcln_off = struct.unpack_from("<8Q", data, pcln + 8)
            ftab = pcln + pcln_off
            funcs: list[tuple[int, int]] = []
            for i in range(nfunc):
                entryoff, funcoff = struct.unpack_from("<II", data, ftab + i * 8)
                funcs.append((tvaddr + entryoff, funcoff))
            rvas = [f[0] for f in funcs]
            for cid_str, csec_str, crefs in pairs:
                for ref in crefs:
                    pos = bisect.bisect_right(rvas, ref) - 1
                    if pos < 0:
                        continue
                    _, foff = funcs[pos]
                    nameoff = struct.unpack_from("<i", data, ftab + foff + 4)[0]
                    noff = pcln + funcname_off + nameoff
                    end = data.find(b"\x00", noff, noff + 256)
                    if end == -1:
                        continue
                    fname = data[noff:end].decode("utf-8", "ignore")
                    if fname.endswith(".newBrowserLogin"):
                        return cid_str, csec_str
        except (struct.error, IndexError):
            pass
    return pairs[0][0], pairs[0][1]


def _discover_agy_oauth_client() -> tuple[str, str] | None:
    global _AGY_OAUTH_CLIENT_CACHE
    if _AGY_OAUTH_CLIENT_CACHE is not None:
        return _AGY_OAUTH_CLIENT_CACHE
    bin_path = _find_agy_binary()
    if bin_path is None:
        return None
    try:
        data = bin_path.read_bytes()
    except OSError:
        return None
    pair = _extract_agy_oauth_from_pe(data)
    if pair is None:
        cids = list(dict.fromkeys(m.group(0).decode("ascii") for m in _AGY_CID_RE.finditer(data)))
        csecs = list(dict.fromkeys(m.group(0).decode("ascii") for m in _AGY_CSEC_RE.finditer(data)))
        if len(cids) == 1 and len(csecs) == 1:
            pair = (cids[0], csecs[0])
    if pair is None:
        return None
    _AGY_OAUTH_CLIENT_CACHE = pair
    # Cache to ~/.le-vibe/agy_oauth.json (0600) so future runs skip scanning the binary.
    try:
        cfg_path = default_auth_file().parent / "agy_oauth.json"
        cfg_path.parent.mkdir(parents=True, exist_ok=True)
        tmp = cfg_path.with_suffix(".tmp")
        tmp.write_text(
            json.dumps({"client_id": pair[0], "client_secret": pair[1]}, indent=2) + "\n",
            encoding="utf-8",
        )
        try:
            os.chmod(tmp, 0o600)
        except OSError:
            pass
        os.replace(tmp, cfg_path)
    except OSError:
        pass
    return pair


def _load_agy_oauth_client() -> tuple[str, str]:
    cid = os.environ.get("AGY_OAUTH_CLIENT_ID", "").strip() or AGY_DEFAULT_CLIENT_ID
    csec = (
        os.environ.get("AGY_OAUTH_CLIENT_SECRET", "").strip()
        or AGY_DEFAULT_CLIENT_SECRET
    )
    if cid and csec:
        return cid, csec
    cfg_path = default_auth_file().parent / "agy_oauth.json"
    if cfg_path.exists():
        try:
            data = json.loads(cfg_path.read_text(encoding="utf-8-sig"))
            file_cid = str(data.get("client_id", "")).strip()
            file_csec = str(data.get("client_secret", "")).strip()
            if file_cid and file_csec:
                return file_cid, file_csec
        except (OSError, json.JSONDecodeError):
            pass
    if not os.environ.get("LE_VIBE_AUTH_FILE"):
        discovered = _discover_agy_oauth_client()
        if discovered is not None:
            return discovered
    raise RuntimeError(
        "Missing AGY_OAUTH_CLIENT_ID / AGY_OAUTH_CLIENT_SECRET (set env vars or ~/.le-vibe/agy_oauth.json)"
    )


def _should_use_agy_oob_auth(env: Mapping[str, str] | None = None) -> bool:
    env_map = env if env is not None else os.environ
    override = env_map.get("ANTIGRAVITY_REMOTE_OOB_AUTH", "").strip().lower()
    if override in {"1", "true", "yes"}:
        return True
    if override in {"0", "false", "no"}:
        return False
    return bool(env_map.get("SSH_CLIENT") or env_map.get("SSH_TTY") or env_map.get("SSH_CONNECTION"))


def _b64url_decode(value: str) -> bytes:
    pad = "=" * (-len(value) % 4)
    return base64.urlsafe_b64decode(value + pad)


def _b64url_encode(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii")


def _pkce_pair() -> tuple[str, str]:
    verifier = _b64url_encode(secrets.token_bytes(32))
    challenge = _b64url_encode(hashlib.sha256(verifier.encode("ascii")).digest())
    return verifier, challenge


class _HTTPServerV6(HTTPServer):
    address_family = socket.AF_INET6


def _bind_agy_loopback_servers(handler_cls: type[BaseHTTPRequestHandler]) -> tuple[list[HTTPServer], int]:
    """Bind 127.0.0.1:0 and [::1]:<same port>, falling back to localhost:0."""
    servers: list[HTTPServer] = []
    try:
        v4 = HTTPServer(("127.0.0.1", 0), handler_cls)
        v4.timeout = 0.25
        port = int(v4.server_address[1])
        servers.append(v4)
        try:
            v6 = _HTTPServerV6(("::1", port), handler_cls)
            v6.timeout = 0.25
            servers.append(v6)
        except OSError:
            pass
        return servers, port
    except OSError:
        fallback = HTTPServer(("localhost", 0), handler_cls)
        fallback.timeout = 0.25
        return [fallback], int(fallback.server_address[1])


class MissingAPIKeyError(RuntimeError):
    def __init__(self, env_key: str, provider: ProviderId) -> None:
        if provider in {"vibe", "agy"}:
            msg = (
                f"Missing OAuth credentials for {provider} provider. "
                f"Run: le-vibe login --provider {provider}"
            )
        else:
            msg = (
                f"Missing {env_key} environment variable or saved credential for {provider} provider. "
                f"Run: le-vibe login --provider {provider}"
            )
        super().__init__(msg)
        self.env_key = env_key
        self.provider = provider


def default_auth_file() -> Path:
    if override := os.environ.get("LE_VIBE_AUTH_FILE"):
        return Path(override)
    return Path.home() / ".le-vibe" / "auth.json"


def _read_auth_store(auth_file: Path | None = None) -> dict[str, dict[str, Any]]:
    path = auth_file or default_auth_file()
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
        return data if isinstance(data, dict) else {}
    except (OSError, json.JSONDecodeError):
        return {}


def _write_auth_store(data: dict[str, dict[str, Any]], auth_file: Path | None = None) -> Path:
    path = auth_file or default_auth_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + f".{os.getpid()}.tmp")
    tmp.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    try:
        os.chmod(tmp, 0o600)
    except OSError:
        pass
    os.replace(tmp, path)
    return path


def _keyring_get(service: str, account: str) -> str | None:
    try:
        import keyring

        value = keyring.get_password(service, account)
        return value if isinstance(value, str) and value.strip() else None
    except Exception:
        return None


def _keyring_set(service: str, account: str, value: str) -> bool:
    try:
        import keyring

        keyring.set_password(service, account, value)
        return True
    except Exception:
        return False


def _keyring_delete(service: str, account: str) -> None:
    try:
        import keyring

        keyring.delete_password(service, account)
    except Exception:
        pass


def save_api_key(provider: ProviderId, api_key: str, *, auth_file: Path | None = None) -> Path:
    if provider != "deepseek":
        raise ValueError(
            f"API keys are not accepted for {provider!r}; use OAuth (le-vibe login --provider {provider})."
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
    subject: str | None = None,
    issuer: str | None = None,
    audience: str | None = None,
    nonce: str | None = None,
    id_token_expires_at: float | None = None,
) -> Path:
    store = _read_auth_store(auth_file)
    entry: dict[str, Any] = {"type": "oidc", "access_token": access_token}
    for key, value in (
        ("refresh_token", refresh_token),
        ("id_token", id_token),
        ("expires_at", expires_at),
        ("token_endpoint", token_endpoint),
        ("client_id", client_id),
        ("project_id", project_id),
        ("organization_id", organization_id),
        ("workspace_id", workspace_id),
        ("subject", subject),
        ("issuer", issuer),
        ("audience", audience),
        ("nonce", nonce),
        ("id_token_expires_at", id_token_expires_at),
    ):
        if value is not None:
            entry[key] = value
    store[provider] = entry
    path = _write_auth_store(store, auth_file)
    if auth_file is None and not os.environ.get("LE_VIBE_AUTH_FILE"):
        if provider == "agy":
            _save_agy_official_token(entry)
        elif provider == "vibe":
            _save_vibe_official_token(entry)
    return path


def _save_agy_official_token(entry: Mapping[str, Any]) -> None:
    expires_at = entry.get("expires_at")
    expiry = ""
    if isinstance(expires_at, (int, float)):
        expiry = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(float(expires_at)))
    composite: dict[str, Any] = {
        "token": {
            "access_token": entry.get("access_token", ""),
            "token_type": "Bearer",
        },
        "project": entry.get("project_id") or "",
        "region": entry.get("region") or "",
        "user_tier": entry.get("user_tier") or "",
        "auth_method": "consumer",
    }
    token = composite["token"]
    if entry.get("refresh_token"):
        token["refresh_token"] = entry["refresh_token"]
    if entry.get("id_token"):
        token["id_token"] = entry["id_token"]
    if expiry:
        token["expiry"] = expiry
    if entry.get("expires_at"):
        composite["expires_at"] = entry["expires_at"]
    raw = json.dumps(composite, separators=(",", ":"))
    # Public file: read by agy-cli when keyring is unavailable.
    try:
        _AGY_TOKEN_FILE.parent.mkdir(parents=True, exist_ok=True)
        tmp = _AGY_TOKEN_FILE.with_suffix(".tmp")
        tmp.write_text(raw, encoding="utf-8")
        try:
            os.chmod(tmp, 0o600)
        except OSError:
            pass
        os.replace(tmp, _AGY_TOKEN_FILE)
    except OSError:
        pass
    if sys.platform.startswith("win"):
        # zalando/go-keyring on Windows stores raw UTF-8 bytes under Target="service:user",
        # while Python's winvault keyring encodes strings as UTF-16LE.
        raw_bytes = raw.encode("utf-8")
        if len(raw_bytes) % 2 == 1:
            raw_bytes += b" "
        _keyring_set(
            f"{_AGY_KEYRING_SERVICE}:{_AGY_KEYRING_ACCOUNT}",
            _AGY_KEYRING_ACCOUNT,
            raw_bytes.decode("utf-16-le"),
        )
    else:
        _keyring_set(_AGY_KEYRING_SERVICE, _AGY_KEYRING_ACCOUNT, raw)


def _decode_keyring_json(raw: str) -> dict[str, Any] | None:
    try:
        data = json.loads(raw)
        if isinstance(data, dict):
            return data
    except json.JSONDecodeError:
        pass
    if sys.platform.startswith("win"):
        try:
            decoded = raw.encode("utf-16-le").decode("utf-8").strip()
            data = json.loads(decoded)
            if isinstance(data, dict):
                return data
        except (UnicodeError, json.JSONDecodeError):
            pass
    return None


def _load_agy_official_token() -> dict[str, Any] | None:
    data: dict[str, Any] | None = None
    for service, account in _AGY_KEYRING_SLOTS:
        raw = _keyring_get(service, account)
        if raw:
            data = _decode_keyring_json(raw)
            if data is not None:
                break
    if data is None:
        try:
            raw_file = _AGY_TOKEN_FILE.read_text(encoding="utf-8-sig")
            parsed = json.loads(raw_file)
            if isinstance(parsed, dict):
                data = parsed
        except (OSError, json.JSONDecodeError):
            return None
    if data is None:
        return None
    token = data.get("token") if isinstance(data.get("token"), dict) else data
    if not isinstance(token, dict):
        return None
    access = token.get("access_token")
    if not isinstance(access, str) or not access.strip():
        return None
    try:
        agy_cid = _load_agy_oauth_client()[0]
    except RuntimeError:
        agy_cid = ""
    entry: dict[str, Any] = {
        "type": "oidc",
        "access_token": access,
        "refresh_token": token.get("refresh_token"),
        "id_token": token.get("id_token"),
        "project_id": data.get("project") or data.get("project_id"),
        "region": data.get("region"),
        "user_tier": data.get("user_tier"),
        "token_endpoint": AGY_OAUTH_TOKEN_URL,
        "client_id": agy_cid or None,
    }
    expiry = token.get("expiry") or data.get("expires_at")
    if isinstance(expiry, str) and expiry:
        try:
            from datetime import UTC, datetime

            dt = datetime.fromisoformat(expiry.replace("Z", "+00:00"))
            if dt.tzinfo is None:
                dt = dt.replace(tzinfo=UTC)
            entry["expires_at"] = dt.timestamp()
        except ValueError:
            try:
                from email.utils import parsedate_to_datetime

                entry["expires_at"] = parsedate_to_datetime(expiry).timestamp()
            except (TypeError, ValueError, OverflowError):
                pass
    elif isinstance(expiry, (int, float)):
        entry["expires_at"] = float(expiry)
    return {k: v for k, v in entry.items() if v is not None}


def _save_vibe_official_token(entry: Mapping[str, Any]) -> None:
    payload: dict[str, Any] = {
        "version": 1,
        "issuer": entry.get("issuer") or MISTRAL_OIDC_ISSUER,
        "client_id": entry.get("client_id") or MISTRAL_OIDC_CLIENT_ID,
        "audience": entry.get("audience") or MISTRAL_OIDC_AUDIENCE,
        "subject": entry.get("subject") or "",
        "access_token": entry.get("access_token") or "",
        "refresh_token": entry.get("refresh_token"),
        "id_token": entry.get("id_token") or "",
        "expires_at": entry.get("expires_at") or 0.0,
        "nonce": entry.get("nonce"),
        "id_token_expires_at": entry.get("id_token_expires_at"),
        "tenant": None,
    }
    if entry.get("organization_id") or entry.get("workspace_id"):
        payload["tenant"] = {
            "selection": os.environ.get("VIBE_CLI_OIDC_WORKSPACE_ID", ""),
            "organization_id": entry.get("organization_id") or "",
            "workspace_id": entry.get("workspace_id") or "",
        }
    _keyring_set(
        _VIBE_OIDC_KEYRING_SERVICE,
        _VIBE_OIDC_KEYRING_ENTRY,
        json.dumps(payload, separators=(",", ":")),
    )


def _discover_vibe_token_endpoint(issuer: str) -> str:
    base = (issuer or MISTRAL_OIDC_ISSUER).rstrip("/")
    if cached := _OIDC_TOKEN_ENDPOINT_CACHE.get(base):
        return cached
    try:
        with httpx.Client(timeout=15.0, follow_redirects=False) as client:
            resp = client.get(f"{base}/.well-known/openid-configuration")
            resp.raise_for_status()
            endpoint = str(resp.json().get("token_endpoint") or "").strip()
            if endpoint:
                _validate_oidc_url(endpoint)
                _OIDC_TOKEN_ENDPOINT_CACHE[base] = endpoint
                return endpoint
    except Exception:
        pass
    return f"{base}/oauth2/token"


def _load_vibe_official_token() -> dict[str, Any] | None:
    raw = _keyring_get(_VIBE_OIDC_KEYRING_SERVICE, _VIBE_OIDC_KEYRING_ENTRY)
    if not raw:
        return None
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return None
    if not isinstance(data, dict) or not data.get("access_token"):
        return None
    issuer = str(data.get("issuer") or MISTRAL_OIDC_ISSUER)
    entry: dict[str, Any] = {
        "type": "oidc",
        "access_token": data.get("access_token"),
        "refresh_token": data.get("refresh_token"),
        "id_token": data.get("id_token"),
        "expires_at": data.get("expires_at"),
        "token_endpoint": _discover_vibe_token_endpoint(issuer),
        "client_id": data.get("client_id"),
        "issuer": issuer,
        "audience": data.get("audience"),
        "subject": data.get("subject"),
        "nonce": data.get("nonce"),
        "id_token_expires_at": data.get("id_token_expires_at"),
    }
    tenant = data.get("tenant")
    if isinstance(tenant, dict):
        entry["organization_id"] = tenant.get("organization_id")
        entry["workspace_id"] = tenant.get("workspace_id")
    return {k: v for k, v in entry.items() if v is not None}


def delete_credential(provider: ProviderId, *, auth_file: Path | None = None) -> bool:
    store = _read_auth_store(auth_file)
    removed = provider in store
    store.pop(provider, None)
    if removed:
        _write_auth_store(store, auth_file)
    if auth_file is None and not os.environ.get("LE_VIBE_AUTH_FILE"):
        if provider == "vibe":
            expected = _load_vibe_official_token()
            if expected and expected.get("refresh_token"):
                _revoke_vibe_refresh_token(str(expected["refresh_token"]))
            _keyring_delete(_VIBE_OIDC_KEYRING_SERVICE, _VIBE_OIDC_KEYRING_ENTRY)
            _remove_vibe_env_key()
        elif provider == "agy":
            for service, account in _AGY_KEYRING_SLOTS:
                _keyring_delete(service, account)
            try:
                _AGY_TOKEN_FILE.unlink()
            except OSError:
                pass
    return removed


def _remove_vibe_env_key() -> None:
    try:
        if not _VIBE_ENV_FILE.exists():
            return
        lines = [
            line
            for line in _VIBE_ENV_FILE.read_text(encoding="utf-8").splitlines()
            if not line.strip().startswith("MISTRAL_API_KEY=")
        ]
        _VIBE_ENV_FILE.write_text("\n".join(lines) + "\n", encoding="utf-8")
    except OSError:
        pass


def _revoke_vibe_refresh_token(refresh_token: str) -> None:
    """RFC 7009 revocation; best-effort, never raises."""
    try:
        with httpx.Client(timeout=5.0) as client:
            disc = client.get(f"{MISTRAL_OIDC_ISSUER}/.well-known/openid-configuration")
            disc.raise_for_status()
            endpoint = disc.json().get("revocation_endpoint")
            if not endpoint:
                return
            client.post(
                endpoint,
                data={
                    "token": refresh_token,
                    "token_type_hint": "refresh_token",
                    "client_id": MISTRAL_OIDC_CLIENT_ID,
                },
            )
    except Exception:
        return


def _extract_agy_auth_code(submitted: str, expected_state: str) -> str:
    raw = submitted.strip()
    if not raw:
        raise RuntimeError("Please enter the authorization code.")
    if "://" in raw or raw.startswith("?") or ("code=" in raw and "&" in raw):
        parsed = urlsplit(raw)
        qs = parse_qs(parsed.query)
        if err := qs.get("error"):
            desc = (qs.get("error_description") or [""])[0]
            detail = f"{err[0]}: {desc}" if desc else err[0]
            raise RuntimeError(f"Antigravity login failed: {detail}")
        got_state = (qs.get("state") or [""])[0]
        if got_state and not hmac.compare_digest(got_state, expected_state):
            raise RuntimeError("Authorization state mismatch. Please copy the code from the latest browser tab.")
        code = (qs.get("code") or [""])[0].strip()
        if not code:
            raise RuntimeError("No authorization code found in the pasted URL.")
        return code
    return raw


def _read_stdin_auth_code() -> str:
    """Read a pasted code from any stdin shape, including a TUI pybridge."""
    env_code = os.environ.get("LE_VIBE_AUTH_CODE", "").strip()
    if env_code:
        return env_code
    for stream in (sys.stdin, getattr(sys, "__stdin__", None)):
        if stream is None:
            continue
        try:
            line = stream.readline()
        except (OSError, ValueError, AttributeError):
            continue
        if line:
            return line
    try:
        return os.read(0, 4096).decode("utf-8", errors="replace")
    except OSError:
        return ""


def _oauth_error_code(resp: httpx.Response) -> str:
    """The RFC 6749 `error` code, which is safe to show (never the description or body)."""
    try:
        return str(resp.json().get("error") or "").strip().lower()
    except Exception:
        return ""


def _is_fatal_oauth_error(resp: httpx.Response) -> bool:
    """A failure retrying cannot fix: the grant itself is dead."""
    if resp.status_code not in {400, 401, 403}:
        return False
    return _oauth_error_code(resp) in _AGY_FATAL_OAUTH_ERRORS


def _agy_endpoint_candidates() -> tuple[str, ...]:
    env_override = (
        os.environ.get("AGY_ENDPOINT_URL", "").strip()
        or os.environ.get("CLOUD_CODE_URL", "").strip()
        or os.environ.get("AICODE_ENDPOINT_URL", "").strip()
    )
    if env_override:
        return (env_override.rstrip("/"),)
    gateway = os.environ.get("AGY_LLM_GATEWAY_URL", "").strip()
    if gateway:
        return (*AGY_ENDPOINTS, gateway.rstrip("/"))
    return AGY_ENDPOINTS


def _agy_headers(
    access_token: str,
    *,
    project_id: str = "",
    request_id: str = "",
    auth_method: str = "consumer",
    request_reason: str = "",
) -> dict[str, str]:
    headers: dict[str, str] = {
        "User-Agent": agy_user_agent(auth_method=auth_method),
        "Authorization": f"Bearer {access_token}",
        "Content-Type": "application/json",
    }
    reason = request_reason or os.environ.get("AGY_REQUEST_REASON", "").strip()
    if reason:
        headers["X-Goog-Request-Reason"] = reason
    if request_id:
        headers["X-Aicode-Request-Id"] = request_id
    user_project = os.environ.get("AGY_USER_PROJECT", "").strip()
    if auth_method != "consumer" and project_id:
        headers["X-Goog-User-Project"] = project_id
    elif user_project:
        headers["X-Goog-User-Project"] = user_project
    return headers


def discover_agy_project_id(access_token: str) -> str:
    """Resolve cloudaicompanionProject from Antigravity loadCodeAssist."""
    for endpoint in _agy_endpoint_candidates():
        try:
            with httpx.Client(timeout=12.0) as client:
                resp = client.post(
                    f"{endpoint}/v1internal:loadCodeAssist",
                    headers=_agy_headers(access_token),
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


def _agy_fetch_user_info(access_token: str, project_id: str) -> bool:
    body = {"project": project_id} if project_id else {}
    for endpoint in _agy_endpoint_candidates():
        try:
            with httpx.Client(timeout=12.0) as client:
                resp = client.post(
                    f"{endpoint}/v1internal:fetchUserInfo",
                    headers=_agy_headers(access_token, project_id=project_id),
                    json=body,
                )
                if resp.is_success:
                    return True
        except Exception:
            continue
    return False


def _agy_granted_scopes(access_token: str) -> set[str]:
    try:
        with httpx.Client(timeout=12.0) as client:
            resp = client.get(
                AGY_TOKENINFO_URL,
                params={"access_token": access_token},
                headers={"User-Agent": agy_user_agent()},
            )
            if not resp.is_success:
                return set()
            data = resp.json()
    except Exception:
        return set()
    scopes = data.get("scope")
    if isinstance(scopes, str):
        return set(scopes.split())
    if isinstance(scopes, list):
        return {str(s) for s in scopes}
    return set()


def _agy_load_code_assist(access_token: str, project_id: str) -> tuple[bool, str]:
    request_id = f"agent/{secrets.token_hex(16)}/{int(time.time() * 1000)}"
    for endpoint in _agy_endpoint_candidates():
        try:
            with httpx.Client(timeout=12.0) as client:
                resp = client.post(
                    f"{endpoint}/v1internal:loadCodeAssist",
                    headers=_agy_headers(access_token, project_id=project_id, request_id=request_id),
                    json={"metadata": {"ideType": "ANTIGRAVITY"}},
                )
                if not resp.is_success:
                    continue
                data = resp.json()
                resolved = project_id
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
                        resolved = raw.strip()
                        break
                    if isinstance(raw, dict) and isinstance(raw.get("id"), str) and raw["id"].strip():
                        resolved = raw["id"].strip()
                        break
                return True, resolved
        except Exception:
            continue
    return False, project_id


def _verify_agy_candidate(
    access_token: str,
    *,
    project_id: str,
) -> str | None:
    """Run fetchUserInfo -> tokeninfo scopes -> loadCodeAssist in order."""
    if not _agy_fetch_user_info(access_token, project_id):
        return None
    granted = _agy_granted_scopes(access_token)
    if not granted or not _AGY_REQUIRED_SCOPES.issubset(granted):
        return None
    ok, resolved = _agy_load_code_assist(access_token, project_id)
    return resolved if ok else None


def _build_agy_auth_url(
    *,
    client_id: str,
    redirect_uri: str,
    state: str,
    challenge: str,
) -> str:
    # url.Values.Encode() order from the reference: alphabetical keys.
    params = urlencode(
        {
            "access_type": "offline",
            "client_id": client_id,
            "code_challenge": challenge,
            "code_challenge_method": "S256",
            "prompt": "consent",
            "redirect_uri": redirect_uri,
            "response_type": "code",
            "scope": AGY_OAUTH_SCOPES,
            "state": state,
        }
    )
    return f"{AGY_OAUTH_AUTH_URL}?{params}"


def login_agy_oauth(
    *,
    auth_file: Path | None = None,
    timeout_s: float = 180.0,
    oob: bool | None = None,
    code_reader: object | None = None,
) -> Path:
    """Run Google Antigravity OAuth 2.0 PKCE login, matching agy-cli 1.5.18."""
    client_id, client_secret = _load_agy_oauth_client()
    verifier, challenge = _pkce_pair()
    state = secrets.token_urlsafe(24)
    use_oob = _should_use_agy_oob_auth() if oob is None else oob

    if use_oob:
        redirect_uri = AGY_OAUTH_OOB_REDIRECT_URI
        auth_url = _build_agy_auth_url(
            client_id=client_id,
            redirect_uri=redirect_uri,
            state=state,
            challenge=challenge,
        )
        print(f"Open this URL in your browser to sign in to Antigravity (agy):\n{auth_url}", file=sys.stderr, flush=True)
        if callable(code_reader):
            submitted = str(code_reader())
        else:
            print("Enter the authorization code (or full callback URL): ", end="", file=sys.stderr, flush=True)
            submitted = _read_stdin_auth_code()
        auth_code = _extract_agy_auth_code(submitted, state)
        tok = _exchange_agy_code(
            client_id=client_id,
            client_secret=client_secret,
            code=auth_code,
            redirect_uri=redirect_uri,
            verifier=verifier,
        )
        resolved_project = _verify_agy_candidate(
            str(tok["access_token"]),
            project_id=str(tok.get("project_id") or ""),
        )
        if resolved_project is None:
            raise RuntimeError(
                "Antigravity login could not be verified (user info, granted scopes, or loadCodeAssist failed)."
            )
        return _store_agy_tokens(tok, client_id=client_id, project_id=resolved_project, auth_file=auth_file)

    captured: dict[str, str] = {}
    done = threading.Event()
    exchange_done = threading.Event()
    response_sent = threading.Event()
    cap_lock = threading.Lock()
    exchange_result: dict[str, Any] = {}

    class _AgyCallbackHandler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            req_url = urlsplit(self.path)
            if req_url.path != AGY_OAUTH_CALLBACK_PATH:
                self.send_response(404)
                self.end_headers()
                return
            qs = parse_qs(req_url.query)
            got_state = (qs.get("state") or [""])[0]
            if not hmac.compare_digest(got_state, state):
                self.send_response(400)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
                self.end_headers()
                self.wfile.write(b"Invalid state parameter")
                with cap_lock:
                    if not done.is_set():
                        captured["error"] = "state_mismatch"
                        done.set()
                return

            if err := qs.get("error"):
                desc = (qs.get("error_description") or [""])[0]
                detail = f"{err[0]}: {desc}" if desc else err[0]
                self.send_response(400)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
                self.end_headers()
                self.wfile.write(f"Google sign-in was canceled or access was denied: {detail}".encode("utf-8"))
                with cap_lock:
                    if not done.is_set():
                        captured["error"] = detail
                        done.set()
                return

            code = (qs.get("code") or [""])[0].strip()
            if not code:
                self.send_response(400)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
                self.end_headers()
                self.wfile.write(b"No authorization code in callback")
                with cap_lock:
                    if not done.is_set():
                        captured["error"] = "no authorization code in callback"
                        done.set()
                return

            with cap_lock:
                if done.is_set():
                    self.send_response(409)
                    self.send_header("Content-Type", "text/plain; charset=utf-8")
                    self.end_headers()
                    self.wfile.write(b"Sign-in session is no longer active. Please start sign-in again.")
                    return
                captured["code"] = code
                done.set()

            # Do not redirect until the code exchange and verification finished.
            exchange_done.wait(timeout=60.0)
            try:
                if exchange_result.get("ok"):
                    self.send_response(302)
                    self.send_header("Location", AGY_OAUTH_SUCCESS_URL)
                    self.end_headers()
                else:
                    self.send_response(400)
                    self.send_header("Content-Type", "text/plain; charset=utf-8")
                    self.end_headers()
                    detail = str(exchange_result.get("error") or "code exchange failed")
                    self.wfile.write(f"Antigravity sign-in failed: {detail}".encode("utf-8"))
                self.wfile.flush()
            finally:
                response_sent.set()

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            return

    servers, port = _bind_agy_loopback_servers(_AgyCallbackHandler)
    redirect_uri = f"http://localhost:{port}{AGY_OAUTH_CALLBACK_PATH}"
    auth_url = _build_agy_auth_url(
        client_id=client_id,
        redirect_uri=redirect_uri,
        state=state,
        challenge=challenge,
    )

    def _serve(srv: HTTPServer) -> None:
        while not done.is_set():
            try:
                srv.handle_request()
            except OSError:
                break

    threads = [threading.Thread(target=_serve, args=(s,), daemon=True) for s in servers]
    for t in threads:
        t.start()

    print(f"Opening browser for Antigravity (agy) login:\n{auth_url}", file=sys.stderr, flush=True)
    webbrowser.open(auth_url)

    try:
        done.wait(timeout=timeout_s)
        if "code" in captured:
            try:
                tok = _exchange_agy_code(
                    client_id=client_id,
                    client_secret=client_secret,
                    code=captured["code"],
                    redirect_uri=redirect_uri,
                    verifier=verifier,
                )
                resolved_project = _verify_agy_candidate(
                    str(tok["access_token"]),
                    project_id=str(tok.get("project_id") or ""),
                )
                if resolved_project is None:
                    raise RuntimeError(
                        "Antigravity login could not be verified (user info, granted scopes, or loadCodeAssist failed)."
                    )
                exchange_result["ok"] = True
                exchange_result["path"] = _store_agy_tokens(
                    tok, client_id=client_id, project_id=resolved_project, auth_file=auth_file
                )
            except Exception as exc:  # noqa: BLE001
                exchange_result["error"] = str(exc)
    finally:
        exchange_done.set()
        if "code" in captured:
            response_sent.wait(timeout=2.0)
        done.set()
        for t in threads:
            t.join(timeout=1.0)
        for s in servers:
            try:
                s.server_close()
            except OSError:
                pass

    if "error" in captured:
        if captured["error"] == "state_mismatch":
            raise RuntimeError("Antigravity login state mismatch")
        raise RuntimeError(f"Antigravity login failed: {captured['error']}")
    if "code" not in captured:
        raise TimeoutError("Timed out waiting for Antigravity browser login callback")
    if not exchange_result.get("ok"):
        raise RuntimeError(f"Antigravity login failed: {exchange_result.get('error') or 'code exchange failed'}")
    return exchange_result["path"]


def _exchange_agy_code(
    *,
    client_id: str,
    client_secret: str,
    code: str,
    redirect_uri: str,
    verifier: str,
) -> dict[str, Any]:
    token_data = {
        "grant_type": "authorization_code",
        "client_id": client_id,
        "client_secret": client_secret,
        "code": code,
        "redirect_uri": redirect_uri,
        "code_verifier": verifier,
    }
    with httpx.Client(timeout=20.0) as client:
        tok_resp = client.post(AGY_OAUTH_TOKEN_URL, data=token_data)
        tok_resp.raise_for_status()
        return tok_resp.json()


def _store_agy_tokens(
    tok: Mapping[str, Any],
    *,
    client_id: str,
    project_id: str,
    auth_file: Path | None,
) -> Path:
    access_token = str(tok["access_token"])
    refresh_token = tok.get("refresh_token")
    id_token = str(tok.get("id_token") or "")
    expires_in = float(tok.get("expires_in", 3600))
    return save_oidc_tokens(
        "agy",
        access_token=access_token,
        refresh_token=str(refresh_token) if refresh_token else None,
        id_token=id_token or None,
        expires_at=time.time() + expires_in,
        token_endpoint=AGY_OAUTH_TOKEN_URL,
        client_id=client_id,
        project_id=project_id or None,
        issuer="https://accounts.google.com",
        audience=client_id,
        auth_file=auth_file,
    )


def get_agy_project_id(*, auth_file: Path | None = None) -> str:
    if env_proj := os.environ.get("AGY_PROJECT_ID"):
        return env_proj.strip()
    saved = _read_auth_store(auth_file).get("agy")
    if isinstance(saved, dict) and isinstance(saved.get("project_id"), str) and saved["project_id"]:
        return saved["project_id"]
    official = _load_agy_official_token()
    if official and isinstance(official.get("project_id"), str):
        return official["project_id"]
    return ""


def _resolve_oidc_credential(
    provider: ProviderId,
    saved: Mapping[str, Any],
    *,
    auth_file: Path | None = None,
    force_refresh: bool = False,
) -> AuthCredential | None:
    lock = _REFRESH_LOCKS.setdefault(provider, threading.RLock())
    with lock:
        # Re-read inside the lock: another process/thread may have rotated it.
        if auth_file is None and not os.environ.get("LE_VIBE_AUTH_FILE"):
            current = _official_oidc_entry(provider) or _read_auth_store(auth_file).get(provider)
        else:
            current = _read_auth_store(auth_file).get(provider)
        if isinstance(current, dict):
            saved = current
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
        issuer = str(saved.get("issuer") or "")
        audience = str(saved.get("audience") or "")
        nonce = str(saved.get("nonce") or "")
        subject = str(saved.get("subject") or "")
        id_token_expires_at = saved.get("id_token_expires_at")

        leeway = _VIBE_TOKEN_LEEWAY_S if provider == "vibe" else AGY_TOKEN_EXPIRY_LEEWAY_S
        needs_refresh = bool(force_refresh)
        if not needs_refresh and isinstance(expires_at, (int, float)):
            needs_refresh = time.time() >= float(expires_at) - leeway
        if provider == "vibe" and isinstance(id_token_expires_at, (int, float)):
            needs_refresh = needs_refresh or time.time() >= float(id_token_expires_at) - leeway
        if (
            not needs_refresh
            and provider == "vibe"
            and not id_token
            and isinstance(refresh_token, str)
            and isinstance(token_endpoint, str)
        ):
            needs_refresh = True

        if needs_refresh and isinstance(refresh_token, str) and isinstance(token_endpoint, str):
            last = _JUST_REFRESHED.get(provider, 0.0)
            if force_refresh and time.time() - last < 120.0:
                # A forced refresh raced another caller; reuse the fresh token.
                needs_refresh = False
            else:
                effective_client_id = (
                    str(saved.get("client_id"))
                    if saved.get("client_id")
                    else (AGY_DEFAULT_CLIENT_ID if provider == "agy" else MISTRAL_OIDC_CLIENT_ID)
                )
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
                        if not refresh_data["client_id"]:
                            raise
                try:
                    with httpx.Client(timeout=20.0) as client:
                        resp = client.post(str(token_endpoint), data=refresh_data)
                        if resp.is_success:
                            data = resp.json()
                            new_access = str(data.get("access_token") or access_token)
                            new_refresh = str(data.get("refresh_token") or refresh_token)
                            new_id = str(data.get("id_token") or id_token)
                            new_exp = time.time() + float(data.get("expires_in", 3600))
                            new_id_exp = id_token_expires_at
                            new_subject = subject
                            new_nonce = nonce
                            if provider == "vibe" and new_id:
                                claims = _verify_vibe_id_token(
                                    new_id,
                                    issuer=issuer or MISTRAL_OIDC_ISSUER,
                                    client_id=effective_client_id,
                                    audience=audience or MISTRAL_OIDC_AUDIENCE,
                                    nonce=new_nonce or None,
                                    previous_subject=subject or None,
                                )
                                new_id_exp = float(claims.get("exp", new_exp))
                                new_subject = str(claims.get("sub") or subject)
                            if provider == "vibe" and new_id and not org_id:
                                org_id, ws_id = _discover_vibe_tenant(new_id)
                            if provider == "agy" and not project_id:
                                project_id = discover_agy_project_id(new_access)
                            expected = dict(saved)
                            # Compare-and-swap: only commit if nobody rotated it.
                            latest = _read_auth_store(auth_file).get(provider)
                            if isinstance(latest, dict) and latest.get("access_token") not in {
                                None,
                                access_token,
                            } and latest.get("refresh_token") != refresh_token:
                                saved = latest
                            else:
                                save_oidc_tokens(
                                    provider,
                                    access_token=new_access,
                                    refresh_token=new_refresh,
                                    id_token=new_id or None,
                                    expires_at=new_exp,
                                    token_endpoint=str(token_endpoint),
                                    client_id=effective_client_id,
                                    project_id=project_id or None,
                                    organization_id=org_id or None,
                                    workspace_id=ws_id or None,
                                    auth_file=auth_file,
                                    subject=new_subject or None,
                                    issuer=issuer or None,
                                    audience=audience or None,
                                    nonce=new_nonce or None,
                                    id_token_expires_at=new_id_exp if isinstance(new_id_exp, (int, float)) else None,
                                )
                                _JUST_REFRESHED[provider] = time.time()
                                saved = dict(_read_auth_store(auth_file).get(provider) or {})
                            access_token = str(saved.get("access_token") or new_access)
                            id_token = str(saved.get("id_token") or new_id)
                            expires_at = saved.get("expires_at", new_exp)
                        elif provider == "agy" and _is_fatal_oauth_error(resp):
                            delete_credential("agy", auth_file=auth_file)
                            return None
                        elif provider == "vibe":
                            # Never swallow refresh errors on Vibe, but say what the user can do.
                            # The credential slot is shared with mistral-vibe, so a dead grant is
                            # reported, not evicted: deleting it would sign the other client out.
                            if _is_fatal_oauth_error(resp):
                                raise MissingAPIKeyError("", "vibe")
                            raise RuntimeError(
                                "vibe OAuth refresh is temporarily unavailable "
                                f"(HTTP {resp.status_code}); the stored session was kept, retry later"
                            )
                        # agy keeps the stored token on transient/network errors.
                except httpx.HTTPError as exc:
                    if provider == "vibe":
                        raise RuntimeError(
                            "vibe OAuth refresh is temporarily unavailable "
                            f"({type(exc).__name__}); the stored session was kept, retry later"
                        ) from exc
                    # agy keeps the stored token on transient/network errors.

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
                    id_token=id_token or None,
                    expires_at=float(expires_at) if isinstance(expires_at, (int, float)) else None,
                    token_endpoint=str(token_endpoint) if isinstance(token_endpoint, str) else None,
                    client_id=str(saved.get("client_id") or ""),
                    project_id=project_id,
                    auth_file=auth_file,
                )

        return AuthCredential(
            provider=provider,
            token=access_token.strip(),
            source="oidc",
            project_id=project_id or None,
        )


_REFRESH_LOCKS: dict[str, threading.RLock] = {}
_JUST_REFRESHED: dict[str, float] = {}


# ---------------------------------------------------------------------------
# Vibe OIDC login / verification
# ---------------------------------------------------------------------------


def _validate_oidc_url(value: str, *, loopback: bool = False) -> str:
    parsed = urlsplit(value)
    try:
        port = parsed.port
    except ValueError:
        raise RuntimeError("Invalid OIDC URL port") from None
    local = parsed.hostname in {"127.0.0.1", "::1"}
    invalid_origin = not parsed.hostname or parsed.username is not None or parsed.password is not None
    invalid_scheme = parsed.scheme != "https" and not (local and parsed.scheme == "http")
    if invalid_origin or invalid_scheme or parsed.query or parsed.fragment:
        raise RuntimeError("OIDC URLs must use HTTPS (HTTP is allowed only on loopback)")
    if loopback and (not local or parsed.scheme != "http" or not port):
        raise RuntimeError("OIDC redirect must use HTTP on a loopback IP with a fixed port")
    return value


def _fetch_jwks(url: str, timeout: float = 20.0) -> dict[str, Any]:
    with httpx.Client(timeout=timeout, follow_redirects=False) as client:
        resp = client.get(url)
        resp.raise_for_status()
        data = resp.json()
    if not isinstance(data, dict) or not isinstance(data.get("keys"), list):
        raise RuntimeError("OIDC JWKS response is invalid")
    return data


def _jwk_public_key(jwk: Mapping[str, Any]):
    from cryptography.hazmat.primitives.asymmetric import ec, rsa

    kty = jwk.get("kty")
    if kty == "RSA":
        n = int.from_bytes(_b64url_decode(str(jwk["n"])), "big")
        e = int.from_bytes(_b64url_decode(str(jwk["e"])), "big")
        return rsa.RSAPublicNumbers(e, n).public_key()
    if kty == "EC":
        crv = str(jwk.get("crv"))
        curve = {
            "P-256": ec.SECP256R1(),
            "P-384": ec.SECP384R1(),
            "P-521": ec.SECP521R1(),
        }.get(crv)
        if curve is None:
            raise RuntimeError(f"Unsupported OIDC EC curve: {crv}")
        x = int.from_bytes(_b64url_decode(str(jwk["x"])), "big")
        y = int.from_bytes(_b64url_decode(str(jwk["y"])), "big")
        return ec.EllipticCurvePublicNumbers(x, y, curve).public_key()
    raise RuntimeError(f"Unsupported OIDC JWK type: {kty}")


def _verify_jwt_signature(token: str, jwks: Mapping[str, Any]) -> dict[str, Any]:
    from cryptography.exceptions import InvalidSignature
    from cryptography.hazmat.primitives import hashes
    from cryptography.hazmat.primitives.asymmetric import ec, padding, utils

    parts = token.split(".")
    if len(parts) != 3:
        raise RuntimeError("OIDC ID token is malformed")
    header = json.loads(_b64url_decode(parts[0]))
    payload = json.loads(_b64url_decode(parts[1]))
    signature = _b64url_decode(parts[2])
    if not isinstance(header, dict) or not isinstance(payload, dict):
        raise RuntimeError("OIDC ID token is malformed")
    alg = str(header.get("alg") or "")
    kid = header.get("kid")
    keys = [k for k in jwks.get("keys", []) if isinstance(k, dict)]
    if kid:
        keys = [k for k in keys if k.get("kid") == kid] or keys
    hash_alg = {
        "RS256": hashes.SHA256,
        "RS384": hashes.SHA384,
        "RS512": hashes.SHA512,
        "ES256": hashes.SHA256,
        "ES384": hashes.SHA384,
        "ES512": hashes.SHA512,
    }.get(alg)
    if hash_alg is None:
        raise RuntimeError(f"Unsupported OIDC signing algorithm: {alg}")
    signing_input = f"{parts[0]}.{parts[1]}".encode("ascii")
    for jwk in keys:
        try:
            key = _jwk_public_key(jwk)
            if alg.startswith("RS"):
                key.verify(signature, signing_input, padding.PKCS1v15(), hash_alg())
            else:
                if len(signature) % 2:
                    raise InvalidSignature
                r = int.from_bytes(signature[: len(signature) // 2], "big")
                s = int.from_bytes(signature[len(signature) // 2 :], "big")
                der = utils.encode_dss_signature(r, s)
                key.verify(der, signing_input, ec.ECDSA(hash_alg()))
            return payload
        except (InvalidSignature, ValueError, TypeError):
            continue
    raise RuntimeError("OIDC ID token signature is invalid")


def _verify_vibe_id_token(
    token: str,
    *,
    issuer: str,
    client_id: str,
    audience: str,
    nonce: str | None,
    previous_subject: str | None = None,
    jwks: Mapping[str, Any] | None = None,
) -> dict[str, Any]:
    if jwks is None:
        with httpx.Client(timeout=20.0, follow_redirects=False) as client:
            disc = client.get(f"{issuer.rstrip('/')}/.well-known/openid-configuration")
            disc.raise_for_status()
            metadata = disc.json()
        _validate_oidc_url(str(metadata.get("jwks_uri") or ""))
        jwks = _fetch_jwks(str(metadata["jwks_uri"]))
    claims = _verify_jwt_signature(token, jwks)
    if claims.get("iss") != issuer:
        raise RuntimeError("OIDC ID token issuer mismatch")
    aud = claims.get("aud")
    aud_values = {aud} if isinstance(aud, str) else set(aud or [])
    if client_id not in aud_values and audience not in aud_values:
        raise RuntimeError("OIDC ID token audience mismatch")
    now = time.time()
    try:
        exp = float(claims["exp"])
    except (KeyError, TypeError, ValueError):
        raise RuntimeError("OIDC ID token has no expiry") from None
    if exp <= now - 30:
        raise RuntimeError("OIDC ID token has expired")
    nbf = claims.get("nbf")
    if isinstance(nbf, (int, float)) and float(nbf) > now + 30:
        raise RuntimeError("OIDC ID token is not yet valid")
    if nonce is not None and claims.get("nonce") != nonce:
        raise RuntimeError("OIDC ID token nonce mismatch")
    subject = claims.get("sub")
    if not isinstance(subject, str) or not subject:
        raise RuntimeError("OIDC identity has no subject")
    if previous_subject is not None and subject != previous_subject:
        raise RuntimeError("OIDC refresh changed the authenticated identity")
    return claims


def _discover_vibe_tenant(id_token: str, *, workspace_id: str | None = None) -> tuple[str, str]:
    """Paginate /v1/users/me/workspaces fully and honour the workspace override."""
    selected = workspace_id if workspace_id is not None else os.environ.get("VIBE_CLI_OIDC_WORKSPACE_ID", "")
    base = f"{MISTRAL_OIDC_AUDIENCE.rstrip('/')}/v1/users/me/workspaces"
    offset = 0
    try:
        with httpx.Client(timeout=20.0, follow_redirects=False) as client:
            while True:
                resp = client.get(
                    base,
                    params={"limit": _WORKSPACE_PAGE_SIZE, "offset": offset},
                    headers={"Authorization": f"Bearer {id_token}"},
                )
                resp.raise_for_status()
                payload = resp.json()
                workspaces = payload.get("workspaces") if isinstance(payload, dict) else None
                if not isinstance(workspaces, list):
                    return "", ""
                for workspace in workspaces:
                    if not isinstance(workspace, dict):
                        continue
                    if not selected or str(workspace.get("id") or "") == selected:
                        return (
                            str(workspace.get("organization_id") or ""),
                            str(workspace.get("id") or ""),
                        )
                if len(workspaces) < _WORKSPACE_PAGE_SIZE:
                    return "", ""
                offset += _WORKSPACE_PAGE_SIZE
    except Exception:
        return "", ""


def _vibe_account_context(id_token: str, *, audience: str) -> dict[str, Any] | None:
    """Console-host account context for a freshly minted ID token.

    This route lives on the console gateway, not on the inference host: the same path under
    `api.mistral.ai` answers `404 no Route matched`, which used to fail every Vibe sign-in.
    The payload is advisory, so callers must tolerate `None`.
    """
    del audience  # Kept for call-site parity with the reference client.
    try:
        with httpx.Client(timeout=20.0, follow_redirects=False) as client:
            resp = client.get(
                f"{MISTRAL_CONSOLE_API_BASE.rstrip('/')}/vibe/account-context",
                headers={"Authorization": f"Bearer {id_token}"},
            )
            if not resp.is_success:
                return None
            data = resp.json()
            return data if isinstance(data, dict) else None
    except Exception:
        return None


def login_vibe_oidc(
    *,
    auth_file: Path | None = None,
    timeout_s: float = 180.0,
) -> Path:
    """Sign in to Mistral Vibe with OIDC PKCE only (no API-key exchange)."""
    with httpx.Client(timeout=20.0, follow_redirects=False) as client:
        disc_resp = client.get(f"{MISTRAL_OIDC_ISSUER}/.well-known/openid-configuration")
        disc_resp.raise_for_status()
        disc = disc_resp.json()
    auth_endpoint = str(disc["authorization_endpoint"])
    token_endpoint = str(disc["token_endpoint"])
    jwks_uri = str(disc["jwks_uri"])
    for endpoint in (auth_endpoint, token_endpoint, jwks_uri):
        _validate_oidc_url(endpoint)
    if disc.get("issuer") != MISTRAL_OIDC_ISSUER:
        raise RuntimeError("OIDC discovery issuer mismatch")

    verifier, challenge = _pkce_pair()
    state = secrets.token_urlsafe(32)
    nonce = secrets.token_urlsafe(32)
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
            got_state = (qs.get("state") or [""])[0]
            if not hmac.compare_digest(got_state.encode(), state.encode()):
                self.send_response(400)
                self.end_headers()
                return
            if err := qs.get("error"):
                captured["error"] = err[0]
            if code := qs.get("code"):
                captured["code"] = code[0]
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            body = (
                "<html><body><h2>Sign-in complete</h2>"
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

    with httpx.Client(timeout=20.0) as client:
        tok_resp = client.post(
            token_endpoint,
            data={
                "grant_type": "authorization_code",
                "client_id": MISTRAL_OIDC_CLIENT_ID,
                "code": captured["code"],
                "redirect_uri": MISTRAL_OIDC_REDIRECT_URI,
                "code_verifier": verifier,
                "audience": MISTRAL_OIDC_AUDIENCE,
            },
        )
        tok_resp.raise_for_status()
        tok_data = tok_resp.json()

    access_token = str(tok_data["access_token"])
    id_token = str(tok_data.get("id_token") or "")
    if not id_token:
        raise RuntimeError("OIDC response did not include an ID token")
    if str(tok_data.get("token_type") or "").lower() != "bearer":
        raise RuntimeError("OIDC token response is invalid")
    claims = _verify_vibe_id_token(
        id_token,
        issuer=MISTRAL_OIDC_ISSUER,
        client_id=MISTRAL_OIDC_CLIENT_ID,
        audience=MISTRAL_OIDC_AUDIENCE,
        nonce=nonce,
        jwks=_fetch_jwks(jwks_uri),
    )
    # Advisory enrichment, never a gate: if this route moves again, sign-in must still work as
    # long as the account can reach a workspace. The ID token is already verified above.
    if _vibe_account_context(id_token, audience=MISTRAL_OIDC_AUDIENCE) is None:
        print(
            "note: Mistral account context unavailable; continuing with workspace discovery",
            file=sys.stderr,
        )
    org_id, ws_id = _discover_vibe_tenant(
        id_token,
        workspace_id=os.environ.get("VIBE_CLI_OIDC_WORKSPACE_ID", ""),
    )
    if not org_id:
        raise RuntimeError(
            "No accessible Mistral workspace found for this account: "
            f"GET {MISTRAL_OIDC_AUDIENCE.rstrip('/')}/v1/users/me/workspaces returned no usable "
            "workspace. Set VIBE_CLI_OIDC_WORKSPACE_ID to pick one explicitly."
        )
    refresh_token = tok_data.get("refresh_token")
    expires_in = float(tok_data.get("expires_in", 3600))
    return save_oidc_tokens(
        "vibe",
        access_token=access_token,
        refresh_token=str(refresh_token) if refresh_token else None,
        id_token=id_token,
        expires_at=time.time() + expires_in,
        token_endpoint=token_endpoint,
        client_id=MISTRAL_OIDC_CLIENT_ID,
        organization_id=org_id or None,
        workspace_id=ws_id or None,
        auth_file=auth_file,
        subject=str(claims.get("sub") or ""),
        issuer=MISTRAL_OIDC_ISSUER,
        audience=MISTRAL_OIDC_AUDIENCE,
        nonce=nonce,
        id_token_expires_at=float(claims.get("exp", time.time() + expires_in)),
    )


# ---------------------------------------------------------------------------
# Credential resolution
# ---------------------------------------------------------------------------


def _official_oidc_entry(provider: ProviderId) -> dict[str, Any] | None:
    if provider == "agy":
        return _load_agy_official_token()
    if provider == "vibe":
        return _load_vibe_official_token()
    return None


def resolve_api_key(
    provider: ProviderId,
    *,
    explicit_key: str | None = None,
    env: Mapping[str, str] | None = None,
    auth_file: Path | None = None,
    force_refresh: bool = False,
) -> AuthCredential:
    if provider not in {"vibe", "deepseek", "agy"}:
        raise ValueError(
            f"Unsupported provider: {provider!r}. Only 'vibe', 'deepseek', and 'agy' are allowed."
        )

    if provider == "deepseek":
        if explicit_key and explicit_key.strip():
            return AuthCredential(provider=provider, token=explicit_key.strip(), source="explicit")
        env_map = env if env is not None else os.environ
        value = env_map.get("DEEPSEEK_API_KEY", "").strip()
        if value:
            return AuthCredential(provider=provider, token=value, source="env")
        saved = _read_auth_store(auth_file).get("deepseek")
        if isinstance(saved, dict) and isinstance(saved.get("key"), str) and saved["key"].strip():
            return AuthCredential(provider=provider, token=saved["key"].strip(), source="saved")
        raise MissingAPIKeyError("DEEPSEEK_API_KEY", provider)

    # OAuth-only providers. Explicit API keys and MISTRAL_API_KEY/AGY_API_KEY
    # are deliberately ignored: the reference clients never accept pasted keys.
    # Prefer official CLI stores first so agy-cli / mistral-vibe and le-vibe share one grant.
    if auth_file is None and not os.environ.get("LE_VIBE_AUTH_FILE"):
        official = _official_oidc_entry(provider)
        if official is not None:
            if cred := _resolve_oidc_credential(provider, official, auth_file=auth_file, force_refresh=force_refresh):
                return cred
    saved = _read_auth_store(auth_file).get(provider)
    if isinstance(saved, dict) and saved.get("type") == "oidc":
        if cred := _resolve_oidc_credential(provider, saved, auth_file=auth_file, force_refresh=force_refresh):
            return cred
    raise MissingAPIKeyError(PROVIDER_ENV_KEYS.get(provider, ""), provider)


def has_auth(
    provider: ProviderId,
    *,
    env: Mapping[str, str] | None = None,
    auth_file: Path | None = None,
) -> bool:
    try:
        resolve_api_key(provider, env=env, auth_file=auth_file)
        return True
    except (MissingAPIKeyError, RuntimeError):
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
        except (MissingAPIKeyError, RuntimeError):
            status[provider] = {"authenticated": False, "source": "none"}
    return status
