#![allow(dead_code)]

use ratatui::style::Color;

/// Official Aura Theme Color Palette (by Dalton Menezes)
/// https://github.com/daltonmenezes/aura-theme
#[derive(Debug, Clone)]
pub struct Theme {
    pub bg_primary: Color,
    pub bg_elevated: Color,
    pub bg_input: Color,
    pub brand_accent: Color, // Aura Purple (#a277ff)
    pub success: Color,      // Aura Mint Green (#61ffca)
    pub destructive: Color,  // Aura Coral Red (#ff6767)
    pub warning: Color,      // Aura Warm Orange (#ffca85)
    pub highlight: Color,    // Aura Pink (#f694ff)
    pub info: Color,         // Aura Cyan Blue (#82e2ff)
    pub text_primary: Color, // Aura Bright Text (#edecee)
    pub muted: Color,        // Aura Slate/Comment (#8a8a93)
    pub border: Color,       // Aura Subtle Border (#3d375e)
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

    pub fn detect(preference: &str) -> Self {
        match preference.to_lowercase().as_str() {
            "soft" | "soft-dark" => Self::aura_soft_dark(),
            "256" | "ansi" => Self::ansi_256(),
            _ => Self::aura_dark(),
        }
    }
}
