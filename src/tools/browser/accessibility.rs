use super::AriaElement;
use crate::error::{Result, ToolError};
use std::collections::HashMap;

struct RawElement {
    abs_start: usize,
    tag: String,
    role: String,
    name: String,
    attrs: HashMap<String, String>,
}

/// Manages versioned element references across DOM revisions
#[derive(Debug, Clone)]
pub struct AccessibilityManager {
    revision: u32,
    current_elements: Vec<AriaElement>,
}

impl Default for AccessibilityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AccessibilityManager {
    pub fn new() -> Self {
        Self {
            revision: 1,
            current_elements: Vec::new(),
        }
    }

    pub fn revision(&self) -> u32 {
        self.revision
    }

    pub fn has_elements(&self) -> bool {
        !self.current_elements.is_empty()
    }

    /// Increments the snapshot revision counter (e.g. after navigation or DOM mutation)
    pub fn next_revision(&mut self) -> u32 {
        self.revision += 1;
        self.revision
    }

    /// Synchronizes the accessibility tree directly from `PageAgent`'s live `VisualElement` scan
    /// (which pierces Shadow DOM, filters out invisible elements, and computes scrollable container metrics)
    pub fn sync_from_visual_elements(
        &mut self,
        visual_elements: &[super::VisualElement],
    ) -> Vec<AriaElement> {
        let rev = self.revision;
        let max_elements = 60;
        let mut elements = Vec::with_capacity(visual_elements.len().min(max_elements));

        for (counter, v) in (1..).zip(visual_elements.iter()) {
            let mut attrs = v.attributes.clone();
            if !v.selector.is_empty() {
                attrs
                    .entry("selector".to_string())
                    .or_insert_with(|| v.selector.clone());
            }
            if v.is_shadow_dom {
                attrs.insert("shadow_dom".to_string(), "true".to_string());
            }
            if let Some(ref sd) = v.scroll_data {
                if let Some(scroll_badge) = sd.format_compact() {
                    attrs.insert("scrollable".to_string(), scroll_badge);
                }
            }
            let display_name = if v.name.trim().is_empty() {
                attrs
                    .get("placeholder")
                    .or_else(|| attrs.get("name"))
                    .or_else(|| attrs.get("id"))
                    .cloned()
                    .unwrap_or_else(|| format!("Unnamed {}", v.role))
            } else {
                v.name.clone()
            };

            elements.push(AriaElement {
                ref_id: format!("@v{}:e{}", rev, counter),
                tag: v.tag.clone(),
                role: v.role.clone(),
                name: display_name,
                attributes: attrs,
            });
            if elements.len() >= max_elements {
                break;
            }
        }

        self.current_elements = elements.clone();
        elements
    }

    /// Parses interactive elements from an HTML document or DOM dump and assigns `@v{rev}:e{idx}` references
    pub fn update_from_html(&mut self, html: &str) -> Vec<AriaElement> {
        let elements = Self::extract_elements_for_revision(html, self.revision, 60);
        self.current_elements = elements.clone();
        elements
    }

    /// Parses interactive elements from raw HTML at the specified revision and cap without mutating state.
    pub(crate) fn extract_elements_for_revision(
        html: &str,
        rev: u32,
        max_elements: usize,
    ) -> Vec<AriaElement> {
        let mut raw_elements = Vec::new();

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

                for attr_key in [
                    "name",
                    "type",
                    "href",
                    "placeholder",
                    "id",
                    "aria-label",
                    "title",
                ] {
                    if let Some(val) = extract_attribute(tag_content, attr_key) {
                        attrs.insert(attr_key.to_string(), val);
                    }
                }

                let field_identity = format!(
                    "{} {} {}",
                    attrs.get("name").map(|s| s.as_str()).unwrap_or(""),
                    attrs.get("id").map(|s| s.as_str()).unwrap_or(""),
                    extract_attribute(tag_content, "autocomplete").unwrap_or_default()
                )
                .to_ascii_lowercase();
                let is_sensitive = attrs
                    .get("type")
                    .map(|t| t.eq_ignore_ascii_case("password"))
                    .unwrap_or(false)
                    || extract_attribute(tag_content, "data-mask")
                        .map(|m| m.eq_ignore_ascii_case("true"))
                        .unwrap_or(false)
                    || extract_attribute(tag_content, "data-sensitive")
                        .map(|m| m.eq_ignore_ascii_case("true"))
                        .unwrap_or(false)
                    || [
                        "password",
                        "passwd",
                        "secret",
                        "api_key",
                        "api-key",
                        "apikey",
                        "access_token",
                        "access-token",
                        "cc-number",
                        "cc-csc",
                        "cvv",
                        "ssn",
                    ]
                    .iter()
                    .any(|kw| field_identity.contains(kw));
                if is_sensitive {
                    attrs.insert("masked".to_string(), "true".to_string());
                }
                if let Some(val) =
                    extract_attribute(tag_content, "value").filter(|s| !s.trim().is_empty())
                {
                    attrs.insert(
                        "value".to_string(),
                        if is_sensitive {
                            "[MASKED]".to_string()
                        } else {
                            val.trim().to_string()
                        },
                    );
                }

                let display_name = if !clean_name.is_empty() {
                    if is_sensitive {
                        "[MASKED]".to_string()
                    } else {
                        clean_name
                    }
                } else if let Some(aria_label) =
                    attrs.get("aria-label").filter(|s| !s.trim().is_empty())
                {
                    aria_label.trim().to_string()
                } else if let Some(title) = attrs.get("title").filter(|s| !s.trim().is_empty()) {
                    title.trim().to_string()
                } else if let Some(alt) =
                    extract_attribute(tag_content, "alt").filter(|s| !s.trim().is_empty())
                {
                    alt.trim().to_string()
                } else if let Some(placeholder) =
                    attrs.get("placeholder").filter(|s| !s.trim().is_empty())
                {
                    placeholder.trim().to_string()
                } else if let Some(value) =
                    extract_attribute(tag_content, "value").filter(|s| !s.trim().is_empty())
                {
                    if is_sensitive {
                        "[MASKED]".to_string()
                    } else {
                        value.trim().to_string()
                    }
                } else if let Some(name) = attrs.get("name").filter(|s| !s.trim().is_empty()) {
                    name.trim().to_string()
                } else if let Some(id) = attrs.get("id").filter(|s| !s.trim().is_empty()) {
                    id.trim().to_string()
                } else {
                    format!("Unnamed {}", default_role)
                };

                raw_elements.push(RawElement {
                    abs_start,
                    tag: tag_name.to_string(),
                    role: default_role.to_string(),
                    name: display_name.chars().take(80).collect(),
                    attrs,
                });

                search_from = abs_start + tag_content.len();
            }
        }

        // Sort by document source offset to preserve top-to-bottom reading order
        raw_elements.sort_by_key(|el| el.abs_start);

        let mut elements = Vec::with_capacity(raw_elements.len().min(max_elements));

        for (counter, el) in (1..).zip(raw_elements) {
            elements.push(AriaElement {
                ref_id: format!("@v{}:e{}", rev, counter),
                tag: el.tag,
                role: el.role,
                name: el.name,
                attributes: el.attrs,
            });
            if elements.len() >= max_elements {
                break;
            }
        }

        elements
    }

    /// Validates an element reference and guards against stale revisions
    pub fn resolve_ref(&self, target_ref: &str) -> Result<&AriaElement> {
        let clean_ref = target_ref.trim();

        // 1. Check if the ref contains a revision prefix (@vX:eY)
        if let Some(rev_part) = clean_ref.strip_prefix("@v") {
            if let Some((rev_str, elem_id)) = rev_part.split_once(':') {
                if let Ok(requested_rev) = rev_str.parse::<u32>() {
                    // Check if element with suffix :elem_id exists in current revision
                    if requested_rev == self.revision {
                        let suffix = format!(":{}", elem_id);
                        if let Some(el) = self
                            .current_elements
                            .iter()
                            .find(|el| el.ref_id.ends_with(&suffix))
                        {
                            return Ok(el);
                        }
                    } else if requested_rev < self.revision {
                        // Resilient resolution for sequential/parallel form fills within the same turn:
                        // If active tree has updated elements (@v{self.revision}:) and the element still exists, resolve it
                        let active_prefix = format!("@v{}:", self.revision);
                        let suffix = format!(":{}", elem_id);
                        if let Some(el) = self.current_elements.iter().find(|el| {
                            el.ref_id.starts_with(&active_prefix) && el.ref_id.ends_with(&suffix)
                        }) {
                            tracing::debug!(
                                requested = %clean_ref,
                                resolved = %el.ref_id,
                                "Gracefully resolved element reference across sequential action strokes"
                            );
                            return Ok(el);
                        }
                    }

                    return Err(ToolError::InvalidArguments {
                        name: "browser_action".to_string(),
                        reason: format!(
                            "Stale element reference '{}'. Current page revision is v{}. Please take a fresh browser_snapshot before interacting.",
                            clean_ref, self.revision
                        ),
                    }
                    .into());
                }
            }
        }

        // 2. Look up element by flexible suffix match (@e1 -> @vX:e1)
        if let Some(el) = self.current_elements.iter().find(|el| {
            el.ref_id == clean_ref
                || (clean_ref.starts_with("@e")
                    && el
                        .ref_id
                        .ends_with(clean_ref.strip_prefix('@').unwrap_or("")))
        }) {
            return Ok(el);
        }

        // 3. Numeric index match (e.g. "8" or "[8]" -> 8th element in tree)
        let num_clean = clean_ref.trim_matches(|c| c == '[' || c == ']');
        if let Ok(idx) = num_clean.parse::<usize>() {
            if idx >= 1 && idx <= self.current_elements.len() {
                return Ok(&self.current_elements[idx - 1]);
            }
        }

        // 4. Element ID match (e.g. "#hover-target" or "hover-target")
        let id_clean = clean_ref.strip_prefix('#').unwrap_or(clean_ref);
        if let Some(el) = self
            .current_elements
            .iter()
            .find(|el| el.attributes.get("id").map(|s| s.as_str()) == Some(id_clean))
        {
            return Ok(el);
        }

        // 5. Name or data-testid attribute match
        if let Some(el) = self.current_elements.iter().find(|el| {
            el.attributes.get("name").map(|s| s.as_str()) == Some(clean_ref)
                || el.attributes.get("data-testid").map(|s| s.as_str()) == Some(clean_ref)
        }) {
            return Ok(el);
        }

        // 6. Case-insensitive visible name/label match (e.g. "Hover Me", "Place Order Now")
        if let Some(el) = self
            .current_elements
            .iter()
            .find(|el| el.name.eq_ignore_ascii_case(clean_ref))
        {
            return Ok(el);
        }

        Err(ToolError::InvalidArguments {
            name: "browser_action".to_string(),
            reason: format!(
                "Element reference '{}' not found in current accessibility tree (revision v{} with {} interactive elements)",
                clean_ref,
                self.revision,
                self.current_elements.len()
            ),
        }
        .into())
    }
}

fn extract_attribute(tag_str: &str, attr_name: &str) -> Option<String> {
    let key = format!("{}=\"", attr_name);
    if let Some(start) = tag_str.find(&key) {
        let val_start = start + key.len();
        if let Some(end) = tag_str[val_start..].find('"') {
            return Some(tag_str[val_start..val_start + end].to_string());
        }
    }
    None
}

pub(crate) fn strip_non_text_elements(s: &str, extra_tags: &[&str]) -> String {
    const BASE_TAGS: &[&str] = &["script", "style", "svg", "noscript"];
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::new();
    let mut last_copy = 0;
    let mut i = 0;

    while i < len {
        if bytes[i] == b'<' {
            let after_lt = i + 1;
            let mut matched_tag: Option<&str> = None;
            for &tag in BASE_TAGS.iter().chain(extra_tags.iter()) {
                let t_bytes = tag.as_bytes();
                let end_tag_name = after_lt + t_bytes.len();
                if end_tag_name <= len
                    && bytes[after_lt..end_tag_name].eq_ignore_ascii_case(t_bytes)
                    && bytes
                        .get(end_tag_name)
                        .is_none_or(|&b| b.is_ascii_whitespace() || b == b'>' || b == b'/')
                {
                    matched_tag = Some(tag);
                    break;
                }
            }

            if let Some(tag) = matched_tag {
                if out.is_empty() && last_copy == 0 {
                    out.reserve(len);
                }
                out.push_str(&s[last_copy..i]);
                out.push(' ');

                let t_bytes = tag.as_bytes();
                let mut j = after_lt + t_bytes.len();
                let mut found_close = None;
                while j + 3 + t_bytes.len() <= len {
                    if bytes[j] == b'<'
                        && bytes[j + 1] == b'/'
                        && bytes[j + 2..j + 2 + t_bytes.len()].eq_ignore_ascii_case(t_bytes)
                        && bytes[j + 2 + t_bytes.len()] == b'>'
                    {
                        found_close = Some(j + 3 + t_bytes.len());
                        break;
                    }
                    j += 1;
                }

                if let Some(close_end) = found_close {
                    i = close_end;
                    last_copy = close_end;
                    continue;
                } else {
                    last_copy = len;
                    break;
                }
            }
        }
        i += 1;
    }

    if last_copy == 0 {
        return s.to_string();
    }
    if last_copy < len {
        out.push_str(&s[last_copy..]);
    }
    out
}

pub(crate) fn strip_html_tags(s: &str) -> String {
    let cleaned = strip_non_text_elements(s, &[]);
    let mut out = String::with_capacity(cleaned.len());
    let mut in_tag = false;
    for c in cleaned.chars() {
        if c == '<' {
            in_tag = true;
            out.push(' ');
        } else if c == '>' {
            in_tag = false;
            out.push(' ');
        } else if !in_tag {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Strips HTML tags while preserving line breaks across block-level HTML elements
/// and inserting spaces across inline tag boundaries so adjacent tags never glue words together.
pub(crate) fn strip_html_tags_preserving_blocks(s: &str) -> String {
    const BLOCK_TAGS: &[&str] = &[
        "p",
        "div",
        "section",
        "article",
        "main",
        "header",
        "footer",
        "nav",
        "aside",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "li",
        "ul",
        "ol",
        "tr",
        "table",
        "blockquote",
        "pre",
        "hr",
        "br",
        "dl",
        "dt",
        "dd",
        "figcaption",
        "details",
        "summary",
        "form",
    ];

    let cleaned = strip_non_text_elements(s, &[]);
    let mut out = String::with_capacity(cleaned.len());
    let mut chars = cleaned.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag_buf = String::new();
            for tc in chars.by_ref() {
                if tc == '>' {
                    break;
                }
                tag_buf.push(tc);
            }
            let trimmed = tag_buf.trim().trim_start_matches('/').trim();
            let tag_name: String = trimmed
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
                .map(|ch| ch.to_ascii_lowercase())
                .collect();
            if BLOCK_TAGS.contains(&tag_name.as_str()) {
                out.push('\n');
            } else {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }

    out.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_versioned_ref_generation_and_resolution() {
        let mut mgr = AccessibilityManager::new();
        assert_eq!(mgr.revision(), 1);

        let html = r#"
            <div>
                <button type="submit">Submit Form</button>
                <input type="text" name="username" placeholder="Enter username" />
                <a href="https://github.com" aria-label="GitHub"><svg><title>Internal SVG Title</title></svg></a>
            </div>
        "#;

        let elements = mgr.update_from_html(html);
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0].ref_id, "@v1:e1");
        assert_eq!(elements[1].ref_id, "@v1:e2");
        assert_eq!(elements[2].ref_id, "@v1:e3");
        assert_eq!(elements[2].name, "GitHub");

        // Resolve exact ref
        let el = mgr.resolve_ref("@v1:e1").unwrap();
        assert_eq!(el.name, "Submit Form");

        // Resolve short ref (@e2 -> @v1:e2)
        let el2 = mgr.resolve_ref("@e2").unwrap();
        assert_eq!(el2.attributes.get("name").unwrap(), "username");
    }

    #[test]
    fn test_stale_ref_detection() {
        let mut mgr = AccessibilityManager::new();
        let html = r#"<button>Old Button</button>"#;
        mgr.update_from_html(html);

        // Advance revision
        mgr.next_revision();
        assert_eq!(mgr.revision(), 2);

        // Attempting to resolve a v1 ref on v2 manager should return stale error
        let err = mgr.resolve_ref("@v1:e1");
        assert!(err.is_err());
        let err_msg = err.unwrap_err().to_string();
        assert!(err_msg.contains("Stale element reference"));
        assert!(err_msg.contains("Current page revision is v2"));
    }

    #[test]
    fn test_strip_html_tags_separates_adjacent_elements_and_preserves_blocks() {
        let inline_html = "<span>minicode</span><span>v0.1</span><a>Philosophy</a><a>Swarm</a>";
        assert_eq!(
            strip_html_tags(inline_html),
            "minicode v0.1 Philosophy Swarm"
        );

        let block_html = "<nav><span>minicode</span><span>v0.1</span></nav><section><h1>Rust AI Agent</h1><p>Zero-overhead terminal workflow.</p></section>";
        let preserved = strip_html_tags_preserving_blocks(block_html);
        let lines: Vec<&str> = preserved.lines().collect();
        assert_eq!(
            lines,
            vec![
                "minicode v0.1",
                "Rust AI Agent",
                "Zero-overhead terminal workflow."
            ]
        );
    }

    #[test]
    fn test_sensitive_password_masking_in_html_extraction() {
        let mut mgr = AccessibilityManager::new();
        let html = r#"
            <form>
                <input type="text" name="email" value="alice@example.com" />
                <input type="password" name="password" value="hunter2secret" />
                <input type="text" name="api_token" data-sensitive="true" value="sk_live_12345" />
            </form>
        "#;
        let elements = mgr.update_from_html(html);
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements[0].attributes.get("value").map(|s| s.as_str()),
            Some("alice@example.com")
        );
        assert_eq!(
            elements[1].attributes.get("value").map(|s| s.as_str()),
            Some("[MASKED]")
        );
        assert_eq!(
            elements[1].attributes.get("masked").map(|s| s.as_str()),
            Some("true")
        );
        assert_eq!(
            elements[2].attributes.get("value").map(|s| s.as_str()),
            Some("[MASKED]")
        );
    }

    #[test]
    fn test_sync_from_visual_elements_preserves_scroll_shadow_and_masked() {
        let mut mgr = AccessibilityManager::new();
        mgr.next_revision();
        assert_eq!(mgr.revision(), 2);

        let mut attrs = HashMap::new();
        attrs.insert("ref".to_string(), "@v2:e1".to_string());
        attrs.insert("type".to_string(), "password".to_string());
        attrs.insert("value".to_string(), "[MASKED]".to_string());
        attrs.insert("masked".to_string(), "true".to_string());

        let visual = vec![
            super::super::VisualElement {
                tag: "input".to_string(),
                role: "textbox".to_string(),
                name: "[MASKED]".to_string(),
                selector: "#secret-input".to_string(),
                x: 20.0,
                y: 40.0,
                width: 200.0,
                height: 32.0,
                is_shadow_dom: true,
                is_scrollable: false,
                scroll_data: None,
                attributes: attrs,
            },
            super::super::VisualElement {
                tag: "div".to_string(),
                role: "scrollable".to_string(),
                name: "Logs Container".to_string(),
                selector: "#logs-panel".to_string(),
                x: 0.0,
                y: 100.0,
                width: 600.0,
                height: 400.0,
                is_shadow_dom: false,
                is_scrollable: true,
                scroll_data: Some(super::super::ScrollData {
                    top: 120.0,
                    bottom: 480.0,
                    left: 0.0,
                    right: 0.0,
                }),
                attributes: HashMap::new(),
            },
        ];

        let synced = mgr.sync_from_visual_elements(&visual);
        assert_eq!(synced.len(), 2);
        assert!(mgr.has_elements());

        let first = mgr.resolve_ref("@v2:e1").unwrap();
        assert_eq!(first.name, "[MASKED]");
        assert_eq!(
            first.attributes.get("selector").map(|s| s.as_str()),
            Some("#secret-input")
        );
        assert_eq!(
            first.attributes.get("shadow_dom").map(|s| s.as_str()),
            Some("true")
        );

        let second = mgr.resolve_ref("@e2").unwrap();
        assert_eq!(second.ref_id, "@v2:e2");
        assert_eq!(
            second.attributes.get("scrollable").map(|s| s.as_str()),
            Some("↑120px ↓480px")
        );
    }
}
