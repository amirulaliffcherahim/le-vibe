#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    Normal,
    Plan,
    Ask,
    Rea,
    Cyber,
}

pub const ALL_MODES: &[AgentMode] = &[
    AgentMode::Normal,
    AgentMode::Plan,
    AgentMode::Ask,
    AgentMode::Rea,
    AgentMode::Cyber,
];

impl AgentMode {
    pub fn name(self) -> &'static str {
        match self {
            AgentMode::Normal => "normal",
            AgentMode::Plan => "plan",
            AgentMode::Ask => "ask",
            AgentMode::Rea => "rea",
            AgentMode::Cyber => "cyber",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            AgentMode::Normal => "Standard coding assistant (all tools)",
            AgentMode::Plan => "Read-only architecture & planning mode (no file edits or shell)",
            AgentMode::Ask => "Read-only Q&A and code explanation mode",
            AgentMode::Rea => "Reverse-engineering & binary/app inspection mode (read/search/shell)",
            AgentMode::Cyber => "Defensive cybersecurity, code/config audit & hardening mode (all tools)",
        }
    }

    pub fn next(self) -> Self {
        match self {
            AgentMode::Normal => AgentMode::Plan,
            AgentMode::Plan => AgentMode::Ask,
            AgentMode::Ask => AgentMode::Rea,
            AgentMode::Rea => AgentMode::Cyber,
            AgentMode::Cyber => AgentMode::Normal,
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        ALL_MODES
            .iter()
            .copied()
            .find(|m| m.name().eq_ignore_ascii_case(s.trim()))
    }

    pub fn allowed_tools(self) -> Option<Vec<String>> {
        match self {
            AgentMode::Normal | AgentMode::Cyber => None,
            AgentMode::Plan | AgentMode::Ask => Some(vec![
                "read".into(),
                "grep".into(),
                "find".into(),
                "ls".into(),
                "memory_search".into(),
                "memory_read".into(),
            ]),
            AgentMode::Rea => Some(vec![
                "read".into(),
                "grep".into(),
                "find".into(),
                "ls".into(),
                "bash".into(),
                "powershell".into(),
                "memory_search".into(),
                "memory_read".into(),
            ]),
        }
    }

    pub fn system_prompt(self) -> &'static str {
        match self {
            AgentMode::Normal => "You are le-vibe, a concise, accurate coding assistant. Inspect files before editing, keep diffs minimal, and verify changes with real tests or builds.",
            AgentMode::Plan => "You are le-vibe in PLAN mode (read-only). You may only use read, grep, find, and ls. Never modify files or run shell commands. Inspect the relevant code and output a concrete, step-by-step plan with exact file paths.",
            AgentMode::Ask => "You are le-vibe in ASK mode (read-only Q&A). You may only use read, grep, find, and ls. Answer questions directly and concisely from inspected files without modifying anything.",
            AgentMode::Rea => "You are le-vibe in REA (reverse-engineering) mode. You have read, grep, find, ls, bash, and powershell (no file edits). Analyze binaries, protocols, strings, exports, and app internals concisely.",
            AgentMode::Cyber => "You are le-vibe in CYBER mode (defensive cybersecurity & hardening). Perform authorized security audits, threat modeling, vulnerability analysis, compliance checks, and secure remediation with concrete evidence.",
        }
    }
}

fn cap_utf8(text: &str, max_bytes: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max_bytes {
        return trimmed.to_string();
    }
    let mut idx = max_bytes;
    while !trimmed.is_char_boundary(idx) && idx > 0 {
        idx -= 1;
    }
    format!("{}...[truncated]", &trimmed[..idx])
}

pub fn build_system_prompt(
    mode: AgentMode,
    shell: &str,
    cwd: &str,
    self_repo: &str,
    skills: &[(String, String, String)],
    memory_toc: &str,
) -> String {
    let os = std::env::consts::OS;
    let mut prompt = format!(
        "{}\n\n\
         <environment>\n\
         os={os}, active_shell={shell}, cwd={cwd}\n\
         Active mode: {} ({})\n\
         </environment>\n\n\
         <modes>\n\
         Switch modes via Shift+Tab or /mode <name>:\n\
         - normal: all tools (read/write/edit/grep/find/ls/shell/todo/ask_user_question)\n\
         - plan: read-only (read/grep/find/ls) — architecture & step-by-step planning\n\
         - ask: read-only (read/grep/find/ls) — concise code Q&A\n\
         - rea: inspection + shell (read/grep/find/ls/bash/powershell) — reverse engineering\n\
         - cyber: all tools — defensive security, code/config audit & hardening\n\
         System prompts are defined in `crates/le-vibe-tui/src/mode.rs` (`AgentMode::system_prompt`, `build_system_prompt`).\n\
         </modes>\n\n\
         <rules>\n\
         - Only write shell commands compatible with `{shell}` on `{os}`.\n\
         - Tool efficiency: batch independent read-only calls (`ls`, `read`, `grep`, `find`) in parallel; start with `README.md` or manifest files instead of repeated `grep` loops; use `offset`/`limit` on `read` for large files; stop exploring as soon as you have enough context to answer.\n\
         - Be concise and direct. Show exact file paths. Only report results directly observed from tool output.\n\
         </rules>\n\n\
         <self_repo>\n\
         le-vibe source repository: `{self_repo}` (Rust harness in `crates/*`, Python provider/policy/extension engine in `python/le_vibe/*`, docs in `docs/*.md`, `examples/example.md`).\n\
         Self-editing workflow (Pi-style):\n\
         - Hot-reloadable with `/reload` (no restart): `~/.le-vibe/SYSTEM.md`, `<cwd>/AGENTS.md`, `~/.le-vibe/settings.json`, `<cwd>/.le-vibe/settings.json`, `~/.le-vibe/policy.json`, `~/.le-vibe/extensions/*.py`, `~/.le-vibe/mcp.json`, `.le-vibe/skills/*/SKILL.md`.\n\
         - Rust source edits (`{self_repo}/crates/*`): edit files, verify with `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, then rebuild with `cargo install --path crates/le-vibe-cli`.\n\
         </self_repo>\n\n\
         <memory>\n\
         Persistent RAG memory tools (`memory_search`, `memory_read`, `memory_write`, `memory_delete`):\n\
         - Call `memory_search` when past architectural decisions, gotchas, or user preferences may apply.\n\
         - Call `memory_write` (with 3-6 synonym `keywords` and a stable `key` for upsert) ONLY for durable decisions, non-obvious constraints, user preferences, or bug root causes—never for ephemeral task chatter.\n\
         - Call `memory_delete` to retire obsolete facts.\n\
         </memory>",
        mode.system_prompt(),
        mode.name(),
        mode.description(),
    );

    if !memory_toc.trim().is_empty() {
        prompt.push_str(&format!(
            "\n\n<memory_toc>\nStored long-term memories (call `memory_search` or `memory_read` to fetch details):\n{}\n</memory_toc>",
            cap_utf8(memory_toc, 1400)
        ));
    }

    if !skills.is_empty() {
        prompt.push_str("\n\n<skills>\nAvailable skills (use `read` on the path or `/skill:<name>` to load):\n");
        for (name, desc, path) in skills {
            prompt.push_str(&format!("- {name}: {desc} ({path})\n"));
        }
        prompt.push_str("</skills>");
    }

    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let user_sys = std::path::Path::new(&home).join(".le-vibe").join("SYSTEM.md");
        if let Ok(raw) = std::fs::read_to_string(&user_sys)
            && !raw.trim().is_empty()
        {
            prompt.push_str(&format!(
                "\n\n<user_instructions path=\"{}\">\n{}\n</user_instructions>",
                user_sys.display(),
                cap_utf8(&raw, 4000)
            ));
        }
        // Pi-compatible APPEND_SYSTEM.md: project file takes precedence over user file
        let project_append = std::path::Path::new(cwd)
            .join(".le-vibe")
            .join("APPEND_SYSTEM.md");
        let user_append = std::path::Path::new(&home)
            .join(".le-vibe")
            .join("APPEND_SYSTEM.md");
        let append_path = if project_append.is_file() {
            Some(project_append)
        } else if user_append.is_file() {
            Some(user_append)
        } else {
            None
        };
        if let Some(ap) = append_path
            && let Ok(raw) = std::fs::read_to_string(&ap)
            && !raw.trim().is_empty()
        {
            prompt.push_str(&format!(
                "\n\n<append_system path=\"{}\">\n{}\n</append_system>",
                ap.display(),
                cap_utf8(&raw, 4000)
            ));
        }
    }

    // Pi-compatible context file discovery: AGENTS.override.md replaces AGENTS.md/CLAUDE.md in cwd
    let cwd_dir = std::path::Path::new(cwd);
    let context_candidates = ["AGENTS.override.md", "AGENTS.md", "CLAUDE.md"];
    for name in context_candidates {
        let candidate = cwd_dir.join(name);
        if let Ok(agents_md) = std::fs::read_to_string(&candidate)
            && !agents_md.trim().is_empty()
        {
            prompt.push_str(&format!(
                "\n\n<project_instructions path=\"{}\">\n{}\n</project_instructions>",
                candidate.display(),
                cap_utf8(&agents_md, 4000)
            ));
            break;
        }
    }

    prompt
}

pub const ALL_SHELLS: &[(&str, &str)] = &[
    ("powershell", "Windows PowerShell (powershell -NoProfile -Command)"),
    ("pwsh", "PowerShell 7+ (pwsh -NoProfile -Command)"),
    ("bash", "Bash / Git Bash (bash -c)"),
    ("cmd", "Windows Command Prompt (cmd /C)"),
    ("zsh", "Zsh (zsh -c)"),
    ("sh", "POSIX Shell (sh -c)"),
    ("other", "Custom shell (type: /term <executable>)"),
];
