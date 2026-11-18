pub mod accessibility;
pub mod debug;
pub mod driver;
pub mod engine;
pub mod guard;
pub mod interaction;
pub mod manager;
pub mod markdown;
pub mod page_agent;
pub mod webdriver;

#[allow(unused_imports)]
pub use accessibility::AccessibilityManager;
#[allow(unused_imports)]
pub use debug::{ConsoleEntry, DebugCollector, LogLevel, NetworkErrorEntry};
#[allow(unused_imports)]
pub use driver::{CdpClient, MockRouteRule};
#[allow(unused_imports)]
pub use engine::{
    BrowserEngine, BrowserMode, BrowserPurpose, EngineConfig, DEBUG_PRIORITY, GUI_PRIORITY,
    HEADLESS_PRIORITY,
};
#[allow(unused_imports)]
pub use interaction::{BatchStep, BatchStepOutcome, BrowserInteractor};
#[allow(unused_imports)]
pub use manager::{
    with_browser_scope, with_browser_space, BrowserManager, EngineProcess, SpaceOwner,
};
#[allow(unused_imports)]
pub use markdown::SmartMarkdownExtractor;
#[allow(unused_imports)]
pub use page_agent::{PageAgent, PageMetrics, QaAuditReport, ScrollData, VisualElement};

use crate::constants::{
    BROWSER_BLOCKED_HOSTS, BROWSER_REPORTS_DIR, BROWSER_SCREENSHOTS_DIR, BROWSER_STATE_DIR,
};
use crate::error::{Result, SecurityError, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// An interactive element identified in the page's accessibility tree
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AriaElement {
    pub ref_id: String, // e.g. "@v1:e1", "@v1:e2"
    pub tag: String,
    pub role: String,
    pub name: String,
    pub attributes: HashMap<String, String>,
}

/// A snapshot of a web page's interactive accessibility tree and text content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageSnapshot {
    pub url: String,
    pub title: String,
    pub engine_used: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport_summary: Option<String>,
    pub interactive_elements: Vec<AriaElement>,
    pub text_summary: String,
}

pub struct BrowserController;

#[derive(Debug, Clone, Serialize)]
struct ReplayFrame {
    step: usize,
    action: String,
    url: String,
    timestamp: String,
    png_base64: String,
}

#[derive(Debug, Clone)]
struct ReplaySession {
    name: String,
    started_at: String,
    frames: Vec<ReplayFrame>,
}

const MAX_REPLAY_FRAMES: usize = 24;

static SESSION_SCREENSHOT_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

static REPLAY_RECORDER: tokio::sync::Mutex<Option<ReplaySession>> =
    tokio::sync::Mutex::const_new(None);

impl BrowserController {
    /// Resets the per-session screenshot counter and any active replay session when the browser engine shuts down.
    pub fn reset_screenshot_counter() {
        SESSION_SCREENSHOT_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut g) = REPLAY_RECORDER.try_lock() {
            *g = None;
        }
    }

    async fn record_engine_telemetry(
        engine: &manager::SharedEngine,
        space_opt: &Option<std::sync::Arc<manager::BrowserSpace>>,
        action: &str,
    ) {
        let space_count = engine.spaces.lock().await.len() + 1;
        BrowserManager::record_live_activity(
            space_count,
            space_opt.as_ref().map(|s| s.name.as_str()),
            action,
        );
    }

    async fn maybe_record_replay_frame(cdp: &CdpClient, action_label: &str) {
        {
            let guard = REPLAY_RECORDER.lock().await;
            if guard.is_none() {
                return;
            }
        }
        let Ok(Some((png_bytes, _))) = cdp.take_screenshot_extended(false, true).await else {
            return;
        };
        let url = cdp
            .evaluate_js("window.location.href")
            .await
            .unwrap_or_else(|_| "about:blank".to_string());
        let mut guard = REPLAY_RECORDER.lock().await;
        if let Some(ref mut session) = *guard {
            let step = session.frames.last().map(|f| f.step + 1).unwrap_or(1);
            if session.frames.len() >= MAX_REPLAY_FRAMES {
                session.frames.remove(1);
            }
            session.frames.push(ReplayFrame {
                step,
                action: action_label.to_string(),
                url,
                timestamp: chrono::Utc::now().format("%H:%M:%S%.3fZ").to_string(),
                png_base64: driver::general_base64_encode(&png_bytes),
            });
        }
    }

    /// Appends matching domain-specific skill hints (`browser-harness` `BH_DOMAIN_SKILLS` pattern)
    /// to `snapshot.viewport_summary` when workspace/global/builtin skills match the navigated host.
    pub(crate) fn attach_domain_skill_hints(snapshot: &mut PageSnapshot, workspace_root: &Path) {
        let store = crate::vault::store::VaultStore::new(workspace_root);
        let matches = store.find_domain_skills_for_url(&snapshot.url);
        if matches.is_empty() {
            return;
        }
        let formatted: Vec<String> = matches
            .iter()
            .map(|(name, desc)| {
                let short_desc = crate::utils::strings::truncate_display(desc, 90);
                format!("`{name}` ({short_desc}) -> `vault_load(name=\"{name}\")`")
            })
            .collect();
        let banner = format!("💡 Matching Domain Skill(s): {}", formatted.join(" | "));
        snapshot.viewport_summary = match snapshot.viewport_summary.take() {
            Some(existing) if !existing.is_empty() => Some(format!("{existing}\n{banner}")),
            _ => Some(banner),
        };
    }
    /// Navigates to a URL with automatic engine fallback and returns a structured ARIA snapshot
    pub async fn navigate_and_snapshot(
        url: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<PageSnapshot> {
        validate_browser_url(url)?;

        // Reconcile loopback port with active dev servers in the registry and resolve active loopback stack
        let (mut reconciled_url, reconciliation_notice) = reconcile_loopback_url(url).await;
        if let Some(ref notice) = reconciliation_notice {
            tracing::info!(notice = %notice, "Reconciled loopback target URL");
        }

        // TCP Socket Readiness Barrier: Verify that loopback target port is listening
        // before attempting CDP navigation to eliminate ConnectionRefused (OS 111) and CDP timeouts.
        if let Ok(mut parsed) = url::Url::parse(&reconciled_url) {
            let is_loopback = matches!(
                parsed.host_str(),
                Some("127.0.0.1") | Some("localhost") | Some("0.0.0.0") | Some("::1")
            );
            if is_loopback {
                let port = parsed.port().unwrap_or(80);
                if !wait_for_port_ready(port, std::time::Duration::from_millis(4000)).await {
                    return Err(ToolError::CommandExec(format!(
                        "Target server at http://127.0.0.1:{} is not yet accepting TCP connections (port {} closed). Please wait for compilation to complete or check logs via minitask(action=\"logs\").",
                        port, port
                    )).into());
                }
                if let Some(listening_host) =
                    crate::dev::ports::resolve_listening_loopback_host(port)
                {
                    let _ = parsed.set_host(Some(listening_host));
                    reconciled_url = parsed.to_string();
                }
            }
        }
        let target_url = reconciled_url.as_str();

        // Try launching preferred browser engine according to priority chain
        match BrowserManager::get_or_launch(mode, workspace_root).await {
            Ok(engine) => {
                if let Ok((cdp, acc_arc, space_opt)) = engine.active_session().await {
                    let engine_name = if let Some(ref sp) = space_opt {
                        format!(
                            "{} ({}, space={})",
                            engine.process.config.engine, mode, sp.name
                        )
                    } else {
                        format!("{} ({})", engine.process.config.engine, mode)
                    };
                    tracing::info!(engine = %engine_name, url = %target_url, "Navigating via browser engine");

                    let cdp_res = async {
                        cdp.navigate(target_url).await?;
                        let _ = PageAgent::inject_probe(&cdp).await;
                        cdp.get_document_html().await
                    }
                    .await;

                    if let Ok(html) = cdp_res {
                        let (elements, metrics) = {
                            let mut acc_mgr = acc_arc.lock().await;
                            let (_, els, m) =
                                BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr)
                                    .await;
                            (els, m)
                        };
                        if space_opt.is_none() && engine.is_aura_enabled() {
                            let _ = PageAgent::inject_simulator_aura(&cdp, Some("AI Agent Ready"))
                                .await;
                        }
                        let engine_used = if let Some(ref notice) = reconciliation_notice {
                            format!("{} [{}]", engine_name, notice)
                        } else {
                            engine_name
                        };
                        let mut snapshot = Self::build_aria_snapshot(target_url, &html, elements);
                        snapshot.engine_used = engine_used;
                        snapshot.viewport_summary = metrics.map(|m| m.format_summary());
                        Self::attach_domain_skill_hints(&mut snapshot, workspace_root);
                        Self::record_engine_telemetry(&engine, &space_opt, "nav").await;
                        Self::maybe_record_replay_frame(&cdp, &format!("navigate {target_url}"))
                            .await;
                        return Ok(snapshot);
                    } else if let Err(e) = cdp_res {
                        tracing::warn!(error = ?e, "CDP navigation failed; falling back to HTTP");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to launch browser; falling back to HTTP");
            }
        }

        // Fallback: zero-browser reader. file:// URLs cannot be fetched over
        // HTTP, so read them straight from disk.
        if let Ok(parsed) = url::Url::parse(url) {
            if parsed.scheme() == "file" {
                let path = parsed.to_file_path().map_err(|_| {
                    ToolError::CommandExec(format!("Invalid file:// URL '{}'", url))
                })?;
                let path = crate::sandbox::path::validate_path_in_workspace(workspace_root, &path)?;
                let html = std::fs::read_to_string(&path).map_err(|e| {
                    ToolError::CommandExec(format!(
                        "Failed to read '{}' from disk: {}",
                        path.display(),
                        e
                    ))
                })?;
                let mut snapshot = Self::parse_html_to_aria_snapshot(url, &html);
                snapshot.engine_used = "File Reader (No browser binary on PATH)".to_string();
                Self::attach_domain_skill_hints(&mut snapshot, workspace_root);
                return Ok(snapshot);
            }
        }

        // Fallback: zero-browser HTTP fetcher
        tracing::info!(url = %target_url, "No browser binary found or launched; using HTTP reader");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(crate::constants::WEB_USER_AGENT)
            .build()
            .map_err(|e| ToolError::CommandExec(format!("Failed to build HTTP client: {}", e)))?;

        let mut response = client.get(target_url).send().await;
        let is_loopback = target_url.contains("127.0.0.1")
            || target_url.contains("localhost")
            || target_url.contains("[::1]");
        if response.is_err() && is_loopback {
            let alt_url = if target_url.contains("127.0.0.1") {
                target_url.replace("127.0.0.1", "[::1]")
            } else {
                target_url.replace("localhost", "127.0.0.1")
            };
            if let Ok(alt_resp) = client.get(&alt_url).send().await {
                response = Ok(alt_resp);
            }
        }

        let mut loopback_retries = 0;
        while response.is_err() && is_loopback && loopback_retries < 3 {
            loopback_retries += 1;
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let resolved_url = normalize_loopback_url(target_url);
            response = client.get(&resolved_url).send().await;
            if response.is_err() && resolved_url.contains("localhost") {
                let v6_url = resolved_url.replace("localhost", "[::1]");
                response = client.get(&v6_url).send().await;
            }
        }

        let response = response.map_err(|e| {
            ToolError::CommandExec(format!("Failed to connect to '{}': {}", url, e))
        })?;

        let html = response
            .text()
            .await
            .map_err(|e| ToolError::CommandExec(format!("Failed to read response body: {}", e)))?;

        let mut snapshot = Self::parse_html_to_aria_snapshot(target_url, &html);
        snapshot.engine_used = "HTTP Reader (No browser binary on PATH)".to_string();
        Self::attach_domain_skill_hints(&mut snapshot, workspace_root);
        Ok(snapshot)
    }

    /// Takes a snapshot of the live DOM without re-navigating if the browser is already at `url`.
    /// If the browser is on a different URL or not yet started, navigates and snapshots as usual.
    pub async fn snapshot_live_or_navigate(
        url: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<PageSnapshot> {
        validate_browser_url(url)?;
        let (reconciled_url, _) = reconcile_loopback_url(url).await;

        if let Ok(engine) = BrowserManager::get_or_launch(mode, workspace_root).await {
            if let Ok((cdp, acc_arc, space_opt)) = engine.active_session().await {
                let engine_name = if let Some(ref sp) = space_opt {
                    format!(
                        "{} ({}, space={})",
                        engine.process.config.engine, mode, sp.name
                    )
                } else {
                    format!("{} ({})", engine.process.config.engine, mode)
                };
                // Check if current browser is already loaded at this URL
                let current_url = cdp
                    .evaluate_js("window.location.href")
                    .await
                    .unwrap_or_default();
                let clean_current = current_url.trim_matches('"').trim_end_matches('/');
                let clean_target = url.trim_end_matches('/');
                let clean_reconciled = reconciled_url.trim_end_matches('/');

                if !clean_current.is_empty()
                    && (clean_current == clean_target || clean_current == clean_reconciled)
                {
                    tracing::info!(
                        url = %reconciled_url,
                        "Live page is already active at URL; snapshotting current DOM state"
                    );
                    let _ = PageAgent::inject_probe(&cdp).await;
                    let html = cdp.get_document_html().await.unwrap_or_default();
                    let (elements, metrics) = {
                        let mut acc_mgr = acc_arc.lock().await;
                        let (_, els, m) =
                            BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
                        (els, m)
                    };
                    let mut snapshot = Self::build_aria_snapshot(&reconciled_url, &html, elements);
                    snapshot.engine_used = engine_name;
                    snapshot.viewport_summary = metrics.map(|m| m.format_summary());
                    return Ok(snapshot);
                }
            }
        }

        Self::navigate_and_snapshot(&reconciled_url, mode, workspace_root).await
    }

    /// Captures a snapshot of the currently active browser page, or navigates to `url_opt` if provided.
    pub async fn snapshot_active_or_navigate(
        url_opt: Option<&str>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<PageSnapshot> {
        if let Some(url) = url_opt {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                return Self::snapshot_live_or_navigate(trimmed, mode, workspace_root).await;
            }
        }

        // URL was omitted: check if a browser is already active and snapshot current page
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, acc_arc, space_opt) = engine.active_session().await?;
        let engine_name = if let Some(ref sp) = space_opt {
            format!(
                "{} ({}, space={})",
                engine.process.config.engine, mode, sp.name
            )
        } else {
            format!("{} ({})", engine.process.config.engine, mode)
        };
        let current_url = cdp
            .evaluate_js("window.location.href")
            .await
            .unwrap_or_default();
        let clean_current = current_url.trim_matches('"').trim_end_matches('/');

        let effective_url = if clean_current.is_empty() || clean_current == "about:blank" {
            "http://localhost"
        } else {
            clean_current
        };

        let _ = PageAgent::inject_probe(&cdp).await;
        let html = cdp.get_document_html().await.unwrap_or_default();
        let (elements, metrics) = {
            let mut acc_mgr = acc_arc.lock().await;
            let (_, els, m) =
                BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
            (els, m)
        };
        let mut snapshot = Self::build_aria_snapshot(effective_url, &html, elements);
        snapshot.engine_used = engine_name;
        snapshot.viewport_summary = metrics.map(|m| m.format_summary());
        Ok(snapshot)
    }

    /// Launches or retrieves the active browser engine + task-scoped Space session and ensures its ARIA tree is seeded.
    async fn prepare_interactive_engine(
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<(
        std::sync::Arc<manager::SharedEngine>,
        std::sync::Arc<CdpClient>,
        std::sync::Arc<tokio::sync::Mutex<AccessibilityManager>>,
        Option<std::sync::Arc<manager::BrowserSpace>>,
    )> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, acc_arc, space_opt) = engine.active_session().await?;
        {
            let mut acc_mgr = acc_arc.lock().await;
            if !acc_mgr.has_elements() {
                let _ = BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
            }
        }
        Ok((engine, cdp, acc_arc, space_opt))
    }

    /// Clicks an element by ARIA reference and returns the updated page snapshot
    pub async fn click_and_snapshot(
        target_ref: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (engine, cdp, acc_arc, space_opt) =
            Self::prepare_interactive_engine(mode, workspace_root).await?;
        let out = {
            let mut acc_mgr = acc_arc.lock().await;
            BrowserInteractor::click_element(&cdp, target_ref, &mut acc_mgr).await?
        };
        Self::record_engine_telemetry(&engine, &space_opt, "click").await;
        Self::maybe_record_replay_frame(&cdp, &format!("click {target_ref}")).await;
        Ok(out)
    }

    /// Dispatches a compositor-level coordinate click at `(x, y)` (`browser-harness` `click_at_xy`)
    /// and returns the updated page accessibility snapshot.
    pub async fn click_xy_and_snapshot(
        x: f64,
        y: f64,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (engine, cdp, acc_arc, space_opt) =
            Self::prepare_interactive_engine(mode, workspace_root).await?;
        cdp.click_at_xy(x, y).await?;
        let _ = cdp
            .wait_for_network_idle(std::time::Duration::from_millis(400))
            .await;
        let (rev, els, m) = {
            let mut acc_mgr = acc_arc.lock().await;
            acc_mgr.next_revision();
            BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await
        };
        let snap = Self::build_aria_snapshot(
            &cdp.evaluate_js("window.location.href")
                .await
                .unwrap_or_default(),
            &cdp.get_document_html().await.unwrap_or_default(),
            els,
        );
        Self::record_engine_telemetry(&engine, &space_opt, "click_xy").await;
        Self::maybe_record_replay_frame(&cdp, &format!("click ({x:.0}, {y:.0})")).await;
        let vp_line = m
            .map(|metrics| format!("{}\n\n", metrics.format_summary()))
            .unwrap_or_default();
        Ok(format!(
            "Clicked viewport coordinates `({x:.0}, {y:.0})` (DOM updated to revision v{rev}):\n\n{}{}",
            vp_line,
            Self::format_snapshot_report(&snap)
        ))
    }

    /// Fills text into an input or textarea element and returns the updated page snapshot
    pub async fn fill_and_snapshot(
        target_ref: &str,
        text: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (engine, cdp, acc_arc, space_opt) =
            Self::prepare_interactive_engine(mode, workspace_root).await?;
        let out = {
            let mut acc_mgr = acc_arc.lock().await;
            BrowserInteractor::fill_element(&cdp, target_ref, text, &mut acc_mgr).await?
        };
        Self::record_engine_telemetry(&engine, &space_opt, "fill").await;
        Self::maybe_record_replay_frame(&cdp, &format!("fill {target_ref}")).await;
        Ok(out)
    }

    /// Streams a secret directly from an approved read-only password CLI (`pass`, `op`, `bw`, `secret-tool`,
    /// `gopass`, `rbw`) into `target_ref` or `selector` without exposing the secret to LLM context (`agweb` pattern).
    pub async fn fill_with_secret_cmd_and_snapshot(
        target_ref: Option<&str>,
        selector: Option<&str>,
        secret_cmd: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let secret_val = guard::resolve_secret_command(secret_cmd).await?;
        let (engine, cdp, acc_arc, space_opt) =
            Self::prepare_interactive_engine(mode, workspace_root).await?;
        let raw_out = if let Some(r) = target_ref {
            let mut acc_mgr = acc_arc.lock().await;
            BrowserInteractor::fill_element(&cdp, r, &secret_val, &mut acc_mgr).await?
        } else if let Some(sel) = selector {
            let steps = vec![BatchStep::Fill {
                target_ref: None,
                selector: Some(sel.to_string()),
                text: secret_val.clone(),
            }];
            let mut acc_mgr = acc_arc.lock().await;
            BrowserInteractor::execute_batch(&cdp, &steps, &mut acc_mgr, workspace_root).await?
        } else {
            return Err(ToolError::InvalidArguments {
                name: "browser_fill".to_string(),
                reason: "Requires either 'ref' or 'selector' when using 'secret_cmd'".to_string(),
            }
            .into());
        };
        Self::record_engine_telemetry(&engine, &space_opt, "fill_secret").await;
        Self::maybe_record_replay_frame(&cdp, "fill [REDACTED_SECRET]").await;
        let scrubbed = if !secret_val.is_empty() {
            raw_out.replace(&secret_val, "[REDACTED_SECRET]")
        } else {
            raw_out
        };
        Ok(scrubbed)
    }

    /// Scrolls the active browser viewport in the given direction
    pub async fn scroll(
        direction: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;
        BrowserInteractor::scroll_page(&cdp, direction).await
    }

    /// Scrolls a specific container element by selector or ARIA reference
    pub async fn scroll_container(
        selector: Option<&str>,
        target_ref: Option<&str>,
        pixels: Option<i32>,
        direction: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (_engine, cdp, acc_arc, _) =
            Self::prepare_interactive_engine(mode, workspace_root).await?;
        let acc_mgr = acc_arc.lock().await;
        let mult = if direction.eq_ignore_ascii_case("up")
            || direction.eq_ignore_ascii_case("top")
            || direction.eq_ignore_ascii_case("pageup")
        {
            -1
        } else {
            1
        };
        let eff_pixels = mult * pixels.unwrap_or(450).abs();
        BrowserInteractor::scroll_container(&cdp, selector, target_ref, eff_pixels, &acc_mgr).await
    }

    /// Hovers over an element by ARIA reference and returns updated page snapshot
    pub async fn hover_and_snapshot(
        target_ref: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (_engine, cdp, acc_arc, _) =
            Self::prepare_interactive_engine(mode, workspace_root).await?;
        let mut acc_mgr = acc_arc.lock().await;
        BrowserInteractor::hover_element(&cdp, target_ref, &mut acc_mgr).await
    }

    /// Selects an option from a `<select>` dropdown and returns updated page snapshot
    pub async fn select_option_and_snapshot(
        target_ref: &str,
        option_text: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (_engine, cdp, acc_arc, _) =
            Self::prepare_interactive_engine(mode, workspace_root).await?;
        let mut acc_mgr = acc_arc.lock().await;
        BrowserInteractor::select_option(&cdp, target_ref, option_text, &mut acc_mgr).await
    }

    /// Scrolls horizontally left or right across the page or within a container
    pub async fn scroll_horizontally(
        direction: &str,
        pixels: Option<i32>,
        selector: Option<&str>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;
        BrowserInteractor::scroll_horizontally(&cdp, direction, pixels, selector).await
    }

    /// Retrieves scroll metrics and viewport geometry for the current page
    pub async fn get_page_metrics(mode: BrowserMode, workspace_root: &Path) -> Result<PageMetrics> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;
        PageAgent::get_page_metrics(&cdp).await
    }

    /// Toggles high-contrast visual badge overlays on interactive elements
    pub async fn toggle_visual_badges(
        enable: bool,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        if enable {
            let count = PageAgent::inject_visual_badges(&engine.cdp).await?;
            Ok(format!(
                "Injected visual badge overlays for {} interactive element(s).",
                count
            ))
        } else {
            let cleared = PageAgent::clear_visual_badges(&engine.cdp).await?;
            if cleared {
                Ok("Cleared visual badge overlays from page.".to_string())
            } else {
                Ok("No active visual badge overlays were present.".to_string())
            }
        }
    }

    /// Toggles the Alibaba-style visual simulator aura (luminous corner glows, AI cursor, status pill)
    pub async fn toggle_simulator_aura(
        enable: bool,
        status: Option<&str>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        engine.set_aura_enabled(enable);
        if enable {
            PageAgent::inject_simulator_aura(&engine.cdp, status).await?;
            let status_msg = status.unwrap_or("AI Agent Active");
            Ok(format!(
                "Activated browser simulator visual aura with status: \"{}\"",
                status_msg
            ))
        } else {
            let cleared = PageAgent::clear_simulator_aura(&engine.cdp).await?;
            if cleared {
                Ok("Deactivated browser simulator visual aura.".to_string())
            } else {
                Ok("No active simulator visual aura was present.".to_string())
            }
        }
    }

    /// Manages browser tabs, `open_or_reuse`, `attach_local_browser`, and isolated background `Spaces`
    pub async fn manage_tabs(
        action: &str,
        url: Option<&str>,
        target_id: Option<&str>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        if matches!(action, "attach_local_browser" | "attach") {
            return BrowserManager::attach_local_running_browser().await;
        }
        if matches!(action, "list" | "spaces" | "list_spaces" | "close_space") {
            let Some(engine) = BrowserManager::peek_live_engine().await else {
                return Ok("ℹ No active browser session is currently running.".to_string());
            };
            let (cdp, _, space_opt) = engine.active_session().await?;
            Self::record_engine_telemetry(&engine, &space_opt, action).await;
            return match action {
                "list" => {
                    let targets = cdp.list_targets().await?;
                    let mut out = format!(
                        "### Active Browser Tabs ({} • `{}`)\n\n",
                        targets.len(),
                        engine.process.config.engine
                    );
                    for (idx, t) in targets.iter().enumerate() {
                        let id = t
                            .get("targetId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let title = t
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(untitled)");
                        let target_url = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
                        let target_type = t.get("type").and_then(|v| v.as_str()).unwrap_or("page");
                        let ctx_note = t
                            .get("browserContextId")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| format!(" [ctx: `{s}`]"))
                            .unwrap_or_default();
                        out.push_str(&format!(
                            "{}. [{}] \"{}\" (`{}`) - ID: `{}`{}\n",
                            idx + 1,
                            target_type,
                            title,
                            target_url,
                            id,
                            ctx_note
                        ));
                    }
                    Ok(out)
                }
                "spaces" | "list_spaces" => {
                    let spaces = engine.list_spaces_summary().await;
                    let pretty =
                        serde_json::to_string_pretty(&spaces).unwrap_or_else(|_| "[]".to_string());
                    Ok(format!(
                        "### 🪐 Active Browser Spaces ({})\n```json\n{}\n```",
                        spaces.len(),
                        pretty
                    ))
                }
                _ => {
                    let name = target_id
                        .or_else(|| space_opt.as_ref().map(|s| s.name.as_str()))
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "browser_tabs".to_string(),
                            reason:
                                "Action 'close_space' requires 'space' or 'target_id' (Space name)"
                                    .to_string(),
                        })?;
                    let closed = engine.close_space(name).await;
                    Ok(if closed {
                        format!(
                            "Disposed isolated browser Space `{name}` and purged its ephemeral storage."
                        )
                    } else {
                        format!("No active isolated Space named `{name}` found.")
                    })
                }
            };
        }
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, space_opt) = engine.active_session().await?;
        Self::record_engine_telemetry(&engine, &space_opt, action).await;
        match action {
            "open_or_reuse" | "reuse" => {
                let target_url = url.unwrap_or("about:blank");
                if target_url != "about:blank" {
                    validate_browser_url(target_url)?;
                }
                let ctx_id = space_opt.as_ref().and_then(|s| s.context_id.as_deref());
                let res = cdp.open_or_reuse_tab(target_url, ctx_id).await?;
                let tid = res
                    .get("targetId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("main");
                let reused = res
                    .get("reused")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Ok(format!(
                    "{} browser tab `{}` at `{}`{}",
                    if reused { "Reused existing" } else { "Opened new" },
                    tid,
                    target_url,
                    space_opt
                        .as_ref()
                        .map(|s| format!(" in Space `{}`", s.name))
                        .unwrap_or_default()
                ))
            }
            "create" | "new" => {
                let target_url = url.unwrap_or("about:blank");
                if target_url != "about:blank" {
                    validate_browser_url(target_url)?;
                }
                let id = cdp.create_tab(target_url, space_opt.is_none()).await?;
                Ok(format!(
                    "Opened new browser tab navigating to '{}' (ID: `{}`)",
                    target_url, id
                ))
            }
            "switch" => {
                let id = target_id.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_tabs".to_string(),
                    reason: "Action 'switch' requires 'target_id'".to_string(),
                })?;
                cdp.switch_tab(id).await?;
                Ok(format!("Switched browser focus to tab ID `{}`", id))
            }
            "close" => {
                let id = target_id.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_tabs".to_string(),
                    reason: "Action 'close' requires 'target_id'".to_string(),
                })?;
                cdp.close_tab(id).await?;
                Ok(format!("Closed browser tab ID `{}`", id))
            }
            other => Err(ToolError::InvalidArguments {
                name: "browser_tabs".to_string(),
                reason: format!(
                    "Unknown tab action '{}'. Supported: list, open_or_reuse, attach_local_browser, spaces, close_space, create, switch, close",
                    other
                ),
            }
            .into()),
        }
    }

    /// Resolves the active session engine for CDP-agnostic QA/debug tools (`browser_qa_audit`,
    /// `browser_debug_bundle`, `browser_inspect_dom`, `browser_debug_logs`): reuses an already-live
    /// general engine (e.g. `Chrome` after `engine="chrome"`) or explicit `engine` override instead
    /// of spawning a duplicate `MiniBrowse` instance, falling back to `get_or_launch_debug` when idle.
    async fn prepare_debug_engine(
        url_opt: Option<&str>,
        explicit_mode: Option<BrowserMode>,
        workspace_root: &Path,
    ) -> Result<(
        std::sync::Arc<manager::SharedEngine>,
        Option<(BrowserEngine, String)>,
    )> {
        if let Some(live) = BrowserManager::peek_live_engine().await {
            if let Some(url) = url_opt.map(str::trim).filter(|s| !s.is_empty()) {
                validate_browser_url(url)?;
                let (reconciled, _) = reconcile_loopback_url(url).await;
                let (cdp, _, _) = live.active_session().await?;
                cdp.navigate(&reconciled).await?;
            }
            return Ok((live, None));
        }
        Self::prepare_minibrowse_debug_engine(url_opt, explicit_mode, workspace_root).await
    }

    /// Launches or retrieves the dedicated `MiniBrowse` debug engine for `MiniBrowse.*`-native RPCs
    /// (`diagnose_web_app`, `flutter_debug`, `visual_diff`, `memory_status`, etc.) without closing
    /// any open `Chrome` / `Firefox` / `Safari` general browser window.
    async fn prepare_minibrowse_debug_engine(
        url_opt: Option<&str>,
        explicit_mode: Option<BrowserMode>,
        workspace_root: &Path,
    ) -> Result<(
        std::sync::Arc<manager::SharedEngine>,
        Option<(BrowserEngine, String)>,
    )> {
        let companion = BrowserManager::snapshot_companion_general_context().await;
        let engine = BrowserManager::get_or_launch_debug(explicit_mode, workspace_root).await?;

        if let Some(url) = url_opt.map(str::trim).filter(|s| !s.is_empty()) {
            validate_browser_url(url)?;
            let (reconciled, _) = reconcile_loopback_url(url).await;
            engine.cdp.navigate(&reconciled).await?;
        } else if let Some((_, Some(ref active_url), _)) = companion {
            let cur = engine
                .cdp
                .evaluate_js("window.location.href")
                .await
                .unwrap_or_default();
            let cur_clean = cur.trim().trim_matches('"').trim_end_matches('/');
            let target_clean = active_url.trim().trim_end_matches('/');
            if cur_clean.is_empty() || cur_clean == "about:blank" || cur_clean != target_clean {
                let _ = engine.cdp.navigate(active_url).await;
            }
        }

        let companion_logs = companion.and_then(|(eng, _, logs)| {
            if logs.starts_with("No runtime console errors") {
                None
            } else {
                Some((eng, logs))
            }
        });
        Ok((engine, companion_logs))
    }

    /// Retrieves diagnostic logs (console errors, unhandled exceptions, and failed HTTP requests) via MiniBrowse
    /// (plus any live runtime logs captured in an active Chrome/Firefox/Safari companion session).
    pub async fn get_debug_logs(_mode: BrowserMode, workspace_root: &Path) -> Result<String> {
        let (engine, companion_logs) =
            Self::prepare_debug_engine(None, None, workspace_root).await?;

        let collector = DebugCollector::new();

        // Real console history captured live from Runtime.consoleAPICalled /
        // Runtime.exceptionThrown / Log.entryAdded CDP events.
        for entry in engine.cdp.drain_console().await {
            let level = if entry.starts_with("[error]") || entry.starts_with("[exception]") {
                LogLevel::Error
            } else if entry.starts_with("[warning]") {
                LogLevel::Warn
            } else {
                LogLevel::Info
            };
            collector.record_console(level, &entry);
        }

        // In-page console buffer installed at session start (engine-agnostic).
        if let Ok(entries) = engine
            .cdp
            .evaluate_js("(window.__minicode_console || []).join('\\n')")
            .await
        {
            let text = entries;
            if !text.is_empty() {
                for line in text.lines().filter(|l| !l.is_empty()) {
                    let level = if line.starts_with("[error]") {
                        LogLevel::Error
                    } else if line.starts_with("[warn]") {
                        LogLevel::Warn
                    } else {
                        LogLevel::Info
                    };
                    collector.record_console(level, line);
                }
            }
        }

        let mut report = collector.format_report();
        if let Some((comp_eng, comp_logs)) = companion_logs {
            report.push_str(&format!(
                "\n\n#### Active `{comp_eng}` Session Diagnostics:\n{comp_logs}"
            ));
        }
        Ok(report)
    }

    /// Evaluates JavaScript in the browser context and returns result (propagating uncaught JS exceptions as errors)
    pub async fn evaluate_js(
        script: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;
        cdp.evaluate_js_strict(script).await
    }

    /// Resolves screenshot paths so relative paths always live inside `.minicode/screenshots/`
    /// instead of polluting the project root directory.
    pub fn resolve_screenshot_path(
        workspace_root: &Path,
        custom_path: Option<&str>,
    ) -> std::path::PathBuf {
        let ensure_png = |mut path: std::path::PathBuf| -> std::path::PathBuf {
            if path.extension().is_none() {
                path.set_extension("png");
            }
            path
        };
        let dir = workspace_root.join(BROWSER_SCREENSHOTS_DIR);
        if let Some(raw) = custom_path.map(str::trim).filter(|s| !s.is_empty()) {
            let p = Path::new(raw);
            if p.is_absolute() {
                return ensure_png(p.to_path_buf());
            }
            let normalized = raw.trim_start_matches("./");
            if normalized.starts_with(".minicode/") {
                return ensure_png(workspace_root.join(normalized));
            }
            let sub = normalized
                .strip_prefix("screenshots/")
                .unwrap_or(normalized);
            return ensure_png(dir.join(sub));
        }
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        dir.join(format!("screenshot_{}.png", timestamp))
    }

    /// Scrolls the active viewport to a target CSS selector (accounting for fixed/sticky headers)
    /// or an explicit `scroll_y` pixel offset prior to capturing a screenshot.
    pub async fn scroll_before_capture(
        cdp: &CdpClient,
        selector: Option<&str>,
        scroll_y: Option<i32>,
    ) {
        if let Some(sel) = selector.map(str::trim).filter(|s| !s.is_empty()) {
            let payload = serde_json::json!({ "selector": sel });
            let scroll_js = format!(
                r#"(() => {{
                    if (document.documentElement) document.documentElement.style.scrollBehavior = 'auto';
                    if (document.body) document.body.style.scrollBehavior = 'auto';
                    const p = {};
                    let el = null;
                    try {{ el = document.querySelector(p.selector); }} catch (_) {{}}
                    if (!el) el = document.getElementById(p.selector.replace(/^#/, ''));
                    if (!el) return 'not_found';
                    let navOffset = 0;
                    const navs = document.querySelectorAll('header, nav, [role="banner"]');
                    for (const n of navs) {{
                        const cs = window.getComputedStyle ? window.getComputedStyle(n) : null;
                        if (cs && (cs.position === 'fixed' || cs.position === 'sticky') && cs.display !== 'none' && cs.visibility !== 'hidden') {{
                            const r = n.getBoundingClientRect();
                            if (r.top <= 16 && r.height > 0 && r.height < 160) {{
                                navOffset = Math.max(navOffset, r.height + 12);
                            }}
                        }}
                    }}
                    const rect = el.getBoundingClientRect();
                    const currentY = window.scrollY || (document.documentElement ? document.documentElement.scrollTop : 0) || 0;
                    const targetY = Math.max(0, currentY + rect.top - navOffset);
                    window.scrollTo({{ top: targetY, behavior: 'instant' }});
                    if (document.documentElement) document.documentElement.scrollTop = targetY;
                    window.dispatchEvent(new Event('scroll'));
                    return 'scrolled';
                }})()"#,
                payload
            );
            let _ = cdp.evaluate_js(&scroll_js).await;
        } else if let Some(y) = scroll_y {
            let target_y = y.max(0);
            let scroll_js = format!(
                "(() => {{ if (document.documentElement) document.documentElement.style.scrollBehavior = 'auto'; if (document.body) document.body.style.scrollBehavior = 'auto'; window.scrollTo({{ top: {}, behavior: 'instant' }}); if (document.documentElement) document.documentElement.scrollTop = {}; window.dispatchEvent(new Event('scroll')); return 'scrolled'; }})()",
                target_y, target_y
            );
            let _ = cdp.evaluate_js(&scroll_js).await;
        }
    }

    /// Captures a screenshot and saves it to `.minicode/screenshots/`
    pub async fn take_screenshot(
        mode: BrowserMode,
        workspace_root: &Path,
        custom_path: Option<&str>,
    ) -> Result<String> {
        Self::take_screenshot_with_scroll(mode, workspace_root, custom_path, None, None).await
    }

    /// Captures a screenshot (optionally scrolling to `selector` or `scroll_y` first) and saves it to `.minicode/screenshots/`
    pub async fn take_screenshot_with_scroll(
        mode: BrowserMode,
        workspace_root: &Path,
        custom_path: Option<&str>,
        selector: Option<&str>,
        scroll_y: Option<i32>,
    ) -> Result<String> {
        Self::take_screenshot_with_options(
            mode,
            workspace_root,
            custom_path,
            selector,
            scroll_y,
            false,
            false,
        )
        .await
    }

    /// Captures a screenshot with optional perceptual `--if-changed` gating and Set-of-Mark `--annotate` overlays.
    #[allow(clippy::too_many_arguments)]
    pub async fn take_screenshot_with_options(
        mode: BrowserMode,
        workspace_root: &Path,
        custom_path: Option<&str>,
        selector: Option<&str>,
        scroll_y: Option<i32>,
        if_changed: bool,
        annotate: bool,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, space_opt) = engine.active_session().await?;
        let keep_aura = space_opt.is_none()
            && engine.effective_mode() == BrowserMode::Gui
            && engine.is_aura_enabled();
        if keep_aura {
            let _ = cdp
                .evaluate_js(
                    "(() => { const el = document.getElementById('minicode-simulator-container'); if (el) el.style.setProperty('display', 'none', 'important'); return true; })()",
                )
                .await;
        } else {
            let _ = cdp
                .evaluate_js(
                    "(() => { const el = document.getElementById('minicode-simulator-container'); if (el) el.remove(); return true; })()",
                )
                .await;
        }
        Self::scroll_before_capture(&cdp, selector, scroll_y).await;
        if selector.is_some() || scroll_y.is_some() {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }
        // Settle GSAP ScrollTrigger / finite tweens and any scroll-reveal or inline `opacity: 0`
        // elements currently intersecting the viewport so screenshots never capture blank mid-reveal regions.
        let _ = cdp
            .evaluate_js(
                r#"(() => {
                    try {
                        if (window.ScrollTrigger && typeof window.ScrollTrigger.update === 'function') {
                            window.ScrollTrigger.update();
                        }
                        if (window.gsap && window.gsap.globalTimeline && typeof window.gsap.globalTimeline.getChildren === 'function') {
                            for (const child of window.gsap.globalTimeline.getChildren(true, true, true)) {
                                if (typeof child.repeat === 'function' && child.repeat() !== -1 && typeof child.progress === 'function') {
                                    const p = child.progress();
                                    if (p > 0 && p < 1) child.progress(1);
                                }
                            }
                        }
                    } catch (_) {}
                    const vh = window.innerHeight || 900;
                    const candidates = document.querySelectorAll('[class*="reveal"], [data-reveal], [data-visible], [data-animate], [style*="opacity"]');
                    for (const el of candidates) {
                        const r = el.getBoundingClientRect();
                        if (r.top < vh && r.bottom > 0 && r.width > 4 && r.height > 4) {
                            const isInlineZeroOpacity = el.style && (el.style.opacity === '0' || el.style.opacity === '0.0');
                            const isAttrReveal = el.matches('[class*="reveal"], [data-reveal], [data-visible], [data-animate]');
                            if (!isAttrReveal && !isInlineZeroOpacity) continue;
                            if (isInlineZeroOpacity) {
                                const cs = window.getComputedStyle(el);
                                if (cs.pointerEvents === 'none' || cs.visibility === 'hidden' || cs.display === 'none') continue;
                            }
                            if (el.classList) {
                                for (const cls of Array.from(el.classList)) {
                                    if (cls.endsWith('-pending') || cls.endsWith('-hidden') || cls === 'opacity-0') {
                                        el.classList.remove(cls);
                                    }
                                }
                                if (isAttrReveal) {
                                    el.classList.add('visible', 'is-visible', 'revealed');
                                }
                            }
                            if (isAttrReveal) {
                                el.setAttribute('data-visible', 'true');
                                el.setAttribute('data-revealed', 'true');
                            }
                            el.style.opacity = '1';
                            el.style.transform = 'none';
                            el.style.transitionDelay = '0s';
                            el.style.transitionDuration = '0s';
                        }
                    }
                    return true;
                })()"#,
            )
            .await;
        let png_res = cdp.take_screenshot_extended(if_changed, annotate).await;
        if keep_aura {
            let _ = cdp
                .evaluate_js(
                    "(() => { const el = document.getElementById('minicode-simulator-container'); if (el) el.style.removeProperty('display'); return true; })()",
                )
                .await;
        }
        let Some((png_bytes, _dist)) = png_res? else {
            return Ok(
                "⏭ Screenshot skipped (`if_changed=true`): viewport visually unchanged since previous capture — saved redundant vision tokens.".to_string(),
            );
        };
        let shot_num =
            SESSION_SCREENSHOT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

        let target_path = Self::resolve_screenshot_path(workspace_root, custom_path);
        if let Some(parent) = target_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        std::fs::write(&target_path, &png_bytes).map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed to write screenshot to '{}': {}",
                target_path.display(),
                e
            ))
        })?;

        let health_notes = Self::collect_dom_and_visual_health_notes(&cdp).await;

        let mut msg = format!(
            "Screenshot saved to '{}' ({} bytes)\n💡 Note: Screenshot is ALREADY attached directly to your active multimodal vision context for visual inspection — do NOT call `read_file` on screenshot files. (Tip: pass `selector: \"#section\"` or `scroll_y: <px>` directly to `browser_screenshot` instead of a separate `browser_eval` scroll call.)",
            target_path.display(),
            png_bytes.len()
        );
        if !health_notes.is_empty() {
            msg.push_str("\n\n[DOM & Visual Health Observation]:\n");
            msg.push_str(&health_notes.join("\n"));
        }
        if shot_num >= 6 {
            msg.push_str(&format!(
                "\n\n⚡ [Visual QA Budget Hint]: You have captured {} screenshots in this browser session. Prefer `browser_qa_audit` (which verifies desktop + mobile viewports, console errors, contrast, and overflow in 1 call) instead of taking redundant per-section screenshots across multiple widths. Live Vite/Next HMR updates the open tab automatically after file edits — capture `browser_screenshot(selector=\"#section\")` directly without running `build` or `browser_navigate` before each shot.",
                shot_num
            ));
        }

        Ok(msg)
    }

    /// Runs the DOM & Visual Health Observation probe on the active page and returns formatted diagnostic notes.
    pub async fn collect_dom_and_visual_health_notes(cdp: &CdpClient) -> Vec<String> {
        let health_probe_js = r#"(() => {
            try {
                const title = document.title || "";
                const hasHead = !!document.head;
                const hasBody = !!document.body;
                const styleSheets = document.styleSheets ? document.styleSheets.length : 0;
                const linkStyles = document.querySelectorAll('link[rel="stylesheet"]').length;
                const styleTags = document.querySelectorAll('style').length;
                const bodyComputed = (window.getComputedStyle && document.body) ? window.getComputedStyle(document.body) : null;
                const htmlComputed = (window.getComputedStyle && document.documentElement) ? window.getComputedStyle(document.documentElement) : null;
                const rootEl = document.querySelector('#root > div, #app > div, main, #root, body > div');
                const rootComputed = (window.getComputedStyle && rootEl) ? window.getComputedStyle(rootEl) : null;
                const fontFamily = bodyComputed ? bodyComputed.fontFamily : "";
                const isTransparent = (c) => !c || c === "rgba(0, 0, 0, 0)" || c === "transparent";
                const h1 = document.querySelector('h1, h2');
                const h1Computed = (window.getComputedStyle && h1) ? window.getComputedStyle(h1) : null;
                const headingColor = h1Computed ? h1Computed.color : (bodyComputed ? bodyComputed.color : "");
                let bgColor = "";
                if (window.getComputedStyle && h1) {
                    let anc = h1;
                    while (anc && anc !== document) {
                        const cs = window.getComputedStyle(anc);
                        if (cs && !isTransparent(cs.backgroundColor)) {
                            bgColor = cs.backgroundColor;
                            break;
                        }
                        anc = anc.parentElement;
                    }
                }
                if (isTransparent(bgColor) && bodyComputed && !isTransparent(bodyComputed.backgroundColor)) {
                    bgColor = bodyComputed.backgroundColor;
                }
                if (isTransparent(bgColor) && rootComputed && !isTransparent(rootComputed.backgroundColor)) {
                    bgColor = rootComputed.backgroundColor;
                }
                if (isTransparent(bgColor) && htmlComputed && !isTransparent(htmlComputed.backgroundColor)) {
                    bgColor = htmlComputed.backgroundColor;
                }
                const parseRgb = (str) => {
                    if (!str || isTransparent(str)) return [255, 255, 255];
                    try {
                        const cv = document.createElement('canvas');
                        cv.width = 1;
                        cv.height = 1;
                        const ctx = cv.getContext('2d');
                        if (ctx) {
                            ctx.fillStyle = '#ffffff';
                            ctx.fillRect(0, 0, 1, 1);
                            ctx.fillStyle = str;
                            ctx.fillRect(0, 0, 1, 1);
                            const d = ctx.getImageData(0, 0, 1, 1).data;
                            return [d[0], d[1], d[2]];
                        }
                    } catch (_) {}
                    return null;
                };
                const lum = (rgb) => {
                    if (!rgb) return null;
                    const srgb = rgb.map(v => {
                        const c = v / 255;
                        return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
                    });
                    return 0.2126 * srgb[0] + 0.7152 * srgb[1] + 0.0722 * srgb[2];
                };
                const bgLum = lum(parseRgb(bgColor) || [255, 255, 255]);
                const fgLum = lum(parseRgb(headingColor));
                let contrastRatio = null;
                if (bgLum !== null && fgLum !== null) {
                    const l1 = Math.max(bgLum, fgLum);
                    const l2 = Math.min(bgLum, fgLum);
                    contrastRatio = Number(((l1 + 0.05) / (l2 + 0.05)).toFixed(2));
                }
                const fLower = fontFamily.toLowerCase();
                const fTokens = fLower.split(',').map(s => s.trim().split('"').join('').split("'").join(''));
                const isDefaultSerif = (fLower.includes("times") || fTokens.includes("serif")) && !fLower.includes("sans-serif");
                const bodyTextLen = document.body ? (document.body.innerText || "").trim().length : 0;
                const semanticSections = document.querySelectorAll('section, main, article, nav, header, footer').length;
                const allSections = Array.from(document.querySelectorAll('section'));
                const vhForSections = window.innerHeight || 800;
                const hiddenSections = allSections.filter(s => {
                    if (!window.getComputedStyle) return false;
                    const cs = window.getComputedStyle(s);
                    const r = s.getBoundingClientRect();
                    return cs && cs.display !== 'none' && parseFloat(cs.opacity) === 0 && r.height > 20 && r.top < vhForSections && r.bottom > 0;
                }).length;
                const visibleNavs = Array.from(document.querySelectorAll('nav')).filter(n => {
                    if (!window.getComputedStyle) return true;
                    const cs = window.getComputedStyle(n);
                    if (!cs || cs.display === 'none' || cs.visibility === 'hidden' || parseFloat(cs.opacity) === 0) return false;
                    const r = n.getBoundingClientRect();
                    return r.width > 100 && r.height > 20 && r.top >= -10 && r.top < 80;
                });
                const overlappingTopNavs = visibleNavs.length > 1;
                const horizontalOverflowPx = Math.max(0, (document.documentElement ? document.documentElement.scrollWidth : 0) - (window.innerWidth || 0));
                const tasteMetrics = {
                    inlineLinkCollisions: 0,
                    collapsedSpacingUtilities: 0,
                    fixedHeaderClipped: false,
                    h1LineCount: 0,
                    h1Words: 0,
                    firstScreenH1: true,
                    lopsidedSections: 0,
                    offCentreSections: 0,
                    blankGaps: 0,
                    edgeTextCount: 0,
                    raggedCardRows: 0,
                    horizontalOverflowPx,
                    overflowCulprits: []
                };
                if (window.getComputedStyle) {
                    const vw = window.innerWidth || 1280;
                    const vh = window.innerHeight || 800;
                    if (horizontalOverflowPx > 4 && document.body) {
                        const allEls = document.body.querySelectorAll('*');
                        const fmtEl = (node) => {
                            const tag = (node.tagName || 'el').toLowerCase();
                            const idPart = node.id ? `#${node.id}` : '';
                            const cls = node.classList ? Array.from(node.classList).slice(0, 2).map(c => `.${c}`).join('') : '';
                            return `${tag}${idPart}${cls}`;
                        };
                        const candidates = [];
                        for (let i = 0; i < allEls.length && i < 5000; i++) {
                            const el = allEls[i];
                            const r = el.getBoundingClientRect();
                            if (r.width <= 0 || r.height <= 0 || r.right <= vw + 4) continue;
                            const cs = window.getComputedStyle(el);
                            if (!cs || cs.display === 'none' || cs.visibility === 'hidden') continue;
                            let clippedByScroll = false;
                            let anc = el.parentElement;
                            while (anc && anc !== document.body && anc !== document.documentElement) {
                                const acs = window.getComputedStyle(anc);
                                if (acs && (acs.overflowX === 'auto' || acs.overflowX === 'scroll' || acs.overflowX === 'hidden' || acs.overflowX === 'clip')) {
                                    const ar = anc.getBoundingClientRect();
                                    if (ar.right <= vw + 4 && ar.left >= -4) {
                                        clippedByScroll = true;
                                        break;
                                    }
                                }
                                anc = anc.parentElement;
                            }
                            if (clippedByScroll) continue;
                            let hasOverflowingChild = false;
                            for (let c = 0; c < el.children.length; c++) {
                                const cr = el.children[c].getBoundingClientRect();
                                if (cr.width > 0 && cr.height > 0 && cr.right > vw + 4) {
                                    hasOverflowingChild = true;
                                    break;
                                }
                            }
                            if (!hasOverflowingChild) {
                                const sec = el.closest('section[id], main, header, footer, nav');
                                const secPrefix = sec ? `${fmtEl(sec)} > ` : '';
                                let gridFlexHint = '';
                                let cur = el.parentElement;
                                while (cur && cur !== document.body) {
                                    const pcs = cur.parentElement ? window.getComputedStyle(cur.parentElement) : null;
                                    if (pcs && (pcs.display.includes('grid') || pcs.display.includes('flex'))) {
                                        const ccs = window.getComputedStyle(cur);
                                        if (ccs && ccs.minWidth !== '0px' && !cur.classList.contains('min-w-0')) {
                                            gridFlexHint = `, grid/flex item ${fmtEl(cur)} missing min-w-0`;
                                            break;
                                        }
                                    }
                                    cur = cur.parentElement;
                                }
                                candidates.push({
                                    right: Math.round(r.right),
                                    desc: `${secPrefix}${fmtEl(el)} (right=${Math.round(r.right)}px, width=${Math.round(r.width)}px${gridFlexHint})`
                                });
                            }
                        }
                        candidates.sort((a, b) => b.right - a.right);
                        const seen = new Set();
                        for (const item of candidates) {
                            if (!seen.has(item.desc)) {
                                seen.add(item.desc);
                                tasteMetrics.overflowCulprits.push(item.desc);
                                if (tasteMetrics.overflowCulprits.length >= 3) break;
                            }
                        }
                    }
                    const interactiveEls = Array.from(document.querySelectorAll('a, button'));
                    for (let i = 0; i < interactiveEls.length; i++) {
                        const a = interactiveEls[i];
                        const next = a.nextElementSibling;
                        if (!next || (next.tagName !== 'A' && next.tagName !== 'BUTTON')) continue;
                        const csA = window.getComputedStyle(a);
                        const csB = window.getComputedStyle(next);
                        if (csA.display === 'inline' && csB.display === 'inline') {
                            const rA = a.getBoundingClientRect();
                            const rB = next.getBoundingClientRect();
                            if (rA.width > 4 && rB.width > 4 && Math.abs(rA.top - rB.top) < 6 && Math.abs(rB.left - rA.right) < 3) {
                                tasteMetrics.inlineLinkCollisions++;
                            }
                        }
                    }
                    const padPrefixes = ['p-', 'px-', 'py-', 'pt-', 'pb-', 'pl-', 'pr-'];
                    const classEls = document.querySelectorAll('[class]');
                    for (let i = 0; i < classEls.length && i < 250; i++) {
                        const el = classEls[i];
                        const clsList = el.classList ? Array.from(el.classList) : [];
                        const hasPositivePadClass = clsList.some(c => {
                            for (const pref of padPrefixes) {
                                if (c.startsWith(pref)) {
                                    const num = Number(c.slice(pref.length));
                                    if (!Number.isNaN(num) && num > 0) return true;
                                }
                            }
                            return false;
                        });
                        if (hasPositivePadClass) {
                            const cs = window.getComputedStyle(el);
                            if (cs && cs.display !== 'none' &&
                                parseFloat(cs.paddingTop) === 0 &&
                                parseFloat(cs.paddingBottom) === 0 &&
                                parseFloat(cs.paddingLeft) === 0 &&
                                parseFloat(cs.paddingRight) === 0) {
                                tasteMetrics.collapsedSpacingUtilities++;
                            }
                        }
                    }
                    if ((window.scrollY || 0) <= 5 && visibleNavs.length >= 1) {
                        const navEl = visibleNavs[0];
                        const navCs = window.getComputedStyle(navEl);
                        const navRect = navEl.getBoundingClientRect();
                        if ((navCs.position === 'fixed' || navCs.position === 'sticky') && navRect.bottom > 30 && navRect.bottom <= 140) {
                            const firstMainEl = document.querySelector('main h1, section h1, main > section:first-of-type > div > div:first-child');
                            if (firstMainEl) {
                                const mRect = firstMainEl.getBoundingClientRect();
                                if (mRect.height > 0 && mRect.top < navRect.bottom - 4) {
                                    tasteMetrics.fixedHeaderClipped = true;
                                }
                            }
                        }
                    }

                    // Taste-Skill v2.1 Probe 1: Headline rendered line-wraps & word count
                    const h1El = document.querySelector('h1');
                    if (h1El) {
                        const h1Rect = h1El.getBoundingClientRect();
                        const h1Text = (h1El.innerText || '').trim();
                        tasteMetrics.h1Words = h1Text ? h1Text.split(/\s+/).length : 0;
                        if ((window.scrollY || 0) <= 30 && (h1Rect.height <= 0 || h1Rect.top > vh * 0.92)) {
                            tasteMetrics.firstScreenH1 = false;
                        }
                        try {
                            const walker = document.createTreeWalker(h1El, NodeFilter.SHOW_TEXT);
                            const range = document.createRange();
                            const lineTops = [];
                            let tNode;
                            while ((tNode = walker.nextNode())) {
                                if (!tNode.nodeValue || !tNode.nodeValue.trim()) continue;
                                range.selectNodeContents(tNode);
                                const rects = range.getClientRects();
                                for (let rIdx = 0; rIdx < rects.length; rIdx++) {
                                    const rr = rects[rIdx];
                                    if (rr.width < 2 || rr.height < 2) continue;
                                    if (!lineTops.some(y => Math.abs(y - rr.top) < 6)) {
                                        lineTops.push(rr.top);
                                    }
                                }
                            }
                            tasteMetrics.h1LineCount = lineTops.length;
                        } catch (_) {}
                    } else if ((window.scrollY || 0) <= 30 && bodyTextLen > 120) {
                        tasteMetrics.firstScreenH1 = false;
                    }

                    // Taste-Skill v2.1 Probe 2: Lopsided sections, off-centre outer margins & blank vertical gaps
                    const layoutSections = allSections.filter(s => {
                        const cs = window.getComputedStyle(s);
                        const r = s.getBoundingClientRect();
                        return cs && cs.display !== 'none' && cs.visibility !== 'hidden' && r.width >= vw * 0.65 && r.height >= 80;
                    });
                    let prevBottom = null;
                    for (let sIdx = 0; sIdx < layoutSections.length; sIdx++) {
                        const sec = layoutSections[sIdx];
                        const sr = sec.getBoundingClientRect();
                        if (prevBottom !== null && sr.top - prevBottom >= vh * 0.45) {
                            tasteMetrics.blankGaps++;
                        }
                        prevBottom = Math.max(prevBottom || 0, sr.bottom);
                    }
                    const visSections = layoutSections.filter(s => {
                        const cs = window.getComputedStyle(s);
                        return cs && parseFloat(cs.opacity) > 0.05;
                    });
                    const paintedTags = { DIV: 1, ARTICLE: 1, ASIDE: 1, UL: 1, OL: 1, DL: 1 };
                    for (let sIdx = 0; sIdx < visSections.length; sIdx++) {
                        const sec = visSections[sIdx];
                        const sr = sec.getBoundingClientRect();

                        if (sr.height >= 220 && vw >= 768) {
                            const children = Array.from(sec.querySelectorAll('div, article, aside, ul, ol, dl, figure, blockquote, pre, code, li, dt, dd, th, td, label, input, textarea, select, span, h1, h2, h3, h4, h5, h6, p, button, a, img, svg, canvas, video, table, form, [data-ui]'));
                            let minX = vw;
                            let maxX = 0;
                            let childCount = 0;
                            for (let cIdx = 0; cIdx < children.length && cIdx < 300; cIdx++) {
                                const child = children[cIdx];
                                const ccs = window.getComputedStyle(child);
                                if (!ccs || ccs.display === 'none' || ccs.visibility === 'hidden' || ccs.pointerEvents === 'none') continue;
                                if (paintedTags[child.tagName] && ccs.borderTopWidth === '0px' && ccs.backgroundColor === 'rgba(0, 0, 0, 0)') continue;
                                const cr = child.getBoundingClientRect();
                                if (cr.width < 8 || cr.height < 8 || cr.width >= vw * 0.99) continue;
                                minX = Math.min(minX, Math.max(0, cr.left));
                                maxX = Math.max(maxX, Math.min(vw, cr.right));
                                childCount++;
                            }
                            if (childCount >= 2 && maxX > minX) {
                                const leftMargin = minX;
                                const rightMargin = vw - maxX;
                                const emptyRatio = (leftMargin + rightMargin) / vw;
                                if (Math.abs(leftMargin - rightMargin) > vw * 0.28 && emptyRatio >= 0.42) {
                                    tasteMetrics.lopsidedSections++;
                                } else if (Math.abs(leftMargin - rightMargin) > 96 && leftMargin > 16 && rightMargin > 16 && emptyRatio < 0.35) {
                                    tasteMetrics.offCentreSections++;
                                }
                            }
                        }
                    }

                    // Taste-Skill v2.1 Probe 3: Edge-pressed text (< 12px from viewport edge) & ragged card rows
                    const textNodes = document.querySelectorAll('h1, h2, h3, p');
                    for (let tIdx = 0; tIdx < textNodes.length && tIdx < 80; tIdx++) {
                        const tr = textNodes[tIdx].getBoundingClientRect();
                        if (tr.width > 40 && tr.height > 10 && tr.top >= 0 && tr.bottom <= vh * 3) {
                            if ((tr.left >= 0 && tr.left < 10) || (tr.right > vw - 10 && tr.right <= vw)) {
                                tasteMetrics.edgeTextCount++;
                            }
                        }
                    }
                    const gridContainers = document.querySelectorAll('[class*="grid"], [class*="flex"]');
                    for (let gIdx = 0; gIdx < gridContainers.length && gIdx < 40; gIdx++) {
                        const kids = Array.from(gridContainers[gIdx].children).filter(k => {
                            const kr = k.getBoundingClientRect();
                            const kcs = window.getComputedStyle(k);
                            return kr.width >= 140 && kr.height >= 100 && kcs && (kcs.borderTopWidth !== '0px' || kcs.backgroundColor !== 'rgba(0, 0, 0, 0)');
                        });
                        if (kids.length >= 2 && kids.length <= 6) {
                            const firstR = kids[0].getBoundingClientRect();
                            const sameRow = kids.filter(k => Math.abs(k.getBoundingClientRect().top - firstR.top) < 12);
                            if (sameRow.length >= 2) {
                                const heights = sameRow.map(k => k.getBoundingClientRect().height);
                                if (Math.max(...heights) - Math.min(...heights) > 24) {
                                    tasteMetrics.raggedCardRows++;
                                }
                            }
                        }
                    }
                }
                return JSON.stringify({
                    title,
                    hasHead,
                    hasBody,
                    styleSheets,
                    linkStyles,
                    styleTags,
                    fontFamily,
                    bgColor,
                    headingColor,
                    contrastRatio,
                    isDefaultSerif,
                    bodyTextLen,
                    semanticSections,
                    hiddenSections,
                    totalSectionTags: allSections.length,
                    overlappingTopNavs,
                    inlineLinkCollisions: tasteMetrics.inlineLinkCollisions,
                    collapsedSpacingUtilities: tasteMetrics.collapsedSpacingUtilities,
                    fixedHeaderClipped: tasteMetrics.fixedHeaderClipped,
                    h1LineCount: tasteMetrics.h1LineCount,
                    h1Words: tasteMetrics.h1Words,
                    firstScreenH1: tasteMetrics.firstScreenH1,
                    lopsidedSections: tasteMetrics.lopsidedSections,
                    offCentreSections: tasteMetrics.offCentreSections,
                    blankGaps: tasteMetrics.blankGaps,
                    edgeTextCount: tasteMetrics.edgeTextCount,
                    raggedCardRows: tasteMetrics.raggedCardRows,
                    horizontalOverflowPx: tasteMetrics.horizontalOverflowPx,
                    overflowCulprits: tasteMetrics.overflowCulprits
                });
            } catch(e) {
                return JSON.stringify({ error: e.toString() });
            }
        })()"#;

        let mut health_notes = Vec::new();
        if let Ok(raw_json) = cdp.evaluate_js(health_probe_js).await {
            if let Ok(probe) = serde_json::from_str::<serde_json::Value>(&raw_json) {
                let has_head = probe
                    .get("hasHead")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let has_body = probe
                    .get("hasBody")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let style_sheets = probe
                    .get("styleSheets")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let link_styles = probe
                    .get("linkStyles")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let style_tags = probe.get("styleTags").and_then(|v| v.as_u64()).unwrap_or(0);
                let is_default_serif = probe
                    .get("isDefaultSerif")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let font_family = probe
                    .get("fontFamily")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let bg_color = probe.get("bgColor").and_then(|v| v.as_str()).unwrap_or("");
                let heading_color = probe
                    .get("headingColor")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let contrast_ratio = probe.get("contrastRatio").and_then(|v| v.as_f64());
                let body_len = probe
                    .get("bodyTextLen")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let sections = probe
                    .get("semanticSections")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let hidden_sections = probe
                    .get("hiddenSections")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let total_section_tags = probe
                    .get("totalSectionTags")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let overlapping_top_navs = probe
                    .get("overlappingTopNavs")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let inline_link_collisions = probe
                    .get("inlineLinkCollisions")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let collapsed_spacing_utilities = probe
                    .get("collapsedSpacingUtilities")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let fixed_header_clipped = probe
                    .get("fixedHeaderClipped")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let h1_line_count = probe
                    .get("h1LineCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let h1_words = probe.get("h1Words").and_then(|v| v.as_u64()).unwrap_or(0);
                let first_screen_h1 = probe
                    .get("firstScreenH1")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let lopsided_sections = probe
                    .get("lopsidedSections")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let off_centre_sections = probe
                    .get("offCentreSections")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let blank_gaps = probe.get("blankGaps").and_then(|v| v.as_u64()).unwrap_or(0);
                let edge_text_count = probe
                    .get("edgeTextCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let ragged_card_rows = probe
                    .get("raggedCardRows")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let horizontal_overflow_px = probe
                    .get("horizontalOverflowPx")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let overflow_culprits: Vec<String> = probe
                    .get("overflowCulprits")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let title = probe.get("title").and_then(|v| v.as_str()).unwrap_or("");

                if !has_head || !has_body {
                    health_notes.push("⚠ CRITICAL DOM DEFECT: Document is missing `<head>` or `<body>` tag! Page structure is truncated or malformed HTML.".to_string());
                }
                if style_sheets == 0 && link_styles == 0 && style_tags == 0 {
                    health_notes.push("⚠ UNSTYLED PAGE ALERT: 0 active stylesheets found! The page is rendering raw unstyled HTML. Check if `<link rel=\"stylesheet\">` is missing in index.html.".to_string());
                } else if is_default_serif {
                    health_notes.push(format!("⚠ UNSTYLED FONT WARNING: Page body is displaying default browser serif font ('{}'). Check if your CSS stylesheet was loaded.", font_family));
                }
                if collapsed_spacing_utilities >= 3 {
                    health_notes.push(format!(
                        "⚠ CRITICAL CSS CASCADE DEFECT: {} visible element(s) with Tailwind padding classes (`p-*`, `px-*`, `py-*`, `pt-*`) have computed padding `0px`! An unlayered `* {{ padding: 0; margin: 0; }}` rule in CSS is overriding Tailwind v4's `@layer utilities`. Remove the unlayered `*` reset or wrap it inside `@layer base {{ ... }}`.",
                        collapsed_spacing_utilities
                    ));
                }
                if fixed_header_clipped {
                    health_notes.push("⚠ FIXED NAVBAR CLIPPING DEFECT: The top of the Hero section is overlapping underneath the fixed `<nav>` bar! Ensure top padding (`pt-20` / `pt-24`) is active on the hero section and not zeroed out by an unlayered CSS reset.".to_string());
                }
                if overlapping_top_navs {
                    health_notes.push("⚠ DUPLICATE NAVBAR OVERLAP DEFECT: Multiple visible `<nav>` bars are rendering at the top of the page! If `src/layouts/RootLayout.tsx` renders `<Navbar />`, remove the duplicate navbar or update `RootLayout.tsx`.".to_string());
                }
                if inline_link_collisions > 0 {
                    health_notes.push(format!(
                        "⚠ INLINE LINK COLLISION WARNING: {} adjacent `<a>`/`<button>` sibling pair(s) have `display: inline` with zero horizontal separation and are running together on the same line! Add `block`, `inline-block`, or flex container spacing (`flex flex-col gap-2` or `flex gap-4`).",
                        inline_link_collisions
                    ));
                }
                if hidden_sections > 0 {
                    health_notes.push(format!(
                        "⚠ INVISIBLE SECTIONS WARNING: {} of {} `<section>` elements have computed `opacity: 0` and are invisible! Avoid locking sections at `opacity: 0` in scroll-reveal animations without immediate fallback.",
                        hidden_sections, total_section_tags
                    ));
                }
                if horizontal_overflow_px > 4 {
                    let culprits_detail = if overflow_culprits.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " Offending element(s): `{}`.",
                            overflow_culprits.join("`, `")
                        )
                    };
                    health_notes.push(format!(
                        "⚠ HORIZONTAL OVERFLOW WARNING: Document overflows viewport width by {}px!{} Add `min-w-0` to CSS Grid/Flex children containing wide `<pre>`/`<table>` blocks, or check for fixed-width children / missing `overflow-x-hidden`.",
                        horizontal_overflow_px, culprits_detail
                    ));
                }
                if h1_line_count > 2 || h1_words > 12 {
                    health_notes.push(format!(
                        "⚠ HEADLINE WRAP WARNING (`taste-skill` rule `headline-lines`): Primary `<h1>` wraps across {} rendered lines ({} words; target <= 2 lines, <= 10 words). Tighten the headline copy or reduce `clamp()` font-size.",
                        h1_line_count, h1_words
                    ));
                }
                if !first_screen_h1 {
                    health_notes.push(
                        "⚠ FIRST-SCREEN TITLE WARNING (`taste-skill` rule `no-title`): No visible `<h1>` found in the first viewport screen! Ensure the hero heading is immediately visible above the fold.".to_string(),
                    );
                }
                if lopsided_sections > 0 {
                    health_notes.push(format!(
                        "⚠ LOPSIDED SECTION WARNING (`taste-skill` rule `lopsided`): {} tall section(s) have content huddled on one side leaving >= 42% of the horizontal viewport width empty! Pair the copy with a live `data-ui` product fragment or center the container (`mx-auto max-w-*`).",
                        lopsided_sections
                    ));
                }
                if off_centre_sections > 0 {
                    health_notes.push(format!(
                        "⚠ OFF-CENTRE MARGIN WARNING (`taste-skill` rule `off-centre`): {} section(s) have asymmetric left vs. right outer viewport margins (>96px difference). Use `mx-auto` and symmetric horizontal padding (`px-6 md:px-12`).",
                        off_centre_sections
                    ));
                }
                if blank_gaps > 0 {
                    health_notes.push(format!(
                        "⚠ BLANK VERTICAL GAP WARNING (`taste-skill` rule `blank-gap`): {} vertical gap(s) >= 45% of viewport height detected between sections! Tighten excessive `py-*`/`my-*`/`min-h-screen` spacing.",
                        blank_gaps
                    ));
                }
                if edge_text_count > 0 {
                    health_notes.push(format!(
                        "⚠ EDGE-PRESSED TEXT WARNING (`taste-skill` rule `edge-text`): {} text element(s) are pressed within <10px of the viewport edge! Add horizontal container padding (`px-4 sm:px-6 lg:px-8`).",
                        edge_text_count
                    ));
                }
                if ragged_card_rows > 0 {
                    health_notes.push(format!(
                        "⚠ RAGGED CARD ROW WARNING (`taste-skill` rule `ragged-row`): {} card row(s) have sibling cards with mismatched bottom heights (>24px difference). Use `h-full flex flex-col justify-between` on grid cards.",
                        ragged_card_rows
                    ));
                }
                if let Some(ratio) = contrast_ratio {
                    if ratio < 2.0 {
                        health_notes.push(format!(
                            "⚠ CRITICAL CONTRAST DEFECT (WCAG ratio {:.2}:1): Heading text ('{}') is nearly invisible against the computed page background ('{}')! Check for invalid CSS variables or malformed color functions (e.g. invalid oklch() values) in your CSS.",
                            ratio, heading_color, bg_color
                        ));
                    }
                }
                if body_len < 50 {
                    health_notes.push("⚠ EMPTY BODY WARNING: Page body contains almost no text content (<50 characters). Check if content loaded properly.".to_string());
                }

                health_notes.push(format!(
                    "• Page Title: \"{}\" | Stylesheets: {} active ({} linked, {} embedded) | Sections: {} | Body Text: {} chars | Background: {} | Heading: {}",
                    title, style_sheets, link_styles, style_tags, sections, body_len, bg_color, heading_color
                ));
            }
        }

        health_notes
    }

    /// Executes an atomic pipeline of browser actions sequentially in a single turn
    pub async fn execute_batch(
        steps: &[BatchStep],
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, acc_arc, space_opt) = engine.active_session().await?;
        let out = {
            let mut acc_mgr = acc_arc.lock().await;
            if !acc_mgr.has_elements() {
                let _ = BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
            }
            BrowserInteractor::execute_batch(&cdp, steps, &mut acc_mgr, workspace_root).await?
        };
        Self::record_engine_telemetry(&engine, &space_opt, &format!("batch({})", steps.len()))
            .await;
        Self::maybe_record_replay_frame(&cdp, &format!("batch ({} steps)", steps.len())).await;
        Ok(out)
    }

    /// Registers or clears mock HTTP responses for route interception
    pub async fn mock_route(
        pattern: &str,
        status: u16,
        body: &str,
        content_type: Option<&str>,
        clear: bool,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        if clear {
            engine.cdp.clear_mock_routes().await?;
            Ok("Cleared all active browser mock routes.".to_string())
        } else {
            engine
                .cdp
                .add_mock_route(MockRouteRule {
                    pattern: pattern.to_string(),
                    status,
                    body: body.to_string(),
                    content_type: content_type.map(str::to_string),
                })
                .await?;
            Ok(format!(
                "Registered mock route for pattern '{}' with HTTP {} response.",
                pattern, status
            ))
        }
    }

    /// Blocks or unblocks URL patterns (e.g. ad networks, analytics)
    #[allow(dead_code)]
    pub async fn block_url(
        pattern: &str,
        clear: bool,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        if clear {
            engine.cdp.clear_blocked_urls().await?;
            Ok("Cleared all active URL block rules.".to_string())
        } else {
            engine.cdp.block_url_pattern(pattern).await?;
            Ok(format!(
                "Blocked URL pattern '{}' in browser engine.",
                pattern
            ))
        }
    }

    /// Aggregates console errors, uncaught exceptions, failed network calls, and MiniBrowse root-cause diagnostics
    pub async fn get_debug_bundle(_mode: BrowserMode, workspace_root: &Path) -> Result<String> {
        let (engine, companion_logs) =
            Self::prepare_debug_engine(None, None, workspace_root).await?;
        let (cdp, acc_arc, _) = engine.active_session().await?;
        let debug_report = cdp.debug_collector().format_report();
        let web_diag_opt = if engine.process.config.engine == BrowserEngine::MiniBrowse {
            cdp.minibrowse_web_diagnose().await.ok()
        } else {
            None
        };
        let mut acc_mgr = acc_arc.lock().await;
        let (_, elements, _) =
            BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;

        let mut report = format!(
            "### Browser Diagnostic Bundle (`{}` • `{}`)\n\n",
            engine.process.config.engine,
            engine.effective_mode()
        );
        report.push_str(&debug_report);
        if let Some((comp_eng, comp_logs)) = companion_logs {
            report.push_str(&format!(
                "\n\n#### Active `{comp_eng}` Session Diagnostics:\n{comp_logs}"
            ));
        }

        if let Some(diag) = web_diag_opt {
            if let Some(overlays) = diag
                .get("framework_overlays")
                .and_then(|v| v.as_array())
                .filter(|a| !a.is_empty())
            {
                report.push_str("\n\n#### 🚨 Framework Error Overlays (Shadow DOM):\n");
                for ov in overlays {
                    let fw = ov
                        .get("framework")
                        .and_then(|v| v.as_str())
                        .unwrap_or("app");
                    let err = ov.get("error").and_then(|v| v.as_str()).unwrap_or("");
                    report.push_str(&format!("  • **[{}]**: `{}`\n", fw, err));
                }
            }
            if let Some(culprits) = diag
                .get("overflow_culprits")
                .and_then(|v| v.as_array())
                .filter(|a| !a.is_empty())
            {
                report.push_str("\n\n#### 📐 Horizontal Overflow Culprits (Computed CSS):\n");
                for c in culprits.iter().take(6) {
                    let sel = c
                        .get("selector")
                        .and_then(|v| v.as_str())
                        .unwrap_or("element");
                    let px = c.get("overflow_px").and_then(|v| v.as_i64()).unwrap_or(0);
                    let css = c.get("computed").cloned().unwrap_or_default();
                    report.push_str(&format!(
                        "  • `{}` (+{}px past viewport) — `{}`\n",
                        sel, px, css
                    ));
                }
            }
        }

        report.push_str("\n\n---\n\n");
        report.push_str(&format!(
            "#### Active Page State (DOM Revision v{}, {} interactive elements):\n",
            acc_mgr.revision(),
            elements.len()
        ));
        for el in elements.iter().take(15) {
            report.push_str(&format!(
                "  • **{}** `<{}>` ({}) \"{}\"\n",
                el.ref_id, el.tag, el.role, el.name
            ));
        }
        if elements.len() > 15 {
            report.push_str(&format!("  ... +{} more elements\n", elements.len() - 15));
        }
        Ok(report)
    }

    /// Navigates (if `url_opt` is given) and runs MiniBrowse Prune4Web research extraction
    /// returning clean information-dense Markdown, code blocks, and citation links.
    pub async fn research_extract(
        url_opt: Option<&str>,
        query_opt: Option<&str>,
        max_chars: usize,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        Self::research_extract_with_format(
            url_opt,
            query_opt,
            max_chars,
            None,
            mode,
            workspace_root,
        )
        .await
    }

    /// Runs `MiniBrowse.researchExtract` supporting `"markdown"` and `"semantic_tree"` formats,
    /// fenced inside cryptographic nonce content boundaries for untrusted external URLs.
    pub async fn research_extract_with_format(
        url_opt: Option<&str>,
        query_opt: Option<&str>,
        max_chars: usize,
        format_opt: Option<&str>,
        _mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch_debug(None, workspace_root).await?;
        engine.cdp.set_minibrowse_resource_profile("research").await;
        if let Some(url) = url_opt.map(str::trim).filter(|s| !s.is_empty()) {
            validate_browser_url(url)?;
            let (reconciled, _) = reconcile_loopback_url(url).await;
            engine.cdp.navigate(&reconciled).await?;
        }
        let val_res = engine
            .cdp
            .minibrowse_research_with_format(query_opt, max_chars, format_opt)
            .await;
        engine.cdp.set_minibrowse_resource_profile("full").await;
        let val = val_res?;
        let title = val
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let page_url = val.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let md = val.get("markdown").and_then(|v| v.as_str()).unwrap_or("");
        let wrapped_md = guard::wrap_untrusted_web_output(page_url, md);

        let mut out = format!(
            "### 📚 MiniBrowse Research Extract: {}\n**Source**: `{}` | **Engine**: `{}`\n\n{}\n",
            title, page_url, engine.process.config.engine, wrapped_md
        );
        if let Some(cites) = val
            .get("citations")
            .and_then(|v| v.as_array())
            .filter(|a| !a.is_empty())
        {
            out.push_str("\n#### Citations & Outbound References:\n");
            for (i, c) in cites.iter().take(15).enumerate() {
                let ctitle = c.get("title").and_then(|v| v.as_str()).unwrap_or("link");
                let curl = c.get("url").and_then(|v| v.as_str()).unwrap_or("");
                out.push_str(&format!("{}. [{}]({})\n", i + 1, ctitle, curl));
            }
        }
        Ok(out)
    }

    /// Runs MiniBrowse deep root-cause Web diagnostics (Shadow DOM bundler overlays,
    /// CSS overflow culprits, WCAG 2.1 contrast ratios, fixed-navbar occlusion, and failed API calls).
    pub async fn diagnose_web_app(
        url_opt: Option<&str>,
        _mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (engine, _) =
            Self::prepare_minibrowse_debug_engine(url_opt, None, workspace_root).await?;
        let diag = engine.cdp.minibrowse_web_diagnose().await?;
        let pretty = serde_json::to_string_pretty(&diag).unwrap_or_else(|_| diag.to_string());
        Ok(format!(
            "### 🩺 MiniBrowse Web Root-Cause Diagnostics (`{}`)\n```json\n{}\n```",
            engine.process.config.engine, pretty
        ))
    }

    /// Inspects and controls Flutter Web Semantics + Dart VM Service (`widget_tree` with `.dart:line:col`
    /// source coordinates, `hot_reload`, `debug_paint`, `brightness`, `status`).
    pub async fn flutter_debug(
        action: &str,
        url_opt: Option<&str>,
        vm_url_opt: Option<&str>,
        query_opt: Option<&str>,
        enabled_opt: Option<bool>,
        _mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (engine, _) =
            Self::prepare_minibrowse_debug_engine(url_opt, None, workspace_root).await?;
        let params = serde_json::json!({
            "action": action,
            "vmUrl": vm_url_opt.unwrap_or(""),
            "query": query_opt.unwrap_or(""),
            "enabled": enabled_opt.unwrap_or(true),
            "dark": enabled_opt.unwrap_or(true),
        });
        let res = engine.cdp.minibrowse_flutter_inspect(params).await?;
        let pretty = serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string());
        Ok(format!(
            "### 🦋 MiniBrowse Flutter & Dart VM Inspector (`{}` • action=`{}`)\n```json\n{}\n```",
            engine.process.config.engine, action, pretty
        ))
    }

    /// Executes unified advanced PageAgent + MiniBrowse interactions (`press_key`, `drag`, `check`,
    /// `uncheck`, `double_click`, `right_click`, `wait_for_text`, `network_waterfall`, `storage_inspect`).
    pub async fn interact_advanced(
        action: &str,
        args: &serde_json::Value,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let norm_action = action.trim().to_ascii_lowercase();
        let is_minibrowse_native_action = matches!(
            norm_action.as_str(),
            "visual_diff"
                | "memory_status"
                | "trim_memory"
                | "media_diet"
                | "set_webgl"
                | "freeze_gui"
                | "thaw_gui"
                | "set_adblock"
                | "suspend_idle_tabs"
        );
        let (engine, cdp, acc_arc, space_opt) = if is_minibrowse_native_action {
            let (eng, _) =
                Self::prepare_minibrowse_debug_engine(None, None, workspace_root).await?;
            let (cdp, acc_arc, space_opt) = eng.active_session().await?;
            (eng, cdp, acc_arc, space_opt)
        } else {
            Self::prepare_interactive_engine(mode, workspace_root).await?
        };
        let target_ref = args.get("ref").and_then(|v| v.as_str());
        let selector = args.get("selector").and_then(|v| v.as_str());
        Self::record_engine_telemetry(&engine, &space_opt, &norm_action).await;

        match norm_action.as_str() {
            "press_key" | "key" | "keyboard" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Enter");
                let modifiers = args
                    .get("modifiers")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let tag =
                    PageAgent::press_key_with_aura(&cdp, key, target_ref, selector, modifiers)
                        .await?;
                let mut acc_mgr = acc_arc.lock().await;
                acc_mgr.next_revision();
                let (_, els, m) =
                    BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
                let snap = Self::build_aria_snapshot(
                    &cdp.evaluate_js("window.location.href")
                        .await
                        .unwrap_or_default(),
                    &cdp.get_document_html().await.unwrap_or_default(),
                    els,
                );
                let viewport_line = m
                    .map(|x| format!("{}\n\n", x.format_summary()))
                    .unwrap_or_default();
                Ok(format!(
                    "Pressed key `{key}` on `<{tag}>`.\n\n{}{}",
                    viewport_line,
                    Self::format_snapshot_report(&snap)
                ))
            }
            "drag" | "drag_and_drop" => {
                let src_ref = args
                    .get("source_ref")
                    .or_else(|| args.get("ref"))
                    .and_then(|v| v.as_str());
                let src_sel = args
                    .get("source_selector")
                    .or_else(|| args.get("selector"))
                    .and_then(|v| v.as_str());
                let dst_ref = args.get("target_ref").and_then(|v| v.as_str());
                let dst_sel = args.get("target_selector").and_then(|v| v.as_str());
                let dx = args.get("dx").and_then(|v| v.as_i64()).map(|v| v as i32);
                let dy = args.get("dy").and_then(|v| v.as_i64()).map(|v| v as i32);
                let ((fx, fy), (tx, ty)) = PageAgent::drag_and_drop_with_aura(
                    &cdp, src_ref, src_sel, dst_ref, dst_sel, dx, dy,
                )
                .await?;
                Ok(format!(
                    "Dragged from `({fx:.0}, {fy:.0})` to `({tx:.0}, {ty:.0})`."
                ))
            }
            "check" | "uncheck" => {
                let desired = norm_action == "check";
                let resulting =
                    PageAgent::set_checked_with_aura(&cdp, target_ref, selector, desired).await?;
                Ok(format!(
                    "Set checkbox/switch `{}` to `checked={resulting}`.",
                    target_ref.or(selector).unwrap_or("element")
                ))
            }
            "double_click" | "right_click" => {
                let (x, y) = PageAgent::pointer_action_with_aura(
                    &cdp,
                    &norm_action,
                    target_ref,
                    selector,
                )
                .await?;
                Ok(format!(
                    "Dispatched `{norm_action}` at `({x:.0}, {y:.0})`."
                ))
            }
            "wait_for_text" => {
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_interact".to_string(),
                        reason: "wait_for_text requires 'text'".to_string(),
                    })?;
                let timeout_ms = args
                    .get("timeout_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5000);
                BrowserInteractor::wait_for_text(
                    &cdp,
                    text,
                    selector,
                    std::time::Duration::from_millis(timeout_ms),
                )
                .await?;
                Ok(format!("Verified text {text:?} appeared in DOM."))
            }
            "network_waterfall" | "waterfall" => {
                let val = cdp.minibrowse_network_waterfall().await?;
                let pretty =
                    serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
                Ok(format!(
                    "### 🌐 MiniBrowse Network Waterfall & Navigation Timings\n```json\n{}\n```",
                    pretty
                ))
            }
            "storage_inspect" | "storage" => {
                let val = cdp.minibrowse_storage_inspect().await?;
                let pretty =
                    serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
                Ok(format!(
                    "### 🗄️ MiniBrowse Storage & Cookie Inspection (Secrets Masked)\n```json\n{}\n```",
                    pretty
                ))
            }
            "extract_tables" | "tables" => {
                let val = cdp.minibrowse_extract_tables(selector).await?;
                let url_str = val
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("about:blank")
                    .to_string();
                let pretty =
                    serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
                let body = format!(
                    "### 📊 MiniBrowse Structured Table Extraction\n```json\n{pretty}\n```"
                );
                Ok(guard::wrap_untrusted_web_output(&url_str, &body))
            }
            "handoff_to_user" | "handoff" => {
                let reason = args
                    .get("reason")
                    .or_else(|| args.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Login, SSO/2FA, or human verification required");
                let win_title = format!("minibrowse — Human Handoff: {reason}");
                let win_res = cdp
                    .set_window_mode(true, true, Some(&win_title))
                    .await
                    .unwrap_or_else(|_| serde_json::json!({ "ok": false, "mode": "headless" }));
                let promoted = win_res
                    .get("ok")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if promoted {
                    engine.set_gui_promoted(true);
                    engine.set_aura_enabled(true);
                }
                if let Some(ref sp) = space_opt {
                    sp.set_owner(SpaceOwner::User, Some(reason.to_string()))
                        .await;
                } else {
                    engine
                        .default_owner
                        .store(SpaceOwner::User as u8, std::sync::atomic::Ordering::Relaxed);
                }
                let banner = format!("⏸ Human Handoff: {reason}");
                let _ = PageAgent::inject_simulator_aura(&cdp, Some(&banner)).await;
                let cur_url = cdp
                    .evaluate_js("window.location.href")
                    .await
                    .unwrap_or_default();
                let space_label = space_opt
                    .as_ref()
                    .map(|s| s.name.as_str())
                    .unwrap_or("default");
                Ok(format!(
                    "🤝 **Live Human-in-the-Loop Handoff Activated** (`space={space_label}`, `gui_window_visible={promoted}`)\n\
                     • Active URL: `{cur_url}`\n\
                     • Reason: {reason}\n\
                     • Next step: Ask the user (via `ask_user` or plain text) to complete sign-in / 2FA / verification directly in the visible MiniBrowse window. Once they confirm, call `browser_interact(action=\"release_handoff\", profile=\"<auth_profile>\")` to persist the authenticated session and resume."
                ))
            }
            "release_handoff" | "resume_from_user" => {
                if let Some(ref sp) = space_opt {
                    sp.set_owner(SpaceOwner::Agent, None).await;
                } else {
                    engine
                        .default_owner
                        .store(SpaceOwner::Agent as u8, std::sync::atomic::Ordering::Relaxed);
                }
                let keep_gui = args
                    .get("keep_gui")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if !keep_gui {
                    if let Ok(res) = cdp.set_window_mode(false, false, None).await {
                        let any_gui = res
                            .get("any_gui_open")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        engine.set_gui_promoted(any_gui);
                    }
                } else {
                    let _ = PageAgent::inject_simulator_aura(&cdp, Some("AI Agent Active")).await;
                }
                let mut saved_note = String::new();
                if let Some(prof) = args
                    .get("profile")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                {
                    let desc = args.get("description").and_then(|v| v.as_str());
                    if let Ok(msg) = Self::save_session_state_with_meta(
                        prof,
                        desc,
                        mode,
                        workspace_root,
                    )
                    .await
                    {
                        saved_note = format!("\n\n{msg}");
                    }
                }
                let mut acc_mgr = acc_arc.lock().await;
                acc_mgr.next_revision();
                let (_, els, _) =
                    BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
                let snap = Self::build_aria_snapshot(
                    &cdp.evaluate_js("window.location.href")
                        .await
                        .unwrap_or_default(),
                    &cdp.get_document_html().await.unwrap_or_default(),
                    els,
                );
                Ok(format!(
                    "✅ Resumed agent control from Human Handoff (`keep_gui={keep_gui}`).{saved_note}\n\n{}",
                    Self::format_snapshot_report(&snap)
                ))
            }
            "forge_skill_blueprint" | "skill_forge" => {
                let raw_name = args
                    .get("skill_name")
                    .or_else(|| args.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("web-api-workflow");
                let save_skill = args
                    .get("save_skill")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let filter = args.get("filter").and_then(|v| v.as_str());
                let cur_url = cdp
                    .evaluate_js("window.location.href")
                    .await
                    .unwrap_or_else(|_| "https://example.com".to_string());
                let cur_title = cdp
                    .evaluate_js("document.title")
                    .await
                    .unwrap_or_default();
                let traffic = cdp
                    .minibrowse_api_traffic(filter, false)
                    .await
                    .unwrap_or_else(|_| serde_json::json!({ "entries": [] }));
                let tables = cdp
                    .minibrowse_extract_tables(None)
                    .await
                    .unwrap_or_else(|_| serde_json::json!({ "tables": [] }));
                let (slug, skill_md) = Self::synthesize_forged_skill_markdown(
                    raw_name,
                    &cur_url,
                    &cur_title,
                    &traffic,
                    &tables,
                );
                let mut saved_banner = String::new();
                if save_skill {
                    let skill_dir = workspace_root
                        .join(".minicode")
                        .join("skills")
                        .join(&slug);
                    tokio::fs::create_dir_all(&skill_dir).await.map_err(|e| {
                        ToolError::CommandExec(format!("Failed creating skill dir: {e}"))
                    })?;
                    let skill_file = skill_dir.join("SKILL.md");
                    tokio::fs::write(&skill_file, &skill_md)
                        .await
                        .map_err(|e| {
                            ToolError::CommandExec(format!("Failed writing forged SKILL.md: {e}"))
                        })?;
                    saved_banner = format!(
                        "\n💾 Persisted reusable skill directly to `.minicode/skills/{slug}/SKILL.md` (loadable anytime via `vault_load(name=\"{slug}\")`).\n"
                    );
                }
                Ok(format!(
                    "### 🛠️ MiniBrowse Native Skill Forge (`{slug}`){saved_banner}\n```markdown\n{skill_md}\n```"
                ))
            }
            "go_back" | "back" | "go_forward" | "forward" => {
                let forward = matches!(norm_action.as_str(), "go_forward" | "forward");
                let moved = cdp.navigate_history_step(forward).await?;
                let mut acc_mgr = acc_arc.lock().await;
                acc_mgr.next_revision();
                let (_, els, m) =
                    BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
                let snap = Self::build_aria_snapshot(
                    &cdp.evaluate_js("window.location.href")
                        .await
                        .unwrap_or_default(),
                    &cdp.get_document_html().await.unwrap_or_default(),
                    els,
                );
                let viewport_line = m
                    .map(|x| format!("{}\n\n", x.format_summary()))
                    .unwrap_or_default();
                let dir_label = if forward { "forward" } else { "back" };
                Ok(format!(
                    "Navigated history `{dir_label}` (moved={moved}).\n\n{}{}",
                    viewport_line,
                    Self::format_snapshot_report(&snap)
                ))
            }
            "upload_file" | "upload" | "set_files" => {
                let mut files: Vec<String> = args
                    .get("files")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                if files.is_empty() {
                    if let Some(single) = args
                        .get("file")
                        .or_else(|| args.get("path"))
                        .and_then(|v| v.as_str())
                    {
                        files.push(single.to_string());
                    }
                }
                if files.is_empty() {
                    return Err(ToolError::InvalidArguments {
                        name: "browser_interact".to_string(),
                        reason: "upload_file requires 'files' (array of file paths) or 'file'"
                            .to_string(),
                    }
                    .into());
                }
                let resolved_files: Vec<String> = files
                    .into_iter()
                    .map(|f| {
                        let p = std::path::Path::new(&f);
                        if p.is_absolute() {
                            f
                        } else {
                            workspace_root.join(p).to_string_lossy().to_string()
                        }
                    })
                    .collect();
                let eff_selector = selector.map(str::to_string).or_else(|| {
                    target_ref.map(|r| {
                        let clean = r.trim_start_matches('@');
                        format!("[data-minicode-ref=\"{clean}\"]")
                    })
                });
                let out = cdp
                    .set_file_input_files(eff_selector.as_deref(), &resolved_files)
                    .await?;
                Ok(format!(
                    "Uploaded {} file(s) into `<input type=\"file\">`: `{}`",
                    resolved_files.len(),
                    out
                ))
            }
            "configure_dialog" | "dialog" => {
                let accept = args
                    .get("accept")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let prompt_text = args
                    .get("prompt_text")
                    .or_else(|| args.get("text"))
                    .and_then(|v| v.as_str());
                cdp.configure_dialog(accept, prompt_text).await?;
                Ok(format!(
                    "Configured non-blocking JS dialog handler (`accept={accept}`, `prompt_text={prompt_text:?}`)."
                ))
            }
            "api_traffic" | "har" | "network_api" => {
                let filter = args
                    .get("filter")
                    .or_else(|| args.get("pattern"))
                    .and_then(|v| v.as_str());
                let only_errors = args
                    .get("only_errors")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let val = cdp.minibrowse_api_traffic(filter, only_errors).await?;
                let pretty =
                    serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
                Ok(format!(
                    "### 📡 MiniBrowse Captured API / XHR Traffic (Secrets Redacted)\n```json\n{pretty}\n```"
                ))
            }
            "visual_diff" | "diff_screenshot" | "diff_url" => {
                let baseline = args.get("baseline").and_then(|v| v.as_str());
                let save_only = args
                    .get("save_only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let url_a = args.get("url_a").and_then(|v| v.as_str());
                let url_b = args.get("url_b").and_then(|v| v.as_str());
                if let Some(ua) = url_a {
                    validate_browser_url(ua)?;
                }
                if let Some(ub) = url_b {
                    validate_browser_url(ub)?;
                }
                let (report_val, diff_png_opt) = cdp
                    .minibrowse_visual_diff(baseline, save_only, url_a, url_b)
                    .await?;
                let mut diff_path_note = String::new();
                if let Some(png_bytes) = diff_png_opt {
                    let safe_base: String = baseline
                        .unwrap_or("default")
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                        .take(32)
                        .collect();
                    let rel_name = format!(
                        "diff_{}.png",
                        if safe_base.is_empty() {
                            "default"
                        } else {
                            &safe_base
                        }
                    );
                    let diff_path = Self::resolve_screenshot_path(workspace_root, Some(&rel_name));
                    if let Some(parent) = diff_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if std::fs::write(&diff_path, &png_bytes).is_ok() {
                        diff_path_note = format!(
                            "\nVisual diff image saved to '{}' ({} bytes, red = changed pixels).",
                            diff_path.display(),
                            png_bytes.len()
                        );
                    }
                }
                let pretty = serde_json::to_string_pretty(&report_val)
                    .unwrap_or_else(|_| report_val.to_string());
                Ok(format!(
                    "### 🎯 MiniBrowse Visual Regression Diff{diff_path_note}\n```json\n{pretty}\n```"
                ))
            }
            "raw_cdp" | "cdp" => {
                let method = args
                    .get("cdp_method")
                    .or_else(|| args.get("method"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_interact".to_string(),
                        reason: "raw_cdp requires 'cdp_method' (e.g. 'DOM.getBoxModel', 'Performance.getMetrics', 'CSS.getComputedStyleForNode')".to_string(),
                    })?;
                if !method.contains('.')
                    || !method
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
                {
                    return Err(ToolError::InvalidArguments {
                        name: "browser_interact".to_string(),
                        reason: format!(
                            "Invalid CDP method '{method}'; expected 'Domain.method' identifier"
                        ),
                    }
                    .into());
                }
                let blocked_raw = matches!(
                    method,
                    "Network.getCookies"
                        | "Network.getAllCookies"
                        | "Storage.getCookies"
                        | "Browser.setDownloadBehavior"
                        | "Page.setDownloadBehavior"
                );
                if blocked_raw {
                    return Err(ToolError::InvalidArguments {
                        name: "browser_interact".to_string(),
                        reason: format!(
                            "CDP method '{method}' is restricted in raw_cdp to protect encrypted session cookies and workspace sandbox boundaries; use `browser_state` or `browser_interact(action=\"storage_inspect\")` instead."
                        ),
                    }
                    .into());
                }
                let params = args
                    .get("cdp_params")
                    .or_else(|| args.get("params"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let res = cdp.send_command(method, params).await?;
                let pretty =
                    serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string());
                let body = format!("### ⚡ Raw CDP `{method}` Result\n```json\n{pretty}\n```");
                if method.starts_with("Runtime.")
                    || method.starts_with("DOM.")
                    || method.starts_with("Accessibility.")
                {
                    let cur_url = cdp
                        .evaluate_js("window.location.href")
                        .await
                        .unwrap_or_default();
                    Ok(guard::wrap_untrusted_web_output(&cur_url, &body))
                } else {
                    Ok(body)
                }
            }
            "find" | "search_elements" => {
                let query = args
                    .get("query")
                    .or_else(|| args.get("text"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_interact".to_string(),
                        reason: "find requires 'query' (or 'text') keyword to filter interactive ARIA elements".to_string(),
                    })?;
                let q_lower = query.to_ascii_lowercase();
                let matches: Vec<AriaElement> = {
                    let mut acc_mgr = acc_arc.lock().await;
                    let (_, els, _) =
                        BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
                    els.into_iter()
                        .filter(|el| {
                            el.name.to_ascii_lowercase().contains(&q_lower)
                                || el.role.to_ascii_lowercase().contains(&q_lower)
                                || el.tag.to_ascii_lowercase().contains(&q_lower)
                                || el.ref_id.to_ascii_lowercase().contains(&q_lower)
                                || el.attributes.iter().any(|(k, v)| {
                                    k != "selector" && v.to_ascii_lowercase().contains(&q_lower)
                                })
                        })
                        .take(15)
                        .collect()
                };
                if matches.is_empty() {
                    Ok(format!(
                        "No interactive ARIA elements matched `{query}` on the current page."
                    ))
                } else {
                    let mut out = format!(
                        "Matched {} interactive element(s) for `{query}`:\n",
                        matches.len()
                    );
                    for el in &matches {
                        out.push_str(&format!(
                            "• **{}** `<{}>` ({}) \"{}\"\n",
                            el.ref_id,
                            el.tag,
                            el.role,
                            crate::utils::strings::truncate_display(&el.name, 80)
                        ));
                    }
                    let cur_url = cdp
                        .evaluate_js("window.location.href")
                        .await
                        .unwrap_or_default();
                    Ok(guard::wrap_untrusted_web_output(&cur_url, &out))
                }
            }
            "set_adblock" | "adblock" => {
                let enabled = args
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let active = cdp.set_adblock(enabled).await?;
                Ok(format!(
                    "🛡 MiniBrowse native content/tracker blocker set to `enabled={active}`."
                ))
            }
            "suspend_idle_tabs" | "suspend" => {
                let idle_secs = args
                    .get("idle_secs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(60);
                let res = cdp.suspend_idle_tabs(idle_secs).await?;
                let pretty =
                    serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string());
                Ok(format!(
                    "### 💤 MiniBrowse Idle Background Tab Suspension (`idle_secs={idle_secs}`)\n```json\n{pretty}\n```"
                ))
            }
            "media_diet" | "set_media_diet" | "audio_only" => {
                let enabled = args.get("enabled").and_then(|v| v.as_bool());
                let audio_only = args
                    .get("audio_only")
                    .and_then(|v| v.as_bool())
                    .or_else(|| (norm_action == "audio_only").then_some(true));
                let res = cdp.set_media_diet(enabled, audio_only).await?;
                let pretty =
                    serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string());
                Ok(format!(
                    "### 🎬 MiniBrowse Smart Media & Codec Diet\n```json\n{pretty}\n```"
                ))
            }
            "set_webgl" | "webgl" => {
                let enabled = args
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let res = cdp.set_webgl(enabled).await?;
                let pretty =
                    serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string());
                Ok(format!(
                    "### 🎮 MiniBrowse GPU / WebGL Pipeline Mode (`enabled={enabled}`)\n```json\n{pretty}\n```"
                ))
            }
            "freeze_gui" | "thaw_gui" => {
                let freeze = if norm_action == "thaw_gui" {
                    false
                } else {
                    args.get("freeze")
                        .or_else(|| args.get("enabled"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true)
                };
                let res = cdp.freeze_gui(freeze).await?;
                let pretty =
                    serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string());
                Ok(format!(
                    "### ❄️ MiniBrowse Freeze-Frame GUI Snapshot Hibernation (`freeze={freeze}`)\n```json\n{pretty}\n```"
                ))
            }
            "set_content" | "set_document_content" => {
                let html = args
                    .get("html")
                    .or_else(|| args.get("text"))
                    .or_else(|| args.get("content"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_interact".to_string(),
                        reason: "set_content requires 'html' (or 'content' / 'text') string"
                            .to_string(),
                    })?;
                cdp.set_document_content(html).await?;
                let mut acc_mgr = acc_arc.lock().await;
                acc_mgr.next_revision();
                let (_, els, _) =
                    BrowserInteractor::refresh_accessibility_tree(&cdp, &mut acc_mgr).await;
                let snap = Self::build_aria_snapshot("about:srcdoc", html, els);
                Ok(format!(
                    "Injected raw HTML via `Page.setDocumentContent` ({} bytes).\n\n{}",
                    html.len(),
                    Self::format_snapshot_report(&snap)
                ))
            }
            "memory_status" | "trim_memory" => {
                let do_trim = norm_action == "trim_memory"
                    || args.get("trim").and_then(|v| v.as_bool()).unwrap_or(false)
                    || args.get("hard").and_then(|v| v.as_bool()).unwrap_or(false);
                let hard = args.get("hard").and_then(|v| v.as_bool()).unwrap_or(false);
                match cdp.status_and_trim_memory(do_trim, hard).await {
                    Ok(val) => {
                        let pretty =
                            serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
                        Ok(format!(
                            "### 🧠 Browser Full Process-Tree Memory & Telemetry\n```json\n{pretty}\n```"
                        ))
                    }
                    Err(_) => {
                        let fallback = engine
                            .process
                            .pid()
                            .and_then(crate::dev::ports::sample_process_tree_memory_mb)
                            .map(|(pss, rss, threads)| {
                                serde_json::json!({
                                    "engine": format!("{}", engine.process.config.engine),
                                    "pss_mb": pss,
                                    "rss_mb": rss,
                                    "tree_threads": threads,
                                })
                            })
                            .unwrap_or_else(|| serde_json::json!({ "note": "External browser attached" }));
                        let pretty = serde_json::to_string_pretty(&fallback)
                            .unwrap_or_else(|_| fallback.to_string());
                        Ok(format!(
                            "### 🧠 Browser Full Process-Tree Memory & Telemetry\n```json\n{pretty}\n```"
                        ))
                    }
                }
            }
            "record_start" => {
                let raw_name = args
                    .get("name")
                    .or_else(|| args.get("profile"))
                    .or_else(|| args.get("baseline"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("session");
                let safe_name: String = raw_name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .take(40)
                    .collect();
                let safe_name = if safe_name.is_empty() {
                    "session".to_string()
                } else {
                    safe_name
                };
                {
                    let mut guard = REPLAY_RECORDER.lock().await;
                    *guard = Some(ReplaySession {
                        name: safe_name.clone(),
                        started_at: chrono::Utc::now().to_rfc3339(),
                        frames: Vec::new(),
                    });
                }
                Self::maybe_record_replay_frame(&cdp, "record_start").await;
                Ok(format!(
                    "🎬 Started visual browser action replay recording (`{safe_name}`). Subsequent navigations, clicks, fills, and batches will capture annotated pointer-reticle frames until `browser_interact(action=\"record_stop\")`."
                ))
            }
            "record_stop" => {
                Self::maybe_record_replay_frame(&cdp, "record_stop").await;
                let session_opt = {
                    let mut guard = REPLAY_RECORDER.lock().await;
                    guard.take()
                };
                let Some(session) = session_opt else {
                    return Ok(
                        "ℹ No active visual replay recording was running. Call `browser_interact(action=\"record_start\")` first."
                            .to_string(),
                    );
                };
                let dir = workspace_root.join(BROWSER_REPORTS_DIR);
                tokio::fs::create_dir_all(&dir).await.map_err(|e| {
                    ToolError::CommandExec(format!("Failed creating reports directory: {e}"))
                })?;
                let out_path = dir.join(format!("replay_{}.html", session.name));
                let html = Self::render_replay_html(&session);
                tokio::fs::write(&out_path, html.as_bytes())
                    .await
                    .map_err(|e| {
                        ToolError::CommandExec(format!(
                            "Failed writing replay HTML scrubber '{}': {e}",
                            out_path.display()
                        ))
                    })?;
                let rel = out_path.strip_prefix(workspace_root).unwrap_or(&out_path);
                Ok(format!(
                    "🎬 Saved self-contained visual action replay scrubber ({} frame(s)) to `{}`.",
                    session.frames.len(),
                    rel.display()
                ))
            }
            other => Err(ToolError::InvalidArguments {
                name: "browser_interact".to_string(),
                reason: format!(
                    "Unknown action '{other}'. Supported: press_key, drag, check, uncheck, double_click, right_click, wait_for_text, go_back, go_forward, upload_file, configure_dialog, api_traffic, visual_diff, extract_tables, handoff_to_user, release_handoff, forge_skill_blueprint, raw_cdp, find, set_adblock, suspend_idle_tabs, media_diet, set_webgl, freeze_gui, thaw_gui, set_content, memory_status, trim_memory, record_start, record_stop, network_waterfall, storage_inspect"
                ),
            }
            .into()),
        }
    }

    /// Renders a self-contained dark-themed interactive HTML timeline scrubber (`agweb` visual replay pattern).
    fn render_replay_html(session: &ReplaySession) -> String {
        let frames_json = serde_json::to_string(&session.frames)
            .unwrap_or_else(|_| "[]".to_string())
            .replace("</", "<\\/");
        let safe_title: String = session
            .name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        format!(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1" />
<title>minibrowse Replay — {safe_title}</title>
<style>
  :root {{ color-scheme: dark; }}
  * {{ box-sizing: border-box; }}
  body {{ margin: 0; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; background: #090b10; color: #e5e7eb; display: flex; flex-direction: column; min-height: 100vh; }}
  header {{ padding: 12px 20px; background: #111520; border-bottom: 1px solid #1f2937; display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; }}
  .badge {{ background: #1e293b; color: #38bdf8; padding: 3px 8px; border-radius: 6px; font-size: 12px; }}
  main {{ flex: 1; display: grid; grid-template-columns: 280px 1fr; gap: 0; min-height: 0; }}
  aside {{ background: #0d111a; border-right: 1px solid #1f2937; overflow-y: auto; padding: 10px; }}
  .step-btn {{ width: 100%; text-align: left; background: transparent; color: #9ca3af; border: 1px solid transparent; padding: 8px 10px; border-radius: 6px; cursor: pointer; font: inherit; font-size: 12px; margin-bottom: 4px; }}
  .step-btn.active {{ background: #172554; color: #eff6ff; border-color: #3b82f6; }}
  section {{ display: flex; flex-direction: column; padding: 16px; gap: 12px; align-items: center; justify-content: center; }}
  .controls {{ width: 100%; max-width: 1100px; display: flex; align-items: center; gap: 12px; background: #111520; padding: 10px 14px; border-radius: 8px; border: 1px solid #1f2937; }}
  .controls button {{ background: #2563eb; color: #fff; border: none; border-radius: 6px; padding: 6px 12px; cursor: pointer; font: inherit; font-size: 12px; }}
  .controls input[type="range"] {{ flex: 1; }}
  .meta {{ font-size: 12px; color: #94a3b8; width: 100%; max-width: 1100px; display: flex; justify-content: space-between; gap: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
  .frame-wrap {{ max-width: 1100px; width: 100%; border: 1px solid #1f2937; border-radius: 8px; overflow: hidden; background: #05070b; display: flex; justify-content: center; }}
  .frame-wrap img {{ max-width: 100%; height: auto; display: block; }}
</style>
</head>
<body>
<header>
  <div><strong>minibrowse Visual Action Replay</strong> &middot; <span class="badge">{safe_title}</span></div>
  <div style="font-size:12px;color:#9ca3af">Started: {started_at} &middot; Use &larr; / &rarr; keys or slider</div>
</header>
<main>
  <aside id="steps"></aside>
  <section>
    <div class="controls">
      <button id="playBtn" type="button">Play</button>
      <input id="scrubber" type="range" min="0" max="0" value="0" />
      <span id="counter" style="font-size:12px">0 / 0</span>
    </div>
    <div class="meta">
      <span id="actLabel">-</span>
      <span id="urlLabel">-</span>
    </div>
    <div class="frame-wrap"><img id="viewportImg" alt="Replay frame" /></div>
  </section>
</main>
<script>
const FRAMES = {frames_json};
const stepsEl = document.getElementById('steps');
const scrubber = document.getElementById('scrubber');
const counter = document.getElementById('counter');
const actLabel = document.getElementById('actLabel');
const urlLabel = document.getElementById('urlLabel');
const img = document.getElementById('viewportImg');
const playBtn = document.getElementById('playBtn');
let idx = 0, timer = null;
function render(i) {{
  if (!FRAMES.length) return;
  idx = Math.max(0, Math.min(FRAMES.length - 1, i));
  const f = FRAMES[idx];
  scrubber.value = String(idx);
  counter.textContent = (idx + 1) + ' / ' + FRAMES.length + ' (' + f.timestamp + ')';
  actLabel.textContent = '#' + f.step + ' ' + f.action;
  urlLabel.textContent = f.url;
  img.src = 'data:image/png;base64,' + f.png_base64;
  Array.from(stepsEl.children).forEach((b, bi) => b.classList.toggle('active', bi === idx));
}}
FRAMES.forEach((f, i) => {{
  const b = document.createElement('button');
  b.className = 'step-btn';
  b.textContent = '#' + f.step + '  ' + f.action;
  b.onclick = () => render(i);
  stepsEl.appendChild(b);
}});
scrubber.max = String(Math.max(0, FRAMES.length - 1));
scrubber.oninput = () => render(Number(scrubber.value));
playBtn.onclick = () => {{
  if (timer) {{ clearInterval(timer); timer = null; playBtn.textContent = 'Play'; }}
  else {{
    playBtn.textContent = 'Pause';
    timer = setInterval(() => {{
      if (idx + 1 >= FRAMES.length) {{ clearInterval(timer); timer = null; playBtn.textContent = 'Play'; }}
      else render(idx + 1);
    }}, 900);
  }}
}};
window.addEventListener('keydown', (e) => {{
  if (e.key === 'ArrowLeft') render(idx - 1);
  if (e.key === 'ArrowRight') render(idx + 1);
}});
render(0);
</script>
</body>
</html>"#,
            started_at = session.started_at
        )
    }

    /// Runs a comprehensive in-page QA audit on the given URL or active page
    pub async fn run_qa_audit(
        url_opt: Option<&str>,
        _mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let (engine, _) = Self::prepare_debug_engine(url_opt, None, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;
        let current_url = cdp
            .evaluate_js("window.location.href")
            .await
            .unwrap_or_else(|_| url_opt.unwrap_or("http://localhost").to_string());

        let report = PageAgent::run_qa_audit(&cdp, &current_url).await?;
        Ok(PageAgent::format_qa_report(&report))
    }

    /// Inspects in-page DOM elements including Shadow DOM and computed geometry
    pub async fn inspect_visual_dom(_mode: BrowserMode, workspace_root: &Path) -> Result<String> {
        let (engine, _) = Self::prepare_debug_engine(None, None, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;
        let elements = PageAgent::scan_visual_tree(&cdp).await?;
        Ok(PageAgent::format_visual_tree_report(&elements))
    }

    /// Configures device viewport emulation and/or network throttling over CDP
    pub async fn emulate_device_and_network(
        viewport: Option<&str>,
        network: Option<&str>,
        custom_width: Option<u32>,
        custom_height: Option<u32>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;
        let mut results = Vec::new();

        // 1. Viewport / Device Emulation
        if let Some((clear_first, device_override, msg)) =
            Self::resolve_viewport_emulation(viewport, custom_width, custom_height)?
        {
            if clear_first {
                cdp.clear_device_metrics().await?;
            }
            if let Some((w, h, mobile, scale)) = device_override {
                cdp.emulate_device(w, h, mobile, scale).await?;
            }
            let verified_vp = cdp
                .evaluate_js("window.innerWidth + 'x' + window.innerHeight")
                .await
                .ok()
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty());
            if let Some(vp) = verified_vp {
                results.push(format!("{msg} [Verified live viewport: {vp}px]"));
            } else {
                results.push(msg);
            }
        }

        // 2. Network Throttling Emulation
        if let Some(net) = network {
            let lower = net.to_ascii_lowercase();
            type NetworkPreset = (&'static [&'static str], bool, f64, f64, f64, &'static str);
            const NETWORK_PRESETS: &[NetworkPreset] = &[
                (
                    &["reset", "clear", "none", "online"],
                    false,
                    0.0,
                    -1.0,
                    -1.0,
                    "Network throttling cleared (unthrottled online)",
                ),
                (
                    &["offline"],
                    true,
                    0.0,
                    0.0,
                    0.0,
                    "Network set to 'offline'",
                ),
                (
                    &["slow_3g"],
                    false,
                    400.0,
                    50_000.0,
                    50_000.0,
                    "Network throttled to 'slow_3g' (latency: 400ms, throughput: 50 KB/s)",
                ),
                (
                    &["fast_3g"],
                    false,
                    150.0,
                    180_000.0,
                    84_000.0,
                    "Network throttled to 'fast_3g' (latency: 150ms, throughput: 180 KB/s)",
                ),
                (
                    &["cable", "wifi"],
                    false,
                    20.0,
                    5_000_000.0,
                    1_000_000.0,
                    "Network set to 'cable / wifi' (latency: 20ms, throughput: 5 MB/s)",
                ),
            ];
            if let Some((_, offline, latency, down, up, msg)) = NETWORK_PRESETS
                .iter()
                .find(|(aliases, ..)| aliases.contains(&lower.as_str()))
            {
                cdp.emulate_network_conditions(*offline, *latency, *down, *up)
                    .await?;
                results.push((*msg).to_string());
            } else {
                return Err(ToolError::InvalidArguments {
                    name: "browser_emulate".to_string(),
                    reason: format!(
                        "Unknown network preset '{}'. Supported: offline, slow_3g, fast_3g, cable, wifi, reset",
                        lower
                    ),
                }
                .into());
            }
        }

        if results.is_empty() {
            Ok("ℹ No emulation parameters specified. Provide 'viewport', 'network', or custom width/height.".to_string())
        } else {
            Ok(format!(
                "📱 Emulation settings applied:\n{}",
                results
                    .iter()
                    .map(|r| format!("• {}", r))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        }
    }

    /// Resolves viewport preset and custom width/height overrides into `(clear_first, Option<(w, h, mobile, scale)>, status_message)`.
    #[allow(clippy::type_complexity)]
    pub(crate) fn resolve_viewport_emulation(
        viewport: Option<&str>,
        custom_width: Option<u32>,
        custom_height: Option<u32>,
    ) -> Result<Option<(bool, Option<(u32, u32, bool, f64)>, String)>> {
        type ViewportPreset = (&'static [&'static str], u32, u32, bool, f64, &'static str);
        const VIEWPORT_PRESETS: &[ViewportPreset] = &[
            (
                &["mobile", "390"],
                390,
                844,
                true,
                2.0,
                "Emulating 'mobile' viewport (390x844, scale: 2.0, mobile: true)",
            ),
            (
                &["mobile_small", "iphone_se", "375"],
                375,
                667,
                true,
                2.0,
                "Emulating 'mobile_small' viewport (375x667, scale: 2.0, mobile: true)",
            ),
            (
                &["mobile_large", "iphone", "414"],
                414,
                896,
                true,
                3.0,
                "Emulating 'iphone / mobile_large' viewport (414x896, scale: 3.0, mobile: true)",
            ),
            (
                &["tablet", "ipad", "768"],
                768,
                1024,
                true,
                2.0,
                "Emulating 'tablet / ipad' viewport (768x1024, scale: 2.0, mobile: true)",
            ),
            (
                &["laptop", "desktop_compact", "1280"],
                1280,
                800,
                false,
                1.0,
                "Emulating 'laptop' viewport (1280x800, scale: 1.0, mobile: false)",
            ),
            (
                &["desktop", "1440"],
                1440,
                900,
                false,
                1.0,
                "Emulating 'desktop' viewport (1440x900, scale: 1.0, mobile: false)",
            ),
            (
                &["desktop_wide", "1920"],
                1920,
                1080,
                false,
                1.0,
                "Emulating 'desktop_wide' viewport (1920x1080, scale: 1.0, mobile: false)",
            ),
        ];

        if let Some(vp) = viewport {
            let lower = vp.trim().to_ascii_lowercase();
            if matches!(lower.as_str(), "reset" | "clear" | "none") {
                if let (Some(w), Some(h)) = (custom_width, custom_height) {
                    return Ok(Some((
                        true,
                        Some((w, h, false, 1.0)),
                        format!(
                            "Cleared device viewport override and set custom viewport to {}x{} (scale: 1.0)",
                            w, h
                        ),
                    )));
                } else {
                    return Ok(Some((
                        true,
                        None,
                        "Cleared device viewport override (reset to default window size)"
                            .to_string(),
                    )));
                }
            } else if lower == "custom" {
                if let (Some(w), Some(h)) = (custom_width, custom_height) {
                    return Ok(Some((
                        false,
                        Some((w, h, false, 1.0)),
                        format!("Custom viewport set to {}x{} (scale: 1.0)", w, h),
                    )));
                }
            } else if let Some((w_str, h_str)) = lower.split_once('x') {
                if let (Ok(w), Ok(h)) = (w_str.trim().parse::<u32>(), h_str.trim().parse::<u32>()) {
                    let eff_w = custom_width.unwrap_or(w);
                    let eff_h = custom_height.unwrap_or(h);
                    let is_mobile = eff_w <= 768;
                    let scale = if is_mobile { 2.0 } else { 1.0 };
                    return Ok(Some((
                        false,
                        Some((eff_w, eff_h, is_mobile, scale)),
                        format!(
                            "Custom viewport set to {}x{} (scale: {:.1}, mobile: {})",
                            eff_w, eff_h, scale, is_mobile
                        ),
                    )));
                }
            }

            if let Some((_, preset_w, preset_h, mobile, scale, msg)) = VIEWPORT_PRESETS
                .iter()
                .find(|(aliases, ..)| aliases.contains(&lower.as_str()))
            {
                let eff_w = custom_width.unwrap_or(*preset_w);
                let eff_h = custom_height.unwrap_or(*preset_h);
                let resolved_msg = if eff_w != *preset_w || eff_h != *preset_h {
                    format!(
                        "Emulating '{}' viewport with custom dimensions ({}x{}, scale: {:.1}, mobile: {})",
                        lower, eff_w, eff_h, scale, mobile
                    )
                } else {
                    (*msg).to_string()
                };
                return Ok(Some((
                    false,
                    Some((eff_w, eff_h, *mobile, *scale)),
                    resolved_msg,
                )));
            }

            return Err(ToolError::InvalidArguments {
                name: "browser_emulate".to_string(),
                reason: format!(
                    "Unknown viewport preset '{}'. Supported: mobile, mobile_large, iphone, tablet, ipad, desktop, desktop_wide, custom, WxH (e.g. 390x844), reset",
                    lower
                ),
            }
            .into());
        } else if let (Some(w), Some(h)) = (custom_width, custom_height) {
            return Ok(Some((
                false,
                Some((w, h, false, 1.0)),
                format!("Custom viewport set to {}x{} (scale: 1.0)", w, h),
            )));
        }

        Ok(None)
    }

    fn sanitize_profile_name(profile_name: &str) -> Result<String> {
        let safe_name: String = profile_name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if safe_name.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "browser_state".to_string(),
                reason: "Profile name must contain valid alphanumeric characters".to_string(),
            }
            .into());
        }
        Ok(safe_name)
    }

    /// Saves the current browser session state (cookies and localStorage) to a named profile in the workspace
    pub async fn save_session_state(
        profile_name: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        Self::save_session_state_with_meta(profile_name, None, mode, workspace_root).await
    }

    /// Saves the current browser session state with optional semantic description (`browser-act` memory pattern)
    pub async fn save_session_state_with_meta(
        profile_name: &str,
        description: Option<&str>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let safe_name = Self::sanitize_profile_name(profile_name)?;

        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;

        let current_url = cdp
            .evaluate_js("window.location.href")
            .await
            .unwrap_or_else(|_| "http://localhost".to_string());
        let cookies = cdp
            .get_cookies()
            .await
            .unwrap_or_else(|_| serde_json::json!([]));
        let ls_raw = cdp
            .evaluate_js(
                "(() => { try { const o = {}; for (let i = 0; i < localStorage.length; i++) { const k = localStorage.key(i); o[k] = localStorage.getItem(k); } return JSON.stringify(o); } catch(e) { return '{}'; } })()",
            )
            .await
            .unwrap_or_else(|_| "{}".to_string());
        let local_storage: serde_json::Value =
            serde_json::from_str(&ls_raw).unwrap_or_else(|_| serde_json::json!({}));

        let state_dir = workspace_root.join(BROWSER_STATE_DIR);
        tokio::fs::create_dir_all(&state_dir).await.map_err(|e| {
            ToolError::CommandExec(format!("Failed to create browser state directory: {}", e))
        })?;

        let file_path = state_dir.join(format!("{}.json", safe_name));
        let desc_str = description.unwrap_or("").trim();
        let state = serde_json::json!({
            "profile": safe_name,
            "description": desc_str,
            "url": current_url,
            "cookies": cookies,
            "local_storage": local_storage,
            "saved_at": chrono::Utc::now().to_rfc3339()
        });

        let encrypted_envelope = guard::encrypt_session_state_value(&state, &safe_name)?;
        let json_str = serde_json::to_string_pretty(&encrypted_envelope).map_err(|e| {
            ToolError::CommandExec(format!("Failed serializing browser session state: {}", e))
        })?;
        tokio::fs::write(&file_path, json_str).await.map_err(|e| {
            ToolError::CommandExec(format!(
                "Failed writing browser session state to '{}': {}",
                file_path.display(),
                e
            ))
        })?;

        let cookies_count = cookies.as_array().map(|a| a.len()).unwrap_or(0);
        let ls_count = local_storage.as_object().map(|o| o.len()).unwrap_or(0);

        let rel_path = file_path.strip_prefix(workspace_root).unwrap_or(&file_path);
        let desc_line = if desc_str.is_empty() {
            String::new()
        } else {
            format!("\n• Description: {desc_str}")
        };
        Ok(format!(
            "💾 Browser session state successfully saved (AEAD encrypted):\n• Profile: `{}`\n• File: `{}`{}\n• Cookies: {}\n• LocalStorage keys: {}\n• Active URL: `{}`",
            safe_name,
            rel_path.display(),
            desc_line,
            cookies_count,
            ls_count,
            current_url
        ))
    }

    /// Restores a previously saved browser session profile (cookies and localStorage) into the active Space/session
    pub async fn restore_session_state(
        profile_name: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let safe_name = Self::sanitize_profile_name(profile_name)?;

        let file_path = workspace_root
            .join(BROWSER_STATE_DIR)
            .join(format!("{}.json", safe_name));
        if !file_path.exists() {
            return Err(ToolError::InvalidArguments {
                name: "browser_state".to_string(),
                reason: format!(
                    "Browser session profile '{}' does not exist at '{}'",
                    safe_name,
                    file_path.display()
                ),
            }
            .into());
        }

        let content = tokio::fs::read_to_string(&file_path).await.map_err(|e| {
            ToolError::CommandExec(format!("Failed reading session state file: {}", e))
        })?;
        let raw_stored: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| ToolError::CommandExec(format!("Corrupt session state JSON: {}", e)))?;
        let state = guard::decrypt_session_state_value(&raw_stored)?;

        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let (cdp, _, _) = engine.active_session().await?;
        let mut applied = Vec::new();

        // Restore cookies
        if let Some(cookies) = state.get("cookies") {
            if cookies.is_array() && !cookies.as_array().map(|a| a.is_empty()).unwrap_or(true) {
                cdp.set_cookies(cookies.clone()).await?;
                applied.push(format!(
                    "Applied {} cookie(s)",
                    cookies.as_array().map(|a| a.len()).unwrap_or(0)
                ));
            }
        }

        // Navigate to saved URL if present
        if let Some(url_str) = state.get("url").and_then(|u| u.as_str()) {
            if !url_str.is_empty()
                && (url_str.starts_with("http://")
                    || url_str.starts_with("https://")
                    || url_str.starts_with("file://"))
            {
                let _ = cdp.navigate(url_str).await;
                applied.push(format!("Navigated to `{}`", url_str));
            }
        }

        // Restore localStorage
        if let Some(ls) = state.get("local_storage").and_then(|l| l.as_object()) {
            if !ls.is_empty() {
                for (k, v) in ls {
                    let v_str = v.as_str().unwrap_or("");
                    let k_escaped = serde_json::to_string(k).unwrap_or_default();
                    let v_escaped = serde_json::to_string(v_str).unwrap_or_default();
                    let script = format!(
                        "try {{ localStorage.setItem({}, {}); }} catch(e) {{}}",
                        k_escaped, v_escaped
                    );
                    let _ = cdp.evaluate_js(&script).await;
                }
                applied.push(format!("Restored {} localStorage item(s)", ls.len()));
            }
        }

        Ok(format!(
            "🔄 Browser session profile `{}` restored successfully:\n• {}",
            safe_name,
            if applied.is_empty() {
                "No state entries found in profile".to_string()
            } else {
                applied.join("\n• ")
            }
        ))
    }

    /// Unified semantic session memory manager (`save`, `restore`, `list`, `describe`, `import_cookies`, `delete`).
    pub async fn manage_session_state(
        action: &str,
        profile_opt: Option<&str>,
        description: Option<&str>,
        cookie_file: Option<&str>,
        url_hint: Option<&str>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let norm = action.trim().to_ascii_lowercase();
        match norm.as_str() {
            "save" => {
                let prof = profile_opt.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_state".to_string(),
                    reason: "Action 'save' requires 'profile'".to_string(),
                })?;
                Self::save_session_state_with_meta(prof, description, mode, workspace_root).await
            }
            "restore" | "load" => {
                let prof = profile_opt.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_state".to_string(),
                    reason: "Action 'restore' requires 'profile'".to_string(),
                })?;
                Self::restore_session_state(prof, mode, workspace_root).await
            }
            "list" => {
                let state_dir = workspace_root.join(BROWSER_STATE_DIR);
                if !state_dir.exists() {
                    return Ok(
                        "No saved browser session profiles found in `.minicode/browser_state/`."
                            .to_string(),
                    );
                }
                let mut entries = Vec::new();
                if let Ok(rd) = std::fs::read_dir(&state_dir) {
                    for entry in rd.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("json") {
                            continue;
                        }
                        let Ok(raw) = std::fs::read_to_string(&path) else {
                            continue;
                        };
                        let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
                            continue;
                        };
                        let profile = val
                            .get("profile")
                            .and_then(|v| v.as_str())
                            .or_else(|| path.file_stem().and_then(|s| s.to_str()))
                            .unwrap_or("unknown")
                            .to_string();
                        let saved_at = val
                            .get("saved_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let meta = val.get("meta");
                        let desc = meta
                            .and_then(|m| m.get("description"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let domain = meta
                            .and_then(|m| m.get("domain"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let cookies = meta
                            .and_then(|m| m.get("cookie_count"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        entries.push((profile, domain, desc, cookies, saved_at));
                    }
                }
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                if entries.is_empty() {
                    return Ok(
                        "No saved browser session profiles found in `.minicode/browser_state/`."
                            .to_string(),
                    );
                }
                let mut out = format!(
                    "### 🗂️ Saved Browser Session Profiles ({}, Secrets Encrypted)\n\n",
                    entries.len()
                );
                for (prof, domain, desc, cookies, saved_at) in entries {
                    let dom_part = if domain.is_empty() {
                        String::new()
                    } else {
                        format!(" (`{domain}`)")
                    };
                    let desc_part = if desc.is_empty() {
                        String::new()
                    } else {
                        format!(" — {desc}")
                    };
                    out.push_str(&format!(
                        "- **`{prof}`**{dom_part}: {cookies} cookie(s), saved `{saved_at}`{desc_part}\n"
                    ));
                }
                Ok(out)
            }
            "describe" | "update_meta" => {
                let prof = profile_opt.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_state".to_string(),
                    reason: "Action 'describe' requires 'profile'".to_string(),
                })?;
                let safe_name = Self::sanitize_profile_name(prof)?;
                let desc = description.unwrap_or("").trim();
                let file_path = workspace_root
                    .join(BROWSER_STATE_DIR)
                    .join(format!("{safe_name}.json"));
                if !file_path.exists() {
                    return Err(ToolError::InvalidArguments {
                        name: "browser_state".to_string(),
                        reason: format!("Profile '{safe_name}' does not exist"),
                    }
                    .into());
                }
                let content = tokio::fs::read_to_string(&file_path)
                    .await
                    .map_err(|e| ToolError::CommandExec(format!("Read error: {e}")))?;
                let raw_stored: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| ToolError::CommandExec(format!("Corrupt JSON: {e}")))?;
                let mut plain = guard::decrypt_session_state_value(&raw_stored)?;
                if let Some(obj) = plain.as_object_mut() {
                    obj.insert("description".to_string(), serde_json::json!(desc));
                }
                let reencrypted = guard::encrypt_session_state_value(&plain, &safe_name)?;
                let pretty = serde_json::to_string_pretty(&reencrypted)
                    .map_err(|e| ToolError::CommandExec(format!("Serialize error: {e}")))?;
                tokio::fs::write(&file_path, pretty)
                    .await
                    .map_err(|e| ToolError::CommandExec(format!("Write error: {e}")))?;
                Ok(format!(
                    "Updated semantic description for browser profile `{safe_name}`: \"{desc}\""
                ))
            }
            "import_cookies" | "import" => {
                let prof = profile_opt.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_state".to_string(),
                    reason: "Action 'import_cookies' requires 'profile'".to_string(),
                })?;
                let safe_name = Self::sanitize_profile_name(prof)?;
                let file_arg = cookie_file.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_state".to_string(),
                    reason:
                        "Action 'import_cookies' requires 'cookie_file' (Netscape cookies.txt or JSON path inside workspace)"
                            .to_string(),
                })?;
                let validated_path = crate::sandbox::path::validate_path_in_workspace(
                    workspace_root,
                    Path::new(file_arg),
                )?;
                let raw_cookies =
                    tokio::fs::read_to_string(&validated_path)
                        .await
                        .map_err(|e| {
                            ToolError::CommandExec(format!(
                                "Failed reading cookie export '{}': {e}",
                                validated_path.display()
                            ))
                        })?;
                let cookies = guard::parse_imported_cookies(&raw_cookies)?;
                if cookies.is_empty() {
                    return Err(ToolError::InvalidArguments {
                        name: "browser_state".to_string(),
                        reason: "No valid cookies found in provided cookie_file".to_string(),
                    }
                    .into());
                }
                let inferred_url = url_hint.map(str::to_string).unwrap_or_else(|| {
                    let dom = cookies
                        .first()
                        .and_then(|c| c.get("domain"))
                        .and_then(|d| d.as_str())
                        .unwrap_or("localhost")
                        .trim_start_matches('.');
                    format!("https://{dom}")
                });
                let state_dir = workspace_root.join(BROWSER_STATE_DIR);
                tokio::fs::create_dir_all(&state_dir)
                    .await
                    .map_err(|e| ToolError::CommandExec(format!("Failed creating state dir: {e}")))?;
                let state = serde_json::json!({
                    "profile": safe_name,
                    "description": description.unwrap_or("Imported browser cookies").trim(),
                    "url": inferred_url,
                    "cookies": cookies,
                    "local_storage": {},
                    "saved_at": chrono::Utc::now().to_rfc3339(),
                });
                let encrypted = guard::encrypt_session_state_value(&state, &safe_name)?;
                let out_path = state_dir.join(format!("{safe_name}.json"));
                let pretty = serde_json::to_string_pretty(&encrypted)
                    .map_err(|e| ToolError::CommandExec(format!("Serialize error: {e}")))?;
                tokio::fs::write(&out_path, pretty)
                    .await
                    .map_err(|e| ToolError::CommandExec(format!("Write error: {e}")))?;

                // Also apply live if an engine is running or can be launched
                if let Ok(engine) = BrowserManager::get_or_launch(mode, workspace_root).await {
                    if let Ok((cdp, _, _)) = engine.active_session().await {
                        let _ = cdp.set_cookies(serde_json::json!(cookies)).await;
                    }
                }
                Ok(format!(
                    "📥 Imported {} cookie(s) into AEAD-encrypted profile `{safe_name}` (`{}`).",
                    cookies.len(),
                    inferred_url
                ))
            }
            "delete" | "remove" => {
                let prof = profile_opt.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_state".to_string(),
                    reason: "Action 'delete' requires 'profile'".to_string(),
                })?;
                let safe_name = Self::sanitize_profile_name(prof)?;
                let file_path = workspace_root
                    .join(BROWSER_STATE_DIR)
                    .join(format!("{safe_name}.json"));
                if file_path.exists() {
                    tokio::fs::remove_file(&file_path)
                        .await
                        .map_err(|e| ToolError::CommandExec(format!("Delete failed: {e}")))?;
                    Ok(format!("Deleted browser session profile `{safe_name}`."))
                } else {
                    Ok(format!("Browser session profile `{safe_name}` did not exist."))
                }
            }
            other => Err(ToolError::InvalidArguments {
                name: "browser_state".to_string(),
                reason: format!(
                    "Unknown browser_state action '{other}'. Supported: save, restore, list, describe, import_cookies, delete"
                ),
            }
            .into()),
        }
    }

    /// Synthesizes a permanent, secret-redacted `MiniVault` `SKILL.md` package from live `MiniBrowse`
    /// API traffic (`api_traffic`) and structured table schemas (`browser-act` Skill Forge pattern).
    pub fn synthesize_forged_skill_markdown(
        raw_name: &str,
        page_url: &str,
        page_title: &str,
        traffic: &serde_json::Value,
        tables: &serde_json::Value,
    ) -> (String, String) {
        let mut slug = String::new();
        for ch in raw_name.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
            } else if (ch == '-' || ch == '_' || ch == ' ')
                && !slug.ends_with('-')
                && !slug.is_empty()
            {
                slug.push('-');
            }
        }
        let slug = slug.trim_matches('-').to_string();
        let slug = if slug.is_empty() {
            "forged-web-workflow".to_string()
        } else {
            slug
        };

        let host = url::Url::parse(page_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "target-site".to_string());

        let mut seen_templates = std::collections::BTreeSet::new();
        let mut endpoint_lines = Vec::new();
        if let Some(entries) = traffic.get("entries").and_then(|v| v.as_array()) {
            for entry in entries {
                let method = entry
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("GET");
                let raw_u = entry.get("url").and_then(|v| v.as_str()).unwrap_or("");
                if raw_u.is_empty() {
                    continue;
                }
                let template = normalize_url_template(raw_u);
                let dedup_key = format!("{method} {template}");
                if !seen_templates.insert(dedup_key) {
                    continue;
                }
                let status = entry.get("status").and_then(|v| v.as_u64()).unwrap_or(200);
                let resp_preview = entry
                    .get("response_body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .chars()
                    .take(140)
                    .collect::<String>();
                endpoint_lines.push(format!(
                    "- `{method} {template}` (`HTTP {status}`)\n  - Sample shape (redacted): `{}`",
                    if resp_preview.is_empty() {
                        "{}".to_string()
                    } else {
                        resp_preview.replace('`', "'")
                    }
                ));
            }
        }

        let mut table_lines = Vec::new();
        if let Some(tbl_arr) = tables.get("tables").and_then(|v| v.as_array()) {
            for tbl in tbl_arr.iter().take(5) {
                let sel = tbl
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("table");
                let cols: Vec<&str> = tbl
                    .get("headers")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                table_lines.push(format!("- `{sel}` — columns: `[{}]`", cols.join(", ")));
            }
        }

        let clean_title = page_title.replace('"', "'").trim().to_string();
        let api_section = if endpoint_lines.is_empty() {
            "- No XHR/fetch JSON endpoints captured on initial load; prefer `browser_interact(action=\"extract_tables\")` or `browser_eval` (`await mb.paginate(...)`).".to_string()
        } else {
            endpoint_lines.join("\n")
        };
        let table_section = if table_lines.is_empty() {
            "- Use `browser_eval` with `await mb.snapshot()` / `mb.extractLinks()` for item extraction.".to_string()
        } else {
            table_lines.join("\n")
        };

        let md = format!(
            "---\n\
name: \"{slug}\"\n\
description: \"Forged API-first & MiniBrowse automation workflow for {host} ({clean_title}).\"\n\
category: \"browser\"\n\
kind: \"workflow\"\n\
triggers: [\"{slug}\", \"{host}\"]\n\
allowed_tools: [\"webfetch\", \"browser_navigate\", \"browser_eval\", \"browser_interact\", \"browser_state\"]\n\
always_apply: false\n\
---\n\n\
# {slug} — Forged Web & API Skill (`{host}`)\n\n\
## Target Entrypoint\n\
- **Origin / Seed URL**: `{page_url}`\n\
- **Title**: {clean_title}\n\n\
## 1. Discovered Internal API Endpoints (API-First Fast Path)\n\
{api_section}\n\n\
## 2. Structured DOM / Table Schemas (Fallback Path)\n\
{table_section}\n\n\
## 3. Execution Protocol\n\
1. **Isolated Space**: Pass `space: \"{slug}\"` on browser calls so execution runs in an isolated background `BrowserContext` without disturbing user tabs.\n\
2. **Session Reuse**: Check `browser_state(action=\"list\")` for an existing `{host}` profile and `restore` it before navigating; if redirected to login/2FA, use `browser_interact(action=\"handoff_to_user\")` (never guess or bypass verification challenges) and pass secrets via `$ENV:VAR_NAME`.\n\
3. **Single-Pass Extraction**: Prefer calling discovered JSON endpoints directly or running `browser_eval(script=\"return await mb.extractTables()\", space=\"{slug}\")` in one turn.\n"
        );
        (slug, md)
    }

    /// Exports the active page or target document to PDF bytes and saves to workspace
    pub async fn export_pdf(
        custom_path: Option<&str>,
        landscape: bool,
        print_background: bool,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let pdf_bytes = engine.cdp.print_to_pdf(landscape, print_background).await?;

        let target_path = if let Some(p) = custom_path {
            crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(p))?
        } else {
            let dir = workspace_root.join(BROWSER_REPORTS_DIR);
            tokio::fs::create_dir_all(&dir).await.map_err(|e| {
                ToolError::CommandExec(format!("Failed to create reports directory: {}", e))
            })?;
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            dir.join(format!("page_export_{}.pdf", timestamp))
        };

        if let Some(parent) = target_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        tokio::fs::write(&target_path, &pdf_bytes)
            .await
            .map_err(|e| {
                ToolError::CommandExec(format!(
                    "Failed saving PDF to '{}': {}",
                    target_path.display(),
                    e
                ))
            })?;

        let rel_path = target_path
            .strip_prefix(workspace_root)
            .unwrap_or(&target_path);

        Ok(format!(
            "📄 PDF document generated successfully:\n• File: `{}`\n• Size: {} bytes\n• Landscape: {}\n• Print Background: {}",
            rel_path.display(),
            pdf_bytes.len(),
            landscape,
            print_background
        ))
    }

    /// Audits DOM elements for hidden text, CSS hiding techniques, and prompt injection signatures
    pub async fn check_prompt_injections(
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let script = r#"
        (() => {
            const findings = [];
            const injectionPatterns = [
                'ignore previous',
                'ignore all',
                'disregard previous',
                'system prompt',
                'you are an ai',
                'you are chatgpt',
                'you must output',
                'jailbreak',
                'repeat the following',
                'developer mode',
                'system override',
                'drop table',
                'transfer money'
            ];

            const all = document.querySelectorAll('*');
            for (let el of all) {
                const tag = el.tagName.toLowerCase();
                if (['script', 'style', 'meta', 'head', 'noscript', 'link', 'title'].includes(tag)) continue;
                if (el.closest && el.closest('[data-minicode-ignore="true"], #minicode-simulator-container')) continue;

                const text = (el.textContent || '').trim();
                if (!text) continue;

                let isHidden = false;
                let reasons = [];

                const style = window.getComputedStyle(el);
                if (style.display === 'none') { isHidden = true; reasons.push('display: none'); }
                if (style.visibility === 'hidden') { isHidden = true; reasons.push('visibility: hidden'); }
                if (style.opacity === '0') { isHidden = true; reasons.push('opacity: 0'); }
                if (style.fontSize === '0px' || style.fontSize === '0') { isHidden = true; reasons.push('font-size: 0'); }

                const rect = el.getBoundingClientRect();
                if (rect.left < -500 || rect.top < -500 || rect.left > window.innerWidth + 500) {
                    isHidden = true; reasons.push('positioned off-screen (' + Math.round(rect.left) + ',' + Math.round(rect.top) + ')');
                }

                const lowerText = text.toLowerCase();
                const matched = injectionPatterns.filter(p => lowerText.includes(p));

                if (matched.length > 0 || (isHidden && text.length > 15)) {
                    findings.push({
                        tag: tag,
                        id: el.id || '',
                        className: (typeof el.className === 'string') ? el.className : '',
                        hidden: isHidden,
                        reasons: reasons,
                        matched: matched,
                        sample: text.length > 120 ? text.substring(0, 120) + '...' : text
                    });
                }
            }
            return JSON.stringify(findings);
        })()
        "#;

        let raw_json = engine.cdp.evaluate_js(script).await?;
        let findings: Vec<serde_json::Value> = serde_json::from_str(&raw_json).unwrap_or_default();

        if findings.is_empty() {
            return Ok("🛡 **DOM Security Audit Passed:** No prompt injection signatures or suspicious hidden DOM elements detected on current page.".to_string());
        }

        let mut injection_alerts = Vec::new();
        let mut hidden_elements = Vec::new();

        for item in &findings {
            let tag = item
                .get("tag")
                .and_then(|t| t.as_str())
                .unwrap_or("element");
            let id = item.get("id").and_then(|i| i.as_str()).unwrap_or("");
            let reasons = item
                .get("reasons")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let sample = item.get("sample").and_then(|s| s.as_str()).unwrap_or("");
            let matched = item
                .get("matched")
                .and_then(|m| m.as_array())
                .map(|arr| arr.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>())
                .unwrap_or_default();

            let id_str = if id.is_empty() {
                String::new()
            } else {
                format!(" id=\"{}\"", id)
            };

            if !matched.is_empty() {
                injection_alerts.push(format!(
                    "• **HIGH ALERT: Prompt Injection Trigger** in `<{}{}>` (triggers: {:?}, {})\n  > Text: \"{}\"",
                    tag, id_str, matched, if reasons.is_empty() { "visible".to_string() } else { reasons }, sample
                ));
            } else {
                hidden_elements.push(format!(
                    "• `<{}{}>` ({})\n  > Text: \"{}\"",
                    tag, id_str, reasons, sample
                ));
            }
        }

        let mut out = String::new();
        if !injection_alerts.is_empty() {
            out.push_str("🚨 **SECURITY WARNING: Potential Prompt Injection / Context Poisoning Detected!**\n\n");
            for alert in &injection_alerts {
                out.push_str(alert);
                out.push_str("\n\n");
            }
        }

        if !hidden_elements.is_empty() {
            out.push_str(&format!(
                "⚠️ **Hidden DOM Elements Detected ({} element(s)):**\n\n",
                hidden_elements.len()
            ));
            for el in hidden_elements.iter().take(10) {
                out.push_str(el);
                out.push_str("\n\n");
            }
            if hidden_elements.len() > 10 {
                out.push_str(&format!(
                    "*...and {} more hidden elements.*",
                    hidden_elements.len() - 10
                ));
            }
        }

        Ok(out)
    }

    /// Parses raw HTML into an accessible tree with numbered element references
    pub fn parse_html_to_aria_snapshot(url: &str, html: &str) -> PageSnapshot {
        let cleaned_html = accessibility::strip_ignored_minicode_elements(html);
        let elements = AccessibilityManager::extract_elements_for_revision(&cleaned_html, 1, 50);
        Self::build_aria_snapshot(url, &cleaned_html, elements)
    }

    /// Builds a `PageSnapshot` from pre-extracted ARIA elements without re-parsing interactive elements from HTML.
    pub(crate) fn build_aria_snapshot(
        url: &str,
        html: &str,
        interactive_elements: Vec<AriaElement>,
    ) -> PageSnapshot {
        let title = extract_title(html).unwrap_or_else(|| url.to_string());
        let text_summary = extract_readable_text(html);

        PageSnapshot {
            url: url.to_string(),
            title,
            engine_used: "Parser".to_string(),
            viewport_summary: None,
            interactive_elements,
            text_summary,
        }
    }

    /// Formats a snapshot into a clean agent-readable Markdown accessibility report
    pub fn format_snapshot_report(snapshot: &PageSnapshot) -> String {
        let mut out = format!(
            "Page: **{}** (`{}`)\nEngine: _{}_\n",
            snapshot.title, snapshot.url, snapshot.engine_used
        );
        if let Some(ref vp) = snapshot.viewport_summary {
            out.push_str(&format!("{}\n", vp));
        }
        out.push('\n');

        if !snapshot.interactive_elements.is_empty() {
            out.push_str("Interactive Accessibility Tree (ARIA References):\n");
            for el in &snapshot.interactive_elements {
                let mut pairs: Vec<String> = el
                    .attributes
                    .iter()
                    // Omit internal Rust CSS selector hint from LLM output to save ~35% snapshot tokens
                    .filter(|(k, _)| k.as_str() != "selector")
                    .map(|(k, v)| format!("{}=\"{}\"", k, v))
                    .collect();
                pairs.sort();
                let attrs_str = if pairs.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", pairs.join(", "))
                };

                out.push_str(&format!(
                    "  • **{}** `<{}>` ({}) \"{}\"{}\n",
                    el.ref_id, el.tag, el.role, el.name, attrs_str
                ));
            }
            out.push('\n');
        }

        out.push_str("Content Text Summary:\n");
        let summary_preview = snapshot
            .text_summary
            .lines()
            .take(15)
            .map(|line| crate::utils::strings::truncate_display(line, 240))
            .collect::<Vec<_>>()
            .join("\n");
        out.push_str(&summary_preview);

        if snapshot.text_summary.lines().count() > 15 {
            out.push_str("\n\n_...[content truncated for token efficiency]..._");
        }

        guard::wrap_untrusted_web_output(&snapshot.url, &out)
    }
}

/// Validates browser URL with loopback/localhost exemption (for testing local development servers)
pub fn validate_browser_url(url_str: &str) -> Result<()> {
    if !url_str.starts_with("http://")
        && !url_str.starts_with("https://")
        && !url_str.starts_with("file://")
    {
        return Err(ToolError::InvalidArguments {
            name: "browser_navigate".to_string(),
            reason: "URL must begin with http://, https://, or file://".to_string(),
        }
        .into());
    }

    let parsed = url::Url::parse(url_str).map_err(|e| ToolError::InvalidArguments {
        name: "browser_navigate".to_string(),
        reason: format!("Invalid URL '{}': {}", url_str, e),
    })?;

    if let Some(host_str) = parsed.host_str() {
        let lower = host_str.to_lowercase();
        // Block cloud metadata services (e.g. AWS/GCP metadata endpoint 169.254.169.254)
        for blocked in BROWSER_BLOCKED_HOSTS {
            if lower == *blocked || lower.ends_with(&format!(".{}", blocked)) {
                return Err(SecurityError::SsrfBlocked {
                    url: url_str.to_string(),
                    reason: format!(
                        "Access to blocked metadata endpoint '{}' is forbidden",
                        host_str
                    ),
                }
                .into());
            }
        }
    }

    Ok(())
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let n_bytes = needle.as_bytes();
    if n_bytes.is_empty() {
        return Some(0);
    }
    haystack
        .as_bytes()
        .windows(n_bytes.len())
        .position(|w| w.eq_ignore_ascii_case(n_bytes))
}

fn extract_title(html: &str) -> Option<String> {
    let start_tag = "<title>";
    let end_tag = "</title>";

    let start_idx = find_ascii_case_insensitive(html, start_tag)?;
    let title_start = start_idx + start_tag.len();
    let end_idx = find_ascii_case_insensitive(&html[title_start..], end_tag)?;
    Some(html[title_start..title_start + end_idx].trim().to_string())
}

fn strip_non_text_elements(s: &str, extra_tags: &[&str]) -> String {
    accessibility::strip_non_text_elements(s, extra_tags)
}

/// Normalizes loopback URLs by probing which loopback stack (`127.0.0.1` IPv4 vs `localhost` IPv6)
/// is actively listening on the target port, defaulting to `127.0.0.1` when no socket is open yet.
pub fn normalize_loopback_url(url: &str) -> String {
    if let Ok(mut parsed) = url::Url::parse(url) {
        if matches!(
            parsed.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("0.0.0.0") | Some("::1")
        ) {
            let port = parsed.port().unwrap_or(80);
            let host =
                crate::dev::ports::resolve_listening_loopback_host(port).unwrap_or("127.0.0.1");
            let _ = parsed.set_host(Some(host));
            return parsed.to_string();
        }
    }
    url.to_string()
}

/// Reconciles requested loopback target URL with active dev servers in the registry.
/// If port 5173 was requested, but our managed dev server auto-shifted to port 5174,
/// this transparently updates the port so we hit our own dev server instead of
/// orphaned external processes or closed sockets.
pub async fn reconcile_loopback_url(url: &str) -> (String, Option<String>) {
    let normalized = normalize_loopback_url(url);
    let Ok(mut parsed) = url::Url::parse(&normalized) else {
        return (normalized, None);
    };

    let is_loopback = matches!(
        parsed.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("0.0.0.0") | Some("::1")
    );
    if !is_loopback {
        return (normalized, None);
    }

    let req_port = parsed.port().unwrap_or(80);
    let reg = crate::dev::registry::get_global_dev_registry();
    let procs = reg.list().await;

    // If req_port is already accepting loopback TCP connections (whether managed by MiniDevRegistry
    // or launched directly via a subshell / user terminal), preserve req_port and normalize host.
    if let Some(host) = crate::dev::ports::resolve_listening_loopback_host(req_port) {
        let _ = parsed.set_host(Some(host));
        return (parsed.to_string(), None);
    }

    // Check if req_port is associated with one of our active processes (even if still warming up)
    let matches_active = procs.iter().any(|p| p.ports.contains(&req_port));
    if matches_active {
        return (normalized, None);
    }

    // Look for an active frontend or backend dev server that has a listening port
    let dev_server = procs.iter().find(|p| {
        p.status.is_alive()
            && matches!(
                p.process_type,
                crate::dev::models::DevProcessType::Frontend
                    | crate::dev::models::DevProcessType::Backend
            )
            && !p.ports.is_empty()
    });

    if let Some(ds) = dev_server {
        // Find the first port of this dev server that is actively listening
        if let Some(&active_port) = ds
            .ports
            .iter()
            .find(|&&p| crate::dev::ports::is_port_listening(p))
        {
            if let Some(host) = crate::dev::ports::resolve_listening_loopback_host(active_port) {
                let _ = parsed.set_host(Some(host));
            }
            if active_port != req_port {
                let _ = parsed.set_port(Some(active_port));
                let new_url = parsed.to_string();
                let notice = format!(
                    "⚡ Port auto-reconciliation: rewritten target from port {} to active dev server port {} ({})",
                    req_port, active_port, ds.name
                );
                return (new_url, Some(notice));
            }
        }
    }

    (normalized, None)
}

/// Probes a loopback TCP port until it is accepting connections, with exponential backoff up to max_wait.
pub async fn wait_for_port_ready(port: u16, max_wait: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    let mut delay = std::time::Duration::from_millis(50);
    while start.elapsed() < max_wait {
        if crate::dev::ports::is_port_listening(port) {
            return true;
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(std::time::Duration::from_millis(500));
    }
    crate::dev::ports::is_port_listening(port)
}

fn extract_readable_text(html: &str) -> String {
    let without_ignored = accessibility::strip_ignored_minicode_elements(html);
    let body_scoped = strip_non_text_elements(&without_ignored, &["head"]);
    accessibility::strip_html_tags_preserving_blocks(&body_scoped)
}

/// Normalizes concrete numeric IDs and hex UUID segments in captured API URLs into `{id}`
/// and replaces sensitive query values (`key`, `token`, `secret`, `auth`) with `$ENV:...` placeholders.
fn normalize_url_template(raw_url: &str) -> String {
    let Ok(parsed) = url::Url::parse(raw_url) else {
        return raw_url.to_string();
    };
    let mut base = format!(
        "{}://{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or("localhost")
    );
    if let Some(p) = parsed.port() {
        base.push_str(&format!(":{p}"));
    }
    let segments: Vec<String> = parsed
        .path_segments()
        .map(|segs| {
            segs.map(|s| {
                let is_num = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
                let is_hex_id =
                    s.len() >= 16 && s.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
                if is_num || is_hex_id {
                    "{id}".to_string()
                } else {
                    s.to_string()
                }
            })
            .collect()
        })
        .unwrap_or_default();
    base.push('/');
    base.push_str(&segments.join("/"));

    let pairs: Vec<String> = parsed
        .query_pairs()
        .map(|(k, _v)| {
            let kl = k.to_ascii_lowercase();
            if kl.contains("key")
                || kl.contains("token")
                || kl.contains("secret")
                || kl.contains("auth")
            {
                let env_name: String = k
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() {
                            c.to_ascii_uppercase()
                        } else {
                            '_'
                        }
                    })
                    .collect();
                format!("{k}=$ENV:{env_name}")
            } else {
                format!("{k}={{{k}}}")
            }
        })
        .collect();
    if !pairs.is_empty() {
        base.push('?');
        base.push_str(&pairs.join("&"));
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_html_to_aria_snapshot() {
        let sample_html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Dashboard - Minicode</title>
                <script type="module">import { injectIntoGlobalHook } from "/@react-refresh";</script>
                <style>.hidden { display: none; }</style>
            </head>
            <body>
                <script>window.__SECRET_TOKEN = "do_not_include";</script>
                <h1>Welcome to Minicode</h1>
                <p>Fast AI coding assistant.</p>
                <form action="/login" method="post">
                    <input type="text" name="username" placeholder="Username" />
                    <input type="password" name="password" placeholder="Password" />
                    <button type="submit">Log In</button>
                </form>
                <a href="/docs">Documentation</a>
                <a href="https://github.com" aria-label="GitHub"><svg><path d="M0 0"/></svg></a>
            </body>
            </html>
        "#;

        let snapshot =
            BrowserController::parse_html_to_aria_snapshot("http://localhost:3000", sample_html);
        assert_eq!(snapshot.title, "Dashboard - Minicode");
        assert!(!snapshot.interactive_elements.is_empty());
        assert!(!snapshot.text_summary.contains("injectIntoGlobalHook"));
        assert!(!snapshot.text_summary.contains("__SECRET_TOKEN"));
        assert!(!snapshot.text_summary.contains("display: none"));
        assert!(snapshot.text_summary.contains("Welcome to Minicode"));

        let report = BrowserController::format_snapshot_report(&snapshot);
        assert!(report.contains("@v1:e1"));
        assert!(report.contains("Log In"));
        assert!(report.contains("Documentation"));
        assert!(report.contains("\"GitHub\""));
        assert!(!report.contains("Unnamed Link"));
    }

    #[test]
    fn test_validate_browser_url_permits_localhost() {
        assert!(validate_browser_url("http://localhost:3000").is_ok());
        assert!(validate_browser_url("http://127.0.0.1:8080/app").is_ok());
        assert!(validate_browser_url("https://example.com/docs").is_ok());
        assert!(validate_browser_url("ftp://example.com").is_err());
    }

    #[test]
    fn test_validate_browser_url_blocks_metadata() {
        assert!(validate_browser_url("http://169.254.169.254/latest/meta-data/").is_err());
    }

    #[test]
    fn test_normalize_loopback_url() {
        assert_eq!(
            normalize_loopback_url("http://localhost:5174"),
            "http://127.0.0.1:5174/"
        );
        assert_eq!(
            normalize_loopback_url("http://localhost:3000/api/users?v=1"),
            "http://127.0.0.1:3000/api/users?v=1"
        );
        assert_eq!(
            normalize_loopback_url("https://example.com/page"),
            "https://example.com/page"
        );
        assert_eq!(
            normalize_loopback_url("file:///tmp/index.html"),
            "file:///tmp/index.html"
        );
    }

    #[test]
    fn test_resolve_screenshot_path_routes_into_minicode_screenshots() {
        let ws = Path::new("/tmp/ws");
        assert_eq!(
            BrowserController::resolve_screenshot_path(ws, Some("screenshots/hero.png")),
            std::path::PathBuf::from("/tmp/ws/.minicode/screenshots/hero.png")
        );
        assert_eq!(
            BrowserController::resolve_screenshot_path(ws, Some("hero.png")),
            std::path::PathBuf::from("/tmp/ws/.minicode/screenshots/hero.png")
        );
        assert_eq!(
            BrowserController::resolve_screenshot_path(ws, Some("hero-section")),
            std::path::PathBuf::from("/tmp/ws/.minicode/screenshots/hero-section.png")
        );
        assert_eq!(
            BrowserController::resolve_screenshot_path(ws, Some(".minicode/screenshots/hero.png")),
            std::path::PathBuf::from("/tmp/ws/.minicode/screenshots/hero.png")
        );
    }

    #[test]
    fn test_resolve_viewport_emulation_honors_custom_dimensions_with_preset() {
        let (_, dims, msg) =
            BrowserController::resolve_viewport_emulation(Some("mobile"), Some(375), Some(667))
                .unwrap()
                .unwrap();
        assert_eq!(dims, Some((375, 667, true, 2.0)));
        assert!(msg.contains("375x667"));
        assert!(msg.contains("mobile: true"));

        let (_, default_dims, default_msg) =
            BrowserController::resolve_viewport_emulation(Some("mobile"), None, None)
                .unwrap()
                .unwrap();
        assert_eq!(default_dims, Some((390, 844, true, 2.0)));
        assert!(default_msg.contains("390x844"));

        let (_, desktop_dims, desktop_msg) =
            BrowserController::resolve_viewport_emulation(Some("desktop"), None, None)
                .unwrap()
                .unwrap();
        assert_eq!(desktop_dims, Some((1440, 900, false, 1.0)));
        assert!(desktop_msg.contains("1440x900"));

        let (_, wxh_dims, _) =
            BrowserController::resolve_viewport_emulation(Some("1440x900"), None, None)
                .unwrap()
                .unwrap();
        assert_eq!(wxh_dims, Some((1440, 900, false, 1.0)));
    }

    #[tokio::test]
    async fn test_reconcile_loopback_url_preserves_live_unregistered_port() {
        // 1. Bind two real ephemeral loopback TCP listeners:
        //    - `unregistered_listener` simulates `vite preview` bound inside a detached subshell (`4180`)
        //    - `registered_listener` simulates `vite dev` supervised in MiniDevRegistry (`5180`)
        let unregistered_listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind unregistered");
        let live_port = unregistered_listener.local_addr().unwrap().port();

        let registered_listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind registered");
        let dev_port = registered_listener.local_addr().unwrap().port();

        let mock_ws =
            std::env::temp_dir().join(format!("minicode_reconcile_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&mock_ws).unwrap();

        let registry = crate::dev::registry::get_global_dev_registry();
        let req = crate::dev::models::SpawnDevRequest {
            command: "sleep 5".to_string(),
            name: Some("mock-vite-dev".to_string()),
            process_type: crate::dev::models::DevProcessType::Frontend,
            working_dir: None,
            extra_env: std::collections::HashMap::new(),
            port_hint: Some(dev_port),
            max_memory_mb: None,
            port_policy: Some(crate::dev::models::PortConflictPolicy::Ignore),
            restart_policy: None,
        };
        let (_summary, handle) = registry
            .spawn_with_options(&mock_ws, req, true)
            .await
            .expect("spawn mock dev");

        // 2. While `unregistered_listener` is actively listening, `reconcile_loopback_url` MUST preserve `live_port`!
        let req_url = format!("http://127.0.0.1:{}/", live_port);
        let (reconciled_live, note_live) = reconcile_loopback_url(&req_url).await;
        assert_eq!(reconciled_live, req_url);
        assert!(
            note_live.is_none(),
            "Live listening port must never be rewritten to another server's port, got: {:?}",
            note_live
        );

        // 3. Once `unregistered_listener` closes (port is dead), `reconcile_loopback_url` reconciles to `dev_port`!
        drop(unregistered_listener);
        let (reconciled_dead, note_dead) = reconcile_loopback_url(&req_url).await;
        assert_eq!(reconciled_dead, format!("http://127.0.0.1:{}/", dev_port));
        assert!(note_dead.is_some());

        let _ = handle.terminate().await;
        drop(registered_listener);
        let _ = std::fs::remove_dir_all(&mock_ws);
    }

    #[test]
    fn test_parse_snapshot_strips_simulator_aura_hud_overlay() {
        let gui_html = r##"<!DOCTYPE html>
            <html>
            <head>
                <title>minicode — Fast Rust TUI Coding Agent</title>
                <style id="minicode-simulator-styles">#minicode-simulator-container { position: fixed; }</style>
            </head>
            <body>
                <div id="root">
                    <main>
                        <h1>Autonomous Terminal Engineering</h1>
                        <a href="#features">Explore Capabilities</a>
                    </main>
                </div>
                <div id="minicode-simulator-container" data-minicode-ignore="true" aria-hidden="true">
                    <div id="minicode-simulator-pill">
                        <span id="minicode-pill-text">AI Agent Ready</span>
                        <span id="minicode-pill-stop" title="Halt Agent">HALT ■</span>
                    </div>
                </div>
            </body>
            </html>"##;

        let snapshot =
            BrowserController::parse_html_to_aria_snapshot("http://127.0.0.1:4173/", gui_html);
        assert!(snapshot
            .text_summary
            .contains("Autonomous Terminal Engineering"));
        assert!(
            !snapshot.text_summary.contains("AI Agent Ready"),
            "Simulator HUD status text leaked into snapshot: {}",
            snapshot.text_summary
        );
        assert!(
            !snapshot.text_summary.contains("HALT"),
            "Simulator HUD halt button leaked into snapshot: {}",
            snapshot.text_summary
        );
    }

    #[tokio::test]
    async fn test_skill_forge_synthesis_and_task_local_space_scope() {
        let traffic = serde_json::json!({
            "entries": [
                {
                    "method": "GET",
                    "url": "https://api.acme.dev/v1/orders/98412?api_key=live_secret_999&page=2",
                    "status": 200,
                    "response_body": "{\"items\":[{\"id\":98412,\"total\":49.0}]}"
                }
            ]
        });
        let tables = serde_json::json!({
            "tables": [
                { "selector": "table#orders", "headers": ["Order ID", "Status", "Total"] }
            ]
        });
        let (slug, md) = BrowserController::synthesize_forged_skill_markdown(
            "Acme Orders API",
            "https://app.acme.dev/orders",
            "Acme Orders Dashboard",
            &traffic,
            &tables,
        );
        assert_eq!(slug, "acme-orders-api");
        assert!(
            md.contains("GET https://api.acme.dev/v1/orders/{id}?api_key=$ENV:API_KEY&page={page}")
        );
        assert!(!md.contains("live_secret_999"));
        assert!(md.contains("table#orders"));
        assert!(md.contains("space: \"acme-orders-api\""));

        // Verify task-local BrowserSpace isolation across concurrent Tokio tasks
        let (a, b) = tokio::join!(
            with_browser_space(
                Some("space-alpha".to_string()),
                Some("http://127.0.0.1:8080".to_string()),
                async {
                    tokio::task::yield_now().await;
                    manager::current_space_scope()
                }
            ),
            with_browser_space(Some("space-beta".to_string()), None, async {
                tokio::task::yield_now().await;
                manager::current_space_scope()
            })
        );
        assert_eq!(a.space.as_deref(), Some("space-alpha"));
        assert_eq!(a.proxy.as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(b.space.as_deref(), Some("space-beta"));
        assert_eq!(b.proxy, None);
        assert_eq!(manager::current_space_scope().space, None);
    }

    #[test]
    fn test_domain_skill_surfacing_coordinate_batch_and_replay_html() {
        let tmp_ws =
            std::env::temp_dir().join(format!("minicode_dom_skill_{}", uuid::Uuid::new_v4()));
        let skill_dir = tmp_ws
            .join(".minicode")
            .join("skills")
            .join("acme-orders-api");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let (_, md) = BrowserController::synthesize_forged_skill_markdown(
            "Acme Orders API",
            "https://app.acme.dev/orders",
            "Acme Orders",
            &serde_json::json!({ "entries": [] }),
            &serde_json::json!({ "tables": [] }),
        );
        std::fs::write(skill_dir.join("SKILL.md"), md).unwrap();

        let mut snap = BrowserController::parse_html_to_aria_snapshot(
            "https://app.acme.dev/orders/42",
            "<html><head><title>Orders</title></head><body><h1>Orders</h1></body></html>",
        );
        BrowserController::attach_domain_skill_hints(&mut snap, &tmp_ws);
        let vp = snap.viewport_summary.as_deref().unwrap_or("");
        assert!(
            vp.contains("Matching Domain Skill") && vp.contains("acme-orders-api"),
            "Expected forged domain skill to surface on host navigation, got: {vp}"
        );
        let _ = std::fs::remove_dir_all(&tmp_ws);

        // Coordinate click BatchStep parsing
        let steps = BatchStep::parse_steps(&serde_json::json!([
            { "action": "click", "x": 320.5, "y": 480 }
        ]))
        .unwrap();
        assert_eq!(steps.len(), 1);
        match &steps[0] {
            BatchStep::Click { x, y, .. } => {
                assert_eq!(*x, Some(320.5));
                assert_eq!(*y, Some(480.0));
            }
            other => panic!("Expected Click step, got {other:?}"),
        }

        // Self-contained Replay HTML scrubber escaping `</script>`
        let session = ReplaySession {
            name: "checkout-flow".to_string(),
            started_at: "2026-11-17T08:00:00Z".to_string(),
            frames: vec![ReplayFrame {
                step: 1,
                action: "click (320, 480)".to_string(),
                url: "https://app.acme.dev/?q=</script><script>alert(1)</script>".to_string(),
                timestamp: "08:00:01.000Z".to_string(),
                png_base64: "iVBORw0KGgo=".to_string(),
            }],
        };
        let html = BrowserController::render_replay_html(&session);
        assert!(html.contains("minibrowse Visual Action Replay"));
        assert!(!html.contains("</script><script>alert(1)"));
        assert!(html.contains("<\\/script>"));
    }
}
