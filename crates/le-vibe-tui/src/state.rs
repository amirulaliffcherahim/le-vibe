use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::style::Print;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::text::Line;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Stdout};
use std::time::Instant;

use crate::mode::AgentMode;
use crate::resync_backend::ResyncBackend;
use crate::theme::ThemeId;

#[derive(Debug, Default)]
pub struct FrameBuffer {
    previous_lines: Vec<String>,
}

impl FrameBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `(line_index, new_text)` for lines that changed since the last render.
    pub fn diff(&mut self, next_lines: Vec<String>) -> Vec<(usize, String)> {
        let mut changes = Vec::new();
        let max_len = self.previous_lines.len().max(next_lines.len());
        for i in 0..max_len {
            let prev = self.previous_lines.get(i).map(String::as_str).unwrap_or("");
            let next = next_lines.get(i).map(String::as_str).unwrap_or("");
            if prev != next {
                changes.push((i, next.to_string()));
            }
        }
        self.previous_lines = next_lines;
        changes
    }
}

pub fn render_status_bar(provider: &str, model: &str) -> String {
    format!("[le-vibe] provider={provider} | model={model}")
}

pub fn format_tokens(count: u64) -> String {
    if count < 1_000 {
        count.to_string()
    } else if count < 10_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else if count < 1_000_000 {
        format!("{}k", count / 1_000)
    } else {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    }
}

/// One tool invocation's presentation state (all formatting lives in `transcript.rs`).
#[derive(Debug, Clone, Default)]
pub struct ToolCard {
    pub name: String,
    /// Raw JSON arguments exactly as the model sent them; parsed by the renderer.
    pub args_json: String,
    pub status: String,
    pub output: String,
    pub duration_ms: Option<u64>,
}

impl ToolCard {
    pub fn new(name: &str, args_json: &str) -> Self {
        Self {
            name: name.to_string(),
            args_json: args_json.to_string(),
            ..Default::default()
        }
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.status = status.to_string();
        self
    }

    pub fn with_output(mut self, output: &str) -> Self {
        self.output = output.to_string();
        self
    }

    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    pub fn is_running(&self) -> bool {
        self.status.starts_with("running") || self.status == "pending"
    }

    pub fn is_error(&self) -> bool {
        self.status.starts_with("failed")
    }
}

#[derive(Debug, Clone)]
pub enum TranscriptBlock {
    User(String),
    Thinking(String),
    Assistant(String),
    Tool(ToolCard),
    Script { command: String, output: String, in_context: bool },
    System(String),
    /// Preformatted output (reports, tables): rendered verbatim, without a bullet or indent.
    Report(String),
}

#[derive(Debug, Default)]
pub struct TranscriptCache {
    pub width: u16,
    pub theme: Option<ThemeId>,
    pub show_thinking: bool,
    pub expand_tools: bool,
    pub entries: Vec<(u64, Vec<Line<'static>>)>,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderUsageStats {
    pub turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    ModelPicker,
    ModePicker,
    WorkflowPicker,
    ThemePicker,
    TermPicker,
    ConfigModal,
    LoginPicker,
    LogoutPicker,
    UsageModal,
    ApprovalModal,
    QuestionModal,
    ExportPicker,
    RewindPicker,
    SessionPicker,
    HelpModal,
    ChangelogModal,
    DebugModal,
    InfoModal,
    TrustModal,
}

pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Why the harness started compacting (pi shows a different sentence per reason).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionReason {
    /// User ran `/compact`.
    Manual,
    /// Auto-compact threshold was crossed.
    Threshold,
    /// Provider reported a context overflow.
    Overflow,
}

/// Which kind of work the status strip is reporting.
///
/// pi renders four distinct indicators; keeping the kind (instead of a free-form string) lets
/// `le-vibe-tui` own all wording, colouring and countdown maths.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum StatusKind {
    #[default]
    Idle,
    Working,
    Retry {
        attempt: usize,
        max_attempts: usize,
        /// When the next attempt starts; the remaining seconds are recomputed every frame.
        deadline: Instant,
    },
    Compaction {
        reason: CompactionReason,
    },
    BranchSummary,
}

impl StatusKind {
    /// Whether the strip should use the warning colour (only retries do, as in pi).
    pub fn is_warning(&self) -> bool {
        matches!(self, StatusKind::Retry { .. })
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, StatusKind::Idle)
    }

    /// Status label for this kind, or `None` when idle (callers fall back to `status_text`).
    pub fn label(&self) -> Option<String> {
        Some(match self {
            StatusKind::Idle => return None,
            StatusKind::Working => "Working...".to_string(),
            StatusKind::Retry {
                attempt,
                max_attempts,
                deadline,
            } => {
                let secs = deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64()
                    .ceil() as u64;
                format!("Retrying ({attempt}/{max_attempts}) in {secs}s... (esc to cancel)")
            }
            StatusKind::Compaction { reason } => match reason {
                CompactionReason::Manual => "Compacting context... (esc to cancel)".to_string(),
                CompactionReason::Threshold => "Auto-compacting... (esc to cancel)".to_string(),
                CompactionReason::Overflow => {
                    "Context overflow detected, Auto-compacting... (esc to cancel)".to_string()
                }
            },
            StatusKind::BranchSummary => "Summarizing branch... (esc to cancel)".to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionItem {
    pub header: String,
    pub question: String,
    pub multi_select: bool,
    pub options: Vec<QuestionOption>,
    pub selected: Vec<bool>,
    pub custom_text: String,
    pub answer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionSession {
    pub items: Vec<QuestionItem>,
    pub active_idx: usize,
}

impl QuestionSession {
    pub fn single(header: String, question: String, options: Vec<(String, String)>) -> Self {
        let len = options.len();
        Self {
            items: vec![QuestionItem {
                header,
                question,
                multi_select: false,
                options: options
                    .into_iter()
                    .map(|(label, description)| QuestionOption {
                        label,
                        description,
                        preview: None,
                    })
                    .collect(),
                selected: vec![false; len],
                custom_text: String::new(),
                answer: None,
            }],
            active_idx: 0,
        }
    }

    pub fn active(&self) -> Option<&QuestionItem> {
        self.items.get(self.active_idx)
    }

    pub fn active_mut(&mut self) -> Option<&mut QuestionItem> {
        self.items.get_mut(self.active_idx)
    }
}

pub struct TuiState {
    pub version: String,
    pub cwd: String,
    pub repo_name: String,
    pub git_branch: Option<String>,
    pub git_badges: String,
    pub provider: String,
    pub model: String,
    pub thinking_level: String,
    pub mode: AgentMode,
    pub theme: ThemeId,
    pub shell: String,
    pub model_default_thinking: BTreeMap<(String, String), String>,
    pub context_window: u64,
    /// Prompt tokens of the most recent single LLM turn (used for context % and auto-compact).
    pub context_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub last_cache_rate: Option<f64>,
    pub last_tps: Option<f64>,
    pub cost_usd: f64,
    pub active_ms: u64,
    pub active_started: Option<Instant>,
    pub provider_usage: BTreeMap<String, ProviderUsageStats>,
    pub provider_auth: BTreeMap<String, (bool, String)>,
    pub provider_quota_badge: BTreeMap<String, String>,
    pub status_text: Option<String>,
    /// Structured status kind; `Idle` falls back to `status_text`.
    pub status_kind: StatusKind,
    pub is_busy: bool,
    pub yolo: bool,
    pub codemode: String,
    pub approval_prompt: Option<(String, String, String)>,
    pub approval_opened_at: Option<Instant>,
    pub collapsed_pastes: Vec<(String, String)>,
    pub question_prompt: Option<QuestionSession>,
    pub spinner_tick: usize,
    pub reduced_motion: bool,
    pub usage_lines: Vec<String>,
    pub usage_scroll: u16,
    pub usage_loading: bool,
    pub input: String,
    pub cursor: usize,
    pub selected_slash_idx: usize,
    pub scroll_offset: u16,
    pub rendered_scroll: Cell<u16>,
    pub transcript_cache: RefCell<TranscriptCache>,
    pub blocks: Vec<TranscriptBlock>,
    pub available_models: Vec<(String, String, String)>,
    /// Models pinned for Ctrl+P cycling, as (provider, model id).
    pub scoped_models: Vec<(String, String)>,
    /// Model picker shows only scoped models while true; Tab flips it.
    pub scope_view: bool,
    /// Set when the scope changes so the caller can persist it.
    pub scope_dirty: bool,
    pub modal: ActiveModal,
    pub modal_idx: usize,
    pub queued_prompts: VecDeque<String>,
    pub steering_prompts: VecDeque<String>,
    /// Active `/goal` Proof Loop: `(stop_condition, attempt, max_attempts)`.
    pub active_goal: Option<(String, usize, usize)>,
    pub trust_prompt_path: String,
    pub trust_prompt_resources: Vec<String>,
    pub loaded_resources: Vec<String>,
    pub rewind_rows: Vec<(String, String, String)>,
    pub session_rows: Vec<(String, String, String)>,
    pub plugin_commands: Vec<(String, String)>,
    pub active_loops: usize,
    pub pending_attachments: Vec<(String, String, String)>,
    pub show_thinking: bool,
    pub expand_tools: bool,
    pub todos: Vec<(u64, String, String)>,
    pub show_todo_pane: bool,
    pub history: Vec<String>,
    pub history_idx: Option<usize>,
    pub modal_filter: String,
    pub help_scroll: u16,
    pub changelog_lines: Vec<String>,
    pub info_modal_title: String,
    pub info_modal_lines: Vec<String>,
    pub workspace_files: Vec<String>,
    pub debug_lines: Vec<String>,
    pub debug_filter: String,
    pub last_draw_ms: u64,
    pub quiet_startup: bool,
    pub bell_on_done: bool,
    pub auto_compact: bool,
    pub compact_threshold_pct: u64,
    pub max_turns: usize,
    pub retry_max: usize,
    pub confirm_on_exit: bool,
    pub double_esc_rewind: bool,
    pub config_origins: BTreeMap<String, String>,
    /// 0 = user (~/.le-vibe/settings.json), 1 = project (.le-vibe/settings.json), 2 = session only
    pub config_save_scope: u8,
}

impl TuiState {
    pub fn new(provider: String, model: String, cwd: String, git_branch: Option<String>) -> Self {
        let mut provider_usage = BTreeMap::new();
        for p in ["vibe", "deepseek", "agy"] {
            provider_usage.insert(p.to_string(), ProviderUsageStats::default());
        }
        let reduced_motion = std::env::var_os("PI_REDUCED_MOTION").is_some()
            || std::env::var_os("NO_COLOR").is_some();
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            cwd,
            repo_name: String::new(),
            git_branch,
            git_badges: String::new(),
            provider,
            model,
            thinking_level: "medium".to_string(),
            mode: AgentMode::Normal,
            theme: ThemeId::Pi,
            shell: if cfg!(windows) {
                "powershell".to_string()
            } else {
                "bash".to_string()
            },
            model_default_thinking: BTreeMap::new(),
            context_window: 256_000,
            context_tokens: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            last_cache_rate: None,
            last_tps: None,
            cost_usd: 0.0,
            active_ms: 0,
            active_started: None,
            provider_usage,
            provider_auth: BTreeMap::new(),
            provider_quota_badge: BTreeMap::new(),
            status_text: None,
            status_kind: StatusKind::Idle,
            is_busy: false,
            yolo: false,
            codemode: "off".to_string(),
            approval_prompt: None,
            approval_opened_at: None,
            collapsed_pastes: Vec::new(),
            question_prompt: None,
            spinner_tick: 0,
            reduced_motion,
            usage_lines: Vec::new(),
            usage_scroll: 0,
            usage_loading: false,
            input: String::new(),
            cursor: 0,
            selected_slash_idx: 0,
            scroll_offset: 0,
            rendered_scroll: Cell::new(0),
            transcript_cache: RefCell::new(TranscriptCache::default()),
            blocks: Vec::new(),
            available_models: Vec::new(),
            queued_prompts: VecDeque::new(),
            steering_prompts: VecDeque::new(),
            active_goal: None,
            trust_prompt_path: String::new(),
            trust_prompt_resources: Vec::new(),
            loaded_resources: Vec::new(),
            rewind_rows: Vec::new(),
            session_rows: Vec::new(),
            plugin_commands: Vec::new(),
            active_loops: 0,
            pending_attachments: Vec::new(),
            show_thinking: true,
            expand_tools: false,
            todos: Vec::new(),
            show_todo_pane: true,
            history: Vec::new(),
            history_idx: None,
            modal_filter: String::new(),
            help_scroll: 0,
            changelog_lines: Vec::new(),
            info_modal_title: String::new(),
            info_modal_lines: Vec::new(),
            workspace_files: Vec::new(),
            debug_lines: Vec::new(),
            debug_filter: "all".to_string(),
            last_draw_ms: 0,
            quiet_startup: false,
            bell_on_done: false,
            auto_compact: true,
            compact_threshold_pct: 85,
            max_turns: 40,
            retry_max: 2,
            confirm_on_exit: false,
            double_esc_rewind: true,
            config_origins: BTreeMap::new(),
            config_save_scope: 0,
            scoped_models: Vec::new(),
            scope_view: false,
            scope_dirty: false,
            modal: ActiveModal::None,
            modal_idx: 0,
        }
    }

    pub fn advance_spinner(&mut self) {
        if !self.reduced_motion {
            self.spinner_tick = (self.spinner_tick + 1) % SPINNER_FRAMES.len();
        }
    }

    pub fn set_retry(&mut self, attempt: usize, max_attempts: usize, delay_ms: u64) {
        self.status_kind = StatusKind::Retry {
            attempt,
            max_attempts,
            deadline: Instant::now() + std::time::Duration::from_millis(delay_ms),
        };
    }

    pub fn set_compaction(&mut self, reason: CompactionReason) {
        self.status_kind = StatusKind::Compaction { reason };
    }

    pub fn set_branch_summary(&mut self) {
        self.status_kind = StatusKind::BranchSummary;
    }

    pub fn set_working(&mut self) {
        self.status_kind = StatusKind::Working;
    }

    pub fn clear_status_kind(&mut self) {
        self.status_kind = StatusKind::Idle;
    }

    /// Effective status label: structured kind wins, then any free-form `status_text`.
    pub fn status_label(&self) -> Option<String> {
        match self.status_kind.label() {
            Some(label) => Some(label),
            None => self
                .status_text
                .clone()
                .or_else(|| self.is_busy.then(|| "Working...".to_string())),
        }
    }

    pub fn spinner_glyph(&self) -> &'static str {
        if self.reduced_motion {
            "•"
        } else {
            SPINNER_FRAMES[self.spinner_tick % SPINNER_FRAMES.len()]
        }
    }

    pub fn is_scoped(&self, provider: &str, model_id: &str) -> bool {
        self.scoped_models
            .iter()
            .any(|(p, m)| p == provider && m == model_id)
    }

    pub fn toggle_scope(&mut self, provider: &str, model_id: &str) {
        if let Some(pos) = self
            .scoped_models
            .iter()
            .position(|(p, m)| p == provider && m == model_id)
        {
            self.scoped_models.remove(pos);
        } else {
            self.scoped_models
                .push((provider.to_string(), model_id.to_string()));
        }
        self.scope_dirty = true;
    }

    /// Models Ctrl+P cycles through: the scope when one is set, else every model.
    pub fn cycle_pool(&self) -> Vec<(String, String)> {
        if self.scoped_models.is_empty() {
            self.available_models
                .iter()
                .map(|(p, id, _)| (p.clone(), id.clone()))
                .collect()
        } else {
            self.scoped_models.clone()
        }
    }

    /// The rows the model picker shows for the current view.
    pub fn picker_rows(&self) -> Vec<(String, String, String)> {
        let q = self.modal_filter.trim().to_lowercase();
        self.available_models
            .iter()
            .filter(|(p, id, _)| !self.scope_view || self.is_scoped(p, id))
            .filter(|(p, id, name)| {
                q.is_empty()
                    || p.to_lowercase().contains(&q)
                    || id.to_lowercase().contains(&q)
                    || name.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    }

    pub fn config_scope_label(&self) -> &'static str {
        match self.config_save_scope % 3 {
            0 => "user",
            1 => "project",
            _ => "session",
        }
    }

    pub fn config_origin(&self, key: &str) -> &str {
        self.config_origins
            .get(key)
            .map(String::as_str)
            .unwrap_or("default")
    }

    /// Filtered `/config` rows as `(key, label, value_with_origin, description)`.
    pub fn config_rows(&self) -> Vec<(&'static str, &'static str, String, &'static str)> {
        let on_off = |b: bool| if b { "on" } else { "off" };
        let all: Vec<(&'static str, &'static str, String, &'static str)> = vec![
            ("theme", "Theme", self.theme.name().to_string(), "TUI palette (owl-cyan / nord / tokyo-night / catppuccin / gruvbox / dracula)"),
            ("terminal", "Terminal Shell", self.shell.clone(), "Shell used by ! / !! and AI environment context"),
            ("mode", "Agent Mode", self.mode.name().to_string(), self.mode.description()),
            ("thinking", "Thinking Level", self.thinking_level.clone(), "Default reasoning effort (off / minimal / low / medium / high / xhigh / max)"),
            ("yolo", "YOLO Auto-Approve", on_off(self.yolo).to_string(), "Skip permission prompts for non-blocked tools"),
            ("codemode", "Codemode", self.codemode.clone(), "Scripted tool-batching tool (off / on / only)"),
            ("show_thinking", "Show Thinking", on_off(self.show_thinking).to_string(), "Display thinking cards in transcript (Ctrl+T)"),
            ("expand_tools", "Expand Tool Output", on_off(self.expand_tools).to_string(), "Show 40 tool output lines instead of 3 (Ctrl+O)"),
            ("reduced_motion", "Reduced Motion", on_off(self.reduced_motion).to_string(), "Disable spinner animation & smooth scroll easing"),
            ("quiet_startup", "Quiet Header", on_off(self.quiet_startup).to_string(), "Compact 1-line header instead of full sleepy owl banner"),
            ("bell_on_done", "Bell on Done", on_off(self.bell_on_done).to_string(), "Ring terminal bell when turn completes or approval is needed"),
            ("auto_compact", "Auto-Compact", on_off(self.auto_compact).to_string(), "Automatically summarize context when threshold is reached"),
            ("compact_threshold_pct", "Compact Threshold", format!("{}%", self.compact_threshold_pct), "Context % that triggers auto-compaction (70 / 80 / 85 / 90)"),
            ("max_turns", "Max Turns", self.max_turns.to_string(), "Max tool-loop turns per prompt (20 / 40 / 80)"),
            ("retry_max", "Transient Retries", self.retry_max.to_string(), "Auto-retries on 429/5xx/timeout (0 / 2 / 3)"),
            ("double_esc_rewind", "Double-Esc Rewind", on_off(self.double_esc_rewind).to_string(), "Press Esc twice on empty input to open /rewind"),
            ("confirm_on_exit", "Confirm on Exit", on_off(self.confirm_on_exit).to_string(), "Require double Ctrl+C within 1.5s to quit"),
        ];
        let q = self.modal_filter.trim().to_lowercase();
        all.into_iter()
            .filter(|(k, label, val, desc)| {
                q.is_empty()
                    || k.contains(&q)
                    || label.to_lowercase().contains(&q)
                    || val.to_lowercase().contains(&q)
                    || desc.to_lowercase().contains(&q)
            })
            .map(|(k, label, val, desc)| {
                let origin = self.config_origin(k);
                (k, label, format!("{val} [{origin}]"), desc)
            })
            .collect()
    }

    pub fn filtered_session_rows(&self) -> Vec<(String, String, String)> {
        let q = self.modal_filter.trim().to_lowercase();
        self.session_rows
            .iter()
            .filter(|(path, label, preview)| {
                q.is_empty()
                    || path.to_lowercase().contains(&q)
                    || label.to_lowercase().contains(&q)
                    || preview.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    }

    /// Extract active `@query` token right before cursor for `@file` autocomplete.
    pub fn active_at_query(&self) -> Option<(usize, String)> {
        let prefix = &self.input[..self.cursor.min(self.input.len())];
        let at_pos = prefix.rfind('@')?;
        if at_pos > 0 && !prefix[..at_pos].ends_with(char::is_whitespace) {
            return None;
        }
        let query = &prefix[at_pos + 1..];
        if query.contains(char::is_whitespace) {
            return None;
        }
        Some((at_pos, query.to_string()))
    }

    pub fn at_file_matches(&self) -> Vec<String> {
        let Some((_, query)) = self.active_at_query() else {
            return Vec::new();
        };
        let q = query.to_lowercase();
        self.workspace_files
            .iter()
            .filter(|f| q.is_empty() || f.to_lowercase().contains(&q))
            .take(12)
            .cloned()
            .collect()
    }

    pub fn apply_default_thinking(&mut self) {
        if let Some(level) = self
            .model_default_thinking
            .get(&(self.provider.clone(), self.model.clone()))
        {
            self.thinking_level = level.clone();
        } else {
            self.thinking_level = "medium".to_string();
        }
    }

    pub fn total_active_ms(&self) -> u64 {
        let live = self
            .active_started
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);
        self.active_ms.saturating_add(live)
    }

    pub fn record_usage(
        &mut self,
        provider: &str,
        in_tok: u64,
        out_tok: u64,
        cache_tok: u64,
        cache_write_tok: u64,
        cost: f64,
    ) {
        self.input_tokens += in_tok;
        self.output_tokens += out_tok;
        self.cache_read_tokens += cache_tok;
        self.cache_write_tokens += cache_write_tok;
        self.cost_usd += cost;
        // All 3 providers (agy, vibe, deepseek) report total prompt tokens in `in_tok`
        // (already including `cache_tok`), so never add `cache_tok` a second time.
        let prompt = in_tok.max(cache_tok);
        if prompt > 0 {
            self.last_cache_rate = Some(((cache_tok as f64 / prompt as f64) * 100.0).min(100.0));
        }
        let entry = self
            .provider_usage
            .entry(provider.to_string())
            .or_default();
        entry.turns += 1;
        entry.input_tokens += in_tok;
        entry.output_tokens += out_tok;
        entry.cache_read_tokens += cache_tok;
        entry.cost_usd += cost;
    }
}

#[cfg(windows)]
mod win_console {
    use std::fs::{File, OpenOptions};
    use std::os::windows::io::AsRawHandle;
    use std::sync::atomic::{AtomicBool, Ordering};

    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
    static OWNED: AtomicBool = AtomicBool::new(false);

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleMode(h_console_handle: *mut std::ffi::c_void, lp_mode: *mut u32) -> i32;
        fn SetConsoleMode(h_console_handle: *mut std::ffi::c_void, dw_mode: u32) -> i32;
    }

    fn open_conin() -> Option<File> {
        OpenOptions::new().read(true).write(true).open("CONIN$").ok()
    }

    fn set_vt_input(file: &File, on: bool) -> bool {
        let handle = file.as_raw_handle();
        let mut mode: u32 = 0;
        // SAFETY: `handle` is a valid console input handle opened from CONIN$.
        unsafe {
            GetConsoleMode(handle, &mut mode) != 0
                && ((mode & ENABLE_VIRTUAL_TERMINAL_INPUT) != 0) != on
                && SetConsoleMode(handle, mode ^ ENABLE_VIRTUAL_TERMINAL_INPUT) != 0
        }
    }

    pub fn disable_vt_input() -> bool {
        OWNED.store(true, Ordering::SeqCst);
        open_conin().is_some_and(|f| set_vt_input(&f, false))
    }

    pub fn restore_vt_input(was_on: bool) {
        OWNED.store(false, Ordering::SeqCst);
        if was_on
            && let Some(f) = open_conin()
        {
            let _ = set_vt_input(&f, true);
        }
    }

    /// Re-clear ENABLE_VIRTUAL_TERMINAL_INPUT if a child process (bash.exe / powershell)
    /// flipped it back on mid-session, preventing ConPTY from leaking SGR mouse escapes as keys.
    pub fn keep_vt_input_off() {
        if !OWNED.load(Ordering::Relaxed) {
            return;
        }
        thread_local! {
            static CONIN: Option<File> = open_conin();
        }
        CONIN.with(|f| {
            if let Some(file) = f {
                let _ = set_vt_input(file, false);
            }
        });
    }
}

#[cfg(not(windows))]
mod win_console {
    pub fn disable_vt_input() -> bool {
        false
    }
    pub fn restore_vt_input(_was_on: bool) {}
    pub fn keep_vt_input_off() {}
}

pub fn keep_vt_input_off() {
    win_console::keep_vt_input_off();
}

pub struct TerminalGuard {
    pub terminal: Terminal<ResyncBackend<Stdout>>,
    vt_input_was_on: bool,
}

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let mut out = io::stdout();
            let _ = execute!(
                out,
                DisableMouseCapture,
                DisableBracketedPaste,
                LeaveAlternateScreen
            );
            if cfg!(windows) {
                let _ = execute!(out, Print("\x1b[?1006l"));
            }
            prev_hook(info);
        }));
        enable_raw_mode()?;
        let vt_input_was_on = win_console::disable_vt_input();
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
        if cfg!(windows) {
            execute!(stdout, Print("\x1b[?1006h"))?;
        }
        let backend = ResyncBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self {
            terminal,
            vt_input_was_on,
        })
    }

    /// Temporarily leave raw mode + alt-screen + mouse/paste capture to run an external
    /// process (e.g. `$EDITOR`), then restore cleanly and clear ratatui's diff buffer.
    pub fn suspend_for<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let mut out = io::stdout();
        let _ = execute!(
            out,
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        if cfg!(windows) {
            let _ = execute!(out, Print("\x1b[?1006l"));
        }
        let _ = disable_raw_mode();
        win_console::restore_vt_input(self.vt_input_was_on);
        let res = f();
        let _ = enable_raw_mode();
        self.vt_input_was_on = win_console::disable_vt_input();
        let _ = execute!(
            out,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        );
        if cfg!(windows) {
            let _ = execute!(out, Print("\x1b[?1006h"));
        }
        let _ = self.terminal.clear();
        res
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        if cfg!(windows) {
            let _ = execute!(self.terminal.backend_mut(), Print("\x1b[?1006l"));
        }
        let _ = self.terminal.show_cursor();
        win_console::restore_vt_input(self.vt_input_was_on);
    }
}


pub enum TuiAction {
    None,
    Redraw,
    Submit(String),
    SubmitFollowUp(String),
    RecallQueued,
    OpenExternalEditor,
    CopyLastReply,
    TrustDecision(bool),
    Interrupt,
    CycleModel,
    SaveScope,
    ConfigCycleKey(String),
    ApprovalDecision(usize),
    QuestionAnswer(String),
    ToggleDebug,
    DebugClear,
    DebugCycleFilter,
    PasteClipboard,
    Pasted(String),
    Quit,
}

