from __future__ import annotations

LIFECYCLE = "lazy"
DESCRIPTION = "Planning tools (todo, ask_user_question)"


def register(host) -> None:
    from lazybug.extensions import PythonToolSpec

    todos: list[dict] = []

    def _todo(args: dict) -> dict:
        action = str(args.get("action") or "list")
        if action == "clear":
            todos.clear()
            return {"cleared": True}
        if action == "create":
            items = args.get("tasks") or [{"subject": args.get("subject", "")}]
            created = []
            for item in items:
                tid = len(todos) + 1
                rec = {
                    "id": tid,
                    "subject": str(item.get("subject", "")),
                    "status": "pending",
                }
                todos.append(rec)
                created.append(rec)
            return {"created": created}
        if action == "update":
            tid = int(args.get("id") or 0)
            for t in todos:
                if t["id"] == tid:
                    if "status" in args:
                        t["status"] = str(args["status"])
                    return {"updated": t}
            return {"error": f"Unknown todo id {tid}"}
        return {"todos": list(todos)}

    def _ask_user(args: dict) -> str:
        qs = args.get("questions") or []
        return f"Questions presented ({len(qs)}): " + "; ".join(
            str(q.get("question", "")) for q in qs
        )

    host.register_tool(
        PythonToolSpec(
            name="todo",
            description="Manage a multi-step task list: create/update/list/get/delete/clear. Status: pending -> in_progress -> completed.",
            parameters={
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": {"type": "string", "enum": ["create", "update", "list", "get", "delete", "clear"]},
                    "id": {"type": "integer"},
                    "subject": {"type": "string"},
                    "status": {"type": "string", "enum": ["pending", "in_progress", "completed", "deleted"]},
                    "tasks": {"type": "array"},
                },
            },
            handler=_todo,
        )
    )
    host.register_tool(
        PythonToolSpec(
            name="ask_user_question",
            description="Ask 1-4 structured clarifying questions; each option needs a label and description.",
            parameters={
                "type": "object",
                "required": ["questions"],
                "properties": {"questions": {"type": "array"}},
            },
            handler=_ask_user,
        )
    )
