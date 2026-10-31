use super::viewer::extract_links;
use super::{is_stale, trust_tier, OkfDocument};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

/// Session-wide guardrailed web crawl state for OKF web enrichment (`tools/web_tools.py`).
#[derive(Debug, Clone)]
pub struct OkfWebCrawlState {
    pub allowed_hosts: BTreeSet<String>,
    pub allowed_path_prefixes: Vec<String>,
    pub denied_path_substrings: Vec<String>,
    pub max_pages: usize,
    pub max_depth: usize,
    pub fetched_count: usize,
    pub visited: HashSet<String>,
    pub url_depth: HashMap<String, usize>,
}

impl OkfWebCrawlState {
    /// Creates a new crawl state seeded with initial URLs at depth 0.
    pub fn new(
        seed_urls: &[String],
        extra_allowed_hosts: &[String],
        allowed_path_prefixes: Vec<String>,
        denied_path_substrings: Vec<String>,
        max_pages: usize,
        max_depth: usize,
    ) -> Self {
        let mut allowed_hosts = BTreeSet::new();
        let mut url_depth = HashMap::new();

        for seed in seed_urls {
            if let Ok(parsed) = Url::parse(seed) {
                if let Some(host) = parsed.host_str() {
                    allowed_hosts.insert(host.to_string());
                }
            }
            url_depth.insert(seed.clone(), 0);
        }
        for host in extra_allowed_hosts {
            if !host.trim().is_empty() {
                allowed_hosts.insert(host.trim().to_string());
            }
        }

        Self {
            allowed_hosts,
            allowed_path_prefixes,
            denied_path_substrings,
            max_pages,
            max_depth,
            fetched_count: 0,
            visited: HashSet::new(),
            url_depth,
        }
    }

    /// Validates whether `url` is permitted by all crawl guardrails and marks it visited if allowed.
    /// Returns `Ok(depth)` on success or `Err(reason)` if rejected (`tools/web_tools.py:fetch_url`).
    pub fn check_and_mark_url(&mut self, url_str: &str) -> std::result::Result<usize, String> {
        let parsed = Url::parse(url_str).map_err(|_| "invalid URL".to_string())?;
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(format!("unsupported scheme: {}", scheme));
        }
        let Some(host) = parsed.host_str() else {
            return Err("missing host in URL".to_string());
        };
        if !self.allowed_hosts.is_empty() && !self.allowed_hosts.contains(host) {
            let allowed_list: Vec<&String> = self.allowed_hosts.iter().collect();
            return Err(format!(
                "host not in allowed list: {} (allowed: {:?})",
                host, allowed_list
            ));
        }
        let path = if parsed.path().is_empty() {
            "/"
        } else {
            parsed.path()
        };
        if !self.allowed_path_prefixes.is_empty()
            && !self
                .allowed_path_prefixes
                .iter()
                .any(|p| path.starts_with(p.as_str()))
        {
            return Err(format!(
                "path not in allowed prefixes: {} (allowed: {:?})",
                path, self.allowed_path_prefixes
            ));
        }
        for bad in &self.denied_path_substrings {
            if !bad.is_empty() && path.contains(bad.as_str()) {
                return Err(format!("path matches denied substring: {:?}", bad));
            }
        }
        if self.visited.contains(url_str) {
            return Err("already fetched in this session".to_string());
        }
        if self.fetched_count >= self.max_pages {
            return Err("max_pages reached".to_string());
        }
        let Some(&depth) = self.url_depth.get(url_str) else {
            return Err(
                "URL not reachable from a seed within the crawl graph (was not returned as a link by any fetched page)".to_string(),
            );
        };
        if depth > self.max_depth {
            return Err(format!(
                "depth {} exceeds max_depth {}",
                depth, self.max_depth
            ));
        }

        self.visited.insert(url_str.to_string());
        self.fetched_count += 1;
        Ok(depth)
    }

    /// Registers outbound links discovered on a page at `parent_depth + 1`.
    pub fn register_discovered_links(&mut self, parent_depth: usize, links: &[String]) {
        let child_depth = parent_depth + 1;
        for link in links {
            self.url_depth.entry(link.clone()).or_insert(child_depth);
        }
    }
}

/// Summary report from validating an OKF v0.2 bundle directory (§11 Conformance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleValidationReport {
    pub conformant: bool,
    pub total_concepts: usize,
    pub errors: Vec<String>,
    pub broken_links: Vec<(String, String)>,
    pub stale_concepts: Vec<String>,
    pub trust_tier_counts: BTreeMap<String, usize>,
    pub attested_computations: usize,
}

/// Audits an OKF bundle directory against OKF v0.2 §11 Conformance rules.
pub fn validate_bundle_conformance(bundle_root: &Path) -> Result<BundleValidationReport> {
    let mut md_files = Vec::new();
    collect_md(bundle_root, &mut md_files);
    md_files.sort();

    let mut errors = Vec::new();
    let mut broken_links = Vec::new();
    let mut stale_concepts = Vec::new();
    let mut trust_tier_counts = BTreeMap::new();
    let mut attested_computations = 0usize;
    let mut known_ids = BTreeSet::new();
    let mut pending_links: Vec<(String, Vec<String>)> = Vec::new();

    for path in &md_files {
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if fname == "index.md" || fname == "log.md" {
            continue;
        }
        let Ok(rel) = path.strip_prefix(bundle_root) else {
            continue;
        };
        let concept_id = rel.with_extension("").to_string_lossy().replace('\\', "/");
        known_ids.insert(concept_id.clone());

        let raw = fs::read_to_string(path).unwrap_or_default();
        match OkfDocument::parse(&raw) {
            Ok((Some(fm), body)) => {
                if let Err(e) = fm.validate() {
                    errors.push(format!("{}: {}", concept_id, e));
                }
                let tier = trust_tier(&fm).to_string();
                *trust_tier_counts.entry(tier).or_insert(0) += 1;
                if is_stale(&fm, None) {
                    stale_concepts.push(concept_id.clone());
                }
                if fm.concept_type == "Attested Computation" {
                    attested_computations += 1;
                }
                let doc_dir = path.parent().unwrap_or(bundle_root);
                let links = extract_links(body, doc_dir, bundle_root);
                pending_links.push((concept_id, links));
            }
            Ok((None, _)) => {
                errors.push(format!(
                    "{}: missing YAML frontmatter block or required `type` key",
                    concept_id
                ));
            }
            Err(e) => {
                errors.push(format!("{}: {}", concept_id, e));
            }
        }
    }

    for (src_id, links) in pending_links {
        for target_id in links {
            if !known_ids.contains(&target_id) {
                broken_links.push((src_id.clone(), target_id));
            }
        }
    }

    Ok(BundleValidationReport {
        conformant: errors.is_empty(),
        total_concepts: known_ids.len(),
        errors,
        broken_links,
        stale_concepts,
        trust_tier_counts,
        attested_computations,
    })
}

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') {
                collect_md(&p, out);
            }
        } else if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(p);
        }
    }
}
