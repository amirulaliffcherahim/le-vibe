# le-vibe

**le-vibe** is a modular coding agent combining:
1. **A Modular Rust Agent Harness (`crates/*`)** — inspired by Pi's architecture: a 9-crate Rust workspace with a fluid 30fps `ratatui` + `crossterm` TUI, native coding tools, JSONL session DAG, and stdio MCP client.
2. **A Unified Python Provider, Policy & Extension Engine (`python/le_vibe/`)** — consolidating **Mistral Vibe** (`vibe`), **DeepSeek** (`deepseek`), and **Google Antigravity** (`agy`) into one Python provider module (`le_vibe.providers`), paired with Python permission policies (`le_vibe.policy`) and extensions/skills (`le_vibe.extensions`).

See [**goal.md**](goal.md), [**plan.md**](plan.md), and [**change-log.md**](change-log.md) for the full design, status, and commit timeline.

---

## Architecture Overview

```text
le-vibe/
├── Cargo.toml                   # 9-crate Rust workspace
├── change-log.md                # Chronological timeline of commits & features
├── goal.md                      # Architectural goals & design principles
├── plan.md                      # Implementation status & architecture boundary
├── python/
│   ├── pyproject.toml
│   ├── tests/                   # 23 pytest tests (providers, policy, extensions, bridge)
│   └── le_vibe/
│       ├── bridge.py            # Streaming NDJSON + JSON-RPC stdio bridge
│       ├── providers/           # Unified provider engine: vibe, deepseek, agy
│       │   ├── agy.py           # Antigravity (Cloud Code Assist SSE, thoughtSignature, thinking routing)
│       │   ├── vibe.py          # Mistral Vibe (completions, reasoning profiles)
│       │   ├── deepseek.py      # DeepSeek (reasoning_content replay, prompt cache cost)
│       │   ├── catalog.py       # 100% live API discovery + ~/.le-vibe/models-cache.json
│       │   ├── auth.py          # Local-only OIDC/OAuth PKCE, keyring & ~/.le-vibe/auth.json
│       │   └── usage.py         # Live quota/budget/balance readers
│       ├── policy/              # PermissionStore, shell safety checks, trusted folders, yolo
│       └── extensions/          # ExtensionHost lifecycle hooks, custom tools, SKILL.md loader
└── crates/
    ├── le-vibe-schema/          # JSON Schema argument validation
    ├── le-vibe-telemetry/       # Span & metric recording
    ├── le-vibe-pybridge/        # Bidirectional Rust <-> Python bridge
    ├── le-vibe-durable/         # Append-only JSONL session DAG, branching & compaction
    ├── le-vibe-tools/           # Native Rust tools: read, write, edit, grep, ls, bash, powershell, todo, ask_user_question
    ├── le-vibe-mcp/             # Stdio JSON-RPC 2.0 MCP client
    ├── le-vibe-agent/           # Event-driven Agent turn loop, modes & approval/question callbacks
    ├── le-vibe-tui/             # ratatui TUI (sleepy owl, shaded cards, 3-line pi-footer, 6 themes)
    └── le-vibe-cli/             # `le-vibe` binary (TUI, --print, --mode json/rpc, login, usage)
```

---

## Supported Providers (100% Live Catalogs)

No model list is hardcoded. Models are discovered dynamically from each authenticated provider's API, collapsed to base families (thinking variants route automatically via `/thinking`, defaulting to `medium` or `high`), and cached in `~/.le-vibe/models-cache.json`.

| Provider | Auth Method | Live Usage / Quota |
|----------|-------------|--------------------|
| **`vibe`** (Mistral Vibe) | OIDC / Console sign-in, OS keyring, or `MISTRAL_API_KEY` | Plan, Vibe Budget `%`, API Budget `%` |
| **`agy`** (Google Antigravity) | Google OAuth 2.0 PKCE (`antigravity/cli/1.4.2`) | Paid tier, Weekly + 5h + Argon quota windows |
| **`deepseek`** (DeepSeek) | `DEEPSEEK_API_KEY` or `/login deepseek <KEY>` | Live account balance (`GET /user/balance`) |

---

## Interactive TUI, Modes, Permissions & Commands

- **Sleepy Owl Banner + Shaded Cards**: `thinking`, `tool`, and `!`/`!!` script outputs render in full-width dark-tinted shaded cards; assistant prose stays unboxed.
- **3-Line `pi-footer`**:
  - Line 1: `repo · branch [git porcelain-v2 badges]`
  - Line 2: `↑ in · ↓ out · ↺ cache% · ⚡ t/s · $ cost · ◔ ctx%` + `(provider) model • thinking`
  - Line 3: `▸ mode · active provider mini quota bar` + `ask|yolo · ◷ active runtime`
- **5 Agent Modes (`Shift+Tab` or `/mode`)**: `normal`, `plan` (read-only), `execute`, `ask` (read-only), `rea` (reverse-engineering).
- **Permission Handler & YOLO**: Read-only tools auto-allow; mutating/shell tools prompt with `Allow once`, `Allow for session`, `Enable YOLO`, or `Deny` (dangerous shell patterns stay blocked). Toggle with `/yolo` or `--yolo`.
- **6 Full-App Themes (`/theme` or `/config`)**: `owl-cyan`, `nord`, `tokyo-night`, `catppuccin`, `gruvbox`, `dracula` (persisted in `~/.le-vibe/settings.json`).
- **Slash Commands**: `/model` (opens Scoped Models; `Tab` All ⇄ Scoped, `Space` pin/unpin, `Ctrl+S` save), `/provider`, `/mode`, `/yolo`, `/thinking`, `/login` (`/auth`), `/logout`, `/usage`, `/term`, `/theme`, `/config`, `/reload`, `/compact`, `/tree`, `/skills`, `/clear`, `/quit`.
- **Shell Shortcuts**: `!<cmd>` (added to session context) and `!!<cmd>` (no context), running in the `/term` shell.

---

## Install, Run & Test

```powershell
# Install global `le-vibe` binary
cargo install --path crates/le-vibe-cli

# Authenticate providers (stored locally in ~/.le-vibe/auth.json)
le-vibe login --provider vibe
le-vibe login --provider agy
le-vibe login --provider deepseek --api-key YOUR_KEY
le-vibe auth

# Interactive TUI (or auto-approve with --yolo)
le-vibe
le-vibe --yolo

# Non-interactive commands
le-vibe --list-models
le-vibe usage
le-vibe --provider agy --print "summarize README.md"
le-vibe --mode json --print "hello"
le-vibe --mode rpc
le-vibe --continue
```

**Run Tests & Lint**
```powershell
uv run --project python --with pytest --with pytest-asyncio --with httpx --with pyyaml --with pydantic --with keyring pytest python/tests
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

MIT / Apache-2.0 (see [LICENSE](LICENSE)).
