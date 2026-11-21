"""Extension: `advise` tool — independent second-opinion review from another provider/model."""

from __future__ import annotations

from typing import Any

from le_vibe.extensions import ExtensionHost, PythonToolSpec
from le_vibe.providers import (
    Message,
    ModelSpec,
    StreamOptions,
    TranscriptContext,
    complete,
    get_provider,
    list_models,
)


def _pick_reviewer(preferred: str | None) -> ModelSpec:
    order = [preferred] if preferred else []
    order.extend(p for p in ("agy", "deepseek", "vibe") if p not in order)
    for prov in order:
        if not prov:
            continue
        try:
            if not get_provider(prov).is_authenticated():
                continue
        except Exception:
            continue
        models = list_models(prov)
        if models:
            return models[0]
    raise RuntimeError("No authenticated provider available for advise review.")


def register(host: ExtensionHost) -> None:
    async def run_advise(args: dict[str, Any]) -> dict[str, Any]:
        plan = str(args.get("plan") or "").strip()
        question = str(args.get("question") or "Review this approach critically.").strip()
        preferred = str(args.get("provider") or "").strip() or None
        if not plan and not question:
            return {"error": "Provide `plan` or `question` for review."}

        model = _pick_reviewer(preferred)
        system = (
            "You are an independent, critical senior engineer giving a second opinion. "
            "Do not agree by default. Output VERDICT: PASS | NEEDS CHANGES, then concrete "
            "Problems (bugs, races, security flaws, over-engineering) and Do Next steps."
        )
        user_msg = f"Question: {question}\n\nPlan / Context:\n{plan}" if plan else question
        ctx = TranscriptContext(
            system_prompt=system,
            messages=[Message(role="user", content=user_msg)],
        )
        reply = await complete(model, ctx, StreamOptions(thinking_level="medium"))
        return {
            "reviewer": f"{model.provider}/{model.id}",
            "advice": reply.content,
        }

    host.register_tool(
        PythonToolSpec(
            name="advise",
            description="Get an independent critical second opinion on a plan or problem from another authenticated provider/model.",
            parameters={
                "type": "object",
                "required": ["question"],
                "properties": {
                    "question": {"type": "string", "description": "Specific question or thing to check"},
                    "plan": {"type": "string", "description": "Intended plan, diff, or context to review"},
                    "provider": {"type": "string", "description": "Optional reviewer provider (agy | deepseek | vibe)"},
                },
            },
            handler=run_advise,
        )
    )
