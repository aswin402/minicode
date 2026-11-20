use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Supported browser engines for minicode automation.
///
/// Routing contract:
/// - **Primary Default**: `MiniBrowse` is always the primary default browser for both headless and GUI.
/// - **General Automation**: Can use `MiniBrowse` (primary), or `Chrome`, `Firefox`, `Safari`
///   (and `Obscura`) whenever requested by the user (`engine="..."`) or configured in settings.
/// - **Debug & Diagnostics**: Always prioritizes `MiniBrowse` (`MiniBrowse.webDebugAudit`,
///   `MiniBrowse.flutterInspect`, Shadow DOM piercing, CSS overflow & WCAG root-cause diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserEngine {
    /// MiniBrowse: Primary native low-RAM (~28-50MB) WebKit2GTK + Pure-Rust agentic browser
    /// built specifically for minicode (Research, Web Debug, Flutter VM & Semantics)
    MiniBrowse,
    /// Google Chrome / Chromium / Brave / Edge: Chromium Blink engine via CDP
    Chrome,
    /// Mozilla Firefox: Gecko engine via WebDriver BiDi (Firefox 141+), W3C WebDriver, or CDP
    Firefox,
    /// Apple Safari: WebKit engine via macOS `safaridriver` (W3C WebDriver HTTP)
    Safari,
    /// Obscura: Ultra-lightweight Pure-Rust V8 engine with built-in stealth (~30MB RAM, <85ms boot)
    Obscura,
}

impl BrowserEngine {
    /// Canonical lowercase configuration slug (`minibrowse`, `chrome`, `firefox`, `safari`, `obscura`).
    pub const fn as_config_slug(self) -> &'static str {
        match self {
            BrowserEngine::MiniBrowse => "minibrowse",
            BrowserEngine::Chrome => "chrome",
            BrowserEngine::Firefox => "firefox",
            BrowserEngine::Safari => "safari",
            BrowserEngine::Obscura => "obscura",
        }
    }

    /// Parses human/config/CLI engine names and common aliases (`minibrowser`, `ff`, `webkit`, `brave`, etc.).
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "minibrowse" | "minibrowser" | "mini-browse" | "mini-browser" | "mb" => {
                Some(BrowserEngine::MiniBrowse)
            }
            "chrome" | "chromium" | "chromium-browser" | "google-chrome" | "brave"
            | "brave-browser" | "edge" | "msedge" => Some(BrowserEngine::Chrome),
            "firefox" | "ff" | "gecko" | "firefox-esr" => Some(BrowserEngine::Firefox),
            "safari" | "safaridriver" | "webkit" => Some(BrowserEngine::Safari),
            "obscura" => Some(BrowserEngine::Obscura),
            _ => None,
        }
    }
}

impl fmt::Display for BrowserEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowserEngine::MiniBrowse => write!(f, "MiniBrowse"),
            BrowserEngine::Chrome => write!(f, "Chrome"),
            BrowserEngine::Firefox => write!(f, "Firefox"),
            BrowserEngine::Safari => write!(f, "Safari"),
            BrowserEngine::Obscura => write!(f, "Obscura"),
        }
    }
}

impl std::str::FromStr for BrowserEngine {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            format!(
                "Unknown browser engine '{}', expected 'minibrowse', 'chrome', 'firefox', 'safari', or 'obscura'",
                s
            )
        })
    }
}

/// Workload intent used to route Debug operations strictly to `MiniBrowse` while allowing
/// General browsing/testing to use `MiniBrowse` (primary) or user-selected `Chrome` / `Firefox` / `Safari`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BrowserPurpose {
    /// General navigation, UI interaction, screenshots, tabs, and state
    #[default]
    General,
    /// Runtime, DOM/CSS, QA, Research, and Flutter/Dart VM debugging — requires `MiniBrowse`
    Debug,
}

/// Execution mode for the browser instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BrowserMode {
    /// Headless: Background execution, zero screen clutter, fast and token-efficient
    #[default]
    Headless,
    /// GUI: Visible window for live developer inspection, layout debugging, or human takeover
    Gui,
}

impl fmt::Display for BrowserMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowserMode::Headless => write!(f, "headless"),
            BrowserMode::Gui => write!(f, "gui"),
        }
    }
}

impl std::str::FromStr for BrowserMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "headless" => Ok(BrowserMode::Headless),
            "gui" | "headed" | "window" => Ok(BrowserMode::Gui),
            other => Err(format!(
                "Unknown browser mode '{}', expected 'headless' or 'gui'",
                other
            )),
        }
    }
}

/// Priority chain for headless automation: MiniBrowse -> Chrome -> Firefox -> Safari -> Obscura.
pub const HEADLESS_PRIORITY: &[BrowserEngine] = &[
    BrowserEngine::MiniBrowse,
    BrowserEngine::Chrome,
    BrowserEngine::Firefox,
    BrowserEngine::Safari,
    BrowserEngine::Obscura,
];

/// Priority chain for GUI windowed automation: MiniBrowse -> Chrome -> Firefox -> Safari -> Obscura.
pub const GUI_PRIORITY: &[BrowserEngine] = &[
    BrowserEngine::MiniBrowse,
    BrowserEngine::Chrome,
    BrowserEngine::Firefox,
    BrowserEngine::Safari,
    BrowserEngine::Obscura,
];

/// Priority chain for Debug & Diagnostics: strictly pins `MiniBrowse` first regardless of general browser config.
pub const DEBUG_PRIORITY: &[BrowserEngine] = &[
    BrowserEngine::MiniBrowse,
    BrowserEngine::Chrome,
    BrowserEngine::Firefox,
    BrowserEngine::Safari,
    BrowserEngine::Obscura,
];

/// Launch configuration resolved for a specific browser engine
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub engine: BrowserEngine,
    pub mode: BrowserMode,
    pub cdp_port: u16,
    pub binary_path: PathBuf,
    pub profile_dir: PathBuf,
    #[allow(dead_code)]
    pub extra_args: Vec<String>,
}

impl EngineConfig {
    pub fn new(
        engine: BrowserEngine,
        mode: BrowserMode,
        cdp_port: u16,
        binary_path: PathBuf,
        profile_dir: PathBuf,
    ) -> Self {
        Self {
            engine,
            mode,
            cdp_port,
            binary_path,
            profile_dir,
            extra_args: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_browser_mode_parsing() {
        assert_eq!(
            BrowserMode::from_str("headless").unwrap(),
            BrowserMode::Headless
        );
        assert_eq!(BrowserMode::from_str("gui").unwrap(), BrowserMode::Gui);
        assert_eq!(BrowserMode::from_str("headed").unwrap(), BrowserMode::Gui);
        assert_eq!(BrowserMode::from_str("window").unwrap(), BrowserMode::Gui);
        assert!(BrowserMode::from_str("invalid").is_err());
    }

    #[test]
    fn test_priority_chains() {
        assert_eq!(HEADLESS_PRIORITY[0], BrowserEngine::MiniBrowse);
        assert_eq!(HEADLESS_PRIORITY[1], BrowserEngine::Chrome);
        assert_eq!(HEADLESS_PRIORITY[2], BrowserEngine::Firefox);
        assert_eq!(HEADLESS_PRIORITY[3], BrowserEngine::Safari);
        assert_eq!(HEADLESS_PRIORITY[4], BrowserEngine::Obscura);

        assert_eq!(GUI_PRIORITY[0], BrowserEngine::MiniBrowse);
        assert_eq!(GUI_PRIORITY[1], BrowserEngine::Chrome);
        assert_eq!(GUI_PRIORITY[2], BrowserEngine::Firefox);
        assert_eq!(GUI_PRIORITY[3], BrowserEngine::Safari);
        assert_eq!(GUI_PRIORITY[4], BrowserEngine::Obscura);

        assert_eq!(DEBUG_PRIORITY[0], BrowserEngine::MiniBrowse);
    }

    #[test]
    fn test_engine_display_and_parsing() {
        assert_eq!(BrowserEngine::MiniBrowse.to_string(), "MiniBrowse");
        assert_eq!(BrowserEngine::Chrome.to_string(), "Chrome");
        assert_eq!(BrowserEngine::Firefox.to_string(), "Firefox");
        assert_eq!(BrowserEngine::Safari.to_string(), "Safari");
        assert_eq!(BrowserEngine::Obscura.to_string(), "Obscura");

        assert_eq!(
            BrowserEngine::parse("minibrowser"),
            Some(BrowserEngine::MiniBrowse)
        );
        assert_eq!(
            BrowserEngine::parse("minibrowse"),
            Some(BrowserEngine::MiniBrowse)
        );
        assert_eq!(BrowserEngine::parse("chrome"), Some(BrowserEngine::Chrome));
        assert_eq!(
            BrowserEngine::parse("firefox"),
            Some(BrowserEngine::Firefox)
        );
        assert_eq!(BrowserEngine::parse("safari"), Some(BrowserEngine::Safari));
        assert!(BrowserEngine::from_str("unknown_browser").is_err());
    }
}
