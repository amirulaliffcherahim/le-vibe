use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

/// Differential terminal frame buffer for le-vibe TUI.
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

#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub accent: Color,
    pub secondary: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub card_thinking_bg: Color,
    pub card_ok_bg: Color,
    pub card_err_bg: Color,
    pub card_quiet_bg: Color,
    pub branch: Color,
    pub git_status: Color,
    pub input: Color,
    pub output: Color,
    pub cache: Color,
    pub tps: Color,
    pub cost: Color,
    pub context: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    OwlCyan,
    Nord,
    TokyoNight,
    Catppuccin,
    Gruvbox,
    Dracula,
}

pub const ALL_THEMES: &[ThemeId] = &[
    ThemeId::OwlCyan,
    ThemeId::Nord,
    ThemeId::TokyoNight,
    ThemeId::Catppuccin,
    ThemeId::Gruvbox,
    ThemeId::Dracula,
];

impl ThemeId {
    pub fn name(self) -> &'static str {
        match self {
            ThemeId::OwlCyan => "owl-cyan",
            ThemeId::Nord => "nord",
            ThemeId::TokyoNight => "tokyo-night",
            ThemeId::Catppuccin => "catppuccin",
            ThemeId::Gruvbox => "gruvbox",
            ThemeId::Dracula => "dracula",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ThemeId::OwlCyan => "Signature electric cyan & lavender sleepy owl palette",
            ThemeId::Nord => "Arctic frost blue & aurora polar night palette",
            ThemeId::TokyoNight => "Deep indigo & neon storm blue Tokyo Night palette",
            ThemeId::Catppuccin => "Soothing pastel Mocha sapphire & mauve palette",
            ThemeId::Gruvbox => "Warm retro Gruvbox aqua, amber & olive palette",
            ThemeId::Dracula => "Vibrant Dracula cyan, pink & purple palette",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        ALL_THEMES
            .iter()
            .copied()
            .find(|t| t.name().eq_ignore_ascii_case(s.trim()))
    }

    pub fn palette(self) -> ThemePalette {
        match self {
            ThemeId::OwlCyan => ThemePalette {
                accent: Color::Rgb(86, 212, 221),
                secondary: Color::Rgb(198, 120, 221),
                success: Color::Rgb(152, 195, 121),
                warning: Color::Rgb(229, 192, 123),
                error: Color::Rgb(224, 108, 117),
                card_thinking_bg: Color::Rgb(22, 27, 36),
                card_ok_bg: Color::Rgb(20, 33, 28),
                card_err_bg: Color::Rgb(42, 24, 28),
                card_quiet_bg: Color::Rgb(24, 26, 32),
                branch: Color::Rgb(86, 212, 221),
                git_status: Color::Rgb(229, 192, 123),
                input: Color::Rgb(86, 212, 221),
                output: Color::Rgb(198, 120, 221),
                cache: Color::Rgb(152, 195, 121),
                tps: Color::Rgb(97, 175, 239),
                cost: Color::Rgb(229, 192, 123),
                context: Color::Rgb(171, 178, 191),
            },
            ThemeId::Nord => ThemePalette {
                accent: Color::Rgb(136, 192, 208),
                secondary: Color::Rgb(180, 142, 173),
                success: Color::Rgb(163, 190, 140),
                warning: Color::Rgb(235, 203, 139),
                error: Color::Rgb(191, 97, 106),
                card_thinking_bg: Color::Rgb(46, 52, 64),
                card_ok_bg: Color::Rgb(42, 54, 52),
                card_err_bg: Color::Rgb(58, 42, 46),
                card_quiet_bg: Color::Rgb(43, 48, 59),
                branch: Color::Rgb(163, 190, 140),
                git_status: Color::Rgb(235, 203, 139),
                input: Color::Rgb(136, 192, 208),
                output: Color::Rgb(180, 142, 173),
                cache: Color::Rgb(163, 190, 140),
                tps: Color::Rgb(129, 161, 193),
                cost: Color::Rgb(208, 135, 112),
                context: Color::Rgb(216, 222, 233),
            },
            ThemeId::TokyoNight => ThemePalette {
                accent: Color::Rgb(122, 162, 247),
                secondary: Color::Rgb(187, 154, 247),
                success: Color::Rgb(158, 206, 106),
                warning: Color::Rgb(224, 175, 104),
                error: Color::Rgb(247, 118, 142),
                card_thinking_bg: Color::Rgb(26, 27, 38),
                card_ok_bg: Color::Rgb(24, 34, 32),
                card_err_bg: Color::Rgb(44, 24, 32),
                card_quiet_bg: Color::Rgb(28, 30, 42),
                branch: Color::Rgb(158, 206, 106),
                git_status: Color::Rgb(224, 175, 104),
                input: Color::Rgb(125, 207, 255),
                output: Color::Rgb(187, 154, 247),
                cache: Color::Rgb(115, 218, 202),
                tps: Color::Rgb(122, 162, 247),
                cost: Color::Rgb(255, 158, 100),
                context: Color::Rgb(192, 202, 245),
            },
            ThemeId::Catppuccin => ThemePalette {
                accent: Color::Rgb(137, 180, 250),
                secondary: Color::Rgb(203, 166, 247),
                success: Color::Rgb(166, 227, 161),
                warning: Color::Rgb(249, 226, 175),
                error: Color::Rgb(243, 139, 168),
                card_thinking_bg: Color::Rgb(30, 30, 46),
                card_ok_bg: Color::Rgb(28, 38, 36),
                card_err_bg: Color::Rgb(48, 28, 36),
                card_quiet_bg: Color::Rgb(33, 33, 48),
                branch: Color::Rgb(166, 227, 161),
                git_status: Color::Rgb(249, 226, 175),
                input: Color::Rgb(148, 226, 213),
                output: Color::Rgb(245, 194, 231),
                cache: Color::Rgb(166, 227, 161),
                tps: Color::Rgb(137, 220, 235),
                cost: Color::Rgb(250, 179, 135),
                context: Color::Rgb(205, 214, 244),
            },
            ThemeId::Gruvbox => ThemePalette {
                accent: Color::Rgb(142, 192, 124),
                secondary: Color::Rgb(211, 134, 155),
                success: Color::Rgb(184, 187, 38),
                warning: Color::Rgb(250, 189, 47),
                error: Color::Rgb(251, 73, 52),
                card_thinking_bg: Color::Rgb(40, 40, 40),
                card_ok_bg: Color::Rgb(36, 44, 32),
                card_err_bg: Color::Rgb(54, 30, 28),
                card_quiet_bg: Color::Rgb(38, 38, 38),
                branch: Color::Rgb(184, 187, 38),
                git_status: Color::Rgb(250, 189, 47),
                input: Color::Rgb(142, 192, 124),
                output: Color::Rgb(211, 134, 155),
                cache: Color::Rgb(184, 187, 38),
                tps: Color::Rgb(131, 165, 152),
                cost: Color::Rgb(254, 128, 25),
                context: Color::Rgb(235, 219, 178),
            },
            ThemeId::Dracula => ThemePalette {
                accent: Color::Rgb(139, 233, 253),
                secondary: Color::Rgb(255, 121, 198),
                success: Color::Rgb(80, 250, 123),
                warning: Color::Rgb(241, 250, 140),
                error: Color::Rgb(255, 85, 85),
                card_thinking_bg: Color::Rgb(40, 42, 54),
                card_ok_bg: Color::Rgb(32, 46, 40),
                card_err_bg: Color::Rgb(56, 32, 38),
                card_quiet_bg: Color::Rgb(36, 38, 48),
                branch: Color::Rgb(80, 250, 123),
                git_status: Color::Rgb(241, 250, 140),
                input: Color::Rgb(139, 233, 253),
                output: Color::Rgb(255, 121, 198),
                cache: Color::Rgb(80, 250, 123),
                tps: Color::Rgb(189, 147, 249),
                cost: Color::Rgb(255, 184, 108),
                context: Color::Rgb(248, 248, 242),
            },
        }
    }

    pub fn accent(self) -> Color {
        self.palette().accent
    }

    pub fn secondary(self) -> Color {
        self.palette().secondary
    }
}

#[derive(Debug, Clone)]
pub struct SlashCommandSpec {
    pub name: &'static str,
    pub hint: &'static str,
    pub description: &'static str,
    pub picker_on_bare: bool,
}

pub const BUILTIN_SLASH_COMMANDS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        name: "/model",
        hint: "[provider/model]",
        description: "Pick model · Tab: all/scoped · Space: pin/unpin · Ctrl+S: save",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/provider",
        hint: "[vibe|deepseek|agy]",
        description: "Open provider selector modal or switch active provider",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/mode",
        hint: "[normal|plan|execute|ask|rea]",
        description: "Switch agent mode (Shift+Tab cycles: normal · plan · execute · ask · rea)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/yolo",
        hint: "",
        description: "Toggle auto-approve (yolo) permission mode for tools",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/thinking",
        hint: "<off|low|medium|high|max>",
        description: "Set reasoning effort (routes the thinking variant)",
        picker_on_bare: false,
    },
    SlashCommandSpec {
        name: "/login",
        hint: "[vibe|deepseek|agy]",
        description: "View auth status for all providers & sign in (OIDC / OAuth / API key)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/logout",
        hint: "[vibe|deepseek|agy]",
        description: "Remove saved credentials for a provider",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/usage",
        hint: "",
        description: "Live plan quota, budget & balance for vibe, deepseek, agy",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/theme",
        hint: "[owl-cyan|nord|tokyo-night|catppuccin|gruvbox|dracula]",
        description: "Switch TUI color theme (owl, cards, editor & footer)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/term",
        hint: "[bash|powershell|pwsh|cmd|zsh|sh|<custom>]",
        description: "Select terminal shell for ! / !! commands (persisted)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/config",
        hint: "",
        description: "Open le-vibe settings modal (theme, shell, mode, yolo, thinking, motion)",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/reload",
        hint: "",
        description: "Reload models, auth status, SKILL.md resources, and git branch",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/compact",
        hint: "",
        description: "Manually compact the session context",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/tree",
        hint: "",
        description: "Inspect session DAG tree history",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/skills",
        hint: "",
        description: "List discovered SKILL.md resources",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/clear",
        hint: "",
        description: "Clear transcript viewport",
        picker_on_bare: true,
    },
    SlashCommandSpec {
        name: "/quit",
        hint: "",
        description: "Quit le-vibe",
        picker_on_bare: true,
    },
];

pub fn filter_slash_commands(input: &str) -> Vec<&'static SlashCommandSpec> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') || trimmed.contains(' ') {
        return Vec::new();
    }
    BUILTIN_SLASH_COMMANDS
        .iter()
        .filter(|cmd| cmd.name.starts_with(trimmed))
        .collect()
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

#[derive(Debug, Clone)]
pub enum TranscriptBlock {
    User(String),
    Thinking(String),
    Assistant(String),
    Tool { name: String, status: String, output: String },
    Script { command: String, output: String, in_context: bool },
    System(String),
    /// Preformatted output (reports, tables): rendered verbatim, without a bullet or indent.
    Report(String),
}

#[derive(Debug, Clone, Default)]
pub struct ProviderUsageStats {
    pub turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    Normal,
    Plan,
    Execute,
    Ask,
    Rea,
}

pub const ALL_MODES: &[AgentMode] = &[
    AgentMode::Normal,
    AgentMode::Plan,
    AgentMode::Execute,
    AgentMode::Ask,
    AgentMode::Rea,
];

impl AgentMode {
    pub fn name(self) -> &'static str {
        match self {
            AgentMode::Normal => "normal",
            AgentMode::Plan => "plan",
            AgentMode::Execute => "execute",
            AgentMode::Ask => "ask",
            AgentMode::Rea => "rea",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            AgentMode::Normal => "Standard coding assistant (read/write/edit/grep/ls/shell)",
            AgentMode::Plan => "Read-only architecture & planning mode (no file edits or shell)",
            AgentMode::Execute => "Autonomous execution mode (full tools + verify)",
            AgentMode::Ask => "Read-only Q&A and code explanation mode",
            AgentMode::Rea => "Reverse-engineering & binary/app inspection mode (read/grep/ls/shell)",
        }
    }

    pub fn next(self) -> Self {
        match self {
            AgentMode::Normal => AgentMode::Plan,
            AgentMode::Plan => AgentMode::Execute,
            AgentMode::Execute => AgentMode::Ask,
            AgentMode::Ask => AgentMode::Rea,
            AgentMode::Rea => AgentMode::Normal,
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
            AgentMode::Normal | AgentMode::Execute => None,
            AgentMode::Plan | AgentMode::Ask => {
                Some(vec!["read".into(), "grep".into(), "ls".into()])
            }
            AgentMode::Rea => Some(vec![
                "read".into(),
                "grep".into(),
                "ls".into(),
                "bash".into(),
                "powershell".into(),
            ]),
        }
    }

    pub fn system_prompt(self) -> &'static str {
        match self {
            AgentMode::Normal => "You are le-vibe, a concise coding assistant.",
            AgentMode::Plan => "You are le-vibe in PLAN mode. Inspect files read-only and produce a concrete, step-by-step implementation plan without modifying files.",
            AgentMode::Execute => "You are le-vibe in EXECUTE mode. Implement the requested changes end-to-end with minimal diff and verify with real tests/builds.",
            AgentMode::Ask => "You are le-vibe in ASK mode. Answer questions concisely using read-only inspection only.",
            AgentMode::Rea => "You are le-vibe in REA (reverse-engineering) mode. Analyze binaries, protocols, strings, exports, and app internals concisely.",
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    ModelPicker,
    ProviderPicker,
    ModePicker,
    ThemePicker,
    TermPicker,
    ConfigModal,
    LoginPicker,
    LogoutPicker,
    UsageModal,
    ApprovalModal,
    QuestionModal,
}

pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub type QuestionPrompt = (String, String, Vec<(String, String)>);

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
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub last_cache_rate: Option<f64>,
    pub last_tps: Option<f64>,
    pub cost_usd: f64,
    pub active_ms: u64,
    pub active_started: Option<Instant>,
    pub provider_usage: BTreeMap<String, ProviderUsageStats>,
    pub provider_auth: BTreeMap<String, (bool, String)>,
    pub provider_quota_badge: BTreeMap<String, String>,
    pub status_text: Option<String>,
    pub is_busy: bool,
    pub yolo: bool,
    pub approval_prompt: Option<(String, String, String)>,
    pub question_prompt: Option<QuestionPrompt>,
    pub question_custom: String,
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
            theme: ThemeId::OwlCyan,
            shell: if cfg!(windows) {
                "powershell".to_string()
            } else {
                "bash".to_string()
            },
            model_default_thinking: BTreeMap::new(),
            context_window: 256_000,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            last_cache_rate: None,
            last_tps: None,
            cost_usd: 0.0,
            active_ms: 0,
            active_started: None,
            provider_usage,
            provider_auth: BTreeMap::new(),
            provider_quota_badge: BTreeMap::new(),
            status_text: None,
            is_busy: false,
            yolo: false,
            approval_prompt: None,
            question_prompt: None,
            question_custom: String::new(),
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
            blocks: Vec::new(),
            available_models: Vec::new(),
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
        if !self.scope_view {
            return self.available_models.clone();
        }
        self.available_models
            .iter()
            .filter(|(p, id, _)| self.is_scoped(p, id))
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
        cost: f64,
    ) {
        self.input_tokens += in_tok;
        self.output_tokens += out_tok;
        self.cache_read_tokens += cache_tok;
        self.cost_usd += cost;
        let prompt = in_tok + cache_tok;
        if prompt > 0 {
            self.last_cache_rate = Some((cache_tok as f64 / prompt as f64) * 100.0);
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

pub struct TerminalGuard {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let mut out = io::stdout();
            let _ = execute!(out, LeaveAlternateScreen);
            prev_hook(info);
        }));
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub fn draw_frame(frame: &mut Frame<'_>, state: &TuiState) {
    let area = frame.area();
    if area.width < 40 || area.height < 12 {
        let msg = Paragraph::new(format!(
            "Terminal too small ({}x{}). Minimum is 40x12.",
            area.width, area.height
        ))
        .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Sleepy Owl Header
            Constraint::Min(5),    // Transcript viewport
            Constraint::Length(1), // Dedicated working/status strip above editor
            Constraint::Length(3), // Untouched bordered Editor box
            Constraint::Length(3), // pi-footer (identity/git, usage/model, mode+quota/runtime)
        ])
        .split(area);

    draw_header(frame, chunks[0], state);
    draw_transcript(frame, chunks[1], state);
    draw_status_strip(frame, chunks[2], state);
    draw_editor(frame, chunks[3], state);
    draw_footer(frame, chunks[4], state);

    let matches = filter_slash_commands(&state.input);
    if !matches.is_empty() && state.modal == ActiveModal::None {
        draw_slash_popup(frame, chunks[3], &matches, state.selected_slash_idx, state.theme);
    }
    if state.modal != ActiveModal::None {
        draw_modal(frame, area, state);
    }
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    let secondary = state.theme.secondary();
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "  ,___,  ",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "le-vibe ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{} ", state.version),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("zZz", Style::default().fg(secondary)),
        ]),
        Line::from(vec![
            Span::styled(
                "  (-v-)  ",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("esc", Style::default().fg(Color::Gray)),
            Span::styled(" interrupt · ", Style::default().fg(Color::DarkGray)),
            Span::styled("ctrl+c", Style::default().fg(Color::Gray)),
            Span::styled(" clear/exit · ", Style::default().fg(Color::DarkGray)),
            Span::styled("ctrl+p", Style::default().fg(Color::Gray)),
            Span::styled(" cycle model · ", Style::default().fg(Color::DarkGray)),
            Span::styled("shift+tab", Style::default().fg(Color::Gray)),
            Span::styled(" mode · ", Style::default().fg(Color::DarkGray)),
            Span::styled("!", Style::default().fg(Color::Gray)),
            Span::styled(" bash · ", Style::default().fg(Color::DarkGray)),
            Span::styled("/", Style::default().fg(Color::Gray)),
            Span::styled(" commands", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" /)_(_\\  ", Style::default().fg(accent)),
            Span::styled(
                "Rust Pi harness · Unified Python provider engine (Vibe, DeepSeek, Antigravity)",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

struct CardStyle {
    bar: Color,
    title: Color,
    bg: Color,
    body: Style,
}

fn push_shaded_card(
    lines: &mut Vec<Line<'static>>,
    area_width: u16,
    title: &str,
    body: &str,
    style: CardStyle,
) {
    let total_w = (area_width as usize).saturating_sub(2).max(20);
    let inner_w = total_w.saturating_sub(4);
    let bg_style = Style::default().bg(style.bg);
    let bar_style = Style::default().fg(style.bar).bg(style.bg);

    // Top breathing room
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(" ".repeat(total_w), bg_style),
    ]));

    // Header line inside shaded card
    let header_pad = inner_w.saturating_sub(title.chars().count());
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("▎ ", bar_style),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(style.title)
                .bg(style.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(header_pad + 2), bg_style),
    ]));

    let raw_lines: Vec<&str> = if body.is_empty() {
        Vec::new()
    } else {
        body.lines().collect()
    };
    for raw in raw_lines {
        let mut remaining = raw;
        loop {
            let chunk: String = remaining.chars().take(inner_w).collect();
            let consumed = chunk.len();
            let pad = inner_w.saturating_sub(chunk.chars().count());
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled("▎ ", bar_style),
                Span::styled(chunk, style.body.bg(style.bg)),
                Span::styled(" ".repeat(pad + 2), bg_style),
            ]));
            remaining = &remaining[consumed..];
            if remaining.is_empty() {
                break;
            }
        }
    }

    // Bottom breathing room
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(" ".repeat(total_w), bg_style),
    ]));
}

fn draw_transcript(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let accent = pal.accent;
    let secondary = pal.secondary;
    let mut lines: Vec<Line<'static>> = Vec::new();

    for block in &state.blocks {
        match block {
            TranscriptBlock::User(text) => {
                lines.push(Line::from(""));
                for (idx, l) in text.lines().enumerate() {
                    let prefix = if idx == 0 { " › " } else { "   " };
                    lines.push(Line::from(vec![
                        Span::styled(
                            prefix,
                            Style::default().fg(accent).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            l.to_string(),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
            }
            TranscriptBlock::Thinking(text) => {
                if !text.is_empty() {
                    lines.push(Line::from(""));
                    push_shaded_card(
                        &mut lines,
                        area.width,
                        "thinking",
                        text,
                        CardStyle {
                            bar: secondary,
                            title: secondary,
                            bg: pal.card_thinking_bg,
                            body: Style::default()
                                .fg(Color::Gray)
                                .add_modifier(Modifier::ITALIC),
                        },
                    );
                }
            }
            TranscriptBlock::Assistant(text) => {
                if !text.is_empty() {
                    lines.push(Line::from(""));
                    for l in text.lines() {
                        lines.push(Line::from(vec![
                            Span::raw("   "),
                            Span::styled(l.to_string(), Style::default().fg(Color::Gray)),
                        ]));
                    }
                }
            }
            TranscriptBlock::Tool {
                name,
                status,
                output,
            } => {
                lines.push(Line::from(""));
                let is_err = status == "failed";
                let title = format!("{name} · {status}");
                let preview = if output.trim().is_empty() {
                    ""
                } else {
                    output.trim()
                };
                push_shaded_card(
                    &mut lines,
                    area.width,
                    &title,
                    preview,
                    CardStyle {
                        bar: if is_err { pal.error } else { pal.success },
                        title: if is_err { pal.error } else { pal.warning },
                        bg: if is_err { pal.card_err_bg } else { pal.card_ok_bg },
                        body: Style::default().fg(Color::Gray),
                    },
                );
            }
            TranscriptBlock::Script {
                command,
                output,
                in_context,
            } => {
                lines.push(Line::from(""));
                let tag = if *in_context {
                    format!("$ {command}")
                } else {
                    format!("$ {command} (no context)")
                };
                let body = output.trim();
                push_shaded_card(
                    &mut lines,
                    area.width,
                    &tag,
                    body,
                    CardStyle {
                        bar: if *in_context { pal.success } else { Color::DarkGray },
                        title: if *in_context { pal.success } else { Color::Gray },
                        bg: if *in_context { pal.card_ok_bg } else { pal.card_quiet_bg },
                        body: Style::default().fg(Color::Gray),
                    },
                );
            }
            TranscriptBlock::System(text) => {
                lines.push(Line::from(""));
                for l in text.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("   • ", Style::default().fg(accent)),
                        Span::styled(l.to_string(), Style::default().fg(Color::Gray)),
                    ]));
                }
            }
            TranscriptBlock::Report(text) => {
                lines.push(Line::from(""));
                for l in text.lines() {
                    if l.is_empty() {
                        lines.push(Line::from(""));
                    } else if l.starts_with(' ') {
                        // Detail lines (bars, budget) stay indented and dim.
                        lines.push(Line::from(Span::styled(
                            l.to_string(),
                            Style::default().fg(Color::Gray),
                        )));
                    } else {
                        lines.push(Line::from(Span::styled(
                            l.to_string(),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        )));
                    }
                }
            }
        }
    }

    let visible_height = area.height.saturating_sub(1);
    let total_lines = lines.len() as u16;
    let auto_scroll = total_lines.saturating_sub(visible_height);
    let target_scroll = auto_scroll.saturating_sub(state.scroll_offset);
    let cur = state.rendered_scroll.get();
    let next_scroll = if state.reduced_motion || cur == target_scroll {
        target_scroll
    } else if cur < target_scroll {
        let step = ((target_scroll - cur) / 3).max(1);
        (cur + step).min(target_scroll)
    } else {
        let step = ((cur - target_scroll) / 3).max(1);
        cur.saturating_sub(step).max(target_scroll)
    };
    state.rendered_scroll.set(next_scroll);

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((next_scroll, 0));
    frame.render_widget(paragraph, area);
}

fn draw_status_strip(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let Some(status) = &state.status_text else {
        return;
    };
    let accent = state.theme.accent();
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", state.spinner_glyph()),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(status.clone(), Style::default().fg(Color::Gray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_editor(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(accent));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let display_line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(state.input.clone(), Style::default().fg(Color::White)),
    ]);
    frame.render_widget(Paragraph::new(display_line), inner);

    if !state.is_busy && state.modal == ActiveModal::None {
        let cursor_x = inner.x + 1 + (state.cursor as u16);
        let cursor_y = inner.y;
        if cursor_x < inner.x + inner.width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

pub fn format_duration_ms(milliseconds: u64) -> String {
    let total = milliseconds / 1_000;
    let hours = total / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn thinking_color(level: &str) -> Color {
    match level {
        "minimal" => Color::Indexed(46),
        "low" => Color::Indexed(82),
        "medium" => Color::Indexed(118),
        "high" => Color::Indexed(220),
        "xhigh" => Color::Indexed(208),
        "max" => Color::Indexed(196),
        _ => Color::DarkGray,
    }
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let c_branch = pal.branch;
    let c_git_status = pal.git_status;
    let c_input = pal.input;
    let c_output = pal.output;
    let c_cache = pal.cache;
    let c_tps = pal.tps;
    let c_cost = pal.cost;
    let c_context = pal.context;

    // Line 1: repo · branch [git badges] (or dir name when not in git)
    let mut line1_spans: Vec<Span<'_>> = vec![Span::raw(" ")];
    let repo_label = if state.repo_name.is_empty() {
        state.cwd.clone()
    } else {
        state.repo_name.clone()
    };
    if let Some(branch) = &state.git_branch {
        line1_spans.push(Span::styled(repo_label, Style::default().fg(Color::DarkGray)));
        line1_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
        line1_spans.push(Span::styled(branch.clone(), Style::default().fg(c_branch)));
    } else {
        line1_spans.push(Span::styled(repo_label, Style::default().fg(c_branch)));
    }
    if !state.git_badges.is_empty() {
        line1_spans.push(Span::raw(" "));
        line1_spans.push(Span::styled(
            state.git_badges.clone(),
            Style::default().fg(c_git_status),
        ));
    }

    // Line 2: ↑ in · ↓ out · ↺ cache% · ⚡ t/s · $ cost · ◔ ctx%   <pad>   (provider) model • thinking
    let cache_str = match state.last_cache_rate {
        Some(rate) => format!("↺ {rate:.1}%"),
        None => "↺ —".to_string(),
    };
    let tps_str = match state.last_tps {
        Some(tps) => format!("⚡ {tps:.1} t/s"),
        None => "⚡ —".to_string(),
    };
    let ctx_pct = if state.context_window > 0 {
        (state.input_tokens as f64 / state.context_window as f64) * 100.0
    } else {
        0.0
    };

    let stats: [(&str, String, Color); 6] = [
        ("", format!("↑ {}", format_tokens(state.input_tokens)), c_input),
        ("", format!("↓ {}", format_tokens(state.output_tokens)), c_output),
        ("", cache_str, c_cache),
        ("", tps_str, c_tps),
        ("", format!("$ {:.3}", state.cost_usd), c_cost),
        ("", format!("◔ {ctx_pct:.1}%"), c_context),
    ];

    let mut line2_spans: Vec<Span<'_>> = vec![Span::raw(" ")];
    let mut left_len = 1usize;
    for (idx, (_, text, col)) in stats.iter().enumerate() {
        if idx > 0 {
            line2_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
            left_len += 3;
        }
        left_len += text.chars().count();
        line2_spans.push(Span::styled(text.clone(), Style::default().fg(*col)));
    }

    let right_model = format!("({}) {} ", state.provider, state.model);
    let right_sep = "• ";
    let right_think = format!("{} ", state.thinking_level);
    let right_len =
        right_model.chars().count() + right_sep.chars().count() + right_think.chars().count();
    let pad2 = (area.width as usize).saturating_sub(left_len + right_len);
    line2_spans.push(Span::raw(" ".repeat(pad2)));
    line2_spans.push(Span::styled(right_model, Style::default().fg(Color::White)));
    line2_spans.push(Span::styled(right_sep, Style::default().fg(Color::DarkGray)));
    line2_spans.push(Span::styled(
        right_think,
        Style::default().fg(thinking_color(&state.thinking_level)),
    ));

    // Line 3: left = ▸ MODE · provider mini quota bar, right = ◷ active runtime timer
    let mode_color = match state.mode {
        AgentMode::Normal => pal.accent,
        AgentMode::Plan => pal.warning,
        AgentMode::Execute => pal.success,
        AgentMode::Ask => pal.tps,
        AgentMode::Rea => pal.secondary,
    };
    let mode_pill = format!(" ▸ {}", state.mode.name());
    let quota_badge = state
        .provider_quota_badge
        .get(&state.provider)
        .cloned()
        .unwrap_or_default();
    let runtime = format!("◷ {} ", format_duration_ms(state.total_active_ms()));

    let mut left3_len = mode_pill.chars().count();
    let mut line3_spans = vec![Span::styled(
        mode_pill,
        Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
    )];
    if !quota_badge.is_empty() {
        let sep = " · ";
        left3_len += sep.chars().count() + quota_badge.chars().count();
        line3_spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));
        line3_spans.push(Span::styled(quota_badge, Style::default().fg(Color::Gray)));
    }
    let perm_label = if state.yolo { "yolo" } else { "ask" };
    let perm_color = if state.yolo {
        pal.warning
    } else {
        Color::DarkGray
    };
    let right3_len = perm_label.chars().count() + 3 + runtime.chars().count();
    let pad3 = (area.width as usize).saturating_sub(left3_len + right3_len);
    line3_spans.push(Span::raw(" ".repeat(pad3)));
    line3_spans.push(Span::styled(
        perm_label,
        if state.yolo {
            Style::default().fg(perm_color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(perm_color)
        },
    ));
    line3_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
    line3_spans.push(Span::styled(runtime, Style::default().fg(Color::DarkGray)));
    let line3 = Line::from(line3_spans);

    let lines = vec![Line::from(line1_spans), Line::from(line2_spans), line3];
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_slash_popup(
    frame: &mut Frame<'_>,
    editor_area: Rect,
    matches: &[&'static SlashCommandSpec],
    selected_idx: usize,
    theme: ThemeId,
) {
    let accent = theme.accent();
    let height = (matches.len() as u16 + 2).min(16);
    let y = editor_area.y.saturating_sub(height);
    // Span full terminal width (with 1-char margin) so wide terminals never clip descriptions.
    let popup_area = Rect {
        x: editor_area.x + 1,
        y,
        width: editor_area.width.saturating_sub(2),
        height,
    };

    let max_cmd_len = matches
        .iter()
        .map(|c| {
            if c.hint.is_empty() {
                c.name.len()
            } else {
                c.name.len() + 1 + c.hint.len()
            }
        })
        .max()
        .unwrap_or(14)
        .min((popup_area.width as usize / 2).max(20))
        + 3;

    frame.render_widget(Clear, popup_area);
    let items: Vec<ListItem<'_>> = matches
        .iter()
        .enumerate()
        .map(|(idx, cmd)| {
            let is_sel = idx == selected_idx;
            let full_cmd = if cmd.hint.is_empty() {
                cmd.name.to_string()
            } else {
                format!("{} {}", cmd.name, cmd.hint)
            };
            let pad = " ".repeat(max_cmd_len.saturating_sub(full_cmd.len()));
            let pointer = if is_sel { " ▸ " } else { "   " };
            let name_style = if is_sel {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let hint_style = if is_sel {
                Style::default().fg(Color::Gray)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let desc_style = if is_sel {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let mut spans = vec![
                Span::styled(pointer, name_style),
                Span::styled(cmd.name, name_style),
            ];
            if !cmd.hint.is_empty() {
                spans.push(Span::styled(format!(" {}", cmd.hint), hint_style));
            }
            spans.push(Span::raw(pad));
            spans.push(Span::styled(cmd.description, desc_style));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                " Commands (↑/↓ navigate · Tab complete · Enter run) ",
                Style::default().fg(accent),
            )),
    );
    frame.render_widget(list, popup_area);
}

fn draw_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    if state.modal == ActiveModal::UsageModal {
        draw_usage_modal(frame, area, state);
        return;
    }
    if state.modal == ActiveModal::ApprovalModal {
        draw_approval_modal(frame, area, state);
        return;
    }
    if state.modal == ActiveModal::QuestionModal {
        draw_question_modal(frame, area, state);
        return;
    }
    let (title, rows): (&str, Vec<(String, String, String)>) = match state.modal {
        ActiveModal::None
        | ActiveModal::UsageModal
        | ActiveModal::ApprovalModal
        | ActiveModal::QuestionModal => return,
        ActiveModal::ModelPicker => (
            if state.scope_view {
                " Scoped Models · Tab: all · Space: add/remove · Ctrl+S: save · Enter · Esc "
            } else {
                " All Models · Tab: scoped · Space: add/remove · Ctrl+S: save · Enter · Esc "
            },
            state
                .picker_rows()
                .iter()
                .map(|(p, id, name)| {
                    let marker = if state.is_scoped(p, id) { "●" } else { "○" };
                    (format!("{marker} ({p})"), id.clone(), name.clone())
                })
                .collect(),
        ),
        ActiveModal::ProviderPicker => (
            " Select Provider (↑/↓ Enter Esc) ",
            vec![
                (
                    "vibe".into(),
                    "Mistral Vibe".into(),
                    "devstral-medium-latest / mistral-large-latest / codestral-latest".into(),
                ),
                (
                    "deepseek".into(),
                    "DeepSeek".into(),
                    "deepseek-v4-pro / deepseek-flash (1M ctx)".into(),
                ),
                (
                    "agy".into(),
                    "Google Antigravity".into(),
                    "gemini-2.5-pro / gemini-2.5-flash / claude-sonnet-4-5".into(),
                ),
            ],
        ),
        ActiveModal::ThemePicker => (
            " Select Theme (↑/↓ Enter Esc) ",
            ALL_THEMES
                .iter()
                .map(|t| (t.name().to_string(), String::new(), t.description().to_string()))
                .collect(),
        ),
        ActiveModal::ModePicker => (
            " Select Mode (↑/↓ Enter Esc · Shift+Tab cycles) ",
            ALL_MODES
                .iter()
                .map(|m| (m.name().to_string(), String::new(), m.description().to_string()))
                .collect(),
        ),
        ActiveModal::TermPicker => (
            " Select Terminal Shell (↑/↓ Enter Esc · or /term <exe>) ",
            ALL_SHELLS
                .iter()
                .map(|(name, desc)| {
                    let active = if *name == state.shell { "●" } else { " " };
                    (format!("{active} {name}"), String::new(), (*desc).to_string())
                })
                .collect(),
        ),
        ActiveModal::ConfigModal => (
            " le-vibe Settings (↑/↓ select · Enter/Space toggle/cycle · Esc close) ",
            vec![
                (
                    "Theme".into(),
                    state.theme.name().to_string(),
                    "owl-cyan / nord / tokyo-night / catppuccin / gruvbox / dracula".into(),
                ),
                (
                    "Terminal Shell".into(),
                    state.shell.clone(),
                    "Shell used by ! / !! and AI environment context".into(),
                ),
                (
                    "Agent Mode".into(),
                    state.mode.name().to_string(),
                    state.mode.description().to_string(),
                ),
                (
                    "YOLO Auto-Approve".into(),
                    if state.yolo { "on (auto-approve)".into() } else { "off (ask)".into() },
                    "Skip permission prompts for non-blocked tools".into(),
                ),
                (
                    "Thinking Level".into(),
                    state.thinking_level.clone(),
                    "Reasoning effort for active model".into(),
                ),
                (
                    "Reduced Motion".into(),
                    if state.reduced_motion { "on (static)".into() } else { "off (fluid)".into() },
                    "Disable animated spinner & smooth scroll easing".into(),
                ),
            ],
        ),
        ActiveModal::LoginPicker | ActiveModal::LogoutPicker => {
            let header = if state.modal == ActiveModal::LoginPicker {
                " Auth Status & Login — Press Enter to Login (↑/↓ Enter Esc) "
            } else {
                " Logout Provider — Press Enter to Remove Credentials (↑/↓ Enter Esc) "
            };
            let list = ["vibe", "deepseek", "agy"]
                .iter()
                .map(|p| {
                    let (authed, src) = state
                        .provider_auth
                        .get(*p)
                        .cloned()
                        .unwrap_or((false, "none".into()));
                    let badge = if authed {
                        format!("✓ authenticated ({src})")
                    } else {
                        "✗ not logged in".to_string()
                    };
                    let flow = match *p {
                        "vibe" => "Mistral OIDC browser login",
                        "agy" => "Google Antigravity OAuth browser login",
                        _ => "DEEPSEEK_API_KEY / saved key",
                    };
                    ((*p).to_string(), badge, flow.to_string())
                })
                .collect();
            (header, list)
        }
    };

    let height = (rows.len() as u16 + 4).min(area.height.saturating_sub(4));
    // Responsive width: 75% of terminal width, clamped between 64 and 116 columns.
    let target_w = ((area.width as u32 * 75) / 100) as u16;
    let width = target_w.clamp(64.min(area.width.saturating_sub(4)), area.width.saturating_sub(4).min(116));
    let popup_area = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let col1_w = rows.iter().map(|(a, _, _)| a.len()).max().unwrap_or(8) + 2;
    let col2_w = rows.iter().map(|(_, b, _)| b.len()).max().unwrap_or(0) + 3;

    frame.render_widget(Clear, popup_area);
    let items: Vec<ListItem<'_>> = rows
        .iter()
        .enumerate()
        .map(|(idx, (c1, c2, c3))| {
            let is_sel = idx == state.modal_idx;
            let pointer = if is_sel { " ▸ " } else { "   " };
            let s1 = if is_sel {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            let s2 = if is_sel {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let s3 = if is_sel {
                Style::default().fg(Color::Gray)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let pad1 = " ".repeat(col1_w.saturating_sub(c1.len()));
            let pad2 = if col2_w > 3 {
                " ".repeat(col2_w.saturating_sub(c2.len()))
            } else {
                String::new()
            };
            ListItem::new(Line::from(vec![
                Span::styled(pointer, s1),
                Span::styled(c1.clone(), s1),
                Span::raw(pad1),
                Span::styled(c2.clone(), s2),
                Span::raw(pad2),
                Span::styled(c3.clone(), s3),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(accent))
            .title(Span::styled(
                title,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(list, popup_area);
}

pub enum TuiAction {
    None,
    Submit(String),
    CycleModel,
    SaveScope,
    ConfigCycle(usize),
    ApprovalDecision(usize),
    QuestionAnswer(String),
    Quit,
}

fn draw_approval_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let Some((tool, detail, reason)) = &state.approval_prompt else {
        return;
    };
    let width = ((area.width as u32 * 76) / 100) as u16;
    let width = width.clamp(50.min(area.width.saturating_sub(2)), area.width.saturating_sub(2).min(104));
    let height = 11u16.min(area.height.saturating_sub(4));
    let popup = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let options = [
        "1. Allow once",
        "2. Allow for this session",
        "3. Enable YOLO (auto-approve all)",
        "4. Deny",
    ];
    let mut lines = vec![
        Line::from(vec![
            Span::styled(" Tool: ", Style::default().fg(Color::DarkGray)),
            Span::styled(tool.clone(), Style::default().fg(pal.warning).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  ({reason})"), Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" Target: ", Style::default().fg(Color::DarkGray)),
            Span::styled(detail.clone(), Style::default().fg(Color::White)),
        ]),
        Line::from(""),
    ];
    for (idx, opt) in options.iter().enumerate() {
        let sel = idx == state.modal_idx;
        let ptr = if sel { " ▸ " } else { "   " };
        let style = if sel {
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        lines.push(Line::from(Span::styled(format!("{ptr}{opt}"), style)));
    }
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pal.warning))
            .title(Span::styled(
                " Permission Required (↑/↓ Enter · 1-4 · Esc deny) ",
                Style::default().fg(pal.warning).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(paragraph, popup);
}

fn draw_question_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let pal = state.theme.palette();
    let Some((header, question, options)) = &state.question_prompt else {
        return;
    };
    let width = ((area.width as u32 * 78) / 100) as u16;
    let width = width.clamp(50.min(area.width.saturating_sub(2)), area.width.saturating_sub(2).min(108));
    let height = (options.len() as u16 + 7).min(area.height.saturating_sub(4));
    let popup = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {question}"),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for (idx, (label, desc)) in options.iter().enumerate() {
        let sel = idx == state.modal_idx;
        let ptr = if sel { " ▸ " } else { "   " };
        let l_style = if sel {
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{ptr}{label} "), l_style),
            Span::styled(desc.clone(), Style::default().fg(Color::DarkGray)),
        ]));
    }
    let custom_idx = options.len();
    let custom_sel = state.modal_idx == custom_idx;
    let custom_ptr = if custom_sel { " ▸ " } else { "   " };
    lines.push(Line::from(vec![
        Span::styled(
            format!("{custom_ptr}Type answer: "),
            Style::default().fg(if custom_sel { pal.accent } else { Color::DarkGray }),
        ),
        Span::styled(state.question_custom.clone(), Style::default().fg(Color::White)),
    ]));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pal.accent))
            .title(Span::styled(
                format!(" {header} (↑/↓ Enter · Esc cancel) "),
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(paragraph, popup);
}

fn draw_usage_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let accent = state.theme.accent();
    let target_w = ((area.width as u32 * 82) / 100) as u16;
    let width = target_w.clamp(
        48.min(area.width.saturating_sub(2)),
        area.width.saturating_sub(2).min(116),
    );
    let content_h = (state.usage_lines.len() as u16 + 4).max(10);
    let height = content_h.min(area.height.saturating_sub(4));
    let popup_area = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, popup_area);
    let title = if state.usage_loading {
        format!(
            " {} Usage & Quotas — refreshing... (↑/↓ scroll · r refresh · Esc close) ",
            state.spinner_glyph()
        )
    } else {
        " Usage & Quotas (↑/↓ scroll · r refresh · Esc/Enter close) ".to_string()
    };

    let mut lines: Vec<Line<'_>> = Vec::new();
    if state.usage_lines.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                " {} Fetching live plan quota, budget & balances...",
                state.spinner_glyph()
            ),
            Style::default().fg(accent),
        )));
    } else {
        for raw in &state.usage_lines {
            if raw.is_empty() {
                lines.push(Line::from(""));
            } else if !raw.starts_with(' ') {
                lines.push(Line::from(Span::styled(
                    format!(" {raw}"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    raw.clone(),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.usage_scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(accent))
                .title(Span::styled(
                    title,
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(paragraph, popup_area);
}

fn modal_len(state: &TuiState) -> usize {
    match state.modal {
        ActiveModal::None | ActiveModal::UsageModal => 0,
        ActiveModal::ModelPicker => state.picker_rows().len(),
        ActiveModal::ProviderPicker | ActiveModal::LoginPicker | ActiveModal::LogoutPicker => 3,
        ActiveModal::ModePicker => ALL_MODES.len(),
        ActiveModal::TermPicker => ALL_SHELLS.len(),
        ActiveModal::ConfigModal => 6,
        ActiveModal::ThemePicker => ALL_THEMES.len(),
        ActiveModal::ApprovalModal => 4,
        ActiveModal::QuestionModal => state
            .question_prompt
            .as_ref()
            .map(|(_, _, opts)| opts.len() + 1)
            .unwrap_or(1),
    }
}

pub fn poll_tui_action(state: &mut TuiState, timeout: Duration) -> io::Result<TuiAction> {
    if !event::poll(timeout)? {
        return Ok(TuiAction::None);
    }
    let Event::Key(key) = event::read()? else {
        return Ok(TuiAction::None);
    };
    if key.kind != KeyEventKind::Press {
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

    if state.modal != ActiveModal::None {
        let len = modal_len(state);
        if state.modal == ActiveModal::ConfigModal {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => state.modal = ActiveModal::None,
                KeyCode::Up => state.modal_idx = state.modal_idx.saturating_sub(1),
                KeyCode::Down if state.modal_idx + 1 < 6 => state.modal_idx += 1,
                KeyCode::Enter | KeyCode::Char(' ') => {
                    return Ok(TuiAction::ConfigCycle(state.modal_idx));
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
                    ActiveModal::ProviderPicker => {
                        let prov = ["vibe", "deepseek", "agy"][idx.min(2)];
                        return Ok(TuiAction::Submit(format!("/provider {prov}")));
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
                    ActiveModal::None
                    | ActiveModal::UsageModal
                    | ActiveModal::ConfigModal
                    | ActiveModal::ApprovalModal
                    | ActiveModal::QuestionModal => {}
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
            KeyCode::Char('p') => return Ok(TuiAction::CycleModel),
            _ => {}
        }
    }

    let matches = filter_slash_commands(&state.input);
    match key.code {
        KeyCode::BackTab => {
            state.mode = state.mode.next();
            return Ok(TuiAction::None);
        }
        KeyCode::Up => {
            if !matches.is_empty() {
                state.selected_slash_idx = state.selected_slash_idx.saturating_sub(1);
            } else {
                state.scroll_offset = state.scroll_offset.saturating_add(1);
            }
        }
        KeyCode::Down => {
            if !matches.is_empty() {
                if state.selected_slash_idx + 1 < matches.len() {
                    state.selected_slash_idx += 1;
                }
            } else {
                state.scroll_offset = state.scroll_offset.saturating_sub(1);
            }
        }
        KeyCode::PageUp => {
            state.scroll_offset = state.scroll_offset.saturating_add(8);
        }
        KeyCode::PageDown => {
            state.scroll_offset = state.scroll_offset.saturating_sub(8);
        }
        KeyCode::Tab => {
            if let Some(cmd) = matches.get(state.selected_slash_idx) {
                state.input = if cmd.hint.is_empty() {
                    cmd.name.to_string()
                } else {
                    format!("{} ", cmd.name)
                };
                state.cursor = state.input.len();
                state.selected_slash_idx = 0;
            }
        }
        KeyCode::Enter => {
            if !matches.is_empty()
                && let Some(cmd) = matches.get(state.selected_slash_idx)
            {
                if cmd.picker_on_bare || cmd.hint.is_empty() {
                    let submitted = cmd.name.to_string();
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
            if !submitted.is_empty() {
                return Ok(TuiAction::Submit(submitted));
            }
        }
        KeyCode::Backspace if state.cursor > 0 => {
            state.input.remove(state.cursor - 1);
            state.cursor -= 1;
            state.selected_slash_idx = 0;
        }
        KeyCode::Left => {
            state.cursor = state.cursor.saturating_sub(1);
        }
        KeyCode::Right if state.cursor < state.input.len() => {
            state.cursor += 1;
        }
        KeyCode::Esc => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffs_only_changed_lines() {
        let mut fb = FrameBuffer::new();
        let first = fb.diff(vec!["line1".into(), "line2".into()]);
        assert_eq!(first.len(), 2);

        let second = fb.diff(vec!["line1".into(), "line2-updated".into()]);
        assert_eq!(second, vec![(1, "line2-updated".to_string())]);
    }

    #[test]
    fn filters_slash_commands_live() {
        let all = filter_slash_commands("/");
        assert!(all.len() >= 10);
        let mo = filter_slash_commands("/mo");
        assert_eq!(mo.len(), 2);
        let m = filter_slash_commands("/model");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "/model");
        let u = filter_slash_commands("/us");
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].name, "/usage");
    }

    #[test]
    fn cycles_all_five_modes() {
        let mut m = AgentMode::Normal;
        let seen: Vec<&str> = (0..5)
            .map(|_| {
                let cur = m.name();
                m = m.next();
                cur
            })
            .collect();
        assert_eq!(seen, vec!["normal", "plan", "execute", "ask", "rea"]);
        assert_eq!(m, AgentMode::Normal);
    }
}
