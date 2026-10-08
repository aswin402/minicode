pub mod accessibility;
pub mod debug;
pub mod driver;
pub mod engine;
pub mod interaction;
pub mod manager;
pub mod markdown;
pub mod page_agent;

#[allow(unused_imports)]
pub use accessibility::AccessibilityManager;
#[allow(unused_imports)]
pub use debug::{ConsoleEntry, DebugCollector, LogLevel, NetworkErrorEntry};
#[allow(unused_imports)]
pub use driver::{CdpClient, MockRouteRule};
#[allow(unused_imports)]
pub use engine::{BrowserEngine, BrowserMode, EngineConfig, GUI_PRIORITY, HEADLESS_PRIORITY};
#[allow(unused_imports)]
pub use interaction::{BatchStep, BatchStepOutcome, BrowserInteractor};
#[allow(unused_imports)]
pub use manager::{BrowserManager, EngineProcess};
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
    pub interactive_elements: Vec<AriaElement>,
    pub text_summary: String,
}

pub struct BrowserController;

impl BrowserController {
    /// Navigates to a URL with automatic engine fallback and returns a structured ARIA snapshot
    pub async fn navigate_and_snapshot(
        url: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<PageSnapshot> {
        validate_browser_url(url)?;

        // Try launching preferred browser engine according to priority chain
        match BrowserManager::get_or_launch(mode, workspace_root).await {
            Ok(engine) => {
                let engine_name = format!("{} ({})", engine.process.config.engine, mode);
                tracing::info!(engine = %engine_name, url = %url, "Navigating via browser engine");

                let cdp_res = async {
                    engine.cdp.navigate(url).await?;
                    let _ = PageAgent::inject_probe(&engine.cdp).await;
                    let _ = PageAgent::scan_visual_tree(&engine.cdp).await;
                    if mode == BrowserMode::Gui {
                        let _ =
                            PageAgent::inject_simulator_aura(&engine.cdp, Some("AI Agent Ready"))
                                .await;
                    }
                    engine.cdp.get_document_html().await
                }
                .await;

                if let Ok(html) = cdp_res {
                    let mut acc_mgr = engine.accessibility.lock().await;
                    let elements = acc_mgr.update_from_html(&html);
                    let mut snapshot = Self::parse_html_to_aria_snapshot(url, &html);
                    snapshot.interactive_elements = elements;
                    snapshot.engine_used = engine_name;
                    return Ok(snapshot);
                } else if let Err(e) = cdp_res {
                    tracing::warn!(error = ?e, "CDP navigation failed; falling back to HTTP");
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
                return Ok(snapshot);
            }
        }

        // Fallback: zero-browser HTTP fetcher
        tracing::info!(url = %url, "No browser binary found or launched; using HTTP reader");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(crate::constants::WEB_USER_AGENT)
            .build()
            .map_err(|e| ToolError::CommandExec(format!("Failed to build HTTP client: {}", e)))?;

        let mut response = client.get(url).send().await;
        // On Linux, localhost may resolve to ::1 (IPv6) first while dev servers (Vite, Express) only listen on 127.0.0.1 (IPv4).
        if response.is_err() && url.contains("localhost") {
            let alt_url = url.replace("localhost", "127.0.0.1");
            if let Ok(alt_resp) = client.get(&alt_url).send().await {
                response = Ok(alt_resp);
            }
        }
        let response = response.map_err(|e| {
            ToolError::CommandExec(format!("Failed to connect to '{}': {}", url, e))
        })?;

        let html = response
            .text()
            .await
            .map_err(|e| ToolError::CommandExec(format!("Failed to read response body: {}", e)))?;

        let mut snapshot = Self::parse_html_to_aria_snapshot(url, &html);
        snapshot.engine_used = "HTTP Reader (No browser binary on PATH)".to_string();
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

        if let Ok(engine) = BrowserManager::get_or_launch(mode, workspace_root).await {
            let engine_name = format!("{} ({})", engine.process.config.engine, mode);
            // Check if current browser is already loaded at this URL
            let current_url = engine
                .cdp
                .evaluate_js("window.location.href")
                .await
                .unwrap_or_default();
            let clean_current = current_url.trim_matches('"').trim_end_matches('/');
            let clean_target = url.trim_end_matches('/');

            if !clean_current.is_empty() && clean_current == clean_target {
                tracing::info!(
                    url = %url,
                    "Live page is already active at URL; snapshotting current DOM state"
                );
                let _ = PageAgent::inject_probe(&engine.cdp).await;
                let html = engine.cdp.get_document_html().await.unwrap_or_default();
                let mut acc_mgr = engine.accessibility.lock().await;
                let elements = acc_mgr.update_from_html(&html);
                let mut snapshot = Self::parse_html_to_aria_snapshot(url, &html);
                snapshot.interactive_elements = elements;
                snapshot.engine_used = engine_name;
                return Ok(snapshot);
            }
        }

        Self::navigate_and_snapshot(url, mode, workspace_root).await
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
        let engine_name = format!("{} ({})", engine.process.config.engine, mode);
        let current_url = engine
            .cdp
            .evaluate_js("window.location.href")
            .await
            .unwrap_or_default();
        let clean_current = current_url.trim_matches('"').trim_end_matches('/');

        let effective_url = if clean_current.is_empty() || clean_current == "about:blank" {
            "http://localhost"
        } else {
            clean_current
        };

        let _ = PageAgent::inject_probe(&engine.cdp).await;
        let html = engine.cdp.get_document_html().await.unwrap_or_default();
        let mut acc_mgr = engine.accessibility.lock().await;
        let elements = acc_mgr.update_from_html(&html);
        let mut snapshot = Self::parse_html_to_aria_snapshot(effective_url, &html);
        snapshot.interactive_elements = elements;
        snapshot.engine_used = engine_name;
        Ok(snapshot)
    }

    /// Clicks an element by ARIA reference and returns the updated page snapshot
    pub async fn click_and_snapshot(
        target_ref: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;

        let current_html = engine.cdp.get_document_html().await.unwrap_or_default();
        let mut acc_mgr = engine.accessibility.lock().await;
        acc_mgr.update_from_html(&current_html);

        BrowserInteractor::click_element(&engine.cdp, target_ref, &mut acc_mgr).await
    }

    /// Fills text into an input or textarea element and returns the updated page snapshot
    pub async fn fill_and_snapshot(
        target_ref: &str,
        text: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;

        let current_html = engine.cdp.get_document_html().await.unwrap_or_default();
        let mut acc_mgr = engine.accessibility.lock().await;
        acc_mgr.update_from_html(&current_html);

        BrowserInteractor::fill_element(&engine.cdp, target_ref, text, &mut acc_mgr).await
    }

    /// Scrolls the active browser viewport in the given direction
    pub async fn scroll(
        direction: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        BrowserInteractor::scroll_page(&engine.cdp, direction).await
    }

    /// Hovers over an element by ARIA reference and returns updated page snapshot
    pub async fn hover_and_snapshot(
        target_ref: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let current_html = engine.cdp.get_document_html().await.unwrap_or_default();
        let mut acc_mgr = engine.accessibility.lock().await;
        acc_mgr.update_from_html(&current_html);
        BrowserInteractor::hover_element(&engine.cdp, target_ref, &mut acc_mgr).await
    }

    /// Selects an option from a `<select>` dropdown and returns updated page snapshot
    pub async fn select_option_and_snapshot(
        target_ref: &str,
        option_text: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let current_html = engine.cdp.get_document_html().await.unwrap_or_default();
        let mut acc_mgr = engine.accessibility.lock().await;
        acc_mgr.update_from_html(&current_html);
        BrowserInteractor::select_option(&engine.cdp, target_ref, option_text, &mut acc_mgr).await
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
        BrowserInteractor::scroll_horizontally(&engine.cdp, direction, pixels, selector).await
    }

    /// Retrieves scroll metrics and viewport geometry for the current page
    pub async fn get_page_metrics(mode: BrowserMode, workspace_root: &Path) -> Result<PageMetrics> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        PageAgent::get_page_metrics(&engine.cdp).await
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

    /// Manages browser tabs / targets (list, create, switch, close)
    pub async fn manage_tabs(
        action: &str,
        url: Option<&str>,
        target_id: Option<&str>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        match action {
            "list" => {
                let targets = engine.cdp.list_targets().await?;
                let mut out = format!("### Active Browser Tabs ({})\n\n", targets.len());
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
                    out.push_str(&format!(
                        "{}. [{}] \"{}\" (`{}`) - ID: `{}`\n",
                        idx + 1,
                        target_type,
                        title,
                        target_url,
                        id
                    ));
                }
                Ok(out)
            }
            "create" | "new" => {
                let target_url = url.unwrap_or("about:blank");
                let id = engine.cdp.create_tab(target_url, true).await?;
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
                engine.cdp.switch_tab(id).await?;
                Ok(format!("Switched browser focus to tab ID `{}`", id))
            }
            "close" => {
                let id = target_id.ok_or_else(|| ToolError::InvalidArguments {
                    name: "browser_tabs".to_string(),
                    reason: "Action 'close' requires 'target_id'".to_string(),
                })?;
                engine.cdp.close_tab(id).await?;
                Ok(format!("Closed browser tab ID `{}`", id))
            }
            other => Err(ToolError::InvalidArguments {
                name: "browser_tabs".to_string(),
                reason: format!(
                    "Unknown tab action '{}'. Supported: list, create, switch, close",
                    other
                ),
            }
            .into()),
        }
    }

    /// Retrieves diagnostic logs (console errors, unhandled exceptions, and failed HTTP requests)
    pub async fn get_debug_logs(mode: BrowserMode, workspace_root: &Path) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;

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

        Ok(collector.format_report())
    }

    /// Evaluates JavaScript in the browser context and returns result
    pub async fn evaluate_js(
        script: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        engine.cdp.evaluate_js(script).await
    }

    /// Captures a screenshot and saves it to `.minicode/screenshots/`
    pub async fn take_screenshot(
        mode: BrowserMode,
        workspace_root: &Path,
        custom_path: Option<&str>,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let png_bytes = engine.cdp.take_screenshot().await?;

        let target_path = if let Some(p) = custom_path {
            workspace_root.join(p)
        } else {
            let dir = workspace_root.join(BROWSER_SCREENSHOTS_DIR);
            let _ = std::fs::create_dir_all(&dir);
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            dir.join(format!("screenshot_{}.png", timestamp))
        };
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

        let health_probe_js = r#"(() => {
            try {
                const title = document.title || "";
                const hasHead = !!document.head;
                const hasBody = !!document.body;
                const styleSheets = document.styleSheets ? document.styleSheets.length : 0;
                const linkStyles = document.querySelectorAll('link[rel="stylesheet"]').length;
                const styleTags = document.querySelectorAll('style').length;
                const bodyComputed = (window.getComputedStyle && document.body) ? window.getComputedStyle(document.body) : null;
                const fontFamily = bodyComputed ? bodyComputed.fontFamily : "";
                const bgColor = bodyComputed ? bodyComputed.backgroundColor : "";
                const fLower = fontFamily.toLowerCase();
                const isDefaultSerif = (fLower.includes("times") || /(^|[\s,])serif([\s,]|$)/i.test(fLower)) && !fLower.includes("sans-serif");
                const bodyTextLen = document.body ? (document.body.innerText || "").trim().length : 0;
                const semanticSections = document.querySelectorAll('section, main, article, nav, header, footer').length;
                return JSON.stringify({
                    title,
                    hasHead,
                    hasBody,
                    styleSheets,
                    linkStyles,
                    styleTags,
                    fontFamily,
                    bgColor,
                    isDefaultSerif,
                    bodyTextLen,
                    semanticSections
                });
            } catch(e) {
                return JSON.stringify({ error: e.toString() });
            }
        })()"#;

        let mut health_notes = Vec::new();
        if let Ok(raw_json) = engine.cdp.evaluate_js(health_probe_js).await {
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
                let body_len = probe
                    .get("bodyTextLen")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let sections = probe
                    .get("semanticSections")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let title = probe.get("title").and_then(|v| v.as_str()).unwrap_or("");

                if !has_head || !has_body {
                    health_notes.push("⚠ CRITICAL DOM DEFECT: Document is missing `<head>` or `<body>` tag! Page structure is truncated or malformed HTML.".to_string());
                }
                if style_sheets == 0 && link_styles == 0 && style_tags == 0 {
                    health_notes.push("⚠ UNSTYLED PAGE ALERT: 0 active stylesheets found! The page is rendering raw unstyled HTML. Check if `<link rel=\"stylesheet\">` is missing in index.html.".to_string());
                } else if is_default_serif {
                    health_notes.push(format!("⚠ UNSTYLED FONT WARNING: Page body is displaying default browser serif font ('{}'). Check if your CSS stylesheet was loaded.", font_family));
                }
                if body_len < 50 {
                    health_notes.push("⚠ EMPTY BODY WARNING: Page body contains almost no text content (<50 characters). Check if content loaded properly.".to_string());
                }

                health_notes.push(format!(
                    "• Page Title: \"{}\" | Stylesheets: {} active ({} linked, {} embedded) | Sections: {} | Body Text: {} chars",
                    title, style_sheets, link_styles, style_tags, sections, body_len
                ));
            }
        }

        let mut msg = format!(
            "Screenshot saved to '{}' ({} bytes)\n💡 Note: Screenshots are binary image files saved for human/browser visual review. Do NOT attempt to read them with `read_file`. Use `browser_snapshot` or the DOM telemetry below to inspect page content.",
            target_path.display(),
            png_bytes.len()
        );
        if !health_notes.is_empty() {
            msg.push_str("\n\n[DOM & Visual Health Observation]:\n");
            msg.push_str(&health_notes.join("\n"));
        }

        Ok(msg)
    }

    /// Executes an atomic pipeline of browser actions sequentially in a single turn
    pub async fn execute_batch(
        steps: &[BatchStep],
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let mut acc_mgr = engine.accessibility.lock().await;
        let current_html = engine.cdp.get_document_html().await.unwrap_or_default();
        acc_mgr.update_from_html(&current_html);

        BrowserInteractor::execute_batch(&engine.cdp, steps, &mut acc_mgr, workspace_root).await
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

    /// Aggregates console errors, uncaught exceptions, and failed network calls into a diagnostic report
    pub async fn get_debug_bundle(mode: BrowserMode, workspace_root: &Path) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let debug_report = engine.cdp.debug_collector().format_report();
        let current_html = engine.cdp.get_document_html().await.unwrap_or_default();
        let mut acc_mgr = engine.accessibility.lock().await;
        let elements = acc_mgr.update_from_html(&current_html);

        let mut report = String::from("### Browser Diagnostic Bundle\n\n");
        report.push_str(&debug_report);
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

    /// Runs a comprehensive in-page QA audit on the given URL or active page
    pub async fn run_qa_audit(
        url_opt: Option<&str>,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let current_url = if let Some(url) = url_opt {
            engine.cdp.navigate(url).await?;
            url.to_string()
        } else {
            engine
                .cdp
                .evaluate_js("window.location.href")
                .await
                .unwrap_or_else(|_| "http://localhost".to_string())
        };

        let report = PageAgent::run_qa_audit(&engine.cdp, &current_url).await?;
        Ok(PageAgent::format_qa_report(&report))
    }

    /// Inspects in-page DOM elements including Shadow DOM and computed geometry
    pub async fn inspect_visual_dom(mode: BrowserMode, workspace_root: &Path) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let elements = PageAgent::scan_visual_tree(&engine.cdp).await?;
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
        let mut results = Vec::new();

        // 1. Viewport / Device Emulation
        if let Some(vp) = viewport {
            let lower = vp.to_lowercase();
            match lower.as_str() {
                "reset" | "clear" | "none" => {
                    engine.cdp.clear_device_metrics().await?;
                    results.push(
                        "Cleared device viewport override (reset to default window size)"
                            .to_string(),
                    );
                }
                "mobile" => {
                    engine.cdp.emulate_device(375, 667, true, 2.0).await?;
                    results.push(
                        "Emulating 'mobile' viewport (375x667, scale: 2.0, mobile: true)"
                            .to_string(),
                    );
                }
                "mobile_large" | "iphone" => {
                    engine.cdp.emulate_device(414, 896, true, 3.0).await?;
                    results.push("Emulating 'iphone / mobile_large' viewport (414x896, scale: 3.0, mobile: true)".to_string());
                }
                "tablet" | "ipad" => {
                    engine.cdp.emulate_device(768, 1024, true, 2.0).await?;
                    results.push(
                        "Emulating 'tablet / ipad' viewport (768x1024, scale: 2.0, mobile: true)"
                            .to_string(),
                    );
                }
                "desktop" => {
                    engine.cdp.emulate_device(1280, 800, false, 1.0).await?;
                    results.push(
                        "Emulating 'desktop' viewport (1280x800, scale: 1.0, mobile: false)"
                            .to_string(),
                    );
                }
                "desktop_wide" => {
                    engine.cdp.emulate_device(1920, 1080, false, 1.0).await?;
                    results.push(
                        "Emulating 'desktop_wide' viewport (1920x1080, scale: 1.0, mobile: false)"
                            .to_string(),
                    );
                }
                other => {
                    return Err(ToolError::InvalidArguments {
                        name: "browser_emulate".to_string(),
                        reason: format!("Unknown viewport preset '{}'. Supported: mobile, mobile_large, iphone, tablet, ipad, desktop, desktop_wide, reset", other),
                    }.into());
                }
            }
        } else if let (Some(w), Some(h)) = (custom_width, custom_height) {
            engine.cdp.emulate_device(w, h, false, 1.0).await?;
            results.push(format!("Custom viewport set to {}x{} (scale: 1.0)", w, h));
        }

        // 2. Network Throttling Emulation
        if let Some(net) = network {
            let lower = net.to_lowercase();
            match lower.as_str() {
                "reset" | "clear" | "none" | "online" => {
                    engine
                        .cdp
                        .emulate_network_conditions(false, 0.0, -1.0, -1.0)
                        .await?;
                    results.push("Network throttling cleared (unthrottled online)".to_string());
                }
                "offline" => {
                    engine
                        .cdp
                        .emulate_network_conditions(true, 0.0, 0.0, 0.0)
                        .await?;
                    results.push("Network set to 'offline'".to_string());
                }
                "slow_3g" => {
                    engine
                        .cdp
                        .emulate_network_conditions(false, 400.0, 50_000.0, 50_000.0)
                        .await?;
                    results.push(
                        "Network throttled to 'slow_3g' (latency: 400ms, throughput: 50 KB/s)"
                            .to_string(),
                    );
                }
                "fast_3g" => {
                    engine
                        .cdp
                        .emulate_network_conditions(false, 150.0, 180_000.0, 84_000.0)
                        .await?;
                    results.push(
                        "Network throttled to 'fast_3g' (latency: 150ms, throughput: 180 KB/s)"
                            .to_string(),
                    );
                }
                "cable" | "wifi" => {
                    engine
                        .cdp
                        .emulate_network_conditions(false, 20.0, 5_000_000.0, 1_000_000.0)
                        .await?;
                    results.push(
                        "Network set to 'cable / wifi' (latency: 20ms, throughput: 5 MB/s)"
                            .to_string(),
                    );
                }
                other => {
                    return Err(ToolError::InvalidArguments {
                        name: "browser_emulate".to_string(),
                        reason: format!("Unknown network preset '{}'. Supported: offline, slow_3g, fast_3g, cable, wifi, reset", other),
                    }.into());
                }
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

    /// Saves the current browser session state (cookies and localStorage) to a named profile in the workspace
    pub async fn save_session_state(
        profile_name: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
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

        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;

        let current_url = engine
            .cdp
            .evaluate_js("window.location.href")
            .await
            .unwrap_or_else(|_| "http://localhost".to_string());
        let cookies = engine
            .cdp
            .get_cookies()
            .await
            .unwrap_or_else(|_| serde_json::json!([]));
        let ls_raw = engine.cdp.evaluate_js(
            "(() => { try { const o = {}; for (let i = 0; i < localStorage.length; i++) { const k = localStorage.key(i); o[k] = localStorage.getItem(k); } return JSON.stringify(o); } catch(e) { return '{}'; } })()"
        ).await.unwrap_or_else(|_| "{}".to_string());
        let local_storage: serde_json::Value =
            serde_json::from_str(&ls_raw).unwrap_or_else(|_| serde_json::json!({}));

        let state_dir = workspace_root.join(BROWSER_STATE_DIR);
        tokio::fs::create_dir_all(&state_dir).await.map_err(|e| {
            ToolError::CommandExec(format!("Failed to create browser state directory: {}", e))
        })?;

        let file_path = state_dir.join(format!("{}.json", safe_name));
        let state = serde_json::json!({
            "profile": safe_name,
            "url": current_url,
            "cookies": cookies,
            "local_storage": local_storage,
            "saved_at": chrono::Utc::now().to_rfc3339()
        });

        let json_str = serde_json::to_string_pretty(&state).map_err(|e| {
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
        Ok(format!(
            "💾 Browser session state successfully saved:\n• Profile: `{}`\n• File: `{}`\n• Cookies: {}\n• LocalStorage keys: {}\n• Active URL: `{}`",
            safe_name,
            rel_path.display(),
            cookies_count,
            ls_count,
            current_url
        ))
    }

    /// Restores a previously saved browser session profile (cookies and localStorage) into the active session
    pub async fn restore_session_state(
        profile_name: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
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
        let state: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| ToolError::CommandExec(format!("Corrupt session state JSON: {}", e)))?;

        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;
        let mut applied = Vec::new();

        // Restore cookies
        if let Some(cookies) = state.get("cookies") {
            if cookies.is_array() && !cookies.as_array().map(|a| a.is_empty()).unwrap_or(true) {
                engine.cdp.set_cookies(cookies.clone()).await?;
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
                let _ = engine.cdp.navigate(url_str).await;
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
                    let _ = engine.cdp.evaluate_js(&script).await;
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
        let title = extract_title(html).unwrap_or_else(|| url.to_string());
        let elements = extract_interactive_elements(html);
        let text_summary = extract_readable_text(html);

        PageSnapshot {
            url: url.to_string(),
            title,
            engine_used: "Parser".to_string(),
            interactive_elements: elements,
            text_summary,
        }
    }

    /// Formats a snapshot into a clean agent-readable Markdown accessibility report
    pub fn format_snapshot_report(snapshot: &PageSnapshot) -> String {
        let mut out = format!(
            "Page: **{}** (`{}`)\nEngine: _{}_\n\n",
            snapshot.title, snapshot.url, snapshot.engine_used
        );

        if !snapshot.interactive_elements.is_empty() {
            out.push_str("Interactive Accessibility Tree (ARIA References):\n");
            for el in &snapshot.interactive_elements {
                let attrs_str = if el.attributes.is_empty() {
                    String::new()
                } else {
                    let pairs: Vec<String> = el
                        .attributes
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, v))
                        .collect();
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
            .collect::<Vec<_>>()
            .join("\n");
        out.push_str(&summary_preview);

        if snapshot.text_summary.lines().count() > 15 {
            out.push_str("\n\n_...[content truncated for token efficiency]..._");
        }

        out
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

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start_tag = "<title>";
    let end_tag = "</title>";

    if let Some(start_idx) = lower.find(start_tag) {
        let title_start = start_idx + start_tag.len();
        if let Some(end_idx) = lower[title_start..].find(end_tag) {
            let title = &html[title_start..title_start + end_idx];
            return Some(title.trim().to_string());
        }
    }
    None
}

fn extract_interactive_elements(html: &str) -> Vec<AriaElement> {
    let mut elements = Vec::new();
    let mut counter = 1;

    let patterns = [
        ("<button", "</button>", "button", "Button"),
        ("<a ", "</a>", "a", "Link"),
        ("<input", ">", "input", "Input"),
        ("<select", "</select>", "select", "Select"),
        ("<textarea", "</textarea>", "textarea", "TextBox"),
    ];

    for (start_pattern, end_pattern, tag_name, default_role) in patterns {
        let mut search_from = 0;
        while let Some(found_start) = html[search_from..].find(start_pattern) {
            let abs_start = search_from + found_start;
            let tag_content = if let Some(found_end) = html[abs_start..].find(end_pattern) {
                &html[abs_start..abs_start + found_end + end_pattern.len()]
            } else {
                &html[abs_start..]
            };

            let clean_name = strip_html_tags(tag_content);
            let mut attrs = HashMap::new();

            if let Some(name_attr) = extract_attr(tag_content, "name") {
                attrs.insert("name".to_string(), name_attr);
            }
            if let Some(type_attr) = extract_attr(tag_content, "type") {
                attrs.insert("type".to_string(), type_attr);
            }
            if let Some(href_attr) = extract_attr(tag_content, "href") {
                attrs.insert("href".to_string(), href_attr);
            }
            if let Some(placeholder) = extract_attr(tag_content, "placeholder") {
                attrs.insert("placeholder".to_string(), placeholder);
            }

            let display_name = if !clean_name.is_empty() {
                clean_name
            } else if let Some(placeholder) = attrs.get("placeholder") {
                placeholder.clone()
            } else if let Some(name) = attrs.get("name") {
                name.clone()
            } else {
                format!("Unnamed {}", default_role)
            };

            elements.push(AriaElement {
                ref_id: format!("@v1:e{}", counter),
                tag: tag_name.to_string(),
                role: default_role.to_string(),
                name: display_name.chars().take(80).collect(),
                attributes: attrs,
            });

            counter += 1;
            search_from = abs_start + tag_content.len();
            if elements.len() >= 50 {
                break;
            }
        }
    }

    elements
}

fn extract_attr(tag_str: &str, attr_name: &str) -> Option<String> {
    let key = format!("{}=\"", attr_name);
    if let Some(start) = tag_str.find(&key) {
        let val_start = start + key.len();
        if let Some(end) = tag_str[val_start..].find('"') {
            return Some(tag_str[val_start..val_start + end].to_string());
        }
    }
    None
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_readable_text(html: &str) -> String {
    let raw_text = strip_html_tags(html);
    raw_text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_html_to_aria_snapshot() {
        let sample_html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>Dashboard - Minicode</title></head>
            <body>
                <h1>Welcome to Minicode</h1>
                <p>Fast AI coding assistant.</p>
                <form action="/login" method="post">
                    <input type="text" name="username" placeholder="Username" />
                    <input type="password" name="password" placeholder="Password" />
                    <button type="submit">Log In</button>
                </form>
                <a href="/docs">Documentation</a>
            </body>
            </html>
        "#;

        let snapshot =
            BrowserController::parse_html_to_aria_snapshot("http://localhost:3000", sample_html);
        assert_eq!(snapshot.title, "Dashboard - Minicode");
        assert!(!snapshot.interactive_elements.is_empty());

        let report = BrowserController::format_snapshot_report(&snapshot);
        assert!(report.contains("@v1:e1"));
        assert!(report.contains("Log In"));
        assert!(report.contains("Documentation"));
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
}
