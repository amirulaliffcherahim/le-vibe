use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
};
use std::io;
use std::time::{Duration, Instant};

use crate::commands::filter_slash_commands_with_dynamic;
use crate::editor::{next_char_boundary, prev_char_boundary};
use crate::mode::{ALL_MODES, ALL_SHELLS};
use crate::state::{ActiveModal, TuiAction, TuiState};
use crate::theme::ALL_THEMES;

fn modal_len(state: &TuiState) -> usize {
    match state.modal {
        ActiveModal::None
        | ActiveModal::UsageModal
        | ActiveModal::HelpModal
        | ActiveModal::ChangelogModal
        | ActiveModal::DebugModal
        | ActiveModal::InfoModal => 0,
        ActiveModal::ModelPicker => state.picker_rows().len(),
        ActiveModal::LoginPicker | ActiveModal::LogoutPicker => 3,
        ActiveModal::ModePicker => ALL_MODES.len(),
        ActiveModal::TermPicker => ALL_SHELLS.len(),
        ActiveModal::ConfigModal => state.config_rows().len().max(1),
        ActiveModal::ThemePicker => ALL_THEMES.len(),
        ActiveModal::ExportPicker => 2,
        ActiveModal::RewindPicker => state.rewind_rows.len().max(1),
        ActiveModal::SessionPicker => state.filtered_session_rows().len().max(1),
        ActiveModal::ApprovalModal => 4,
        ActiveModal::TrustModal => 2,
        ActiveModal::QuestionModal => state
            .question_prompt
            .as_ref()
            .map(|(_, _, opts)| opts.len() + 1)
            .unwrap_or(1),
    }
}

pub fn poll_tui_action(state: &mut TuiState, timeout: Duration) -> io::Result<TuiAction> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !event::poll(remaining)? {
            return Ok(TuiAction::None);
        }
        match event::read()? {
            Event::Paste(text) => {
                if state.modal == ActiveModal::None {
                    return Ok(TuiAction::Pasted(text));
                }
                return Ok(TuiAction::Redraw);
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    if state.modal == ActiveModal::UsageModal {
                        state.usage_scroll = state.usage_scroll.saturating_sub(3);
                    } else if matches!(
                        state.modal,
                        ActiveModal::HelpModal
                            | ActiveModal::ChangelogModal
                            | ActiveModal::DebugModal
                            | ActiveModal::InfoModal
                    ) {
                        state.help_scroll = state.help_scroll.saturating_sub(3);
                    } else if state.modal == ActiveModal::None {
                        state.scroll_offset = state.scroll_offset.saturating_add(3);
                    }
                    return Ok(TuiAction::Redraw);
                }
                MouseEventKind::ScrollDown => {
                    if state.modal == ActiveModal::UsageModal {
                        state.usage_scroll = state.usage_scroll.saturating_add(3);
                    } else if matches!(
                        state.modal,
                        ActiveModal::HelpModal
                            | ActiveModal::ChangelogModal
                            | ActiveModal::DebugModal
                            | ActiveModal::InfoModal
                    ) {
                        state.help_scroll = state.help_scroll.saturating_add(3);
                    } else if state.modal == ActiveModal::None {
                        state.scroll_offset = state.scroll_offset.saturating_sub(3);
                    }
                    return Ok(TuiAction::Redraw);
                }
                // Ignore mouse move/click noise immediately so it causes zero lag or redraws.
                _ => continue,
            },
            Event::Resize(_, _) => return Ok(TuiAction::Redraw),
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let action = handle_key_event(state, key)?;
                if matches!(action, TuiAction::None) {
                    return Ok(TuiAction::Redraw);
                }
                return Ok(action);
            }
            _ => continue,
        }
    }
}

fn handle_key_event(state: &mut TuiState, key: crossterm::event::KeyEvent) -> io::Result<TuiAction> {

    if key.code == KeyCode::F(12)
        && state.modal != ActiveModal::ApprovalModal
        && state.modal != ActiveModal::QuestionModal
    {
        return Ok(TuiAction::ToggleDebug);
    }

    if state.modal == ActiveModal::DebugModal {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.modal = ActiveModal::None;
                state.help_scroll = 0;
            }
            KeyCode::Char('c') => return Ok(TuiAction::DebugClear),
            KeyCode::Char('f') => return Ok(TuiAction::DebugCycleFilter),
            KeyCode::Up => state.help_scroll = state.help_scroll.saturating_sub(1),
            KeyCode::Down => state.help_scroll = state.help_scroll.saturating_add(1),
            KeyCode::PageUp => state.help_scroll = state.help_scroll.saturating_sub(10),
            KeyCode::PageDown => state.help_scroll = state.help_scroll.saturating_add(10),
            _ => {}
        }
        return Ok(TuiAction::None);
    }

    if state.modal == ActiveModal::TrustModal {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('1') => {
                state.modal = ActiveModal::None;
                return Ok(TuiAction::TrustDecision(true));
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('2') => {
                state.modal = ActiveModal::None;
                return Ok(TuiAction::TrustDecision(false));
            }
            KeyCode::Up => state.modal_idx = 0,
            KeyCode::Down => state.modal_idx = 1,
            KeyCode::Enter => {
                let allow = state.modal_idx == 0;
                state.modal = ActiveModal::None;
                return Ok(TuiAction::TrustDecision(allow));
            }
            _ => {}
        }
        return Ok(TuiAction::None);
    }

    if state.modal == ActiveModal::ApprovalModal {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('4') => {
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                return Ok(TuiAction::ApprovalDecision(3));
            }
            KeyCode::Char('1') | KeyCode::Char('y') => {
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                return Ok(TuiAction::ApprovalDecision(0));
            }
            KeyCode::Char('2') | KeyCode::Char('s') => {
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                return Ok(TuiAction::ApprovalDecision(1));
            }
            KeyCode::Char('3') | KeyCode::Char('a') => {
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                return Ok(TuiAction::ApprovalDecision(2));
            }
            KeyCode::Up => state.modal_idx = state.modal_idx.saturating_sub(1),
            KeyCode::Down if state.modal_idx + 1 < 4 => state.modal_idx += 1,
            KeyCode::Enter => {
                let idx = state.modal_idx.min(3);
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                return Ok(TuiAction::ApprovalDecision(idx));
            }
            _ => {}
        }
        return Ok(TuiAction::None);
    }

    if state.modal == ActiveModal::QuestionModal {
        let total = modal_len(state);
        let custom_idx = total.saturating_sub(1);
        match key.code {
            KeyCode::Esc => {
                state.modal = ActiveModal::None;
                state.question_prompt = None;
                state.question_custom.clear();
                return Ok(TuiAction::QuestionAnswer("User cancelled question.".to_string()));
            }
            KeyCode::Up => state.modal_idx = state.modal_idx.saturating_sub(1),
            KeyCode::Down if state.modal_idx + 1 < total => state.modal_idx += 1,
            KeyCode::Backspace if state.modal_idx == custom_idx => {
                state.question_custom.pop();
            }
            KeyCode::Char(c) if state.modal_idx == custom_idx => {
                state.question_custom.push(c);
            }
            KeyCode::Enter => {
                let answer = if state.modal_idx == custom_idx {
                    state.question_custom.trim().to_string()
                } else {
                    state
                        .question_prompt
                        .as_ref()
                        .and_then(|(_, _, opts)| opts.get(state.modal_idx))
                        .map(|(label, _)| label.clone())
                        .unwrap_or_default()
                };
                state.modal = ActiveModal::None;
                state.question_prompt = None;
                state.question_custom.clear();
                return Ok(TuiAction::QuestionAnswer(answer));
            }
            _ => {}
        }
        return Ok(TuiAction::None);
    }

    if state.modal == ActiveModal::UsageModal {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                state.modal = ActiveModal::None;
            }
            KeyCode::Char('r') => {
                return Ok(TuiAction::Submit("/usage".to_string()));
            }
            KeyCode::Up => {
                state.usage_scroll = state.usage_scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                state.usage_scroll = state.usage_scroll.saturating_add(1);
            }
            _ => {}
        }
        return Ok(TuiAction::None);
    }

    if state.modal == ActiveModal::HelpModal
        || state.modal == ActiveModal::ChangelogModal
        || state.modal == ActiveModal::InfoModal
    {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                state.modal = ActiveModal::None;
                state.help_scroll = 0;
            }
            KeyCode::Up => {
                state.help_scroll = state.help_scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                state.help_scroll = state.help_scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                state.help_scroll = state.help_scroll.saturating_sub(8);
            }
            KeyCode::PageDown => {
                state.help_scroll = state.help_scroll.saturating_add(8);
            }
            _ => {}
        }
        return Ok(TuiAction::None);
    }

    if state.modal != ActiveModal::None {
        let len = modal_len(state);
        if state.modal == ActiveModal::ConfigModal {
            let cfg_rows = state.config_rows();
            match key.code {
                KeyCode::Esc => {
                    state.modal = ActiveModal::None;
                    state.modal_filter.clear();
                }
                KeyCode::Tab => {
                    state.config_save_scope = (state.config_save_scope + 1) % 3;
                }
                KeyCode::Up => state.modal_idx = state.modal_idx.saturating_sub(1),
                KeyCode::Down if state.modal_idx + 1 < cfg_rows.len() => state.modal_idx += 1,
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some((key_name, _, _, _)) = cfg_rows.get(state.modal_idx) {
                        return Ok(TuiAction::ConfigCycleKey((*key_name).to_string()));
                    }
                }
                KeyCode::Backspace => {
                    state.modal_filter.pop();
                    state.modal_idx = 0;
                }
                KeyCode::Char(c) if !c.is_control() => {
                    state.modal_filter.push(c);
                    state.modal_idx = 0;
                }
                _ => {}
            }
            return Ok(TuiAction::None);
        }
        if state.modal == ActiveModal::ModelPicker {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
                return Ok(TuiAction::SaveScope);
            }
            match key.code {
                // Tab flips between every model and the scoped set.
                KeyCode::Tab => {
                    state.scope_view = !state.scope_view;
                    state.modal_idx = 0;
                    return Ok(TuiAction::None);
                }
                // Space pins or unpins the highlighted model.
                KeyCode::Char(' ') => {
                    if let Some((provider, model_id, _)) = state.picker_rows().get(state.modal_idx) {
                        let (provider, model_id) = (provider.clone(), model_id.clone());
                        state.toggle_scope(&provider, &model_id);
                        let rows = state.picker_rows().len();
                        if state.modal_idx >= rows && rows > 0 {
                            state.modal_idx = rows - 1;
                        }
                    }
                    return Ok(TuiAction::None);
                }
                KeyCode::Backspace => {
                    state.modal_filter.pop();
                    state.modal_idx = 0;
                    return Ok(TuiAction::None);
                }
                KeyCode::Char(c) if !c.is_control() => {
                    state.modal_filter.push(c);
                    state.modal_idx = 0;
                    return Ok(TuiAction::None);
                }
                _ => {}
            }
        }
        if state.modal == ActiveModal::SessionPicker {
            match key.code {
                KeyCode::Backspace => {
                    state.modal_filter.pop();
                    state.modal_idx = 0;
                    return Ok(TuiAction::None);
                }
                KeyCode::Char(c) if !c.is_control() => {
                    state.modal_filter.push(c);
                    state.modal_idx = 0;
                    return Ok(TuiAction::None);
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc => state.modal = ActiveModal::None,
            KeyCode::Up => {
                state.modal_idx = state.modal_idx.saturating_sub(1);
            }
            KeyCode::Down if state.modal_idx + 1 < len => {
                state.modal_idx += 1;
            }
            KeyCode::Enter => {
                let current_modal = state.modal.clone();
                let idx = state.modal_idx;
                state.modal = ActiveModal::None;
                match current_modal {
                    ActiveModal::ModelPicker => {
                        if let Some((prov, id, _)) = state.picker_rows().get(idx) {
                            return Ok(TuiAction::Submit(format!("/model {prov}/{id}")));
                        }
                    }
                    ActiveModal::ThemePicker => {
                        let theme = ALL_THEMES[idx.min(ALL_THEMES.len() - 1)];
                        return Ok(TuiAction::Submit(format!("/theme {}", theme.name())));
                    }
                    ActiveModal::ModePicker => {
                        let mode = ALL_MODES[idx.min(ALL_MODES.len() - 1)];
                        return Ok(TuiAction::Submit(format!("/mode {}", mode.name())));
                    }
                    ActiveModal::TermPicker => {
                        let (shell, _) = ALL_SHELLS[idx.min(ALL_SHELLS.len() - 1)];
                        if shell == "other" {
                            state.input = "/term ".to_string();
                            state.cursor = state.input.len();
                            return Ok(TuiAction::None);
                        }
                        return Ok(TuiAction::Submit(format!("/term {shell}")));
                    }
                    ActiveModal::LoginPicker => {
                        let prov = ["vibe", "deepseek", "agy"][idx.min(2)];
                        return Ok(TuiAction::Submit(format!("/login {prov}")));
                    }
                    ActiveModal::LogoutPicker => {
                        let prov = ["vibe", "deepseek", "agy"][idx.min(2)];
                        return Ok(TuiAction::Submit(format!("/logout {prov}")));
                    }
                    ActiveModal::ExportPicker => {
                        let fmt = ["md", "html"][idx.min(1)];
                        return Ok(TuiAction::Submit(format!("/export {fmt}")));
                    }
                    ActiveModal::RewindPicker => {
                        if let Some((id, _, _)) = state.rewind_rows.get(idx) {
                            return Ok(TuiAction::Submit(format!("/rewind {id}")));
                        }
                    }
                    ActiveModal::SessionPicker => {
                        let rows = state.filtered_session_rows();
                        if let Some((path, _, _)) = rows.get(idx) {
                            return Ok(TuiAction::Submit(format!("/resume {path}")));
                        }
                    }
                    ActiveModal::None
                    | ActiveModal::UsageModal
                    | ActiveModal::ConfigModal
                    | ActiveModal::ApprovalModal
                    | ActiveModal::QuestionModal
                    | ActiveModal::HelpModal
                    | ActiveModal::ChangelogModal
                    | ActiveModal::DebugModal
                    | ActiveModal::InfoModal
                    | ActiveModal::TrustModal => {}
                }
            }
            _ => {}
        }
        return Ok(TuiAction::None);
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => {
                if state.input.is_empty() {
                    return Ok(TuiAction::Quit);
                }
                state.input.clear();
                state.cursor = 0;
                return Ok(TuiAction::None);
            }
            KeyCode::Char('d') => return Ok(TuiAction::Quit),
            KeyCode::Char('g') => return Ok(TuiAction::OpenExternalEditor),
            KeyCode::Char('x') => return Ok(TuiAction::CopyLastReply),
            KeyCode::Char('l') => return Ok(TuiAction::Submit("/clear".to_string())),
            KeyCode::Char('v') => return Ok(TuiAction::PasteClipboard),
            KeyCode::Char('p') => return Ok(TuiAction::CycleModel),
            KeyCode::Char('t') => {
                state.show_thinking = !state.show_thinking;
                return Ok(TuiAction::None);
            }
            KeyCode::Char('o') => {
                state.expand_tools = !state.expand_tools;
                return Ok(TuiAction::None);
            }
            KeyCode::Char('j') => {
                state.input.insert(state.cursor, '\n');
                state.cursor += 1;
                return Ok(TuiAction::None);
            }
            KeyCode::Char('a') => {
                state.cursor = 0;
                return Ok(TuiAction::None);
            }
            KeyCode::Char('e') => {
                state.cursor = state.input.len();
                return Ok(TuiAction::None);
            }
            KeyCode::Char('u') => {
                state.input.drain(..state.cursor);
                state.cursor = 0;
                return Ok(TuiAction::None);
            }
            KeyCode::Char('k') => {
                state.input.truncate(state.cursor);
                return Ok(TuiAction::None);
            }
            KeyCode::Char('w') if state.cursor > 0 => {
                let mut idx = state.cursor;
                while idx > 0 && state.input[..idx].ends_with(char::is_whitespace) {
                    idx = prev_char_boundary(&state.input, idx);
                }
                while idx > 0 && !state.input[..idx].ends_with(char::is_whitespace) {
                    idx = prev_char_boundary(&state.input, idx);
                }
                state.input.drain(idx..state.cursor);
                state.cursor = idx;
                return Ok(TuiAction::None);
            }
            _ => {}
        }
    }

    let matches = filter_slash_commands_with_dynamic(&state.input, &state.plugin_commands);
    let file_matches = if matches.is_empty() {
        state.at_file_matches()
    } else {
        Vec::new()
    };
    match key.code {
        KeyCode::BackTab => {
            state.mode = state.mode.next();
            return Ok(TuiAction::None);
        }
        KeyCode::Up
            if key.modifiers.contains(KeyModifiers::ALT)
                && (!state.queued_prompts.is_empty() || !state.steering_prompts.is_empty()) =>
        {
            return Ok(TuiAction::RecallQueued);
        }
        KeyCode::Up => {
            if !matches.is_empty() || !file_matches.is_empty() {
                state.selected_slash_idx = state.selected_slash_idx.saturating_sub(1);
            } else if key.modifiers.contains(KeyModifiers::SHIFT) {
                state.scroll_offset = state.scroll_offset.saturating_add(3);
            } else if !state.history.is_empty() {
                let next_idx = match state.history_idx {
                    None => state.history.len() - 1,
                    Some(i) => i.saturating_sub(1),
                };
                state.history_idx = Some(next_idx);
                state.input = state.history[next_idx].clone();
                state.cursor = state.input.len();
            }
        }
        KeyCode::Down => {
            if !matches.is_empty() {
                if state.selected_slash_idx + 1 < matches.len() {
                    state.selected_slash_idx += 1;
                }
            } else if !file_matches.is_empty() {
                if state.selected_slash_idx + 1 < file_matches.len() {
                    state.selected_slash_idx += 1;
                }
            } else if key.modifiers.contains(KeyModifiers::SHIFT) {
                state.scroll_offset = state.scroll_offset.saturating_sub(3);
            } else if let Some(idx) = state.history_idx {
                if idx + 1 < state.history.len() {
                    let next_idx = idx + 1;
                    state.history_idx = Some(next_idx);
                    state.input = state.history[next_idx].clone();
                    state.cursor = state.input.len();
                } else {
                    state.history_idx = None;
                    state.input.clear();
                    state.cursor = 0;
                }
            }
        }
        KeyCode::Home => {
            state.cursor = 0;
        }
        KeyCode::End => {
            if state.input.is_empty() {
                state.scroll_offset = 0;
            } else {
                state.cursor = state.input.len();
            }
        }
        KeyCode::Delete if state.cursor < state.input.len() => {
            state.input.remove(state.cursor);
            state.selected_slash_idx = 0;
        }
        KeyCode::PageUp => {
            state.scroll_offset = state.scroll_offset.saturating_add(8);
        }
        KeyCode::PageDown => {
            state.scroll_offset = state.scroll_offset.saturating_sub(8);
        }
        KeyCode::Tab => {
            if let Some(cmd) = matches.get(state.selected_slash_idx) {
                state.input = if cmd.picker_on_bare {
                    cmd.name.clone()
                } else {
                    format!("{} ", cmd.name)
                };
                state.cursor = state.input.len();
                state.selected_slash_idx = 0;
            } else if !file_matches.is_empty()
                && let Some((at_pos, _)) = state.active_at_query()
            {
                let chosen = &file_matches[state.selected_slash_idx.min(file_matches.len() - 1)];
                let replacement = format!("@{chosen} ");
                state.input.replace_range(at_pos..state.cursor, &replacement);
                state.cursor = at_pos + replacement.len();
                state.selected_slash_idx = 0;
            }
        }
        KeyCode::Enter
            if state.is_busy
                && (key.modifiers.contains(KeyModifiers::ALT)
                    || key.modifiers.contains(KeyModifiers::CONTROL)) =>
        {
            let text = state.input.trim().to_string();
            if !text.is_empty() {
                state.input.clear();
                state.cursor = 0;
                state.history_idx = None;
                state.selected_slash_idx = 0;
                return Ok(TuiAction::SubmitFollowUp(text));
            }
        }
        KeyCode::Enter
            if key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            state.input.insert(state.cursor, '\n');
            state.cursor += 1;
            state.selected_slash_idx = 0;
        }
        KeyCode::Enter => {
            // If more input events are already queued at the exact same instant, this Enter came
            // from an unbracketed multi-line paste burst — insert a newline instead of submitting.
            if event::poll(Duration::ZERO).unwrap_or(false) {
                state.input.insert(state.cursor, '\n');
                state.cursor += 1;
                state.selected_slash_idx = 0;
                return Ok(TuiAction::None);
            }
            if !file_matches.is_empty()
                && let Some((at_pos, _)) = state.active_at_query()
            {
                let chosen = &file_matches[state.selected_slash_idx.min(file_matches.len() - 1)];
                let replacement = format!("@{chosen} ");
                state.input.replace_range(at_pos..state.cursor, &replacement);
                state.cursor = at_pos + replacement.len();
                state.selected_slash_idx = 0;
                return Ok(TuiAction::None);
            }
            if !matches.is_empty()
                && let Some(cmd) = matches.get(state.selected_slash_idx)
            {
                if cmd.picker_on_bare {
                    let submitted = cmd.name.clone();
                    state.input.clear();
                    state.cursor = 0;
                    state.selected_slash_idx = 0;
                    return Ok(TuiAction::Submit(submitted));
                }
                if state.input.trim() != cmd.name {
                    state.input = format!("{} ", cmd.name);
                    state.cursor = state.input.len();
                    state.selected_slash_idx = 0;
                    return Ok(TuiAction::None);
                }
            }
            let submitted = state.input.trim().to_string();
            state.input.clear();
            state.cursor = 0;
            state.selected_slash_idx = 0;
            state.scroll_offset = 0;
            state.history_idx = None;
            if !submitted.is_empty() {
                if state.history.last() != Some(&submitted) {
                    state.history.push(submitted.clone());
                    if state.history.len() > 500 {
                        state.history.remove(0);
                    }
                }
                return Ok(TuiAction::Submit(submitted));
            }
        }
        KeyCode::Backspace if state.cursor > 0 => {
            let prev = prev_char_boundary(&state.input, state.cursor);
            state.input.remove(prev);
            state.cursor = prev;
            state.selected_slash_idx = 0;
        }
        KeyCode::Left => {
            state.cursor = prev_char_boundary(&state.input, state.cursor);
        }
        KeyCode::Right if state.cursor < state.input.len() => {
            state.cursor = next_char_boundary(&state.input, state.cursor);
        }
        KeyCode::Esc => {
            if state.is_busy || state.input.is_empty() {
                return Ok(TuiAction::Interrupt);
            }
            state.input.clear();
            state.cursor = 0;
            state.selected_slash_idx = 0;
        }
        KeyCode::Char(c) => {
            state.input.insert(state.cursor, c);
            state.cursor += c.len_utf8();
            state.selected_slash_idx = 0;
        }
        _ => {}
    }

    Ok(TuiAction::None)
}

