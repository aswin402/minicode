use crate::context::okf::{
    is_stale, trust_tier, write_concept_doc, OkfDocument, OkfFrontmatter, OkfManager, OkfSource,
    OkfVerification,
};
use crate::error::{Result, ToolError};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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

/// Optional OKF v0.2 extended metadata when writing a wiki/bundle concept document.
#[derive(Debug, Clone, Default)]
pub struct WikiOkfOptions {
    pub concept_type: Option<String>,
    pub description: Option<String>,
    pub resource: Option<String>,
    pub status: Option<String>,
    pub stale_after: Option<String>,
    pub verified_by: Option<String>,
    pub runtime: Option<String>,
    pub computation: Option<String>,
    pub parameters: Vec<crate::context::okf::OkfParameter>,
    pub executor: Option<crate::context::okf::OkfExecutor>,
    pub attester: Option<crate::context::okf::OkfAttester>,
    pub enforce_augmentation_guard: bool,
    pub bundle_dir: Option<PathBuf>,
}

pub struct WikiManager;

impl WikiManager {
    /// Returns the directory path `.minicode/wiki` in the workspace.
    pub fn wiki_dir(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".minicode").join("wiki")
    }

    /// Creates or updates a structured OKF v0.2 knowledge wiki document in `.minicode/wiki/<topic>.md`.
    pub fn write_entry(
        workspace_root: &Path,
        topic: &str,
        title: &str,
        content: &str,
        tags: &[String],
        references: &[String],
    ) -> Result<String> {
        Self::write_entry_with_okf(
            workspace_root,
            topic,
            title,
            content,
            tags,
            references,
            WikiOkfOptions::default(),
        )
    }

    /// Creates or updates an OKF v0.2 concept document with full provenance, trust, lifecycle, and attestation support.
    pub fn write_entry_with_okf(
        workspace_root: &Path,
        topic: &str,
        title: &str,
        content: &str,
        tags: &[String],
        references: &[String],
        opts: WikiOkfOptions,
    ) -> Result<String> {
        let concept_id = sanitize_concept_id(topic);
        if concept_id.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "wiki_write".to_string(),
                reason: "Topic / concept_id cannot be empty".to_string(),
            }
            .into());
        }

        let dir = opts
            .bundle_dir
            .clone()
            .unwrap_or_else(|| Self::wiki_dir(workspace_root));
        fs::create_dir_all(&dir).map_err(|e| ToolError::FileOp {
            path: dir.display().to_string(),
            source: e,
        })?;

        let now_iso = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let sources: Vec<OkfSource> = references
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.trim().is_empty())
            .map(|(idx, r)| OkfSource {
                id: Some(format!("ref-{}", idx + 1)),
                resource: r.trim().to_string(),
                title: None,
                author: None,
                usage_count: None,
                last_modified: None,
            })
            .collect();

        let verified_events = if let Some(ref by) = opts.verified_by {
            if !by.trim().is_empty() {
                vec![OkfVerification {
                    by: by.trim().to_string(),
                    at: now_iso.clone(),
                }]
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let first_sentence = opts.description.clone().unwrap_or_else(|| {
            content
                .lines()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("```"))
                .unwrap_or(title)
                .chars()
                .take(140)
                .collect::<String>()
        });

        let fm = OkfFrontmatter {
            concept_type: opts.concept_type.unwrap_or_else(|| "Concept".to_string()),
            title: Some(title.to_string()),
            description: Some(first_sentence),
            resource: opts.resource,
            tags: tags.to_vec(),
            sources,
            generated: None, // Automatically populated by write_concept_doc
            verified: verified_events.first().cloned(),
            verified_events,
            status: Some(opts.status.unwrap_or_else(|| "stable".to_string())),
            superseded_by: None,
            stale_after: opts.stale_after,
            usage_window: None,
            runtime: opts.runtime,
            parameters: opts.parameters,
            computation: opts.computation,
            executor: opts.executor,
            attester: opts.attester,
            extensions: indexmap::IndexMap::new(),
        };

        let trimmed_content = content.trim();
        let body = if trimmed_content.starts_with("# ") {
            format!("{}\n", trimmed_content)
        } else {
            format!("# {}\n\n{}\n", title, trimmed_content)
        };

        let outcome = write_concept_doc(
            &dir,
            &concept_id,
            fm,
            &body,
            opts.enforce_augmentation_guard,
        )?;

        // Regenerate OKF v0.2 recursive index.md and append to log.md
        OkfManager::generate_index_md(&dir)?;
        OkfManager::append_log_entry(
            &dir,
            &format!("minicode/v{}", env!("CARGO_PKG_VERSION")),
            "Update",
            &outcome.relative_path,
            &format!("Updated OKF concept `{}` ({})", title, outcome.concept_id),
        )?;

        Ok(format!(
            "✔ Successfully saved OKF v0.2 knowledge concept: `{}` ({}/{})",
            title,
            dir.strip_prefix(workspace_root).unwrap_or(&dir).display(),
            outcome.relative_path
        ))
    }

    /// Reads a knowledge wiki / OKF concept document by topic or concept_id.
    pub fn read_entry(workspace_root: &Path, topic: &str) -> Result<String> {
        Self::read_entry_from_bundle(workspace_root, topic, None)
    }

    /// Reads an OKF concept document from `.minicode/wiki` (or a specified bundle directory),
    /// enriching the header with derived OKF v0.2 trust tier and freshness status.
    pub fn read_entry_from_bundle(
        workspace_root: &Path,
        topic: &str,
        bundle_dir: Option<&Path>,
    ) -> Result<String> {
        let concept_id = sanitize_concept_id(topic);
        let dir = bundle_dir
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| Self::wiki_dir(workspace_root));
        let file_path = dir.join(format!("{}.md", concept_id));

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

        if let Ok((Some(fm), _)) = OkfDocument::parse(&content) {
            let tier = trust_tier(&fm);
            let stale = is_stale(&fm, None);
            let freshness = if stale {
                format!(
                    "STALE (since {})",
                    fm.stale_after.as_deref().unwrap_or("expired")
                )
            } else {
                "fresh".to_string()
            };
            let status = fm.status.as_deref().unwrap_or("stable");
            return Ok(format!(
                "[OKF v0.2 Concept: `{}` | Type: `{}` | Status: `{}` | Trust Tier: `{}` | Freshness: `{}`]\n\n{}",
                concept_id, fm.concept_type, status, tier, freshness, content
            ));
        }

        Ok(content)
    }

    /// Searches wiki & OKF knowledge entries matching keywords in concept_id, title, tags, or content.
    pub fn search_entries(workspace_root: &Path, query: &str) -> Result<String> {
        let query_lower = query.to_lowercase();
        let dir = Self::wiki_dir(workspace_root);

        if !dir.exists() {
            return Ok("ℹ No wiki knowledge entries found in repository yet.".to_string());
        }

        let mut md_files = Vec::new();
        collect_md_recursive(&dir, &mut md_files);
        md_files.sort();

        let mut matches = Vec::new();
        for path in md_files {
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if fname == "index.md" || fname == "log.md" {
                continue;
            }
            let rel_id = path
                .strip_prefix(&dir)
                .unwrap_or(&path)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");

            if let Ok(content) = fs::read_to_string(&path) {
                let content_lower = content.to_lowercase();
                if rel_id.to_lowercase().contains(&query_lower)
                    || content_lower.contains(&query_lower)
                {
                    let (fm_opt, body) = OkfDocument::parse(&content).unwrap_or((None, &content));
                    let title = fm_opt
                        .as_ref()
                        .and_then(|f| f.title.clone())
                        .or_else(|| {
                            body.lines()
                                .find(|l| l.starts_with("# "))
                                .map(|l| l.trim_start_matches("# ").trim().to_string())
                        })
                        .unwrap_or_else(|| rel_id.clone());
                    let badge = if let Some(ref fm) = fm_opt {
                        let tier = trust_tier(fm);
                        let stale_flag = if is_stale(fm, None) { ", STALE" } else { "" };
                        format!(" [{} | {}{}]", fm.concept_type, tier, stale_flag)
                    } else {
                        String::new()
                    };
                    matches.push((rel_id, title, badge, body.to_string()));
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
            "🔍 Wiki & OKF Search Results for \"{}\" ({} found):\n\n",
            query,
            matches.len()
        );
        for (idx, (topic, title, badge, body)) in matches.iter().enumerate() {
            let preview = body
                .lines()
                .filter(|l| !l.starts_with("---") && !l.starts_with("# "))
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(150)
                .collect::<String>();

            out.push_str(&format!(
                "{}. **{}** (`{}`){}\n   _{}..._\n\n",
                idx + 1,
                title,
                topic,
                badge,
                preview.trim()
            ));
        }

        Ok(out)
    }

    /// Rebuilds `.minicode/wiki/index.md` (and any subdirectory indexes) using OKF v0.2 bottom-up indexing.
    pub fn rebuild_index(workspace_root: &Path) -> Result<()> {
        let dir = Self::wiki_dir(workspace_root);
        if !dir.exists() {
            return Ok(());
        }
        OkfManager::generate_index_md(&dir)?;
        Ok(())
    }
}

fn collect_md_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_md_recursive(&p, out);
        } else if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("md") {
            out.push(p);
        }
    }
}

/// Sanitizes a topic or slash-separated concept_id into safe segments (`tables/events_`, `architecture-overview`).
fn sanitize_concept_id(topic: &str) -> String {
    let trimmed = topic.trim().trim_end_matches(".md");
    let segments: Vec<String> = trimmed
        .split('/')
        .map(|seg| {
            seg.trim()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                        c.to_ascii_lowercase()
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .filter(|s| !s.is_empty() && s != "." && s != "..")
        .collect();
    segments.join("/")
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

        // 2. Read Entry (verifies OKF v0.2 header & content)
        let doc = WikiManager::read_entry(ws, "architecture-overview").unwrap();
        assert!(doc.contains("Architecture Overview"));
        assert!(doc.contains("Ratatui"));
        assert!(doc.contains("Trust Tier: `unverified`"));

        // 3. Search Entry
        let search_res = WikiManager::search_entries(ws, "Tokio").unwrap();
        assert!(search_res.contains("Architecture Overview"));

        // 4. Verify OKF v0.2 index.md and log.md were generated
        assert!(ws.join(".minicode/wiki/index.md").exists());
        assert!(ws.join(".minicode/wiki/log.md").exists());
    }
}
