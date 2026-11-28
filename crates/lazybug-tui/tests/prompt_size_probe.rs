//! Prints the prompt budget so a change can be compared before/after.
//! Run with: `cargo test -p lazybug-tui --test prompt_size_probe -- --nocapture`

use lazybug_tui::{ALL_MODES, build_system_prompt};

#[test]
fn report_prompt_budget() {
    for mode in ALL_MODES {
        let full = build_system_prompt(*mode, "powershell", "C:/work/demo", "", &[], "");
        let core_end = full.find("</mode>").map(|i| i + 7).unwrap_or(full.len());
        let core = &full[..core_end];
        println!(
            "{:8} static_core={:4} words / {:5} chars | full_prompt={:4} words / {:5} chars",
            mode.name(),
            core.split_whitespace().count(),
            core.len(),
            full.split_whitespace().count(),
            full.len()
        );
    }
}
