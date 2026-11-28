# MCP & Native Rust Tools

## Native Tools (`crates/lazybug-tools/src/lib.rs`)

| Tool | Capabilities |
|------|--------------|
| `read` | 1-indexed line numbers (`1: ...`), `offset`, `limit`, `[showing lines X..Y of Z]` footer |
| `write` | Creates parent dirs, writes UTF-8 file |
| `edit` | Multi-span exact non-overlapping text replacements |
| `grep` | `regex::RegexBuilder`, `case_insensitive`, `glob`, `context` (0–5 lines), skips `.git`/`target`/`node_modules`/`.venv`, caps at 200 matches |
| `find` | Recursive path/glob search with optional `max_depth`, caps at 500 paths |
| `ls` | Sorted directory listing with `/` suffix on directories |
| `bash` / `powershell` | Spawned with `timeout_secs` (default 60s, max 600s) and `try_wait` kill on timeout |
| `todo` | In-session multi-step task store (`create`, `update`, `list`, `get`, `delete`, `clear`) synced to TUI Todo strip |
| `ask_user_question` | Interactive modal for 1–4 structured questions |

## Stdio MCP (`crates/lazybug-mcp/src/lib.rs`)

`McpManager` loads `~/.lazybug/mcp.json` and `<cwd>/.lazybug/mcp.json` (`{"mcpServers": {...}}`), spawns each server over stdio JSON-RPC 2.0, exposes tools namespaced as `mcp__<server>__<tool>`, and gates execution through `PolicyPort`.

Inspect connected servers with `/mcp` in TUI or `lazybug mcp list` from the CLI.
