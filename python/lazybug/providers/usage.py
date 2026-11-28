"""Live plan quota, budget, and balance reads for lazybug's three providers."""

from __future__ import annotations

from typing import Any

import httpx

from lazybug.providers.auth import (
    VIBE_USER_AGENT,
    MissingAPIKeyError,
    _agy_endpoint_candidates,
    _agy_headers,
    get_agy_project_id,
    resolve_api_key,
)
from lazybug.providers.transport import request_with_oauth_retry

DEEPSEEK_BALANCE_URL = "https://api.deepseek.com/user/balance"
MISTRAL_API_BASE = "https://api.mistral.ai/v1"


def _safe_str(value: Any) -> str:
    return value.strip() if isinstance(value, str) else ""


def _agy_family(model_id: str) -> str:
    lowered = model_id.lower()
    if lowered.startswith("gemini"):
        return "Gemini Models"
    if lowered.startswith(("claude", "gpt-oss", "gpt")):
        return "Claude and GPT Models"
    return "Other Models"


async def _agy_usage(client: httpx.AsyncClient) -> dict[str, Any]:
    try:
        cred = resolve_api_key("agy")
    except (MissingAPIKeyError, RuntimeError):
        return {"authenticated": False, "error": "not logged in"}

    project_id = cred.project_id or get_agy_project_id()
    headers = _agy_headers(cred.token, project_id=project_id)

    async def _refresh_agy() -> dict[str, str] | None:
        try:
            refreshed = resolve_api_key("agy", force_refresh=True)
        except (MissingAPIKeyError, RuntimeError):
            return None
        return _agy_headers(refreshed.token, project_id=refreshed.project_id or project_id)

    for endpoint in _agy_endpoint_candidates():
        try:
            tier_resp = await request_with_oauth_retry(
                client,
                "POST",
                f"{endpoint}/v1internal:loadCodeAssist",
                headers=headers,
                json_body={"metadata": {"ideType": "ANTIGRAVITY"}},
                on_unauthorized=_refresh_agy,
            )
            summary_resp = await request_with_oauth_retry(
                client,
                "POST",
                f"{endpoint}/v1internal:retrieveUserQuotaSummary",
                headers=headers,
                json_body={"project": project_id} if project_id else {},
                on_unauthorized=_refresh_agy,
            )
            models_resp = await request_with_oauth_retry(
                client,
                "POST",
                f"{endpoint}/v1internal:fetchAvailableModels",
                headers=headers,
                json_body={"project": project_id},
                on_unauthorized=_refresh_agy,
            )
        except Exception:
            continue

        tier_name = ""
        tier_id = ""
        if tier_resp.is_success:
            body = tier_resp.json()
            # `currentTier` is what this request is granted; `paidTier` is the plan the
            # account actually subscribes to, which is what the user recognises.
            current = body.get("currentTier") or {}
            paid = body.get("paidTier") or {}
            if isinstance(paid, dict) and _safe_str(paid.get("id")):
                tier_name = _safe_str(paid.get("name")) or _safe_str(current.get("name"))
                tier_id = _safe_str(paid.get("id"))
            elif isinstance(current, dict):
                tier_name = _safe_str(current.get("name"))
                tier_id = _safe_str(current.get("id"))

        if not models_resp.is_success:
            continue
        models_map = models_resp.json().get("models") or {}
        if not isinstance(models_map, dict):
            continue

        fractions: list[float] = []
        resets: list[str] = []
        exhausted: list[str] = []
        # Antigravity meters each model family against its own window, so a family is
        # a (label, reset time) pair: one bar per window the account actually has.
        groups: dict[tuple[str, str], dict[str, Any]] = {}
        for mid, info in models_map.items():
            if not isinstance(info, dict):
                continue
            # Tab/chat preview ids are editor helpers, not models an agent can select.
            if str(mid).startswith(("tab_", "chat_")):
                continue
            quota = info.get("quotaInfo")
            if not isinstance(quota, dict):
                continue
            frac = quota.get("remainingFraction")
            if isinstance(frac, (int, float)):
                fractions.append(float(frac))
                if float(frac) <= 0.0:
                    exhausted.append(str(mid))
            reset = _safe_str(quota.get("resetTime"))
            if reset:
                resets.append(reset)
            if not isinstance(frac, (int, float)):
                continue
            key = (_agy_family(str(mid)), reset)
            bucket = groups.get(key)
            if bucket is None:
                groups[key] = {"models": 1, "remaining": float(frac)}
            else:
                bucket["models"] += 1
                bucket["remaining"] = min(bucket["remaining"], float(frac))

        summary_groups: list[dict[str, Any]] = []
        if summary_resp.is_success:
            for grp in (summary_resp.json().get("groups") or []):
                if not isinstance(grp, dict):
                    continue
                buckets_out = []
                for b in grp.get("buckets") or []:
                    if not isinstance(b, dict):
                        continue
                    frac = b.get("remainingFraction")
                    if not isinstance(frac, (int, float)):
                        continue
                    rem = max(0.0, min(1.0, float(frac)))
                    fractions.append(rem)
                    if reset := _safe_str(b.get("resetTime")):
                        resets.append(reset)
                    buckets_out.append(
                        {
                            "label": _safe_str(b.get("displayName")) or _safe_str(b.get("bucketId")) or "Limit",
                            "window": _safe_str(b.get("window")) or None,
                            "remaining": round(rem, 4),
                            "reset_at": _safe_str(b.get("resetTime")) or None,
                        }
                    )
                if buckets_out:
                    summary_groups.append(
                        {
                            "label": _safe_str(grp.get("displayName")) or "Quota Group",
                            "buckets": buckets_out,
                        }
                    )

        final_groups = summary_groups or [
            {
                "label": label,
                "remaining": round(bucket["remaining"], 4),
                "reset_at": reset or None,
                "models": bucket["models"],
            }
            for (label, reset), bucket in sorted(
                groups.items(), key=lambda kv: (kv[0][0], kv[0][1])
            )
        ]

        return {
            "authenticated": True,
            "plan": tier_name or tier_id or "Antigravity",
            "tier_id": tier_id,
            "models_available": len(models_map),
            "quota_remaining": min(fractions) if fractions else None,
            "quota_reset_at": min(resets) if resets else None,
            "groups": final_groups,
            "exhausted_models": sorted(exhausted),
        }

    return {"authenticated": True, "error": "no Antigravity endpoint answered"}


async def _vibe_usage(client: httpx.AsyncClient) -> dict[str, Any]:
    try:
        cred = resolve_api_key("vibe")
    except (MissingAPIKeyError, RuntimeError):
        return {"authenticated": False, "error": "not logged in"}

    headers = {
        "Authorization": f"Bearer {cred.token}",
        "User-Agent": VIBE_USER_AGENT,
        **cred.extra_headers,
    }

    async def _refresh_vibe() -> dict[str, str] | None:
        try:
            refreshed = resolve_api_key("vibe", force_refresh=True)
        except (MissingAPIKeyError, RuntimeError):
            return None
        return {
            "Authorization": f"Bearer {refreshed.token}",
            "User-Agent": VIBE_USER_AGENT,
            **refreshed.extra_headers,
        }

    out: dict[str, Any] = {"authenticated": True}
    try:
        sub = await request_with_oauth_retry(
            client,
            "GET",
            f"{MISTRAL_API_BASE}/billing/subscription",
            headers=headers,
            on_unauthorized=_refresh_vibe,
        )
        if sub.is_success:
            data = sub.json()
            out["plan"] = _safe_str(data.get("plan_name")) or None
            out["payg_enabled"] = data.get("vibe_payg_enabled")
        else:
            out["subscription_status"] = sub.status_code
    except Exception:
        pass

    try:
        budget = await request_with_oauth_retry(
            client,
            "GET",
            f"{MISTRAL_API_BASE}/billing/budget",
            headers=headers,
            on_unauthorized=_refresh_vibe,
        )
        if budget.is_success:
            b_json = budget.json() or {}
            vibe_budget = b_json.get("vibe_budget")
            if isinstance(vibe_budget, dict):
                out["budget_used_pct"] = vibe_budget.get("usage_percentage")
                out["budget_total"] = vibe_budget.get("initial_budget")
                out["budget_reset_at"] = _safe_str(vibe_budget.get("reset_at")) or None
                out["payg_enabled"] = vibe_budget.get("payg_enabled", out.get("payg_enabled"))
            api_budget = b_json.get("api_budget")
            if isinstance(api_budget, dict):
                out["api_budget_used_pct"] = api_budget.get("usage_percentage")
                out["api_budget_total"] = api_budget.get("initial_budget")
                out["api_budget_reset_at"] = _safe_str(api_budget.get("reset_at")) or None
        else:
            out["budget_status"] = budget.status_code
    except Exception:
        pass
    return out


async def _deepseek_usage(client: httpx.AsyncClient) -> dict[str, Any]:
    try:
        cred = resolve_api_key("deepseek")
    except MissingAPIKeyError:
        return {"authenticated": False, "error": "not logged in"}

    headers = {"Authorization": f"Bearer {cred.token}"}
    try:
        resp = await client.get(DEEPSEEK_BALANCE_URL, headers=headers)
    except Exception as exc:  # noqa: BLE001
        return {"authenticated": True, "error": str(exc)}

    if not resp.is_success:
        return {"authenticated": True, "error": f"HTTP {resp.status_code}"}

    data = resp.json()
    balances = []
    for item in data.get("balance_infos") or []:
        if not isinstance(item, dict):
            continue
        balances.append(
            {
                "currency": _safe_str(item.get("currency")) or "USD",
                "total": _safe_str(item.get("total_balance")),
                "granted": _safe_str(item.get("granted_balance")),
                "topped_up": _safe_str(item.get("topped_up_balance")),
            }
        )
    return {
        "authenticated": True,
        "available": bool(data.get("is_available")),
        "balances": balances,
    }


async def fetch_usage(
    provider: str | None = None,
    *,
    timeout_s: float = 8.0,
) -> dict[str, dict[str, Any]]:
    """Read live plan quota / budget / balance for every provider (or one)."""
    targets = (provider,) if provider else ("vibe", "deepseek", "agy")
    out: dict[str, dict[str, Any]] = {}
    async with httpx.AsyncClient(timeout=timeout_s) as client:
        for prov in targets:
            if prov == "vibe":
                out["vibe"] = await _vibe_usage(client)
            elif prov == "deepseek":
                out["deepseek"] = await _deepseek_usage(client)
            elif prov == "agy":
                out["agy"] = await _agy_usage(client)
    return out
