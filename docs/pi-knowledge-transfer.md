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
