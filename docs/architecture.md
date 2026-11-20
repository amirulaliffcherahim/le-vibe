# le-vibe Architecture

`le-vibe` splits responsibilities cleanly between a **9-crate Rust harness** (`crates/*`) and a **unified Python engine** (`python/le_vibe/`).

```text
+----------------------------------------------------------------------+
|  Rust TUI & CLI (crates/le-vibe-cli, crates/le-vibe-tui)             |
|  - Dirty-driven 30fps ratatui loop, modals, Markdown, editor         |
|  - Non-blocking worker thread (mpsc::channel)                        |
+----------------------------------|-----------------------------------+
                                   |
                                   v
+----------------------------------------------------------------------+
|  Rust Core (agent, tools, durable, mcp, schema, telemetry)           |
|  - Turn loop, Esc cancellation, 429/5xx retry, mode/model sanitize   |
|  - Native tools (read/write/edit/grep/find/ls/bash/powershell/todo)  |
|  - Append-only JSONL DAG + stdio MCP manager                         |
+----------------------------------|-----------------------------------+
                                   | Stdio NDJSON (PYTHONUTF8=1)
                                   v
+----------------------------------------------------------------------+
|  Python Engine (python/le_vibe/)                                     |
|  - providers/ (vibe, deepseek, agy — streaming, auth, catalog, usage)|
|  - policy/ (PermissionStore, policy.json loader)                     |
|  - extensions/ (ExtensionHost hooks, custom tools, SKILL.md)         |
+----------------------------------------------------------------------+
```

## Crate Map

| Crate | Responsibility |
|-------|----------------|
| `le-vibe-schema` | JSON Schema validation for tool arguments |
| `le-vibe-telemetry` | In-memory span & timing recorder |
| `le-vibe-pybridge` | Stdio NDJSON/JSON-RPC bridge (`PythonBridge`, `ProviderPort`, `PolicyPort`, `ExtensionPort`) |
| `le-vibe-durable` | Append-only JSONL session DAG (`SessionStore`, branching, compaction, fork/clone) |
| `le-vibe-tools` | Native coding tools (`read`, `write`, `edit`, `grep`, `find`, `ls`, `bash`, `powershell`, `todo`, `ask_user_question`) |
| `le-vibe-mcp` | Stdio JSON-RPC 2.0 MCP client & `McpManager` (`mcp__<server>__<tool>`) |
| `le-vibe-agent` | Event-driven turn loop (`run_turn_loop_cancellable`), retry, cross-provider/mode sanitization |
| `le-vibe-tui` | Modular `ratatui` UI (`theme`, `mode`, `commands`, `state`, `editor`, `markdown`, `transcript`, `modal`, `input`, `export`) |
| `le-vibe-cli` | Binary entry point (`cli`, `settings`, `worker`, `rpc`, `main`) |
