pub mod accessibility;
pub mod debug;
pub mod driver;
pub mod engine;
pub mod interaction;
pub mod manager;
pub mod markdown;

#[allow(unused_imports)]
pub use accessibility::AccessibilityManager;
#[allow(unused_imports)]
pub use debug::{ConsoleEntry, DebugCollector, LogLevel, NetworkErrorEntry};
#[allow(unused_imports)]
pub use driver::CdpClient;
#[allow(unused_imports)]
pub use engine::{BrowserEngine, BrowserMode, EngineConfig, GUI_PRIORITY, HEADLESS_PRIORITY};
#[allow(unused_imports)]
pub use interaction::BrowserInteractor;
#[allow(unused_imports)]
pub use manager::{BrowserManager, EngineProcess};
#[allow(unused_imports)]
pub use markdown::SmartMarkdownExtractor;

use crate::constants::{BROWSER_BLOCKED_HOSTS, BROWSER_SCREENSHOTS_DIR};
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
                    engine.cdp.get_document_html().await
                }
                .await;

                if let Ok(html) = cdp_res {
                    let mut snapshot = Self::parse_html_to_aria_snapshot(url, &html);
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

        let response = client.get(url).send().await.map_err(|e| {
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

    /// Clicks an element by ARIA reference and returns the updated page snapshot
    pub async fn click_and_snapshot(
        target_ref: &str,
        mode: BrowserMode,
        workspace_root: &Path,
    ) -> Result<String> {
        let engine = BrowserManager::get_or_launch(mode, workspace_root).await?;

        let current_html = engine.cdp.get_document_html().await.unwrap_or_default();
        let mut acc_mgr = AccessibilityManager::new();
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
        let mut acc_mgr = AccessibilityManager::new();
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

        Ok(format!(
            "Screenshot saved to '{}' ({} bytes)",
            target_path.display(),
            png_bytes.len()
        ))
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
