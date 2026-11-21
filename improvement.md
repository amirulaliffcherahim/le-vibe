# le-vibe — Hardened Improvement Roadmap (Post v0.3.0-beta.1)

Cross-referenced against `goal.md`, `earendil-works/pi`, `mistralai/mistral-vibe`, `anthropics/claude-code`, and reviewed for Windows + Rust/Python boundary correctness.

**Non-negotiable boundary**: Keep `python/le_vibe/` strictly limited to `providers`, `policy`, `extensions`, and `bridge.py`. All TUI, agent loop, session DAG, trust, templates, and CLI work stays in the 9-crate Rust workspace.

---

## Phase A — `v0.3.0-beta.2` (Reliability, Daily UX & Trust Boundary) — COMPLETED

- [x] **A0**: Exact word-wrap visual height (`wrap_lines_exact` in `transcript.rs`).
- [x] **A1**: Steering (`Enter`) vs. Follow-up (`Alt+Enter` / `Ctrl+Enter` / `/followup`) + `Esc`/`Alt+Up` editor recovery.
- [x] **A2**: Project Trust Gate (`~/.le-vibe/trust.json`, `--approve` / `--no-approve`, `TrustModal`, `ReloadProjectResources`).
- [x] **A3**: External Editor (`Ctrl+G` via `TerminalGuard::suspend_for`) & Quick Copy (`Ctrl+X`).
- [x] **A4**: Prompt Templates (`~/.le-vibe/prompts/*.md` & `.le-vibe/prompts/*.md` with `$1..$9`, `$@`, `$ARGUMENTS`).
- [x] **A5**: Multi-Width TUI Layout Golden Tests (`60x20`, `120x35`, `180x50` in `render_tests.rs`).

### A0. Fix Word-Wrap Scroll Undercount (`crates/le-vibe-tui/src/{transcript.rs, markdown.rs}`)
- **Why**: `l.width().div_ceil(wrap_w)` undercounts word-wrapped prose compared to `Paragraph::wrap`, causing `auto_scroll` to stop short and clip bottom lines.
- **Fix**: Pre-wrap prose/user/system lines to `area.width` during line construction (matching `push_pi_surface` and `push_full_width_line`), so `lines.len()` equals exact visual height 1:1 with zero wrap estimation error.

### A1. Steering (`Enter`) vs. Follow-up (`Alt+Enter` / `Ctrl+Enter` / `/followup`) + `Esc`/`Alt+Up` Queue Recovery
- **Tool-batch safety**: In `le-vibe-agent`, drain the shared `Arc<Mutex<VecDeque<String>>>` steering queue **only after all tool calls in the current assistant message finish and all `tool_call_id` results are appended**, right before the next provider request (prevents HTTP 400 adjacency errors on Vibe/DeepSeek/AGY).
- **No ghost transcript blocks**: Do NOT push `TranscriptBlock::User` when queuing. Keep pending steering & follow-up items visible in `draw_status_strip`; emit `AgentEvent::SteeringInjected { text }` when the worker actually consumes an item so `main.rs` appends `TranscriptBlock::User` at consumption time.
- **Zero lost input**: `Esc` and `Alt+Up` return unconsumed steering + follow-up items back into `state.input` (joined with `\n`).
- **Windows Terminal compatibility**: Bind Follow-up to `Alt+Enter` **and** `Ctrl+Enter` (plus `/followup <msg>`), since Windows Terminal intercepts `Alt+Enter` for fullscreen by default.

### A2. Project Trust Gate (`~/.le-vibe/trust.json` + `--approve` / `--no-approve`)
- **Timing (no bypass)**: Detect project resources (`.le-vibe/{settings.json, mcp.json, policy.json, SYSTEM.md, APPEND_SYSTEM.md, extensions/, prompts/}`) **before** loading them. If none exist, skip prompt.
- **Path normalization**: Canonicalize repo path (strip Windows `\\?\` prefix, normalize case) before checking `~/.le-vibe/trust.json`.
- **Safe default + hot-reload**: Start untrusted when unapproved; if approved via the interactive trust modal, persist to `~/.le-vibe/trust.json` and dispatch `WorkerCmd::ReloadProjectResources` to activate project settings, MCP, extensions, and prompts dynamically.
- **Headless/CI**: Honor `--approve` / `--no-approve` and `defaultProjectTrust` (`"ask"` | `"always"` | `"never"`).

### A3. External Editor (`Ctrl+G`) & Quick Copy (`Ctrl+X`)
- **`Ctrl+G` terminal lifecycle (Windows-safe)**:
  1. `DisableMouseCapture`, `DisableBracketedPaste`, `LeaveAlternateScreen`, `disable_raw_mode()`.
  2. Write `state.input` to temp `.md` file; split `$VISUAL`/`$EDITOR` into program + args (on Windows run via `cmd /C` for `.cmd`/`.bat` shims like `code --wait`; fallback `notepad.exe`).
  3. On editor exit: `enable_raw_mode()`, `EnterAlternateScreen`, `EnableBracketedPaste`, `EnableMouseCapture`, and **`terminal.clear()`** (prevents stale ratatui diff cells).
- **`Ctrl+X`**: Copies the last `TranscriptBlock::Assistant` directly to clipboard (reusing `/copy`).

### A4. Prompt Templates (`~/.le-vibe/prompts/*.md` & `.le-vibe/prompts/*.md`)
- Pure-Rust loader in `crates/le-vibe-cli/src/features.rs` scanning user & trusted-project `prompts/` directories.
- Exposed automatically in `/` autocomplete alongside built-in and plugin commands; expands `$1..$9`, `$@`, and `$ARGUMENTS` before submission.

### A5. Multi-Width TUI Layout Golden Tests (`crates/le-vibe-tui/tests/render_tests.rs`)
- Deterministic `TestBackend` tests across `60x20`, `120x35`, and `180x50` covering long wrapped prose, UTF-16LE shell errors, steering/follow-up pills, Draw.io diagrams, and floating modals — asserting exact line counts, zero clipping, and clean card padding.

---

## Phase B — `v0.3.0` (Power Features)

6. **`/tree` Branch Navigator (Deterministic Pure-Rust DAG)**:
   - Interactive DAG tree picker over `le-vibe-durable::SessionStore` (user + assistant nodes).
   - Keep switching instant and zero-cost; make abandoned-branch LLM summarization an explicit opt-in action (`S` key or `/tree summarize`), never automatic on navigation.
7. **`APPEND_SYSTEM.md`, `AGENTS.override.md` & Startup Loaded-Resources Banner**:
   - Append instructions without replacing defaults (`APPEND_SYSTEM.md`), override `AGENTS.md` per directory (`AGENTS.override.md`), and list loaded context/skills/extensions/prompts in the scrollable startup banner.
8. **Custom Keybindings (`~/.le-vibe/keybindings.json`) + `/hotkeys` + Custom Themes (`~/.le-vibe/themes/*.json`)**:
   - User-remappable shortcuts, `/hotkeys` modal, and drop-in JSON color palettes alongside the 8 built-in themes.
9. **Structured `--mode json` Event Stream**:
   - Unified JSONL event stream (`le-vibe-schema`) for scripts and CI pipelines.

---

## Phase C — Post-`v0.3.0` (Claude Code, Hermes & Distribution Ergonomics — Zero Bloat)

10. **Declarative Lifecycle Hooks (`PreToolUse` / `PostToolUse` / `Stop` in `settings.json`)** *(Claude Code)*
11. **Scoped Tool Permission Patterns in `python/le_vibe/policy` (`bash(cargo *)`, `write(src/**)`)** *(Claude Code + Mistral Vibe)*
12. **Closed Learning Loop — Zero-Bloat Subset** *(Hermes Agent)*:
    - **Cross-session BM25 search (`session_search` tool / `/search <query>`)** reusing our existing pure-Rust BM25 scorer over `~/.le-vibe/projects/*/sessions/*.jsonl` (0 new deps).
    - **Skill distillation (`skill_write` / `/skill-save`)** saving proven multi-step procedures to `.le-vibe/skills/<name>/SKILL.md` (`agentskills.io` standard).
    - **`le-vibe doctor`** single-command health check (Python/uv, provider auth, MCP, `trust.json`, `settings.json`, `models-cache.json`).
    - **SSE Stream Watchdog** in `le-vibe-pybridge` (fail fast on stalled streams instead of hanging forever).
    - *Explicitly skipped from Hermes (bloat)*: messaging gateway daemons (Telegram/Discord/WhatsApp), 6 cloud VM backends, desktop app, voice transcription, Honcho cloud server.
13. **Isolated `spawn_agent` Subagent Tool + One-Liner Installers (`install.ps1` / `install.sh`)**
