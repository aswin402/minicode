use ratatui::style::Color;

/// Theme metadata for interactive palette selector
#[derive(Debug, Clone)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub swatches: Vec<Color>,
}

/// Comprehensive Theme Color Palette
#[derive(Debug, Clone)]
pub struct Theme {
    pub bg_primary: Color,
    pub bg_elevated: Color,
    #[allow(dead_code)]
    pub bg_input: Color,
    pub brand_accent: Color,
    pub success: Color,
    pub destructive: Color,
    pub warning: Color,
    pub highlight: Color,
    pub info: Color,
    pub text_primary: Color,
    pub muted: Color,
    pub border: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::aura_dark()
    }
}

impl Theme {
    /// Official Aura Dark Theme (Default)
    pub fn aura_dark() -> Self {
        Self {
            bg_primary: Color::Rgb(21, 20, 27),      // #15141b
            bg_elevated: Color::Rgb(33, 32, 46),     // #21202e
            bg_input: Color::Rgb(41, 38, 60),        // #29263c
            brand_accent: Color::Rgb(162, 119, 255), // #a277ff (Aura Purple)
            success: Color::Rgb(97, 255, 202),       // #61ffca (Aura Mint Green)
            destructive: Color::Rgb(255, 103, 103),  // #ff6767 (Aura Coral Red)
            warning: Color::Rgb(255, 202, 133),      // #ffca85 (Aura Warm Orange)
            highlight: Color::Rgb(246, 148, 255),    // #f694ff (Aura Pink)
            info: Color::Rgb(130, 226, 255),         // #82e2ff (Aura Cyan)
            text_primary: Color::Rgb(237, 236, 238), // #edecee (Aura Bright)
            muted: Color::Rgb(138, 138, 147),        // #8a8a93 (Aura Comment)
            border: Color::Rgb(61, 55, 94),          // #3d375e (Aura Line)
        }
    }

    /// Official Aura Soft Dark Theme
    pub fn aura_soft_dark() -> Self {
        Self {
            bg_primary: Color::Rgb(18, 16, 22),  // #121016
            bg_elevated: Color::Rgb(28, 26, 36), // #1c1a24
            bg_input: Color::Rgb(36, 33, 49),    // #242131
            brand_accent: Color::Rgb(162, 119, 255),
            success: Color::Rgb(97, 255, 202),
            destructive: Color::Rgb(255, 103, 103),
            warning: Color::Rgb(255, 202, 133),
            highlight: Color::Rgb(246, 148, 255),
            info: Color::Rgb(130, 226, 255),
            text_primary: Color::Rgb(237, 236, 238),
            muted: Color::Rgb(110, 110, 125),
            border: Color::Rgb(50, 45, 75),
        }
    }

    /// Tokyo Night Theme
    pub fn tokyo_night() -> Self {
        Self {
            bg_primary: Color::Rgb(26, 27, 38),      // #1a1b26
            bg_elevated: Color::Rgb(36, 40, 59),     // #24283b
            bg_input: Color::Rgb(41, 46, 66),        // #292e42
            brand_accent: Color::Rgb(122, 162, 247), // #7aa2f7 (Blue)
            success: Color::Rgb(158, 206, 106),      // #9ece6a (Green)
            destructive: Color::Rgb(247, 118, 142),  // #f7768e (Red)
            warning: Color::Rgb(224, 175, 104),      // #e0af68 (Orange)
            highlight: Color::Rgb(187, 154, 247),    // #bb9af7 (Magenta)
            info: Color::Rgb(125, 207, 255),         // #7dcfff (Cyan)
            text_primary: Color::Rgb(192, 202, 245), // #c0caf5
            muted: Color::Rgb(86, 95, 137),          // #565f89
            border: Color::Rgb(65, 72, 104),         // #414868
        }
    }

    /// Catppuccin Mocha Theme
    pub fn catppuccin_mocha() -> Self {
        Self {
            bg_primary: Color::Rgb(30, 30, 46),      // #1e1e2e (Base)
            bg_elevated: Color::Rgb(24, 24, 37),     // #181825 (Mantle)
            bg_input: Color::Rgb(49, 50, 68),        // #313244 (Surface0)
            brand_accent: Color::Rgb(203, 166, 247), // #cba6f7 (Mauve)
            success: Color::Rgb(166, 227, 161),      // #a6e3a1 (Green)
            destructive: Color::Rgb(243, 139, 168),  // #f38ba8 (Red)
            warning: Color::Rgb(250, 179, 135),      // #fab387 (Peach)
            highlight: Color::Rgb(245, 194, 231),    // #f5c2e7 (Pink)
            info: Color::Rgb(137, 180, 250),         // #89b4fa (Blue)
            text_primary: Color::Rgb(205, 214, 244), // #cdd6f4 (Text)
            muted: Color::Rgb(147, 153, 178),        // #9399b2 (Overlay2)
            border: Color::Rgb(88, 91, 112),         // #585b70 (Surface2)
        }
    }

    /// Nord Frost Theme
    pub fn nord_frost() -> Self {
        Self {
            bg_primary: Color::Rgb(46, 52, 64),      // #2e3440 (Polar Night)
            bg_elevated: Color::Rgb(59, 66, 82),     // #3b4252
            bg_input: Color::Rgb(67, 76, 94),        // #434c5e
            brand_accent: Color::Rgb(136, 192, 208), // #88c0d0 (Frost Cyan)
            success: Color::Rgb(163, 190, 140),      // #a3be8c (Aurora Green)
            destructive: Color::Rgb(191, 97, 106),   // #bf616a (Aurora Red)
            warning: Color::Rgb(235, 203, 139),      // #ebcb8b (Aurora Yellow)
            highlight: Color::Rgb(180, 142, 173),    // #b48ead (Aurora Purple)
            info: Color::Rgb(129, 161, 193),         // #81a1c1 (Frost Blue)
            text_primary: Color::Rgb(236, 239, 244), // #eceff4
            muted: Color::Rgb(108, 112, 126),
            border: Color::Rgb(76, 86, 106), // #4c566a
        }
    }

    /// Gruvbox Dark Theme
    pub fn gruvbox_dark() -> Self {
        Self {
            bg_primary: Color::Rgb(40, 40, 40),      // #282828
            bg_elevated: Color::Rgb(50, 48, 47),     // #32302f
            bg_input: Color::Rgb(60, 56, 54),        // #3c3836
            brand_accent: Color::Rgb(250, 189, 47),  // #fabd2f (Yellow)
            success: Color::Rgb(184, 187, 38),       // #b8bb26 (Green)
            destructive: Color::Rgb(251, 73, 52),    // #fb4934 (Red)
            warning: Color::Rgb(254, 128, 25),       // #fe8019 (Orange)
            highlight: Color::Rgb(211, 134, 155),    // #d3869b (Purple)
            info: Color::Rgb(131, 165, 152),         // #83a598 (Blue)
            text_primary: Color::Rgb(235, 219, 178), // #ebdbb2
            muted: Color::Rgb(146, 131, 116),        // #928374
            border: Color::Rgb(80, 73, 69),          // #504945
        }
    }

    /// Dracula Theme
    pub fn dracula() -> Self {
        Self {
            bg_primary: Color::Rgb(40, 42, 54),      // #282a36
            bg_elevated: Color::Rgb(52, 55, 70),     // #343746
            bg_input: Color::Rgb(68, 71, 90),        // #44475a
            brand_accent: Color::Rgb(189, 147, 249), // #bd93f9 (Purple)
            success: Color::Rgb(80, 250, 123),       // #50fa7b (Green)
            destructive: Color::Rgb(255, 85, 85),    // #ff5555 (Red)
            warning: Color::Rgb(255, 184, 108),      // #ffb86c (Orange)
            highlight: Color::Rgb(255, 121, 198),    // #ff79c6 (Pink)
            info: Color::Rgb(139, 233, 253),         // #8be9fd (Cyan)
            text_primary: Color::Rgb(248, 248, 242), // #f8f8f2
            muted: Color::Rgb(98, 114, 164),         // #6272a4
            border: Color::Rgb(68, 71, 90),
        }
    }

    /// Cyberpunk Matrix Theme
    pub fn cyberpunk_matrix() -> Self {
        Self {
            bg_primary: Color::Rgb(13, 17, 23),      // #0d1117
            bg_elevated: Color::Rgb(22, 27, 34),     // #161b22
            bg_input: Color::Rgb(33, 38, 45),        // #21262d
            brand_accent: Color::Rgb(0, 255, 102),   // #00ff66 (Neon Green)
            success: Color::Rgb(0, 255, 136),        // #00ff88
            destructive: Color::Rgb(255, 0, 85),     // #ff0055 (Neon Pink)
            warning: Color::Rgb(255, 230, 0),        // #ffe600 (Neon Yellow)
            highlight: Color::Rgb(255, 0, 234),      // #ff00ea (Neon Magenta)
            info: Color::Rgb(0, 229, 255),           // #00e5ff (Neon Cyan)
            text_primary: Color::Rgb(240, 246, 252), // #f0f6fc
            muted: Color::Rgb(102, 112, 128),
            border: Color::Rgb(48, 54, 61),
        }
    }

    /// Fallback 256-color palette
    pub fn ansi_256() -> Self {
        Self {
            bg_primary: Color::Indexed(234),
            bg_elevated: Color::Indexed(235),
            bg_input: Color::Indexed(236),
            brand_accent: Color::Indexed(141), // Purple
            success: Color::Indexed(84),       // Mint
            destructive: Color::Indexed(203),  // Red
            warning: Color::Indexed(215),      // Orange
            highlight: Color::Indexed(213),    // Pink
            info: Color::Indexed(117),         // Cyan
            text_primary: Color::White,
            muted: Color::Indexed(244),
            border: Color::Indexed(239),
        }
    }

    /// List all selectable themes with metadata and preview swatches
    pub fn list_themes() -> Vec<ThemeInfo> {
        vec![
            ThemeInfo {
                id: "aura".to_string(),
                name: "Aura Dark (Default)".to_string(),
                description: "Official Aura Purple & Mint palette by Dalton Menezes".to_string(),
                swatches: vec![
                    Color::Rgb(21, 20, 27),
                    Color::Rgb(162, 119, 255),
                    Color::Rgb(97, 255, 202),
                    Color::Rgb(246, 148, 255),
                ],
            },
            ThemeInfo {
                id: "tokyo-night".to_string(),
                name: "Tokyo Night".to_string(),
                description: "Clean Tokyo Midnight Blue & Lavender aesthetic".to_string(),
                swatches: vec![
                    Color::Rgb(26, 27, 38),
                    Color::Rgb(122, 162, 247),
                    Color::Rgb(158, 206, 106),
                    Color::Rgb(187, 154, 247),
                ],
            },
            ThemeInfo {
                id: "catppuccin".to_string(),
                name: "Catppuccin Mocha".to_string(),
                description: "Soothing Pastel Mauve & Rosewater palette".to_string(),
                swatches: vec![
                    Color::Rgb(30, 30, 46),
                    Color::Rgb(203, 166, 247),
                    Color::Rgb(166, 227, 161),
                    Color::Rgb(137, 180, 250),
                ],
            },
            ThemeInfo {
                id: "nord".to_string(),
                name: "Nord Frost".to_string(),
                description: "Arctic Slate & Polar Blue developer theme".to_string(),
                swatches: vec![
                    Color::Rgb(46, 52, 64),
                    Color::Rgb(136, 192, 208),
                    Color::Rgb(163, 190, 140),
                    Color::Rgb(129, 161, 193),
                ],
            },
            ThemeInfo {
                id: "gruvbox".to_string(),
                name: "Gruvbox Dark".to_string(),
                description: "Retro Warm Amber, Rust & Forest Green aesthetic".to_string(),
                swatches: vec![
                    Color::Rgb(40, 40, 40),
                    Color::Rgb(250, 189, 47),
                    Color::Rgb(184, 187, 38),
                    Color::Rgb(254, 128, 25),
                ],
            },
            ThemeInfo {
                id: "dracula".to_string(),
                name: "Dracula".to_string(),
                description: "Vampire Purple, Cyan & Emerald high-contrast palette".to_string(),
                swatches: vec![
                    Color::Rgb(40, 42, 54),
                    Color::Rgb(189, 147, 249),
                    Color::Rgb(80, 250, 123),
                    Color::Rgb(139, 233, 253),
                ],
            },
            ThemeInfo {
                id: "cyberpunk".to_string(),
                name: "Cyberpunk Matrix".to_string(),
                description: "Neon Emerald, Electric Cyan & Pitch Black terminal".to_string(),
                swatches: vec![
                    Color::Rgb(13, 17, 23),
                    Color::Rgb(0, 255, 102),
                    Color::Rgb(0, 229, 255),
                    Color::Rgb(255, 0, 85),
                ],
            },
            ThemeInfo {
                id: "soft-dark".to_string(),
                name: "Aura Soft Dark".to_string(),
                description: "Lower contrast dark background variant".to_string(),
                swatches: vec![
                    Color::Rgb(18, 16, 22),
                    Color::Rgb(162, 119, 255),
                    Color::Rgb(97, 255, 202),
                    Color::Rgb(130, 226, 255),
                ],
            },
            ThemeInfo {
                id: "ansi".to_string(),
                name: "ANSI 256 Fallback".to_string(),
                description: "Compatibility palette for 8-bit color terminals".to_string(),
                swatches: vec![
                    Color::Indexed(234),
                    Color::Indexed(141),
                    Color::Indexed(84),
                    Color::Indexed(117),
                ],
            },
        ]
    }

    pub fn detect(preference: &str) -> Self {
        let caps = detect_terminal_caps();
        let key = preference.to_lowercase().replace('_', "-");
        match key.as_str() {
            "tokyo" | "tokyo-night" | "tokyonight" if caps.supports_truecolor => {
                Self::tokyo_night()
            }
            "catppuccin" | "catppuccin-mocha" | "mocha" if caps.supports_truecolor => {
                Self::catppuccin_mocha()
            }
            "nord" | "nord-frost" if caps.supports_truecolor => Self::nord_frost(),
            "gruvbox" | "gruvbox-dark" if caps.supports_truecolor => Self::gruvbox_dark(),
            "dracula" if caps.supports_truecolor => Self::dracula(),
            "cyberpunk" | "matrix" | "cyberpunk-matrix" if caps.supports_truecolor => {
                Self::cyberpunk_matrix()
            }
            "soft" | "soft-dark" | "aura-soft" if caps.supports_truecolor => Self::aura_soft_dark(),
            "256" | "ansi" => Self::ansi_256(),
            "dark" | "aura" | "default" if caps.supports_truecolor => Self::aura_dark(),
            _ => {
                if caps.supports_truecolor {
                    Self::aura_dark()
                } else {
                    Self::ansi_256()
                }
            }
        }
    }

    /// Returns the distinct accent color for a specific subagent role
    pub fn role_accent_color(&self, role_name: &str) -> Color {
        let lower = role_name.to_lowercase();
        if lower.contains("researcher") || lower.contains("research") {
            self.info // Cyan / Aqua
        } else if lower.contains("reviewer") || lower.contains("critic") {
            self.highlight // Pink / Magenta
        } else if lower.contains("test") || lower.contains("qa") {
            self.success // Green / Mint
        } else if lower.contains("security") || lower.contains("audit") {
            self.warning // Orange / Warm Yellow
        } else {
            self.brand_accent // Purple / Brand
        }
    }
}

/// Probed terminal color and styling capabilities
#[derive(Debug, Clone)]
pub struct TerminalCapabilities {
    pub supports_truecolor: bool,
    #[allow(dead_code)]
    pub supports_unicode: bool,
}

/// Automatically probe terminal capabilities via environment variables
pub fn detect_terminal_caps() -> TerminalCapabilities {
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();

    let supports_truecolor = colorterm.contains("truecolor")
        || colorterm.contains("24bit")
        || term.contains("direct")
        || term.contains("truecolor")
        || term.contains("256color")
        || term.contains("xterm")
        || term.contains("kitty")
        || term.contains("alacritty");

    let supports_unicode = !term.contains("linux") && !term.contains("dumb");

    TerminalCapabilities {
        supports_truecolor,
        supports_unicode,
    }
}
