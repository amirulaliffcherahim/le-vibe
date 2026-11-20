# le-vibe Examples & Configuration Guide

Quickstart, extensions, permission policies, MCP servers, skills, and prompt templates for `le-vibe`.

## Quickstart

```sh
le-vibe                              # Launch interactive TUI
le-vibe --yolo                       # Auto-approve non-blocked tools
le-vibe --print "Summarize repo"     # One-shot non-interactive run
le-vibe --continue                   # Resume latest session
le-vibe --resume <session-id>        # Resume a specific session
le-vibe --mode rpc                   # JSONL stdin/stdout RPC mode
```

## Agent Modes (`Shift+Tab` or `/mode <name>`)

| Mode | Allowed Tools | Purpose |
|------|---------------|---------|
| `normal` | All tools | Interactive coding assistant |
| `plan` | `read`, `grep`, `find`, `ls` | Read-only architecture & planning |
| `execute` | All tools | End-to-end implementation & verification |
| `ask` | `read`, `grep`, `find`, `ls` | Read-only Q&A |
| `rea` | `read`, `grep`, `find`, `ls`, `bash`, `powershell` | Reverse-engineering & inspection |

Customize user-level instructions in `~/.le-vibe/SYSTEM.md` and project instructions in `<cwd>/AGENTS.md`.

## Installing & Testing Extensions

Copy any `.py` extension from `examples/extensions/` to `~/.le-vibe/extensions/` (global) or `.le-vibe/extensions/` (project):

```sh
mkdir -p ~/.le-vibe/extensions
cp examples/extensions/secret_guard.py ~/.le-vibe/extensions/
```

Each extension defines `def register(host: ExtensionHost) -> None:` and can register:
- **Lifecycle hooks**: `host.on("before_tool_call", cb)` / `host.on("after_tool_call", cb)`
- **Custom Python tools**: `host.register_tool(PythonToolSpec(name=..., description=..., parameters=..., handler=...))`

Run `/reload` inside `le-vibe` to hot-reload extensions without restarting.

## Installing & Testing Policies

Copy a profile from `examples/policies/` to `~/.le-vibe/policy.json` or `.le-vibe/policy.json`:

```sh
cp examples/policies/balanced.json ~/.le-vibe/policy.json
```

### Policy JSON Schema

```json
{
  "trusted_folders": ["."],
  "tool_permissions": {
    "read": "always",
    "write": "ask",
    "edit": "ask",
    "bash": "ask",
    "powershell": "ask"
  },
  "allowed_commands": ["git status", "git diff *", "cargo test *"],
  "allowed_paths": ["docs/*"],
  "blocked_commands": ["git push --force"]
}
```

Hardcoded dangerous shell patterns (`rm -rf /`, `mkfs`, `dd if=`, etc.) are always blocked and cannot be overridden by policy files.

## Adding an MCP Server

Create `~/.le-vibe/mcp.json` or `.le-vibe/mcp.json`:

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
    }
  }
}
```

## Adding a Skill (`SKILL.md`)

Place a `SKILL.md` under `.le-vibe/skills/<name>/SKILL.md` or `~/.le-vibe/skills/<name>/SKILL.md`:

```markdown
---
name: my-skill
description: Short description shown in the system prompt <skills> list
---

Step-by-step instructions loaded when invoked.
```

## Adding a Prompt Template

Place `<name>.md` under `.le-vibe/prompts/` or `~/.le-vibe/prompts/`. Invoke with `/<name> [arguments]` (`$ARGUMENTS` is replaced with your arguments).
