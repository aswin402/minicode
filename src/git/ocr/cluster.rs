//! Deterministic AST-graph + directory-affinity file clustering for parallel code review.
//!
//! Inspired by `alibaba/open-code-review`'s semantic `grouping_task`, but executed at **0 LLM tokens**
//! by exploiting `minicode`'s Tree-sitter `CodeGraph` import/dependency edges and test-file pairing.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// A cohesive cluster of related files to be reviewed together by one Reviewer worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFileCluster {
    pub id: usize,
    pub label: String,
    pub files: Vec<String>,
}

/// Partitions reviewable files into semantically related clusters (`<= max_cluster_size` each)
/// using `CodeGraph` dependency adjacency, test↔source stem pairing, and directory grouping.
pub struct ReviewFileClusterer;

impl ReviewFileClusterer {
    /// Default upper bound per cluster (matches OCR's max-10 heuristic, tuned slightly tighter
    /// to keep each MiniPub Reviewer's context compact and fast).
    pub const DEFAULT_MAX_CLUSTER_SIZE: usize = 8;
    /// Maximum number of concurrent clusters to avoid excessive subagent fan-out on huge diffs.
    pub const DEFAULT_MAX_CLUSTERS: usize = 3;

    /// Partitions `files` into cohesive clusters.
    /// Returns a single cluster when `files.len() <= max_cluster_size` (zero overhead on small diffs).
    #[must_use]
    pub fn cluster_files(
        workspace_root: &Path,
        files: &[String],
        max_cluster_size: usize,
        max_clusters: usize,
    ) -> Vec<ReviewFileCluster> {
        let cap = max_cluster_size.max(2);
        let cluster_cap = max_clusters.max(1);
        let mut norm_files: Vec<String> = files
            .iter()
            .map(|f| f.replace('\\', "/"))
            .filter(|f| !f.is_empty())
            .collect();
        norm_files.sort();
        norm_files.dedup();

        if norm_files.is_empty() {
            return Vec::new();
        }
        if norm_files.len() <= cap || cluster_cap == 1 {
            return vec![ReviewFileCluster {
                id: 1,
                label: derive_cluster_label(&norm_files),
                files: norm_files,
            }];
        }

        let n = norm_files.len();
        let index_by_path: HashMap<String, usize> = norm_files
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), i))
            .collect();

        // Build undirected weighted adjacency using:
        // 1. Co-located source ↔ test pairing (e.g. `src/lib/storage.ts` ↔ `src/lib/__tests__/storage.test.ts`)
        // 2. AST CodeGraph edges (shared imports / call references across changed files)
        // 3. Same parent directory affinity
        let mut parent: Vec<usize> = (0..n).collect();
        let mut group_size: Vec<usize> = vec![1; n];

        let find = |parent: &mut [usize], mut x: usize| -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        };
        let try_union = |parent: &mut [usize], sizes: &mut [usize], a: usize, b: usize| {
            let ra = find(parent, a);
            let rb = find(parent, b);
            if ra != rb && sizes[ra] + sizes[rb] <= cap {
                if sizes[ra] >= sizes[rb] {
                    parent[rb] = ra;
                    sizes[ra] += sizes[rb];
                } else {
                    parent[ra] = rb;
                    sizes[rb] += sizes[ra];
                }
            }
        };

        // Pass 1: Pair test files with their matching implementation stem
        let mut stem_buckets: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, rel) in norm_files.iter().enumerate() {
            let stem = canonical_test_agnostic_stem(rel);
            if !stem.is_empty() {
                stem_buckets.entry(stem).or_default().push(i);
            }
        }
        for idxs in stem_buckets.values() {
            if idxs.len() >= 2 && idxs.len() <= 4 {
                for pair in idxs.windows(2) {
                    try_union(&mut parent, &mut group_size, pair[0], pair[1]);
                }
            }
        }

        // Pass 2: Union files linked by AST CodeGraph edges (imports / symbol references)
        let graph_opt = crate::context::graph::CodeGraph::try_get_or_load_fast(workspace_root)
            .or_else(|| {
                if norm_files.len() <= 64 {
                    crate::context::graph::CodeGraph::get_or_build_cached(workspace_root).ok()
                } else {
                    None
                }
            });
        if let Some(graph) = graph_opt {
            let pg = graph.graph();
            let mut node_to_file_idx: HashMap<petgraph::graph::NodeIndex, usize> = HashMap::new();
            for node_idx in pg.node_indices() {
                if let Some(node) = pg.node_weight(node_idx) {
                    let rel_norm = node
                        .file_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&node.file_path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    if let Some(&file_i) = index_by_path.get(&rel_norm) {
                        node_to_file_idx.insert(node_idx, file_i);
                    }
                }
            }
            for edge in pg.edge_indices() {
                if let Some((src, dst)) = pg.edge_endpoints(edge) {
                    if let (Some(&fi), Some(&fj)) =
                        (node_to_file_idx.get(&src), node_to_file_idx.get(&dst))
                    {
                        if fi != fj {
                            try_union(&mut parent, &mut group_size, fi, fj);
                        }
                    }
                }
            }
        }

        // Pass 3: Merge remaining files sharing the exact parent directory
        let mut dir_buckets: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, rel) in norm_files.iter().enumerate() {
            let dir = parent_dir_key(rel);
            dir_buckets.entry(dir).or_default().push(i);
        }
        for idxs in dir_buckets.values() {
            for pair in idxs.windows(2) {
                try_union(&mut parent, &mut group_size, pair[0], pair[1]);
            }
        }

        // Collect components
        let mut comp_map: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for (i, rel) in norm_files.iter().enumerate() {
            let root = find(&mut parent, i);
            comp_map.entry(root).or_default().push(rel.clone());
        }
        let mut clusters: Vec<Vec<String>> = comp_map.into_values().collect();

        // Bin-pack tiny singleton/duo fragments together (up to `cap`) so we never spawn
        // redundant subagents for 1-2 leftover config/root files.
        clusters.sort_by(|a, b| {
            b.len()
                .cmp(&a.len())
                .then_with(|| a.first().cmp(&b.first()))
        });
        let mut packed: Vec<Vec<String>> = Vec::new();
        for mut group in clusters {
            if let Some(target) = packed
                .iter_mut()
                .find(|bucket| bucket.len() + group.len() <= cap)
            {
                target.append(&mut group);
            } else {
                packed.push(group);
            }
        }

        // Enforce `cluster_cap` by merging smallest buckets into the least-loaded kept bucket
        while packed.len() > cluster_cap {
            if let Some(mut overflow) = packed.pop() {
                if let Some(smallest) = packed.iter_mut().min_by_key(|b| b.len()) {
                    smallest.append(&mut overflow);
                }
            }
        }

        packed
            .into_iter()
            .enumerate()
            .map(|(idx, mut files)| {
                files.sort();
                files.dedup();
                ReviewFileCluster {
                    id: idx + 1,
                    label: derive_cluster_label(&files),
                    files,
                }
            })
            .collect()
    }
}

fn parent_dir_key(rel: &str) -> String {
    match rel.rsplit_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => "(root)".to_string(),
    }
}

fn canonical_test_agnostic_stem(rel: &str) -> String {
    let file_name = rel.rsplit('/').next().unwrap_or(rel);
    let lower = file_name.to_ascii_lowercase();
    let without_ext = lower
        .split('.')
        .next()
        .unwrap_or(&lower)
        .trim_start_matches("test_")
        .trim_end_matches("_test")
        .trim_end_matches("_spec");
    if matches!(
        without_ext,
        "index" | "mod" | "lib" | "main" | "types" | "utils" | "setup"
    ) {
        String::new()
    } else {
        without_ext.to_string()
    }
}

fn derive_cluster_label(files: &[String]) -> String {
    let mut dirs: HashSet<String> = HashSet::new();
    for f in files {
        let key = match f.rsplit_once('/') {
            Some((dir, _)) => dir.to_string(),
            None => "root".to_string(),
        };
        dirs.insert(key);
    }
    let mut sorted: Vec<String> = dirs.into_iter().collect();
    sorted.sort();
    match sorted.len() {
        0 => "workspace".to_string(),
        1 => sorted[0].clone(),
        2 => format!("{} + {}", sorted[0], sorted[1]),
        n => format!("{} + {} (+{} dirs)", sorted[0], sorted[1], n - 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_diff_stays_single_cluster() {
        let tmp = tempfile::tempdir().unwrap();
        let files = vec![
            "src/lib/date.ts".to_string(),
            "src/lib/streaks.ts".to_string(),
            "src/App.tsx".to_string(),
        ];
        let clusters = ReviewFileClusterer::cluster_files(tmp.path(), &files, 8, 3);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].files.len(), 3);
    }

    #[test]
    fn test_large_diff_pairs_tests_and_respects_cluster_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let files: Vec<String> = vec![
            "src/lib/storage.ts",
            "src/lib/__tests__/storage.test.ts",
            "src/lib/metrics.ts",
            "src/lib/__tests__/metrics.test.ts",
            "src/components/charts/BarChart.tsx",
            "src/components/charts/TrendChart.tsx",
            "src/components/charts/RingChart.tsx",
            "src/components/forms/AddForm.tsx",
            "src/components/forms/BudgetEditor.tsx",
            "src/components/forms/FilterBar.tsx",
            "src/hooks/useLedger.ts",
            "src/hooks/useToday.ts",
            "src/App.tsx",
            "src/index.css",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let clusters = ReviewFileClusterer::cluster_files(tmp.path(), &files, 6, 3);
        assert!(clusters.len() >= 2 && clusters.len() <= 3);

        // Every input file must appear in exactly one cluster (100% coverage invariant!)
        let mut flat: Vec<String> = clusters.iter().flat_map(|c| c.files.clone()).collect();
        flat.sort();
        let mut expected = files.clone();
        expected.sort();
        assert_eq!(flat, expected);

        // `storage.ts` and `storage.test.ts` must stay in the same cluster
        let storage_cluster = clusters
            .iter()
            .find(|c| c.files.iter().any(|f| f == "src/lib/storage.ts"))
            .unwrap();
        assert!(storage_cluster
            .files
            .iter()
            .any(|f| f == "src/lib/__tests__/storage.test.ts"));
    }
}
