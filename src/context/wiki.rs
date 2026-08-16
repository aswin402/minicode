use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Metadata frontmatter for a persistent Markdown wiki knowledge document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiEntryMeta {
    pub topic: String,
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub references: Vec<String>,
    pub updated_at: u64,
}

pub struct WikiManager;

impl WikiManager {
    /// Returns the directory path `.minicode/wiki` in the workspace.
    pub fn wiki_dir(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".minicode").join("wiki")
    }

    /// Creates or updates a structured knowledge wiki document in `.minicode/wiki/<topic>.md`.
    pub fn write_entry(
        workspace_root: &Path,
        topic: &str,
        title: &str,
        content: &str,
        tags: &[String],
        references: &[String],
    ) -> Result<String> {
        let topic_clean = sanitize_topic(topic);
        if topic_clean.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "wiki_write".to_string(),
                reason: "Topic name cannot be empty".to_string(),
            }
            .into());
        }

        let dir = Self::wiki_dir(workspace_root);
        fs::create_dir_all(&dir).map_err(|e| ToolError::FileOp {
            path: dir.display().to_string(),
            source: e,
        })?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        let meta = WikiEntryMeta {
            topic: topic_clean.clone(),
            title: title.to_string(),
            tags: tags.to_vec(),
            references: references.to_vec(),
            updated_at: now,
        };

        let frontmatter_yaml = serde_yaml_compact(&meta);
        let doc_content = format!(
            "---\n{}\n---\n\n# {}\n\n{}\n",
            frontmatter_yaml,
            title,
            content.trim()
        );

        let file_path = dir.join(format!("{}.md", topic_clean));
        fs::write(&file_path, &doc_content).map_err(|e| ToolError::FileOp {
            path: file_path.display().to_string(),
            source: e,
        })?;

        // Automatically refresh index.md
        Self::rebuild_index(workspace_root)?;

        Ok(format!(
            "✔ Successfully saved knowledge wiki entry: `{}` (.minicode/wiki/{}.md)",
            title, topic_clean
        ))
    }

    /// Reads a knowledge wiki document by topic.
    pub fn read_entry(workspace_root: &Path, topic: &str) -> Result<String> {
        let topic_clean = sanitize_topic(topic);
        let dir = Self::wiki_dir(workspace_root);
        let file_path = dir.join(format!("{}.md", topic_clean));

        if !file_path.exists() {
            return Err(ToolError::NotFound {
                name: format!("wiki:{}", topic),
            }
            .into());
        }

        let content = fs::read_to_string(&file_path).map_err(|e| ToolError::FileOp {
            path: file_path.display().to_string(),
            source: e,
        })?;

        Ok(content)
    }

    /// Searches wiki knowledge entries matching keywords in topic, title, tags, or content.
    pub fn search_entries(workspace_root: &Path, query: &str) -> Result<String> {
        let query_lower = query.to_lowercase();
        let dir = Self::wiki_dir(workspace_root);

        if !dir.exists() {
            return Ok("ℹ No wiki knowledge entries found in repository yet.".to_string());
        }

        let mut matches = Vec::new();
        let entries = fs::read_dir(&dir).map_err(|e| ToolError::FileOp {
            path: dir.display().to_string(),
            source: e,
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let filename = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                if filename == "index" {
                    continue;
                }

                if let Ok(content) = fs::read_to_string(&path) {
                    let content_lower = content.to_lowercase();
                    if filename.to_lowercase().contains(&query_lower)
                        || content_lower.contains(&query_lower)
                    {
                        // Extract first heading
                        let title = content
                            .lines()
                            .find(|l| l.starts_with("# "))
                            .map(|l| l.trim_start_matches("# ").trim())
                            .unwrap_or(filename);

                        matches.push((filename.to_string(), title.to_string(), content));
                    }
                }
            }
        }

        if matches.is_empty() {
            return Ok(format!(
                "ℹ No wiki entries found matching query: \"{}\"",
                query
            ));
        }

        let mut out = format!(
            "🔍 Wiki Search Results for \"{}\" ({} found):\n\n",
            query,
            matches.len()
        );
        for (idx, (topic, title, content)) in matches.iter().enumerate() {
            let preview = content
                .lines()
                .filter(|l| !l.starts_with("---") && !l.starts_with("# "))
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(150)
                .collect::<String>();

            out.push_str(&format!(
                "{}. **{}** (`{}`)\n   _{}..._\n\n",
                idx + 1,
                title,
                topic,
                preview.trim()
            ));
        }

        Ok(out)
    }

    /// Rebuilds `.minicode/wiki/index.md` cataloging all knowledge entries.
    pub fn rebuild_index(workspace_root: &Path) -> Result<()> {
        let dir = Self::wiki_dir(workspace_root);
        if !dir.exists() {
            return Ok(());
        }

        let mut out = "# Repository Knowledge Wiki Index 📚\n\n".to_string();
        out.push_str("Persistent catalog of architectural decisions, component mappings, and domain rules.\n\n");

        let entries = fs::read_dir(&dir).map_err(|e| ToolError::FileOp {
            path: dir.display().to_string(),
            source: e,
        })?;

        let mut items = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let filename = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                if filename == "index" {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(&path) {
                    let title = content
                        .lines()
                        .find(|l| l.starts_with("# "))
                        .map(|l| l.trim_start_matches("# ").trim())
                        .unwrap_or(filename);
                    items.push((filename.to_string(), title.to_string()));
                }
            }
        }

        items.sort_by(|a, b| a.0.cmp(&b.0));
        for (topic, title) in items {
            out.push_str(&format!("• [**{}**]({}.md) — `{}`\n", title, topic, topic));
        }

        let index_path = dir.join("index.md");
        fs::write(&index_path, out).map_err(|e| ToolError::FileOp {
            path: index_path.display().to_string(),
            source: e,
        })?;

        Ok(())
    }
}

/// Helper to sanitize topic filenames to alphanumeric and hyphens/underscores.
fn sanitize_topic(topic: &str) -> String {
    topic
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Lightweight YAML serializer for frontmatter without heavy external dependencies.
fn serde_yaml_compact(meta: &WikiEntryMeta) -> String {
    let mut out = format!("topic: \"{}\"\n", meta.topic);
    out.push_str(&format!("title: \"{}\"\n", meta.title));
    out.push_str(&format!("updated_at: {}\n", meta.updated_at));
    if !meta.tags.is_empty() {
        out.push_str("tags:\n");
        for tag in &meta.tags {
            out.push_str(&format!("  - \"{}\"\n", tag));
        }
    }
    if !meta.references.is_empty() {
        out.push_str("references:\n");
        for r in &meta.references {
            out.push_str(&format!("  - \"{}\"\n", r));
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_wiki_write_read_and_search() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        // 1. Write Entry
        let res = WikiManager::write_entry(
            ws,
            "architecture-overview",
            "Architecture Overview",
            "This repository uses Tokio and Ratatui for TUI streaming.",
            &["rust".to_string(), "tui".to_string()],
            &["src/main.rs".to_string()],
        )
        .unwrap();

        assert!(res.contains("architecture-overview"));

        // 2. Read Entry
        let doc = WikiManager::read_entry(ws, "architecture-overview").unwrap();
        assert!(doc.contains("Architecture Overview"));
        assert!(doc.contains("Ratatui"));

        // 3. Search Entry
        let search_res = WikiManager::search_entries(ws, "Tokio").unwrap();
        assert!(search_res.contains("Architecture Overview"));
    }
}
