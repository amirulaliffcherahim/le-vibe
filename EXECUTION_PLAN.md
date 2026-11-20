# le-vibe Production TUI Execution Plan (For Smaller Coding Models)

> **Purpose:** Step-by-step implementation blueprint to bring `le-vibe` to full Pi + Vibe feature parity with a production-ready Rust TUI, runnable extension/policy examples, and complete runtime wiring.
> **Audience:** Designed to be executed sequentially (Phase 0 -> Phase 7) by a smaller coding model without guesswork.

---

## 0. Design Philosophy & Hard Guardrails (Read Before Every Phase)

### Design Philosophy (Pi + Vibe Synthesis)
- **Clear Split of Responsibility:** Rust (`crates/*`) owns all latency-sensitive work (TUI, input, native tools, durable JSONL DAG, MCP, cancellation). Python (`python/le_vibe/*`) owns provider wire protocols, permission policies, and user extensions.
- **UI Thread Never Blocks:** All network, disk scans, git status, MCP calls, and LLM turns run on worker threads. Redraws are dirty-driven (redraw on input, worker event, resize, or 80ms spinner tick only while busy — zero idle CPU burn).
- **Never Lose User Work:** Every user message, tool call, model switch, and interrupted/partial turn is persisted append-only in JSONL. Compaction and branching never delete prior history.
- **Progressive Disclosure:** Assistant prose is clean and unboxed; thinking and tool output live in subtle shaded, collapsible cards with primary arguments in the header (`read path/to/file · done`).
- **Per-Input Acceptance & Control:** Users always see what the agent is about to mutate (command or edit diff preview in approval modal), can interrupt (`Esc`) or steer/queue mid-turn, and get a clear per-turn outcome (files touched, verification status, todo progress, tokens/cost/TPS).

### Hard Guardrails
1. **DO NOT modify `python/le_vibe/providers/*`.** Provider auth, live catalog discovery, streaming, and `/usage` are already working and out of scope.
2. **One phase at a time.** Do not start Phase `N+1` until Phase `N` compiles cleanly and all verification commands pass.
3. **Zero warnings policy.** Every phase must pass:
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - For Python changes: `uv run --project python --with pytest --with pytest-asyncio --with httpx --with pyyaml --with pydantic --with keyring pytest python/tests` (or `PYTHONPATH=python pytest python/tests`)
4. **Windows-first UTF-8 safety.** Never slice Rust `&str` by byte index without `is_char_boundary` / `char_indices()`. Keep `PYTHONUTF8=1` and `PYTHONIOENCODING=utf-8` on Python subprocesses.
5. **No hardcoded model lists.** Never add static provider model arrays anywhere in Rust or Python.
6. **Keep files modular.** After Phase 0, keep each `.rs` file focused on one concern (< 500 lines preferred).
7. **Never commit secrets or user files** (`~/.le-vibe/auth.json`, `~/.le-vibe/agy_oauth.json`, `models-cache.json`).

---

## 1. Current State vs. Pi + Vibe Gap Matrix

| Area | Pi / Vibe Reference | Current `le-vibe` Status | Target Phase |
|------|---------------------|--------------------------|--------------|
| Providers (`vibe`, `deepseek`, `agy`) | Multi-provider + live quota | **Done — DO NOT TOUCH** | Locked |
| Codebase structure | Modular files per concern | `le-vibe-tui/src/lib.rs` (~2,550 lines) & `le-vibe-cli/src/main.rs` (~2,200 lines) are monoliths | Phase 0 |
| Examples & Docs | Pi `examples/`, Vibe configs | Missing `examples/example.md`, extension examples, policy examples | Phase 1 |
| Python Extensions, Skills & System Prompt | Auto-load hooks, custom tools, sectioned prompt (`~/.le-vibe/SYSTEM.md`, `AGENTS.md`, `<skills>`) | `ExtensionHost` exists, **never loaded from disk or invoked**; skills not injected; no user `SYSTEM.md` | Phase 1 |
| Python Policy Config | File-based policy rules + `/trust` | `PermissionStore` hardcoded in `__init__`, **no config file loader** | Phase 1 |
| MCP Client | Config-driven MCP servers + `/mcp` | `crates/le-vibe-mcp` exists, **not in `le-vibe-cli/Cargo.toml`** | Phase 2 |
| CLI Commands & Flags | `--help`, `--version`, piped stdin, `-p`, `-c`, `-r`, `--no-session`, `--session`, `--fork`, `--name`, `--tools`, `--list-models`, `--export` | `--help` launches TUI & spams ANSI; missing stdin pipe, `--no-session`, `--fork`, `--tools`, `--list-models`, etc. | Phase 2B |
| Cancel / Retry / Steering / Mid-Session Model & Mode Switch | `Esc` aborts stream, 429/5xx retry, queue while busy, clean cross-provider switch (`ModelChange`), OpenCode-style edge-triggered mode switch (`ModeChange` + stale reminder pruning + cache-stable base prompt) | `Esc` only clears input; no retry; input blocked while busy; `EntryKind::ModelChange` never written; mode switch mutates top-level system prompt (busts prompt cache) and leaves no transition signal | Phase 3 |
| Native Rust Tools | Regex `grep`, `find`, shell timeout, active shell, line-numbered `read`, diff preview | Plain substring `grep`, no `find`, no shell timeout, hardcoded `bash`/`powershell` | Phase 4 |
| Sessions, Context, Compaction & Handover | Per-project session dirs, LLM compaction (`keepRecentTokens`, custom instructions), auto-compact, interactive `/tree`, `/fork`, `/clone`, `/resume`, `/session`, `/name`, `/import`, `/handover`, `/context` | Flat session dir; placeholder compaction; read-only `/tree`; tool calls not persisted; no handover | Phase 5 |
| Production Rust TUI & Smoothness | Dirty-driven redraw, Markdown, collapsible cards, multiline/paste/history editor, `@file`, templates, mouse scroll, smart auto-scroll, diff preview in approval, `/copy`, `/help`, `/hotkeys`, `/trust`, `/changelog`, modal search, live Todo strip | Fixed-poll redraw, plain text assistant blocks, history-less editor, no `@file`, no mouse scroll, no modal search, no diff preview | Phase 6 |
| Automated TUI, E2E Tests & Codebase Docs | `TestBackend` render tests + narrow/wide layout checks + readable architecture/internals `.md` docs | Only 23 basic unit tests, no TUI frame render tests, no `docs/*.md` deep-dive guides | Phase 7 |

---

## Phase 0: Split Monolithic Rust Files (Pure Refactor, Zero Behavior Change)

**Why first:** Splitting the 2,500-line `lib.rs` and 2,200-line `main.rs` keeps every file small enough for fast, accurate edits in Phases 1–7.

### Exact Changes
1. **`crates/le-vibe-tui/src/`** — split `lib.rs` into:
   - `theme.rs` (`ThemePalette`, `ThemeId`, `ALL_THEMES`)
   - `mode.rs` (`AgentMode`, `ALL_MODES`, `ALL_SHELLS`, `build_system_prompt`)
   - `commands.rs` (`SlashCommandSpec`, `BUILTIN_SLASH_COMMANDS`, `filter_slash_commands`)
   - `state.rs` (`TranscriptBlock`, `ProviderUsageStats`, `ActiveModal`, `TuiState`, `TuiAction`, `FrameBuffer`)
   - `editor.rs` (`wrap_editor_lines`, `prev_char_boundary`, `next_char_boundary`, `draw_editor`)
   - `transcript.rs` (`draw_header`, `draw_transcript`, `draw_status_strip`, `draw_footer`, formatting helpers)
   - `modal.rs` (`truncate_ellipsis`, `draw_slash_popup`, `draw_modal`, `draw_approval_modal`, `draw_question_modal`, `draw_usage_modal`)
   - `input.rs` (`poll_tui_action`, `modal_len`)
   - `export.rs` (`export_session_markdown`, `export_session_html`)
   - `lib.rs` (re-export all existing public items so `le-vibe-cli` imports do not break).
2. **`crates/le-vibe-cli/src/`** — split `main.rs` into:
   - `settings.rs` (`load_settings`, `save_setting`, `load_scoped_models`, `save_scoped_models`, path helpers)
   - `worker.rs` (`WorkerCmd`, `WorkerMsg`, `spawn_worker`, `summarize_git_status`, quota formatting)
   - `rpc.rs` (`run_rpc_mode`, `run_streaming_prompt`)
   - `main.rs` (CLI arg parsing, `handle_login`/`logout`/`auth`/`usage`, `run_interactive_tui` event loop).

### Acceptance Criteria & Verification
- Zero public API breaks; all existing tests pass unchanged.
- Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

---

## Phase 1: Examples (`example.md`, Extensions, Policies) + Runtime Auto-Loading

### 1A. Create `examples/` Directory
Create the following files:
1. **`examples/example.md`**:
   - Quickstart (`le-vibe`, `--yolo`, `--print`, `--continue`, `--resume`, `--mode rpc`).
   - Modes table (`normal`, `plan`, `execute`, `ask`, `rea`) and how to customize prompts.
   - How to install & test an extension (copy to `~/.le-vibe/extensions/` or `.le-vibe/extensions/`).
   - How to install & test a policy config (copy to `~/.le-vibe/policy.json` or `.le-vibe/policy.json`).
   - How to add an MCP server (`~/.le-vibe/mcp.json` or `.le-vibe/mcp.json`).
   - How to add a `SKILL.md` (`.le-vibe/skills/<name>/SKILL.md`) and prompt template (`.le-vibe/prompts/<name>.md`).
2. **`examples/extensions/`** (3 runnable Python extensions, each exposing `def register(host: ExtensionHost) -> None`):
   - `examples/extensions/audit_logger.py` — hooks `before_tool_call` and `after_tool_call`, appends timestamped JSONL audit records to `~/.le-vibe/tool-audit.jsonl`.
   - `examples/extensions/project_stats_tool.py` — registers a custom `PythonToolSpec` named `project_stats` that counts files/lines by extension under a path (skipping `.git`, `target`, `node_modules`).
   - `examples/extensions/secret_guard.py` — hooks `before_tool_call` to return `{"blocked": True, "reason": "..."}` if a `read`/`write`/`edit` targets `.env`, `*.pem`, `*.key`, or `auth.json`.
3. **`examples/policies/`** (3 JSON policy profiles + schema explanation in `examples/example.md`):
   - `examples/policies/balanced.json` — default developer profile (`read`/`grep`/`ls`/`find`/`todo`/`ask_user_question`: `always`; `write`/`edit`/`bash`/`powershell`: `ask`; allowlisted read-only shell prefixes like `git status`, `git diff`, `cargo test *`, `pytest *`).
   - `examples/policies/strict.json` — asks on everything except `read`/`ls`/`grep`; denies `bash`/`powershell`; restricts `trusted_folders` to `["."]`.
   - `examples/policies/ci_readonly.json` — `write`/`edit`/`bash`/`powershell`: `never`; `read`/`grep`/`ls`/`find`: `always`.

### 1B. Policy File Loader (`python/le_vibe/policy/__init__.py`)
- Add `load_policy_file(path: Path)` and `PermissionStore.load_from_files(paths: Sequence[Path]) -> PermissionStore`.
- Supported JSON schema:
  ```json
  {
    "trusted_folders": ["."],
    "tool_permissions": { "write": "ask", "bash": "ask" },
    "allowed_commands": ["git status", "git diff *", "cargo test *"],
    "allowed_paths": ["docs/*"],
    "blocked_commands": ["git push --force"]
  }
  ```
- **Security invariant:** User/project policy files may ADD to `blocked_commands`, never remove hardcoded `_DANGEROUS_SHELL_PATTERNS`.
- Load `~/.le-vibe/policy.json` first, then `<cwd>/.le-vibe/policy.json` (project overrides user).

### 1C. Extension Auto-Loader (`python/le_vibe/extensions/__init__.py` & `python/le_vibe/bridge.py`)
- Add `load_extensions_from_dirs(host: ExtensionHost, dirs: Sequence[Path]) -> list[str]` using `importlib.util.spec_from_file_location`:
  - Scans `*.py` (skipping `_*.py`), imports module, calls `module.register(host)` if callable.
  - Errors in one extension must be caught and recorded (never crash the bridge).
- In `BridgeRuntime.__init__` (`python/le_vibe/bridge.py`):
  - Auto-load policy from `[~/.le-vibe/policy.json, Path.cwd() / ".le-vibe/policy.json"]`.
  - Auto-load extensions from `[~/.le-vibe/extensions, Path.cwd() / ".le-vibe/extensions"]`.
  - Add RPC method `extensions.reload` and `policy.reload` so `/reload` picks up changes live.

### 1D. Wire Extensions & Skills Into Rust Agent (`crates/le-vibe-agent`, `crates/le-vibe-pybridge`, `crates/le-vibe-tui`)
- In `crates/le-vibe-agent/src/lib.rs`:
  - Require `B: ProviderPort + PolicyPort + ExtensionPort` on `run_turn_loop*` (add default/mock impl in tests).
  - Merge `bridge.list_extension_tools()` into the `tools` vector sent to `CompletionRequest` (respecting `allowed_tools` if mode is read-only).
  - Before each tool call: invoke `bridge.invoke_hook("before_tool_call", json!({"tool": tc.name, "arguments": tc.arguments}))`. If response has `blocked == true`, skip execution and return `reason` as tool error.
  - If `tc.name` matches a Python extension tool: evaluate policy (`PolicyPort`), then run `bridge.execute_extension_tool(&tc.name, tc.arguments.clone())`.
  - After each tool call: invoke `bridge.invoke_hook("after_tool_call", json!({"tool": tc.name, "is_error": is_error, "output": preview}))`.
- In `crates/le-vibe-tui/src/mode.rs`:
  - Structure `build_system_prompt` into explicit XML sections (`preamble`, `<environment>`, `<modes>`, `<rules>`, `<self_repo>`, `<skills>`, `<user_instructions>`, `<project_instructions>`), matching Pi's `buildSystemPromptSections`:
    - Accept `skills: &[(String, String, String)]` (`(name, description, path)`) and render `<skills>` when non-empty.
    - Load optional `~/.le-vibe/SYSTEM.md` (capped at 4,000 chars) into `<user_instructions>` and `<cwd>/AGENTS.md` (capped at 4,000 chars) into `<project_instructions>`.
    - Support `/skill:<name>` invocation in the slash dispatcher (reads `SKILL.md` body and injects it into the user turn).

### Acceptance Criteria & Verification
- Unit tests in `python/tests/` for `load_policy_file`, `load_extensions_from_dirs`, and all 3 example extensions + 3 example policies.
- Rust unit test in `le-vibe-agent` proving `before_tool_call` blocking and Python extension tool dispatch.
- Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` + `pytest python/tests`.

---

## Phase 2: Wire Native Rust MCP (`crates/le-vibe-mcp` -> `le-vibe-cli` & `le-vibe-agent`)

### Exact Changes
1. **`crates/le-vibe-mcp/src/lib.rs`**:
   - Add `McpServerConfig { command: String, args: Vec<String>, env: BTreeMap<String, String> }` and `McpClient::spawn_with_env(&McpServerConfig)`.
   - Add `McpManager`:
     - Loads `~/.le-vibe/mcp.json` and `<cwd>/.le-vibe/mcp.json` (`{"mcpServers": { ... }}`).
     - Spawns + initializes each server (non-fatal on single server failure; records status `Connected(n tools)` or `Error(msg)`).
     - Exposes `tool_definitions() -> Vec<ToolDefinition>` with namespaced tool names `mcp__<server>__<tool>`.
     - Exposes `call_namespaced_tool(full_name: &str, args: Value) -> Result<String, McpError>` that flattens MCP `content` text blocks into a string.
     - Exposes `status_summary() -> Vec<(String, String)>` for `/mcp`.
2. **`crates/le-vibe-cli/Cargo.toml` & `crates/le-vibe-agent/Cargo.toml`**:
   - Add `le-vibe-mcp = { path = "../le-vibe-mcp" }`.
3. **`crates/le-vibe-agent/src/lib.rs`**:
   - Pass `mcp: Option<&mut McpManager>` (or trait `McpPort`) into `run_turn_loop_interactive`.
   - Include MCP tools in `CompletionRequest.tools` (only in modes that allow tools beyond read-only, or check policy).
   - Route `mcp__*` calls through `PolicyPort::evaluate_tool_call` (defaults to `ask` unless approved/yolo) before calling `McpManager::call_namespaced_tool`.
4. **`crates/le-vibe-tui` & `crates/le-vibe-cli`**:
   - Add `/mcp` slash command to `BUILTIN_SLASH_COMMANDS` and `WorkerCmd::McpStatus` / `WorkerCmd::Reload` (reloads MCP servers too).

### Acceptance Criteria & Verification
- Test in `le-vibe-mcp` with a mock Python stdio MCP server loaded via `McpManager::from_config_json`.
- `/mcp` displays connected servers and tool counts without blocking the TUI thread.
- Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

---

## Phase 2B: CLI Commands, Flags & Non-Interactive Piping (Pi CLI Parity)

### Exact Changes (`crates/le-vibe-cli/src/main.rs` & `cli.rs`)
1. **`--help` / `-h` and `--version` / `-V`:** Print clean usage help / version and exit `0` (fixes current bug where `--help` launches the full TUI).
2. **Complete CLI flags:**
   - `-p` / `--print <prompt>` (also accept positional prompt args after flags and `--` separator; prepend piped `stdin` when `!io::stdin().is_terminal()`).
   - `@<path>` arguments on CLI: read referenced file(s) from cwd and prepend to the first prompt.
   - `-c` / `--continue`, `-r` / `--resume [id]`, `--session <path|id>`, `--session-dir <dir>`, `--fork <id>`, `--no-session`, `-n` / `--name <name>`.
   - `--provider <name>`, `--model <provider/id[:thinking]>`, `--thinking <level>`, `--agent-mode <normal|plan|execute|ask|rea>`, `--tools <csv>`, `--list-models [search]`.
   - `--export <session.jsonl> [output.md|output.html]` (offline export without launching TUI).
   - `--mode <text|json|rpc>` (keep `--mode json` and `--mode rpc`; auto-switch to print mode when stdout is not a TTY and neither json/rpc is set).
3. **Subcommands:** `le-vibe login`, `le-vibe logout`, `le-vibe auth`, `le-vibe usage`, `le-vibe mcp list`.

### Acceptance Criteria & Verification
- `le-vibe --help` and `le-vibe --version` exit `0` with plain text (zero ANSI codes).
- Unit tests for CLI arg parsing (`-p`, `-c`, `--no-session`, `--model provider/id:high`, `--tools read,grep`).
- Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

---

## Phase 3: Turn Cancellation (`Esc`), Retry, Steering & Mid-Session Model Switching

### Exact Changes
1. **Cancellation (`crates/le-vibe-pybridge`, `crates/le-vibe-agent`, `crates/le-vibe-cli`, `crates/le-vibe-tui`)**:
   - Add `cancel: Arc<AtomicBool>` shared between UI thread and worker thread.
   - In `crates/le-vibe-tui/src/input.rs`: when `state.is_busy` and `KeyCode::Esc` is pressed (and no modal is open), return `TuiAction::Interrupt`.
   - In `crates/le-vibe-cli`: on `TuiAction::Interrupt`, set `cancel.store(true, Ordering::SeqCst)` and show status `"Interrupting..."`.
   - In `crates/le-vibe-agent`: check `cancel.load(Ordering::SeqCst)` before each turn, inside the streaming callback, and before each tool call. When cancelled, persist partial assistant text + `"[Interrupted by user]"` to `SessionStore` and return `Err(AgentError::Cancelled)`.
   - In `crates/le-vibe-pybridge`: support `cancel_token` on `stream_complete` — if triggered while waiting on Python stdout, kill & respawn the bridge cleanly on next call so a stalled network stream aborts immediately.
2. **Transient Error Retry (`crates/le-vibe-agent/src/lib.rs`)**:
   - On `BridgeError::Remote { message, .. }` containing `HTTP 429`, `HTTP 502`, `HTTP 503`, `HTTP 504`, `ReadTimeout`, or `ConnectError`, retry up to 2 times with exponential backoff (`1s`, `3s`), checking `cancel` before sleeping and emitting `AgentEvent::Status { text }` so the user sees `"Rate limited — retrying (1/2)..."`.
3. **Steering / Follow-up Queue (`crates/le-vibe-tui`, `crates/le-vibe-cli`)**:
   - Add `pub queued_prompts: VecDeque<String>` to `TuiState`.
   - Allow typing in the editor while `state.is_busy` (keep cursor visible when editor is focused). Pressing `Enter` while busy pushes to `state.queued_prompts` and shows `"Queued (N)"` in the status strip.
   - When `WorkerMsg::Idle` arrives and `state.queued_prompts` is non-empty, pop the front prompt and automatically dispatch `WorkerCmd::RunPrompt`.
4. **Clean Mid-Session Model / Provider Switching (`crates/le-vibe-durable`, `crates/le-vibe-agent`, `crates/le-vibe-cli`)**:
   - Whenever the user switches model or provider (`/model`, `/provider`, `Ctrl+P`), append an `EntryKind::ModelChange` entry (`content: "{provider}/{model}:{thinking}"`) to `SessionStore`.
   - Clamp `state.thinking_level` immediately to the new model's supported `thinking_level_map` (falling back to `medium` -> `high` -> `off`), update `state.context_window` from the new `ModelSpec`, and recalculate the footer context `%` immediately.
   - **Cross-provider message sanitization in `le-vibe-agent` before sending `CompletionRequest`:**
     - Strip provider-specific `thought_signature` when targeting a non-`agy` provider.
     - Strip or fold orphaned `reasoning_content` when switching between providers whose APIs reject foreign reasoning blocks.
     - Ensure every `assistant` message with `tool_calls` is followed by matching `tool` result messages (if a prior turn was interrupted mid-tool, synthesize a `"[interrupted]"` tool result so the new provider never returns HTTP 400 on malformed message alternation).
5. **OpenCode-Style Mid-Session Mode / Persona Switching (`crates/le-vibe-durable`, `crates/le-vibe-agent`, `crates/le-vibe-tui`, `crates/le-vibe-cli`)**:
   - **Why (lessons from OpenCode `#24121`, `#24343`, `#31763`, `#38067`, `#40474`):**
     - Mutating the top-level system prompt on every `Shift+Tab` invalidates the entire prompt cache prefix.
     - Leaving old read-only mode reminders in replayed history causes the model to stay stuck refusing edits after switching to `execute`/`normal`.
     - Level-triggering a switch reminder fires it on every subsequent turn instead of once.
   - **Implementation:**
     - Keep the base system prompt prefix **mode-stable** (shared harness identity, `<environment>`, `<modes>`, `<rules>`, `<self_repo>`, `<skills>`, `<user_instructions>`, `<project_instructions>`), so prompt caching stays warm across mode switches.
     - Add `EntryKind::ModeChange` to `SessionStore` and write it whenever `/mode` or `Shift+Tab` changes the active mode.
     - **Edge-triggered transition reminder:** Track `last_sent_mode: Option<AgentMode>` per session. Only when `last_sent_mode != Some(current_mode)`, inject a single synthetic `<system-reminder>` block before the user's message:
       - Entering `plan` / `ask`: `"Operational mode changed to PLAN (read-only). Only read, grep, find, and ls are available. Do not attempt file edits or shell commands."`
       - Leaving `plan` / `ask` -> `execute` / `normal`: `"Operational mode changed from PLAN to EXECUTE. You are no longer in read-only mode. Full file edit, shell, and verification tools are now permitted."`
       - Entering `rea`: `"Operational mode changed to REA (reverse-engineering). Read, search, and shell inspection tools are enabled; file modification tools are disabled."`
     - **Drop stale reminders on replay:** Tag synthetic mode reminders (e.g., `EntryKind::ModeChange` or synthetic marker) and strip all *prior* mode reminders when building `messages` for the LLM, keeping only the latest active transition if applicable.
     - **Hard tool-gating:** Always enforce `mode.allowed_tools()` both in `CompletionRequest.tools` and inside tool dispatch (reject disallowed tool calls even if the model hallucinates one from earlier turns).

### Acceptance Criteria & Verification
- Unit tests in `le-vibe-agent` for: (a) cancellation mid-loop persists partial output and returns `AgentError::Cancelled`; (b) transient 503 succeeds on retry #2; (c) mid-session provider switch writes `EntryKind::ModelChange` and normalizes incomplete tool-call turns cleanly; (d) mid-session mode switch injects the transition reminder **exactly once** (edge-triggered), prunes stale prior reminders on replay, and preserves the base system prompt prefix.
- Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

---

## Phase 4: Native Rust Tools Parity (`crates/le-vibe-tools`)

### Exact Changes
1. **Dependencies (`crates/le-vibe-tools/Cargo.toml`)**:
   - Add `regex = "1"` (and `globset = "0.4"` if needed for glob filtering).
2. **`read` tool**:
   - Prefix each output line with 1-indexed line numbers (`1: ...`, `2: ...`) and append a footer `[showing lines X..Y of Z]` when truncated/sliced so the model makes exact `edit` calls on the first try.
3. **`grep` tool**:
   - Support optional parameters: `case_insensitive: bool` (default `false`), `glob: String` (e.g. `*.rs`), `context: integer` (0–5 lines before/after).
   - Compile `pattern` with `regex::RegexBuilder` (fall back to literal `regex::escape` if invalid regex, or return a clear error).
4. **`find` tool (new built-in)**:
   - Parameters: `path` (default `"."`), `pattern` (glob or substring on filename/relative path), `max_depth` (optional integer).
   - Skips `.git`, `node_modules`, `target`, `.venv`, `__pycache__`, `dist`, `build`; caps at 500 paths.
   - Add `"find"` to `AgentMode::Plan`, `Ask`, and `Rea` `allowed_tools()` and to `PermissionStore` (`ToolPermission::ALWAYS`).
5. **`bash` & `powershell` shell tools**:
   - Accept optional `timeout_secs` (default `60`, max `600`). Spawn child process and poll `try_wait` in 50ms intervals; on timeout, kill child and return `ToolError::Edit("command timed out after Xs")` (or formatted error output with partial stdout/stderr).
   - Add `execute_shell_with_active_shell(active_shell: &str, args: &Value)` so `/term pwsh`, `/term bash`, `/term cmd`, etc. actually run the configured shell.

### Acceptance Criteria & Verification
- Unit tests in `crates/le-vibe-tools/src/lib.rs` for:
  - `read` line numbering + offset/limit footer
  - `grep` regex + `case_insensitive` + `glob`
  - `find` skipping `target`/`.git`
  - shell timeout killing a sleeping command within ~2s (using `timeout_secs: 1`).
- Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

---

## Phase 5: Real LLM Compaction, Context Budgeting, Handover & Session DAG

### Exact Changes
1. **Per-Project Session Storage & Rich Entries (`crates/le-vibe-durable/src/lib.rs`)**:
   - Group sessions by normalized working directory under `~/.le-vibe/sessions/<cwd-slug>/session-<ts>.jsonl` (still fall back to flat `~/.le-vibe/sessions/` when reading legacy files) so `--continue` resumes the **current project's** session.
   - Add optional session header metadata (`session_name`, `cwd`, `created_at`) and extend `EntryKind` with `ToolCall`, `ToolResult`, and `BranchSummary`, with optional `timestamp`, `tool_call_id`, `tool_name` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) for full backward compatibility.
   - Add `SessionStore::list_sessions(dir: &Path) -> Vec<SessionMeta>` (filename, display name, entry count, first user prompt preview, modified time, sorted newest first), `SessionStore::fork_from(&self, entry_id: &str, dest: &Path)`, `SessionStore::clone_branch(&self, dest: &Path)`, and `SessionStore::set_name(&mut self, name: &str)`.
   - Persist tool calls & tool results during `run_turn_loop_interactive` so `--continue`/`--resume` replays full tool context.
2. **Context Budgeting & Real LLM Compaction (`crates/le-vibe-cli/src/worker.rs`, `crates/le-vibe-agent`)**:
   - **Tool-result context hygiene:** Cap any single tool result stored into the live message array at `24KB` (preserving head + tail + `"...[truncated N bytes]"`) so one large file read cannot blow the context window.
   - **Real `/compact [custom instructions]`:** Replace placeholder compaction with a real LLM summarization turn using the active provider/model:
     - Summarize older entries while keeping the most recent ~20% of context (or last 4 messages) verbatim (`keepRecentTokens`), honoring optional user `[custom instructions]` when provided.
     - Save via `session.append_compaction(summary)` and display before/after token estimate.
   - **Auto-compaction:** When `context_window > 0` and `last_input_tokens >= (context_window * 85) / 100`, emit a notice and run compaction automatically before the next turn.
   - **`/context` (and `/session`):** Display exact context breakdown (estimated tokens for system prompt + `AGENTS.md` + `<skills>` + tool schemas + active branch messages vs `context_window`), session file path, ID, name, model switches, and cost.
3. **Handover & Branch Summaries (`/handover`, `/fork`, `/clone`, `/tree`, `/resume`, `/new`, `/name`, `/import`)**:
   - **`/handover [goal]`:** Generates a concise structured handoff summary from the active branch (goal, files read/modified, key decisions, open todos, exact verification commands), starts a fresh session seeded with that handoff as initial context, and optionally switches models cleanly without carrying stale turns.
   - **`/tree` (`ActiveModal::TreePicker`):** Navigate all DAG entries with tree glyphs (`├─`/`└─`) and `[●]` on active leaf. Selecting a user entry puts its text into the editor so you can edit & branch; selecting an assistant entry continues after it. Optionally summarize the abandoned branch into an `EntryKind::BranchSummary`.
   - **`/fork`:** Creates a new session file branched from a selected earlier user message.
   - **`/clone`:** Duplicates the current active branch into a new session file.
   - **`/resume` (`ActiveModal::SessionPicker`):** Lists saved project sessions (with search/filter, name, preview, timestamp); `Enter` loads it into the worker and rebuilds `state.blocks`.
   - **`/new`, `/name [title]`, `/import <path>`:** Fresh session, set/show display name, import external JSONL session.

### Acceptance Criteria & Verification
- Unit tests in `le-vibe-durable` for backward-compatible JSONL reload, `ToolCall`/`ToolResult` roundtrip, `fork_from`, `clone_branch`, `set_name`, and `list_sessions`.
- Unit tests in `le-vibe-tui` for `TreePicker` and `SessionPicker` modal bounds/selection.
- Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

---

## Phase 6: Production-Ready Rust TUI (`crates/le-vibe-tui` & `crates/le-vibe-cli`)

### 6A. Markdown Rendering (`crates/le-vibe-tui/src/markdown.rs`)
- Render `TranscriptBlock::Assistant` with a lightweight, allocation-safe line-by-line Markdown renderer producing `Vec<Line<'static>>`:
  - `#` / `##` / `###` headings (bold + theme `accent` / `secondary`).
  - Fenced code blocks (` ```lang ` ... ` ``` `) rendered inside a shaded card (`pal.card_quiet_bg`) with a muted language label header and `▎` left bar.
  - Inline `` `code` `` (highlighted fg + subtle bg), `**bold**`, `*italic*`.
  - Bullet lists (`- ` / `* `) rendered with `• `, numbered lists preserved, blockquotes (`> `) with `│ ` bar, horizontal rules (`---`) rendered as dim line.
  - Simple pipe tables (`| a | b |`) rendered with aligned dim borders.

### 6B. Full-Featured Editor (`crates/le-vibe-tui/src/editor.rs` & `input.rs`)
- Multi-line input: `Shift+Enter`, `Alt+Enter`, or `Ctrl+J` inserts `\n` (editor auto-grows 1–6 lines).
- Navigation & editing: `Home`/`Ctrl+A` (start of line), `End`/`Ctrl+E` (end of line), `Ctrl+Left`/`Ctrl+Right` (word jump), `Delete` (delete forward char), `Ctrl+W` (delete previous word), `Ctrl+U` (clear to line start), `Ctrl+K` (clear to line end).
- Bracketed paste: enable `EnableBracketedPaste` in `TerminalGuard` (disable on Drop); handle `Event::Paste(text)` cleanly without firing `Enter` submit mid-paste.
- Persistent prompt history: load/append `~/.le-vibe/history.txt` (cap 500 entries). When input is empty or on line 0 and no slash popup is active, `Up`/`Down` cycles history (while `Shift+Up`/`Shift+Down`, `PageUp`/`PageDown`, and mouse wheel scroll the transcript).

### 6C. `@file` Fuzzy Autocomplete & Prompt Templates
- Typing `@<prefix>` in the editor opens a compact popup above the editor listing matching workspace files (cached/refreshed lazily, skipping `.git`/`target`/`node_modules`/`.venv`); `Tab`/`Enter` inserts the selected relative path.
- Discover `*.md` prompt templates from `~/.le-vibe/prompts/` and `<cwd>/.le-vibe/prompts/`; expose them in the `/` slash popup as `/<stem>` and expand `$ARGUMENTS` when invoked.

### 6D. Transcript Ergonomics, Collapsible Cards, Todo Strip & Mouse Scroll
- **Collapsible cards:** Add `show_thinking: bool` (default `true`) and `expand_tools: bool` (default `false`) to `TuiState`:
  - `Ctrl+T` toggles thinking block body visibility (collapsed shows `▎ thinking (N lines · Ctrl+T to expand)`).
  - `Ctrl+O` toggles tool output preview (collapsed shows 3 lines, expanded shows up to 40 lines).
  - Include primary argument summary in `AgentEvent::ToolCallStart` / `TranscriptBlock::Tool` (e.g., `read crates/le-vibe-cli/src/main.rs · done`).
- **Live Todo strip:** When `TodoStore` has non-deleted items, send `WorkerMsg::TodosUpdated(Vec<(u64, String, String)>)` and render a compact 1–2 line summary strip above the status bar (e.g., `☑ 2/4 · ▸ Fixing modal overflow`).
- **Mouse wheel scroll:** Enable `EnableMouseCapture` in `TerminalGuard` (toggleable in `/config` so users can also select text with terminal mouse), handle `Event::Mouse(MouseEventKind::ScrollUp | ScrollDown)` to adjust `state.scroll_offset`.
- **Scroll indicator:** When `state.scroll_offset > 0`, show a subtle pill `↓ scrolled up (End to jump)` on the right side of the status strip; `End` (when input is empty) or `Ctrl+End` snaps `scroll_offset = 0`.

### 6E. Smoothness, Per-Input Acceptance UX, Modal Search & Utility Commands
- **Dirty-driven event loop (`crates/le-vibe-cli/src/main.rs`):** Track `needs_redraw: bool`. Only call `terminal.draw(...)` when input arrives, worker messages arrive, terminal resizes, smooth scroll is easing, or spinner ticks while `state.is_busy || state.usage_loading` (eliminates idle flicker and CPU burn).
- **Per-input acceptance & diff preview:**
  - In `ApprovalModal`: for `edit` and `write`, render a compact colored unified diff preview (`- old` red, `+ new` green, capped at 12 lines) inside the modal before the user selects Allow/Deny.
  - On `TurnDone`: append a subtle dim turn-summary line when tools ran (e.g., `✓ 3 tools · 1 file edited · 1.2k tok · 42.1 t/s · 4.1s`) so every user input has clear acceptance feedback.
  - In `plan` mode: after a plan finishes, show a non-intrusive hint in the status strip (`Plan ready — Shift+Tab to switch to execute mode`).
- **Type-to-filter in `/model` (and `/resume`) pickers:** Add `modal_filter: String` to `TuiState`; typing filters rows live and shows the query in the modal title; `Backspace` edits the filter.
- **Remaining Pi parity slash commands:**
  - `/copy`: copies last `TranscriptBlock::Assistant` to OS clipboard (Windows `powershell -NoProfile -Command Set-Clipboard`, macOS `pbcopy`, Linux `wl-copy`/`xclip`).
  - `/help` & `/hotkeys`: opens `ActiveModal::HelpModal` listing all keybindings and slash commands.
  - `/trust`: appends current working directory to `~/.le-vibe/policy.json` `trusted_folders` and reloads policy.
  - `/changelog`: opens `change-log.md` in a scrollable modal.

### Acceptance Criteria & Verification
- Unit tests in `le-vibe-tui` for: Markdown renderer (headings, code blocks, inline code, lists), editor multiline + word-jump + UTF-8 deletion, `@file` filter, `/model` filter, collapsible card rendering.
- Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

---

## Phase 7: Production Hardening, `TestBackend` Render Tests & Final Verification

### Exact Changes
1. **`ratatui::backend::TestBackend` render tests (`crates/le-vibe-tui/src/lib.rs` or `tests/render_tests.rs`)**:
   - Render full `draw_frame` at both **minimum size (`40x12`)** and **desktop size (`140x40`)** across every `ActiveModal` (`None`, `ModelPicker`, `ProviderPicker`, `ModePicker`, `ThemePicker`, `TermPicker`, `ConfigModal`, `LoginPicker`, `LogoutPicker`, `UsageModal`, `ApprovalModal`, `QuestionModal`, `ExportPicker`, `TreePicker`, `SessionPicker`, `HelpModal`) and with `/` slash popup open.
   - Assert zero panics, zero out-of-bounds cursor positions, and that modal borders stay within frame bounds.
2. **End-to-end integration tests**:
   - Python: load `examples/policies/*.json` and `examples/extensions/*.py` into `BridgeRuntime` and verify `secret_guard` blocks `.env`, `project_stats` returns counts, and `strict.json` denies `bash`.
   - Rust: verify `le-vibe --print` smoke test, `le-vibe --mode rpc` JSON roundtrip, and `SessionStore` branch + compaction roundtrip.
3. **Codebase Documentation (`docs/*.md`) & Release Polish**:
   - Write clear, concise, code-referenced Markdown docs so a developer can read and understand every subsystem without grepping the whole repo:
     - `docs/architecture.md` — 9-crate Rust workspace + Python engine map, thread model (UI thread vs worker thread vs Python subprocess), NDJSON streaming/RPC wire protocol, ASCII data-flow diagram.
     - `docs/modes-and-prompts.md` — sectioned system prompt assembly (`preamble`, `<environment>`, `<modes>`, `<rules>`, `<skills>`, `SYSTEM.md`, `AGENTS.md`), OpenCode-style edge-triggered mid-session mode transitions, stale reminder pruning, and prompt-cache preservation.
     - `docs/sessions-and-context.md` — JSONL DAG format (`EntryKind`), per-project session dirs, branching (`/tree`, `/fork`, `/clone`), LLM compaction (`keepRecentTokens`, auto-compact at 85%), `/handover`, context budgeting, and cross-provider mid-session model switching.
     - `docs/tui-internals.md` — `ratatui` layout constraints, dirty-driven redraw loop, Markdown renderer, multiline/history/paste editor, `@file` picker, modal overflow & scroll windowing, themes, and keybinding map.
     - `docs/extensions-and-policy.md` — Python `ExtensionHost` lifecycle (`before_tool_call`, `after_tool_call`, `PythonToolSpec`), `PermissionStore` evaluation order, `policy.json` schema, and immutable dangerous-command guardrails.
     - `docs/mcp-and-tools.md` — native Rust tool suite (`read`, `write`, `edit`, `grep`, `find`, `ls`, `bash`, `powershell`, `todo`, `ask_user_question`), timeout/shell handling, and stdio `McpManager` (`mcp.json`, namespacing, policy gating).
     - `docs/cli-reference.md` — complete CLI flags, subcommands, piped stdin, `--mode json` / `--mode rpc` schemas, and `/` slash commands table.
   - Update `README.md` (with a Documentation index linking `docs/*.md` and `examples/example.md`), `plan.md`, and `change-log.md`.
   - Reinstall global binary: `cargo install --path crates/le-vibe-cli`.

### Final Release Gate Checklist
```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run --project python --with pytest --with pytest-asyncio --with httpx --with pyyaml --with pydantic --with keyring pytest python/tests
cargo install --path crates/le-vibe-cli
```

---

## Suggested Prompt to Hand Off to a Smaller Model

When handing this plan to a smaller model, run **one phase per session/prompt**:

> Read `EXECUTION_PLAN.md` completely. Execute **Phase X** only. Follow all rules in Section 0 (do NOT touch `python/le_vibe/providers/*`). After implementing Phase X, run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` (plus `pytest python/tests` if Python changed) and fix every failure before reporting completion.
