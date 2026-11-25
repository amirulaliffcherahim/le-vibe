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

#[derive(Debug, Clone, Copy)]
pub struct ModeWorkflow {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub prompt_template: &'static str,
}

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
            AgentMode::Normal => "Execute mode — Claude Code Proof Loop (Act → Run → Inspect → Assert → Terminate)",
            AgentMode::Plan => "Elephant-Goldfish Model (EGM) — Brainstorm, Draft Spec & Fresh Goldfish Review",
            AgentMode::Ask => "Read-only Q&A and code architecture explanation",
            AgentMode::Rea => "Reverse-engineering — Red-Hat Worker + REA (morluto/rea) & Ghidra pipeline",
            AgentMode::Cyber => "Defensive cybersecurity — Authorized Recon, White-Box Audit & Hardening",
        }
    }

    pub fn workflows(self) -> &'static [ModeWorkflow] {
        match self {
            AgentMode::Plan => &[
                ModeWorkflow {
                    id: "egm_brainstorm",
                    label: "EGM Brainstorm (Elephant)",
                    description: "Explore idea, surface trade-offs with ask_user_question, draft concepts_brief.md",
                    prompt_template: "Run an Elephant-Goldfish Model (EGM) Brainstorm: explore the problem with full project context, inspect relevant code first, ask clarifying trade-offs via ask_user_question, and produce a concise Concepts Brief (goals, non-goals, options, open questions) for: ",
                },
                ModeWorkflow {
                    id: "egm_draft",
                    label: "EGM Draft Spec (Elephant)",
                    description: "Turn brief into structured Feature Spec (Heilmeier + architecture + failure modes)",
                    prompt_template: "Run an EGM Draft: turn our agreed direction into a concrete Feature Specification (Heilmeier framing, exact file:line touchpoints, data/API contracts, failure modes, edge cases, and binary verification criteria). Pause for approval before any implementation: ",
                },
                ModeWorkflow {
                    id: "egm_goldfish",
                    label: "EGM Goldfish Review (Fresh Context)",
                    description: "Spawn isolated zero-history reviewer across 5 EGM lenses (Comprehension/Structural/Codebase/Critic/Readiness)",
                    prompt_template: "/goldfish",
                },
                ModeWorkflow {
                    id: "egm_drift",
                    label: "EGM Drift Report",
                    description: "Compare existing design docs/specs against live source code and flag divergences",
                    prompt_template: "Run an EGM Drift Report: inspect the design docs/README against the live source code and list every divergence with exact file:line citations and whether to update docs or code: ",
                },
                ModeWorkflow {
                    id: "egm_lite",
                    label: "EGM Lite (Fast-Track Spec)",
                    description: "Compact single-pass spec for small refactors, revisions, and bug fixes",
                    prompt_template: "Run EGM Lite: inspect the relevant files and produce a compact 1-page spec (root cause, minimal file:line plan, edge cases, exact test command) for: ",
                },
            ],
            AgentMode::Normal => &[
                ModeWorkflow {
                    id: "proof_goal",
                    label: "Proof Loop Goal (/goal)",
                    description: "Set a strict binary stop condition (max 5 auto-verify iterations until proven)",
                    prompt_template: "/goal ",
                },
                ModeWorkflow {
                    id: "proof_loop",
                    label: "Verified Fix (Act → Run → Inspect → Assert)",
                    description: "Make minimal surgical edit and prove it with deterministic test/lint/build evidence",
                    prompt_template: "Execute using the strict Proof Loop (1. Act minimal diff -> 2. Run smallest deterministic test/lint/build -> 3. Inspect raw stdout/stderr/diff -> 4. Assert & rerun on failure -> 5. Terminate only on concrete pass evidence): ",
                },
                ModeWorkflow {
                    id: "fresh_review",
                    label: "Fresh-Context Diff Review",
                    description: "Audit current git diff with zero-bias checklist for regressions & edge cases",
                    prompt_template: "/goldfish diff",
                },
            ],
            AgentMode::Rea => &[
                ModeWorkflow {
                    id: "rea_triage",
                    label: "Stage 1 · Binary / App Triage (Red-Hat Worker)",
                    description: "Identify format, arch, runtime, packers, strings, imports/exports & entry points",
                    prompt_template: "Run REA Stage 1 (Triage): identify target format, architecture, runtime/packing, strings, imports/exports, and entry points. Separate Confirmed observations from Hypotheses for: ",
                },
                ModeWorkflow {
                    id: "rea_ghidra",
                    label: "Stage 2 · REA / Ghidra Decompile & Trace",
                    description: "List functions, decompile key routines & trace xrefs via REA MCP/CLI or Ghidra headless",
                    prompt_template: "Run REA Stage 2 (REA + Ghidra Decompile): use connected REA MCP/CLI tools (or Ghidra analyzeHeadless fallback) to list key functions, decompile entry/handler routines, trace xrefs, and record exact addresses/symbols for: ",
                },
                ModeWorkflow {
                    id: "rea_protocol",
                    label: "Stage 3 · Protocol / IPC & Struct Map",
                    description: "Reconstruct wire schemas, IPC endpoints, state machines & auth/crypto boundaries",
                    prompt_template: "Run REA Stage 3 (Protocol & Struct Reconstruction): trace IPC/network/file handlers, reconstruct message schemas and auth/crypto boundaries with confirmed addresses/file:line evidence for: ",
                },
                ModeWorkflow {
                    id: "rea_diff",
                    label: "Stage 4 · Version Diff & Clean-Room Spec",
                    description: "Compare two builds and produce a clean-room behavior spec",
                    prompt_template: "Run REA Stage 4 (Version Diff & Clean-Room Spec): compare symbols/functions/strings between versions and document confirmed behavioral deltas for: ",
                },
            ],
            AgentMode::Cyber => &[
                ModeWorkflow {
                    id: "cyber_recon",
                    label: "Stage 1 · Attack Surface & Trust Map",
                    description: "Map endpoints, auth boundaries, secrets, dependencies & headers (authorized scope only)",
                    prompt_template: "Run Cyber Stage 1 (Authorized Recon & Attack Surface Map — local/authorized scope only): map entry points, auth/session boundaries, secrets/config, dependencies, and trust boundaries with file:line citations for: ",
                },
                ModeWorkflow {
                    id: "cyber_audit",
                    label: "Stage 2 · White-Box Code & Config Audit",
                    description: "OWASP/CWE review: auth/RBAC, injection sinks, SSRF, path traversal, crypto & least privilege",
                    prompt_template: "Run Cyber Stage 2 (White-Box Audit): audit auth/RBAC, input validation, injection sinks, SSRF/path traversal, crypto, and secret handling. Report severity, file:line evidence, and minimal remediation for: ",
                },
                ModeWorkflow {
                    id: "cyber_verify",
                    label: "Stage 3 · Safe Proof-of-Control Check",
                    description: "Benign/read-only verification via unit tests or local 127.0.0.1 checks (no offensive payloads)",
                    prompt_template: "Run Cyber Stage 3 (Safe Verification): write or run deterministic defensive unit/integration checks on authorized local code/127.0.0.1 to prove controls hold (no offensive exploit artifacts): ",
                },
                ModeWorkflow {
                    id: "cyber_harden",
                    label: "Stage 4 · Hardening & Remediation Patch",
                    description: "Apply minimal defense-in-depth fixes and prove them with regression tests",
                    prompt_template: "Run Cyber Stage 4 (Hardening): apply minimal surgical hardening fixes for confirmed findings and verify with deterministic tests: ",
                },
            ],
            AgentMode::Ask => &[
                ModeWorkflow {
                    id: "ask_trace",
                    label: "Trace Execution Path",
                    description: "Trace a feature end-to-end with exact file:line citations",
                    prompt_template: "Trace the execution path end-to-end with exact file:line and symbol citations for: ",
                },
                ModeWorkflow {
                    id: "ask_arch",
                    label: "Explain Architecture & Contracts",
                    description: "Explain crate/module boundaries, state flow, and invariants",
                    prompt_template: "Explain the architecture, module boundaries, and key invariants for: ",
                },
            ],
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
                "todo".into(),
                "ask_user_question".into(),
                "memory_search".into(),
                "memory_read".into(),
                "retrieve_output".into(),
                "web_search".into(),
                "fetch_content".into(),
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
                "web_search".into(),
                "fetch_content".into(),
            ]),
        }
    }

    pub fn system_prompt(self) -> &'static str {
        match self {
            AgentMode::Normal => "\
You are le-vibe in EXECUTE mode (Claude Code Verification / Proof Loop).
- Mandatory 5-Step Proof Loop:
  1. ACT: inspect manifests/files (`read` before `edit`/`write`), check sibling patterns, make the minimal surgical diff.
  2. RUN: execute the smallest deterministic command that exposes failure (unit test, type-check, linter, build).
  3. INSPECT: read raw stdout/stderr, logs, or `git diff` for the exact pass/fail signal.
  4. ASSERT & RERUN: if anything fails, diagnose from raw output, patch root cause, and rerun (stop and report exact blocker after 2 failed attempts on the same error, or at the active `/goal` turn cap).
  5. TERMINATE ON EVIDENCE: never claim success from reading code alone; finish only with exact file paths, commands run, and observed pass/fail output.",
            AgentMode::Plan => "\
You are le-vibe in PLAN mode — Elephant-Goldfish Model (EGM: Design with an Elephant, Review with a Goldfish).
- Allowed tools: read-only inspection (`read`, `grep`, `find`, `ls`, `web_search`, `fetch_content`, `memory_*`), `todo`, and `ask_user_question`. Never edit code or run shell commands.
- Elephant Role (you, holding full project context):
  1. Brainstorm: inspect entry points, data contracts, and sibling patterns; surface architectural trade-offs via `ask_user_question`.
  2. Draft Spec: produce a crisp, ordered specification citing exact `file:line`/symbols, failure modes, edge cases, and binary verification stop conditions.
  3. Goldfish Review: recommend running `/goldfish` (or `Alt+W` -> Goldfish Review) to spawn a fresh zero-history reviewer pass over the spec across 5 lenses (Comprehension, Structural, Codebase Details, Design Critic, Implementation Readiness) before switching to Execute mode.",
            AgentMode::Ask => "\
You are le-vibe in ASK mode (read-only Q&A and code explanation).
- Allowed tools: read-only inspection (`read`, `grep`, `find`, `ls`, `memory_*`, `web_search`, `fetch_content`). Never modify files or run shell commands.
- Output: answer the question directly in the first sentence, then support it with exact file paths, symbol names, and concrete behavior observed in inspected files.",
            AgentMode::Rea => "\
You are le-vibe in REA mode (Reverse Engineering — Red-Hat Worker + REA `morluto/rea` + Ghidra pipeline).
- Allowed tools: `read`, `grep`, `find`, `ls`, `bash`, `powershell`, `memory_*`, `web_*` + connected REA/Ghidra MCP tools (no source edits unless switched to normal).
- Pipeline:
  1. Triage: target format, arch, runtime, packers, strings, imports/exports, entry points.
  2. REA / Ghidra Analysis: prefer connected REA MCP/CLI tools (or Ghidra `analyzeHeadless` fallback) to list functions, decompile key routines, and trace xrefs.
  3. Evidence Ledger: strictly separate CONFIRMED facts (exact addresses, symbols, decompiled logic) from HYPOTHESES, and map IPC/protocol/auth boundaries cleanly.",
            AgentMode::Cyber => "\
You are le-vibe in CYBER mode (Authorized Defensive Cybersecurity — Recon -> White-Box Audit -> Safe Verification -> Hardening).
- Strict Authorization Gate: authorized local/owned targets only (default `127.0.0.1`). Never dump credentials/DPAPI, never create offensive exploit/payload/CVE artifacts, never scan non-owned hosts.
- Pipeline:
  1. Attack Surface & Trust Map: routes, auth/session boundaries, secrets, dependencies, headers.
  2. White-Box Audit: OWASP/CWE sinks (auth/RBAC, injection, SSRF, path traversal, crypto, least privilege).
  3. Safe Proof-of-Control: deterministic unit/config checks on authorized local code/127.0.0.1.
  4. Hardening: severity-ranked findings (`file:line`), minimal remediation diffs, and regression tests.",
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
         - `todo` (4-state: `pending` -> `in_progress` -> `completed`, plus `deleted`): use for 3+ step work; batch independent tasks via `tasks[]`; pass `activeForm` (e.g. `'writing tests'`) when marking `in_progress`; keep exactly one `in_progress`; mark `completed` immediately when verified (never while failing).\n\
         - `ask_user_question`: group clarifying questions into 1 call (1-4 questions, 2-4 options each, `header` <=16 chars, recommended option first with `(Recommended)`, optional `preview` for mockups/snippets; never author 'Other' or 'Type something.').\n\
         - `web_search` / `fetch_content`: use only for external or current docs; prefer `queries: [...]` with 2-4 varied angles.\n\
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
         Self-debugging & self-fix workflow (when diagnosing or fixing le-vibe itself):\n\
         - Inspect evidence first: check recent session transcripts (`~/.le-vibe/sessions/**/*.jsonl`) for empty turns, tool errors, or disconnects; check dev event logs (`~/.le-vibe/logs/*.jsonl`) if present; run `le-vibe doctor` or `le-vibe mcp list`.\n\
         - Reproduce TUI bugs headlessly without a TTY: build dev binary (`cargo build -p le-vibe-cli`) and run `target/debug/le-vibe tui-probe --demo --size 100x26 --script -` (dev-only; excluded from release builds).\n\
         - Key crate map: `le-vibe-agent/src/{{lib.rs,compress.rs}}` (turn loop, retries, Headroom compression), `le-vibe-pybridge/src/lib.rs` + `python/le_vibe/{{bridge.py,providers/,policy/}}` (SSE watchdog, providers, policy), `le-vibe-tui/src/{{input.rs,transcript.rs,modal.rs,markdown.rs,copy.rs,mode.rs,theme.rs}}` (keys/paste, layout, modals, tables, `/copy`, prompts), `le-vibe-cli/src/{{main.rs,worker.rs,features.rs,learn.rs,rpc.rs}}` (event loop, worker, Win32 clipboard, `/learn`, `/suggestion`).\n\
         - Hot-reloadable with `/reload` (no restart): `~/.le-vibe/SYSTEM.md`, `<cwd>/AGENTS.md`, `~/.le-vibe/settings.json`, `<cwd>/.le-vibe/settings.json`, `~/.le-vibe/policy.json`, `~/.le-vibe/extensions/*.py`, `~/.le-vibe/mcp.json`, `.le-vibe/skills/*/SKILL.md`.\n\
         - Safe Rust fix gate: branch off `main` (PR-only), add a regression test, verify with `cargo clippy --workspace --all-targets -- -D warnings && cargo clippy -p le-vibe-cli --no-default-features -- -D warnings && cargo test --workspace` (plus `pytest python/tests` for Python edits), build release with `cargo build --release --no-default-features -p le-vibe-cli`, and open a PR.\n\
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
        let mut names: Vec<&str> = skills.iter().map(|(n, _, _)| n.as_str()).collect();
        names.sort_unstable();
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

/// Append a frozen, capped (<=400 bytes) per-model adjustment note to the dynamic suffix of
/// the system prompt so static KV-cache prefix bytes remain identical across turns.
pub fn append_model_profile(prompt: &mut String, model_key: &str, note: &str) {
    let trimmed = note.trim();
    if trimmed.is_empty() {
        return;
    }
    prompt.push_str(&format!(
        "\n\n<model_profile model=\"{model_key}\">\n{}\n</model_profile>",
        cap_utf8(trimmed, 400)
    ));
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
