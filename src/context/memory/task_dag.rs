//! Unified `petgraph` Topological Task-DAG & Write-Boundary Conflict Coloring (`task_dag`).
//!
//! Bridges `create_plan` / `update_progress` (`WorkingMemory`) and `execute_task_dag` (`SwarmScheduler`):
//! 1. Validates acyclicity and computes longest-path topological waves via `petgraph::algo::toposort`.
//! 2. Constructs per-wave Write-Boundary Conflict Graphs over normalized `target_files` path prefixes
//!    and applies Welsh-Powell greedy graph coloring to partition waves into 100% conflict-free
//!    parallel sub-waves (preventing both shared-workspace races and worktree merge conflicts).
//! 3. Persists `.minicode/plan/task_dag.json` and synchronizes live task statuses with `task_plan.md` / `todo.md`.

use crate::context::memory::working_memory::{TaskItem, TaskItemStatus, WorkingMemory};
use crate::error::{ContextError, Result};
use chrono::Utc;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Canonical path under `.minicode/plan/` storing the structured Task-DAG.
pub const TASK_DAG_FILENAME: &str = "task_dag.json";

/// A single node in the unified `.minicode/plan/task_dag.json` graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanDagNode {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub substeps: Vec<String>,
    #[serde(default)]
    pub target_files: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_cmd: Option<String>,
    pub status: TaskItemStatus,
}

/// Persistent Task-DAG representation unifying Tier-1/Tier-2 plans with parallel swarm waves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanDag {
    pub title: String,
    /// `true` when at least one step explicitly declared `depends_on` / `dependencies`.
    #[serde(default)]
    pub explicit_dependencies: bool,
    pub nodes: Vec<PlanDagNode>,
    pub created_at: String,
    pub updated_at: String,
}

/// A conflict-free parallel sub-wave produced by topological level decomposition + graph coloring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSubWave {
    pub wave_index: usize,
    pub subwave_index: usize,
    pub node_indices: Vec<usize>,
    pub node_ids: Vec<String>,
    /// `true` when `node_ids.len() >= 2` (guaranteed disjoint `target_files` boundaries).
    pub parallel_safe: bool,
}

/// Complete topological & conflict-coloring schedule for a `PlanDag`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionTopology {
    pub topological_waves: Vec<Vec<usize>>,
    pub conflict_free_subwaves: Vec<ExecutionSubWave>,
    pub max_parallelism: usize,
}

/// Normalizes a workspace-relative path prefix for write-boundary overlap checks.
#[must_use]
pub fn normalize_boundary_path(raw: &str) -> String {
    let unified = raw.trim().replace('\\', "/");
    let stripped = unified.trim_start_matches("./").trim_end_matches('/');
    stripped.to_string()
}

/// Returns `true` when two normalized path prefixes overlap (`p == q`, `p` is an ancestor directory
/// of `q`, `q` is an ancestor directory of `p`, or either covers workspace root `"."`/`""`).
#[must_use]
pub fn boundary_paths_overlap(a: &str, b: &str) -> bool {
    let pa = normalize_boundary_path(a);
    let pb = normalize_boundary_path(b);
    if pa.is_empty() || pb.is_empty() || pa == "." || pb == "." {
        return true;
    }
    if pa == pb {
        return true;
    }
    if let Some(rest) = pa.strip_prefix(&pb) {
        if rest.starts_with('/') {
            return true;
        }
    }
    if let Some(rest) = pb.strip_prefix(&pa) {
        if rest.starts_with('/') {
            return true;
        }
    }
    false
}

fn is_readonly_role(role: Option<&str>) -> bool {
    let Some(r) = role else {
        return false;
    };
    matches!(
        r.trim().to_ascii_lowercase().as_str(),
        "scout" | "reviewer" | "architect" | "researcher" | "explorer" | "auditor"
    )
}

/// Returns `true` when two DAG nodes cannot safely mutate files concurrently in the same sub-wave.
#[must_use]
pub fn nodes_have_write_conflict(a: &PlanDagNode, b: &PlanDagNode) -> bool {
    if is_readonly_role(a.role.as_deref()) || is_readonly_role(b.role.as_deref()) {
        return false;
    }
    if a.target_files.is_empty() || b.target_files.is_empty() {
        return true;
    }
    a.target_files.iter().any(|fa| {
        b.target_files
            .iter()
            .any(|fb| boundary_paths_overlap(fa, fb))
    })
}

impl PlanDag {
    #[must_use]
    pub fn file_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".minicode")
            .join("plan")
            .join(TASK_DAG_FILENAME)
    }

    /// Loads `.minicode/plan/task_dag.json` if present and valid.
    #[must_use]
    pub fn load(workspace_root: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(Self::file_path(workspace_root)).ok()?;
        serde_json::from_str::<Self>(&raw).ok()
    }

    /// Persists `.minicode/plan/task_dag.json` atomically.
    pub fn save(&self, workspace_root: &Path) -> Result<()> {
        let path = Self::file_path(workspace_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ContextError::Memory(e.to_string()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let serialized =
            serde_json::to_string_pretty(self).map_err(|e| ContextError::Memory(e.to_string()))?;
        std::fs::write(&tmp, serialized).map_err(|e| ContextError::Memory(e.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|e| ContextError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Constructs a `PlanDag` from `create_plan` JSON arguments paired with the reconciled
    /// macro `steps` and `step_substeps`.
    pub fn from_plan_args(
        title: &str,
        raw_args: &Value,
        reconciled_steps: &[String],
        reconciled_substeps: &[Vec<String>],
    ) -> Result<Self> {
        let raw_items: Vec<&Value> = ["steps", "tasks", "plan"]
            .iter()
            .find_map(|k| raw_args.get(*k)?.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default();

        let explicit_dependencies = raw_items.iter().any(|item| {
            item.as_object()
                .is_some_and(|o| o.contains_key("depends_on") || o.contains_key("dependencies"))
        });

        let str_list = |obj: &serde_json::Map<String, Value>, keys: &[&str]| -> Vec<String> {
            keys.iter()
                .find_map(|k| obj.get(*k)?.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            v.as_str()
                                .map(str::to_string)
                                .or_else(|| v.as_u64().map(|n| n.to_string()))
                        })
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };

        let mut assigned_ids: Vec<String> = Vec::with_capacity(reconciled_steps.len());
        let mut seen_ids: HashSet<String> = HashSet::with_capacity(reconciled_steps.len());
        for idx in 0..reconciled_steps.len() {
            let candidate = raw_items
                .get(idx)
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("id"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| format!("t{}", idx + 1));
            let unique_id = if seen_ids.contains(&candidate) {
                format!("{}_{}", candidate, idx + 1)
            } else {
                candidate
            };
            seen_ids.insert(unique_id.clone());
            assigned_ids.push(unique_id);
        }

        let mut nodes = Vec::with_capacity(reconciled_steps.len());
        for (idx, step_str) in reconciled_steps.iter().enumerate() {
            let clean_title = WorkingMemory::sanitize_step_title(step_str);
            let subs = reconciled_substeps.get(idx).cloned().unwrap_or_default();
            let obj_opt = raw_items.get(idx).and_then(|v| v.as_object());

            let description = obj_opt
                .and_then(|o| o.get("description").or_else(|| o.get("details")))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| clean_title.clone());

            let mut target_files = obj_opt
                .map(|o| str_list(o, &["target_files", "files", "file_boundaries"]))
                .unwrap_or_default();
            if target_files.is_empty() {
                let mut combined = clean_title.clone();
                if !subs.is_empty() {
                    combined.push(' ');
                    combined.push_str(&subs.join(" "));
                }
                target_files = WorkingMemory::extract_candidate_files(&combined);
            }
            target_files = target_files
                .into_iter()
                .map(|p| normalize_boundary_path(&p))
                .filter(|p| !p.is_empty())
                .collect::<indexmap::IndexSet<_>>()
                .into_iter()
                .collect();

            let raw_deps = obj_opt
                .map(|o| str_list(o, &["depends_on", "dependencies"]))
                .unwrap_or_default();
            let depends_on: Vec<String> = if explicit_dependencies {
                raw_deps
                    .into_iter()
                    .map(|dep| {
                        if seen_ids.contains(&dep) {
                            return dep;
                        }
                        if let Ok(one_based) = dep.parse::<usize>() {
                            if one_based >= 1 && one_based <= assigned_ids.len() {
                                return assigned_ids[one_based - 1].clone();
                            }
                        }
                        dep
                    })
                    .filter(|d| d != &assigned_ids[idx])
                    .collect()
            } else if idx == 0 {
                Vec::new()
            } else {
                vec![assigned_ids[idx - 1].clone()]
            };

            let role = obj_opt
                .and_then(|o| o.get("role"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);

            let verify_cmd = obj_opt
                .and_then(|o| {
                    o.get("verify_cmd")
                        .or_else(|| o.get("check_cmd"))
                        .or_else(|| o.get("test_command"))
                })
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);

            nodes.push(PlanDagNode {
                id: assigned_ids[idx].clone(),
                title: clean_title,
                description,
                substeps: subs,
                target_files,
                depends_on,
                role,
                verify_cmd,
                status: if idx == 0 {
                    TaskItemStatus::InProgress
                } else {
                    TaskItemStatus::Pending
                },
            });
        }

        let now = Utc::now().to_rfc3339();
        let dag = Self {
            title: title.to_string(),
            explicit_dependencies,
            nodes,
            created_at: now.clone(),
            updated_at: now,
        };
        // Validate acyclicity and dependency references upfront
        let _ = dag.compute_execution_topology()?;
        Ok(dag)
    }

    /// Computes topological waves via `petgraph::algo::toposort` + longest-path DP, then colors
    /// each wave's Write-Boundary Conflict Graph into pairwise-disjoint parallel sub-waves.
    pub fn compute_execution_topology(&self) -> Result<ExecutionTopology> {
        if self.nodes.is_empty() {
            return Ok(ExecutionTopology::default());
        }

        let n = self.nodes.len();
        let mut graph = DiGraph::<usize, ()>::with_capacity(n, n);
        let mut id_to_idx: HashMap<&str, NodeIndex> = HashMap::with_capacity(n);
        let mut graph_nodes: Vec<NodeIndex> = Vec::with_capacity(n);

        for (i, node) in self.nodes.iter().enumerate() {
            if id_to_idx.contains_key(node.id.as_str()) {
                return Err(ContextError::Memory(format!(
                    "Duplicate task DAG node ID `{}`",
                    node.id
                ))
                .into());
            }
            let g_idx = graph.add_node(i);
            id_to_idx.insert(node.id.as_str(), g_idx);
            graph_nodes.push(g_idx);
        }

        for (i, node) in self.nodes.iter().enumerate() {
            let target = graph_nodes[i];
            for dep_id in &node.depends_on {
                let Some(&source) = id_to_idx.get(dep_id.as_str()) else {
                    return Err(ContextError::Memory(format!(
                        "Task `{}` depends on unknown task ID `{}`",
                        node.id, dep_id
                    ))
                    .into());
                };
                graph.add_edge(source, target, ());
            }
        }

        let topo_order = toposort(&graph, None).map_err(|cycle| {
            let cycle_idx = graph[cycle.node_id()];
            ContextError::Memory(format!(
                "Dependency cycle detected in Task-DAG involving `{}` (\"{}\")",
                self.nodes[cycle_idx].id, self.nodes[cycle_idx].title
            ))
        })?;

        let mut depth = vec![0usize; n];
        let mut max_depth = 0usize;
        for g_node in topo_order {
            let u = graph[g_node];
            let d = graph
                .neighbors_directed(g_node, Direction::Incoming)
                .map(|pred| depth[graph[pred]] + 1)
                .max()
                .unwrap_or(0);
            depth[u] = d;
            if d > max_depth {
                max_depth = d;
            }
        }

        let mut topological_waves = vec![Vec::new(); max_depth + 1];
        for (idx, &d) in depth.iter().enumerate() {
            topological_waves[d].push(idx);
        }

        let mut conflict_free_subwaves = Vec::new();
        let mut max_parallelism = 1usize;

        for (w_idx, wave_members) in topological_waves.iter().enumerate() {
            if wave_members.is_empty() {
                continue;
            }
            let colored = color_wave_conflicts(&self.nodes, wave_members);
            for (sub_idx, group) in colored.into_iter().enumerate() {
                let parallel_safe = group.len() >= 2;
                if group.len() > max_parallelism {
                    max_parallelism = group.len();
                }
                let node_ids = group.iter().map(|&i| self.nodes[i].id.clone()).collect();
                conflict_free_subwaves.push(ExecutionSubWave {
                    wave_index: w_idx,
                    subwave_index: sub_idx,
                    node_indices: group,
                    node_ids,
                    parallel_safe,
                });
            }
        }

        Ok(ExecutionTopology {
            topological_waves,
            conflict_free_subwaves,
            max_parallelism,
        })
    }

    /// Formats a concise execution topology summary for `create_plan` when parallel waves or
    /// explicit target boundaries are present.
    #[must_use]
    pub fn format_topology_note(&self) -> String {
        let has_metadata = self.explicit_dependencies
            || self
                .nodes
                .iter()
                .any(|n| !n.target_files.is_empty() || n.role.is_some() || n.verify_cmd.is_some());
        if !has_metadata || self.nodes.len() < 2 {
            return String::new();
        }
        let Ok(topo) = self.compute_execution_topology() else {
            return String::new();
        };
        let mut lines = Vec::new();
        for sw in &topo.conflict_free_subwaves {
            let labels: Vec<String> = sw
                .node_indices
                .iter()
                .map(|&idx| {
                    let n = &self.nodes[idx];
                    if n.target_files.is_empty() {
                        format!("`{}`", n.id)
                    } else {
                        format!("`{}` ({})", n.id, n.target_files.join(", "))
                    }
                })
                .collect();
            let mode_tag = if sw.parallel_safe {
                "parallel-safe · disjoint write sets"
            } else {
                "sequential"
            };
            lines.push(format!(
                "  - Wave {}.{} [{}]: {}",
                sw.wave_index,
                sw.subwave_index,
                mode_tag,
                labels.join(" ∥ ")
            ));
        }
        let dispatch_hint = if topo.max_parallelism >= 2 {
            "\n  ↳ Parallel sub-waves detected: call `execute_task_dag(from_active_plan=true)` to dispatch ready parallel waves automatically."
        } else {
            ""
        };
        format!(
            "\n• Task-DAG Topology (`.minicode/plan/task_dag.json`, max parallelism `{}`):\n{}{}",
            topo.max_parallelism,
            lines.join("\n"),
            dispatch_hint
        )
    }

    /// Synchronizes node statuses from `WorkingMemory` parsed top-level tasks so `task_dag.json`
    /// stays aligned with `todo.md` / `task_plan.md` completions.
    pub fn sync_statuses_from_tasks(&mut self, tasks: &[TaskItem]) -> bool {
        let mut changed = false;
        for task in tasks {
            let clean_t = WorkingMemory::sanitize_step_title(&task.title);
            if let Some(node) = self.nodes.iter_mut().find(|n| {
                n.title.eq_ignore_ascii_case(&clean_t)
                    || clean_t.contains(&n.title)
                    || n.title.contains(&clean_t)
            }) {
                if node.status != task.status {
                    node.status = task.status;
                    changed = true;
                }
            }
        }
        if changed {
            self.updated_at = Utc::now().to_rfc3339();
        }
        changed
    }

    /// Converts all uncompleted (`Pending` / `InProgress`) nodes in this `PlanDag` into a
    /// `SwarmPlan` ready for `SwarmScheduler` or conflict-free sub-wave execution, pruning
    /// dependencies on already-`Completed` upstream tasks.
    pub fn to_remaining_swarm_plan(&self) -> Result<crate::agent::swarm::SwarmPlan> {
        self.to_remaining_swarm_plan_with_workspace(None)
    }

    /// Workspace-enriched variant of `to_remaining_swarm_plan` that automatically injects
    /// settled `[D1..DN]` architectural rulings and Tree-sitter AST exported signatures from
    /// upstream `depends_on` files directly into each downstream worker's prompt.
    pub fn to_remaining_swarm_plan_with_workspace(
        &self,
        workspace_root: Option<&Path>,
    ) -> Result<crate::agent::swarm::SwarmPlan> {
        let completed_ids: HashSet<&str> = self
            .nodes
            .iter()
            .filter(|n| n.status == TaskItemStatus::Completed)
            .map(|n| n.id.as_str())
            .collect();

        let remaining_nodes: Vec<&PlanDagNode> = self
            .nodes
            .iter()
            .filter(|n| n.status != TaskItemStatus::Completed)
            .collect();

        if remaining_nodes.is_empty() {
            return Err(ContextError::Memory(
                "All tasks in `.minicode/plan/task_dag.json` are already completed.".to_string(),
            )
            .into());
        }

        let node_by_id: HashMap<&str, &PlanDagNode> =
            self.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let settled_decisions_opt =
            workspace_root.and_then(crate::agent::decisions::read_for_prompt);
        let cached_shards =
            workspace_root.and_then(crate::context::graph::graph_store::GraphStore::load_shards);
        let ast_extractor =
            workspace_root.map(|_| crate::context::repomap::RepoMapExtractor::new());

        let remaining_ids: HashSet<&str> = remaining_nodes.iter().map(|n| n.id.as_str()).collect();
        let mut tasks = Vec::with_capacity(remaining_nodes.len());

        for node in remaining_nodes {
            let active_deps: Vec<String> = node
                .depends_on
                .iter()
                .filter(|d| {
                    !completed_ids.contains(d.as_str()) && remaining_ids.contains(d.as_str())
                })
                .cloned()
                .collect();

            let mut prompt = format!("### Task `{}`: {}\n", node.id, node.title);
            if !node.description.is_empty() && node.description != node.title {
                prompt.push_str(&format!("\n{}\n", node.description));
            }
            if !node.substeps.is_empty() {
                prompt.push_str("\n#### Required Sub-Steps:\n");
                for (i, sub) in node.substeps.iter().enumerate() {
                    prompt.push_str(&format!("{}. {}\n", i + 1, sub));
                }
            }
            if !node.target_files.is_empty() {
                prompt.push_str(&format!(
                    "\n#### Assigned Write Boundaries (stay within these paths):\n- `{}`\n",
                    node.target_files.join("`\n- `")
                ));
            }

            // Inject exact AST exported signatures from upstream `depends_on` files already on disk
            if let (Some(ws), Some(ref extractor)) = (workspace_root, &ast_extractor) {
                let mut contract_lines = Vec::new();
                let mut seen_upstream_files = HashSet::new();
                for dep_id in &node.depends_on {
                    let Some(dep_node) = node_by_id.get(dep_id.as_str()) else {
                        continue;
                    };
                    for rel_file in &dep_node.target_files {
                        if !seen_upstream_files.insert(rel_file.as_str()) {
                            continue;
                        }
                        let sigs = collect_file_ast_signatures(
                            ws,
                            rel_file,
                            cached_shards.as_ref(),
                            extractor,
                        );
                        if !sigs.is_empty() {
                            contract_lines.push(format!(
                                "- `{}` (from `{}`): `{}`",
                                rel_file,
                                dep_id,
                                sigs.join("` · `")
                            ));
                        }
                    }
                }
                if !contract_lines.is_empty() {
                    prompt.push_str(
                        "\n#### Upstream AST Interface Contracts (reuse exact signatures):\n",
                    );
                    prompt.push_str(&contract_lines.join("\n"));
                    prompt.push('\n');
                }
            }

            if let Some(ref dec_md) = settled_decisions_opt {
                let compact_dec: String = dec_md.trim().chars().take(1200).collect();
                if !compact_dec.is_empty() {
                    prompt.push_str(
                        "\n#### Settled Architectural Decisions (`.minicode/decisions.md`):\n",
                    );
                    prompt.push_str(&compact_dec);
                    prompt.push('\n');
                }
            }

            tasks.push(crate::agent::swarm::SwarmTaskSpec {
                id: node.id.clone(),
                title: node.title.clone(),
                role_title: node.role.clone().unwrap_or_else(|| "coder".to_string()),
                instructions: String::new(),
                prompt,
                file_boundaries: node.target_files.clone(),
                dependencies: active_deps,
                check_command: node.verify_cmd.clone(),
                expected_artifacts: node.target_files.clone(),
                workspace_mode: None,
                max_iterations: None,
            });
        }

        let plan = crate::agent::swarm::SwarmPlan {
            id: format!("dag_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            title: self.title.clone(),
            objective: self.title.clone(),
            tasks,
            created_at: Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };
        plan.validate()
            .map_err(|e| ContextError::Memory(e.to_string()))?;
        Ok(plan)
    }
}

/// Extracts concise non-import AST symbol signatures (`pub struct`, `export interface`, `fn`, etc.)
/// for an upstream dependency file using cached `FileAstShard`s or fast in-memory Tree-sitter parsing.
fn collect_file_ast_signatures(
    workspace_root: &Path,
    rel_file: &str,
    cached_shards: Option<&HashMap<PathBuf, crate::context::graph::FileAstShard>>,
    extractor: &crate::context::repomap::RepoMapExtractor,
) -> Vec<String> {
    let clean_rel = normalize_boundary_path(rel_file);
    let abs_path = workspace_root.join(&clean_rel);
    if !abs_path.is_file() {
        return Vec::new();
    }

    if let Some(shards) = cached_shards {
        if let Some(shard) = shards
            .get(Path::new(&clean_rel))
            .or_else(|| shards.get(&abs_path))
        {
            let sigs: Vec<String> = shard
                .all_symbols
                .iter()
                .filter(|s| s.kind != "import" && !s.signature.trim().is_empty())
                .take(8)
                .map(|s| s.signature.trim().to_string())
                .collect();
            if !sigs.is_empty() {
                return sigs;
            }
        }
    }

    let Some(ext) = abs_path.extension().and_then(|e| e.to_str()) else {
        return Vec::new();
    };
    let Ok(code) = std::fs::read_to_string(&abs_path) else {
        return Vec::new();
    };
    if code.len() > 120_000 {
        return Vec::new();
    }
    extractor
        .extract_symbols_from_str(ext, &code)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.kind != "import" && !s.signature.trim().is_empty())
        .take(8)
        .map(|s| s.signature.trim().to_string())
        .collect()
}

/// Welsh-Powell degree-ordered greedy graph coloring on a single topological wave's
/// Write-Boundary Conflict Graph. Returns color classes where each class has 0 internal edges.
fn color_wave_conflicts(nodes: &[PlanDagNode], wave_members: &[usize]) -> Vec<Vec<usize>> {
    let k = wave_members.len();
    if k <= 1 {
        return vec![wave_members.to_vec()];
    }

    let mut adj = vec![vec![false; k]; k];
    let mut degrees = vec![0usize; k];
    for i in 0..k {
        for j in (i + 1)..k {
            if nodes_have_write_conflict(&nodes[wave_members[i]], &nodes[wave_members[j]]) {
                adj[i][j] = true;
                adj[j][i] = true;
                degrees[i] += 1;
                degrees[j] += 1;
            }
        }
    }

    // Order vertices by descending conflict degree, tie-breaking by original wave order
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&a, &b| degrees[b].cmp(&degrees[a]).then_with(|| a.cmp(&b)));

    let mut color_of: Vec<Option<usize>> = vec![None; k];
    let mut color_classes: Vec<Vec<usize>> = Vec::new();

    for &local_idx in &order {
        let mut assigned_color = None;
        for (c_idx, members_in_color) in color_classes.iter().enumerate() {
            let conflicts_with_color = members_in_color
                .iter()
                .any(|&other_local| adj[local_idx][other_local]);
            if !conflicts_with_color {
                assigned_color = Some(c_idx);
                break;
            }
        }
        match assigned_color {
            Some(c) => {
                color_of[local_idx] = Some(c);
                color_classes[c].push(local_idx);
            }
            None => {
                let c = color_classes.len();
                color_of[local_idx] = Some(c);
                color_classes.push(vec![local_idx]);
            }
        }
    }

    color_classes
        .into_iter()
        .map(|mut local_group| {
            local_group.sort_unstable();
            local_group.into_iter().map(|li| wave_members[li]).collect()
        })
        .collect()
}

/// Hydrates canonical `spec.md` when `spec.md` is still an untouched MiniKit skeleton stub
/// (`<= 20` non-empty lines with placeholder text) and settled decisions or architecture exist,
/// eliminating dead boilerplate `spec.md` files during `create_plan`.
pub fn hydrate_canonical_spec_if_stub(
    workspace_root: &Path,
    title: &str,
    architecture: Option<&str>,
    dag: &PlanDag,
) {
    let spec_path = crate::tools::minikit::resolve_doc_path(workspace_root, "spec.md");
    let spec_parent_exists = spec_path.parent().is_some_and(Path::exists);
    if !spec_path.exists() && !spec_parent_exists {
        return;
    }

    let is_stub = match std::fs::read_to_string(&spec_path) {
        Ok(existing) => {
            let non_empty_lines = existing.lines().filter(|l| !l.trim().is_empty()).count();
            non_empty_lines <= 20
                && (existing.contains("Core invariants, interfaces, and constraints")
                    || existing.contains("_Placeholder")
                    || non_empty_lines <= 8)
        }
        Err(_) => true,
    };
    if !is_stub {
        return;
    }

    let decisions = crate::agent::decisions::read_for_prompt(workspace_root);
    if architecture.is_none() && decisions.is_none() && dag.nodes.is_empty() {
        return;
    }

    let mut content = format!(
        "---\ntype: Specification\ntitle: \"Technical Specification — {}\"\nstatus: active\n---\n\n# Technical Specification — {}\n\n",
        title.replace('"', "'"),
        title
    );

    if let Some(arch) = architecture.map(str::trim).filter(|a| !a.is_empty()) {
        content.push_str("## 1. Architecture & System Invariants\n\n");
        content.push_str(arch);
        content.push_str("\n\n");
    }

    if let Some(dec_md) = decisions {
        content.push_str("## 2. Settled Architectural Decisions\n\n");
        content.push_str(dec_md.trim());
        content.push_str("\n\n");
    }

    if !dag.nodes.is_empty() {
        content.push_str("## 3. Module Ownership & Verification Contracts\n\n");
        for node in &dag.nodes {
            let files_part = if node.target_files.is_empty() {
                String::new()
            } else {
                format!(" — files: `{}`", node.target_files.join("`, `"))
            };
            let verify_part = node
                .verify_cmd
                .as_deref()
                .map(|c| format!(" (verify: `{}`)", c))
                .unwrap_or_default();
            content.push_str(&format!(
                "- **`{}`**: {}{}{}\n",
                node.id, node.title, files_part, verify_part
            ));
        }
    }

    let _ = std::fs::write(&spec_path, content);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_petgraph_topological_waves_and_write_boundary_coloring() {
        let args = json!({
            "title": "Fullstack Feature",
            "steps": [
                {
                    "id": "t1_schema",
                    "title": "Define shared schema",
                    "target_files": ["src/types/schema.ts"],
                    "depends_on": []
                },
                {
                    "id": "t2_hero",
                    "title": "Build Hero component",
                    "target_files": ["src/components/Hero.tsx"],
                    "depends_on": ["t1_schema"]
                },
                {
                    "id": "t3_pricing",
                    "title": "Build Pricing component",
                    "target_files": ["src/components/Pricing.tsx"],
                    "depends_on": ["t1_schema"]
                },
                {
                    "id": "t4_hero_polish",
                    "title": "Add Hero animations",
                    "target_files": ["src/components/Hero.tsx"],
                    "depends_on": ["t1_schema"]
                },
                {
                    "id": "t5_app",
                    "title": "Wire App & verify",
                    "target_files": ["src/App.tsx"],
                    "depends_on": ["t2_hero", "t3_pricing", "t4_hero_polish"],
                    "verify_cmd": "npm run build"
                }
            ]
        });

        let steps: Vec<String> = vec![
            "Define shared schema".into(),
            "Build Hero component".into(),
            "Build Pricing component".into(),
            "Add Hero animations".into(),
            "Wire App & verify".into(),
        ];
        let subs = vec![Vec::new(); 5];
        let dag = PlanDag::from_plan_args("Fullstack Feature", &args, &steps, &subs).unwrap();
        let topo = dag.compute_execution_topology().unwrap();

        // 3 topological depths: Wave 0 ([t1]), Wave 1 ([t2, t3, t4]), Wave 2 ([t5])
        assert_eq!(topo.topological_waves.len(), 3);
        assert_eq!(topo.topological_waves[0], vec![0]);
        assert_eq!(topo.topological_waves[1], vec![1, 2, 3]);
        assert_eq!(topo.topological_waves[2], vec![4]);

        // Inside Wave 1: `t2_hero` and `t4_hero_polish` touch the SAME file (`src/components/Hero.tsx`)
        // so graph coloring MUST separate them into different sub-waves, while pairing `t3_pricing`
        // (`src/components/Pricing.tsx`) in parallel with one of them!
        let wave1_subwaves: Vec<&ExecutionSubWave> = topo
            .conflict_free_subwaves
            .iter()
            .filter(|sw| sw.wave_index == 1)
            .collect();
        assert_eq!(wave1_subwaves.len(), 2);
        assert_eq!(topo.max_parallelism, 2);

        for sw in &wave1_subwaves {
            let has_t2 = sw.node_ids.contains(&"t2_hero".to_string());
            let has_t4 = sw.node_ids.contains(&"t4_hero_polish".to_string());
            assert!(
                !(has_t2 && has_t4),
                "Conflicting writers on src/components/Hero.tsx must never share a parallel sub-wave: {:?}",
                sw.node_ids
            );
        }
    }

    #[test]
    fn test_cycle_detection_rejects_circular_task_dag() {
        let args = json!({
            "steps": [
                { "id": "a", "title": "Step A", "depends_on": ["b"] },
                { "id": "b", "title": "Step B", "depends_on": ["a"] }
            ]
        });
        let steps = vec!["Step A".to_string(), "Step B".to_string()];
        let subs = vec![Vec::new(); 2];
        let err = PlanDag::from_plan_args("Cyclic", &args, &steps, &subs).unwrap_err();
        assert!(err.to_string().contains("cycle detected"));
    }

    #[test]
    fn test_downstream_wave_workers_receive_upstream_ast_signatures_and_settled_decisions() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::write(
            ws.join("src/schema.ts"),
            "export interface MetricPoint { timestamp: number; value: number; }\nexport function normalizeSeries(pts: MetricPoint[]): MetricPoint[] { return pts; }\n",
        )
        .unwrap();
        crate::agent::decisions::upsert_decision(
            ws,
            "Chart Library",
            "Render SVG sparklines with zero third-party chart packages",
            None,
        )
        .unwrap();

        let raw_args = json!({
            "steps": [
                { "id": "t1_schema", "title": "Define schema", "target_files": ["src/schema.ts"], "depends_on": [] },
                { "id": "t2_chart", "title": "Build Sparkline", "target_files": ["src/Sparkline.tsx"], "depends_on": ["t1_schema"] }
            ]
        });
        let steps = vec!["Define schema".to_string(), "Build Sparkline".to_string()];
        let mut dag =
            PlanDag::from_plan_args("Metrics", &raw_args, &steps, &vec![Vec::new(); 2]).unwrap();
        dag.nodes[0].status = TaskItemStatus::Completed;

        let swarm_plan = dag
            .to_remaining_swarm_plan_with_workspace(Some(ws))
            .unwrap();
        assert_eq!(swarm_plan.tasks.len(), 1);
        let worker_prompt = &swarm_plan.tasks[0].prompt;
        assert!(
            worker_prompt.contains("Upstream AST Interface Contracts"),
            "missing upstream AST contracts in worker prompt:\n{worker_prompt}"
        );
        assert!(worker_prompt.contains("interface MetricPoint"));
        assert!(worker_prompt.contains("normalizeSeries"));
        assert!(worker_prompt.contains("[D1]"));
        assert!(worker_prompt.contains("SVG sparklines"));
    }
}
