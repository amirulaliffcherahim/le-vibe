//! System-prompt contract: a short operating manual, not a wall of rules.
//!
//! These tests exist because prompt regressions are invisible until a model starts ignoring rules.
//! They pin the *structure* (cache-friendly ordering, rules at top and bottom) and the *budget*
//! (the static core stays small enough that the middle is still read).

use lazybug_tui::{AgentMode, ALL_MODES, build_system_prompt};

/// Static core = role + contract + tools + environment + mode block, i.e. the part that does not
/// depend on user files (AGENTS.md, SYSTEM.md, skills, memory). Measuring only this keeps the
/// budget test deterministic across machines.
fn static_core(mode: AgentMode, cwd: &str) -> String {
    let full = build_system_prompt(mode, "powershell", cwd, "C:/somewhere/else", &[], "");
    match full.find("</mode>") {
        Some(idx) => full[..idx + "</mode>".len()].to_string(),
        None => full,
    }
}

fn words(s: &str) -> usize {
    s.split_whitespace().count()
}

#[test]
fn static_core_stays_small() {
    for mode in ALL_MODES {
        let core = static_core(*mode, "C:/work/demo");
        let w = words(&core);
        assert!(
            w <= 420,
            "{} mode static core is {w} words ({} chars); the budget is 420. Trim it or move \
             detail into a skill:\n{core}",
            mode.name(),
            core.len()
        );
    }
}

#[test]
fn rules_sit_at_the_top_and_bottom() {
    let core = static_core(AgentMode::Normal, "C:/work/demo");
    assert!(
        core.starts_with("You are lazybug"),
        "the prompt must open with the role line"
    );
    let contract = core.find("<operating_contract>").expect("contract block");
    let mode_block = core.find("<mode name=").expect("mode block");
    assert!(
        contract < mode_block,
        "the shared contract must precede mode-specific text"
    );
    let full = build_system_prompt(AgentMode::Normal, "powershell", "C:/work/demo", "", &[], "");
    assert!(
        full.trim_end().ends_with("</non_negotiables>"),
        "the reminder block must be the very last thing: {}",
        &full[full.len().saturating_sub(120)..]
    );
}

#[test]
fn the_workflow_section_is_phrased_positively() {
    let core = static_core(AgentMode::Normal, "C:/work/demo");
    let start = core.find("<operating_contract>").unwrap();
    let end = core.find("</operating_contract>").unwrap();
    let contract = &core[start..end];
    for banned in ["Don't", "Do not", "Never", "Avoid"] {
        assert!(
            !contract.contains(banned),
            "prefer positive instructions in the workflow section, found {banned:?}:\n{contract}"
        );
    }
}

#[test]
fn switching_mode_keeps_a_long_shared_prefix() {
    // Prompt caching only helps if the mode block is near the end of the stable text.
    let normal = build_system_prompt(AgentMode::Normal, "powershell", "C:/work/demo", "", &[], "");
    let plan = build_system_prompt(AgentMode::Plan, "powershell", "C:/work/demo", "", &[], "");
    let shared = normal
        .bytes()
        .zip(plan.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    assert!(
        shared > 800,
        "only {shared} bytes shared between normal and plan prompts"
    );
}

#[test]
fn self_repo_guidance_only_loads_inside_the_repo() {
    let repo = "C:/Users/amiru/lazybug";
    let inside = build_system_prompt(AgentMode::Normal, "powershell", "C:/Users/amiru/lazybug/crates/lazybug-tui", repo, &[], "");
    assert!(
        inside.contains("<self_repo>"),
        "self-debugging guidance should load when working on lazybug"
    );

    let outside = build_system_prompt(AgentMode::Normal, "powershell", "C:/work/client-app", repo, &[], "");
    assert!(
        !outside.contains("<self_repo>"),
        "self-debugging guidance must not cost tokens in other repos"
    );
    assert!(
        words(&outside) < words(&inside),
        "the outside-repo prompt should be strictly smaller"
    );
}

#[test]
fn volatile_blocks_come_after_stable_context() {
    let skills = vec![("demo-skill".to_string(), "d".to_string(), "p".to_string())];
    let prompt = build_system_prompt(
        AgentMode::Normal,
        "powershell",
        "C:/work/demo",
        "",
        &skills,
        "- [a] memory title",
    );
    let env = prompt.find("</environment>").unwrap();
    let skills_at = prompt.find("<skills>").unwrap();
    let memory_at = prompt.find("<memory_toc>").unwrap();
    let reminders = prompt.find("<non_negotiables>").unwrap();
    assert!(env < skills_at, "skills are dynamic, keep them after the core");
    assert!(
        skills_at < memory_at,
        "memory_toc changes most often, so it goes last before the reminders"
    );
    assert!(memory_at < reminders, "reminders stay last");
}

#[test]
fn every_mode_states_its_own_method_and_write_permission() {
    for mode in ALL_MODES {
        assert!(!mode.system_prompt().is_empty(), "{} has no method", mode.name());
        let core = static_core(*mode, "C:/work/demo");
        assert!(
            core.contains(&format!("<mode name=\"{}\">", mode.name())),
            "{} mode block missing",
            mode.name()
        );
    }
    // Read-only modes cannot write; execute modes can.
    assert!(!AgentMode::Plan.can_write());
    assert!(!AgentMode::Ask.can_write());
    assert!(AgentMode::Normal.can_write());
    assert!(AgentMode::Rea.can_write());
    assert!(AgentMode::Cyber.can_write());
}

#[test]
fn trivial_inputs_are_told_to_answer_without_tools() {
    // A one-word prompt must not become a filesystem raid: the cost of one unnecessary turn is
    // larger than any context saving, and a 4-call spiral on "test" is pure waste.
    let core = static_core(AgentMode::Normal, "C:/work/demo");
    assert!(
        core.contains("no tool calls"),
        "the contract must authorise a zero-tool reply: {core}"
    );
    assert!(
        core.contains("1-2 word"),
        "the contract must name trivial one- or two-word inputs: {core}"
    );
}

#[test]
fn the_environment_block_marks_whether_the_cwd_is_a_repo() {
    let dir = std::env::temp_dir().join(format!("lazybug-nonrepo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cwd = dir.display().to_string();

    let outside = build_system_prompt(AgentMode::Normal, "powershell", &cwd, "", &[], "");
    assert!(
        outside.contains("workspace=not a git repo"),
        "starting in a plain directory must be stated so the model does not scan for a project"
    );

    std::fs::create_dir_all(dir.join(".git")).unwrap();
    let inside = build_system_prompt(AgentMode::Normal, "powershell", &cwd, "", &[], "");
    assert!(inside.contains("workspace=git repo"), "{inside}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(windows)]
#[test]
fn windows_hints_name_powershell_and_rule_out_unix_tools() {
    let core = static_core(AgentMode::Normal, "powershell");
    assert!(core.contains("PowerShell cmdlets"), "{core}");
    assert!(
        core.contains("Unix `tail`/`head`/`sed`/`grep` are unavailable"),
        "{core}"
    );
}

#[test]
fn codemode_rule_directs_localized_work_to_native_tools_first() {
    let core = static_core(AgentMode::Normal, "C:/work/demo");
    let start = core.find("<operating_contract>").unwrap();
    let end = core.find("</operating_contract>").unwrap();
    let contract = &core[start..end];
    assert!(
        contract.contains("single-file reads") && contract.contains("localized edits"),
        "Rule 7 must direct simple reads/edits to native tools: {contract}"
    );
    assert!(
        contract.contains("reserve `codemode` for multi-file sweeps"),
        "Rule 7 must reserve codemode for sweeps/log filtering/batch transforms: {contract}"
    );
    assert!(
        !contract.contains("\n\n8."),
        "Rule 7 must not emit a stray blank line before Rule 8: {contract}"
    );
}

#[test]
fn workspace_block_sits_after_instructions_so_cd_preserves_cache_prefix() {
    let skills = vec![("demo-skill".to_string(), "d".to_string(), "p".to_string())];
    let p1 = build_system_prompt(
        AgentMode::Normal,
        "powershell",
        "C:/work/alpha",
        "",
        &skills,
        "- [a] mem",
    );
    let p2 = build_system_prompt(
        AgentMode::Normal,
        "powershell",
        "C:/work/beta",
        "",
        &skills,
        "- [a] mem",
    );

    let env_start = p1.find("<environment>").unwrap();
    let env_end = p1.find("</environment>").unwrap();
    let env_block = &p1[env_start..env_end];
    assert!(
        !env_block.contains("cwd=") && !env_block.contains("workspace="),
        "static <environment> must not contain volatile cwd/workspace: {env_block}"
    );

    let skills_end = p1.find("</skills>").unwrap() + "</skills>".len();
    let ws_at = p1.find("<workspace>").expect("volatile <workspace> block");
    let mem_at = p1.find("<memory_toc>").unwrap();
    let rem_at = p1.find("<non_negotiables>").unwrap();
    assert!(skills_end < ws_at, "<workspace> must come after <skills>");
    assert!(ws_at < rem_at, "<workspace> must come before <non_negotiables>");
    assert!(mem_at < rem_at);

    // Changing cwd must keep the entire prefix through </skills> byte-identical.
    assert_eq!(
        &p1[..skills_end],
        &p2[..skills_end],
        "changing cwd mutated the cacheable prefix before </skills>"
    );
}

#[test]
fn core_tools_block_omits_unloaded_memory_and_web_instructions() {
    use lazybug_tui::{ModeSpec, build_system_prompt_for_spec, parse_mode_markdown};

    let core_prompt = build_system_prompt(AgentMode::Normal, "powershell", "C:/work/demo", "", &[], "");
    let tools_start = core_prompt.find("<tools>").unwrap();
    let tools_end = core_prompt.find("</tools>").unwrap();
    let tools_block = &core_prompt[tools_start..tools_end];
    assert!(
        !tools_block.contains("memory_search") && !tools_block.contains("memory_write"),
        "default core <tools> must omit memory_* instructions when unloaded:\n{tools_block}"
    );
    assert!(
        !tools_block.contains("web_search") && !tools_block.contains("fetch_content"),
        "default core <tools> must omit web_* instructions when unloaded:\n{tools_block}"
    );

    // When a mode enables web + memory, those tool hints appear.
    let custom: ModeSpec = parse_mode_markdown(
        "research",
        "---\ndescription: Research\nextensions: [web]\ntools:\n  memory_search: true\n---\nResearch.",
    );
    let custom_prompt =
        build_system_prompt_for_spec(&custom, "powershell", "C:/work/demo", "", &[], "");
    let c_start = custom_prompt.find("<tools>").unwrap();
    let c_end = custom_prompt.find("</tools>").unwrap();
    let c_tools = &custom_prompt[c_start..c_end];
    assert!(c_tools.contains("web_search") && c_tools.contains("fetch_content"));
    assert!(c_tools.contains("memory_search") && c_tools.contains("memory_write"));
}

