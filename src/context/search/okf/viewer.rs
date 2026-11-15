use super::viewer_assets::{VIZ_CSS, VIZ_HTML_TEMPLATE, VIZ_JS};
use super::{is_stale, normalize_verified, trust_tier, OkfDocument};
use crate::error::{ContextError, Result};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Statistics returned after generating an interactive HTML bundle visualization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualizationStats {
    pub concepts: usize,
    pub edges: usize,
    pub bytes: usize,
    pub output_path: String,
}

/// Broken internal cross-reference or `[[wikilink]]` discovered during graph linting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokenWikiLink {
    pub from_concept: String,
    pub target_concept: String,
}

/// Bidirectional OpenKB / OKF v0.2 knowledge graph health report (`okf_bundle(action="lint")`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiLintReport {
    pub total_concepts: usize,
    pub total_edges: usize,
    pub broken_links: Vec<BrokenWikiLink>,
    pub orphaned_concepts: Vec<String>,
    pub stale_concepts: Vec<String>,
    pub deprecated_linked_concepts: Vec<String>,
    pub healthy: bool,
}

#[derive(Debug, Clone)]
struct ViewerConcept {
    id: String,
    concept_type: String,
    title: String,
    description: String,
    resource: String,
    tags: Vec<String>,
    body: String,
    status: String,
    generated: Value,
    verified: Value,
    stale_after: String,
    sources: Value,
    trust_tier: String,
    stale: bool,
    links_to: Vec<String>,
}

impl ViewerConcept {
    fn to_node(&self) -> Value {
        let color = node_color_for_type(&self.concept_type);
        let size = 30 + (self.body.len() / 200).min(60);
        json!({
            "data": {
                "id": self.id,
                "label": if self.title.is_empty() { &self.id } else { &self.title },
                "type": self.concept_type,
                "description": self.description,
                "resource": self.resource,
                "tags": self.tags,
                "status": self.status,
                "generated": self.generated,
                "verified": self.verified,
                "stale_after": self.stale_after,
                "sources": self.sources,
                "trust_tier": self.trust_tier,
                "stale": self.stale,
                "color": color,
                "size": size,
            }
        })
    }
}

fn node_color_for_type(concept_type: &str) -> &'static str {
    match concept_type {
        "BigQuery Dataset" | "Dataset" | "Architecture" => "#8b5cf6",
        "BigQuery Table" | "Table" | "PRD" | "Spec" => "#3b82f6",
        "Reference" | "Skill" | "Package" => "#10b981",
        "Attested Computation" | "Computation" => "#f59e0b",
        "Metric" | "Design" => "#ec4899",
        _ => "#94a3b8",
    }
}

/// Walks an OKF bundle directory and writes a single self-contained HTML visualization (`viewer/generator.py:generate_visualization`).
pub fn generate_visualization(
    bundle_root: &Path,
    out_path: Option<&Path>,
    bundle_name: Option<&str>,
) -> Result<VisualizationStats> {
    if !bundle_root.is_dir() {
        return Err(ContextError::Okf(format!(
            "Bundle directory not found: {}",
            bundle_root.display()
        ))
        .into());
    }

    let target_out = out_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| bundle_root.join("viz.html"));

    let concepts = walk_concepts(bundle_root);
    let (graph_val, edge_count) = build_graph(&concepts);

    let resolved_name = bundle_name
        .map(|s| s.to_string())
        .or_else(|| {
            bundle_root
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "OKF Bundle".to_string());

    let name_json = serde_json::to_string(&resolved_name).unwrap_or_else(|_| "\"OKF\"".to_string());
    let data_json = serde_json::to_string(&graph_val).unwrap_or_else(|_| "{}".to_string());

    let html = VIZ_HTML_TEMPLATE
        .replace("/*__VIZ_CSS__*/", VIZ_CSS)
        .replace("/*__VIZ_JS__*/", VIZ_JS)
        .replace("__BUNDLE_NAME__", &name_json)
        .replace("__BUNDLE_DATA__", &data_json);

    if let Some(parent) = target_out.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&target_out, &html).map_err(|e| {
        ContextError::Okf(format!(
            "Failed to write visualization to {}: {}",
            target_out.display(),
            e
        ))
    })?;

    Ok(VisualizationStats {
        concepts: concepts.len(),
        edges: edge_count,
        bytes: html.len(),
        output_path: target_out.display().to_string(),
    })
}

fn walk_concepts(bundle_root: &Path) -> Vec<ViewerConcept> {
    let mut md_files = Vec::new();
    collect_md_files(bundle_root, &mut md_files);
    md_files.sort();

    let mut concepts = Vec::new();
    for md_path in md_files {
        let Some(fname) = md_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if fname == "index.md" || fname == "log.md" {
            continue;
        }
        let Ok(rel) = md_path.strip_prefix(bundle_root) else {
            continue;
        };
        let concept_id = rel.with_extension("").to_string_lossy().replace('\\', "/");

        let Ok(raw) = fs::read_to_string(&md_path) else {
            continue;
        };
        let (fm_opt, body) = match OkfDocument::parse(&raw) {
            Ok(res) => res,
            Err(_) => continue,
        };
        let fm = fm_opt.unwrap_or_default();
        let doc_dir = md_path.parent().unwrap_or(bundle_root);
        let links_to = extract_links(body, doc_dir, bundle_root);
        let verified_list = normalize_verified(&fm);

        concepts.push(ViewerConcept {
            id: concept_id.clone(),
            concept_type: if fm.concept_type.is_empty() {
                "Concept".to_string()
            } else {
                fm.concept_type.clone()
            },
            title: fm.title.clone().unwrap_or_else(|| concept_id.clone()),
            description: fm.description.clone().unwrap_or_default(),
            resource: fm.resource.clone().unwrap_or_default(),
            tags: fm.tags.clone(),
            body: body.to_string(),
            status: fm.status.clone().unwrap_or_else(|| "stable".to_string()),
            generated: serde_json::to_value(&fm.generated).unwrap_or(json!({})),
            verified: serde_json::to_value(&verified_list).unwrap_or(json!([])),
            stale_after: fm.stale_after.clone().unwrap_or_default(),
            sources: serde_json::to_value(&fm.sources).unwrap_or(json!([])),
            trust_tier: trust_tier(&fm).to_string(),
            stale: is_stale(&fm, None),
            links_to,
        });
    }
    concepts
}

fn collect_md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "raw" {
                collect_md_files(&p, out);
            }
        } else if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(p);
        }
    }
}

/// Extracts internal `.md` cross-links and Obsidian/OpenKB `[[wikilinks]]` from a markdown body,
/// ignoring fenced code blocks and inline code spans via `pulldown-cmark`.
pub fn extract_links(body: &str, doc_dir: &Path, bundle_root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut prose_buf = String::with_capacity(body.len().min(8192));
    let mut in_code_block = false;

    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(body, opts) {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                prose_buf.push('\n');
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                prose_buf.push('\n');
            }
            Event::Start(Tag::Link { dest_url, .. }) if !in_code_block => {
                let target = dest_url.split('#').next().unwrap_or("").trim();
                if target.ends_with(".md") && !target.contains("://") {
                    if let Some(resolved_id) = resolve_bundle_link(target, doc_dir, bundle_root) {
                        if !resolved_id.is_empty() && seen.insert(resolved_id.clone()) {
                            out.push(resolved_id);
                        }
                    }
                }
            }
            Event::Text(text) if !in_code_block => {
                prose_buf.push_str(&text);
            }
            Event::Code(_) => {
                prose_buf.push(' ');
            }
            Event::SoftBreak | Event::HardBreak => {
                prose_buf.push(' ');
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Item
                | TagEnd::Heading(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::TableCell,
            ) => {
                prose_buf.push('\n');
            }
            _ => {}
        }
    }

    let mut raw_wikilinks = BTreeSet::new();
    crate::context::doc_tree::extract_wikilinks_from_prose(&prose_buf, &mut raw_wikilinks);
    if !raw_wikilinks.is_empty() {
        let mut bundle_md_files = Vec::new();
        collect_md_files(bundle_root, &mut bundle_md_files);
        bundle_md_files.sort();

        for wiki_target in raw_wikilinks {
            if let Some(resolved_id) =
                resolve_wikilink_target(&wiki_target, doc_dir, bundle_root, &bundle_md_files)
            {
                if !resolved_id.is_empty() && seen.insert(resolved_id.clone()) {
                    out.push(resolved_id);
                }
            }
        }
    }

    out
}

fn resolve_wikilink_target(
    raw_target: &str,
    doc_dir: &Path,
    bundle_root: &Path,
    bundle_md_files: &[PathBuf],
) -> Option<String> {
    let clean = raw_target.trim().trim_end_matches(".md");
    if clean.is_empty() || clean.contains("://") {
        return None;
    }

    if clean.starts_with('/') {
        return resolve_bundle_link(clean, doc_dir, bundle_root);
    }

    if clean.starts_with('.') {
        return resolve_bundle_link(clean, doc_dir, bundle_root);
    }

    if clean.contains('/') {
        if doc_dir.join(format!("{clean}.md")).exists() {
            return resolve_bundle_link(clean, doc_dir, bundle_root);
        }
        return normalize_rel_path(Path::new(clean));
    }

    // Bare slug `[[concept]]`: check local dir first, then bundle root, then unique/first stem match across bundle
    if doc_dir.join(format!("{clean}.md")).exists() {
        return resolve_bundle_link(clean, doc_dir, bundle_root);
    }
    if bundle_root.join(format!("{clean}.md")).exists() {
        return normalize_rel_path(Path::new(clean));
    }

    let clean_lower = clean.to_ascii_lowercase();
    let mut ci_fallback: Option<String> = None;
    for md_path in bundle_md_files {
        let Some(fname) = md_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if fname == "index.md" || fname == "log.md" {
            continue;
        }
        let Some(stem) = md_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem == clean {
            if let Ok(rel) = md_path.strip_prefix(bundle_root) {
                return Some(rel.with_extension("").to_string_lossy().replace('\\', "/"));
            }
        } else if ci_fallback.is_none() && stem.to_ascii_lowercase() == clean_lower {
            if let Ok(rel) = md_path.strip_prefix(bundle_root) {
                ci_fallback = Some(rel.with_extension("").to_string_lossy().replace('\\', "/"));
            }
        }
    }
    if ci_fallback.is_some() {
        return ci_fallback;
    }

    // Target does not exist — return deterministic unresolved ID so conformance/lint flags it
    resolve_bundle_link(clean, doc_dir, bundle_root)
}

/// Returns sorted `(outgoing_links, incoming_backlinks)` for `concept_id` within `bundle_root`.
pub fn concept_neighbors(bundle_root: &Path, concept_id: &str) -> (Vec<String>, Vec<String>) {
    if !bundle_root.is_dir() {
        return (Vec::new(), Vec::new());
    }
    let concepts = walk_concepts(bundle_root);
    let mut outgoing = BTreeSet::new();
    let mut incoming = BTreeSet::new();

    for c in &concepts {
        if c.id == concept_id {
            for target in &c.links_to {
                if target != concept_id {
                    outgoing.insert(target.clone());
                }
            }
        } else if c.links_to.iter().any(|t| t == concept_id) {
            incoming.insert(c.id.clone());
        }
    }

    (
        outgoing.into_iter().collect(),
        incoming.into_iter().collect(),
    )
}

/// Runs an OpenKB / Karpathy-style structural graph lint across an OKF knowledge bundle:
/// detects broken `.md` & `[[wikilink]]` edges, orphaned islands, expired `stale_after` concepts,
/// and active concepts still linking to `status: deprecated` concepts.
pub fn lint_bundle_graph(bundle_root: &Path) -> Result<WikiLintReport> {
    if !bundle_root.is_dir() {
        return Err(ContextError::Okf(format!(
            "Bundle directory not found: {}",
            bundle_root.display()
        ))
        .into());
    }

    let concepts = walk_concepts(bundle_root);
    let known_ids: BTreeSet<&str> = concepts.iter().map(|c| c.id.as_str()).collect();
    let deprecated_ids: BTreeSet<&str> = concepts
        .iter()
        .filter(|c| c.status == "deprecated")
        .map(|c| c.id.as_str())
        .collect();

    let mut broken_links = Vec::new();
    let mut stale_concepts = Vec::new();
    let mut deprecated_linked_set = BTreeSet::new();
    let mut connected_ids = BTreeSet::new();
    let mut seen_edges = BTreeSet::new();

    for c in &concepts {
        if c.stale {
            stale_concepts.push(c.id.clone());
        }
        for target in &c.links_to {
            if target == &c.id {
                continue;
            }
            if known_ids.contains(target.as_str()) {
                connected_ids.insert(c.id.clone());
                connected_ids.insert(target.clone());
                seen_edges.insert((c.id.clone(), target.clone()));
                if c.status != "deprecated" && deprecated_ids.contains(target.as_str()) {
                    deprecated_linked_set.insert(target.clone());
                }
            } else {
                broken_links.push(BrokenWikiLink {
                    from_concept: c.id.clone(),
                    target_concept: target.clone(),
                });
            }
        }
    }

    let orphaned_concepts = if concepts.len() > 1 {
        concepts
            .iter()
            .filter(|c| !connected_ids.contains(&c.id))
            .map(|c| c.id.clone())
            .collect()
    } else {
        Vec::new()
    };

    let deprecated_linked_concepts: Vec<String> = deprecated_linked_set.into_iter().collect();
    let healthy = broken_links.is_empty()
        && stale_concepts.is_empty()
        && deprecated_linked_concepts.is_empty();

    Ok(WikiLintReport {
        total_concepts: concepts.len(),
        total_edges: seen_edges.len(),
        broken_links,
        orphaned_concepts,
        stale_concepts,
        deprecated_linked_concepts,
        healthy,
    })
}

fn resolve_bundle_link(target: &str, doc_dir: &Path, bundle_root: &Path) -> Option<String> {
    let without_md = target.strip_suffix(".md").unwrap_or(target);
    if let Some(abs_rel) = without_md.strip_prefix('/') {
        // Bundle-relative link (`/tables/customers.md` per OKF §6.1)
        let clean = normalize_rel_path(Path::new(abs_rel))?;
        return Some(clean);
    }

    // Relative to `doc_dir`
    let doc_rel = doc_dir.strip_prefix(bundle_root).ok()?;
    let joined = doc_rel.join(without_md);
    normalize_rel_path(&joined)
}

fn normalize_rel_path(path: &Path) -> Option<String> {
    let mut stack: Vec<String> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.pop().is_none() {
                    return None; // Escaped bundle root
                }
            }
            Component::Normal(os) => {
                stack.push(os.to_string_lossy().to_string());
            }
            _ => return None,
        }
    }
    if stack.is_empty() {
        None
    } else {
        Some(stack.join("/"))
    }
}

fn build_graph(concepts: &[ViewerConcept]) -> (Value, usize) {
    let ids: BTreeSet<&str> = concepts.iter().map(|c| c.id.as_str()).collect();
    let nodes: Vec<Value> = concepts.iter().map(|c| c.to_node()).collect();
    let mut edges = Vec::new();
    let mut seen_edges = BTreeSet::new();

    for c in concepts {
        for target in &c.links_to {
            if target == &c.id || !ids.contains(target.as_str()) {
                continue;
            }
            if seen_edges.insert((c.id.clone(), target.clone())) {
                edges.push(json!({
                    "data": {
                        "id": format!("{}__{}", c.id, target),
                        "source": c.id,
                        "target": target,
                    }
                }));
            }
        }
    }

    let mut bodies = BTreeMap::new();
    let mut types_set = BTreeSet::new();
    for c in concepts {
        bodies.insert(c.id.clone(), c.body.clone());
        types_set.insert(c.concept_type.clone());
    }
    let types: Vec<String> = types_set.into_iter().collect();
    let edge_count = edges.len();

    let graph = json!({
        "nodes": nodes,
        "edges": edges,
        "bodies": bodies,
        "types": types,
        "palette": {
            "BigQuery Dataset": "#8b5cf6",
            "BigQuery Table": "#3b82f6",
            "Reference": "#10b981",
            "Attested Computation": "#f59e0b",
            "Metric": "#ec4899"
        }
    });
    (graph, edge_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::okf::{write_concept_doc, OkfFrontmatter};
    use tempfile::tempdir;

    #[test]
    fn test_wikilinks_backlinks_and_bundle_graph_lint() {
        let dir = tempdir().unwrap();
        let bundle = dir.path();

        // 1. Core concept (target of wikilink)
        write_concept_doc(
            bundle,
            "concepts/turboquant",
            OkfFrontmatter {
                concept_type: "Architecture".to_string(),
                title: Some("TurboQuant 2-Bit Polar Cache".to_string()),
                ..Default::default()
            },
            "# Overview\nUses Walsh-Hadamard rotation and Lloyd-Max 2-bit centroids.\n\n```md\n[[ignored_inside_code_fence]]\n```\n",
            false,
        )
        .unwrap();

        // 2. Deprecated concept that is still linked
        write_concept_doc(
            bundle,
            "concepts/legacy-vec",
            OkfFrontmatter {
                concept_type: "Architecture".to_string(),
                title: Some("Legacy Float32 Store".to_string()),
                status: Some("deprecated".to_string()),
                ..Default::default()
            },
            "# Legacy\nSuperseded by [[turboquant]].\n",
            false,
        )
        .unwrap();

        // 3. Stale consumer concept linking via [[turboquant|2-Bit Quantizer]], [[concepts/legacy-vec]], and broken [[missing_page]]
        write_concept_doc(
            bundle,
            "search/semantic-engine",
            OkfFrontmatter {
                concept_type: "Component".to_string(),
                title: Some("Semantic Engine".to_string()),
                stale_after: Some("2020-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            "# Pipeline\nPowered by [[turboquant|2-Bit Quantizer]] and migrating off [[concepts/legacy-vec#v1]].\nAlso references [[missing_page]] and inline `[[not_a_real_link]]`.\n",
            false,
        )
        .unwrap();

        // 4. Orphaned island concept with zero incoming or outgoing links
        write_concept_doc(
            bundle,
            "notes/isolated-idea",
            OkfFrontmatter {
                concept_type: "Note".to_string(),
                title: Some("Isolated Idea".to_string()),
                ..Default::default()
            },
            "# Scratchpad\nStandalone note without links.\n",
            false,
        )
        .unwrap();

        // Verify bidirectional neighbors for `concepts/turboquant`
        let (out_tq, backlinks_tq) = concept_neighbors(bundle, "concepts/turboquant");
        assert!(out_tq.is_empty(), "code-fenced wikilink must be ignored");
        assert_eq!(
            backlinks_tq,
            vec![
                "concepts/legacy-vec".to_string(),
                "search/semantic-engine".to_string()
            ]
        );

        // Verify graph lint catches broken link, orphan, stale concept, and linked deprecated concept
        let report = lint_bundle_graph(bundle).unwrap();
        assert!(!report.healthy);
        assert_eq!(report.total_concepts, 4);
        assert_eq!(report.total_edges, 3);
        assert_eq!(
            report.broken_links,
            vec![BrokenWikiLink {
                from_concept: "search/semantic-engine".to_string(),
                target_concept: "search/missing_page".to_string(),
            }]
        );
        assert_eq!(
            report.orphaned_concepts,
            vec!["notes/isolated-idea".to_string()]
        );
        assert_eq!(
            report.stale_concepts,
            vec!["search/semantic-engine".to_string()]
        );
        assert_eq!(
            report.deprecated_linked_concepts,
            vec!["concepts/legacy-vec".to_string()]
        );
    }
}
