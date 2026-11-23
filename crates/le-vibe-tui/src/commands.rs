#[derive(Debug, Clone)]
pub struct SlashCommandSpec {
    pub name: &'static str,
    pub hint: &'static str,
    pub description: &'static str,
    pub picker_on_bare: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommandItem {
    pub name: String,
    pub description: String,
    pub picker_on_bare: bool,
}

pub const BUILTIN_SLASH_COMMANDS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        name: "/model",
        hint: "",
        description: "Pick or switch model (Tab: all/scoped · Space: pin · Type to filter)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/mode",
        hint: "",
        description: "Switch agent mode (Shift+Tab cycles: normal · plan · ask · rea · cyber)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/thinking",
        hint: "",
        description: "Set reasoning effort level",
        picker_on_bare: false,
    },
    SlashCommandSpec {
        name: "/config",
        hint: "",
        description: "Interactive settings modal (theme, shell, mode, yolo, thinking, motion)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/theme",
        hint: "",
        description: "Switch TUI color theme",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/term",
        hint: "",
        description: "Select terminal shell for ! / !! commands",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/yolo",
        hint: "",
        description: "Toggle auto-approve (yolo) permission mode",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/policy",
        hint: "",
        description: "List active policies, trusted folders, tool permissions & rules",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/login",
        hint: "",
        description: "View auth status & sign in (OIDC / OAuth / API key)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/logout",
        hint: "",
        description: "Remove saved credentials for a provider",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/usage",
        hint: "",
        description: "Live plan quota, budget & balance for vibe, deepseek, agy",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/auth",
        hint: "",
        description: "View provider authentication status & sign in",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/new",
        hint: "",
        description: "Start a fresh session (resets tokens & cost)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/clear",
        hint: "",
        description: "Clear transcript viewport (preserves session context)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/resume",
        hint: "",
        description: "Browse, filter & resume saved project sessions",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/rename",
        hint: "",
        description: "Set or show the current session display name",
        picker_on_bare: false,
    },
    SlashCommandSpec {
        name: "/status",
        hint: "",
        description: "Show current session file, entries, leaf & stats",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/context",
        hint: "",
        description: "Show context window breakdown (system, tools, messages)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/search",
        hint: "<query>",
        description: "BM25 search across all saved past sessions",
        picker_on_bare: false,
    },
    SlashCommandSpec {
        name: "/hotkeys",
        hint: "",
        description: "Show active keyboard shortcuts (and ~/.le-vibe/keybindings.json)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/tree",
        hint: "",
        description: "Navigate session DAG branches & jump to any user/assistant node",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/rewind",
        hint: "",
        description: "Rewind to an earlier user-input checkpoint",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/clone",
        hint: "",
        description: "Duplicate active branch into a new session file",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/compact",
        hint: "",
        description: "Summarize older context via LLM",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/handover",
        hint: "",
        description: "Generate structured handoff summary into a fresh session",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/export",
        hint: "",
        description: "Export the whole session transcript to Markdown or HTML",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/copy",
        hint: "[chat|code]",
        description: "Copy last reply (raw Markdown, `chat` for WhatsApp/Teams, or `code` block)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/report",
        hint: "",
        description: "Generate industry-standard bug report PDF from session context",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/suggestion",
        hint: "",
        description: "Analyze previous sessions & suggest le-vibe optimizations to apply",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/memory",
        hint: "",
        description: "Inspect or search persistent Rust RAG memory",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/skill-save",
        hint: "<name> [focus]",
        description: "Distill this workflow into .le-vibe/skills/<name>/SKILL.md (agentskills.io)",
        picker_on_bare: false,
    },
    SlashCommandSpec {
        name: "/diagram",
        hint: "",
        description: "Project latest XML / draw.io diagram (or local .xml/.drawio file) in modal",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/loop",
        hint: "",
        description: "Schedule recurring prompts (e.g. /loop 2m cargo test · /loop stop)",
        picker_on_bare: false,
    },
    SlashCommandSpec {
        name: "/plugins",
        hint: "",
        description: "List loaded extensions, tools, hooks & custom slash commands",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/reload-plugins",
        hint: "",
        description: "Reload extensions/plugins and their custom slash commands",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/followup",
        hint: "<prompt>",
        description: "Queue a follow-up prompt after the current turn (or Alt+Enter / Ctrl+Enter)",
        picker_on_bare: false,
    },
    SlashCommandSpec {
        name: "/trust",
        hint: "",
        description: "Trust current working directory in ~/.le-vibe/policy.json",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/reload",
        hint: "",
        description: "Reload catalogs, policy, plugins, MCP servers & skills",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/skills",
        hint: "",
        description: "List discovered SKILL.md resources",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/mcp",
        hint: "",
        description: "Show connected MCP servers and tool counts",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/help",
        hint: "",
        description: "Show keybindings and slash commands",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/changelog",
        hint: "",
        description: "View recent le-vibe changelog entries",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/quit",
        hint: "",
        description: "Quit le-vibe",
        picker_on_bare: true,
    },
];

pub fn filter_slash_commands(input: &str) -> Vec<&'static SlashCommandSpec> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') || trimmed.contains(' ') {
        return Vec::new();
    }
    BUILTIN_SLASH_COMMANDS
        .iter()
        .filter(|cmd| cmd.name.starts_with(trimmed))
        .collect()
}

pub fn filter_slash_commands_with_dynamic(
    input: &str,
    dynamic: &[(String, String)],
) -> Vec<SlashCommandItem> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') || trimmed.contains(' ') {
        return Vec::new();
    }
    let mut out: Vec<SlashCommandItem> = BUILTIN_SLASH_COMMANDS
        .iter()
        .filter(|cmd| cmd.name.starts_with(trimmed))
        .map(|cmd| SlashCommandItem {
            name: cmd.name.to_string(),
            description: cmd.description.to_string(),
            picker_on_bare: cmd.picker_on_bare,
        })
        .collect();
    for (name, desc) in dynamic {
        let cmd_name = if name.starts_with('/') {
            name.clone()
        } else {
            format!("/{name}")
        };
        if cmd_name.starts_with(trimmed) && !out.iter().any(|c| c.name == cmd_name) {
            out.push(SlashCommandItem {
                name: cmd_name,
                description: format!("[plugin] {desc}"),
                picker_on_bare: true,
            });
        }
    }
    out
}
