use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Supported browser engines for minicode automation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserEngine {
    /// Obscura: Ultra-lightweight Pure-Rust V8 engine with built-in stealth (~30MB RAM, <85ms boot)
    Obscura,
    /// Mozilla Firefox: LAST RESORT. Mozilla removed CDP in Firefox 141;
    /// only works with Firefox <= 140 ESR + remote.active-protocols pref.
    Firefox,
    /// Google Chrome / Chromium / Brave: Chromium Blink engine via CDP
    Chrome,
}

impl fmt::Display for BrowserEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowserEngine::Obscura => write!(f, "Obscura"),
            BrowserEngine::Firefox => write!(f, "Firefox"),
            BrowserEngine::Chrome => write!(f, "Chrome"),
        }
    }
}

/// Execution mode for the browser instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
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

/// Priority chain for headless automation: Obscura -> Chrome -> Firefox.
///
/// Firefox is LAST RESORT only: Mozilla removed CDP in Firefox 141
/// (fxdx.dev/cdp-retirement-in-firefox), so it only works with Firefox 140 ESR
/// or older plus the `remote.active-protocols` preference (written into the
/// profile's user.js automatically). Modern Firefox installs will fail CDP
/// startup and fall through to the HTTP reader. Full Firefox support requires
/// a WebDriver BiDi client (future work).
pub const HEADLESS_PRIORITY: &[BrowserEngine] = &[
    BrowserEngine::Obscura,
    BrowserEngine::Chrome,
    BrowserEngine::Firefox,
];

/// Priority chain for GUI windowed automation: Chrome -> Obscura -> Firefox.
///
/// Chrome is preferred for GUI mode: full CDP + real rendering. Obscura's
/// rendering pipeline suits snapshots; Firefox has the CDP caveat above.
pub const GUI_PRIORITY: &[BrowserEngine] = &[
    BrowserEngine::Chrome,
    BrowserEngine::Obscura,
    BrowserEngine::Firefox,
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
        assert_eq!(HEADLESS_PRIORITY[0], BrowserEngine::Obscura);
        assert_eq!(HEADLESS_PRIORITY[1], BrowserEngine::Chrome);
        assert_eq!(HEADLESS_PRIORITY[2], BrowserEngine::Firefox);

        assert_eq!(GUI_PRIORITY[0], BrowserEngine::Chrome);
        assert_eq!(GUI_PRIORITY[1], BrowserEngine::Obscura);
        assert_eq!(GUI_PRIORITY[2], BrowserEngine::Firefox);
    }

    #[test]
    fn test_engine_display() {
        assert_eq!(BrowserEngine::Obscura.to_string(), "Obscura");
        assert_eq!(BrowserEngine::Firefox.to_string(), "Firefox");
        assert_eq!(BrowserEngine::Chrome.to_string(), "Chrome");
    }
}
