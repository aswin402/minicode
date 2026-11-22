//! Terminal Nerd Font capability detection & zero-box icon resolver.
//!
//! Private Use Area (PUA) Nerd Font glyphs (`U+EA60..U+F2FF`) render as hollow tofu boxes (`▯`)
//! whenever the active terminal emulator is not configured with a Nerd Font (for example,
//! VS Code's integrated terminal using `JetBrains Mono` or `DejaVu Sans Mono`).
//!
//! Every Nerd Font icon site in `minicode` resolves through [`nf`] so that:
//! - Terminals with an active Nerd Font (or explicit `ui.nerd_font_pua = true` / `MINICODE_NERD_FONT=1`)
//!   render crisp, collision-free VS Code Codicons / FontAwesome glyphs (`U+EA60..U+F2DB`).
//! - Terminals without a Nerd Font automatically render curated, 1-cell standard Unicode glyphs
//!   (`✦`, `✎`, `⚙`, `❯`, `◎`, `◈`, `⎇`, `⌘`, `⌂`, `❖`, `▣`, `➤`, `✶`, `⚡`, `◷`, `✓`, `✗`)
//!   that exist in standard system fonts (`DejaVu Sans` / `Noto Sans`) — **zero `▯` boxes anywhere**.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

const OVERRIDE_AUTO: u8 = 0;
const OVERRIDE_FORCE_ON: u8 = 1;
const OVERRIDE_FORCE_OFF: u8 = 2;

static NERD_FONT_OVERRIDE: AtomicU8 = AtomicU8::new(OVERRIDE_AUTO);
static DETECTED_NERD_FONT: OnceLock<bool> = OnceLock::new();

/// Semantic icon slots shared by animations, activity bar, status bar, mode badges,
/// context meter, toast notifications, autocomplete menus, and inline tool cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NerdIcon {
    Think,
    Sparkle,
    Write,
    Edit,
    Read,
    File,
    Folder,
    Search,
    Globe,
    Terminal,
    Bug,
    Gear,
    Comment,
    Checklist,
    GitBranch,
    Database,
    Chip,
    Server,
    Bot,
    Rocket,
    Flame,
    Bolt,
    Clock,
    Symbol,
    Image,
    Check,
    Error,
    Warning,
    Info,
}

impl NerdIcon {
    /// Collision-free Nerd Font Codicon / FontAwesome BMP glyph (`U+EA60..U+F2DB`).
    /// Explicitly avoids `U+F000..U+F03F` which collide with `Verdana.ttf` / `Arial.ttf` PUA tables.
    #[must_use]
    pub const fn raw_pua(self) -> &'static str {
        match self {
            Self::Think => "\u{ea61}",     // cod-lightbulb
            Self::Sparkle => "\u{eb65}",   // cod-sparkle
            Self::Write => "\u{ea73}",     // cod-edit
            Self::Edit => "\u{eb10}",      // cod-tools
            Self::Read => "\u{eaa4}",      // cod-book
            Self::File => "\u{ea7b}",      // cod-file
            Self::Folder => "\u{ea83}",    // cod-folder
            Self::Search => "\u{ea6d}",    // cod-search
            Self::Globe => "\u{eb01}",     // cod-globe
            Self::Terminal => "\u{ea85}",  // cod-terminal
            Self::Bug => "\u{eaaf}",       // cod-bug
            Self::Gear => "\u{eaf8}",      // cod-settings-gear
            Self::Comment => "\u{ea6b}",   // cod-comment
            Self::Checklist => "\u{eaa6}", // cod-checklist
            Self::GitBranch => "\u{ea68}", // cod-git-branch
            Self::Database => "\u{eace}",  // cod-database
            Self::Chip => "\u{f2db}",      // fa-microchip
            Self::Server => "\u{eb50}",    // cod-server
            Self::Bot => "\u{eb08}",       // cod-hubot
            Self::Rocket => "\u{eb25}",    // cod-rocket
            Self::Flame => "\u{eaf2}",     // cod-flame
            Self::Bolt => "\u{f0e7}",      // fa-bolt
            Self::Clock => "\u{ea82}",     // cod-history / clock
            Self::Symbol => "\u{ea8b}",    // cod-symbol-method
            Self::Image => "\u{eb3f}",     // cod-file-media
            Self::Check => "\u{eab2}",     // cod-check
            Self::Error => "\u{ea87}",     // cod-error
            Self::Warning => "\u{ea6c}",   // cod-warning
            Self::Info => "\u{ea74}",      // cod-info
        }
    }

    /// Universal standard Unicode glyph verified in `DejaVu Sans` / `Noto Sans` (zero `▯` boxes).
    #[must_use]
    pub const fn universal(self) -> &'static str {
        match self {
            Self::Think => "✦",
            Self::Sparkle => "★",
            Self::Write => "✎",
            Self::Edit => "⚙",
            Self::Read => "§",
            Self::File => "◩",
            Self::Folder => "⌂",
            Self::Search => "◈",
            Self::Globe => "◎",
            Self::Terminal => "❯",
            Self::Bug => "⚒",
            Self::Gear => "⏣",
            Self::Comment => "❝",
            Self::Checklist => "⎇",
            Self::GitBranch => "⎇",
            Self::Database => "▣",
            Self::Chip => "⌘",
            Self::Server => "⊕",
            Self::Bot => "❖",
            Self::Rocket => "➤",
            Self::Flame => "✶",
            Self::Bolt => "⚡",
            Self::Clock => "◷",
            Self::Symbol => "ƒ",
            Self::Image => "◩",
            Self::Check => "✓",
            Self::Error => "✗",
            Self::Warning => "△",
            Self::Info => "◉",
        }
    }
}

/// Checks whether a font family string references a Nerd Font or Symbols Nerd Font patch.
#[must_use]
pub fn font_family_looks_like_nerd_font(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains("nerd")
        || lower.contains("meslolgs")
        || lower.contains("symbols nf")
        || lower.contains("nf,")
        || lower.contains("nf'")
        || lower.contains("nf\"")
        || lower.ends_with(" nf")
        || lower.ends_with("nf")
}

/// Inspects a VS Code / Electron `settings.json` payload to see if `terminal.integrated.fontFamily`
/// (or fallback `editor.fontFamily`) explicitly names a Nerd Font.
#[must_use]
pub fn vscode_settings_uses_nerd_font(contents: &str) -> bool {
    let mut terminal_font: Option<String> = None;
    let mut editor_font: Option<String> = None;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.starts_with("//") {
            continue;
        }
        let extract_val = |key: &str| -> Option<String> {
            let pos = line.find(key)?;
            let after_key = &line[pos + key.len()..];
            let colon = after_key.find(':')?;
            let val_part = after_key[colon + 1..].trim();
            let rest = val_part.strip_prefix('"')?;
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        };
        if let Some(v) = extract_val("\"terminal.integrated.fontFamily\"") {
            if !v.trim().is_empty() {
                terminal_font = Some(v);
            }
        } else if let Some(v) = extract_val("\"editor.fontFamily\"") {
            if !v.trim().is_empty() {
                editor_font = Some(v);
            }
        }
    }

    terminal_font
        .as_deref()
        .or(editor_font.as_deref())
        .is_some_and(font_family_looks_like_nerd_font)
}

fn detect_terminal_nerd_font_support() -> bool {
    #[cfg(test)]
    {
        false
    }
    #[cfg(not(test))]
    {
        if let Ok(val) = std::env::var("MINICODE_NERD_FONT") {
            let norm = val.trim().to_ascii_lowercase();
            if matches!(norm.as_str(), "1" | "true" | "yes" | "on") {
                return true;
            }
            if matches!(norm.as_str(), "0" | "false" | "no" | "off") {
                return false;
            }
        }

        // Terminals that ship built-in Symbols Nerd Font fallback automatically
        if std::env::var_os("KITTY_WINDOW_ID").is_some() {
            return true;
        }
        if let Ok(term) = std::env::var("TERM") {
            if term.to_ascii_lowercase().contains("kitty") {
                return true;
            }
        }
        if let Ok(tp) = std::env::var("TERM_PROGRAM") {
            let tp_lower = tp.trim().to_ascii_lowercase();
            if matches!(tp_lower.as_str(), "wezterm" | "ghostty") {
                return true;
            }
            if tp_lower == "vscode" {
                if let Some(cfg_dir) = dirs::config_dir() {
                    for app_dir in ["Code", "Cursor", "VSCodium", "Windsurf", "Antigravity"] {
                        let settings_path =
                            cfg_dir.join(app_dir).join("User").join("settings.json");
                        if let Ok(text) = std::fs::read_to_string(settings_path) {
                            if vscode_settings_uses_nerd_font(&text) {
                                return true;
                            }
                        }
                    }
                }
                return false;
            }
        }

        false
    }
}

/// Synchronizes the global runtime Nerd Font PUA mode from `config.ui.nerd_font_pua`.
/// - `true`: forces raw Nerd Font PUA Codicons on unconditionally.
/// - `false`: uses smart auto-detection (raw PUA only on terminals with Nerd Font support,
///   automatic zero-box universal Unicode icons everywhere else).
pub fn sync_nerd_font_pua_from_config(force_pua: bool) {
    let mode = if force_pua {
        OVERRIDE_FORCE_ON
    } else {
        OVERRIDE_AUTO
    };
    NERD_FONT_OVERRIDE.store(mode, Ordering::Relaxed);
}

/// Explicitly overrides PUA rendering (`Some(true)` = force PUA, `Some(false)` = force universal,
/// `None` = auto-detect). Used by runtime toggles and unit tests.
#[allow(dead_code)]
pub fn set_nerd_font_pua_override(force: Option<bool>) {
    let mode = match force {
        Some(true) => OVERRIDE_FORCE_ON,
        Some(false) => OVERRIDE_FORCE_OFF,
        None => OVERRIDE_AUTO,
    };
    NERD_FONT_OVERRIDE.store(mode, Ordering::Relaxed);
}

/// Returns `true` when raw Nerd Font PUA glyphs (`U+EA60..U+F2DB`) should be emitted directly.
#[must_use]
pub fn use_raw_nerd_font_pua() -> bool {
    match NERD_FONT_OVERRIDE.load(Ordering::Relaxed) {
        OVERRIDE_FORCE_ON => true,
        OVERRIDE_FORCE_OFF => false,
        _ => *DETECTED_NERD_FONT.get_or_init(detect_terminal_nerd_font_support),
    }
}

/// Resolves a semantic [`NerdIcon`] into either its native Nerd Font Codicon PUA glyph
/// (when the active terminal supports Nerd Fonts or `ui.nerd_font_pua` is enabled)
/// or its crisp, 1-cell standard Unicode equivalent (zero `▯` tofu boxes in any font).
#[must_use]
#[inline]
pub fn nf(icon: NerdIcon) -> &'static str {
    if use_raw_nerd_font_pua() {
        icon.raw_pua()
    } else {
        icon.universal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_family_and_vscode_settings_detection() {
        assert!(font_family_looks_like_nerd_font("FiraMono Nerd Font"));
        assert!(font_family_looks_like_nerd_font(
            "'JetBrainsMono NF', monospace"
        ));
        assert!(font_family_looks_like_nerd_font("MesloLGS NF"));
        assert!(!font_family_looks_like_nerd_font("JetBrains Mono"));
        assert!(!font_family_looks_like_nerd_font("DejaVu Sans Mono"));

        let no_nf = r#"{
            "editor.fontFamily": "JetBrains Mono",
            "terminal.integrated.fontFamily": "Fira Code"
        }"#;
        assert!(!vscode_settings_uses_nerd_font(no_nf));

        let with_nf = r#"{
            "editor.fontFamily": "JetBrains Mono",
            "terminal.integrated.fontFamily": "FiraMono Nerd Font"
        }"#;
        assert!(vscode_settings_uses_nerd_font(with_nf));
    }

    #[test]
    fn test_nf_resolver_auto_fallback_and_force_pua_override() {
        set_nerd_font_pua_override(None);
        assert!(!use_raw_nerd_font_pua());
        assert_eq!(nf(NerdIcon::GitBranch), "⎇");
        assert_eq!(nf(NerdIcon::Terminal), "❯");
        assert_eq!(nf(NerdIcon::Write), "✎");
        assert_eq!(nf(NerdIcon::Check), "✓");

        // Every universal glyph must be outside the Unicode Private Use Area (U+E000..U+F8FF)
        for icon in [
            NerdIcon::Think,
            NerdIcon::Sparkle,
            NerdIcon::Write,
            NerdIcon::Edit,
            NerdIcon::Read,
            NerdIcon::File,
            NerdIcon::Folder,
            NerdIcon::Search,
            NerdIcon::Globe,
            NerdIcon::Terminal,
            NerdIcon::Bug,
            NerdIcon::Gear,
            NerdIcon::Comment,
            NerdIcon::Checklist,
            NerdIcon::GitBranch,
            NerdIcon::Database,
            NerdIcon::Chip,
            NerdIcon::Server,
            NerdIcon::Bot,
            NerdIcon::Rocket,
            NerdIcon::Flame,
            NerdIcon::Bolt,
            NerdIcon::Clock,
            NerdIcon::Symbol,
            NerdIcon::Image,
            NerdIcon::Check,
            NerdIcon::Error,
            NerdIcon::Warning,
            NerdIcon::Info,
        ] {
            let uni = icon.universal();
            assert!(
                uni.chars().all(|c| !('\u{e000}'..='\u{f8ff}').contains(&c)),
                "Universal fallback for {:?} must not contain PUA chars: {:?}",
                icon,
                uni
            );
            let pua = icon.raw_pua();
            assert!(
                pua.chars().all(|c| !('\u{f000}'..='\u{f03f}').contains(&c)),
                "Raw PUA for {:?} must avoid collided Verdana/Arial range U+F000..U+F03F: {:?}",
                icon,
                pua
            );
        }

        set_nerd_font_pua_override(Some(true));
        assert!(use_raw_nerd_font_pua());
        assert_eq!(nf(NerdIcon::GitBranch), "\u{ea68}");
        assert_eq!(nf(NerdIcon::Terminal), "\u{ea85}");
        assert_eq!(nf(NerdIcon::Write), "\u{ea73}");
        assert_eq!(nf(NerdIcon::Check), "\u{eab2}");

        set_nerd_font_pua_override(None);
    }
}
