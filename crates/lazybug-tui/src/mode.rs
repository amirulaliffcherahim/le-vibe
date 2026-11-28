use std::collections::BTreeMap;

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
                "retrieve_output".into(),
                "load_skill".into(),
            ]),
            AgentMode::Rea => Some(vec![
                "read".into(),
                "grep".into(),
                "find".into(),
                "ls".into(),
                "bash".into(),
                "powershell".into(),
                "retrieve_output".into(),
                "load_skill".into(),
            ]),
        }
    }

    /// Mode-specific method. Kept short and positive: the shared rules live in
    /// `<operating_contract>` (top) and `<non_negotiables>` (bottom) of `build_system_prompt`.
    pub fn system_prompt(self) -> &'static str {
        match self {
            AgentMode::Normal => "Execute: locate the exact code, make the minimal surgical diff, prove it with a real command, report the evidence.",
            AgentMode::Plan => "Plan: investigate read-only, then produce a file-by-file spec with verification steps. Make zero edits.",
            AgentMode::Ask => "Answer: explain what the code does. Make zero edits. Answer in the first sentence, then cite `file:line`.",
            AgentMode::Rea => "Reverse-engineer: triage the artifact, decompile what matters, and separate CONFIRMED facts from HYPOTHESES.",
            AgentMode::Cyber => "Defensive security on authorized targets: map trust boundaries, audit sinks, verify safely, then harden.",
        }
    }

    /// Extra method detail for modes that have a real pipeline (plan/rea/cyber).
    /// Normal/ask add nothing: their method line plus the shared contract is enough.
    pub fn mode_steps(self) -> &'static str {
        match self {
            AgentMode::Normal => "",
            AgentMode::Plan => "\
- Brainstorm: inspect entry points, data contracts and sibling patterns; surface trade-offs with `ask_user_question`.
- Draft the spec: ordered steps citing exact `file:line`/symbols, failure modes, edge cases and binary stop conditions.
- Offer `/goldfish` (or `Alt+W`) for a fresh zero-history review before switching to Execute.",
            AgentMode::Ask => "",
            AgentMode::Rea => "\
1. Triage: format, arch, runtime, packers, strings, imports/exports, entry points.
2. Prefer connected REA/Ghidra tools (or Ghidra `analyzeHeadless`) to list functions, decompile key routines and trace xrefs.
3. Evidence ledger: exact addresses/symbols for CONFIRMED, explicitly labelled HYPOTHESIS for the rest; map IPC/protocol/auth boundaries.",
            AgentMode::Cyber => "\
1. Attack surface and trust map: routes, auth/session boundaries, secrets, dependencies, headers.
2. White-box audit: OWASP/CWE sinks (auth/RBAC, injection, SSRF, path traversal, crypto, least privilege).
3. Safe proof-of-control: deterministic unit/config checks on local code or `127.0.0.1`.
4. Harden: severity-ranked findings with `file:line`, minimal remediation diffs, regression tests.",
        }
    }

    /// Whether the mode can change files. Drives the `self_repo`/tool wording.
    pub fn can_write(self) -> bool {
        !matches!(self, AgentMode::Plan | AgentMode::Ask)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeScope {
    Primary,
    Subagent,
    All,
}

impl ModeScope {
    pub fn from_str_lossy(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "primary" => Self::Primary,
            "subagent" => Self::Subagent,
            _ => Self::All,
        }
    }

    pub fn is_primary(self) -> bool {
        !matches!(self, Self::Subagent)
    }

    pub fn is_subagent(self) -> bool {
        !matches!(self, Self::Primary)
    }
}

/// Editable persona / mode specification (built-in or loaded from `~/.lazybug/modes/*.md` or
/// `<repo>/.lazybug/modes/*.md` with YAML frontmatter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeSpec {
    pub id: String,
    pub description: String,
    pub scope: ModeScope,
    pub can_write: bool,
    pub verify_edits: Option<bool>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub max_turns: Option<usize>,
    pub tools: BTreeMap<String, bool>,
    pub extensions: Vec<String>,
    pub body: String,
    pub builtin: Option<AgentMode>,
}

const ALL_BUILTIN_TOOL_NAMES: &[&str] = &[
    "read",
    "write",
    "edit",
    "grep",
    "find",
    "ls",
    "bash",
    "powershell",
    "retrieve_output",
    "load_skill",
    "codemode",
];

const READONLY_TOOL_NAMES: &[&str] = &[
    "read",
    "grep",
    "find",
    "ls",
    "retrieve_output",
    "load_skill",
];

impl ModeSpec {
    pub fn from_builtin(mode: AgentMode) -> Self {
        let steps = mode.mode_steps();
        let body = if steps.is_empty() {
            mode.system_prompt().to_string()
        } else {
            format!("{}\n{steps}", mode.system_prompt())
        };
        Self {
            id: mode.name().to_string(),
            description: mode.description().to_string(),
            scope: ModeScope::All,
            can_write: mode.can_write(),
            verify_edits: None,
            model: None,
            thinking: None,
            max_turns: None,
            tools: BTreeMap::new(),
            extensions: Vec::new(),
            body,
            builtin: Some(mode),
        }
    }

    pub fn builtins() -> Vec<Self> {
        vec![Self::from_builtin(AgentMode::Normal)]
    }

    /// Resolve the tool allowlist for this mode (`None` = all registered tools allowed).
    pub fn resolve_allowed_tools(&self) -> Option<Vec<String>> {
        if self.tools.is_empty() {
            if let Some(b) = self.builtin
                && self.can_write == b.can_write()
            {
                return b.allowed_tools();
            }
            if !self.can_write {
                return Some(READONLY_TOOL_NAMES.iter().map(|s| (*s).to_string()).collect());
            }
            return None;
        }
        let base: Vec<&str> = if self.can_write {
            ALL_BUILTIN_TOOL_NAMES.to_vec()
        } else {
            READONLY_TOOL_NAMES.to_vec()
        };
        let mut allowed: std::collections::BTreeSet<String> =
            base.into_iter().map(str::to_string).collect();
        for (name, enabled) in &self.tools {
            if *enabled {
                allowed.insert(name.clone());
            } else {
                allowed.remove(name);
            }
        }
        Some(allowed.into_iter().collect())
    }
}

fn strip_inline_comment(val: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    for (idx, ch) in val.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return val[..idx].trim(),
            _ => {}
        }
    }
    val.trim()
}

fn unquote_yaml(val: &str) -> String {
    let s = strip_inline_comment(val).trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Parse a Markdown mode file with optional YAML frontmatter (`--- ... ---`).
/// Pure Rust, zero external YAML crate needed.
pub fn parse_mode_markdown(id: &str, raw: &str) -> ModeSpec {
    let clean_id = id.trim().to_ascii_lowercase();
    let mut spec = AgentMode::from_name(&clean_id)
        .map(ModeSpec::from_builtin)
        .unwrap_or_else(|| ModeSpec {
            id: clean_id.clone(),
            description: format!("Custom mode `{clean_id}`"),
            scope: ModeScope::All,
            can_write: true,
            verify_edits: None,
            model: None,
            thinking: None,
            max_turns: None,
            tools: BTreeMap::new(),
            extensions: Vec::new(),
            body: String::new(),
            builtin: None,
        });

    let normalized = raw.replace("\r\n", "\n");
    let trimmed = normalized.trim_start_matches('\u{feff}');
    let (frontmatter, body) = if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let after = &rest[end + 4..];
            let body = after.strip_prefix('\n').unwrap_or(after).trim();
            (Some(fm), body)
        } else {
            (None, trimmed.trim())
        }
    } else {
        (None, trimmed.trim())
    };

    if !body.is_empty() {
        spec.body = body.to_string();
    }

    if let Some(fm) = frontmatter {
        let mut in_tools = false;
        let mut in_extensions = false;
        for line in fm.lines() {
            let raw_trimmed = line.trim();
            if raw_trimmed.is_empty() || raw_trimmed.starts_with('#') {
                continue;
            }
            let indented = line.starts_with(' ') || line.starts_with('\t');
            if in_tools && indented {
                if let Some((k, v)) = raw_trimmed.split_once(':') {
                    let tool_name = unquote_yaml(k);
                    let val = unquote_yaml(v).to_ascii_lowercase();
                    if !tool_name.is_empty() && matches!(val.as_str(), "true" | "false") {
                        spec.tools.insert(tool_name, val == "true");
                    }
                }
                continue;
            }
            if in_extensions && indented {
                let item = unquote_yaml(raw_trimmed.trim_start_matches('-').trim());
                if !item.is_empty() && !spec.extensions.contains(&item) {
                    spec.extensions.push(item);
                }
                continue;
            }
            in_tools = false;
            in_extensions = false;
            if let Some((k, v)) = raw_trimmed.split_once(':') {
                let key = k.trim().to_ascii_lowercase();
                let val = unquote_yaml(v);
                match key.as_str() {
                    "description" if !val.is_empty() => spec.description = val,
                    "scope" if !val.is_empty() => spec.scope = ModeScope::from_str_lossy(&val),
                    "can_write" => {
                        if matches!(val.to_ascii_lowercase().as_str(), "true" | "false") {
                            spec.can_write = val.eq_ignore_ascii_case("true");
                        }
                    }
                    "verify_edits" => {
                        if matches!(val.to_ascii_lowercase().as_str(), "true" | "false") {
                            spec.verify_edits = Some(val.eq_ignore_ascii_case("true"));
                        }
                    }
                    "model" if !val.is_empty() => spec.model = Some(val),
                    "thinking" if !val.is_empty() => spec.thinking = Some(val),
                    "max_turns" => {
                        if let Ok(n) = val.parse::<usize>()
                            && n > 0
                        {
                            spec.max_turns = Some(n);
                        }
                    }
                    "tools" => {
                        in_tools = true;
                    }
                    "extensions" => {
                        if val.is_empty() {
                            in_extensions = true;
                        } else {
                            for part in val.trim_matches(['[', ']']).split(',') {
                                let ext = unquote_yaml(part);
                                if !ext.is_empty() && !spec.extensions.contains(&ext) {
                                    spec.extensions.push(ext);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    spec
}

pub fn build_system_prompt(
    mode: AgentMode,
    shell: &str,
    cwd: &str,
    self_repo: &str,
    skills: &[(String, String, String)],
    memory_toc: &str,
) -> String {
    build_system_prompt_for_spec(
        &ModeSpec::from_builtin(mode),
        shell,
        cwd,
        self_repo,
        skills,
        memory_toc,
    )
}

pub fn build_system_prompt_for_spec(
    spec: &ModeSpec,
    shell: &str,
    cwd: &str,
    self_repo: &str,
    skills: &[(String, String, String)],
    memory_toc: &str,
) -> String {
    let os = std::env::consts::OS;
    // Tell the model which shell dialect is real here: a Unix-flavoured hint primes `tail`/`grep`
    // on Windows and burns a whole turn on a guaranteed error.
    let shell_hint = if cfg!(windows) {
        "- Use PowerShell cmdlets or the Python stdlib inside `codemode`; Unix `tail`/`head`/`sed`/`grep` \
are unavailable.\n"
    } else {
        ""
    };
    let has_memory = spec.tools.get("memory_search") == Some(&true)
        || spec.tools.get("memory_write") == Some(&true)
        || spec.extensions.iter().any(|e| e.eq_ignore_ascii_case("memory"));
    let has_web = spec.tools.get("web_search") == Some(&true)
        || spec.tools.get("fetch_content") == Some(&true)
        || spec.extensions.iter().any(|e| e.eq_ignore_ascii_case("web"));
    let memory_hint = if has_memory {
        "- `memory_search` when past decisions, gotchas or user preferences may apply. `memory_write` only \
for durable decisions, constraints, preferences or bug root causes (stable `key`, 3-6 synonym \
`keywords`) - never for task chatter.\n"
    } else {
        ""
    };
    let web_hint = if has_web {
        "- `web_search` / `fetch_content` only for external or current documentation.\n"
    } else {
        ""
    };
    // A non-repo start directory is a strong hint that the task is not a codebase sweep.
    let repo_state = if std::path::Path::new(cwd).join(".git").exists() {
        "git repo"
    } else {
        "not a git repo"
    };
    // Order is deliberate: the cacheable core goes first (KV-cache prefix), then per-repo context,
    // then the most volatile blocks (`<skills>`, `<memory_toc>`) last, so a memory write or a new
    // skill does not invalidate the prefix for everything before it.
    let mut prompt = format!(
        "You are lazybug, a terminal coding agent. Solve the user's task directly, accurately, and \
with minimal wasted turns.\n\n\
<operating_contract>\n\
1. Locate before reading: use `grep` or `read` with `offset`/`limit` to inspect only the relevant \
files, and read a file before editing it.\n\
2. Act on a clear, localized change; state a 3-5 bullet plan first only when the change spans \
modules or carries architectural risk.\n\
3. Prefer `edit` over rewriting a whole file. Preserve existing style and unrelated behavior.\n\
4. Prove every code or config edit with the narrowest relevant check (unit test, linter, build, or \
reproduction script).\n\
5. Base every claim on actual tool output. Report exact exit codes and failing lines.\n\
6. Batch independent lookups into one turn (parallel `grep`/`read`): every step re-sends the \
transcript, so an extra turn costs more than extra bytes. Read enough that `edit` stays unambiguous.\n\
7. Use `read`, `edit`, and `powershell`/`bash` directly for single-file reads, localized edits, and \
single test runs; reserve `codemode` for multi-file sweeps, log filtering, or scripted batch \
transforms whose raw output would bloat context.\n\
8. After three attempts at the same failure, stop patching: restate the task, list assumptions that \
may be wrong, and change approach.\n\
9. Match effort to the ask: greetings, connectivity checks, general questions and 1-2 word inputs \
get a direct answer with no tool calls, plus one focused question when intent is unclear.\n\
</operating_contract>\n\n\

<tools>\n\
- Shell commands must be valid for `{shell}` on `{os}`.\n\
{shell_hint}\
- Compressed tool output keeps a reference id: call `retrieve_output` only when the missing lines \
are needed for a decision.\n\
{memory_hint}\
{web_hint}\
- Style: plain language, ready-to-run commands and code in fenced blocks, compact tables, no \
filler or tool narration.\n\
</tools>\n\n\
<environment>\n\
os={os}, active_shell={shell}\n\
</environment>\n\n\
<mode name=\"{}\">\n\
{}\n\
</mode>\n\n",
        spec.id,
        spec.body,
    );

    // Self-debugging guidance is only worth its tokens when working inside lazybug itself.
    let norm = |p: &str| p.replace('\\', "/").to_lowercase();
    let is_self_repo = !self_repo.trim().is_empty() && norm(cwd).starts_with(&norm(self_repo));
    if is_self_repo {
        prompt.push_str(&format!(
            "\n\n<self_repo>\n\
lazybug source at `{self_repo}`: Rust harness in `crates/*`, Python providers/policy/extensions in \
`python/lazybug/*`, docs in `docs/*.md`.\n\
- Evidence first: session transcripts `~/.lazybug/sessions/**/*.jsonl`, dev logs \
`~/.lazybug/logs/*.jsonl`, `lazybug doctor`, `lazybug mcp list`.\n\
- Reproduce TUI bugs headlessly: `cargo build -p lazybug-cli` then \
`target/debug/lazybug tui-probe --demo --size 100x26 --script -` (dev-only).\n\
- Crate map: agent (turn loop, retries, compression), pybridge + `python/lazybug` (providers, \
policy, SSE watchdog), tui (keys, layout, modals, markdown, prompts), cli (event loop, worker, \
`/learn`, `/suggestion`).\n\
- `/reload` hot-reloads SYSTEM.md, AGENTS.md, settings, policy, extensions, mcp.json, skills.\n\
- Rust gate: branch off `main`, add a regression test, then `cargo clippy --workspace --all-targets \
-- -D warnings`, `cargo clippy -p lazybug-cli --no-default-features -- -D warnings`, \
`cargo test --workspace`, `pytest python/tests`; release with `--release --no-default-features`.\n\
</self_repo>"
        ));
    }

    // On-demand skill *names* only: the bodies load via `load_skill`, so the index costs ~15 tokens.
    if !skills.is_empty() {
        let mut names: Vec<&str> = skills.iter().map(|(n, _, _)| n.as_str()).collect();
        names.sort_unstable();
        prompt.push_str(&format!(
            "\n\n<skills>\nCall `load_skill` with a name only when it is relevant to the task: {}\n</skills>",
            names.join(", ")
        ));
    }

    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let user_sys = std::path::Path::new(&home).join(".lazybug").join("SYSTEM.md");
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
            .join(".lazybug")
            .join("APPEND_SYSTEM.md");
        let user_append = std::path::Path::new(&home)
            .join(".lazybug")
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
        // Bounded runtime rules from `/optimize`: frozen at session start (reloaded only on `/reload`).
        let learned_path = std::path::Path::new(&home)
            .join(".lazybug")
            .join("LEARNED_USAGE.md");
        if let Ok(raw) = std::fs::read_to_string(&learned_path)
            && !raw.trim().is_empty()
        {
            prompt.push_str(&format!(
                "\n\n<learned_usage path=\"{}\">\n{}\n</learned_usage>",
                learned_path.display(),
                cap_utf8(&raw, 1500)
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

    // Volatile workspace metadata sits after static instructions so a `cd` does not
    // invalidate `<mode>`, `<skills>`, or static instruction files in the KV cache prefix.
    prompt.push_str(&format!(
        "\n\n<workspace>\ncwd={cwd}, workspace={repo_state}\n</workspace>"
    ));

    // Most volatile block last: a memory write mid-session must not invalidate the prefix above it.
    if !memory_toc.trim().is_empty() {
        prompt.push_str(&format!(
            "\n\n<memory_toc>\nStored long-term memories (call `memory_search` or `memory_read` for details):\n{}\n</memory_toc>",
            cap_utf8(memory_toc, 1400)
        ));
    }

    // Recency: the rules that decay fastest in a long turn, restated at the very end.
    prompt.push_str(
        "\n\n<non_negotiables>\n\
1. Run a real check after editing, then report the command and its output.\n\
2. Change only what the task requires; keep the diff minimal.\n\
3. Batch independent reads and searches into one turn; each extra step re-bills the transcript.\n\
4. When blocked, report what you checked and ask one concise question instead of guessing.\n\
5. Never fabricate test results, file paths, or tool output.\n\
</non_negotiables>",
    );

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

#[cfg(test)]
mod mode_spec_tests {
    use super::*;

    #[test]
    fn parses_frontmatter_tools_and_body() {
        let raw = "\
---
description: Read-only code & usage reviewer
scope: subagent           # primary | subagent | all
can_write: false
verify_edits: false
model: vibe/mistral-large-4-0
thinking: medium
max_turns: 20
tools:
  read: true
  grep: true
  edit: false
  powershell: false
---
Review the diff for regressions. Cite exact file:line. Make zero edits.
";
        let spec = parse_mode_markdown("reviewer", raw);
        assert_eq!(spec.id, "reviewer");
        assert_eq!(spec.description, "Read-only code & usage reviewer");
        assert_eq!(spec.scope, ModeScope::Subagent);
        assert!(!spec.can_write);
        assert_eq!(spec.verify_edits, Some(false));
        assert_eq!(spec.model.as_deref(), Some("vibe/mistral-large-4-0"));
        assert_eq!(spec.thinking.as_deref(), Some("medium"));
        assert_eq!(spec.max_turns, Some(20));
        assert_eq!(spec.tools.get("read"), Some(&true));
        assert_eq!(spec.tools.get("edit"), Some(&false));
        assert!(spec.body.contains("Review the diff"));

        let allowed = spec.resolve_allowed_tools().expect("filtered list");
        assert!(allowed.contains(&"read".to_string()));
        assert!(allowed.contains(&"grep".to_string()));
        assert!(!allowed.contains(&"edit".to_string()));
        assert!(!allowed.contains(&"powershell".to_string()));
    }

    #[test]
    fn custom_mode_keeps_core_kv_cache_prefix_identical_to_builtin() {
        let custom = parse_mode_markdown(
            "reviewer",
            "---\ndescription: Reviewer\ncan_write: false\n---\nAudit only.",
        );
        let p_norm = build_system_prompt(AgentMode::Normal, "powershell", "C:/w", "", &[], "");
        let p_rev = build_system_prompt_for_spec(&custom, "powershell", "C:/w", "", &[], "");
        let mode_at = p_norm.find("<mode name=").unwrap();
        assert_eq!(&p_norm[..mode_at], &p_rev[..mode_at]);
        assert!(p_rev.contains("<mode name=\"reviewer\">\nAudit only.\n</mode>"));
    }

    #[test]
    fn default_builtins_ship_only_normal_mode() {
        let builtins = ModeSpec::builtins();
        assert_eq!(builtins.len(), 1);
        assert_eq!(builtins[0].id, "normal");
    }

    #[test]
    fn shipped_example_modes_parse_cleanly() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("examples")
            .join("modes");
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("examples/modes exists")
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();
        assert!(!entries.is_empty(), "expected example mode files");
        for path in entries {
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap();
            let raw = std::fs::read_to_string(&path).unwrap();
            let spec = parse_mode_markdown(id, &raw);
            assert!(!spec.description.trim().is_empty(), "{id} empty description");
            assert!(!spec.body.trim().is_empty(), "{id} empty body");
        }
    }
}

