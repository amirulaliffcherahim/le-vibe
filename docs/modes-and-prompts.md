# Modes & System Prompts

## Agent Modes (`crates/le-vibe-tui/src/mode.rs`)

Cycle with `Shift+Tab` or switch with `/mode <name>`:

| Mode | Allowed Tools | Behavior |
|------|---------------|----------|
| `normal` | All tools | Interactive coding assistant; inspects before editing |
| `plan` | `read`, `grep`, `find`, `ls`, `memory_search`, `memory_read` | Read-only architecture & step-by-step planning |
| `ask` | `read`, `grep`, `find`, `ls`, `memory_search`, `memory_read` | Read-only concise Q&A |
| `rea` | `read`, `grep`, `find`, `ls`, `bash`, `powershell`, `memory_search`, `memory_read` | Reverse-engineering & inspection (no file writes) |
| `cyber` | All tools | Defensive cybersecurity, code/config audit & hardening |

## Sectioned System Prompt (`build_system_prompt`)

`build_system_prompt` assembles XML-delimited sections:
1. Mode first-line persona (`AgentMode::system_prompt`)
2. `<environment>` (`os`, `active_shell`, `cwd`, active mode)
3. `<modes>` (mode summary + `crates/le-vibe-tui/src/mode.rs` reference)
4. `<rules>` (shell compatibility, parallel read-only tool batching, verification)
5. `<self_repo>` (`le-vibe` repository layout & test commands)
6. `<skills>` (discovered `SKILL.md` entries from `.le-vibe/skills` and `.pi/skills`)
7. `<user_instructions>` (optional `~/.le-vibe/SYSTEM.md`, capped at 4KB)
8. `<project_instructions>` (optional `<cwd>/AGENTS.md`, capped at 4KB)

## Mid-Session Mode Switching (`crates/le-vibe-agent/src/lib.rs`)

Mode transitions use an **edge-triggered** `<system-reminder>` (modeled after OpenCode):
- Every mode change appends `EntryKind::ModeChange` to `SessionStore`.
- On the first turn after a switch, `mode_transition_reminder` injects a single `<system-reminder>` lifting or applying read-only constraints.
- On subsequent turns, `sanitize_messages_for_provider` strips historical `<system-reminder>Operational mode changed...` entries so the base system prompt stays cache-stable and old read-only reminders never poison later `execute` turns.
