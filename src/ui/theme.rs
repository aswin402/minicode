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

    /// Aura Dark Plus (Deep OLED Contrast Variant)
    pub fn aura_dark_plus() -> Self {
        Self {
            bg_primary: Color::Rgb(14, 13, 19),      // #0e0d13
            bg_elevated: Color::Rgb(22, 20, 31),     // #16141f
            bg_input: Color::Rgb(31, 28, 45),        // #1f1c2d
            brand_accent: Color::Rgb(176, 132, 255), // #b084ff
            success: Color::Rgb(84, 255, 196),       // #54ffc4
            destructive: Color::Rgb(255, 92, 92),    // #ff5c5c
            warning: Color::Rgb(255, 208, 128),      // #ffd080
            highlight: Color::Rgb(250, 156, 255),    // #fa9cff
            info: Color::Rgb(120, 230, 255),         // #78e6ff
            text_primary: Color::Rgb(245, 244, 247), // #f5f4f7
            muted: Color::Rgb(128, 126, 145),        // #807e91
            border: Color::Rgb(68, 58, 108),         // #443a6c
        }
    }

    /// Aura Dark Soft Text (Official Eye-Care Soft Foreground Variant)
    pub fn aura_soft_text() -> Self {
        Self {
            bg_primary: Color::Rgb(21, 20, 27),      // #15141b
            bg_elevated: Color::Rgb(31, 30, 42),     // #1f1e2a
            bg_input: Color::Rgb(39, 36, 56),        // #272438
            brand_accent: Color::Rgb(132, 100, 198), // #8464c6 (Soft Purple)
            success: Color::Rgb(84, 197, 159),       // #54c59f (Soft Mint)
            destructive: Color::Rgb(197, 88, 88),    // #c55858 (Soft Coral)
            warning: Color::Rgb(199, 160, 111),      // #c7a06f (Soft Amber)
            highlight: Color::Rgb(193, 122, 199),    // #c17ac7 (Soft Pink)
            info: Color::Rgb(108, 178, 199),         // #6cb2c7 (Soft Cyan)
            text_primary: Color::Rgb(189, 189, 189), // #bdbdbd (Aura Soft Text)
            muted: Color::Rgb(109, 109, 109),        // #6d6d6d
            border: Color::Rgb(52, 47, 79),          // #342f4f
        }
    }

    /// Aura Emerald Mint (Mint-First Aura Variant)
    pub fn aura_emerald() -> Self {
        Self {
            bg_primary: Color::Rgb(17, 23, 24),     // #111718
            bg_elevated: Color::Rgb(25, 34, 36),    // #192224
            bg_input: Color::Rgb(33, 46, 49),       // #212e31
            brand_accent: Color::Rgb(97, 255, 202), // #61ffca (Aura Mint Primary)
            success: Color::Rgb(130, 226, 255),     // #82e2ff (Aura Cyan)
            destructive: Color::Rgb(255, 103, 103), // #ff6767
            warning: Color::Rgb(255, 202, 133),     // #ffca85
            highlight: Color::Rgb(162, 119, 255),   // #a277ff (Aura Purple)
            info: Color::Rgb(120, 240, 218),        // #78f0da
            text_primary: Color::Rgb(235, 244, 241),
            muted: Color::Rgb(118, 142, 138),
            border: Color::Rgb(46, 78, 74),
        }
    }

    /// Aura Neon Rose (Magenta-First Aura Variant)
    pub fn aura_rose() -> Self {
        Self {
            bg_primary: Color::Rgb(24, 18, 27),      // #18121b
            bg_elevated: Color::Rgb(36, 27, 41),     // #241b29
            bg_input: Color::Rgb(48, 35, 56),        // #302338
            brand_accent: Color::Rgb(246, 148, 255), // #f694ff (Aura Pink Primary)
            success: Color::Rgb(97, 255, 202),       // #61ffca
            destructive: Color::Rgb(255, 95, 126),   // #ff5f7e
            warning: Color::Rgb(255, 202, 133),      // #ffca85
            highlight: Color::Rgb(162, 119, 255),    // #a277ff
            info: Color::Rgb(130, 226, 255),         // #82e2ff
            text_primary: Color::Rgb(244, 236, 246),
            muted: Color::Rgb(145, 126, 152),
            border: Color::Rgb(78, 52, 92),
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

    /// Tokyo Night Storm Variant
    pub fn tokyo_night_storm() -> Self {
        Self {
            bg_primary: Color::Rgb(36, 40, 59),      // #24283b
            bg_elevated: Color::Rgb(41, 46, 66),     // #292e42
            bg_input: Color::Rgb(52, 59, 88),        // #343b58
            brand_accent: Color::Rgb(122, 162, 247), // #7aa2f7
            success: Color::Rgb(158, 206, 106),      // #9ece6a
            destructive: Color::Rgb(247, 118, 142),  // #f7768e
            warning: Color::Rgb(224, 175, 104),      // #e0af68
            highlight: Color::Rgb(187, 154, 247),    // #bb9af7
            info: Color::Rgb(42, 195, 222),          // #2ac3de
            text_primary: Color::Rgb(192, 202, 245), // #c0caf5
            muted: Color::Rgb(86, 95, 137),          // #565f89
            border: Color::Rgb(65, 72, 104),         // #414868
        }
    }

    /// Tokyo Night Moon Variant
    pub fn tokyo_night_moon() -> Self {
        Self {
            bg_primary: Color::Rgb(34, 36, 54),      // #222436
            bg_elevated: Color::Rgb(47, 51, 77),     // #2f334d
            bg_input: Color::Rgb(59, 66, 97),        // #3b4261
            brand_accent: Color::Rgb(130, 170, 255), // #82aaff
            success: Color::Rgb(195, 232, 141),      // #c3e88d
            destructive: Color::Rgb(255, 117, 127),  // #ff757f
            warning: Color::Rgb(255, 199, 119),      // #ffc777
            highlight: Color::Rgb(192, 153, 255),    // #c099ff
            info: Color::Rgb(134, 225, 252),         // #86e1fc
            text_primary: Color::Rgb(200, 211, 245), // #c8d3f5
            muted: Color::Rgb(99, 109, 166),         // #636da6
            border: Color::Rgb(68, 74, 115),         // #444a73
        }
    }

    /// Catppuccin Macchiato Variant
    pub fn catppuccin_macchiato() -> Self {
        Self {
            bg_primary: Color::Rgb(36, 39, 58),      // #24273a (Base)
            bg_elevated: Color::Rgb(30, 32, 48),     // #1e2030 (Mantle)
            bg_input: Color::Rgb(54, 58, 79),        // #363a4f (Surface0)
            brand_accent: Color::Rgb(198, 160, 246), // #c6a0f6 (Mauve)
            success: Color::Rgb(166, 218, 149),      // #a6da95 (Green)
            destructive: Color::Rgb(237, 135, 150),  // #ed8796 (Red)
            warning: Color::Rgb(245, 169, 127),      // #f5a97f (Peach)
            highlight: Color::Rgb(245, 189, 230),    // #f5bde6 (Pink)
            info: Color::Rgb(138, 173, 244),         // #8aadf4 (Blue)
            text_primary: Color::Rgb(202, 211, 245), // #cad3f5 (Text)
            muted: Color::Rgb(147, 154, 183),        // #939ab7 (Overlay2)
            border: Color::Rgb(91, 96, 120),         // #5b6078 (Surface2)
        }
    }

    /// Catppuccin Frappe Variant
    pub fn catppuccin_frappe() -> Self {
        Self {
            bg_primary: Color::Rgb(48, 52, 70),      // #303446 (Base)
            bg_elevated: Color::Rgb(41, 44, 60),     // #292c3c (Mantle)
            bg_input: Color::Rgb(65, 69, 89),        // #414559 (Surface0)
            brand_accent: Color::Rgb(202, 158, 230), // #ca9ee6 (Mauve)
            success: Color::Rgb(166, 209, 137),      // #a6d189 (Green)
            destructive: Color::Rgb(231, 130, 132),  // #e78284 (Red)
            warning: Color::Rgb(239, 159, 118),      // #ef9f76 (Peach)
            highlight: Color::Rgb(244, 184, 228),    // #f4b8e4 (Pink)
            info: Color::Rgb(140, 170, 238),         // #8caaee (Blue)
            text_primary: Color::Rgb(198, 208, 245), // #c6d0f5 (Text)
            muted: Color::Rgb(148, 156, 187),        // #949cbb (Overlay2)
            border: Color::Rgb(98, 104, 128),        // #626880 (Surface2)
        }
    }

    /// Nord Deep Aurora Variant
    pub fn nord_aurora() -> Self {
        Self {
            bg_primary: Color::Rgb(31, 36, 45),      // #1f242d
            bg_elevated: Color::Rgb(41, 47, 58),     // #292f3a
            bg_input: Color::Rgb(51, 58, 72),        // #333a48
            brand_accent: Color::Rgb(163, 190, 140), // #a3be8c (Aurora Green)
            success: Color::Rgb(136, 192, 208),      // #88c0d0 (Frost Cyan)
            destructive: Color::Rgb(191, 97, 106),   // #bf616a (Aurora Red)
            warning: Color::Rgb(235, 203, 139),      // #ebcb8b (Aurora Yellow)
            highlight: Color::Rgb(180, 142, 173),    // #b48ead (Aurora Purple)
            info: Color::Rgb(143, 188, 187),         // #8fbcbb (Frost Teal)
            text_primary: Color::Rgb(236, 239, 244), // #eceff4
            muted: Color::Rgb(108, 118, 136),
            border: Color::Rgb(67, 76, 94),
        }
    }

    /// Gruvbox Material Hard Variant
    pub fn gruvbox_material() -> Self {
        Self {
            bg_primary: Color::Rgb(29, 32, 33),      // #1d2021
            bg_elevated: Color::Rgb(40, 40, 40),     // #282828
            bg_input: Color::Rgb(50, 48, 47),        // #32302f
            brand_accent: Color::Rgb(231, 138, 78),  // #e78a4e (Material Orange)
            success: Color::Rgb(169, 182, 101),      // #a9b665 (Material Green)
            destructive: Color::Rgb(234, 105, 98),   // #ea6962 (Material Red)
            warning: Color::Rgb(216, 166, 87),       // #d8a657 (Material Yellow)
            highlight: Color::Rgb(211, 134, 155),    // #d3869b (Material Purple)
            info: Color::Rgb(125, 174, 163),         // #7daea3 (Material Aqua)
            text_primary: Color::Rgb(212, 190, 152), // #d4be98
            muted: Color::Rgb(146, 131, 116),        // #928374
            border: Color::Rgb(69, 64, 61),          // #45403d
        }
    }

    /// Dracula Blade Pro Variant
    pub fn dracula_pro() -> Self {
        Self {
            bg_primary: Color::Rgb(24, 23, 33),      // #181721
            bg_elevated: Color::Rgb(34, 33, 46),     // #22212e
            bg_input: Color::Rgb(46, 44, 62),        // #2e2c3e
            brand_accent: Color::Rgb(255, 128, 191), // #ff80bf (Blade Pink)
            success: Color::Rgb(138, 255, 128),      // #8aff80 (Blade Green)
            destructive: Color::Rgb(255, 149, 128),  // #ff9580 (Blade Coral)
            warning: Color::Rgb(255, 255, 128),      // #ffff80 (Blade Yellow)
            highlight: Color::Rgb(149, 128, 255),    // #9580ff (Blade Purple)
            info: Color::Rgb(128, 255, 234),         // #80ffea (Blade Cyan)
            text_primary: Color::Rgb(248, 248, 242), // #f8f8f2
            muted: Color::Rgb(121, 112, 169),        // #7970a9
            border: Color::Rgb(65, 61, 88),          // #413d58
        }
    }

    /// Cyberpunk Edgerunner Variant
    pub fn cyberpunk_edgerunner() -> Self {
        Self {
            bg_primary: Color::Rgb(18, 14, 26),     // #120e1a
            bg_elevated: Color::Rgb(28, 22, 40),    // #1c1628
            bg_input: Color::Rgb(40, 31, 56),       // #281f38
            brand_accent: Color::Rgb(252, 238, 10), // #fcee0a (Night City Yellow)
            success: Color::Rgb(0, 240, 255),       // #00f0ff (Cyberware Cyan)
            destructive: Color::Rgb(255, 0, 60),    // #ff003c (Samurai Red)
            warning: Color::Rgb(255, 153, 0),       // #ff9900
            highlight: Color::Rgb(255, 42, 109),    // #ff2a6d (Edgerunner Pink)
            info: Color::Rgb(5, 217, 232),          // #05d9e8
            text_primary: Color::Rgb(242, 245, 250),
            muted: Color::Rgb(122, 110, 148),
            border: Color::Rgb(72, 48, 102),
        }
    }

    /// Rose Pine (Soho Vibes)
    pub fn rose_pine() -> Self {
        Self {
            bg_primary: Color::Rgb(25, 23, 36),      // #191724 (Base)
            bg_elevated: Color::Rgb(31, 29, 46),     // #1f1d2e (Surface)
            bg_input: Color::Rgb(38, 35, 58),        // #26233a (Overlay)
            brand_accent: Color::Rgb(196, 167, 231), // #c4a7e7 (Iris)
            success: Color::Rgb(156, 207, 216),      // #9ccfd8 (Foam)
            destructive: Color::Rgb(235, 111, 146),  // #eb6f92 (Love)
            warning: Color::Rgb(246, 193, 119),      // #f6c177 (Gold)
            highlight: Color::Rgb(235, 188, 186),    // #ebbcba (Rose)
            info: Color::Rgb(49, 116, 143),          // #31748f (Pine)
            text_primary: Color::Rgb(224, 222, 244), // #e0def4 (Text)
            muted: Color::Rgb(110, 106, 134),        // #6e6a86 (Muted)
            border: Color::Rgb(64, 61, 82),          // #403d52 (Highlight Med)
        }
    }

    /// Rose Pine Moon Variant
    pub fn rose_pine_moon() -> Self {
        Self {
            bg_primary: Color::Rgb(35, 33, 54),      // #232136 (Base)
            bg_elevated: Color::Rgb(42, 39, 63),     // #2a273f (Surface)
            bg_input: Color::Rgb(57, 53, 82),        // #393552 (Overlay)
            brand_accent: Color::Rgb(196, 167, 231), // #c4a7e7 (Iris)
            success: Color::Rgb(156, 207, 216),      // #9ccfd8 (Foam)
            destructive: Color::Rgb(235, 111, 146),  // #eb6f92 (Love)
            warning: Color::Rgb(246, 193, 119),      // #f6c177 (Gold)
            highlight: Color::Rgb(234, 154, 151),    // #ea9a97 (Rose)
            info: Color::Rgb(62, 143, 176),          // #3e8fb0 (Pine)
            text_primary: Color::Rgb(224, 222, 244), // #e0def4 (Text)
            muted: Color::Rgb(110, 106, 134),        // #6e6a86 (Muted)
            border: Color::Rgb(68, 65, 90),          // #44415a
        }
    }

    /// Kanagawa Wave Theme
    pub fn kanagawa_wave() -> Self {
        Self {
            bg_primary: Color::Rgb(31, 31, 40),      // #1f1f28 (SumiInk1)
            bg_elevated: Color::Rgb(42, 42, 55),     // #2a2a37 (SumiInk2)
            bg_input: Color::Rgb(54, 54, 70),        // #363646 (SumiInk3)
            brand_accent: Color::Rgb(149, 127, 184), // #957fb8 (OniViolet)
            success: Color::Rgb(152, 187, 108),      // #98bb6c (SpringGreen)
            destructive: Color::Rgb(228, 104, 118),  // #e46876 (WaveRed)
            warning: Color::Rgb(230, 195, 132),      // #e6c384 (CarpYellow)
            highlight: Color::Rgb(210, 126, 153),    // #d27e99 (SakuraPink)
            info: Color::Rgb(126, 156, 216),         // #7e9cd8 (CrystalBlue)
            text_primary: Color::Rgb(220, 215, 186), // #dcd7ba (FujiWhite)
            muted: Color::Rgb(114, 113, 105),        // #727169 (FujiGray)
            border: Color::Rgb(84, 84, 109),         // #54546d (SumiInk4)
        }
    }

    /// Kanagawa Dragon Variant
    pub fn kanagawa_dragon() -> Self {
        Self {
            bg_primary: Color::Rgb(24, 22, 22),      // #181616 (DragonBlack3)
            bg_elevated: Color::Rgb(30, 28, 28),     // #1e1c1c (DragonBlack4)
            bg_input: Color::Rgb(40, 39, 39),        // #282727 (DragonBlack5)
            brand_accent: Color::Rgb(196, 116, 110), // #c4746e (DragonRed)
            success: Color::Rgb(135, 169, 135),      // #87a987 (DragonGreen)
            destructive: Color::Rgb(196, 116, 110),  // #c4746e
            warning: Color::Rgb(196, 178, 138),      // #c4b28a (DragonYellow)
            highlight: Color::Rgb(162, 146, 163),    // #a292a3 (DragonPink)
            info: Color::Rgb(139, 164, 176),         // #8ba4b0 (DragonBlue)
            text_primary: Color::Rgb(197, 201, 197), // #c5c9c5 (DragonWhite)
            muted: Color::Rgb(115, 124, 115),        // #737c73
            border: Color::Rgb(57, 56, 54),          // #393836
        }
    }

    /// Everforest Dark Theme
    pub fn everforest_dark() -> Self {
        Self {
            bg_primary: Color::Rgb(39, 46, 51),      // #272e33
            bg_elevated: Color::Rgb(46, 56, 60),     // #2e383c
            bg_input: Color::Rgb(55, 65, 69),        // #374145
            brand_accent: Color::Rgb(167, 192, 128), // #a7c080 (Forest Green)
            success: Color::Rgb(131, 192, 146),      // #83c092 (Aqua)
            destructive: Color::Rgb(230, 126, 128),  // #e67e80 (Red)
            warning: Color::Rgb(219, 188, 127),      // #dbbc7f (Yellow)
            highlight: Color::Rgb(214, 153, 182),    // #d699b6 (Purple)
            info: Color::Rgb(127, 187, 179),         // #7fbbb3 (Blue)
            text_primary: Color::Rgb(211, 198, 170), // #d3c6aa (Wheat)
            muted: Color::Rgb(133, 146, 137),        // #859289
            border: Color::Rgb(65, 75, 80),          // #414b50
        }
    }

    /// One Dark Pro Theme
    pub fn onedark_pro() -> Self {
        Self {
            bg_primary: Color::Rgb(40, 44, 52),      // #282c34
            bg_elevated: Color::Rgb(33, 37, 43),     // #21252b
            bg_input: Color::Rgb(44, 49, 60),        // #2c313c
            brand_accent: Color::Rgb(97, 175, 239),  // #61afef (Blue)
            success: Color::Rgb(152, 195, 121),      // #98c379 (Green)
            destructive: Color::Rgb(224, 108, 117),  // #e06c75 (Coral Red)
            warning: Color::Rgb(229, 192, 123),      // #e5c07b (Chalky Yellow)
            highlight: Color::Rgb(198, 120, 221),    // #c678dd (Purple)
            info: Color::Rgb(86, 182, 194),          // #56b6c2 (Cyan)
            text_primary: Color::Rgb(171, 178, 191), // #abb2bf
            muted: Color::Rgb(92, 99, 112),          // #5c6370
            border: Color::Rgb(62, 68, 81),          // #3e4451
        }
    }

    /// Vesper Peppermint Theme
    pub fn vesper() -> Self {
        Self {
            bg_primary: Color::Rgb(16, 16, 16),      // #101010
            bg_elevated: Color::Rgb(24, 24, 24),     // #181818
            bg_input: Color::Rgb(32, 32, 32),        // #202020
            brand_accent: Color::Rgb(255, 199, 153), // #ffc799 (Vesper Gold)
            success: Color::Rgb(153, 255, 228),      // #99ffe4 (Peppermint)
            destructive: Color::Rgb(255, 128, 128),  // #ff8080
            warning: Color::Rgb(255, 214, 153),      // #ffd699
            highlight: Color::Rgb(217, 179, 255),    // #d9b3ff
            info: Color::Rgb(153, 230, 255),         // #99e6ff
            text_primary: Color::Rgb(255, 255, 255), // #ffffff
            muted: Color::Rgb(126, 126, 126),        // #7e7e7e
            border: Color::Rgb(46, 46, 46),          // #2e2e2e
        }
    }

    /// Synthwave '84 Theme
    pub fn synthwave_84() -> Self {
        Self {
            bg_primary: Color::Rgb(38, 35, 53),      // #262335
            bg_elevated: Color::Rgb(36, 27, 47),     // #241b2f
            bg_input: Color::Rgb(52, 41, 79),        // #34294f
            brand_accent: Color::Rgb(255, 126, 219), // #ff7edb (Sunset Neon Pink)
            success: Color::Rgb(114, 241, 184),      // #72f1b8 (Laser Mint)
            destructive: Color::Rgb(254, 68, 80),    // #fe4450 (Outrun Red)
            warning: Color::Rgb(254, 222, 93),       // #fede5d (Chrome Yellow)
            highlight: Color::Rgb(249, 126, 114),    // #f97e72 (Coral Sunset)
            info: Color::Rgb(54, 249, 246),          // #36f9f6 (Electric Cyan)
            text_primary: Color::Rgb(255, 255, 255),
            muted: Color::Rgb(132, 139, 178), // #848bb2
            border: Color::Rgb(73, 59, 110),  // #493b6e
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
                id: "soft-dark".to_string(),
                name: "Aura Soft Dark".to_string(),
                description: "Lower contrast dark background Aura variant".to_string(),
                swatches: vec![
                    Color::Rgb(18, 16, 22),
                    Color::Rgb(162, 119, 255),
                    Color::Rgb(97, 255, 202),
                    Color::Rgb(130, 226, 255),
                ],
            },
            ThemeInfo {
                id: "aura-dark-plus".to_string(),
                name: "Aura Dark Plus (OLED)".to_string(),
                description: "Deep OLED #0e0d13 canvas with high-contrast Aura neon accents"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(14, 13, 19),
                    Color::Rgb(176, 132, 255),
                    Color::Rgb(84, 255, 196),
                    Color::Rgb(250, 156, 255),
                ],
            },
            ThemeInfo {
                id: "aura-soft-text".to_string(),
                name: "Aura Dark Soft Text".to_string(),
                description: "Official Aura Dark canvas with reduced-glare eye-care soft text"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(21, 20, 27),
                    Color::Rgb(132, 100, 198),
                    Color::Rgb(84, 197, 159),
                    Color::Rgb(189, 189, 189),
                ],
            },
            ThemeInfo {
                id: "aura-emerald".to_string(),
                name: "Aura Emerald Mint".to_string(),
                description: "Mint-first Aura Dark variant with Emerald & Cyan highlights"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(17, 23, 24),
                    Color::Rgb(97, 255, 202),
                    Color::Rgb(130, 226, 255),
                    Color::Rgb(162, 119, 255),
                ],
            },
            ThemeInfo {
                id: "aura-rose".to_string(),
                name: "Aura Neon Rose".to_string(),
                description: "Magenta-first Aura Dark variant with Electric Pink & Violet accents"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(24, 18, 27),
                    Color::Rgb(246, 148, 255),
                    Color::Rgb(162, 119, 255),
                    Color::Rgb(97, 255, 202),
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
                id: "tokyo-night-storm".to_string(),
                name: "Tokyo Night Storm".to_string(),
                description: "Deep slate indigo storm variant of Tokyo Night".to_string(),
                swatches: vec![
                    Color::Rgb(36, 40, 59),
                    Color::Rgb(122, 162, 247),
                    Color::Rgb(42, 195, 222),
                    Color::Rgb(187, 154, 247),
                ],
            },
            ThemeInfo {
                id: "tokyo-night-moon".to_string(),
                name: "Tokyo Night Moon".to_string(),
                description: "Soft lunar indigo & pastel sky variant of Tokyo Night".to_string(),
                swatches: vec![
                    Color::Rgb(34, 36, 54),
                    Color::Rgb(130, 170, 255),
                    Color::Rgb(195, 232, 141),
                    Color::Rgb(192, 153, 255),
                ],
            },
            ThemeInfo {
                id: "catppuccin".to_string(),
                name: "Catppuccin Mocha".to_string(),
                description: "Soothing Pastel Mauve & Rosewater dark palette".to_string(),
                swatches: vec![
                    Color::Rgb(30, 30, 46),
                    Color::Rgb(203, 166, 247),
                    Color::Rgb(166, 227, 161),
                    Color::Rgb(137, 180, 250),
                ],
            },
            ThemeInfo {
                id: "catppuccin-macchiato".to_string(),
                name: "Catppuccin Macchiato".to_string(),
                description: "Medium-dark espresso & velvety pastel Catppuccin variant".to_string(),
                swatches: vec![
                    Color::Rgb(36, 39, 58),
                    Color::Rgb(198, 160, 246),
                    Color::Rgb(166, 218, 149),
                    Color::Rgb(138, 173, 244),
                ],
            },
            ThemeInfo {
                id: "catppuccin-frappe".to_string(),
                name: "Catppuccin Frappe".to_string(),
                description: "Soft twilight slate & muted pastel Catppuccin variant".to_string(),
                swatches: vec![
                    Color::Rgb(48, 52, 70),
                    Color::Rgb(202, 158, 230),
                    Color::Rgb(166, 209, 137),
                    Color::Rgb(140, 170, 238),
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
                id: "nord-aurora".to_string(),
                name: "Nord Deep Aurora".to_string(),
                description: "Darker polar night canvas with Aurora Borealis green & violet"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(31, 36, 45),
                    Color::Rgb(163, 190, 140),
                    Color::Rgb(136, 192, 208),
                    Color::Rgb(180, 142, 173),
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
                id: "gruvbox-material".to_string(),
                name: "Gruvbox Material Hard".to_string(),
                description: "Deep #1d2021 hard-contrast Gruvbox with refined earth tones"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(29, 32, 33),
                    Color::Rgb(231, 138, 78),
                    Color::Rgb(169, 182, 101),
                    Color::Rgb(125, 174, 163),
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
                id: "dracula-pro".to_string(),
                name: "Dracula Blade".to_string(),
                description: "Obsidian dark Dracula variant with neon Blade pink & cyan"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(24, 23, 33),
                    Color::Rgb(255, 128, 191),
                    Color::Rgb(138, 255, 128),
                    Color::Rgb(128, 255, 234),
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
                id: "cyberpunk-edgerunner".to_string(),
                name: "Cyberpunk Edgerunner".to_string(),
                description: "Night City high-voltage yellow, cyberware cyan & hot magenta"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(18, 14, 26),
                    Color::Rgb(252, 238, 10),
                    Color::Rgb(0, 240, 255),
                    Color::Rgb(255, 42, 109),
                ],
            },
            ThemeInfo {
                id: "rose-pine".to_string(),
                name: "Rose Pine".to_string(),
                description: "All natural pine, faux fur & Soho vibes with iris and rose gold"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(25, 23, 36),
                    Color::Rgb(196, 167, 231),
                    Color::Rgb(156, 207, 216),
                    Color::Rgb(235, 188, 186),
                ],
            },
            ThemeInfo {
                id: "rose-pine-moon".to_string(),
                name: "Rose Pine Moon".to_string(),
                description: "Lunar plum variant of Rose Pine with warm gold & foam accents"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(35, 33, 54),
                    Color::Rgb(196, 167, 231),
                    Color::Rgb(246, 193, 119),
                    Color::Rgb(234, 154, 151),
                ],
            },
            ThemeInfo {
                id: "kanagawa-wave".to_string(),
                name: "Kanagawa Wave".to_string(),
                description: "Inspired by Hokusai's Great Wave: sumi ink, violet & crystal blue"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(31, 31, 40),
                    Color::Rgb(149, 127, 184),
                    Color::Rgb(152, 187, 108),
                    Color::Rgb(126, 156, 216),
                ],
            },
            ThemeInfo {
                id: "kanagawa-dragon".to_string(),
                name: "Kanagawa Dragon".to_string(),
                description: "Volcanic charcoal Kanagawa variant with dragon crimson & sage"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(24, 22, 22),
                    Color::Rgb(196, 116, 110),
                    Color::Rgb(135, 169, 135),
                    Color::Rgb(196, 178, 138),
                ],
            },
            ThemeInfo {
                id: "everforest-dark".to_string(),
                name: "Everforest Dark".to_string(),
                description: "Warm woodland green & wheat palette designed for eye comfort"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(39, 46, 51),
                    Color::Rgb(167, 192, 128),
                    Color::Rgb(131, 192, 146),
                    Color::Rgb(219, 188, 127),
                ],
            },
            ThemeInfo {
                id: "onedark-pro".to_string(),
                name: "One Dark Pro".to_string(),
                description: "Iconic Atom One Dark Pro sapphire blue, coral & spring green"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(40, 44, 52),
                    Color::Rgb(97, 175, 239),
                    Color::Rgb(152, 195, 121),
                    Color::Rgb(198, 120, 221),
                ],
            },
            ThemeInfo {
                id: "vesper".to_string(),
                name: "Vesper Peppermint".to_string(),
                description: "Pitch-black #101010 minimalist theme with amber gold & peppermint"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(16, 16, 16),
                    Color::Rgb(255, 199, 153),
                    Color::Rgb(153, 255, 228),
                    Color::Rgb(217, 179, 255),
                ],
            },
            ThemeInfo {
                id: "synthwave-84".to_string(),
                name: "Synthwave '84".to_string(),
                description: "Retro 1984 outrun sunset neon pink, laser cyan & chrome yellow"
                    .to_string(),
                swatches: vec![
                    Color::Rgb(38, 35, 53),
                    Color::Rgb(255, 126, 219),
                    Color::Rgb(54, 249, 246),
                    Color::Rgb(254, 222, 93),
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
            "aura-dark-plus" | "aura-plus" | "aura-oled" if caps.supports_truecolor => {
                Self::aura_dark_plus()
            }
            "aura-soft-text" | "soft-text" if caps.supports_truecolor => Self::aura_soft_text(),
            "aura-emerald" | "aura-mint" if caps.supports_truecolor => Self::aura_emerald(),
            "aura-rose" | "aura-pink" if caps.supports_truecolor => Self::aura_rose(),
            "tokyo" | "tokyo-night" | "tokyonight" if caps.supports_truecolor => {
                Self::tokyo_night()
            }
            "tokyo-night-storm" | "tokyo-storm" if caps.supports_truecolor => {
                Self::tokyo_night_storm()
            }
            "tokyo-night-moon" | "tokyo-moon" if caps.supports_truecolor => {
                Self::tokyo_night_moon()
            }
            "catppuccin" | "catppuccin-mocha" | "mocha" if caps.supports_truecolor => {
                Self::catppuccin_mocha()
            }
            "catppuccin-macchiato" | "macchiato" if caps.supports_truecolor => {
                Self::catppuccin_macchiato()
            }
            "catppuccin-frappe" | "frappe" if caps.supports_truecolor => Self::catppuccin_frappe(),
            "nord" | "nord-frost" if caps.supports_truecolor => Self::nord_frost(),
            "nord-aurora" | "aurora" if caps.supports_truecolor => Self::nord_aurora(),
            "gruvbox" | "gruvbox-dark" if caps.supports_truecolor => Self::gruvbox_dark(),
            "gruvbox-material" | "gruvbox-hard" if caps.supports_truecolor => {
                Self::gruvbox_material()
            }
            "dracula" if caps.supports_truecolor => Self::dracula(),
            "dracula-pro" | "dracula-blade" if caps.supports_truecolor => Self::dracula_pro(),
            "cyberpunk" | "matrix" | "cyberpunk-matrix" if caps.supports_truecolor => {
                Self::cyberpunk_matrix()
            }
            "cyberpunk-edgerunner" | "edgerunner" if caps.supports_truecolor => {
                Self::cyberpunk_edgerunner()
            }
            "rose-pine" | "rosepine" if caps.supports_truecolor => Self::rose_pine(),
            "rose-pine-moon" | "rosepine-moon" if caps.supports_truecolor => Self::rose_pine_moon(),
            "kanagawa" | "kanagawa-wave" if caps.supports_truecolor => Self::kanagawa_wave(),
            "kanagawa-dragon" if caps.supports_truecolor => Self::kanagawa_dragon(),
            "everforest" | "everforest-dark" if caps.supports_truecolor => Self::everforest_dark(),
            "onedark" | "onedark-pro" | "one-dark" if caps.supports_truecolor => {
                Self::onedark_pro()
            }
            "vesper" if caps.supports_truecolor => Self::vesper(),
            "synthwave" | "synthwave-84" if caps.supports_truecolor => Self::synthwave_84(),
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
        fn has_ci(haystack: &str, needle: &str) -> bool {
            haystack
                .as_bytes()
                .windows(needle.len())
                .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
        }
        if has_ci(role_name, "research") {
            self.info // Cyan / Aqua
        } else if has_ci(role_name, "reviewer") || has_ci(role_name, "critic") {
            self.highlight // Pink / Magenta
        } else if has_ci(role_name, "test") || has_ci(role_name, "qa") {
            self.success // Green / Mint
        } else if has_ci(role_name, "security") || has_ci(role_name, "audit") {
            self.warning // Orange / Warm Yellow
        } else {
            self.brand_accent // Purple / Brand
        }
    }
}

/// Probed terminal color and styling capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCapabilities {
    pub supports_truecolor: bool,
    #[allow(dead_code)]
    pub supports_unicode: bool,
}

/// Automatically probe terminal capabilities via environment variables (cached in `OnceLock`)
pub fn detect_terminal_caps() -> TerminalCapabilities {
    static CAPS: std::sync::OnceLock<TerminalCapabilities> = std::sync::OnceLock::new();
    *CAPS.get_or_init(|| {
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
    })
}
