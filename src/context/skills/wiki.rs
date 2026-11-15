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

        let (outgoing_links, incoming_backlinks) =
            crate::context::okf::concept_neighbors(&dir, &concept_id);
        let graph_footer = if !outgoing_links.is_empty() || !incoming_backlinks.is_empty() {
            let mut footer = String::from("\n\n---\n## Knowledge Graph Links\n");
            if !outgoing_links.is_empty() {
                let formatted: Vec<String> = outgoing_links
                    .iter()
                    .map(|id| format!("`[[{id}]]`"))
                    .collect();
                footer.push_str(&format!("- **Links To:** {}\n", formatted.join(", ")));
            }
            if !incoming_backlinks.is_empty() {
                let formatted: Vec<String> = incoming_backlinks
                    .iter()
                    .map(|id| format!("`[[{id}]]`"))
                    .collect();
                footer.push_str(&format!("- **Referenced By:** {}\n", formatted.join(", ")));
            }
            footer
        } else {
            String::new()
        };

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
                "[OKF v0.2 Concept: `{}` | Type: `{}` | Status: `{}` | Trust Tier: `{}` | Freshness: `{}`]\n\n{}{}",
                concept_id, fm.concept_type, status, tier, freshness, content, graph_footer
            ));
        }

        Ok(format!("{content}{graph_footer}"))
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

    /// Karpathy / `OpenKB` `raw/` -> `wiki/sources/` ingestion compiler:
    /// Scans immutable raw artifacts (`<bundle_dir>/raw`, `.minicode/raw`, or custom `raw_dir_override`)
    /// and compiles them into OKF v0.2 `Source Note` concepts (`sources/<slug>.md`) with provenance
    /// `sources`, `machine-confirmed` verification, `PageIndex` outline headers, and auto-linked
    /// `[[concept_id|Title]]` bidirectional wikilinks to existing wiki concepts.
    pub fn ingest_raw_sources(
        workspace_root: &Path,
        bundle_dir: Option<&Path>,
        raw_dir_override: Option<&Path>,
    ) -> Result<WikiIngestReport> {
        let target_bundle = bundle_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| Self::wiki_dir(workspace_root));

        let raw_dir = if let Some(override_dir) = raw_dir_override {
            override_dir.to_path_buf()
        } else if target_bundle.join("raw").is_dir() {
            target_bundle.join("raw")
        } else {
            workspace_root.join(".minicode").join("raw")
        };

        let raw_display = raw_dir
            .strip_prefix(workspace_root)
            .unwrap_or(&raw_dir)
            .to_string_lossy()
            .replace('\\', "/");
        let bundle_display = target_bundle
            .strip_prefix(workspace_root)
            .unwrap_or(&target_bundle)
            .to_string_lossy()
            .replace('\\', "/");

        if !raw_dir.exists() {
            return Ok(WikiIngestReport {
                raw_dir: raw_display,
                bundle_dir: bundle_display,
                ingested_count: 0,
                skipped_count: 0,
                created_concepts: Vec::new(),
                auto_linked_edges: 0,
            });
        }

        // Collect existing concepts in target_bundle for deterministic [[wikilink]] entity linking
        let mut existing_md = Vec::new();
        collect_md_recursive(&target_bundle, &mut existing_md);
        existing_md.sort();

        let mut existing_concepts: Vec<(String, String)> = Vec::new();
        for p in existing_md {
            let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if fname == "index.md" || fname == "log.md" {
                continue;
            }
            let rel_id = p
                .strip_prefix(&target_bundle)
                .unwrap_or(&p)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            if rel_id.starts_with("sources/") {
                continue;
            }
            let title = fs::read_to_string(&p)
                .ok()
                .and_then(|raw| {
                    OkfDocument::parse(&raw)
                        .ok()
                        .and_then(|(fm, _)| fm.and_then(|f| f.title))
                })
                .unwrap_or_else(|| rel_id.clone());
            existing_concepts.push((rel_id, title));
        }

        // Build a single pure-Rust Aho–Corasick word-bounded automaton across all concept aliases
        let mut ac_patterns: Vec<(usize, &str)> = Vec::with_capacity(existing_concepts.len() * 2);
        for (idx, (cid, ctitle)) in existing_concepts.iter().enumerate() {
            let id_leaf = cid.rsplit('/').next().unwrap_or(cid);
            ac_patterns.push((idx, id_leaf));
            if !ctitle.eq_ignore_ascii_case(id_leaf) {
                ac_patterns.push((idx, ctitle.as_str()));
            }
        }
        let ac_matcher =
            crate::context::search::lexical::AhoCorasickWordMatcher::build(ac_patterns);

        let mut raw_files = Vec::new();
        let walker = ignore::WalkBuilder::new(&raw_dir)
            .hidden(false)
            .git_ignore(true)
            .max_depth(Some(6))
            .build();
        for entry in walker.flatten() {
            let p = entry.path();
            if p.is_file() {
                raw_files.push(p.to_path_buf());
            }
        }
        raw_files.sort();

        let mut ingested_count = 0usize;
        let mut skipped_count = 0usize;
        let mut created_concepts = Vec::new();
        let mut auto_linked_edges = 0usize;

        for raw_file in raw_files {
            // Never recursively re-ingest generated index/log/viewer artifacts
            let fname = raw_file.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(fname, "index.md" | "log.md" | "viz.html") {
                skipped_count += 1;
                continue;
            }

            // Algorithmic UTF-8 + NUL-byte content sniffer (no hardcoded extension whitelist)
            let Ok(raw_bytes) = fs::read(&raw_file) else {
                skipped_count += 1;
                continue;
            };
            if !crate::context::search::lexical::sniff_utf8_text_bytes(&raw_bytes) {
                skipped_count += 1;
                continue;
            }
            let Ok(raw_text) = String::from_utf8(raw_bytes) else {
                skipped_count += 1;
                continue;
            };
            if raw_text.trim().is_empty() {
                skipped_count += 1;
                continue;
            }

            let ext = raw_file
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let lang_tag = if ext.is_empty() {
                "text".to_string()
            } else {
                ext.clone()
            };

            let rel_in_raw = raw_file.strip_prefix(&raw_dir).unwrap_or(&raw_file);
            let rel_stem = rel_in_raw
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let slug = format!("sources/{}", sanitize_concept_id(&rel_stem));
            let provenance_path = raw_file
                .strip_prefix(workspace_root)
                .unwrap_or(&raw_file)
                .to_string_lossy()
                .replace('\\', "/");

            let is_markdown = matches!(ext.as_str(), "md" | "mdx" | "markdown")
                || (ext.is_empty() && raw_text.trim_start().starts_with("# "));
            let stripped_body = if is_markdown {
                OkfDocument::parse(&raw_text)
                    .map(|(_, b)| b.trim().to_string())
                    .unwrap_or_else(|_| raw_text.trim().to_string())
            } else {
                raw_text.trim().to_string()
            };

            let stem_leaf = raw_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("source");
            let inferred_title = if is_markdown {
                stripped_body
                    .lines()
                    .find(|l| l.starts_with("# "))
                    .map(|l| l.trim_start_matches("# ").trim().to_string())
                    .unwrap_or_else(|| humanize_slug(stem_leaf))
            } else {
                format!("{} ({})", humanize_slug(stem_leaf), lang_tag)
            };

            let matched_concept_ids = ac_matcher.find_word_bounded_ids(&stripped_body);
            auto_linked_edges += matched_concept_ids.len();

            let mut compiled_body = if is_markdown {
                if stripped_body.starts_with("# ") {
                    stripped_body.clone()
                } else {
                    format!("# {}\n\n{}", inferred_title, stripped_body)
                }
            } else {
                format!(
                    "# {}\n\nCompiled verbatim from raw `{}` artifact `{}`.\n\n```{}\n{}\n```",
                    inferred_title, lang_tag, provenance_path, lang_tag, stripped_body
                )
            };

            if !matched_concept_ids.is_empty() {
                let wikilink_bullets: Vec<String> = matched_concept_ids
                    .into_iter()
                    .map(|cidx| {
                        let (ref cid, ref ctitle) = existing_concepts[cidx];
                        format!("- [[{}|{}]]", cid, ctitle)
                    })
                    .collect();
                compiled_body.push_str("\n\n## Related Wiki Concepts\n");
                compiled_body.push_str(&wikilink_bullets.join("\n"));
                compiled_body.push('\n');
            }

            let tags = vec!["raw-ingested".to_string(), lang_tag];
            let refs = vec![provenance_path.clone()];
            Self::write_entry_with_okf(
                workspace_root,
                &slug,
                &inferred_title,
                &compiled_body,
                &tags,
                &refs,
                WikiOkfOptions {
                    concept_type: Some("Source Note".to_string()),
                    resource: Some(provenance_path),
                    status: Some("stable".to_string()),
                    verified_by: Some("process:openkb-raw-compiler".to_string()),
                    bundle_dir: Some(target_bundle.clone()),
                    ..Default::default()
                },
            )?;

            ingested_count += 1;
            created_concepts.push(slug);
        }

        Ok(WikiIngestReport {
            raw_dir: raw_display,
            bundle_dir: bundle_display,
            ingested_count,
            skipped_count,
            created_concepts,
            auto_linked_edges,
        })
    }
}

/// Summary returned by `WikiManager::ingest_raw_sources` (`okf_bundle(action="ingest_raw")`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiIngestReport {
    pub raw_dir: String,
    pub bundle_dir: String,
    pub ingested_count: usize,
    pub skipped_count: usize,
    pub created_concepts: Vec<String>,
    pub auto_linked_edges: usize,
}

fn humanize_slug(stem: &str) -> String {
    stem.split(['-', '_', '.'])
        .filter(|s| !s.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut s = first.to_uppercase().collect::<String>();
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect_md_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "raw" {
                collect_md_recursive(&p, out);
            }
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

        // 5. Write second entry referencing `[[architecture-overview|Architecture]]` and verify bidirectional backlinks
        WikiManager::write_entry(
            ws,
            "tui-scheduler",
            "TUI Frame Scheduler",
            "Implements dirty-flag rendering on top of [[architecture-overview|Architecture]].",
            &["tui".to_string()],
            &[],
        )
        .unwrap();

        let target_doc = WikiManager::read_entry(ws, "architecture-overview").unwrap();
        assert!(target_doc.contains("## Knowledge Graph Links"));
        assert!(target_doc.contains("**Referenced By:** `[[tui-scheduler]]`"));

        let source_doc = WikiManager::read_entry(ws, "tui-scheduler").unwrap();
        assert!(source_doc.contains("**Links To:** `[[architecture-overview]]`"));

        // 6. Verify Karpathy / OpenKB `raw/` -> `wiki/sources/` compiler & auto-wikilinking
        let raw_dir = ws.join(".minicode/wiki/raw/rfcs");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(
            raw_dir.join("001-frame-pacing.md"),
            "# RFC 001: Dirty Frame Pacing\nBuilds on the Architecture Overview and TUI Frame Scheduler to hit 0% idle CPU.\n",
        )
        .unwrap();

        let ingest = WikiManager::ingest_raw_sources(ws, None, None).unwrap();
        assert_eq!(ingest.ingested_count, 1);
        assert_eq!(
            ingest.created_concepts,
            vec!["sources/rfcs/001-frame-pacing".to_string()]
        );
        assert!(ingest.auto_linked_edges >= 2);

        let compiled = WikiManager::read_entry(ws, "sources/rfcs/001-frame-pacing").unwrap();
        assert!(compiled.contains("Trust Tier: `machine-confirmed`"));
        assert!(compiled.contains("[[architecture-overview|Architecture Overview]]"));
        assert!(compiled.contains("[[tui-scheduler|TUI Frame Scheduler]]"));
    }
}
