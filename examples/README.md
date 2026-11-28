# `le-vibe` Examples

Ready-to-copy templates for all five customization axes:

| Directory | What's Inside | Install Target (Global / Project) |
| --- | --- | --- |
| `modes/` | Editable Markdown personas (`reviewer.md`, `researcher.md`) | `~/.le-vibe/mode/` or `.le-vibe/mode/` |
| `skills/` | On-demand workflows (`skill-creator/`, `mode-creator/`) | `~/.le-vibe/skill/` or `.le-vibe/skill/` |
| `prompts/` | Reusable prompt templates (`review.md`, `deslop.md`, `commit-push-pr.md`) | `~/.le-vibe/prompts/` or `.le-vibe/prompts/` |
| `extensions/` | Python tool & hook extensions (`advise.py`, `secret_guard.py`, `audit_logger.py`, ...) | `~/.le-vibe/extension/` or `.le-vibe/extension/` |
| `policies/` | Permission policy presets (`balanced.json`, `strict.json`, `ci_readonly.json`) | `~/.le-vibe/policy.json` |

## Quick Install

**Windows (PowerShell):**
```powershell
New-Item -ItemType Directory -Force "$HOME\.le-vibe\skill", "$HOME\.le-vibe\mode" | Out-Null
Copy-Item -Recurse -Force examples\skills\* "$HOME\.le-vibe\skill\"
Copy-Item -Force examples\modes\* "$HOME\.le-vibe\mode\"
```

**macOS / Linux:**
```bash
mkdir -p ~/.le-vibe/skill ~/.le-vibe/mode
cp -r examples/skills/* ~/.le-vibe/skill/
cp examples/modes/* ~/.le-vibe/mode/
```

Then run `/reload` inside `le-vibe` (or start a fresh session).
