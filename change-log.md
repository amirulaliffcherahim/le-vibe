# le-vibe — Change Log & Session Timeline

Chronological timeline of milestones on `master` (`https://github.com/amirulaliffcherahim/le-vibe.git`).

---

## 2026-11-21 — v0.2.0-beta.1: Production Rust TUI, Hybrid RAG Memory v2, Layered /config, MCP & Self-Optimization

- **Rust Hybrid RAG Memory v2 (`crates/le-vibe-tools/src/memory.rs`)**: Pure-Rust Lucene-IDF BM25 + 512-dim scaled `i8` cosine vector cache (`rag-cache.bin`) + RRF, partitioned by git root (`~/.le-vibe/projects/<hash>/memory/`) with zero git noise, dual-emit `camelCase`/`snake_case` tokenizer, `keywords` write-time expansion, `key` upsert, `memory_delete`, daily recency decay, adaptive inlining, session-pinned `<memory_toc>`, pre-compaction memory flush, and `/memory`.
- **Modularized Rust Workspace & Production TUI**: Split `le-vibe-tui` into 10 focused modules and `le-vibe-cli` into 6 modules; added Markdown renderer, dirty-driven redraw loop, 16ms input burst draining, collapsible thinking (`Ctrl+T`) & tool (`Ctrl+O`) cards, live Todo strip, `@file` fuzzy autocomplete, default `Ctrl+V`/drag-and-drop image & file paste with native multimodal forwarding (`vibe` `image_url`, `agy` `inlineData`), and `TestBackend` render tests (`40x12` & `140x40`).
- **Instant `Esc` Cancel, Retry, Steering & Mid-Session Switching**: Background stdout reader thread in `le-vibe-pybridge` aborts in-flight streams within 25ms and replays `yolo` + `granted_rules` on respawn; added 429/5xx exponential backoff retry, prompt queueing while busy, cross-provider message sanitization, and OpenCode-style edge-triggered mode transitions (`normal`, `plan`, `ask`, `rea`, `cyber`).
- **Layered `/config` & New Slash Commands**: Upgraded `/config` to 16 searchable settings across `user`/`project`/`session` scopes; added `/policy`, `/rewind` (user-input checkpoints), `/rename`, `/status`, `/context`, `/clone`, `/handover`, `/export` (`.md`/`.html`), `/report` (bug report PDF), `/loop`, `/plugins`, `/reload-plugins` (custom plugin `/` commands), `/suggestion` (session-driven self-optimization), `/copy`, `/help`, `/trust`, `/changelog`, and `/mcp`.
- **Native MCP, CLI Parity, Examples & Docs**: Wired `le-vibe-mcp` (`McpManager` + `mcp.json`); added `--help`, `--version`, `-p`, piped stdin, `@file`, `--no-session`, `--fork`, `--tools`, `--export`; added `examples/example.md` + 3 Python extensions + 3 JSON policies; added 7 `docs/*.md` guides; added compile-time-gated dev debugger (`F12` + `~/.le-vibe/logs/`, stripped in `--no-default-features` official releases).

---

## 2026-11-20 — Permissions, YOLO, Todo, Ask-User, /config & Main Branch Cutover

- **`1c88849f7`** — Added `/config` interactive settings modal (Theme, Terminal Shell, Agent Mode, YOLO, Thinking Level, Reduced Motion) persisted in `~/.le-vibe/settings.json`.
- **`c2f9247b0`** — Defaulted `/model` picker to **Scoped Models** (`Tab` toggles All Models).
- **`8766ef1f2`** — Fixed Antigravity `thoughtSignature` replay and merged parallel `tool` responses into a single `user` turn; switched public repo to sole `main` branch (removed `pi`, `vibe`, `master` from `origin`).
- **`9687715b6` .. `02f3b8aa5`** — Added Vibe-style `PermissionStore` approval modal (`Allow once` / `Allow for session` / `YOLO` / `Deny`), `/yolo` & `--yolo`, `todo` tool, `ask_user_question` interactive modal, and self-repo build/test context in the system prompt.

## 2026-11-20 — TUI Polish, Modes, Themes & Shell Awareness

- **`ad6195a5f`** — Added full-app `ThemePalette` (`owl-cyan`, `nord`, `tokyo-night`, `catppuccin`, `gruvbox`, `dracula`) affecting the sleepy owl logo, shaded cards, editor, modals, and 3-line `pi-footer`, persisted in `~/.le-vibe/settings.json`.
- **`5969db953` & `adfb7842e`** — Added `/term` shell selector (`powershell`, `pwsh`, `bash`, `cmd`, `zsh`, `sh`, `other`) persisted in `~/.le-vibe/settings.json`, and injected active OS + shell into the agent system prompt so the AI writes compatible commands.
- **`10406d22a` & `74905c52b`** — Replaced wireframe `╭───╮` boxes with Pi-style dark-tinted shaded cards (`▎` accent bar + breathing room) for `thinking`, `tool`, and `script` blocks (keeping `assistant` prose unboxed), plus 30fps eased downward scrolling.
- **`9f6dd34f9` & `4bd994ef5`** — Added `!` (context-aware) and `!!` (no-context) shell commands and tool output previews in `AgentEvent::ToolCallEnd`.
- **`2457d12fd` & `899d9d66b`** — Merged `/scoped-models` into `/model` (`Tab` toggles All/Scoped, `Space` pins/unpins, `Ctrl+S` saves to `~/.le-vibe/scoped-models.json`).
- **`3cfaa0a1e`, `3c2c4202e`, `441875b43`** — Moved working status into a dedicated strip above the untouched editor box; added lowercase mode pill + active provider mini quota bar (`agy` 5h `%`, `vibe` remaining `%`, `deepseek` balance) in footer line 3.
- **`6cec4177b`** — Added 5 agent modes (`normal`, `plan`, `execute`, `ask`, `rea`) via `Shift+Tab` / `/mode`, and `medium`-first default thinking (`high` fallback when `medium` is absent).
- **`8c795aa47` & `fbe968679`** — Ported `@henryqw/pi-footer` (3-line layout: repo/branch/git-porcelain-v2 badges, colored token/cache/TPS/cost/context stats, 256-color thinking indicator, `◷` active runtime timer) and hid hardware cursor while spinning/in modals.
- **`013c25b9d`** — Moved `PythonBridge`, `SessionStore`, and LLM/catalog/usage/login calls to a background worker thread (`mpsc::channel`) with animated spinner, floating `/usage` modal, minimum terminal size guard (`40x12`), and panic terminal restore.

## 2026-11-20 — Live Provider Catalogs, Quotas & Antigravity 1.4.2

- **`0dff08001`** — Added Mistral `API Budget` alongside `Vibe Budget` in `/usage`.
- **`6ca161f66` & `6b90a7970`** — Updated Antigravity `User-Agent` to `antigravity/cli/1.4.2` (unlocking `Gemini 4 Argon` and `Gemini 4 Neon`), wired `POST /v1internal:retrieveUserQuotaSummary` for full Weekly + 5-hour + Argon quota windows, folded `-xhigh`/`-tiered-xhigh` variants, and skipped `*-disabled` entries.
- **`a1035bd17` & `8aed77b28`** — Tracked per-endpoint routing, `quotaInfo.remainingFraction <= 0` exhaustion state, catalog freshness (`LE_VIBE_CATALOG_REFRESH_S`, 30m default auto-refresh), and `paidTier` (`Google AI Pro (g1-pro-tier)`) in `/usage`.

## 2026-11-19 — Pure Live Discovery & Base-Model Grouping

- **`39e405ab6` & `6a75f035e`** — Kept all distinct base model IDs across all Antigravity endpoints and humanized bare model IDs.
- **`e926464bd` & `35429dc54`** — Removed all hardcoded model lists (`list_models()` driven 100% by connected provider APIs + `~/.le-vibe/models-cache.json`), grouped thinking suffixes into `thinking_level_map`, and added live `/usage` (`le-vibe usage`).

## 2026-11-19 — Rust Pi Harness Cutover & Unified Python Engine

- **`58e53b3a1` & `ea58bce7f`** — Built full-screen `ratatui` TUI with sleepy owl banner, `/` command popup, `/model`, `/provider`, `/login`+`/auth`, `/logout`, `/usage`, `/reload`, `/theme`; added keyring/OIDC/workspace header support for `vibe` and `loadCodeAssist` + `fetchAvailableModels` for `agy`.
- **`3f3300678` & `8b1899924`** — Replaced initial stdout REPL with `ratatui` + `crossterm` TUI.
- **`294fa36df`** — Completed Phase 5.5–5.7: added `le-vibe-mcp` (stdio JSON-RPC 2.0 MCP client), persistent `~/.le-vibe/sessions/` + `--continue` + `--resume` + `--mode rpc`, and removed all legacy TypeScript/Node `packages/` and `scripts/`.
- **`276c81c34`** — Completed Phase 5.1–5.4: streaming NDJSON `PythonBridge`, native Rust tools (`read`, `write`, `edit`, `grep`, `ls`, `bash`, `powershell`), JSONL DAG + compaction in `le-vibe-durable`, and `AgentEvent` turn loop.
- **`cfacaa08c` .. `e6b577e15`** — Added Google Antigravity (`agy`) OAuth 2.0 PKCE provider and global `le-vibe.exe` install.
- **`4f39d2d97` & `96e67f47d`** — Added Mistral OIDC PKCE browser login and `~/.le-vibe/auth.json` credential persistence.
- **`5f56f6d59`** — Scaffolded initial 8-crate Rust workspace + unified Python `providers` (`vibe` + `deepseek`), `policy`, `extensions`, and stdio JSON-RPC `bridge.py`.
- **`3c88b633b`** — Removed non-Pi/le-vibe apps (`apps/p2`, `p2-mac`, `p3`).
- **`b0df348f6` & `7c642883b`** — Initialized `le-vibe` repository (`master`, `pi`, `vibe` branches), stripped 42 legacy TS providers to `deepseek` only, added `.github/workflows/sync-upstream.yml` (2-day upstream sync for `pi` and `vibe`), and authored `goal.md` and `plan.md`.
