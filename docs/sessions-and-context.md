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
- **Auto-compaction**: triggers automatically before a turn when `context_tokens >= 85%` of `context_window` (the percentage lives in the CLI; see below).
- **`/context`**: displays estimated token breakdown (system + `AGENTS.md` + `<skills>`, active branch messages, and utilization %).

### Context Lifecycle Policies (`le-vibe-agent`, P0 — tested, not yet wired)

`overflow.rs` and `compact.rs` hold the pi-equivalent decision functions. They are pure, so they
are covered by `crates/le-vibe-agent/tests/context_lifecycle.rs` without a provider or session file.
Wiring them into the turn loop is P1; until then the CLI's 85% rule remains the active trigger.

- `overflow::is_context_overflow(...)` — error-pattern overflow (case-insensitive, ~29 provider
  wordings), silent overflow (`stop` with prompt tokens already over the window), and rate-limit /
  throttling exclusions so a Bedrock `ThrottlingException: Too many tokens` is never mistaken for one.
- `overflow::is_recoverable_length(...)` — `length` stop whose output came in under the requested cap
  (input filled the window, leaving no room to generate).
- `compact::should_compact(tokens, window, reserve)` — pi's rule is a **token reserve**, not a
  percentage: `tokens > window - 16384`. A percentage is wrong at both ends (on a 32k window 85%
  leaves 4.8k, less than one large tool result; on a 1M window it wastes 150k).
- `compact::plan_cut(entry_tokens, keep_recent_tokens)` — keeps the newest ~20k tokens verbatim and
  summarizes only the head, always retaining at least the newest entry.

### Retry Policy (`le-vibe-agent`)

One schedule for both retry paths: `retry_delay_ms(base, max, attempt) = min(base * 2^(attempt-1), max)`
with pi's defaults `base = 2000ms`, `max = 60000ms`, `max_retries = 3`. A provider `Retry-After`
still overrides the schedule. A retried attempt no longer replays the partial text it replaces
(`last_partial_text` is cleared before the sleep), which previously could duplicate half a streamed
answer into the transcript.

## Handover & Branching Commands

- `/handover [goal]` — summarizes the current session into a fresh session seeded with the handoff summary.
- `/tree` — interactive DAG modal to branch from any entry.
- `/fork [id]` — forks an entry into a new session file.
- `/clone` — duplicates the active branch into a new session file.
- `/resume` — interactive picker of saved project sessions.
- `/new`, `/name [title]`, `/session`, `/import <path>`, `/export [md|html] [path]`.

## Mid-Session Model Switching

`sanitize_messages_for_provider` strips provider-specific `thought_signature` when leaving `agy`, clears foreign `reasoning_content` when leaving `deepseek`, and synthesizes `[interrupted]` tool results for incomplete tool-call turns.
