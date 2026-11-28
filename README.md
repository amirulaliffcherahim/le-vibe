# lazybug

**lazybug** is a modular coding agent combining:
1. **A Modular Rust Agent Harness (`crates/*`)** — inspired by Pi's architecture: a 9-crate Rust workspace with a dirty-driven `ratatui` + `crossterm` TUI, Markdown rendering, native coding tools, a pure-Rust hybrid BM25 + 512-dim scaled `i8` vector RAG memory engine, JSONL session DAG, and stdio MCP client.
2. **A Unified Python Provider, Policy & Extension Engine (`python/lazybug/`)** — consolidating **Mistral Vibe** (`vibe`), **DeepSeek** (`deepseek`), and **Google Antigravity** (`agy`) into one Python provider module (`lazybug.providers`), paired with file-driven Python permission policies (`lazybug.policy`) and auto-loaded extensions/plugins (`lazybug.extensions`).

See [**goal.md**](goal.md), [**plan.md**](plan.md), [**change-log.md**](change-log.md), [**examples/example.md**](examples/example.md), and the deep-dive guides in [**docs/**](docs/architecture.md).

---

## Documentation Index

- [**docs/architecture.md**](docs/architecture.md) — 9-crate Rust workspace, thread model & NDJSON bridge
- [**docs/modes-and-prompts.md**](docs/modes-and-prompts.md) — 5 agent modes, sectioned prompt, `<memory>`, edge-triggered mid-session mode switches
- [**docs/sessions-and-context.md**](docs/sessions-and-context.md) — JSONL DAG, `/rewind`, `/compact`, `/handover`, mid-session model switching
- [**docs/tui-internals.md**](docs/tui-internals.md) — `ratatui` layout, Markdown renderer, multiline editor, `@file` autocomplete, modals
- [**docs/extensions-and-policy.md**](docs/extensions-and-policy.md) — Python `ExtensionHost` hooks/tools/commands & `policy.json`
- [**docs/mcp-and-tools.md**](docs/mcp-and-tools.md) — Native Rust tools, RAG memory tools & stdio `McpManager`
- [**docs/cli-reference.md**](docs/cli-reference.md) — CLI flags, subcommands, piped stdin, JSON/RPC modes & `/` commands
- [**examples/example.md**](examples/example.md) — Runnable Python extension & policy examples (`examples/extensions/`, `examples/policies/`)

---

## Architecture Overview

```text
lazybug/
├── Cargo.toml                   # 9-crate Rust workspace (v0.2.0-beta.1)
├── docs/                        # Deep-dive architecture & internals guides
├── examples/                    # Runnable extension (.py) & policy (.json) examples
├── python/
│   ├── pyproject.toml
│   ├── tests/                   # 29 pytest tests (providers, policy, extensions, bridge)
│   └── lazybug/
│       ├── bridge.py            # Streaming NDJSON + JSON-RPC stdio bridge (PYTHONUTF8=1)
│       ├── providers/           # Unified provider engine: vibe, deepseek, agy (multimodal vision)
│       ├── policy/              # PermissionStore + ~/.lazybug/policy.json & .lazybug/policy.json
│       └── extensions/          # ExtensionHost hooks, custom tools, custom /commands, SKILL.md
└── crates/
    ├── lazybug-schema/          # JSON Schema argument validation
    ├── lazybug-telemetry/       # Span & metric recording
    ├── lazybug-pybridge/        # Cancellable Rust <-> Python bridge (instant Esc abort & respawn)
    ├── lazybug-durable/         # Append-only JSONL session DAG, branching, compaction, clone
    ├── lazybug-tools/           # Native Rust tools + Hybrid BM25/512d scaled-i8 RAG memory
    ├── lazybug-mcp/             # Stdio JSON-RPC 2.0 MCP client & McpManager
    ├── lazybug-agent/           # Streaming turn loop, retry, cross-provider & mode sanitization
    ├── lazybug-tui/             # Modular ratatui TUI (Markdown, 6 themes, modals, @file, export)
    └── lazybug-cli/             # `lazybug` binary (TUI, --print, --mode json/rpc, /report, /loop)
```

---

## Supported Providers (100% Live Catalogs)

No model list is hardcoded. Models are discovered dynamically from each authenticated provider's API, collapsed to base families (thinking variants route automatically via `/thinking`, defaulting to `medium` or `high`), and cached in `~/.lazybug/models-cache.json`.

| Provider | Auth Method | Live Usage / Quota |
|----------|-------------|--------------------|
| **`vibe`** (Mistral Vibe) | OIDC / Console sign-in, OS keyring, or `MISTRAL_API_KEY` | Plan, Vibe Budget `%`, API Budget `%` |
| **`agy`** (Google Antigravity) | Google OAuth 2.0 PKCE (`antigravity/cli/1.4.2`) | Paid tier, Weekly + 5h + Argon quota windows |
| **`deepseek`** (DeepSeek) | `DEEPSEEK_API_KEY` or `/login deepseek <KEY>` | Live account balance (`GET /user/balance`) |

---

## Key Features

- **5 Agent Modes (`Shift+Tab` or `/mode`)**: `normal`, `plan` (read-only), `ask` (read-only), `rea` (reverse-engineering), `cyber` (defensive security & hardening). Mid-session switches use OpenCode-style edge-triggered reminders to preserve prompt caching.
- **Rust Hybrid RAG Memory (`memory_search`, `memory_write`, `memory_read`, `memory_delete`, `/memory`)**:
  - Markdown source of truth partitioned by git root (`~/.lazybug/projects/<hash>/memory/` + global `~/.lazybug/memory/` — zero git noise).
  - Lucene non-negative BM25 + 512-dim scaled `i8` cosine vector cache (`rag-cache.bin`) + Reciprocal Rank Fusion, pre-RRF hard abstention, daily recency decay, `key` upsert, and pre-compaction memory flush.
- **Layered `/config` (16 Settings)**: User (`~/.lazybug/settings.json`), project (`.lazybug/settings.json`), and session-only scopes (`Tab` toggles save target; type to filter).
- **Sessions, Checkpoints & Handover**: `/rewind` (user-input checkpoints + double-`Esc`), `/resume` (searchable), `/rename`, `/status`, `/context`, `/clone`, `/compact`, `/handover`, `/export` (Markdown/HTML), `/report` (industry-standard bug report PDF).
- **Productivity & Self-Editing**: `/suggestion` (analyzes past sessions & logs to suggest `lazybug` optimizations), `/loop` (recurring prompt scheduler), `/policy`, `/plugins`, `/reload-plugins`, `/reload`, `/mcp`, `/skills`, `@file` autocomplete, default `Ctrl+V`/drag-and-drop image & file paste, instant `Esc` cancellation, and `F12` live event debugger (dev builds only).

---

## Install, Run & Test

```powershell
# Dev / Beta build (includes F12 debugger & ~/.lazybug/logs/ event log)
cargo install --path crates/lazybug-cli

# Official Release build (zero event logs, zero debugger compiled in)
cargo install --path crates/lazybug-cli --no-default-features

# Authenticate providers (stored locally in ~/.lazybug/auth.json)
lazybug login --provider vibe
lazybug login --provider agy
lazybug login --provider deepseek --api-key YOUR_KEY
lazybug auth

# Interactive TUI & CLI
lazybug
lazybug --yolo
lazybug --help
lazybug --list-models
lazybug usage
lazybug mcp list
lazybug --provider agy -p "summarize README.md"
lazybug --mode json -p "hello"
lazybug --mode rpc
lazybug --continue
```

**Run Tests & Lint**
```powershell
uv run --project python --with pytest --with pytest-asyncio --with httpx --with pyyaml --with pydantic --with keyring pytest python/tests
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p lazybug-cli --no-default-features -- -D warnings
cargo test --workspace
```

## License

MIT / Apache-2.0 (see [LICENSE](LICENSE)).
