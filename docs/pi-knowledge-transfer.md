# Pi → le-vibe Knowledge & Session Transfer Guide

How to migrate your Pi system prompt, skills, long-term memory, and past sessions into `le-vibe`'s pure-Rust Hybrid RAG Memory (`~/.le-vibe/memory/`).

## 1. System Prompt & Standards
`le-vibe` automatically loads both files on every turn (hot-reloadable via `/reload`):
- `~/.le-vibe/SYSTEM.md` — base user instructions
- `~/.le-vibe/APPEND_SYSTEM.md` — appended hard rules (copied from `~/.pi/agent/APPEND_SYSTEM.md`, with `STANDARDS.md` and `RULES_EXTRA.md` alongside)

## 2. Skills (On-Demand, ~65 tok Startup Cost)
- Copy skill folders (`<name>/SKILL.md`) into `~/.le-vibe/skills/` (or leave them in `~/.pi/agent/skills/` and `~/.agents/skills/` — `le-vibe` discovers all three).
- At startup `le-vibe` injects only a 1-line name index; full `SKILL.md` files load on demand via the built-in `load_skill(name)` tool.

## 3. Long-Term Memory (Hybrid BM25 + Cosine RAG)
- Global memory lives in **`~/.le-vibe/memory/MEMORY.md`** (indexed automatically by `MemoryRagStore` into `rag-cache.bin`).
- Project-scoped memory lives centrally in **`~/.le-vibe/projects/<git-root-hash>/memory/MEMORY.md`** (zero `git status` noise).
- Structure each durable fact as a `### [key:slug] Title #tag1 #tag2` section with a `keywords: ...` line for high-recall `memory_search` hits.
- Use `memory_write(content, key=..., keywords=[...], scope="global"|"project")` during sessions to upsert new facts, or `/memory` to inspect stored memories.

## 4. Searching Past Sessions
- Run `/search <query>` inside `le-vibe` to run pure-Rust BM25 search across all saved JSONL sessions in `~/.le-vibe/sessions/` and `~/.le-vibe/projects/*/sessions/`.
- To index older Pi JSONL sessions too, copy desired `.jsonl` files into `~/.le-vibe/sessions/`.

## 5. TUI Parity with Pi (P0–P2, complete)

All user-visible strings, layout rules and colours live in `le-vibe-tui` (a strict leaf crate).
`le-vibe-agent` forwards plain values (`args_json`, `duration_ms`, `Retry`) and formats nothing;
`crates/le-vibe-cli/src/main.rs` only wires state. The read-only Pi oracle sits in the git-ignored
`reference/pi-src`.

### P0 — tool cards, status strip, footer
| Item | Rule |
| --- | --- |
| Tool header | `read <path>:<start>-<end>`, `<shell> <command> (timeout Ns)`, `<name> key=json ...` |
| Duration | `4.2s`, then `1m 30s`, then `2h 3m 4s` |
| Preview budget | 10 lines (40 expanded); shell tools keep the **last** 5 and say `N earlier lines`, others keep the **first** and say `N more lines` |
| Compact read | Collapsed `SKILL.md` → `[skill] <name> (ctrl+o to expand)` with no body; `AGENTS.md`/`CLAUDE.md` → `read resource <path>` |
| Status kinds | `Working` / `Retrying (a/max) in Ns... (esc to cancel)` with a live countdown / `Compacting context...` / `Auto-compacting...` / `Context overflow detected, Auto-compacting...` / `Summarizing branch...`; only retries use the warning colour |
| Footer | `↑ ↓ R<cacheRead> W<cacheWrite> ↺hit% ⚡t/s $cost ◔ctx`; zero-valued counters omitted; `◔ ?/1.0M (auto)` until the next turn reports a prompt size |
| Height stability | The status row is always allocated, so idle → busy never reflows the editor or footer |

### P1 — user turns, generic args, edit preview
| Item | Rule |
| --- | --- |
| User turns | Rendered through `markdown.rs` inside the `userMessageBg` surface; ordered markers and backslash escapes stay literal |
| Generic args | Collapsed `name key=JSON ...` cut to 100 chars; expanded puts each `key: value` on its own two-space-indented header row |
| Edit preview | Pre-execution two-row (`-` then `+`) word diff with only the changed spans reversed; common indentation is excluded from the comparison |

### P2 — zones, key hints, interleaving
| Item | Rule |
| --- | --- |
| Key hints | Every on-screen hint derives from `keyhint::KeyAction`; nothing is typed twice. Actions: `ctrl+o` expand, `ctrl+t` thinking, `Esc` interrupt, `Alt+Enter` queue, `End` scroll, `Shift+Tab` mode, `Alt+W` workflows, `/` commands, `@` files, `ctrl+c` quit |
| Interleaving | `TuiState::push_stream_delta` merges into the newest run only, so `thinking → text → thinking` keeps arrival order; the renderer suppresses the blank line inside one interleaved run (the neighbour kind is part of the render cache key) |
| OSC 133 | `osc133.rs` holds byte-exact `A`/`B`/`C` marks plus `prompt_zone_for()` (assistant turns followed by tool calls open no zone). Not wired into the render loop — see deviations |

## 6. Agent Loop & Compaction Parity (P0–P1, complete)

All context-lifecycle logic lives in `le-vibe-agent`; `main.rs`/`worker.rs` only pass plain values
(`context_window`, `reserve_tokens`, retry budget) and render the events they receive.

| Item | Rule |
| --- | --- |
| Compaction trigger | `tokens > context_window - reserve_tokens`, reserve `16_384` — a token reserve, never a percentage. Capped at 2 per prompt. |
| Retention | `plan_cut` keeps the newest ~`20_000` tokens verbatim, summarizes the head, always keeps at least the newest entry. |
| Overflow detection | ~29 provider error wordings (case-insensitive), silent overflow (successful reply with prompt tokens over the window), and zero-output `length` stops. Rate-limit/throttling messages are excluded. |
| Overflow recovery | Compact once → retry the turn once (`overflow_recovery_attempted` latch, per prompt). Second overflow hard-fails with pi's text: *“Context overflow recovery failed after one compact-and-retry attempt. Try reducing context or switching to a larger-context model.”* |
| Retry schedule | `min(base * 2^(attempt-1), max)` with `base = 2000ms`, `max = 60000ms`, `max_retries = 3`; a provider `Retry-After` wins. |
| Failed-attempt handling | The partial stream of a failed attempt is cleared before the retry, so it cannot be replayed twice (pi durably omits the attempt). |
| Summarizer | `compact::summarize` (in `le-vibe-agent`) with the same retry policy; used by the loop, `/compact`, `/handover`, and bug reports. |
| Events | `AgentEvent::Compaction { reason, summarized, kept, before_tokens }` → CLI maps `reason` onto `CompactionReason`; `TurnStart` clears the status strip. |
| Verification gate | `le-vibe-agent::verification::VerificationGate` refuses a stop after an unverified edit (one nudge per prompt, then latches). pi has no equivalent — it relies on the Proof Loop prompt text. `settings.json` → `verify_edits` (default on) disables it. |

## 7. Sessions Parity (P0–P1, complete)

| Item | Rule |
| --- | --- |
| Compaction boundary | `first_kept_entry_id` + `tokens_before` on the compaction entry; `context_entries()` = pi's `buildContextEntries` (summary once, then the kept window, then everything after). |
| Branch summaries | `branch_with_summary(from_id, summary)` stores pi's `fromId` (the abandoned leaf); rewind summarizes the abandoned path, tree navigation does not. |
| Labels | `EntryKind::Label` with `target_id`; last write wins, empty content clears. Labels never move the active leaf. |
| Tree | `tree_nodes()` (nested, active flags, state-only entries re-parented) and `tree_rows()` (connectors + labels) live in `le-vibe-durable`; the CLI renders. |
| Fork | `/fork` and `/clone` both route to `fork_from`, writing a new session file and leaving the original alone. |
| Legacy files | The three new fields are `Option` with `serde(default)`; pre-P0 JSONL loads unchanged and normal entries serialise without them. |

## 8. Tools & MCP Parity (P0–P1, complete)

| Item | Rule |
| --- | --- |
| Truncation | `le-vibe-tools::truncate` — 2000 lines / 50 KB, first limit hit wins, `TruncationResult { truncated_by, total_lines, output_lines, first_line_exceeds_limit, last_line_partial }`. `read` truncates the head (never a partial line); shells truncate the tail (may clip the final line) and spill the full output to a temp file named in the notice. |
| `read` | Offset past EOF is an **error**, not an empty result. Truncation emits `[Showing lines A-B of T. Use offset=N to continue.]`; a single over-limit line points at `sed -n 'Np' <path>`. |
| Shell exit codes | A non-zero exit is a **tool error** whose body still starts with the verbatim `exit_code=N` line, so the UI renders an error card while `VerificationGate` keeps parsing `exit_code=0` as the only success signal. |
| Shell timeout | Kills the whole **process tree** (`taskkill /T /F`, `kill -TERM -pid`), not just the shell. |
| MCP timeout | Every request has a deadline — 60 s default, `mcpServers[].timeout` in seconds. A dedicated reader thread feeds a channel so a wedged server returns `mcp request timed out after Ns` instead of freezing the session. |
| MCP tool errors | `result.isError: true` surfaces as `mcp tool error: …` rather than looking like normal output. |
| Shell cancel | `Esc` sets the turn's `AtomicBool`, which `run_shell` polls every 50 ms: it kills the process tree and returns `Command aborted`. |
| Edit input | `edits` is accepted as an array, a JSON **string**, a single object, or the legacy `oldText`/`newText` pair — and a mix of legacy + array merges. Real errors (missing / non-unique `oldText`) still fail. |
| Mutation queue | `write` and `edit` share a per-canonical-path mutex, so two mutations to one file cannot interleave; different files stay parallel. |
| Shell wall time | `wall_time_seconds=N` rides on its own trailing line (pi uses `structuredContent`); `exit_code=N` stays a standalone first line so `VerificationGate` keeps parsing it. |
| MCP config | `enabled` (default true, disabled servers report `disabled (enabled: false)` and are never spawned) and `description` are honoured. |
| MCP progress | Any notification received restarts the request deadline — a slow job that reports progress is not a timeout (pi: “Progress notifications reset it”). |

### Intentional deviations
- **OSC 133 is dormant.** Pi renders inline, so its prompt zones sit in scrollback. `le-vibe` draws an
alternate buffer, which has no scrollback, so the marks would be invisible. Writing escape bytes into
cell symbols would corrupt `unicode-width` math and the `ResyncBackend` diff path, so the sequences
are exposed as a tested capability (`LE_VIBE_OSC133=1` gate ready) and wiring them stays deferred
until an inline renderer exists.
- **Generic arg order is alphabetical.** `serde_json` sorts object keys, so pairs come out
`amount=5 private=true title="Thanks!"` rather than pi's model-argument order. Fixing it needs the
workspace-wide `preserve_order` feature; deterministic ordering is the better trade.
- **OSC 133 + tool calls.** Pi skips the zone when a message has tool calls; `prompt_zone_for()`
encodes that rule so a future inline renderer inherits it for free.
- **"Recoverable length" needs a configured output cap.** Pi compares `usage.output` against the
model's `maxTokens`; `le-vibe` has no per-model output cap in the agent, so only the unambiguous
signature (a `length` stop with **zero** output) triggers overflow recovery. A genuinely truncated
answer keeps the existing truncated-tool-call guidance instead.
- **`active_branch()` still short-circuits at compaction.** Pi has one walk; `le-vibe` keeps two so
  existing UI listings and resume previews don't suddenly grow. `context_entries()` is the pi-exact
  compaction-aware walk and is what the agent sends to the model. Sessions P0 removed the earlier
  re-append workaround — the boundary now lives on the compaction entry as `first_kept_entry_id`.
- **Tree flattening lives in `le-vibe-durable`, not the UI.** Pi's `tree-selector.ts` builds the
  connector rows in the component; `le-vibe` has no session-aware UI crate, so `tree_rows()` owns the
  projection and the CLI only decorates it. It keeps ~86 lines of graph walking out of `worker.rs`.
- **The verification gate is le-vibe-only.** pi enforces "prove it works" through prose in the Proof
  Loop prompt; a long turn erodes that, so `le-vibe` checks it in Rust. It nudges once and then gives
  up rather than looping, and it reads the shell `exit_code=` because a failing command is still a
  successful tool call.
- **Label clearing is an empty label entry.** Pi stores `label: undefined` in JSON; `le-vibe` stores
  an empty `content` string, because `SessionEntry.content` is non-optional. Same last-write-wins
  resolution.
- **`/compact`, `/handover`, bug reports summarize everything.** They pass `keep_recent_tokens = 0`,
so manual compaction keeps the previous whole-branch behaviour; only the automatic triggers retain a
verbatim tail.
