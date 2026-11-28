# Modes & System Prompts

## Agent Modes (`crates/lazybug-tui/src/mode.rs`)

Cycle with `Shift+Tab` or switch with `/mode <name>`:

| Mode | Allowed Tools | Behavior |
|------|---------------|----------|
| `normal` | All tools | Interactive coding assistant; inspects before editing |
| `plan` | `read`, `grep`, `find`, `ls`, `memory_search`, `memory_read` | Read-only architecture & step-by-step planning |
| `ask` | `read`, `grep`, `find`, `ls`, `memory_search`, `memory_read` | Read-only concise Q&A |
| `rea` | `read`, `grep`, `find`, `ls`, `bash`, `powershell`, `memory_search`, `memory_read` | Reverse-engineering & inspection (no file writes) |
| `cyber` | All tools | Defensive cybersecurity, code/config audit & hardening |

## Editable Modes & Creator Skills (`examples/modes/`, `examples/skills/`)

`lazybug` ships `normal` as the single built-in default mode. Add custom personas as Markdown files
with YAML frontmatter under `~/.lazybug/mode/*.md` or `.lazybug/mode/*.md` (see
`examples/modes/reviewer.md` and `examples/modes/researcher.md`), and copy
`examples/skills/skill-creator/` or `examples/skills/mode-creator/` into `~/.lazybug/skill/` (then
run `/reload`) to author new skills and modes interactively.

## Sectioned System Prompt (`build_system_prompt`)


The prompt is a **short operating manual**, ordered so the cacheable prefix comes first and the
most volatile block comes last. Rules sit at the top (`<operating_contract>`) and are restated at
the bottom (`<non_negotiables>`) because attention decays across a long turn.

| # | Section | Stability | Contents |
|---|---|---|---|
| 1 | `<operating_contract>` | static | Locate → act → minimal edit → prove → evidence. Positive phrasing only. |
| 2 | `<tools>` | static | Only what a schema cannot say: shell must match `active_shell`, `retrieve_output` for compressed output, memory durability rule, web scope, output style. |
| 3 | `<environment>` | per session | `os`, `active_shell`, `cwd`. |
| 4 | `<mode>` | per mode | Method line (`AgentMode::system_prompt`) + pipeline steps (`mode_steps`) for plan/rea/cyber. Placed here so a mode switch only invalidates this block onward. |
| 5 | `<self_repo>` | conditional | Self-debugging guidance, loaded **only when `cwd` is inside the lazybug checkout**. |
| 6 | `<user_instructions>` / `<append_system>` | per user | `~/.lazybug/SYSTEM.md`, `APPEND_SYSTEM.md` (project file wins), capped 4KB. |
| 7 | `<project_instructions>` | per repo | `AGENTS.override.md` → `AGENTS.md` → `CLAUDE.md`, root-to-cwd, capped 4KB. |
| 8 | `<skills>` | per repo | Names only; bodies load through `load_skill`. |
| 9 | `<memory_toc>` | most volatile | RAG index summary, capped 1.4KB. |
| 10 | `<non_negotiables>` | static | Verify then report, minimal diff, ask once when blocked, never fabricate. |

**Budget:** the static core (sections 1–4) is ~230 words (~1.6KB). A test enforces ≤ 420 words
(`crates/lazybug-tui/tests/prompt_contract.rs`), and `tests/prompt_size_probe.rs` prints the current
sizes: `cargo test -p lazybug-tui --test prompt_size_probe -- --nocapture`.

**Measured effect** of the 2026-11 restructure: total prompt 8,738 → 4,935 chars (−43%), first-turn
prompt tokens 6,532 → 5,420 (−17%; the remainder is unchanged tool schemas), and the
`short_bugfix` benchmark passed 3/3 on both prompts with unchanged wall time and −20% session
tokens.

Most of the remaining first-turn budget is **tool schemas**, not prose: with 20 tools registered
they dominate the count. Trimming the tool surface (`codemode` only) is the next lever.

## Mid-Session Mode Switching (`crates/lazybug-agent/src/lib.rs`)

Mode transitions use an **edge-triggered** `<system-reminder>` (modeled after OpenCode):
- Every mode change appends `EntryKind::ModeChange` to `SessionStore`.
- On the first turn after a switch, `mode_transition_reminder` injects a single `<system-reminder>` lifting or applying read-only constraints.
- On subsequent turns, `sanitize_messages_for_provider` strips historical `<system-reminder>Operational mode changed...` entries so the base system prompt stays cache-stable and old read-only reminders never poison later `execute` turns.
