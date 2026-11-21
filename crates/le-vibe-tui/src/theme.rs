use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub accent: Color,
    pub secondary: Color,
    pub complementary: Color,
    pub fg: Color,
    pub muted: Color,
    pub modal_bg: Color,
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
    RosePine,
    Kanagawa,
    TokyoNight,
    Catppuccin,
    Nord,
    Gruvbox,
    Dracula,
}

pub const ALL_THEMES: &[ThemeId] = &[
    ThemeId::OwlCyan,
    ThemeId::RosePine,
    ThemeId::Kanagawa,
    ThemeId::TokyoNight,
    ThemeId::Catppuccin,
    ThemeId::Nord,
    ThemeId::Gruvbox,
    ThemeId::Dracula,
];

impl ThemeId {
    pub fn name(self) -> &'static str {
        match self {
            ThemeId::OwlCyan => "owl-cyan",
            ThemeId::RosePine => "rose-pine",
            ThemeId::Kanagawa => "kanagawa",
            ThemeId::TokyoNight => "tokyo-night",
            ThemeId::Catppuccin => "catppuccin",
            ThemeId::Nord => "nord",
            ThemeId::Gruvbox => "gruvbox",
            ThemeId::Dracula => "dracula",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ThemeId::OwlCyan => "Luminous seafoam teal, soft iris & warm amber gold",
            ThemeId::RosePine => "Rosé Pine — warm rose, iris & gold on deep pine",
            ThemeId::Kanagawa => "Kanagawa Wave — crystal blue, oni violet & carp gold",
            ThemeId::TokyoNight => "Tokyo Night — electric sky, amethyst & tangerine",
            ThemeId::Catppuccin => "Catppuccin Mocha — sapphire, mauve & warm peach",
            ThemeId::Nord => "Nord — arctic frost cyan, aurora purple & terracotta",
            ThemeId::Gruvbox => "Gruvbox — warm retro aqua, goldenrod & terracotta",
            ThemeId::Dracula => "Dracula — electric cyan, neon pink & warm orange",
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
                accent: Color::Rgb(56, 232, 216),        // #38E8D8 seafoam teal (primary)
                secondary: Color::Rgb(192, 132, 252),    // #C084FC soft iris (secondary)
                complementary: Color::Rgb(245, 178, 72), // #F5B248 warm amber gold (complementary)
                fg: Color::Rgb(230, 237, 243),           // #E6EDF3 crisp ivory
                muted: Color::Rgb(139, 155, 180),        // #8B9BB4 slate
                modal_bg: Color::Rgb(19, 25, 36),        // #131924 midnight slate
                success: Color::Rgb(110, 231, 183),
                warning: Color::Rgb(245, 178, 72),
                error: Color::Rgb(248, 113, 113),
                card_thinking_bg: Color::Rgb(22, 25, 38),
                card_ok_bg: Color::Rgb(20, 34, 30),
                card_err_bg: Color::Rgb(44, 22, 28),
                card_quiet_bg: Color::Rgb(20, 25, 35),
                branch: Color::Rgb(56, 232, 216),
                git_status: Color::Rgb(245, 178, 72),
                input: Color::Rgb(56, 232, 216),
                output: Color::Rgb(192, 132, 252),
                cache: Color::Rgb(110, 231, 183),
                tps: Color::Rgb(96, 165, 250),
                cost: Color::Rgb(245, 178, 72),
                context: Color::Rgb(176, 190, 210),
            },
            ThemeId::RosePine => ThemePalette {
                accent: Color::Rgb(235, 188, 186),       // #EBBCBA rose
                secondary: Color::Rgb(196, 167, 231),    // #C4A7E7 iris
                complementary: Color::Rgb(246, 193, 119),// #F6C177 gold
                fg: Color::Rgb(224, 222, 244),           // #E0DEF4 text
                muted: Color::Rgb(144, 140, 170),        // #908CAA subtle
                modal_bg: Color::Rgb(31, 29, 46),        // #1F1D2E surface
                success: Color::Rgb(156, 207, 216),      // #9CCFD8 foam
                warning: Color::Rgb(246, 193, 119),
                error: Color::Rgb(235, 111, 146),        // #EB6F92 love
                card_thinking_bg: Color::Rgb(35, 32, 52),
                card_ok_bg: Color::Rgb(28, 38, 44),
                card_err_bg: Color::Rgb(50, 26, 38),
                card_quiet_bg: Color::Rgb(31, 29, 46),
                branch: Color::Rgb(156, 207, 216),
                git_status: Color::Rgb(246, 193, 119),
                input: Color::Rgb(156, 207, 216),
                output: Color::Rgb(196, 167, 231),
                cache: Color::Rgb(235, 188, 186),
                tps: Color::Rgb(49, 116, 143),
                cost: Color::Rgb(246, 193, 119),
                context: Color::Rgb(224, 222, 244),
            },
            ThemeId::Kanagawa => ThemePalette {
                accent: Color::Rgb(126, 156, 216),       // #7E9CD8 crystal blue
                secondary: Color::Rgb(149, 127, 184),    // #957FB8 oni violet
                complementary: Color::Rgb(230, 195, 132),// #E6C384 carp yellow
                fg: Color::Rgb(220, 215, 186),           // #DCD7BA fuji white
                muted: Color::Rgb(114, 113, 105),        // #727169 fuji gray
                modal_bg: Color::Rgb(31, 31, 40),        // #1F1F28 sumi ink 3
                success: Color::Rgb(152, 187, 108),      // #98BB6C spring green
                warning: Color::Rgb(230, 195, 132),
                error: Color::Rgb(228, 104, 118),        // #E46876 wave red
                card_thinking_bg: Color::Rgb(34, 34, 46),
                card_ok_bg: Color::Rgb(30, 38, 32),
                card_err_bg: Color::Rgb(48, 28, 32),
                card_quiet_bg: Color::Rgb(31, 31, 40),
                branch: Color::Rgb(152, 187, 108),
                git_status: Color::Rgb(230, 195, 132),
                input: Color::Rgb(127, 180, 202),
                output: Color::Rgb(149, 127, 184),
                cache: Color::Rgb(152, 187, 108),
                tps: Color::Rgb(126, 156, 216),
                cost: Color::Rgb(255, 160, 102),
                context: Color::Rgb(220, 215, 186),
            },
            ThemeId::TokyoNight => ThemePalette {
                accent: Color::Rgb(125, 207, 255),       // #7DCFFF electric sky
                secondary: Color::Rgb(187, 154, 247),    // #BB9AF7 amethyst
                complementary: Color::Rgb(255, 158, 100),// #FF9E64 tangerine
                fg: Color::Rgb(192, 202, 245),           // #C0CAF5
                muted: Color::Rgb(120, 132, 168),
                modal_bg: Color::Rgb(26, 27, 38),        // #1A1B26
                success: Color::Rgb(158, 206, 106),
                warning: Color::Rgb(224, 175, 104),
                error: Color::Rgb(247, 118, 142),
                card_thinking_bg: Color::Rgb(28, 30, 44),
                card_ok_bg: Color::Rgb(24, 36, 32),
                card_err_bg: Color::Rgb(46, 24, 34),
                card_quiet_bg: Color::Rgb(26, 28, 40),
                branch: Color::Rgb(158, 206, 106),
                git_status: Color::Rgb(255, 158, 100),
                input: Color::Rgb(125, 207, 255),
                output: Color::Rgb(187, 154, 247),
                cache: Color::Rgb(115, 218, 202),
                tps: Color::Rgb(122, 162, 247),
                cost: Color::Rgb(255, 158, 100),
                context: Color::Rgb(192, 202, 245),
            },
            ThemeId::Catppuccin => ThemePalette {
                accent: Color::Rgb(137, 180, 250),       // #89B4FA sapphire/blue
                secondary: Color::Rgb(203, 166, 247),    // #CBA6F7 mauve
                complementary: Color::Rgb(250, 179, 135),// #FAB387 peach
                fg: Color::Rgb(205, 214, 244),           // #CDD6F4
                muted: Color::Rgb(147, 153, 178),
                modal_bg: Color::Rgb(30, 30, 46),        // #1E1E2E
                success: Color::Rgb(166, 227, 161),
                warning: Color::Rgb(249, 226, 175),
                error: Color::Rgb(243, 139, 168),
                card_thinking_bg: Color::Rgb(32, 32, 50),
                card_ok_bg: Color::Rgb(28, 40, 36),
                card_err_bg: Color::Rgb(48, 28, 36),
                card_quiet_bg: Color::Rgb(30, 30, 46),
                branch: Color::Rgb(166, 227, 161),
                git_status: Color::Rgb(250, 179, 135),
                input: Color::Rgb(148, 226, 213),
                output: Color::Rgb(245, 194, 231),
                cache: Color::Rgb(166, 227, 161),
                tps: Color::Rgb(137, 220, 235),
                cost: Color::Rgb(250, 179, 135),
                context: Color::Rgb(205, 214, 244),
            },
            ThemeId::Nord => ThemePalette {
                accent: Color::Rgb(136, 192, 208),       // #88C0D0 frost
                secondary: Color::Rgb(180, 142, 173),    // #B48EAD aurora purple
                complementary: Color::Rgb(208, 135, 112),// #D08770 aurora orange
                fg: Color::Rgb(236, 239, 244),           // #ECEFF4
                muted: Color::Rgb(143, 158, 180),
                modal_bg: Color::Rgb(46, 52, 64),        // #2E3440
                success: Color::Rgb(163, 190, 140),
                warning: Color::Rgb(235, 203, 139),
                error: Color::Rgb(191, 97, 106),
                card_thinking_bg: Color::Rgb(46, 52, 64),
                card_ok_bg: Color::Rgb(40, 54, 50),
                card_err_bg: Color::Rgb(58, 40, 46),
                card_quiet_bg: Color::Rgb(42, 48, 60),
                branch: Color::Rgb(163, 190, 140),
                git_status: Color::Rgb(235, 203, 139),
                input: Color::Rgb(136, 192, 208),
                output: Color::Rgb(180, 142, 173),
                cache: Color::Rgb(163, 190, 140),
                tps: Color::Rgb(129, 161, 193),
                cost: Color::Rgb(208, 135, 112),
                context: Color::Rgb(216, 222, 233),
            },
            ThemeId::Gruvbox => ThemePalette {
                accent: Color::Rgb(142, 192, 124),       // #8EC07C aqua
                secondary: Color::Rgb(250, 189, 47),     // #FABD2F goldenrod
                complementary: Color::Rgb(254, 128, 25), // #FE8019 terracotta
                fg: Color::Rgb(235, 219, 178),           // #EBDBB2
                muted: Color::Rgb(168, 153, 132),
                modal_bg: Color::Rgb(40, 40, 40),        // #282828
                success: Color::Rgb(184, 187, 38),
                warning: Color::Rgb(250, 189, 47),
                error: Color::Rgb(251, 73, 52),
                card_thinking_bg: Color::Rgb(44, 42, 40),
                card_ok_bg: Color::Rgb(38, 46, 32),
                card_err_bg: Color::Rgb(56, 30, 28),
                card_quiet_bg: Color::Rgb(40, 38, 36),
                branch: Color::Rgb(184, 187, 38),
                git_status: Color::Rgb(254, 128, 25),
                input: Color::Rgb(142, 192, 124),
                output: Color::Rgb(211, 134, 155),
                cache: Color::Rgb(184, 187, 38),
                tps: Color::Rgb(131, 165, 152),
                cost: Color::Rgb(254, 128, 25),
                context: Color::Rgb(235, 219, 178),
            },
            ThemeId::Dracula => ThemePalette {
                accent: Color::Rgb(139, 233, 253),       // #8BE9FD cyan
                secondary: Color::Rgb(255, 121, 198),    // #FF79C6 pink
                complementary: Color::Rgb(255, 184, 108),// #FFB86C orange
                fg: Color::Rgb(248, 248, 242),           // #F8F8F2
                muted: Color::Rgb(148, 156, 186),
                modal_bg: Color::Rgb(40, 42, 54),        // #282A36
                success: Color::Rgb(80, 250, 123),
                warning: Color::Rgb(241, 250, 140),
                error: Color::Rgb(255, 85, 85),
                card_thinking_bg: Color::Rgb(42, 44, 58),
                card_ok_bg: Color::Rgb(32, 46, 40),
                card_err_bg: Color::Rgb(56, 32, 38),
                card_quiet_bg: Color::Rgb(38, 40, 52),
                branch: Color::Rgb(80, 250, 123),
                git_status: Color::Rgb(255, 184, 108),
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
