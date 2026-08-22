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

    /// Increments the snapshot revision counter (e.g. after navigation or DOM mutation)
    pub fn next_revision(&mut self) -> u32 {
        self.revision += 1;
        self.revision
    }

    /// Parses interactive elements from an HTML document or DOM dump and assigns `@v{rev}:e{idx}` references
    pub fn update_from_html(&mut self, html: &str) -> Vec<AriaElement> {
        let rev = self.revision;
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

                let clean_name = strip_tags(tag_content);
                let mut attrs = HashMap::new();

                if let Some(name_attr) = extract_attribute(tag_content, "name") {
                    attrs.insert("name".to_string(), name_attr);
                }
                if let Some(type_attr) = extract_attribute(tag_content, "type") {
                    attrs.insert("type".to_string(), type_attr);
                }
                if let Some(href_attr) = extract_attribute(tag_content, "href") {
                    attrs.insert("href".to_string(), href_attr);
                }
                if let Some(placeholder) = extract_attribute(tag_content, "placeholder") {
                    attrs.insert("placeholder".to_string(), placeholder);
                }
                if let Some(id_attr) = extract_attribute(tag_content, "id") {
                    attrs.insert("id".to_string(), id_attr);
                }

                let display_name = if !clean_name.is_empty() {
                    clean_name
                } else if let Some(placeholder) = attrs.get("placeholder") {
                    placeholder.clone()
                } else if let Some(name) = attrs.get("name") {
                    name.clone()
                } else if let Some(id) = attrs.get("id") {
                    id.clone()
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

        let mut elements = Vec::new();
        let mut counter = 1;

        for el in raw_elements {
            elements.push(AriaElement {
                ref_id: format!("@v{}:e{}", rev, counter),
                tag: el.tag,
                role: el.role,
                name: el.name,
                attributes: el.attrs,
            });
            counter += 1;
            if elements.len() >= 60 {
                break;
            }
        }

        self.current_elements = elements.clone();
        elements
    }

    /// Validates an element reference and guards against stale revisions
    pub fn resolve_ref(&self, target_ref: &str) -> Result<&AriaElement> {
        let clean_ref = target_ref.trim();

        // Check if the ref contains a revision prefix (@vX:eY)
        if let Some(rev_part) = clean_ref.strip_prefix("@v") {
            if let Some((rev_str, _)) = rev_part.split_once(':') {
                if let Ok(requested_rev) = rev_str.parse::<u32>() {
                    if requested_rev != self.revision {
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
        }

        // Look up element by exact ref_id or flexible suffix match (@e1 -> @vX:e1)
        self.current_elements
            .iter()
            .find(|el| {
                el.ref_id == clean_ref
                    || (clean_ref.starts_with("@e")
                        && el.ref_id.ends_with(clean_ref.strip_prefix('@').unwrap_or("")))
            })
            .ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "browser_action".to_string(),
                    reason: format!(
                        "Element reference '{}' not found in current accessibility tree (revision v{} with {} interactive elements)",
                        clean_ref,
                        self.revision,
                        self.current_elements.len()
                    ),
                }
                .into()
            })
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

fn strip_tags(s: &str) -> String {
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
            </div>
        "#;

        let elements = mgr.update_from_html(html);
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].ref_id, "@v1:e1");
        assert_eq!(elements[1].ref_id, "@v1:e2");

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
}
