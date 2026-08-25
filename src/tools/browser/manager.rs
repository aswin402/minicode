use super::engine::{BrowserEngine, BrowserMode, EngineConfig, GUI_PRIORITY, HEADLESS_PRIORITY};
use crate::constants::{BROWSER_CDP_BASE_PORT, BROWSER_PROFILES_DIR, BROWSER_STARTUP_TIMEOUT_MS};
use crate::error::{Result, ToolError};
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

/// Running browser process with assigned CDP port and metadata
pub struct EngineProcess {
    #[allow(dead_code)]
    pub config: EngineConfig,
    pub child: Child,
    #[allow(dead_code)]
    pub cdp_port: u16,
    pub cdp_http_url: String,
}

impl EngineProcess {
    /// Kill process and all descendants
    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.child.kill().await;
        Ok(())
    }
}

/// Global supervisor managing active browser instances across modes
#[derive(Clone, Default)]
pub struct BrowserManager {
    #[allow(dead_code)]
    instances: Arc<Mutex<HashMap<BrowserMode, u16>>>, // Mode -> Port
}

impl BrowserManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            instances: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Discovers an available browser binary according to priority rules
    pub fn discover_engine(mode: BrowserMode, workspace_root: &Path) -> Option<EngineConfig> {
        let priority = match mode {
            BrowserMode::Headless => HEADLESS_PRIORITY,
            BrowserMode::Gui => GUI_PRIORITY,
        };

        for &engine in priority {
            if let Some(binary_path) = find_binary_for_engine(engine) {
                let port = find_available_port(BROWSER_CDP_BASE_PORT);
                let profile_dir = workspace_root.join(BROWSER_PROFILES_DIR).join(format!(
                    "{}_{}",
                    engine.to_string().to_lowercase(),
                    mode
                ));

                return Some(EngineConfig::new(
                    engine,
                    mode,
                    port,
                    binary_path,
                    profile_dir,
                ));
            }
        }

        None
    }

    /// Prepares isolated profile directory with engine-specific preferences
    pub fn setup_profile_dir(config: &EngineConfig) -> Result<()> {
        std::fs::create_dir_all(&config.profile_dir).map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed to create browser profile dir '{}': {}",
                config.profile_dir.display(),
                e
            ))
        })?;

        // For Firefox: write user.js enabling remote debugging without prompts
        if config.engine == BrowserEngine::Firefox {
            let user_js_path = config.profile_dir.join("user.js");
            let user_js_content = r#"
user_pref("devtools.debugger.remote-enabled", true);
user_pref("devtools.chrome.enabled", true);
user_pref("devtools.debugger.prompt-connection", false);
user_pref("remote.active-protocols", 3);
"#;
            let _ = std::fs::write(&user_js_path, user_js_content);
        }

        Ok(())
    }

    /// Builds command-line arguments for launching the browser engine
    pub fn build_launch_args(config: &EngineConfig) -> Vec<String> {
        let port_str = config.cdp_port.to_string();
        let profile_str = config.profile_dir.to_string_lossy().to_string();

        let mut args = match config.engine {
            BrowserEngine::Obscura => {
                // Global flags must precede the `serve` subcommand.
                // --allow-private-network matches minicode's own policy of
                // permitting loopback dev servers in validate_browser_url.
                vec![
                    "--stealth".to_string(),
                    "--allow-private-network".to_string(),
                    "serve".to_string(),
                    "--port".to_string(),
                    port_str,
                ]
            }
            BrowserEngine::Firefox => match config.mode {
                BrowserMode::Headless => {
                    vec![
                        "--headless".to_string(),
                        "--remote-debugging-port".to_string(),
                        port_str,
                        "--profile".to_string(),
                        profile_str,
                        "--no-remote".to_string(),
                    ]
                }
                BrowserMode::Gui => {
                    vec![
                        "--remote-debugging-port".to_string(),
                        port_str,
                        "--profile".to_string(),
                        profile_str,
                        "--no-remote".to_string(),
                    ]
                }
            },
            BrowserEngine::Chrome => match config.mode {
                BrowserMode::Headless => {
                    vec![
                        "--headless=new".to_string(),
                        format!("--remote-debugging-port={}", port_str),
                        format!("--user-data-dir={}", profile_str),
                        "--disable-gpu".to_string(),
                        "--no-sandbox".to_string(),
                        "--disable-dev-shm-usage".to_string(),
                        "--no-first-run".to_string(),
                        "--no-default-browser-check".to_string(),
                    ]
                }
                BrowserMode::Gui => {
                    vec![
                        format!("--remote-debugging-port={}", port_str),
                        format!("--user-data-dir={}", profile_str),
                        "--disable-gpu".to_string(),
                        "--no-first-run".to_string(),
                        "--no-default-browser-check".to_string(),
                    ]
                }
            },
        };

        args.extend(config.extra_args.clone());
        args
    }

    /// Spawns the browser engine process and waits until the CDP endpoint is responsive
    pub async fn spawn_engine(config: &EngineConfig) -> Result<EngineProcess> {
        Self::setup_profile_dir(config)?;
        let args = Self::build_launch_args(config);

        tracing::info!(
            engine = %config.engine,
            mode = %config.mode,
            port = config.cdp_port,
            binary = %config.binary_path.display(),
            "Launching browser engine"
        );

        let mut cmd = Command::new(&config.binary_path);
        cmd.args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        // Pass performance budgets for Obscura if applicable
        if config.engine == BrowserEngine::Obscura {
            cmd.env("OBSCURA_SCRIPT_DEADLINE_MS", "15000");
            cmd.env("OBSCURA_MODULE_BUDGET_MS", "5000");
        }

        let child = cmd.spawn().map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed to spawn {} ({}): {}",
                config.engine,
                config.binary_path.display(),
                e
            ))
        })?;

        let cdp_http_url = format!("http://127.0.0.1:{}", config.cdp_port);

        // Poll CDP readiness
        let start = Instant::now();
        let timeout = Duration::from_millis(BROWSER_STARTUP_TIMEOUT_MS);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(400))
            .build()
            .unwrap_or_default();

        let mut ready = false;
        let version_url = format!("{}/json/version", cdp_http_url);

        while start.elapsed() < timeout {
            if let Ok(resp) = client.get(&version_url).send().await {
                if resp.status().is_success() {
                    ready = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !ready {
            tracing::warn!(
                engine = %config.engine,
                port = config.cdp_port,
                "Browser did not respond to /json/version within timeout; proceeding with best effort"
            );
        }

        Ok(EngineProcess {
            config: config.clone(),
            child,
            cdp_port: config.cdp_port,
            cdp_http_url,
        })
    }
}

/// Helper to search system PATH for candidate binaries corresponding to a BrowserEngine
fn find_binary_for_engine(engine: BrowserEngine) -> Option<PathBuf> {
    let candidates: &[&str] = match engine {
        BrowserEngine::Obscura => &["obscura"],
        BrowserEngine::Firefox => &[
            "firefox",
            "firefox-esr",
            "firefox-developer-edition",
            "firefox-nightly",
        ],
        BrowserEngine::Chrome => &[
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "brave-browser",
            "brave",
            "msedge",
        ],
    };

    for &name in candidates {
        if let Ok(path) = which::which(name) {
            return Some(path);
        }
    }
    None
}

/// Finds an available TCP port starting from a base port
fn find_available_port(base_port: u16) -> u16 {
    for port in base_port..base_port + 50 {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    base_port
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_launch_args_obscura() {
        let config = EngineConfig::new(
            BrowserEngine::Obscura,
            BrowserMode::Headless,
            9222,
            PathBuf::from("/usr/bin/obscura"),
            PathBuf::from("/tmp/profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert_eq!(
            args,
            vec![
                "--stealth",
                "--allow-private-network",
                "serve",
                "--port",
                "9222"
            ]
        );
    }

    #[test]
    fn test_build_launch_args_firefox_headless() {
        let config = EngineConfig::new(
            BrowserEngine::Firefox,
            BrowserMode::Headless,
            9223,
            PathBuf::from("/usr/bin/firefox"),
            PathBuf::from("/tmp/ff_profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert!(args.contains(&"--headless".to_string()));
        assert!(args.contains(&"--remote-debugging-port".to_string()));
        assert!(args.contains(&"9223".to_string()));
        assert!(args.contains(&"--no-remote".to_string()));
    }

    #[test]
    fn test_build_launch_args_chrome_headless() {
        let config = EngineConfig::new(
            BrowserEngine::Chrome,
            BrowserMode::Headless,
            9224,
            PathBuf::from("/usr/bin/google-chrome"),
            PathBuf::from("/tmp/cr_profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert!(args.contains(&"--headless=new".to_string()));
        assert!(args.contains(&"--remote-debugging-port=9224".to_string()));
        assert!(args.contains(&"--disable-gpu".to_string()));
        assert!(args.contains(&"--user-data-dir=/tmp/cr_profile".to_string()));
    }

    #[test]
    fn test_build_launch_args_firefox_gui() {
        let config = EngineConfig::new(
            BrowserEngine::Firefox,
            BrowserMode::Gui,
            9225,
            PathBuf::from("/usr/bin/firefox"),
            PathBuf::from("/tmp/ff_gui_profile"),
        );
        let args = BrowserManager::build_launch_args(&config);
        assert!(!args.contains(&"--headless".to_string()));
        assert!(args.contains(&"--remote-debugging-port".to_string()));
    }

    #[test]
    fn test_find_available_port() {
        let port = find_available_port(9222);
        assert!(port >= 9222);
    }
}
