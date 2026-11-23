use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};

thread_local! {
    static PENDING_EVENTS: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) };
}

fn next_terminal_event(timeout: Duration) -> io::Result<Option<Event>> {
    if let Some(ev) = PENDING_EVENTS.with(|q| q.borrow_mut().pop_front()) {
        return Ok(Some(ev));
    }
    if !event::poll(timeout)? {
        return Ok(None);
    }
    Ok(Some(event::read()?))
}

/// Detect unbracketed multi-line pastes by peeking past ConPTY `Release` / `Mouse` noise
/// for an immediately queued character/newline key press.
fn has_queued_paste_key() -> bool {
    if PENDING_EVENTS.with(|q| {
        q.borrow().iter().any(|ev| {
            matches!(
                ev,
                Event::Key(k)
                    if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat)
                        && matches!(k.code, KeyCode::Char(_) | KeyCode::Enter | KeyCode::Tab)
            )
        })
    }) {
        return true;
    }
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let Ok(ev) = event::read() else {
            break;
        };
        match &ev {
            Event::Key(k) if k.kind == KeyEventKind::Release => continue,
            Event::Mouse(m)
                if !matches!(
                    m.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                ) =>
            {
                continue;
            }
            Event::Key(k)
                if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat)
                    && matches!(k.code, KeyCode::Char(_) | KeyCode::Enter | KeyCode::Tab) =>
            {
                PENDING_EVENTS.with(|q| q.borrow_mut().push_back(ev));
                return true;
            }
            _ => {
                PENDING_EVENTS.with(|q| q.borrow_mut().push_back(ev));
                return false;
            }
        }
    }
    false
}

use crate::commands::filter_slash_commands_with_dynamic;
use crate::editor::{next_char_boundary, prev_char_boundary};
use crate::mode::{ALL_MODES, ALL_SHELLS};
use crate::state::{ActiveModal, TuiAction, TuiState, keep_vt_input_off};
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
            .and_then(|s| s.active())
            .map(|q| q.options.len() + 1)
            .unwrap_or(1),
    }
}

/// Scroll the active surface (transcript or modal list) by one wheel notch.
pub fn apply_wheel_scroll(state: &mut TuiState, up: bool) {
    match state.modal {
        ActiveModal::None => {
            if up {
                state.scroll_offset = state.scroll_offset.saturating_add(3);
            } else {
                state.scroll_offset = state.scroll_offset.saturating_sub(3);
            }
        }
        ActiveModal::UsageModal => {
            if up {
                state.usage_scroll = state.usage_scroll.saturating_sub(3);
            } else {
                state.usage_scroll = state.usage_scroll.saturating_add(3);
            }
        }
        ActiveModal::HelpModal
        | ActiveModal::ChangelogModal
        | ActiveModal::DebugModal
        | ActiveModal::InfoModal => {
            if up {
                state.help_scroll = state.help_scroll.saturating_sub(3);
            } else {
                state.help_scroll = state.help_scroll.saturating_add(3);
            }
        }
        _ => {
            let len = modal_len(state);
            if len > 0 {
                if up {
                    state.modal_idx = state.modal_idx.saturating_sub(1);
                } else if state.modal_idx + 1 < len {
                    state.modal_idx += 1;
                }
            }
        }
    }
}

/// Parse a leaked SGR mouse CSI payload (after `[<`) like `35;82;17M` or `64;50;12M`.
/// Returns `Some(Some(true))` for wheel up, `Some(Some(false))` for wheel down,
/// `Some(None)` for motion/click to swallow, or `None` if not a valid SGR mouse payload.
fn parse_sgr_mouse_payload(payload: &str) -> Option<Option<bool>> {
    let body = payload.strip_suffix('M').or_else(|| payload.strip_suffix('m'))?;
    let mut parts = body.split(';');
    let btn: u32 = parts.next()?.parse().ok()?;
    let _x: u32 = parts.next()?.parse().ok()?;
    let _y: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    match btn {
        64 => Some(Some(true)),
        65 => Some(Some(false)),
        _ => Some(None),
    }
}

/// Drain a leaked `\x1b[<Cb;Cx;CyM` sequence if `Esc` arrived as part of a rapid key burst
/// (e.g. ConPTY race with a child process or MSYS2/SSH). Returns `Some(wheel_opt)` if swallowed.
fn try_swallow_leaked_sgr_mouse() -> Option<Option<bool>> {
    if !event::poll(Duration::ZERO).unwrap_or(false) {
        return None;
    }
    let Ok(Event::Key(k1)) = event::read() else {
        return None;
    };
    if k1.kind != KeyEventKind::Press || k1.code != KeyCode::Char('[') {
        return None;
    }
    if !event::poll(Duration::from_millis(2)).unwrap_or(false) {
        return None;
    }
    let Ok(Event::Key(k2)) = event::read() else {
        return None;
    };
    if k2.kind != KeyEventKind::Press || k2.code != KeyCode::Char('<') {
        return None;
    }
    let mut buf = String::with_capacity(16);
    for _ in 0..24 {
        if !event::poll(Duration::from_millis(2)).unwrap_or(false) {
            break;
        }
        if let Ok(Event::Key(k)) = event::read() {
            if !matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }
            if let KeyCode::Char(c) = k.code {
                buf.push(c);
                if c == 'M' || c == 'm' {
                    return parse_sgr_mouse_payload(&buf);
                }
            } else {
                break;
            }
        }
    }
    Some(None)
}

pub fn poll_tui_action(state: &mut TuiState, timeout: Duration) -> io::Result<TuiAction> {
    keep_vt_input_off();
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Some(ev) = next_terminal_event(remaining)? else {
            return Ok(TuiAction::None);
        };
        match ev {
            Event::Paste(text) => {
                if state.modal == ActiveModal::None {
                    return Ok(TuiAction::Pasted(text));
                }
                return Ok(TuiAction::Redraw);
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    apply_wheel_scroll(state, true);
                    return Ok(TuiAction::Redraw);
                }
                MouseEventKind::ScrollDown => {
                    apply_wheel_scroll(state, false);
                    return Ok(TuiAction::Redraw);
                }
                // Ignore mouse move/click noise immediately so it causes zero lag or redraws.
                _ => continue,
            },
            Event::Resize(_, _) => return Ok(TuiAction::Redraw),
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                if key.code == KeyCode::Esc
                    && let Some(wheel) = try_swallow_leaked_sgr_mouse()
                {
                    if let Some(up) = wheel {
                        apply_wheel_scroll(state, up);
                        return Ok(TuiAction::Redraw);
                    }
                    continue;
                }
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

/// Apply one pressed key to the UI state, returning the action the event loop must run.
pub fn handle_key_event(state: &mut TuiState, key: crossterm::event::KeyEvent) -> io::Result<TuiAction> {

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
        // 400ms typing grace period (Mistral Vibe pattern): prevent accidental mid-sentence approval.
        let in_grace = state
            .approval_opened_at
            .is_some_and(|t| t.elapsed() < Duration::from_millis(400));
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('4') => {
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                state.approval_opened_at = None;
                return Ok(TuiAction::ApprovalDecision(3));
            }
            KeyCode::Char('1') | KeyCode::Char('y') if !in_grace => {
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                state.approval_opened_at = None;
                return Ok(TuiAction::ApprovalDecision(0));
            }
            KeyCode::Char('2') | KeyCode::Char('s') if !in_grace => {
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                state.approval_opened_at = None;
                return Ok(TuiAction::ApprovalDecision(1));
            }
            KeyCode::Char('3') | KeyCode::Char('a') if !in_grace => {
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                state.approval_opened_at = None;
                return Ok(TuiAction::ApprovalDecision(2));
            }
            KeyCode::Up => state.modal_idx = state.modal_idx.saturating_sub(1),
            KeyCode::Down if state.modal_idx + 1 < 4 => state.modal_idx += 1,
            KeyCode::Enter if !in_grace => {
                let idx = state.modal_idx.min(3);
                state.modal = ActiveModal::None;
                state.approval_prompt = None;
                state.approval_opened_at = None;
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
                return Ok(TuiAction::QuestionAnswer("User cancelled question.".to_string()));
            }
            KeyCode::Up => state.modal_idx = state.modal_idx.saturating_sub(1),
            KeyCode::Down if state.modal_idx + 1 < total => state.modal_idx += 1,
            KeyCode::Left | KeyCode::BackTab => {
                if let Some(session) = state.question_prompt.as_mut()
                    && session.items.len() > 1
                {
                    session.active_idx = if session.active_idx == 0 {
                        session.items.len() - 1
                    } else {
                        session.active_idx - 1
                    };
                    state.modal_idx = 0;
                }
            }
            KeyCode::Right | KeyCode::Tab => {
                if let Some(session) = state.question_prompt.as_mut()
                    && session.items.len() > 1
                {
                    session.active_idx = (session.active_idx + 1) % session.items.len();
                    state.modal_idx = 0;
                }
            }
            KeyCode::Char(' ')
                if state.modal_idx < custom_idx
                    && state
                        .question_prompt
                        .as_ref()
                        .and_then(|s| s.active())
                        .is_some_and(|q| q.multi_select) =>
            {
                if let Some(item) = state.question_prompt.as_mut().and_then(|s| s.active_mut())
                    && let Some(flag) = item.selected.get_mut(state.modal_idx)
                {
                    *flag = !*flag;
                }
            }
            KeyCode::Char(c @ '1'..='9') if state.modal_idx != custom_idx => {
                let idx = (c as usize) - ('1' as usize);
                if idx < total {
                    state.modal_idx = idx;
                }
            }
            KeyCode::Backspace if state.modal_idx == custom_idx => {
                if let Some(item) = state.question_prompt.as_mut().and_then(|s| s.active_mut()) {
                    item.custom_text.pop();
                }
            }
            KeyCode::Char(c) if state.modal_idx == custom_idx => {
                if let Some(item) = state.question_prompt.as_mut().and_then(|s| s.active_mut()) {
                    item.custom_text.push(c);
                }
            }
            KeyCode::Enter => {
                let mut final_payload: Option<String> = None;
                if let Some(session) = state.question_prompt.as_mut() {
                    if let Some(item) = session.active_mut() {
                        let chosen = if state.modal_idx == custom_idx {
                            item.custom_text.trim().to_string()
                        } else if item.multi_select {
                            let picked: Vec<String> = item
                                .options
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| item.selected.get(*i).copied().unwrap_or(false))
                                .map(|(_, o)| o.label.clone())
                                .collect();
                            if picked.is_empty() {
                                item.options
                                    .get(state.modal_idx)
                                    .map(|o| o.label.clone())
                                    .unwrap_or_default()
                            } else {
                                picked.join(", ")
                            }
                        } else {
                            item.options
                                .get(state.modal_idx)
                                .map(|o| o.label.clone())
                                .unwrap_or_default()
                        };
                        item.answer = Some(chosen);
                    }
                    if session.items.len() == 1 {
                        final_payload = session.items[0].answer.clone();
                    } else if let Some(next_unanswered) =
                        session.items.iter().position(|q| q.answer.is_none())
                    {
                        session.active_idx = next_unanswered;
                        state.modal_idx = 0;
                    } else {
                        let pairs: Vec<String> = session
                            .items
                            .iter()
                            .map(|q| {
                                format!(
                                    "{}:{}",
                                    json_quote(&q.header),
                                    json_quote(q.answer.as_deref().unwrap_or(""))
                                )
                            })
                            .collect();
                        final_payload = Some(format!("{{{}}}", pairs.join(",")));
                    }
                }
                if let Some(answer) = final_payload {
                    state.modal = ActiveModal::None;
                    state.question_prompt = None;
                    return Ok(TuiAction::QuestionAnswer(answer));
                }
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
            KeyCode::Char('b') => {
                state.show_todo_pane = !state.show_todo_pane;
                return Ok(TuiAction::None);
            }
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
        KeyCode::Up | KeyCode::Char('q') | KeyCode::Char('Q')
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
            if has_queued_paste_key() {
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

fn json_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_leaked_sgr_mouse_sequences() {
        assert_eq!(parse_sgr_mouse_payload("35;82;17M"), Some(None));
        assert_eq!(parse_sgr_mouse_payload("64;50;12M"), Some(Some(true)));
        assert_eq!(parse_sgr_mouse_payload("65;50;12m"), Some(Some(false)));
        assert_eq!(parse_sgr_mouse_payload("not-mouse"), None);
    }
}

