use super::accessibility::AccessibilityManager;
use super::driver::CdpClient;
use super::page_agent::PageAgent;
use super::AriaElement;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

/// Declarative browser action step in a multi-action batch pipeline
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BatchStep {
    Navigate {
        url: String,
    },
    Click {
        #[serde(default, rename = "ref")]
        target_ref: Option<String>,
        #[serde(default)]
        selector: Option<String>,
    },
    Hover {
        #[serde(default, rename = "ref")]
        target_ref: Option<String>,
        #[serde(default)]
        selector: Option<String>,
    },
    Fill {
        #[serde(default, rename = "ref")]
        target_ref: Option<String>,
        #[serde(default)]
        selector: Option<String>,
        text: String,
    },
    SelectOption {
        #[serde(default, rename = "ref")]
        target_ref: Option<String>,
        #[serde(default)]
        selector: Option<String>,
        option_text: String,
    },
    Scroll {
        direction: String,
    },
    ScrollHorizontal {
        direction: String,
        #[serde(default)]
        pixels: Option<i32>,
        #[serde(default)]
        selector: Option<String>,
    },
    ScrollContainer {
        #[serde(default)]
        selector: Option<String>,
        #[serde(default, rename = "ref")]
        target_ref: Option<String>,
        pixels: i32,
    },
    WaitForSelector {
        selector: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    WaitForNetworkIdle {
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    WaitDelay {
        ms: u64,
    },
    EvaluateJs {
        script: String,
    },
    AssertText {
        text: String,
        #[serde(default)]
        selector: Option<String>,
    },
    Screenshot {
        #[serde(default)]
        path: Option<String>,
    },
}

/// Recorded outcome of an individual batch step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStepOutcome {
    pub step_number: usize,
    pub action: String,
    pub detail: String,
    pub success: bool,
    pub duration_ms: u64,
}

/// Dispatches DOM interaction commands and returns the updated page snapshot
pub struct BrowserInteractor;

impl BrowserInteractor {
    /// Clicks an element identified by its ARIA reference (@v1:e1) or selector with dual-dispatch
    pub async fn click_element(
        cdp: &CdpClient,
        target_ref: &str,
        acc_mgr: &mut AccessibilityManager,
    ) -> Result<String> {
        let el_opt = acc_mgr.resolve_ref(target_ref).ok().cloned();
        let target_name = el_opt
            .as_ref()
            .map(|e| e.name.as_str())
            .unwrap_or(target_ref);
        let tag = el_opt.as_ref().map(|e| e.tag.as_str()).unwrap_or("element");

        tracing::info!(
            target_ref = %target_ref,
            tag = %tag,
            name = %target_name,
            "Executing dual-dispatch browser click with visual aura"
        );

        // 1. In-page click via PageAgent probe (updates visual aura, cursor glide, ripple, and dispatches W3C DOM events)
        let (cx, cy, clicked_name) =
            match PageAgent::click_element_with_aura(cdp, target_ref, target_name, None).await {
                Ok(coords) => coords,
                Err(e) => {
                    if let Some(ref el) = el_opt {
                        let click_js = build_click_js(el);
                        let exec_res = cdp.evaluate_js(&click_js).await?;
                        if exec_res.starts_with("Error:") {
                            return Err(ToolError::CommandExec(format!(
                                "Failed clicking element '{}' ({}): {}",
                                target_ref, el.name, exec_res
                            ))
                            .into());
                        }
                        (100.0, 100.0, el.name.clone())
                    } else {
                        return Err(e);
                    }
                }
            };

        // 2. Dual-dispatch: also trigger native OS-level CDP mouse click at (cx, cy)
        if cx > 0.0 || cy > 0.0 {
            let _ = cdp.mouse_click_at(cx, cy).await;
        }

        // Allow DOM / network to settle dynamically
        let _ = cdp.wait_for_network_idle(Duration::from_millis(500)).await;

        // Advance accessibility revision for subsequent actions
        acc_mgr.next_revision();

        // Retrieve updated HTML and rebuild accessibility tree
        let updated_html = cdp.get_document_html().await.unwrap_or_default();
        let updated_elements = acc_mgr.update_from_html(&updated_html);

        let confirmation = format!(
            "Clicked **{}** `<{}>` \"{}\" at ({:.0}, {:.0}) (DOM updated to revision v{} with {} interactive elements):\n\n",
            target_ref,
            tag,
            clicked_name,
            cx,
            cy,
            acc_mgr.revision(),
            updated_elements.len()
        );

        let report = format_updated_tree(acc_mgr.revision(), &updated_elements);
        Ok(format!("{}{}", confirmation, report))
    }

    /// Types text into an input or textarea element (@v1:e2)
    pub async fn fill_element(
        cdp: &CdpClient,
        target_ref: &str,
        text: &str,
        acc_mgr: &mut AccessibilityManager,
    ) -> Result<String> {
        let el = acc_mgr.resolve_ref(target_ref)?.clone();

        tracing::info!(
            target_ref = %target_ref,
            tag = %el.tag,
            text_len = text.len(),
            "Executing browser text fill"
        );

        let fill_js = build_fill_js(&el, text);
        let exec_res = cdp.evaluate_js(&fill_js).await?;

        if exec_res.starts_with("Error:") {
            return Err(ToolError::CommandExec(format!(
                "Failed filling element '{}' ({}): {}",
                target_ref, el.name, exec_res
            ))
            .into());
        }

        let _ = cdp.wait_for_network_idle(Duration::from_millis(300)).await;
        acc_mgr.next_revision();

        let updated_html = cdp.get_document_html().await.unwrap_or_default();
        let updated_elements = acc_mgr.update_from_html(&updated_html);

        let confirmation = format!(
            "Filled **{}** `<{}>` \"{}\" with \"{}\" (revision v{}):\n\n",
            target_ref,
            el.tag,
            el.name,
            text,
            acc_mgr.revision()
        );

        let report = format_updated_tree(acc_mgr.revision(), &updated_elements);
        Ok(format!("{}{}", confirmation, report))
    }

    /// Builds the Chrome CDP evaluation script for viewport scrolling
    pub fn build_scroll_js(direction: &str) -> &'static str {
        match direction.to_lowercase().as_str() {
            "up" | "pageup" => {
                "(() => { window.scrollBy(0, -window.innerHeight * 0.75); return 'scrolled_up'; })()"
            }
            "down" | "pagedown" => {
                "(() => { window.scrollBy(0, window.innerHeight * 0.75); return 'scrolled_down'; })()"
            }
            "top" => "(() => { window.scrollTo(0, 0); return 'scrolled_top'; })()",
            "bottom" => "(() => { window.scrollTo(0, document.body.scrollHeight); return 'scrolled_bottom'; })()",
            _ => "(() => { window.scrollBy(0, 500); return 'scrolled_down'; })()",
        }
    }

    /// Scrolls the viewport in the specified direction ("up", "down", "top", "bottom")
    /// Uses intelligent container fallback + native CDP mouse wheel dispatch
    pub async fn scroll_page(cdp: &CdpClient, direction: &str) -> Result<String> {
        // 1. In-page scroll with container fallback and simulator aura update
        let _ = PageAgent::scroll_with_container_fallback(cdp, direction, None).await;

        // 2. Dual-dispatch: also issue CDP native mouse wheel scroll at center of viewport
        let metrics: super::page_agent::PageMetrics =
            PageAgent::get_page_metrics(cdp).await.unwrap_or_default();
        let vw = if metrics.viewport_width > 0.0 {
            metrics.viewport_width
        } else {
            1280.0
        };
        let vh = if metrics.viewport_height > 0.0 {
            metrics.viewport_height
        } else {
            800.0
        };
        let scroll_delta = match direction.to_lowercase().as_str() {
            "up" | "pageup" => -500.0,
            "top" => -2000.0,
            "bottom" => 2000.0,
            _ => 500.0,
        };
        let _ = cdp
            .mouse_wheel_scroll(vw / 2.0, vh / 2.0, 0.0, scroll_delta)
            .await;

        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(format!("Scrolled page {}", direction))
    }

    /// Hovers over an interactive element identified by its ARIA reference (@v1:e1)
    pub async fn hover_element(
        cdp: &CdpClient,
        target_ref: &str,
        acc_mgr: &mut AccessibilityManager,
    ) -> Result<String> {
        let el = acc_mgr.resolve_ref(target_ref)?.clone();

        tracing::info!(
            target_ref = %target_ref,
            tag = %el.tag,
            name = %el.name,
            "Executing browser hover"
        );

        let hover_js = build_hover_js(&el);
        let exec_res = cdp.evaluate_js(&hover_js).await?;

        if exec_res.starts_with("Error:") {
            return Err(ToolError::CommandExec(format!(
                "Failed hovering element '{}' ({}): {}",
                target_ref, el.name, exec_res
            ))
            .into());
        }

        let _ = cdp.wait_for_network_idle(Duration::from_millis(300)).await;
        acc_mgr.next_revision();

        let updated_html = cdp.get_document_html().await.unwrap_or_default();
        let updated_elements = acc_mgr.update_from_html(&updated_html);

        let confirmation = format!(
            "Hovered over **{}** `<{}>` \"{}\" (DOM revision v{} with {} interactive elements):\n\n",
            target_ref,
            el.tag,
            el.name,
            acc_mgr.revision(),
            updated_elements.len()
        );

        let report = format_updated_tree(acc_mgr.revision(), &updated_elements);
        Ok(format!("{}{}", confirmation, report))
    }

    /// Selects an option from a `<select>` dropdown by its visible text or value
    pub async fn select_option(
        cdp: &CdpClient,
        target_ref: &str,
        option_text: &str,
        acc_mgr: &mut AccessibilityManager,
    ) -> Result<String> {
        let el = acc_mgr.resolve_ref(target_ref)?.clone();

        tracing::info!(
            target_ref = %target_ref,
            tag = %el.tag,
            option_text = %option_text,
            "Executing browser select option"
        );

        let select_js = build_select_option_js(&el, option_text);
        let exec_res = cdp.evaluate_js(&select_js).await?;

        if exec_res.starts_with("Error:") {
            return Err(ToolError::CommandExec(format!(
                "Failed selecting option '{}' in element '{}' ({}): {}",
                option_text, target_ref, el.name, exec_res
            ))
            .into());
        }

        let _ = cdp.wait_for_network_idle(Duration::from_millis(300)).await;
        acc_mgr.next_revision();

        let updated_html = cdp.get_document_html().await.unwrap_or_default();
        let updated_elements = acc_mgr.update_from_html(&updated_html);

        let confirmation = format!(
            "Selected option \"{}\" in **{}** `<{}>` (revision v{}):\n\n",
            option_text,
            target_ref,
            el.tag,
            acc_mgr.revision()
        );

        let report = format_updated_tree(acc_mgr.revision(), &updated_elements);
        Ok(format!("{}{}", confirmation, report))
    }

    /// Scrolls horizontally left or right across the page or within a specific scrollable container
    pub async fn scroll_horizontally(
        cdp: &CdpClient,
        direction: &str,
        pixels: Option<i32>,
        selector: Option<&str>,
    ) -> Result<String> {
        let scroll_js = build_horizontal_scroll_js(direction, pixels, selector);
        cdp.evaluate_js(&scroll_js).await?;
        tokio::time::sleep(Duration::from_millis(200)).await;

        let target_desc = selector
            .map(|s| format!(" container '{}'", s))
            .unwrap_or_else(|| " page".to_string());
        Ok(format!(
            "Scrolled{} horizontally {}",
            target_desc, direction
        ))
    }

    /// Scrolls a specific scrollable container by vertical pixels
    #[allow(dead_code)]
    pub async fn scroll_container(
        cdp: &CdpClient,
        selector: Option<&str>,
        target_ref: Option<&str>,
        pixels: i32,
        acc_mgr: &AccessibilityManager,
    ) -> Result<String> {
        let el = if let Some(r) = target_ref {
            Some(acc_mgr.resolve_ref(r)?.clone())
        } else {
            None
        };

        let scroll_js = build_container_scroll_js(selector, el.as_ref(), pixels);
        let res = cdp.evaluate_js(&scroll_js).await?;
        if res.starts_with("Error:") {
            return Err(ToolError::CommandExec(res).into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;

        Ok(format!("Scrolled container by {}px", pixels))
    }

    /// Executes an atomic pipeline of browser actions sequentially in a single turn
    pub async fn execute_batch(
        cdp: &CdpClient,
        steps: &[BatchStep],
        acc_mgr: &mut AccessibilityManager,
        workspace_root: &std::path::Path,
    ) -> Result<String> {
        let mut outcomes = Vec::with_capacity(steps.len());
        let mut failure: Option<String> = None;

        for (idx, step) in steps.iter().enumerate() {
            let step_num = idx + 1;
            let start_time = std::time::Instant::now();

            let (step_action, step_detail, res): (&str, String, Result<()>) = match step {
                BatchStep::Navigate { url } => {
                    let action = "navigate";
                    let detail = format!("Navigated to '{}'", url);
                    (action, detail, cdp.navigate(url).await)
                }
                BatchStep::Click {
                    target_ref,
                    selector,
                } => {
                    let action = "click";
                    if let Some(r) = target_ref {
                        match acc_mgr.resolve_ref(r) {
                            Ok(el) => {
                                let el = el.clone();
                                let detail = format!("Clicked {} <{}> \"{}\"", r, el.tag, el.name);
                                let click_res = match PageAgent::click_element_with_aura(
                                    cdp, r, &el.name, None,
                                )
                                .await
                                {
                                    Ok((cx, cy, _)) => {
                                        if cx > 0.0 || cy > 0.0 {
                                            let _ = cdp.mouse_click_at(cx, cy).await;
                                        }
                                        Ok(())
                                    }
                                    Err(_) => {
                                        let click_js = build_click_js(&el);
                                        match cdp.evaluate_js(&click_js).await {
                                            Ok(res_str) if res_str.starts_with("Error:") => {
                                                Err(ToolError::CommandExec(format!(
                                                    "Click failed: {}",
                                                    res_str
                                                ))
                                                .into())
                                            }
                                            Ok(_) => Ok(()),
                                            Err(e) => Err(e),
                                        }
                                    }
                                };
                                let _ = cdp.wait_for_network_idle(Duration::from_millis(400)).await;
                                (action, detail, click_res)
                            }
                            Err(e) => (action, format!("Failed to resolve ref '{}'", r), Err(e)),
                        }
                    } else if let Some(sel) = selector {
                        let detail = format!("Clicked selector '{}'", sel);
                        let click_res = match PageAgent::click_element_with_aura(
                            cdp,
                            "",
                            "",
                            Some(sel),
                        )
                        .await
                        {
                            Ok((cx, cy, _)) => {
                                if cx > 0.0 || cy > 0.0 {
                                    let _ = cdp.mouse_click_at(cx, cy).await;
                                }
                                Ok(())
                            }
                            Err(_) => {
                                let click_js = build_click_by_selector_js(sel);
                                match cdp.evaluate_js(&click_js).await {
                                    Ok(res_str) if res_str.starts_with("Error:") => {
                                        Err(ToolError::CommandExec(format!(
                                            "Click failed: {}",
                                            res_str
                                        ))
                                        .into())
                                    }
                                    Ok(_) => Ok(()),
                                    Err(e) => Err(e),
                                }
                            }
                        };
                        if click_res.is_ok() {
                            let _ = cdp.wait_for_network_idle(Duration::from_millis(400)).await;
                        }
                        (action, detail, click_res)
                    } else {
                        (
                            action,
                            "Missing 'ref' or 'selector'".to_string(),
                            Err(ToolError::InvalidArguments {
                                name: "browser_batch".to_string(),
                                reason: "Click action requires either 'ref' or 'selector'"
                                    .to_string(),
                            }
                            .into()),
                        )
                    }
                }
                BatchStep::Fill {
                    target_ref,
                    selector,
                    text,
                } => {
                    let action = "fill";
                    if let Some(r) = target_ref {
                        match acc_mgr.resolve_ref(r) {
                            Ok(el) => {
                                let el = el.clone();
                                let detail = format!("Filled {} <{}> with \"{}\"", r, el.tag, text);
                                let fill_js = build_fill_js(&el, text);
                                let exec_res = cdp.evaluate_js(&fill_js).await;
                                match exec_res {
                                    Ok(res_str) if res_str.starts_with("Error:") => (
                                        action,
                                        detail,
                                        Err(ToolError::CommandExec(format!(
                                            "Fill failed: {}",
                                            res_str
                                        ))
                                        .into()),
                                    ),
                                    Ok(_) => (action, detail, Ok(())),
                                    Err(e) => (action, detail, Err(e)),
                                }
                            }
                            Err(e) => (action, format!("Failed to resolve ref '{}'", r), Err(e)),
                        }
                    } else if let Some(sel) = selector {
                        let detail = format!("Filled selector '{}' with \"{}\"", sel, text);
                        let fill_js = build_fill_by_selector_js(sel, text);
                        let exec_res = cdp.evaluate_js(&fill_js).await;
                        match exec_res {
                            Ok(res_str) if res_str.starts_with("Error:") => (
                                action,
                                detail,
                                Err(ToolError::CommandExec(format!("Fill failed: {}", res_str))
                                    .into()),
                            ),
                            Ok(_) => (action, detail, Ok(())),
                            Err(e) => (action, detail, Err(e)),
                        }
                    } else {
                        (
                            action,
                            "Missing 'ref' or 'selector'".to_string(),
                            Err(ToolError::InvalidArguments {
                                name: "browser_batch".to_string(),
                                reason: "Fill action requires either 'ref' or 'selector'"
                                    .to_string(),
                            }
                            .into()),
                        )
                    }
                }
                BatchStep::Hover {
                    target_ref,
                    selector,
                } => {
                    let action = "hover";
                    if let Some(r) = target_ref {
                        match acc_mgr.resolve_ref(r) {
                            Ok(el) => {
                                let el = el.clone();
                                let detail =
                                    format!("Hovered over {} <{}> \"{}\"", r, el.tag, el.name);
                                let hover_js = build_hover_js(&el);
                                let exec_res = cdp.evaluate_js(&hover_js).await;
                                match exec_res {
                                    Ok(res_str) if res_str.starts_with("Error:") => (
                                        action,
                                        detail,
                                        Err(ToolError::CommandExec(format!(
                                            "Hover failed: {}",
                                            res_str
                                        ))
                                        .into()),
                                    ),
                                    Ok(_) => {
                                        let _ = cdp
                                            .wait_for_network_idle(Duration::from_millis(300))
                                            .await;
                                        (action, detail, Ok(()))
                                    }
                                    Err(e) => (action, detail, Err(e)),
                                }
                            }
                            Err(e) => (action, format!("Failed to resolve ref '{}'", r), Err(e)),
                        }
                    } else if let Some(sel) = selector {
                        let detail = format!("Hovered over selector '{}'", sel);
                        let hover_js = build_hover_by_selector_js(sel);
                        let exec_res = cdp.evaluate_js(&hover_js).await;
                        match exec_res {
                            Ok(res_str) if res_str.starts_with("Error:") => (
                                action,
                                detail,
                                Err(ToolError::CommandExec(format!("Hover failed: {}", res_str))
                                    .into()),
                            ),
                            Ok(_) => {
                                let _ = cdp.wait_for_network_idle(Duration::from_millis(300)).await;
                                (action, detail, Ok(()))
                            }
                            Err(e) => (action, detail, Err(e)),
                        }
                    } else {
                        (
                            action,
                            "Missing 'ref' or 'selector'".to_string(),
                            Err(ToolError::InvalidArguments {
                                name: "browser_batch".to_string(),
                                reason: "Hover action requires either 'ref' or 'selector'"
                                    .to_string(),
                            }
                            .into()),
                        )
                    }
                }
                BatchStep::SelectOption {
                    target_ref,
                    selector,
                    option_text,
                } => {
                    let action = "select_option";
                    if let Some(r) = target_ref {
                        match acc_mgr.resolve_ref(r) {
                            Ok(el) => {
                                let el = el.clone();
                                let detail = format!(
                                    "Selected option \"{}\" in {} <{}>",
                                    option_text, r, el.tag
                                );
                                let select_js = build_select_option_js(&el, option_text);
                                let exec_res = cdp.evaluate_js(&select_js).await;
                                match exec_res {
                                    Ok(res_str) if res_str.starts_with("Error:") => (
                                        action,
                                        detail,
                                        Err(ToolError::CommandExec(format!(
                                            "Select option failed: {}",
                                            res_str
                                        ))
                                        .into()),
                                    ),
                                    Ok(_) => {
                                        let _ = cdp
                                            .wait_for_network_idle(Duration::from_millis(300))
                                            .await;
                                        (action, detail, Ok(()))
                                    }
                                    Err(e) => (action, detail, Err(e)),
                                }
                            }
                            Err(e) => (action, format!("Failed to resolve ref '{}'", r), Err(e)),
                        }
                    } else if let Some(sel) = selector {
                        let detail =
                            format!("Selected option \"{}\" in selector '{}'", option_text, sel);
                        let select_js = build_select_option_by_selector_js(sel, option_text);
                        let exec_res = cdp.evaluate_js(&select_js).await;
                        match exec_res {
                            Ok(res_str) if res_str.starts_with("Error:") => (
                                action,
                                detail,
                                Err(ToolError::CommandExec(format!(
                                    "Select option failed: {}",
                                    res_str
                                ))
                                .into()),
                            ),
                            Ok(_) => {
                                let _ = cdp.wait_for_network_idle(Duration::from_millis(300)).await;
                                (action, detail, Ok(()))
                            }
                            Err(e) => (action, detail, Err(e)),
                        }
                    } else {
                        (
                            action,
                            "Missing 'ref' or 'selector'".to_string(),
                            Err(ToolError::InvalidArguments {
                                name: "browser_batch".to_string(),
                                reason: "SelectOption requires either 'ref' or 'selector'"
                                    .to_string(),
                            }
                            .into()),
                        )
                    }
                }
                BatchStep::Scroll { direction } => {
                    let action = "scroll";
                    let detail = format!("Scrolled {}", direction);
                    let scroll_js = Self::build_scroll_js(direction);
                    (action, detail, cdp.evaluate_js(scroll_js).await.map(|_| ()))
                }
                BatchStep::ScrollHorizontal {
                    direction,
                    pixels,
                    selector,
                } => {
                    let action = "scroll_horizontal";
                    let detail = format!("Scrolled horizontally {}", direction);
                    let scroll_js =
                        build_horizontal_scroll_js(direction, *pixels, selector.as_deref());
                    (
                        action,
                        detail,
                        cdp.evaluate_js(&scroll_js).await.map(|_| ()),
                    )
                }
                BatchStep::ScrollContainer {
                    selector,
                    target_ref,
                    pixels,
                } => {
                    let action = "scroll_container";
                    let detail = format!("Scrolled container by {}px", pixels);
                    let el = if let Some(r) = target_ref {
                        match acc_mgr.resolve_ref(r) {
                            Ok(element) => Some(element.clone()),
                            Err(e) => return Err(e),
                        }
                    } else {
                        None
                    };
                    let scroll_js =
                        build_container_scroll_js(selector.as_deref(), el.as_ref(), *pixels);
                    let res = cdp.evaluate_js(&scroll_js).await;
                    match res {
                        Ok(res_str) if res_str.starts_with("Error:") => {
                            (action, detail, Err(ToolError::CommandExec(res_str).into()))
                        }
                        Ok(_) => (action, detail, Ok(())),
                        Err(e) => (action, detail, Err(e)),
                    }
                }
                BatchStep::WaitForSelector {
                    selector,
                    timeout_ms,
                } => {
                    let action = "wait_for_selector";
                    let detail = format!("Waited for selector '{}'", selector);
                    let dur = Duration::from_millis(timeout_ms.unwrap_or(5000));
                    (action, detail, cdp.wait_for_selector(selector, dur).await)
                }
                BatchStep::WaitForNetworkIdle { timeout_ms } => {
                    let action = "wait_for_network_idle";
                    let detail = "Waited for network idle".to_string();
                    let dur = Duration::from_millis(timeout_ms.unwrap_or(2000));
                    (action, detail, cdp.wait_for_network_idle(dur).await)
                }
                BatchStep::WaitDelay { ms } => {
                    let action = "delay";
                    let detail = format!("Waited {}ms", ms);
                    tokio::time::sleep(Duration::from_millis(*ms)).await;
                    (action, detail, Ok(()))
                }
                BatchStep::EvaluateJs { script } => {
                    let action = "eval_js";
                    match cdp.evaluate_js(script).await {
                        Ok(output) => (action, format!("Evaluated JS -> {}", output), Ok(())),
                        Err(e) => (action, "Failed evaluating JS".to_string(), Err(e)),
                    }
                }
                BatchStep::AssertText { text, selector } => {
                    let action = "assert_text";
                    let check_script = if let Some(sel) = selector {
                        format!(
                            "Boolean(document.querySelector(\"{}\")?.innerText?.includes({:?}))",
                            sel.replace('"', "\\\""),
                            text
                        )
                    } else {
                        format!("document.body.innerText.includes({:?})", text)
                    };
                    match cdp.evaluate_js(&check_script).await {
                        Ok(res) if res.trim() == "true" => (
                            action,
                            format!("Assertion passed: text {:?} is present", text),
                            Ok(()),
                        ),
                        Ok(_) => {
                            let detail = format!("Assertion failed: text {:?} not found", text);
                            (
                                action,
                                detail.clone(),
                                Err(ToolError::CommandExec(detail).into()),
                            )
                        }
                        Err(e) => (action, "Assertion evaluation failed".to_string(), Err(e)),
                    }
                }
                BatchStep::Screenshot { path } => {
                    let action = "screenshot";
                    match cdp.take_screenshot().await {
                        Ok(png_bytes) => {
                            let target_path = if let Some(p) = path {
                                workspace_root.join(p)
                            } else {
                                let dir =
                                    workspace_root.join(crate::constants::BROWSER_SCREENSHOTS_DIR);
                                let _ = std::fs::create_dir_all(&dir);
                                let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                                dir.join(format!("screenshot_{}.png", timestamp))
                            };
                            if let Some(parent) = target_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            match std::fs::write(&target_path, &png_bytes) {
                                Ok(_) => (
                                    action,
                                    format!(
                                        "Captured screenshot saved to '{}'",
                                        target_path.display()
                                    ),
                                    Ok(()),
                                ),
                                Err(e) => (
                                    action,
                                    format!(
                                        "Failed writing screenshot to '{}'",
                                        target_path.display()
                                    ),
                                    Err(ToolError::FileOp {
                                        path: target_path.display().to_string(),
                                        source: e,
                                    }
                                    .into()),
                                ),
                            }
                        }
                        Err(e) => (action, "Failed taking screenshot".to_string(), Err(e)),
                    }
                }
            };

            let duration_ms = start_time.elapsed().as_millis() as u64;

            match res {
                Ok(()) => {
                    outcomes.push(BatchStepOutcome {
                        step_number: step_num,
                        action: step_action.to_string(),
                        detail: step_detail,
                        success: true,
                        duration_ms,
                    });
                }
                Err(e) => {
                    outcomes.push(BatchStepOutcome {
                        step_number: step_num,
                        action: step_action.to_string(),
                        detail: format!("Failed: {}", e),
                        success: false,
                        duration_ms,
                    });
                    failure = Some(format!("Step {} failed: {}", step_num, e));
                    break;
                }
            }
        }

        // Update accessibility tree at end of batch
        acc_mgr.next_revision();
        let updated_html = cdp.get_document_html().await.unwrap_or_default();
        let updated_elements = acc_mgr.update_from_html(&updated_html);

        // Format detailed markdown response for agent
        let mut report = String::from("### Browser Batch Execution Report\n\n");
        let succeeded_count = outcomes.iter().filter(|o| o.success).count();

        if let Some(ref err) = failure {
            report.push_str(&format!("❌ **Batch halted with error:** {}\n", err));
            report.push_str(&format!(
                "Executed {} of {} step(s) before failure.\n\n",
                succeeded_count,
                steps.len()
            ));
        } else {
            report.push_str(&format!(
                "✅ **All {} step(s) executed successfully.**\n\n",
                steps.len()
            ));
        }

        report.push_str("#### Step Outcomes:\n");
        for o in &outcomes {
            let icon = if o.success { "✓" } else { "✗" };
            report.push_str(&format!(
                "{}. [{}] `{}` — {} (took {}ms)\n",
                o.step_number, icon, o.action, o.detail, o.duration_ms
            ));
        }
        report.push('\n');

        let tree_report = format_updated_tree(acc_mgr.revision(), &updated_elements);
        report.push_str(&tree_report);

        if failure.is_some() {
            Err(ToolError::CommandExec(report).into())
        } else {
            Ok(report)
        }
    }
}

fn build_click_js(el: &AriaElement) -> String {
    let tag = &el.tag;
    let name = el.name.replace('"', "\\\"");
    let ref_id = &el.ref_id;
    let id_attr = el.attributes.get("id").map(|s| s.as_str()).unwrap_or("");
    let name_attr = el.attributes.get("name").map(|s| s.as_str()).unwrap_or("");
    let href_attr = el.attributes.get("href").map(|s| s.as_str()).unwrap_or("");

    let payload = json!({
        "ref": ref_id,
        "tag": tag,
        "name": name,
        "id": id_attr,
        "attr_name": name_attr,
        "href": href_attr,
    });

    format!(
        r#"(function() {{
            const p = {};
            let target = null;
            if (window.__minicode_page_agent && typeof window.__minicode_page_agent.resolveElement === 'function') {{
                target = window.__minicode_page_agent.resolveElement(p.ref, p.name, null);
            }}
            if (!target && p.ref) {{
                try {{ target = document.querySelector('[data-minicode-ref="' + CSS.escape(p.ref) + '"]'); }} catch (_) {{}}
            }}
            if (!target && p.id) {{
                target = document.getElementById(p.id);
            }}
            if (!target && p.attr_name) {{
                try {{ target = document.querySelector('[name="' + CSS.escape(p.attr_name) + '"]'); }} catch (_) {{}}
            }}
            if (!target && p.href) {{
                try {{ target = document.querySelector('[href="' + CSS.escape(p.href) + '"]'); }} catch (_) {{}}
            }}
            if (!target) {{
                const candidates = Array.from(document.querySelectorAll(p.tag));
                target = candidates.find(el => {{
                    if (p.id && el.id === p.id) return true;
                    if (p.attr_name && el.name === p.attr_name) return true;
                    if (p.href && el.getAttribute('href') === p.href) return true;
                    if (el.innerText && el.innerText.trim().toLowerCase() === p.name.toLowerCase()) return true;
                    if (el.innerText && el.innerText.trim().includes(p.name)) return true;
                    return false;
                }});
            }}

            if (!target) return 'Error: Element matching tag <' + p.tag + '> not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            target.focus();
            ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click'].forEach(evt => {{
                target.dispatchEvent(new MouseEvent(evt, {{ bubbles: true, cancelable: true, view: window }}));
            }});
            if (typeof target.click === 'function') {{
                try {{ target.click(); }} catch (_) {{}}
            }}
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_fill_js(el: &AriaElement, text: &str) -> String {
    let tag = &el.tag;
    let text_escaped = text.replace('"', "\\\"").replace('\n', "\\n");
    let id_attr = el.attributes.get("id").map(|s| s.as_str()).unwrap_or("");
    let name_attr = el.attributes.get("name").map(|s| s.as_str()).unwrap_or("");
    let placeholder = el
        .attributes
        .get("placeholder")
        .map(|s| s.as_str())
        .unwrap_or("");

    let payload = json!({
        "tag": tag,
        "text": text_escaped,
        "id": id_attr,
        "attr_name": name_attr,
        "placeholder": placeholder,
    });

    format!(
        r#"(function() {{
            const p = {};
            const candidates = Array.from(document.querySelectorAll('input, textarea, [contenteditable="true"]'));
            let target = candidates.find(el => {{
                if (p.id && el.id === p.id) return true;
                if (p.attr_name && el.name === p.attr_name) return true;
                if (p.placeholder && el.getAttribute('placeholder') === p.placeholder) return true;
                return false;
            }}) || candidates[0];

            if (!target) return 'Error: Input element not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            target.focus();

            if (target.isContentEditable || target.getAttribute('contenteditable') === 'true') {{
                let inserted = false;
                try {{
                    if (target.dispatchEvent(new InputEvent('beforeinput', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: p.text }}))) {{
                        target.innerText = p.text;
                        target.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: p.text }}));
                        inserted = target.innerText.trim() === p.text.trim();
                    }}
                }} catch (_) {{}}
                if (!inserted) {{
                    try {{
                        const doc = target.ownerDocument || document;
                        const sel = (doc.defaultView || window).getSelection();
                        const range = doc.createRange();
                        range.selectNodeContents(target);
                        sel?.removeAllRanges();
                        sel?.addRange(range);
                        doc.execCommand('delete', false);
                        doc.execCommand('insertText', false, p.text);
                    }} catch (_) {{
                        target.innerText = p.text;
                    }}
                }}
                target.dispatchEvent(new Event('change', {{ bubbles: true }}));
                target.blur();
                return 'OK';
            }}

            try {{
                const proto = Object.getPrototypeOf(target);
                const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set
                    || Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')?.set
                    || Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value')?.set;
                if (setter) {{
                    setter.call(target, p.text);
                }} else {{
                    target.value = p.text;
                }}
                if (target._valueTracker) {{
                    target._valueTracker.setValue('');
                }}
            }} catch (_) {{
                target.value = p.text;
            }}
            target.dispatchEvent(new Event('input', {{ bubbles: true }}));
            target.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_click_by_selector_js(selector: &str) -> String {
    let payload = json!({ "selector": selector });
    format!(
        r#"(function() {{
            const p = {};
            const target = document.querySelector(p.selector);
            if (!target) return 'Error: Element matching selector "' + p.selector + '" not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            target.focus();
            ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click'].forEach(evt => {{
                target.dispatchEvent(new MouseEvent(evt, {{ bubbles: true, cancelable: true, view: window }}));
            }});
            if (typeof target.click === 'function') {{
                try {{ target.click(); }} catch (_) {{}}
            }}
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_hover_js(el: &AriaElement) -> String {
    let tag = &el.tag;
    let name = el.name.replace('"', "\\\"");
    let id_attr = el.attributes.get("id").map(|s| s.as_str()).unwrap_or("");
    let name_attr = el.attributes.get("name").map(|s| s.as_str()).unwrap_or("");
    let href_attr = el.attributes.get("href").map(|s| s.as_str()).unwrap_or("");

    let payload = json!({
        "tag": tag,
        "name": name,
        "id": id_attr,
        "attr_name": name_attr,
        "href": href_attr,
    });

    format!(
        r#"(function() {{
            const p = {};
            const candidates = Array.from(document.querySelectorAll(p.tag));
            let target = candidates.find(el => {{
                if (p.id && el.id === p.id) return true;
                if (p.attr_name && el.name === p.attr_name) return true;
                if (p.href && el.getAttribute('href') === p.href) return true;
                if (el.innerText && el.innerText.trim().includes(p.name)) return true;
                return false;
            }}) || candidates[0];

            if (!target) return 'Error: Element matching tag <' + p.tag + '> not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            const rect = target.getBoundingClientRect();
            const x = rect.left + rect.width / 2;
            const y = rect.top + rect.height / 2;
            const pointerOpts = {{ bubbles: true, cancelable: true, clientX: x, clientY: y, pointerType: 'mouse' }};
            const mouseOpts = {{ bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 }};
            target.dispatchEvent(new PointerEvent('pointerover', pointerOpts));
            target.dispatchEvent(new PointerEvent('pointerenter', Object.assign({{}}, pointerOpts, {{ bubbles: false }})));
            target.dispatchEvent(new MouseEvent('mouseover', mouseOpts));
            target.dispatchEvent(new MouseEvent('mouseenter', Object.assign({{}}, mouseOpts, {{ bubbles: false }})));
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_hover_by_selector_js(selector: &str) -> String {
    let payload = json!({ "selector": selector });
    format!(
        r#"(function() {{
            const p = {};
            const target = document.querySelector(p.selector);
            if (!target) return 'Error: Element matching selector "' + p.selector + '" not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            const rect = target.getBoundingClientRect();
            const x = rect.left + rect.width / 2;
            const y = rect.top + rect.height / 2;
            const pointerOpts = {{ bubbles: true, cancelable: true, clientX: x, clientY: y, pointerType: 'mouse' }};
            const mouseOpts = {{ bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 }};
            target.dispatchEvent(new PointerEvent('pointerover', pointerOpts));
            target.dispatchEvent(new PointerEvent('pointerenter', Object.assign({{}}, pointerOpts, {{ bubbles: false }})));
            target.dispatchEvent(new MouseEvent('mouseover', mouseOpts));
            target.dispatchEvent(new MouseEvent('mouseenter', Object.assign({{}}, mouseOpts, {{ bubbles: false }})));
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_select_option_js(el: &AriaElement, option_text: &str) -> String {
    let tag = &el.tag;
    let id_attr = el.attributes.get("id").map(|s| s.as_str()).unwrap_or("");
    let name_attr = el.attributes.get("name").map(|s| s.as_str()).unwrap_or("");

    let payload = json!({
        "tag": tag,
        "id": id_attr,
        "attr_name": name_attr,
        "option_text": option_text,
    });

    format!(
        r#"(function() {{
            const p = {};
            const candidates = Array.from(document.querySelectorAll('select'));
            let target = candidates.find(el => {{
                if (p.id && el.id === p.id) return true;
                if (p.attr_name && el.name === p.attr_name) return true;
                return false;
            }}) || candidates[0];

            if (!target) return 'Error: Select element not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            target.focus();

            const options = Array.from(target.options);
            const opt = options.find(o => {{
                const t = (o.text || '').trim().toLowerCase();
                const v = (o.value || '').trim().toLowerCase();
                const targetText = p.option_text.trim().toLowerCase();
                return t === targetText || v === targetText || t.includes(targetText);
            }});

            if (!opt) return 'Error: Option matching "' + p.option_text + '" not found in <select>';
            target.value = opt.value;
            target.dispatchEvent(new Event('input', {{ bubbles: true }}));
            target.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_select_option_by_selector_js(selector: &str, option_text: &str) -> String {
    let payload = json!({ "selector": selector, "option_text": option_text });
    format!(
        r#"(function() {{
            const p = {};
            let target = document.querySelector(p.selector);
            if (!target) return 'Error: Element matching selector "' + p.selector + '" not found in DOM';
            if (target.tagName.toLowerCase() !== 'select') {{
                target = target.querySelector('select') || target;
            }}
            if (target.tagName.toLowerCase() !== 'select') {{
                return 'Error: Element matching selector "' + p.selector + '" is not a <select> element';
            }}
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            target.focus();

            const options = Array.from(target.options);
            const opt = options.find(o => {{
                const t = (o.text || '').trim().toLowerCase();
                const v = (o.value || '').trim().toLowerCase();
                const targetText = p.option_text.trim().toLowerCase();
                return t === targetText || v === targetText || t.includes(targetText);
            }});

            if (!opt) return 'Error: Option matching "' + p.option_text + '" not found in <select>';
            target.value = opt.value;
            target.dispatchEvent(new Event('input', {{ bubbles: true }}));
            target.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_horizontal_scroll_js(
    direction: &str,
    pixels: Option<i32>,
    selector: Option<&str>,
) -> String {
    let mult = if direction.eq_ignore_ascii_case("left") {
        -1
    } else {
        1
    };
    let px = mult * pixels.unwrap_or(400);
    let payload = json!({ "pixels": px, "selector": selector });

    format!(
        r#"(function() {{
            const p = {};
            if (p.selector) {{
                const el = document.querySelector(p.selector);
                if (el) {{
                    el.scrollBy({{ left: p.pixels, behavior: 'instant' }});
                    return 'scrolled_container_horizontally';
                }}
            }}
            window.scrollBy({{ left: p.pixels, behavior: 'instant' }});
            return 'scrolled_page_horizontally';
        }})()"#,
        payload
    )
}

fn build_container_scroll_js(
    selector: Option<&str>,
    el: Option<&AriaElement>,
    pixels: i32,
) -> String {
    let sel = selector.unwrap_or("");
    let el_id = el
        .and_then(|e| e.attributes.get("id"))
        .map(|s| s.as_str())
        .unwrap_or("");
    let payload = json!({ "selector": sel, "id": el_id, "pixels": pixels });

    format!(
        r#"(function() {{
            const p = {};
            let target = null;
            if (p.selector) target = document.querySelector(p.selector);
            if (!target && p.id) target = document.getElementById(p.id);
            if (!target) return 'Error: Scrollable container not found in DOM';
            target.scrollBy({{ top: p.pixels, behavior: 'instant' }});
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_fill_by_selector_js(selector: &str, text: &str) -> String {
    let payload = json!({ "selector": selector, "text": text });
    format!(
        r#"(function() {{
            const p = {};
            const target = document.querySelector(p.selector);
            if (!target) return 'Error: Input matching selector "' + p.selector + '" not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            target.focus();

            if (target.isContentEditable || target.getAttribute('contenteditable') === 'true') {{
                let inserted = false;
                try {{
                    if (target.dispatchEvent(new InputEvent('beforeinput', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: p.text }}))) {{
                        target.innerText = p.text;
                        target.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: p.text }}));
                        inserted = target.innerText.trim() === p.text.trim();
                    }}
                }} catch (_) {{}}
                if (!inserted) {{
                    try {{
                        const doc = target.ownerDocument || document;
                        const sel = (doc.defaultView || window).getSelection();
                        const range = doc.createRange();
                        range.selectNodeContents(target);
                        sel?.removeAllRanges();
                        sel?.addRange(range);
                        doc.execCommand('delete', false);
                        doc.execCommand('insertText', false, p.text);
                    }} catch (_) {{
                        target.innerText = p.text;
                    }}
                }}
                target.dispatchEvent(new Event('change', {{ bubbles: true }}));
                target.blur();
                return 'OK';
            }}

            try {{
                const proto = Object.getPrototypeOf(target);
                const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set
                    || Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')?.set
                    || Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value')?.set;
                if (setter) {{
                    setter.call(target, p.text);
                }} else {{
                    target.value = p.text;
                }}
                if (target._valueTracker) {{
                    target._valueTracker.setValue('');
                }}
            }} catch (_) {{
                target.value = p.text;
            }}
            target.dispatchEvent(new Event('input', {{ bubbles: true }}));
            target.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'OK';
        }})()"#,
        payload
    )
}

fn format_updated_tree(revision: u32, elements: &[AriaElement]) -> String {
    let mut out = format!("Interactive Accessibility Tree (Revision v{}):\n", revision);
    for el in elements.iter().take(20) {
        out.push_str(&format!(
            "  • **{}** `<{}>` ({}) \"{}\"\n",
            el.ref_id, el.tag, el.role, el.name
        ));
    }
    if elements.len() > 20 {
        out.push_str(&format!(
            "  ... +{} more interactive elements\n",
            elements.len() - 20
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_js_is_valid_iife() {
        for dir in &["up", "down", "top", "bottom", "pageup", "pagedown", "other"] {
            let js = BrowserInteractor::build_scroll_js(dir);
            assert!(js.starts_with("(() => {"));
            assert!(js.ends_with("})()"));
            assert!(js.contains("return 'scrolled_"));
        }
    }

    #[test]
    fn test_batch_step_deserialization() {
        let json_input = r#"[
            {"action": "navigate", "url": "http://localhost:3000"},
            {"action": "fill", "ref": "@v1:e2", "text": "alice@example.com"},
            {"action": "click", "ref": "@v1:e3"},
            {"action": "wait_for_selector", "selector": ".dashboard", "timeout_ms": 3000},
            {"action": "assert_text", "text": "Welcome Alice"}
        ]"#;

        let steps: Vec<BatchStep> = serde_json::from_str(json_input).expect("Valid batch steps");
        assert_eq!(steps.len(), 5);
        assert_eq!(
            steps[0],
            BatchStep::Navigate {
                url: "http://localhost:3000".to_string()
            }
        );
        assert_eq!(
            steps[1],
            BatchStep::Fill {
                target_ref: Some("@v1:e2".to_string()),
                selector: None,
                text: "alice@example.com".to_string()
            }
        );
        assert_eq!(
            steps[2],
            BatchStep::Click {
                target_ref: Some("@v1:e3".to_string()),
                selector: None
            }
        );
        assert_eq!(
            steps[3],
            BatchStep::WaitForSelector {
                selector: ".dashboard".to_string(),
                timeout_ms: Some(3000)
            }
        );
        assert_eq!(
            steps[4],
            BatchStep::AssertText {
                text: "Welcome Alice".to_string(),
                selector: None
            }
        );

        let json_selector = r##"[
            {"action": "fill", "selector": "#email", "text": "bob@example.com"},
            {"action": "click", "selector": "#submit"}
        ]"##;
        let selector_steps: Vec<BatchStep> =
            serde_json::from_str(json_selector).expect("Valid selector batch steps");
        assert_eq!(
            selector_steps[0],
            BatchStep::Fill {
                target_ref: None,
                selector: Some("#email".to_string()),
                text: "bob@example.com".to_string()
            }
        );
        assert_eq!(
            selector_steps[1],
            BatchStep::Click {
                target_ref: None,
                selector: Some("#submit".to_string())
            }
        );
    }

    #[test]
    fn test_synthetic_event_generators() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert("id".to_string(), "submit-btn".to_string());
        let el = AriaElement {
            ref_id: "@v1:e1".to_string(),
            tag: "button".to_string(),
            role: "button".to_string(),
            name: "Submit".to_string(),
            attributes: attrs,
        };

        let click_script = build_click_js(&el);
        assert!(click_script.contains("pointerdown"));
        assert!(click_script.contains("mousedown"));
        assert!(click_script.contains("click"));
        assert!(click_script.contains("submit-btn"));

        let fill_script = build_fill_js(&el, "hello world");
        assert!(fill_script.contains("HTMLInputElement.prototype"));
        assert!(fill_script.contains("hello world"));
        assert!(fill_script.contains("dispatchEvent"));

        let hover_script = build_hover_js(&el);
        assert!(hover_script.contains("pointerover"));
        assert!(hover_script.contains("mouseover"));

        let select_script = build_select_option_js(&el, "Option 2");
        assert!(select_script.contains("Option 2"));
        assert!(select_script.contains("target.options"));

        let h_scroll = build_horizontal_scroll_js("right", Some(300), None);
        assert!(h_scroll.contains("300"));
        assert!(h_scroll.contains("scrollBy"));
    }

    #[test]
    fn test_batch_step_deserialization_new_actions() {
        let json_input = r##"[
            {"action": "hover", "ref": "@v1:e1"},
            {"action": "select_option", "ref": "@v1:e2", "option_text": "California"},
            {"action": "scroll_horizontal", "direction": "right", "pixels": 250},
            {"action": "scroll_container", "selector": "#code-editor", "pixels": 500}
        ]"##;

        let steps: Vec<BatchStep> =
            serde_json::from_str(json_input).expect("Valid new batch steps");
        assert_eq!(steps.len(), 4);
        assert_eq!(
            steps[0],
            BatchStep::Hover {
                target_ref: Some("@v1:e1".to_string()),
                selector: None,
            }
        );
        assert_eq!(
            steps[1],
            BatchStep::SelectOption {
                target_ref: Some("@v1:e2".to_string()),
                selector: None,
                option_text: "California".to_string(),
            }
        );
        assert_eq!(
            steps[2],
            BatchStep::ScrollHorizontal {
                direction: "right".to_string(),
                pixels: Some(250),
                selector: None,
            }
        );
        assert_eq!(
            steps[3],
            BatchStep::ScrollContainer {
                selector: Some("#code-editor".to_string()),
                target_ref: None,
                pixels: 500,
            }
        );
    }
}
