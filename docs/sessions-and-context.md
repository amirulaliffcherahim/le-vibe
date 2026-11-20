# Sessions, Context & Handover

## Storage & DAG Format (`crates/le-vibe-durable/src/lib.rs`)

Sessions are stored per-project under `~/.le-vibe/sessions/<cwd-slug>/session-<ts>.jsonl`.
Each line is a `SessionEntry` with `id`, `parent_id`, `kind`, `role`, `content`, and optional `timestamp`, `tool_call_id`, `tool_name`:

- `message` — user or assistant text
- `tool_call` / `tool_result` — persisted tool invocations and 24KB-capped outputs
- `compaction_summary` — active-branch boundary created by `/compact` or `/handover`
- `branch_summary` — optional summary when switching branches
- `model_change` / `mode_change` — mid-session model or mode transitions
- `session_meta` — session display name (`/name`)

`SessionStore::active_branch()` walks from `leaf_id` back to root, stopping at the latest `compaction_summary`.

## Context Budgeting & Compaction

- **Tool-result cap**: `cap_tool_output` caps any single tool output pushed into the LLM message array at 24KB (16KB head + 6KB tail).
- **`/compact [instructions]`**: calls the active model to summarize the active branch into a `compaction_summary` entry.
- **Auto-compaction**: triggers automatically before a turn when `input_tokens >= 85%` of `context_window`.
- **`/context`**: displays estimated token breakdown (system + `AGENTS.md` + `<skills>`, active branch messages, and utilization %).

## Handover & Branching Commands

- `/handover [goal]` — summarizes the current session into a fresh session seeded with the handoff summary.
- `/tree` — interactive DAG modal to branch from any entry.
- `/fork [id]` — forks an entry into a new session file.
- `/clone` — duplicates the active branch into a new session file.
- `/resume` — interactive picker of saved project sessions.
- `/new`, `/name [title]`, `/session`, `/import <path>`, `/export [md|html] [path]`.

## Mid-Session Model Switching

`sanitize_messages_for_provider` strips provider-specific `thought_signature` when leaving `agy`, clears foreign `reasoning_content` when leaving `deepseek`, and synthesizes `[interrupted]` tool results for incomplete tool-call turns.
