# lazybug Examples & Configuration Guide

Quickstart, extensions, permission policies, MCP servers, skills, and prompt templates for `lazybug`.

## Quickstart

```sh
lazybug                              # Launch interactive TUI
lazybug --yolo                       # Auto-approve non-blocked tools
lazybug --print "Summarize repo"     # One-shot non-interactive run
lazybug --continue                   # Resume latest session
lazybug --resume <session-id>        # Resume a specific session
lazybug --mode rpc                   # JSONL stdin/stdout RPC mode
```

## Agent Modes (`Shift+Tab` or `/mode <name>`)

| Mode | Allowed Tools | Purpose |
|------|---------------|---------|
| `normal` | All tools | Interactive coding assistant |
| `plan` | `read`, `grep`, `find`, `ls` | Read-only architecture & planning |
| `execute` | All tools | End-to-end implementation & verification |
| `ask` | `read`, `grep`, `find`, `ls` | Read-only Q&A |
| `rea` | `read`, `grep`, `find`, `ls`, `bash`, `powershell` | Reverse-engineering & inspection |

Customize user-level instructions in `~/.lazybug/SYSTEM.md` and project instructions in `<cwd>/AGENTS.md`.

## Installing & Testing Extensions

Copy any `.py` extension from `examples/extensions/` to `~/.lazybug/extensions/` (global) or `.lazybug/extensions/` (project):

```sh
mkdir -p ~/.lazybug/extensions
cp examples/extensions/secret_guard.py ~/.lazybug/extensions/
```

Each extension defines `def register(host: ExtensionHost) -> None:` and can register:
- **Lifecycle hooks**: `host.on("before_tool_call", cb)` / `host.on("after_tool_call", cb)`
- **Custom Python tools**: `host.register_tool(PythonToolSpec(name=..., description=..., parameters=..., handler=...))`

Run `/reload` inside `lazybug` to hot-reload extensions without restarting.

## Installing & Testing Policies

Copy a profile from `examples/policies/` to `~/.lazybug/policy.json` or `.lazybug/policy.json`:

```sh
cp examples/policies/balanced.json ~/.lazybug/policy.json
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

Create `~/.lazybug/mcp.json` or `.lazybug/mcp.json`:

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

Place a `SKILL.md` under `.lazybug/skills/<name>/SKILL.md` or `~/.lazybug/skills/<name>/SKILL.md`:

```markdown
---
name: my-skill
description: Short description shown in the system prompt <skills> list
---

Step-by-step instructions loaded when invoked.
```

## Adding a Prompt Template

Place `<name>.md` under `.lazybug/prompts/` or `~/.lazybug/prompts/`. Invoke with `/<name> [arguments]` (`$ARGUMENTS` is replaced with your arguments).
