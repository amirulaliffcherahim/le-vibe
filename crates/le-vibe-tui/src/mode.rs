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
                "retrieve_output".into(),
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
                "retrieve_output".into(),
            ]),
        }
    }

    pub fn system_prompt(self) -> &'static str {
        match self {
            AgentMode::Normal => "\
You are le-vibe in NORMAL mode (full coding agent).
- Workflow: inspect relevant manifests/files first (`read` before `edit`/`write`), check sibling patterns in the repo, make minimal surgical diffs, then verify with the real test/lint/build command.
- Integrity: never claim success from reading code alone; report exact file paths, commands run, and observed pass/fail output. If blocked after 2 repair attempts on the same error, stop and report the exact blocker.",
            AgentMode::Plan => "\
You are le-vibe in PLAN mode (read-only architecture & planning).
- Allowed tools: `read`, `grep`, `find`, `ls`, `memory_search`, `memory_read`. Never edit files or run shell commands.
- Workflow: trace entry points, data/type contracts, and sibling implementations first.
- Output: deliver a concrete, ordered implementation plan citing exact `file:line` / symbol names, key edge cases, trade-offs, and exact verification commands.",
            AgentMode::Ask => "\
You are le-vibe in ASK mode (read-only Q&A and code explanation).
- Allowed tools: `read`, `grep`, `find`, `ls`, `memory_search`, `memory_read`. Never modify files or run shell commands.
- Output: answer the question directly in the first sentence, then support it with exact file paths, symbol names, and concrete behavior observed in inspected files.",
            AgentMode::Rea => "\
You are le-vibe in REA mode (reverse-engineering & binary/app inspection).
- Allowed tools: `read`, `grep`, `find`, `ls`, `bash`, `powershell`, `memory_search`, `memory_read` (no source edits unless switched to normal).
- Workflow: identify target format/runtime (`file`, headers, strings, exports, symbols), trace entry points, IPC/protocol schemas, and auth/crypto boundaries; separate confirmed observations from hypotheses.",
            AgentMode::Cyber => "\
You are le-vibe in CYBER mode (defensive cybersecurity, audit & hardening).
- Scope: only authorized local targets or explicitly scoped hosts (default `127.0.0.1`). Never generate offensive exploit/payload/CVE artifacts or dump credentials.
- Workflow: audit trust boundaries, auth/session handling, input validation, secret exposure, and least-privilege defaults; report `file:line` or command evidence, severity, and minimal remediation diffs.",
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
    // Static sections come first for provider KV-cache / prompt-cache prefix hits;
    // session/workspace-specific blocks (<environment>, <memory_toc>, <skills>) follow below.
    let mut prompt = format!(
        "{}\n\n\
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
         - Be concise, direct, and easy to understand: use plain, simple language instead of heavy technical jargon; if a technical term is unavoidable, explain it in a few plain words.\n\
         - Make every output copy-paste friendly: put ready-to-run commands, configs, and code in clean fenced code blocks (` ```lang `), and keep Markdown tables compact with short cell values.\n\
         - Show exact file paths. Only report results directly observed from tool output.\n\
         - Use `todo` for multi-step work (3+ steps); keep exactly one task `in_progress` and verify before marking `completed`.\n\
         </rules>\n\n\
         <efficiency>\n\
         - Context engineering: start with manifests/entry points (`README.md`, `Cargo.toml`, `package.json`), locate symbols with `grep`/`find`, then `read` targeted `offset`/`limit` ranges instead of dumping whole files.\n\
         - Parallel batching: issue independent read-only calls (`read`, `grep`, `find`, `ls`) in parallel; stop exploring as soon as you have enough context.\n\
         - High-density output: skip filler, restatements, and obvious tool narration; prefer compact lists/tables over repetitive prose.\n\
         - Bounded repair loop: run tests/lint/build after edits, fix root causes once, and stop after 2 failed attempts on the same error.\n\
         </efficiency>\n\n\
         <memory>\n\
         Persistent RAG memory tools (`memory_search`, `memory_read`, `memory_write`, `memory_delete`):\n\
         - Call `memory_search` when past architectural decisions, gotchas, or user preferences may apply.\n\
         - Call `memory_write` (with 3-6 synonym `keywords` and a stable `key` for upsert) ONLY for durable decisions, non-obvious constraints, user preferences, or bug root causes—never for ephemeral task chatter.\n\
         - Call `memory_delete` to retire obsolete facts.\n\
         </memory>\n\n\
         <self_repo>\n\
         le-vibe source repository: `{self_repo}` (Rust harness in `crates/*`, Python provider/policy/extension engine in `python/le_vibe/*`, docs in `docs/*.md`, `examples/example.md`).\n\
         Self-editing workflow (Pi-style):\n\
         - Hot-reloadable with `/reload` (no restart): `~/.le-vibe/SYSTEM.md`, `<cwd>/AGENTS.md`, `~/.le-vibe/settings.json`, `<cwd>/.le-vibe/settings.json`, `~/.le-vibe/policy.json`, `~/.le-vibe/extensions/*.py`, `~/.le-vibe/mcp.json`, `.le-vibe/skills/*/SKILL.md`.\n\
         - Rust source edits (`{self_repo}/crates/*`): edit files, verify with `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, then rebuild with `cargo install --path crates/le-vibe-cli`.\n\
         </self_repo>\n\n\
         <environment>\n\
         os={os}, active_shell={shell}, cwd={cwd}\n\
         Active mode: {} ({})\n\
         </environment>",
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
        let names: Vec<&str> = skills.iter().map(|(n, _, _)| n.as_str()).collect();
        prompt.push_str(&format!(
            "\n\n<skills>\nOn-demand skills (call `load_skill` with a skill name only when relevant to the task): {}\n</skills>",
            names.join(", ")
        ));
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

    // Pi/Claude-compatible context discovery: walk cwd up to git root (or 4 levels),
    // applying root-to-cwd order; AGENTS.override.md replaces AGENTS.md/CLAUDE.md in the same dir.
    let mut dirs_to_check: Vec<std::path::PathBuf> = Vec::new();
    let mut cur = std::path::Path::new(cwd).to_path_buf();
    for _ in 0..5 {
        dirs_to_check.push(cur.clone());
        if cur.join(".git").exists() {
            break;
        }
        if !cur.pop() {
            break;
        }
    }
    dirs_to_check.reverse();
    let context_candidates = ["AGENTS.override.md", "AGENTS.md", "CLAUDE.md"];
    for dir in dirs_to_check {
        for name in context_candidates {
            let candidate = dir.join(name);
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
