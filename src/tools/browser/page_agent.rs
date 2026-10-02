use super::driver::CdpClient;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Embedded in-page probe that runs inside the browser DOM context.
/// Pierces Shadow DOM, computes accurate layout geometry, and checks accessibility.
pub const PAGE_PROBE_JS: &str = r#"(function() {
    if (window.__minicode_page_agent) return 'already_installed';

    window.__minicode_page_agent = {
        // Pierces light DOM and Shadow DOM to find visible interactive elements
        scanInteractables: function() {
            var results = [];
            var idCounter = 1;

            function getShortSelector(el) {
                if (el.id) return '#' + CSS.escape(el.id);
                if (el.getAttribute('data-testid')) return '[data-testid="' + CSS.escape(el.getAttribute('data-testid')) + '"]';
                if (el.getAttribute('name')) return el.tagName.toLowerCase() + '[name="' + CSS.escape(el.getAttribute('name')) + '"]';
                if (el.getAttribute('aria-label')) return el.tagName.toLowerCase() + '[aria-label="' + CSS.escape(el.getAttribute('aria-label')) + '"]';
                
                var tag = el.tagName.toLowerCase();
                var parent = el.parentElement;
                if (!parent) return tag;
                var siblings = Array.from(parent.children).filter(function(c) { return c.tagName === el.tagName; });
                if (siblings.length === 1) return tag;
                var index = siblings.indexOf(el) + 1;
                return tag + ':nth-of-type(' + index + ')';
            }

            function isElementVisible(el, rect, style) {
                if (rect.width <= 0 || rect.height <= 0) return false;
                if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
                if (style.pointerEvents === 'none') return false;
                return true;
            }

            function walk(root, inShadow) {
                var elements = root.querySelectorAll ? Array.from(root.querySelectorAll('*')) : [];
                for (var i = 0; i < elements.length; i++) {
                    var el = elements[i];

                    // Pierce Shadow DOM
                    if (el.shadowRoot) {
                        walk(el.shadowRoot, true);
                    }

                    var tag = el.tagName.toLowerCase();
                    var role = (el.getAttribute('role') || tag).toLowerCase();
                    var isNativeInteractive = /^(button|a|input|select|textarea|summary)$/.test(tag);
                    var isAriaInteractive = /^(button|link|checkbox|radio|combobox|menuitem|tab|switch)$/.test(role);
                    var hasClick = el.onclick !== null || el.hasAttribute('onclick');
                    var isFocusable = el.hasAttribute('tabindex') && el.getAttribute('tabindex') !== '-1';

                    if (isNativeInteractive || isAriaInteractive || hasClick || isFocusable) {
                        var rect = el.getBoundingClientRect();
                        var style = window.getComputedStyle(el);

                        if (isElementVisible(el, rect, style)) {
                            var text = (el.innerText || el.value || el.getAttribute('placeholder') || el.getAttribute('aria-label') || '').trim();
                            if (text.length > 80) text = text.substring(0, 77) + '...';

                            var attrs = {};
                            if (el.id) attrs.id = el.id;
                            if (el.getAttribute('name')) attrs.name = el.getAttribute('name');
                            if (el.getAttribute('placeholder')) attrs.placeholder = el.getAttribute('placeholder');
                            if (el.getAttribute('href')) attrs.href = el.getAttribute('href');
                            if (el.getAttribute('data-testid')) attrs.data_testid = el.getAttribute('data-testid');

                            results.push({
                                tag: tag,
                                role: role,
                                name: text,
                                selector: getShortSelector(el),
                                x: Math.round(rect.x),
                                y: Math.round(rect.y),
                                width: Math.round(rect.width),
                                height: Math.round(rect.height),
                                is_shadow_dom: inShadow,
                                attributes: attrs
                            });
                        }
                    }
                }
            }

            walk(document, false);
            return results;
        },

        // Audits broken images, broken links, form accessibility, and page diagnostics
        auditPage: function() {
            var brokenImages = [];
            var images = document.querySelectorAll('img');
            images.forEach(function(img) {
                var src = img.getAttribute('src') || '';
                if (!src || (img.complete && img.naturalWidth === 0)) {
                    brokenImages.push(src ? src : '<img without src>');
                }
            });

            var brokenLinks = [];
            var links = document.querySelectorAll('a');
            links.forEach(function(a) {
                var href = (a.getAttribute('href') || '').trim();
                var text = (a.innerText || a.getAttribute('aria-label') || '').trim();
                if (!href || href === '#' || href.toLowerCase().indexOf('javascript:void') === 0) {
                    brokenLinks.push('"' + text + '" -> href="' + href + '"');
                }
            });

            var formsSummary = [];
            var forms = document.querySelectorAll('form');
            forms.forEach(function(f, idx) {
                var inputs = f.querySelectorAll('input, select, textarea');
                var unlabelled = 0;
                inputs.forEach(function(input) {
                    var hasLabel = input.id && document.querySelector('label[for="' + CSS.escape(input.id) + '"]');
                    var hasAria = input.getAttribute('aria-label') || input.getAttribute('placeholder');
                    if (!hasLabel && !hasAria && input.type !== 'hidden' && input.type !== 'submit') {
                        unlabelled++;
                    }
                });
                var action = f.getAttribute('action') || '(current page)';
                var method = (f.getAttribute('method') || 'GET').toUpperCase();
                formsSummary.push('Form #' + (idx + 1) + ' [' + method + ' ' + action + ']: ' + inputs.length + ' fields' + (unlabelled > 0 ? ' (' + unlabelled + ' missing labels)' : ''));
            });

            var h1Count = document.querySelectorAll('h1').length;
            var title = document.title || '';
            var hasViewport = Boolean(document.querySelector('meta[name="viewport"]'));

            return {
                title: title,
                has_viewport: hasViewport,
                h1_count: h1Count,
                broken_images: brokenImages,
                broken_links: brokenLinks,
                forms_summary: formsSummary,
                total_images: images.length,
                total_links: links.length
            };
        }
    };
    return 'installed';
})()"#;

/// An in-page visual element grounded with computed coordinates and selectors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualElement {
    pub tag: String,
    pub role: String,
    pub name: String,
    pub selector: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_shadow_dom: bool,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

/// Raw audit payload returned by the in-page probe
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RawPageAudit {
    pub title: String,
    pub has_viewport: bool,
    pub h1_count: usize,
    pub broken_images: Vec<String>,
    pub broken_links: Vec<String>,
    pub forms_summary: Vec<String>,
    pub total_images: usize,
    pub total_links: usize,
}

/// Structured Quality Assurance audit report for a web page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaAuditReport {
    pub url: String,
    pub title: String,
    pub pass_count: usize,
    pub warn_count: usize,
    pub error_count: usize,
    pub broken_images: Vec<String>,
    pub broken_links: Vec<String>,
    pub console_errors: Vec<String>,
    pub network_errors: Vec<String>,
    pub forms_summary: Vec<String>,
    pub interactive_count: usize,
    pub recommendations: Vec<String>,
}

/// Native Rust controller managing in-page agent inspection and testing
pub struct PageAgent;

impl PageAgent {
    /// Injects the probe into the active page context
    pub async fn inject_probe(cdp: &CdpClient) -> Result<()> {
        cdp.evaluate_js(PAGE_PROBE_JS).await?;
        Ok(())
    }

    /// Scans the DOM tree including Shadow DOM and returns visible interactive elements with bounding boxes
    pub async fn scan_visual_tree(cdp: &CdpClient) -> Result<Vec<VisualElement>> {
        Self::inject_probe(cdp).await?;

        let res_json = cdp
            .evaluate_js("JSON.stringify(window.__minicode_page_agent.scanInteractables())")
            .await?;

        let elements: Vec<VisualElement> = serde_json::from_str(&res_json).map_err(|e| {
            ToolError::CommandExec(format!("Failed parsing in-page visual elements: {}", e))
        })?;

        Ok(elements)
    }

    /// Runs a comprehensive QA audit on the active page
    pub async fn run_qa_audit(cdp: &CdpClient, url: &str) -> Result<QaAuditReport> {
        Self::inject_probe(cdp).await?;

        let audit_json = cdp
            .evaluate_js("JSON.stringify(window.__minicode_page_agent.auditPage())")
            .await?;

        let raw_audit: RawPageAudit = serde_json::from_str(&audit_json).unwrap_or_default();
        let interactables = Self::scan_visual_tree(cdp).await.unwrap_or_default();

        let mut pass_count = 0;
        let mut warn_count = 0;
        let mut error_count = 0;
        let mut recommendations = Vec::new();

        // 1. Meta / SEO / Structure checks
        if raw_audit.has_viewport {
            pass_count += 1;
        } else {
            warn_count += 1;
            recommendations.push("Add a `<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">` tag for mobile responsiveness.".to_string());
        }

        if raw_audit.h1_count == 1 {
            pass_count += 1;
        } else if raw_audit.h1_count == 0 {
            warn_count += 1;
            recommendations.push("Page has no `<h1>` heading. Add a primary `<h1>` element for accessibility and semantic structure.".to_string());
        } else {
            warn_count += 1;
            recommendations.push(format!(
                "Page has multiple ({}) `<h1>` tags. Ensure only one top-level `<h1>` is used.",
                raw_audit.h1_count
            ));
        }

        // 2. Images check
        if raw_audit.broken_images.is_empty() {
            pass_count += 1;
        } else {
            error_count += raw_audit.broken_images.len();
            recommendations.push(format!(
                "Fix {} broken image(s) that failed to load or have empty `src` attributes.",
                raw_audit.broken_images.len()
            ));
        }

        // 3. Links check
        if raw_audit.broken_links.is_empty() {
            pass_count += 1;
        } else {
            warn_count += raw_audit.broken_links.len();
            recommendations.push(format!("Replace {} empty/placeholder link(s) (href=\"#\" or empty) with valid URLs or `<button>` elements.", raw_audit.broken_links.len()));
        }

        // 4. Runtime Console and Network errors
        let debug_report = cdp.debug_collector().format_report();
        let mut console_errors = Vec::new();
        let mut network_errors = Vec::new();

        for line in debug_report.lines() {
            if line.contains("[ERROR]") || line.contains("[exception]") {
                console_errors.push(line.to_string());
                error_count += 1;
            } else if line.contains("[HTTP 4") || line.contains("[HTTP 5") {
                network_errors.push(line.to_string());
                error_count += 1;
            }
        }

        if console_errors.is_empty() && network_errors.is_empty() {
            pass_count += 1;
        } else {
            if !console_errors.is_empty() {
                recommendations.push(format!(
                    "Resolve {} uncaught runtime JavaScript exception(s).",
                    console_errors.len()
                ));
            }
            if !network_errors.is_empty() {
                recommendations.push(format!(
                    "Fix {} failed HTTP network request(s) (4xx/5xx).",
                    network_errors.len()
                ));
            }
        }

        Ok(QaAuditReport {
            url: url.to_string(),
            title: raw_audit.title,
            pass_count,
            warn_count,
            error_count,
            broken_images: raw_audit.broken_images,
            broken_links: raw_audit.broken_links,
            console_errors,
            network_errors,
            forms_summary: raw_audit.forms_summary,
            interactive_count: interactables.len(),
            recommendations,
        })
    }

    /// Formats the QA audit into an agent-readable Markdown report
    pub fn format_qa_report(report: &QaAuditReport) -> String {
        let mut out = format!("### Website QA Audit Report: {}\n", report.title);
        out.push_str(&format!("**URL:** `{}`\n\n", report.url));

        let status_emoji = if report.error_count == 0 && report.warn_count == 0 {
            "🟢 **HEALTHY**"
        } else if report.error_count == 0 {
            "🟡 **WARNINGS FOUND**"
        } else {
            "🔴 **CRITICAL ISSUES DETECTED**"
        };

        out.push_str(&format!(
            "**Overall Status:** {} (✅ {} passed, ⚠️ {} warnings, ❌ {} errors)\n\n",
            status_emoji, report.pass_count, report.warn_count, report.error_count
        ));

        if !report.recommendations.is_empty() {
            out.push_str("#### Recommended Fixes:\n");
            for (i, rec) in report.recommendations.iter().enumerate() {
                out.push_str(&format!("{}. {}\n", i + 1, rec));
            }
            out.push('\n');
        }

        if !report.console_errors.is_empty() {
            out.push_str("#### Runtime JavaScript Exceptions:\n");
            for err in &report.console_errors {
                out.push_str(&format!("• {}\n", err));
            }
            out.push('\n');
        }

        if !report.network_errors.is_empty() {
            out.push_str("#### Network Loading Failures (4xx / 5xx):\n");
            for err in &report.network_errors {
                out.push_str(&format!("• {}\n", err));
            }
            out.push('\n');
        }

        if !report.broken_images.is_empty() {
            out.push_str("#### Broken Images:\n");
            for img in &report.broken_images {
                out.push_str(&format!("• `{}`\n", img));
            }
            out.push('\n');
        }

        if !report.broken_links.is_empty() {
            out.push_str("#### Empty / Placeholder Links:\n");
            for link in &report.broken_links {
                out.push_str(&format!("• {}\n", link));
            }
            out.push('\n');
        }

        if !report.forms_summary.is_empty() {
            out.push_str("#### Forms & Inputs:\n");
            for f in &report.forms_summary {
                out.push_str(&format!("• {}\n", f));
            }
            out.push('\n');
        }

        out.push_str(&format!(
            "**Interactive Elements:** {} actionable components discovered on page.\n",
            report.interactive_count
        ));

        out
    }

    /// Formats in-page visual elements into a structured spatial Markdown table
    pub fn format_visual_tree_report(elements: &[VisualElement]) -> String {
        let mut out = format!(
            "### In-Page Visual DOM Tree ({} elements)\n\n",
            elements.len()
        );
        out.push_str("| Target Selector | Tag/Role | Text / Label | Coordinates (X, Y, W, H) | Shadow DOM |\n");
        out.push_str("| :--- | :--- | :--- | :--- | :--- |\n");

        for el in elements.iter().take(30) {
            let shadow_badge = if el.is_shadow_dom { "yes" } else { "no" };
            let clean_name = el.name.replace('|', "\\|").replace('\n', " ");
            let label = if clean_name.is_empty() {
                "_(no text)_"
            } else {
                &clean_name
            };
            out.push_str(&format!(
                "| `{}` | `<{}>` ({}) | {} | ({}, {}, {}x{}) | {} |\n",
                el.selector, el.tag, el.role, label, el.x, el.y, el.width, el.height, shadow_badge
            ));
        }

        if elements.len() > 30 {
            out.push_str(&format!(
                "\n_... +{} more visual elements omitted for token efficiency._\n",
                elements.len() - 30
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_probe_js_syntax() {
        assert!(PAGE_PROBE_JS.starts_with("(function()"));
        assert!(PAGE_PROBE_JS.contains("scanInteractables"));
        assert!(PAGE_PROBE_JS.contains("auditPage"));
        assert!(PAGE_PROBE_JS.ends_with("})()"));
    }

    #[test]
    fn test_format_qa_report() {
        let report = QaAuditReport {
            url: "http://localhost:3000".to_string(),
            title: "Test Store".to_string(),
            pass_count: 3,
            warn_count: 1,
            error_count: 1,
            broken_images: vec!["/logo-missing.png".to_string()],
            broken_links: vec!["\"Contact\" -> href=\"#\"".to_string()],
            console_errors: vec![
                "[ERROR] Uncaught TypeError: Cannot read property 'map'".to_string()
            ],
            network_errors: vec!["[HTTP 500] GET /api/cart".to_string()],
            forms_summary: vec!["Form #1 [POST /checkout]: 3 fields".to_string()],
            interactive_count: 12,
            recommendations: vec!["Fix broken image /logo-missing.png".to_string()],
        };

        let formatted = PageAgent::format_qa_report(&report);
        assert!(formatted.contains("CRITICAL ISSUES DETECTED"));
        assert!(formatted.contains("/logo-missing.png"));
        assert!(formatted.contains("Uncaught TypeError"));
        assert!(formatted.contains("[HTTP 500]"));
    }

    #[test]
    fn test_format_visual_tree_report() {
        let elements = vec![VisualElement {
            tag: "button".to_string(),
            role: "button".to_string(),
            name: "Add to Cart".to_string(),
            selector: "#add-to-cart-btn".to_string(),
            x: 100.0,
            y: 200.0,
            width: 150.0,
            height: 40.0,
            is_shadow_dom: false,
            attributes: HashMap::new(),
        }];

        let formatted = PageAgent::format_visual_tree_report(&elements);
        assert!(formatted.contains("#add-to-cart-btn"));
        assert!(formatted.contains("Add to Cart"));
        assert!(formatted.contains("(100, 200, 150x40)"));
    }
}
