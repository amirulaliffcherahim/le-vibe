# le-vibe — Change Log & Session Timeline

Chronological timeline of milestones on `master` (`https://github.com/amirulaliffcherahim/le-vibe.git`).

---

## 2026-11-21 — v0.4.0: Codemode, Lifecycle Hooks, Scoped Policy Rules, Subagents, Global Skills & Mistral Vibe UX Guards

- **Pi-Style `codemode` Setting & Runner (`crates/le-vibe-tools/src/codemode.rs`, `/config`)**: Toggle `off` / `on` / `only` in `/config`; lets the model run policy- and hook-checked Python scripts calling `tools.<name>(**kwargs)` (`read`, `grep`, `find`, `ls`, `write`, `edit`, `bash`, `powershell`), `text()`, `store()`, and `load()` to collapse multi-tool loops into 1 turn.
- **Declarative Lifecycle Hooks (`crates/le-vibe-tools/src/hooks.rs`, `crates/le-vibe-agent/src/lib.rs`)**: `PreToolUse`, `PostToolUse`, and `Stop` hooks loaded from `~/.le-vibe/hooks.json` and `.le-vibe/hooks.json` with pipe-separated tool matchers.
- **Scoped Policy Patterns & Safety Guards (`python/le_vibe/policy/__init__.py`)**:
  - `allow_patterns` & `deny_patterns` (e.g. `bash(cargo test*)`, `write(src/**)`, `bash(*--force*)`) with Windows backslash normalization (`deny_patterns` enforced even in `yolo`).
  - Native `require_read_before_edit` (blocks blind `edit` on unread files) and `guard_dirty_repo` (blocks destructive git commands when `git status` is dirty).
- **Isolated `spawn_agent` Subagent Tool & `/skill-save`**: Read-only isolated child turn loop (max 12 turns) keeping main context clean; `/skill-save <name>` distills solved workflows into `.le-vibe/skills/<name>/SKILL.md` (`agentskills.io`).
- **Global Skill Discovery & Ancestor `AGENTS.md` Walk**: Scans `.le-vibe/skills`, `.pi/skills`, `~/.le-vibe/skills`, `~/.pi/agent/skills`, and `~/.agents/skills`; walks `cwd` up to `.git` root for `AGENTS.override.md` / `AGENTS.md` / `CLAUDE.md`.
- **New Extensions, Prompt Templates, Installers & TUI Polish**:
  - Added `examples/extensions/{advise.py, security_guidance.py}`, `examples/prompts/{deslop.md, review.md, commit-push-pr.md}`, and `scripts/{install.ps1, install.sh}`.
  - Added `Alt+Q` steering/follow-up retraction, 400ms `ApprovalModal` typing grace period, and long-paste collapse pills (`[pasted #N: M lines]`).

---

## 2026-11-21 — v0.3.1: Windows ConPTY Mouse-Wheel Scrolling Fix, SGR Escape Guard & Right-Edge Scrollbar

- **Windows ConPTY Mouse-Wheel Fix (`crates/le-vibe-tui/src/state.rs`)**: Adopted Mistral Vibe's Win32 `CONIN$` fix — clears `ENABLE_VIRTUAL_TERMINAL_INPUT` (`0x0200`) before enabling `EnableMouseCapture` + `\x1b[?1006h`, re-verifies on every `poll_tui_action` tick (`keep_vt_input_off`) in case child `bash.exe`/`powershell.exe` tools flip it back on, and restores cleanly on exit/suspend. Eliminates `[<35;x;yM` mouse escapes leaking into keyboard input.
- **Belt-and-Suspenders SGR Key-Burst Filter & Full Modal Wheel Scrolling (`crates/le-vibe-tui/src/input.rs`)**: Swallows any leaked `\x1b[<Cb;Cx;CyM/m` key burst silently, maps wheel buttons `64`/`65` to `ScrollUp`/`ScrollDown`, and routes wheel scrolling to both text modals and picker modals (`/model`, `/theme`, `/config`, `/tree`, `/sessions`).
- **Right-Edge Scrollbar Thumb (`crates/le-vibe-tui/src/transcript.rs`)**: Draws a subtle themed scrollbar (`┃`/`│`) whenever the transcript exceeds viewport height.

---

## 2026-11-21 — v0.3.0: Full Pi-Grade TUI, DAG `/tree`, `le-vibe doctor`, Cross-Session BM25 `/search`, SSE Watchdog & `/hotkeys`

Includes everything from `v0.3.0-beta.1` and `v0.3.0-beta.2`, plus Phase B:
- **`/tree` Pure-Rust DAG Branch Navigator (`worker.rs`, `commands.rs`, `main.rs`)**: Interactive tree view of all user, assistant, and summary nodes in `SessionStore` with `★` active leaf indicator; jump cleanly to any branch without losing abandoned branches.
- **`APPEND_SYSTEM.md`, `AGENTS.override.md` & Startup Loaded-Resources Banner (`mode.rs`, `state.rs`, `transcript.rs`)**: Append project/user instructions without replacing defaults, override `AGENTS.md` per directory, and display loaded context/templates in the scrollable startup banner.
- **Zero-Bloat Reliability & Recall (`pybridge`, `features.rs`, `cli.rs`, `main.rs`)**:
  - `le-vibe doctor` single-command environment, auth, trust, MCP, and config health check.
  - `/search <query>` pure-Rust BM25 search across all saved past sessions.
  - 90s SSE stream watchdog in `le-vibe-pybridge` to fail fast and recover from stalled streams.
- **`/hotkeys`, `~/.le-vibe/keybindings.json` & `~/.le-vibe/themes/*.json`**: Floating `/hotkeys` reference modal with custom keybinding overrides and user theme discovery.

---

## 2026-11-21 — v0.3.0-beta.2: Steering vs. Follow-up, Queue Recovery, Project Trust Gate, External Editor, Prompt Templates & Exact Word-Wrap

- **Exact Word-Wrap Visual Height (`crates/le-vibe-tui/src/transcript.rs`)**: Added `wrap_lines_exact` so every transcript line is pre-wrapped to `area.width` preserving span styles (`visual_lines.len()` matches screen rows 1:1 with zero `auto_scroll` clipping).
- **Steering vs. Follow-up & `Esc`/`Alt+Up` Editor Recovery (`le-vibe-agent`, `le-vibe-tui`, `le-vibe-cli`)**:
  - `Enter` while busy queues a steering message drained only after all `tool_call_id` results for the current assistant turn are appended, emitting `AgentEvent::SteeringInjected` at consumption time (zero ghost user blocks).
  - `Alt+Enter`, `Ctrl+Enter`, and `/followup <prompt>` queue follow-ups; `Esc` and `Alt+Up` return unconsumed steering + follow-up text back into the editor.
- **Project Trust Gate (`~/.le-vibe/trust.json`, `--approve`, `--no-approve`, `TrustModal`)**: Detects protected `.le-vibe/*` resources before loading, normalizes Windows paths, prompts interactively when unapproved, and hot-reloads via `WorkerCmd::ReloadProjectResources` on approval.
- **External Editor (`Ctrl+G`) & Quick Copy (`Ctrl+X`)**: `TerminalGuard::suspend_for` cleanly leaves raw/alt-screen/mouse/paste modes to run `$VISUAL`/`$EDITOR` (Windows `.cmd`-safe via `cmd /C`, fallback `notepad.exe`) and clears ratatui's diff buffer on return; `Ctrl+X` copies the last assistant reply.
- **Prompt Templates (`~/.le-vibe/prompts/*.md` & `.le-vibe/prompts/*.md`)**: Exposed automatically in `/` autocomplete with `$1..$9`, `$@`, and `$ARGUMENTS` expansion.
- **Multi-Width TUI Layout Golden Tests (`crates/le-vibe-tui/tests/render_tests.rs`)**: `TestBackend` assertions across `60x20`, `120x35`, and `180x50` for wrapped prose, UTF-16LE shell errors, Draw.io diagrams, steering/follow-up pills, and `TrustModal`.

---

## 2026-11-21 — v0.3.0-beta.1: Pi-Grade TUI Redesign, Curated Complementary Themes, XML/Draw.io Projection & Queue/Overflow Hardening

- **Pi-Philosophy TUI Redesign (`crates/le-vibe-tui/src/{transcript.rs, markdown.rs, modal.rs}`)**:
  - Replaced pinned top header with a scrollable startup banner for 100% viewport height; replaced `▎` card bars with clean edge-to-edge borderless tinted surfaces (`push_pi_surface`) with diff green/red tinting and quiet collapsed thinking lines.
  - Fixed viewport and horizontal overflow: exact wrapped visual line count (`div_ceil(wrap_w)` clamped to `u16::MAX`), strict status-strip width clamping with visible inline queue preview (`↳ queued (N)`), and UTF-16LE NUL/control sanitization in `le-vibe-tools` + TUI cards so Windows `bash.exe`/WSL output never breaks card right edges.
  - Eliminated cursor flicker during spinner animations via DEC mode 2026 synchronized terminal updates; converted `/policy`, `/skills`, `/plugins`, and `/diagram` to scrollable floating modals.
- **Upgraded Primary / Secondary / Complementary Theme Engine (`crates/le-vibe-tui/src/theme.rs`)**:
  - Added 2 new themes (`rose-pine` and `kanagawa`, 8 total) and upgraded all palettes with cohesive primary/secondary/complementary accents, crisp foreground/muted typography, elevated `modal_bg` surfaces, and color-coded diagram projection boxes.
- **Pure-Rust XML / Draw.io (`mxfile`) & SVG Diagram Projection (`crates/le-vibe-tui/src/diagram.rs`)**:
  - Automatic inline Unicode diagram projection for fenced `xml`/`drawio`/`svg`/`mxfile` blocks with fan-out tree grouping (`├─▶`, `└─▶`), collapsed raw XML (`Ctrl+O` to expand), and interactive `/diagram [path]` modal.
- **Scroll, Paste, `Esc` Interrupt & Queue Reliability (`crates/le-vibe-cli/src/{main.rs, worker.rs}`, `crates/le-vibe-tui/src/input.rs`)**:
  - Mouse wheel scrolls transcript/modals cleanly while `Up`/`Down` cycle prompt history (`Shift+Up/Down` & `PgUp/Dn` scroll); multi-line paste no longer auto-submits; `Esc` interrupts all slow bridge calls and visibly clears queued prompts.
  - Fixed queue freeze/double-dispatch race via `active_prompt` tracking, `cancel` reset + scroll reset before queued dispatch, queued `!`/`!!` shell support, worker disconnect recovery, and busy guards on mutating slash commands.
- **CLI Subcommand Matrix (`crates/le-vibe-cli/src/{cli.rs, main.rs, worker.rs}`, `python/le_vibe/bridge.py`)**:
  - Added `config`, `install`, `remove`, `list`, and `update` subcommands plus `-e`/`--extension`, `--no-extensions`, `--system-prompt`, and `--offline` flags; rejected unknown `/` commands locally.

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
