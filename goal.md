# le-vibe — Project Goal

## Executive Summary

**le-vibe** (`https://github.com/amirulaliffcherahim/le-vibe.git`, branch `main`) is a modular coding agent combining:
1. **The Pi Agent Harness in Modular Rust (`crates/*`)** — a 9-crate Cargo workspace (`schema`, `telemetry`, `pybridge`, `durable`, `tools`, `mcp`, `agent`, `tui`, `cli`) with a full-screen `ratatui` + `crossterm` TUI, native coding tools, JSONL session DAG, and stdio MCP client.
2. **A Unified Python Provider, Policy & Extension Engine (`python/le_vibe/`)** — consolidating **Mistral Vibe** (`vibe`), **DeepSeek** (`deepseek`), and **Google Antigravity** (`agy`) into one Python provider module (`le_vibe.providers`), paired with Python-native **permission policies** (`le_vibe.policy`) and **extensions/skills** (`le_vibe.extensions`).

---

## Core Architectural Goals (Achieved)

### 1. Unified Python Provider Engine (`le_vibe.providers`)
- **Zero legacy provider bloat**: Only `vibe`, `deepseek`, and `agy` are supported.
- **Zero hardcoded models**: Every model is discovered dynamically from each authenticated provider's API (`model-catalog`, `fetchAvailableModels`, `/v1/models`) and cached to `~/.le-vibe/models-cache.json` with 30-minute auto-refresh.
- **Base-model grouping & thinking routing**: Thinking variants (`-low`, `-medium`, `-high`, `-xhigh`, `-tiered`) collapse into clean base IDs; `/thinking` defaults to `medium` (or `high` when `medium` is absent).
- **Live quota & balance telemetry**: `/usage` queries Antigravity `retrieveUserQuotaSummary` + `paidTier`, Mistral `/v1/billing/subscription` + `/v1/billing/budget` (`Vibe Budget` + `API Budget`), and DeepSeek `/user/balance`.
- **Local-only credential storage**: Credentials stay strictly local in `~/.le-vibe/auth.json`, `~/.le-vibe/agy_oauth.json`, OS keyring, or environment variables — never committed to git.

### 2. Python Policy & Extensibility (`le_vibe.policy` & `le_vibe.extensions`)
- **`le_vibe.policy`**: `PermissionStore` with `allow`/`ask`/`deny` evaluation, session rule grants, dangerous shell blocking, trusted-folder boundaries, and `yolo` auto-approve toggle (`/yolo` or `--yolo`).
- **`le_vibe.extensions`**: `ExtensionHost` lifecycle hooks, custom Python tools, and `SKILL.md` discovery.

### 3. Modular Rust Pi Harness (`crates/*`)
- **`le-vibe-schema`**: JSON Schema argument validation.
- **`le-vibe-telemetry`**: Span & metric recording.
- **`le-vibe-pybridge`**: Bidirectional streaming NDJSON + JSON-RPC bridge to Python.
- **`le-vibe-durable`**: Append-only JSONL session DAG, `branch_from()`, and compaction.
- **`le-vibe-tools`**: Native `read`, `write`, `edit`, `grep`, `ls`, `bash`, `powershell`, `todo`, and `ask_user_question`.
- **`le-vibe-mcp`**: Native stdio JSON-RPC 2.0 MCP client.
- **`le-vibe-agent`**: Event-driven turn loop with streaming `AgentEvent`s, 5 agent modes (`normal`, `plan`, `execute`, `ask`, `rea`), and interactive approval/question callbacks.
- **`le-vibe-tui`**: Fluid 30fps `ratatui` TUI with sleepy owl banner, shaded cards, 3-line `pi-footer`, 6 full-app themes, and floating modals (`/model`, `/usage`, `/config`, `/term`, `/theme`, `/mode`, `/login`, `/logout`).
- **`le-vibe-cli`**: Global `le-vibe` binary supporting TUI, `--print`, `--mode json`, `--mode rpc`, `--continue`, `--resume`, `--yolo`, `login`, `logout`, `auth`, and `usage`.
