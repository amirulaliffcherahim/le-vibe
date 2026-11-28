---
name: skill-creator
description: Create or refine a reusable lazybug skill (on-demand workflow loaded via load_skill) with clean structure, low token footprint, and verification steps.
---

# Skill Creator

Build a focused, reusable `lazybug` skill that stays zero-cost until loaded via `load_skill`.

## 1. Confirm the Right Extension Axis
Choose the right layer before writing files:
- **Skill** (this workflow): on-demand procedure, checklist, or domain recipe triggered only when relevant.
- **Mode** (`mode-creator`): persistent persona, read/write boundary, tool allowlist, or per-role `model`/`thinking` override.
- **Extension** (`*.py`): new executable tools or policy hooks.

## 2. Choose Storage Location
Ask once only if scope is ambiguous; otherwise default to project scope inside a git repo:
- **Project (lazybug only):** `.lazybug/skill/<name>/SKILL.md` (or flat `.lazybug/skill/<name>.md`)
- **Project (cross-agent):** `.agents/skill/<name>/SKILL.md`
- **Global (all repos):** `~/.lazybug/skill/<name>/SKILL.md` (or `~/.agents/skill/<name>/SKILL.md`)

Use lowercase kebab-case for `<name>` (e.g., `pr-review`, `db-migration`, `release-check`).

## 3. Authoring Rules
1. **Required frontmatter:** Include `name` and a concrete, 1-sentence `description` naming the exact trigger conditions.
2. **Keep it lean:** Target 40–100 lines. Every line enters context once `load_skill` runs.
3. **Use positive, concrete steps:** State exact actions, commands, file patterns, and output shape. Skip filler and omit rules already in `<operating_contract>`.
4. **Stay tool-honest:** Rely on core tools (`read`, `write`, `edit`, `grep`, `find`, `ls`, active OS shell, `codemode`). If the workflow needs a lazy extension (`web`, `planning`) or MCP tool, state it explicitly as a prerequisite (`/ext load <name>`).
5. **Define done and stop conditions:** Specify the exact verification command and what to report if a prerequisite is missing.
6. **Split heavy references:** If the skill needs long templates or checklists, place them in sibling files under `<name>/` (e.g., `<name>/checklist.md`) and instruct the agent to `read` them only when needed.

## 4. Template
```markdown
---
name: <kebab-case-name>
description: <One sentence: what this skill does and when to load it.>
---

# <Title>

## Prerequisites
- <Required tools, extensions, or environment checks; omit if none.>

## Workflow
1. <Inspect or locate step with exact paths/commands.>
2. <Action step with constraints and edge cases.>
3. <Verification step with exact command and pass criteria.>

## Output Format
- <Exact structure expected in the final response.>
```

## 5. Verify After Writing
1. Write the file as UTF-8 with LF line endings.
2. Remind the user to run `/reload` (or start a fresh session) so `<skills>` indexes the new name.
3. Report the exact file path, line count, and how to invoke it (`load_skill` or mentioning the trigger).
