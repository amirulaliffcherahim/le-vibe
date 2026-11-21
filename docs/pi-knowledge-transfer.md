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
