//! One table for every keybinding shown in the UI.
//!
//! pi builds hints from the live keybinding registry (`keyHint(keybinding, description)` →
//! `dim(key) muted(" description")`) so a remap can never leave a stale hint in a tool card.
//! `le-vibe` has no user-remappable bindings yet, so the equivalent is a single static table:
//! every hint string in the TUI is derived from [`KeyAction`], never typed twice.

/// A user action that appears in on-screen hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    /// Expand/collapse tool output and raw XML.
    ExpandTools,
    /// Show/hide the thinking block.
    ToggleThinking,
    /// Cancel the running turn.
    Interrupt,
    /// Queue a follow-up prompt.
    QueueFollowUp,
    /// Jump back to the newest transcript line.
    ScrollEnd,
    /// Switch agent mode.
    CycleMode,
    /// Open a mode's workflow launcher.
    OpenWorkflows,
    /// Slash commands.
    SlashCommands,
    /// `@` file references.
    AtFiles,
    /// Quit the app.
    Quit,
}

impl KeyAction {
    /// Every action, so tests can assert none is left without a binding.
    pub const ALL: &'static [KeyAction] = &[
        KeyAction::ExpandTools,
        KeyAction::ToggleThinking,
        KeyAction::Interrupt,
        KeyAction::QueueFollowUp,
        KeyAction::ScrollEnd,
        KeyAction::CycleMode,
        KeyAction::OpenWorkflows,
        KeyAction::SlashCommands,
        KeyAction::AtFiles,
        KeyAction::Quit,
    ];

    /// The literal key chord, exactly as the input handler matches it.
    pub fn key(self) -> &'static str {
        match self {
            KeyAction::ExpandTools => "ctrl+o",
            KeyAction::ToggleThinking => "ctrl+t",
            KeyAction::Interrupt => "Esc",
            KeyAction::QueueFollowUp => "Alt+Enter",
            KeyAction::ScrollEnd => "End",
            KeyAction::CycleMode => "Shift+Tab",
            KeyAction::OpenWorkflows => "Alt+W",
            KeyAction::SlashCommands => "/",
            KeyAction::AtFiles => "@",
            KeyAction::Quit => "ctrl+c",
        }
    }
}

/// The key chord for an action (`ctrl+o`).
pub fn key_text(action: KeyAction) -> &'static str {
    action.key()
}

/// `key + description`, e.g. `ctrl+o to expand`.
pub fn key_hint(action: KeyAction, description: &str) -> String {
    format!("{} {description}", action.key())
}

/// Compact `key action` pair for placeholders and status lines, e.g. `Alt+W workflows`.
pub fn key_label(action: KeyAction, label: &str) -> String {
    format!("{} {label}", action.key())
}

/// Empty-editor ghost hint, rebuilt from the same table.
pub fn placeholder() -> String {
    format!(
        "Ask anything…  {} commands · {} files · {} mode · {} workflows",
        KeyAction::SlashCommands.key(),
        KeyAction::AtFiles.key(),
        KeyAction::CycleMode.key(),
        KeyAction::OpenWorkflows.key()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_a_non_empty_binding() {
        for action in KeyAction::ALL {
            assert!(!action.key().trim().is_empty(), "{action:?} has no binding");
        }
    }

    #[test]
    fn bindings_match_the_hint_strings_used_by_the_ui() {
        assert_eq!(key_hint(KeyAction::ExpandTools, "to expand"), "ctrl+o to expand");
        assert_eq!(
            key_hint(KeyAction::ToggleThinking, "to toggle"),
            "ctrl+t to toggle"
        );
        assert_eq!(
            key_hint(KeyAction::Interrupt, "interrupt"),
            "Esc interrupt"
        );
        assert_eq!(key_label(KeyAction::OpenWorkflows, "workflows"), "Alt+W workflows");
    }

    #[test]
    fn placeholder_uses_the_table() {
        assert_eq!(
            placeholder(),
            "Ask anything…  / commands · @ files · Shift+Tab mode · Alt+W workflows"
        );
    }

    #[test]
    fn actions_are_unique() {
        for (i, a) in KeyAction::ALL.iter().enumerate() {
            for b in &KeyAction::ALL[i + 1..] {
                assert_ne!(a, b, "duplicate action in table");
            }
        }
    }
}
