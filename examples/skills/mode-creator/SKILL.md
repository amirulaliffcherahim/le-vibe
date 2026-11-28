---
name: mode-creator
description: Create or customize a lazybug Markdown mode (persona) with validated ModeSpec YAML frontmatter, least-privilege tools, and cache-safe instructions.
---

# Mode Creator

Create an editable, OpenCode-style Markdown persona for `lazybug` (`Tab` / `/mode <name>` / `spawn_agent(mode="...")`).

## 1. Confirm Scope & Precedence
Discovery precedence (later overrides earlier; matching a built-in name like `normal` overrides it):
1. Built-in `normal`
2. `~/.lazybug/mode/<name>.md` (global)
3. `~/.agents/mode/<name>.md` (global cross-agent)
4. `.agents/mode/<name>.md` (project cross-agent)
5. `.lazybug/mode/<name>.md` (project override)

Default to `.lazybug/mode/<name>.md` for repo-specific roles and `~/.lazybug/mode/<name>.md` for personal daily-drive personas.

## 2. Supported Frontmatter (`ModeSpec`)
Include only the fields the persona actually overrides:

```yaml
---
description: Short 1-line summary shown in the mode picker
scope: all                    # primary | subagent | all (default: all)
can_write: false              # false enforces read-only tools and skips edit verification
verify_edits: true            # override VerificationGate (relevant when can_write: true)
model: vibe/mistral-large-4-0 # optional provider/model override for this persona
thinking: medium              # optional thinking override (clamped per model)
max_turns: 25                 # optional per-prompt turn cap
extensions:                   # optional lazy extensions to load with this mode
  - web
tools:                        # optional per-tool allow/deny map
  read: true
  grep: true
  find: true
  ls: true
  edit: false
  write: false
  powershell: false
---
```

## 3. Tool & Token Discipline
- **Least privilege:** Set `can_write: false` and disable mutating tools for reviewers, planners, and researchers.
- **Keep schemas small:** Enable only the core tools (`read`, `write`, `edit`, `grep`, `find`, `ls`, active OS shell, `retrieve_output`, `load_skill`, `codemode`) plus explicitly needed lazy extensions (`web`, `planning`).
- **Route cost deliberately:** Pair strong planner/reviewer modes (`model` override, low `max_turns`) with fast execution modes (`normal`).
- **Subagent-only personas:** Set `scope: subagent` when the mode is meant for `spawn_agent(mode="<name>")` without cluttering `Tab` cycling.

## 4. Body Authoring Rules
1. **Keep the body concise (30–90 words):** The body renders inside `<mode name="...">` directly below `<operating_contract>`, `<tools>`, and `<environment>`, preserving the shared KV-cache prefix.
2. **State only the persona delta:** Define the role's objective, inspection sequence, evidence format (e.g., `file:line` citations), and stop rule. Omit generic rules already in `<operating_contract>`.
3. **Use positive phrasing:** Describe what the persona should do and produce.

## 5. Verify After Writing
1. Write the file as UTF-8 with LF line endings and confirm valid `---` frontmatter fences.
2. Instruct the user to run `/reload`.
3. Confirm how to use it:
   - `primary` or `all`: switch via `/mode <name>` or `Tab`.
   - `subagent` or `all`: invoke via `spawn_agent(mode="<name>")`.
