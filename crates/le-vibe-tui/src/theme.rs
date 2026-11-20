use ratatui::style::Color;

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
