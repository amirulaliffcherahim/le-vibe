# `lazybug` Examples

Ready-to-copy templates for all five customization axes:

| Directory | What's Inside | Install Target (Global / Project) |
| --- | --- | --- |
| `modes/` | Editable Markdown personas (`reviewer.md`, `researcher.md`) | `~/.lazybug/mode/` or `.lazybug/mode/` |
| `skills/` | On-demand workflows (`skill-creator/`, `mode-creator/`) | `~/.lazybug/skill/` or `.lazybug/skill/` |
| `prompts/` | Reusable prompt templates (`review.md`, `deslop.md`, `commit-push-pr.md`) | `~/.lazybug/prompts/` or `.lazybug/prompts/` |
| `extensions/` | Python tool & hook extensions (`advise.py`, `secret_guard.py`, `audit_logger.py`, ...) | `~/.lazybug/extension/` or `.lazybug/extension/` |
| `policies/` | Permission policy presets (`balanced.json`, `strict.json`, `ci_readonly.json`) | `~/.lazybug/policy.json` |

## Quick Install

**Windows (PowerShell):**
```powershell
New-Item -ItemType Directory -Force "$HOME\.lazybug\skill", "$HOME\.lazybug\mode" | Out-Null
Copy-Item -Recurse -Force examples\skills\* "$HOME\.lazybug\skill\"
Copy-Item -Force examples\modes\* "$HOME\.lazybug\mode\"
```

**macOS / Linux:**
```bash
mkdir -p ~/.lazybug/skill ~/.lazybug/mode
cp -r examples/skills/* ~/.lazybug/skill/
cp examples/modes/* ~/.lazybug/mode/
```

Then run `/reload` inside `lazybug` (or start a fresh session).
