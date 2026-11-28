# Sessions, Context & Handover

## Storage & DAG Format (`crates/lazybug-durable/src/lib.rs`)

Sessions are stored per-project under `~/.lazybug/sessions/<cwd-slug>/session-<ts>.jsonl`.
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
- **Auto-compaction**: run by `lazybug-agent` **before** a request when the last prompt used more
  than `context_window - reserve_tokens` (default reserve `16_384`). There is no percentage knob: the
  `auto_compact` setting just passes `context_window: 0` to disable it.
- **`/context`**: displays estimated token breakdown (system + `AGENTS.md` + `<skills>`, active branch messages, and utilization %).

### Context Lifecycle (`lazybug-agent`) — live

All three triggers live in the agent loop; the CLI only forwards `context_window` and renders the
`AgentEvent::Compaction` it receives. Covered by `crates/lazybug-agent/tests/context_lifecycle.rs`
(pure rules) and `tests/compaction_loop.rs` (loop behaviour, with a scripted provider).

- **Threshold** (`compact::should_compact`) — pi's rule is a **token reserve**, not a percentage:
  `tokens > window - 16384`. A percentage is wrong at both ends (on a 32k window 85% leaves 4.8k,
  less than one large tool result; on a 1M window it wastes 150k). Capped at 2 compactions per
  prompt so a still-oversized summary cannot loop.
- **Overflow** (`overflow::is_context_overflow_text`) — provider error patterns (case-insensitive,
  ~29 wordings) with rate-limit/throttling exclusions so a Bedrock `ThrottlingException: Too many
  tokens` is never mistaken for overflow. Compact once, retry the turn once; a second overflow on
  the same prompt hard-fails with pi's exact message.
- **Silent overflow / recoverable length** — a *successful* reply whose prompt tokens already exceed
  the window, or a `length` stop with **no output** (the input filled the window). These are checked
  before the empty-response guard, since such a reply has no content.
- **Retention** (`compact::plan_cut`) — keeps the newest ~20k tokens verbatim and summarizes only the
  head, always retaining at least the newest entry. The boundary is stored on the compaction entry as
  `first_kept_entry_id` (+ `tokens_before`), so `SessionStore::context_entries()` restores the tail in
  its original position and **nothing is duplicated** in the JSONL file.

### Reading a session: `active_branch` vs `context_entries`

| Method | Behaviour |
| --- | --- |
| `active_branch()` | Leaf→root, stops at the newest `compaction_summary`. Kept for compatibility (UI listings, resume previews). |
| `context_entries()` | pi's `buildContextEntries`: emits the newest compaction once, then the entries from `first_kept_entry_id` onward (pre-boundary `role == "system"` messages dropped), then everything after it. This is what the agent sends to the model. |

### Session tree, labels, and branching (Sessions P0–P1)

All DAG/tree/label logic lives in `lazybug-durable`; the CLI only renders rows.

| API | Behaviour |
| --- | --- |
| `tree_nodes()` | Nested `SessionTreeNode { entry, children, label, active }`. State-only entries (`tool_call`, `tool_result`, `model_change`, `label`, …) are skipped and their children re-parented to the nearest visible ancestor, so a tool round-trip never creates a dangling branch. `active` marks the leaf's ancestry. |
| `tree_rows()` | The DAG→list projection: `├─`/`└─` connectors, `*` for the active path, resolved labels (`· label`), 64-char preview. `worker.rs::ListTreeNodes` is now a 13-line call to it (was ~99 lines of graph walking). |
| `set_label(id, Some("x"))` | Appends an `EntryKind::Label` entry with `target_id`; **does not move the leaf**. `set_label(id, None)` appends an empty label, which clears it (pi's `LabelEntry { label: undefined }`). `label_for(id)` resolves last-write-wins. |
| `branch_with_summary(from_id, summary)` | Branch to `from_id` (`None` = fresh root) and record the abandoned leaf in `from_id`. |
| `fork_from(id, dest)` | New session file containing only root→id. |

#### Dynamic vs static navigation

| Command | Cost | Behaviour |
| --- | --- | --- |
| `/tree` → select | instant | `SwitchBranch`: moves the leaf, **no summarizer call**. |
| `/rewind` → select | one summarizer call | `compact::branch_summary` summarizes the abandoned path into a `branch_summary` entry, using the same retry policy as a turn; on failure it falls back to a plain switch and says so. |
| `/fork` or `/clone` → select | file copy | `fork_from` writes a new session file; the original is untouched. |

`compact::SUMMARIZE_ALL` is the explicit "summarize the whole branch" sentinel. It is **not** the
same as a branch that fits its budget (`plan_cut` keeps everything and summarizes nothing) —
conflating the two sent an empty transcript to the summarizer.
- **Summarizer** (`compact::summarize`) — moved out of the CLI so it shares one retry policy with
  normal turns; `/compact`, `/handover`, and bug-report generation all call it.

### Retry Policy (`lazybug-agent`)

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
