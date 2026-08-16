use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An interactive element identified in the page's accessibility tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AriaElement {
    pub ref_id: String, // e.g. "@e1", "@e2"
    pub tag: String,
    pub role: String,
    pub name: String,
    pub attributes: HashMap<String, String>,
}

/// A snapshot of a web page's interactive accessibility tree and text content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageSnapshot {
    pub url: String,
    pub title: String,
    pub interactive_elements: Vec<AriaElement>,
    pub text_summary: String,
}

pub struct BrowserController;

impl BrowserController {
    /// Fetches and parses a web page into a structured ARIA accessibility tree snapshot.
    pub async fn navigate(url: &str) -> Result<PageSnapshot> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .map_err(|e| ToolError::CommandExec(format!("Failed to build HTTP client: {}", e)))?;

        let response = client.get(url).send().await.map_err(|e| {
            ToolError::CommandExec(format!("Failed to connect to '{}': {}", url, e))
        })?;

        let html = response
            .text()
            .await
            .map_err(|e| ToolError::CommandExec(format!("Failed to read response body: {}", e)))?;

        let snapshot = Self::parse_html_to_aria_snapshot(url, &html);
        Ok(snapshot)
    }

    /// Parses raw HTML into an accessible tree with numbered element references.
    pub fn parse_html_to_aria_snapshot(url: &str, html: &str) -> PageSnapshot {
        let title = extract_title(html).unwrap_or_else(|| url.to_string());
        let elements = extract_interactive_elements(html);
        let text_summary = extract_readable_text(html);

        PageSnapshot {
            url: url.to_string(),
            title,
            interactive_elements: elements,
            text_summary,
        }
    }

    /// Formats a snapshot into a clean agent-readable Markdown accessibility report.
    pub fn format_snapshot_report(snapshot: &PageSnapshot) -> String {
        let mut out = format!("🌐 Page: **{}** (`{}`)\n\n", snapshot.title, snapshot.url);

        if !snapshot.interactive_elements.is_empty() {
            out.push_str("🎯 **Interactive Accessibility Tree (ARIA References):**\n");
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

        out.push_str("📄 **Content Text Summary:**\n");
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
                ref_id: format!("@e{}", counter),
                tag: tag_name.to_string(),
                role: default_role.to_string(),
                name: display_name.chars().take(80).collect(),
                attributes: attrs,
            });

            counter += 1;
            search_from = abs_start + tag_content.len();
            if elements.len() >= 50 {
                break; // Keep accessibility tree compact
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
            <head><title>Dashboard &bull; Minicode</title></head>
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
        assert_eq!(snapshot.title, "Dashboard &bull; Minicode");
        assert!(!snapshot.interactive_elements.is_empty());

        let report = BrowserController::format_snapshot_report(&snapshot);
        assert!(report.contains("@e1"));
        assert!(report.contains("Log In"));
        assert!(report.contains("Documentation"));
    }
}
