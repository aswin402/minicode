use super::viewer_assets::{VIZ_CSS, VIZ_HTML_TEMPLATE, VIZ_JS};
use super::{is_stale, normalize_verified, trust_tier, OkfDocument};
use crate::error::{ContextError, Result};
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
            if !name.starts_with('.') {
                collect_md_files(&p, out);
            }
        } else if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(p);
        }
    }
}

/// Extracts internal `.md` cross-links from markdown body (supporting both `/bundle/rel.md` and `../rel.md`).
pub fn extract_links(body: &str, doc_dir: &Path, bundle_root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();

    let mut rest = body;
    while let Some(open_idx) = rest.find("](") {
        let after_open = &rest[open_idx + 2..];
        if let Some(close_idx) = after_open.find(')') {
            let raw_target = after_open[..close_idx].trim();
            let target = raw_target.split('#').next().unwrap_or("").trim();
            if target.ends_with(".md") && !target.contains("://") {
                if let Some(resolved_id) = resolve_bundle_link(target, doc_dir, bundle_root) {
                    if !resolved_id.is_empty() && seen.insert(resolved_id.clone()) {
                        out.push(resolved_id);
                    }
                }
            }
            rest = &after_open[close_idx + 1..];
        } else {
            break;
        }
    }

    out
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
