use crate::error::{ContextError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Provenance source record in OKF v0.2
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OkfSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub resource: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

/// Generation metadata record in OKF v0.2
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfGenerated {
    pub by: String,
    pub at: String,
}

/// Trust and verification record in OKF v0.2
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfVerification {
    pub by: String,
    pub at: String,
}

/// YAML Frontmatter metadata container for OKF v0.2 concept documents
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfFrontmatter {
    #[serde(rename = "type")]
    pub concept_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<OkfSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated: Option<OkfGenerated>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<OkfVerification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>, // "active", "deprecated", "superseded"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

impl Default for OkfFrontmatter {
    fn default() -> Self {
        Self {
            concept_type: "Concept".to_string(),
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            sources: Vec::new(),
            generated: Some(OkfGenerated {
                by: format!("minicode/v{}", env!("CARGO_PKG_VERSION")),
                at: Utc::now().to_rfc3339(),
            }),
            verified: None,
            status: Some("active".to_string()),
            superseded_by: None,
        }
    }
}

/// Parser and serializer for OKF documents
pub struct OkfDocument;

impl OkfDocument {
    /// Parses an OKF document splitting YAML frontmatter and markdown body
    pub fn parse(content: &str) -> Result<(Option<OkfFrontmatter>, &str)> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Ok((None, content));
        }

        let rest = &trimmed[3..];
        let end_idx = match rest.find("\n---") {
            Some(idx) => idx,
            None => return Ok((None, content)),
        };

        let yaml_str = &rest[..end_idx].trim();
        let body = rest[end_idx + 4..].trim_start_matches('\n');

        let frontmatter: OkfFrontmatter = match serde_yaml_from_str(yaml_str) {
            Ok(fm) => fm,
            Err(_) => return Ok((None, content)),
        };

        Ok((Some(frontmatter), body))
    }

    /// Serializes an OKF frontmatter and body into compliant UTF-8 markdown
    #[allow(dead_code)]
    pub fn serialize(frontmatter: &OkfFrontmatter, body: &str) -> String {
        let yaml_str = serde_yaml_to_string(frontmatter).unwrap_or_default();
        format!("---\n{}---\n\n{}", yaml_str, body.trim_start())
    }
}

/// Helper simple YAML serializer for OKF frontmatter
#[allow(dead_code)]
fn serde_yaml_to_string(fm: &OkfFrontmatter) -> Result<String> {
    let mut out = String::new();
    out.push_str(&format!("type: {}\n", fm.concept_type));
    if let Some(t) = &fm.title {
        out.push_str(&format!("title: \"{}\"\n", t.replace('"', "\\\"")));
    }
    if let Some(d) = &fm.description {
        out.push_str(&format!("description: \"{}\"\n", d.replace('"', "\\\"")));
    }
    if let Some(r) = &fm.resource {
        out.push_str(&format!("resource: {}\n", r));
    }
    if !fm.tags.is_empty() {
        out.push_str(&format!("tags: [{}]\n", fm.tags.join(", ")));
    }
    if let Some(g) = &fm.generated {
        out.push_str(&format!(
            "generated: {{ by: \"{}\", at: \"{}\" }}\n",
            g.by, g.at
        ));
    }
    if let Some(v) = &fm.verified {
        out.push_str(&format!(
            "verified: {{ by: \"{}\", at: \"{}\" }}\n",
            v.by, v.at
        ));
    }
    if let Some(s) = &fm.status {
        out.push_str(&format!("status: {}\n", s));
    }
    if let Some(sb) = &fm.superseded_by {
        out.push_str(&format!("superseded_by: {}\n", sb));
    }
    Ok(out)
}

/// Helper simple YAML parser for OKF frontmatter
fn serde_yaml_from_str(s: &str) -> Result<OkfFrontmatter> {
    let mut fm = OkfFrontmatter::default();
    let mut type_found = false;

    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim().trim_matches('"').trim_matches('\'');

            match key {
                "type" => {
                    fm.concept_type = val.to_string();
                    type_found = true;
                }
                "title" => fm.title = Some(val.to_string()),
                "description" => fm.description = Some(val.to_string()),
                "resource" => fm.resource = Some(val.to_string()),
                "status" => fm.status = Some(val.to_string()),
                "superseded_by" => fm.superseded_by = Some(val.to_string()),
                "tags" => {
                    let cleaned = val.trim_start_matches('[').trim_end_matches(']');
                    fm.tags = cleaned
                        .split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    if type_found {
        Ok(fm)
    } else {
        Err(
            ContextError::Okf("Missing required OKF 'type' field in frontmatter".to_string())
                .into(),
        )
    }
}

/// Manager for generating and maintaining OKF knowledge bundles and ledgers
pub struct OkfManager;

impl OkfManager {
    /// Scans an `onpkg_docs/` directory and synthesizes an `index.md` progressive disclosure listing
    pub fn generate_index_md(docs_dir: &Path) -> Result<String> {
        if !docs_dir.exists() {
            fs::create_dir_all(docs_dir).ok();
        }

        let mut entries = Vec::new();

        if let Ok(dir_entries) = fs::read_dir(docs_dir) {
            for entry in dir_entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                        // Skip reserved filenames in concept listings
                        if fname == "index.md" || fname == "log.md" || !fname.ends_with(".md") {
                            continue;
                        }

                        let content = fs::read_to_string(&path).unwrap_or_default();
                        let (fm_opt, _) = OkfDocument::parse(&content)?;

                        let title = fm_opt
                            .as_ref()
                            .and_then(|f| f.title.clone())
                            .unwrap_or_else(|| fname.trim_end_matches(".md").to_string());
                        let concept_type = fm_opt
                            .as_ref()
                            .map(|f| f.concept_type.clone())
                            .unwrap_or_else(|| "Document".to_string());
                        let desc = fm_opt
                            .as_ref()
                            .and_then(|f| f.description.clone())
                            .unwrap_or_else(|| "No description provided.".to_string());

                        entries.push((fname.to_string(), title, concept_type, desc));
                    }
                }
            }
        }

        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut out = String::new();
        out.push_str("---\ntype: Directory Index\ntitle: Knowledge Catalog Index\n---\n\n");
        out.push_str("# Knowledge Catalog Index 📚\n\n");
        out.push_str("> Generated according to Open Knowledge Format (OKF v0.2).\n\n");
        out.push_str("| Concept | Type | Description |\n");
        out.push_str("| :--- | :--- | :--- |\n");

        for (fname, title, ctype, desc) in &entries {
            out.push_str(&format!(
                "| [**{}**](./{}) | `{}` | {} |\n",
                title, fname, ctype, desc
            ));
        }

        out.push_str("\n---\n*Last synchronized:* `");
        out.push_str(&Utc::now().to_rfc3339());
        out.push_str("`\n");

        let index_path = docs_dir.join("index.md");
        fs::write(index_path, &out).ok();

        Ok(out)
    }

    /// Appends a chronological record to `log.md`
    pub fn append_log_entry(
        docs_dir: &Path,
        actor: &str,
        action: &str,
        target_file: &str,
        summary: &str,
    ) -> Result<()> {
        if !docs_dir.exists() {
            fs::create_dir_all(docs_dir).ok();
        }

        let log_path = docs_dir.join("log.md");
        let mut existing = if log_path.exists() {
            fs::read_to_string(&log_path).unwrap_or_default()
        } else {
            "---\ntype: Audit Log\ntitle: Knowledge Evolution Ledger\n---\n\n# Knowledge Evolution Ledger 📜\n\n| Timestamp (UTC) | Actor | Action | Target | Summary |\n| :--- | :--- | :--- | :--- | :--- |\n".to_string()
        };

        let timestamp = Utc::now().to_rfc3339();
        let entry = format!(
            "| `{}` | `{}` | **{}** | `{}` | {} |\n",
            timestamp, actor, action, target_file, summary
        );

        existing.push_str(&entry);
        fs::write(log_path, existing).ok();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_okf_document_parse_and_serialize() {
        let content = r#"---
type: PRD
title: "Product Requirements Document"
description: "Core agent architecture specifications"
tags: [architecture, agent, rust]
status: active
---

# Product Requirements Document
This is the markdown body.
"#;

        let (fm, body) = OkfDocument::parse(content).unwrap();
        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert_eq!(fm.concept_type, "PRD");
        assert_eq!(fm.title.as_deref(), Some("Product Requirements Document"));
        assert_eq!(fm.tags, vec!["architecture", "agent", "rust"]);
        assert!(body.contains("This is the markdown body."));

        let serialized = OkfDocument::serialize(&fm, body);
        assert!(serialized.starts_with("---\n"));
        assert!(serialized.contains("type: PRD"));
        assert!(serialized.contains("This is the markdown body."));
    }
}
