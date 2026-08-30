use crate::error::{Result, ToolError};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type GraphRepresentation = (
    DiGraph<String, ()>,
    HashMap<String, NodeIndex>,
    HashMap<NodeIndex, String>,
);

/// Lifecycle status of an individual task node in the DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Failed,
}

/// An atomic task item within a structured feature plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskItem {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default = "default_complexity")]
    pub complexity_score: u8, // 1 to 10 scale
    #[serde(default = "default_status")]
    pub status: TaskStatus,
    #[serde(default)]
    pub assigned_role: Option<String>,
    #[serde(default)]
    pub worktree_branch: Option<String>,
}

fn default_complexity() -> u8 {
    3
}

fn default_status() -> TaskStatus {
    TaskStatus::Pending
}

/// Dependency-managed Directed Acyclic Graph (DAG) for software task execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskDag {
    pub tasks: HashMap<String, TaskItem>,
}

impl TaskDag {
    /// Creates a new empty Task DAG.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// Persists the DAG state to `.minicode/task_dag.json`
    pub fn save(&self, workspace_root: &std::path::Path) -> Result<()> {
        let dir = workspace_root.join(".minicode");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("task_dag.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Loads the persisted DAG state from `.minicode/task_dag.json`
    pub fn load(workspace_root: &std::path::Path) -> Result<Self> {
        let path = workspace_root.join(".minicode").join("task_dag.json");
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        let dag: Self = serde_json::from_str(&content)?;
        Ok(dag)
    }

    /// Adds a task to the DAG.
    pub fn add_task(&mut self, mut task: TaskItem) {
        if task.complexity_score == 0 {
            task.complexity_score = Self::estimate_complexity(&task);
        }
        self.tasks.insert(task.id.clone(), task);
    }

    /// Estimates task complexity heuristics (1 to 10) based on scope, descriptions, and dependencies.
    pub fn estimate_complexity(task: &TaskItem) -> u8 {
        let mut score = 2u8;

        // Keyword risk weighting
        let desc_lower = task.description.to_lowercase();
        if desc_lower.contains("refactor")
            || desc_lower.contains("migrate")
            || desc_lower.contains("rewrite")
        {
            score += 3;
        }
        if desc_lower.contains("database")
            || desc_lower.contains("concurrency")
            || desc_lower.contains("async")
        {
            score += 2;
        }
        if desc_lower.contains("security")
            || desc_lower.contains("auth")
            || desc_lower.contains("protocol")
        {
            score += 2;
        }

        // Dependency depth weighting
        score += (task.dependencies.len() as u8).min(3);

        score.clamp(1, 10)
    }

    /// Builds a petgraph `DiGraph` representation for cycle detection and topological sorting.
    fn build_graph(&self) -> Result<GraphRepresentation> {
        let mut graph = DiGraph::new();
        let mut id_to_node = HashMap::new();
        let mut node_to_id = HashMap::new();

        for id in self.tasks.keys() {
            let idx = graph.add_node(id.clone());
            id_to_node.insert(id.clone(), idx);
            node_to_id.insert(idx, id.clone());
        }

        for (id, task) in &self.tasks {
            let target_node = id_to_node[id];
            for dep in &task.dependencies {
                if let Some(&source_node) = id_to_node.get(dep) {
                    // Directed edge: source (dependency) -> target (dependent)
                    graph.add_edge(source_node, target_node, ());
                } else {
                    return Err(ToolError::InvalidArguments {
                        name: "task_dag".to_string(),
                        reason: format!(
                            "Task '{}' references nonexistent dependency '{}'",
                            id, dep
                        ),
                    }
                    .into());
                }
            }
        }

        Ok((graph, id_to_node, node_to_id))
    }

    /// Returns the topological execution order of tasks if no cycles exist.
    pub fn topological_order(&self) -> Result<Vec<String>> {
        let (graph, _, node_to_id) = self.build_graph()?;
        match toposort(&graph, None) {
            Ok(nodes) => {
                let ordered = nodes
                    .into_iter()
                    .map(|idx| node_to_id[&idx].clone())
                    .collect();
                Ok(ordered)
            }
            Err(cycle) => {
                let node_id = &node_to_id[&cycle.node_id()];
                Err(ToolError::InvalidArguments {
                    name: "task_dag".to_string(),
                    reason: format!("Circular dependency detected involving task '{}'", node_id),
                }
                .into())
            }
        }
    }

    /// Calculates parallel execution waves (sets of independent tasks that can run concurrently).
    pub fn calculate_execution_waves(&self) -> Result<Vec<Vec<String>>> {
        let topo = self.topological_order()?;
        let mut waves: Vec<Vec<String>> = Vec::new();
        let mut task_to_wave: HashMap<String, usize> = HashMap::new();

        for task_id in &topo {
            let task = &self.tasks[task_id];
            let mut wave_idx = 0;

            for dep in &task.dependencies {
                if let Some(&dep_wave) = task_to_wave.get(dep) {
                    wave_idx = wave_idx.max(dep_wave + 1);
                }
            }

            task_to_wave.insert(task_id.clone(), wave_idx);

            while waves.len() <= wave_idx {
                waves.push(Vec::new());
            }

            waves[wave_idx].push(task_id.clone());
        }

        Ok(waves)
    }

    /// Dynamically splits a high-complexity task into child subtasks, rewiring dependencies cleanly.
    pub fn split_task(
        &mut self,
        parent_id: &str,
        child_tasks: Vec<TaskItem>,
    ) -> Result<Vec<String>> {
        if child_tasks.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "split_task".to_string(),
                reason: "Child tasks list cannot be empty".to_string(),
            }
            .into());
        }

        let parent_task =
            self.tasks
                .get(parent_id)
                .cloned()
                .ok_or_else(|| ToolError::NotFound {
                    name: format!("task:{}", parent_id),
                })?;

        let parent_deps = parent_task.dependencies.clone();
        let mut child_ids = Vec::new();

        // 1. Add child tasks inheriting parent's upstream dependencies for initial child
        for (idx, mut child) in child_tasks.into_iter().enumerate() {
            if idx == 0 && child.dependencies.is_empty() {
                child.dependencies = parent_deps.clone();
            }
            child_ids.push(child.id.clone());
            self.add_task(child);
        }

        // 2. Point all downstream tasks previously depending on parent to depend on child tasks
        if let Some(last_child_id) = child_ids.last() {
            for task in self.tasks.values_mut() {
                if task.dependencies.contains(&parent_id.to_string()) {
                    task.dependencies.retain(|d| d != parent_id);
                    if !task.dependencies.contains(last_child_id) {
                        task.dependencies.push(last_child_id.clone());
                    }
                }
            }
        }

        // 3. Mark parent task as Completed (or remove)
        self.set_task_status(parent_id, TaskStatus::Completed)?;

        // 4. Validate graph integrity (no cycles)
        self.topological_order()?;

        Ok(child_ids)
    }

    /// Returns all unblocked tasks ready for immediate execution.
    pub fn next_executable_tasks(&self) -> Vec<&TaskItem> {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending)
            .filter(|t| {
                // All dependencies must be Completed
                t.dependencies.iter().all(|dep_id| {
                    self.tasks
                        .get(dep_id)
                        .map(|dep| dep.status == TaskStatus::Completed)
                        .unwrap_or(false)
                })
            })
            .collect()
    }

    /// Marks a task status and updates dependent task readiness.
    pub fn set_task_status(&mut self, task_id: &str, status: TaskStatus) -> Result<()> {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = status;
            Ok(())
        } else {
            Err(ToolError::NotFound {
                name: format!("task:{}", task_id),
            }
            .into())
        }
    }

    /// Generates a human-readable complexity and progress report.
    pub fn generate_report(&self) -> String {
        let total = self.tasks.len();
        let completed = self
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Completed)
            .count();
        let in_progress = self
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::InProgress)
            .count();
        let pending = self
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending)
            .count();
        let failed = self
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Failed)
            .count();

        let progress_pct = if total > 0 {
            (completed as f64 / total as f64 * 100.0) as usize
        } else {
            0
        };

        let mut out = format!(
            "📊 Task DAG Progress: {}% ({} total: {} completed, {} in progress, {} pending, {} failed)\n\n",
            progress_pct, total, completed, in_progress, pending, failed
        );

        if let Ok(waves) = self.calculate_execution_waves() {
            out.push_str("⚡ Parallel Execution Waves:\n");
            for (idx, wave) in waves.iter().enumerate() {
                out.push_str(&format!("  • Wave {}: {}\n", idx + 1, wave.join(", ")));
            }
            out.push('\n');
        }

        if let Ok(order) = self.topological_order() {
            out.push_str("Topological Task Order:\n");
            for id in order {
                if let Some(t) = self.tasks.get(&id) {
                    let status_badge = match t.status {
                        TaskStatus::Completed => "✔ Completed",
                        TaskStatus::InProgress => "◉ In Progress",
                        TaskStatus::Pending => "○ Pending",
                        TaskStatus::Blocked => "⏸ Blocked",
                        TaskStatus::Failed => "✗ Failed",
                    };
                    out.push_str(&format!(
                        "  [{}] `{}`: {} (Complexity: {}/10)\n",
                        status_badge, t.id, t.title, t.complexity_score
                    ));
                    if !t.dependencies.is_empty() {
                        out.push_str(&format!(
                            "      ↳ Depends on: {}\n",
                            t.dependencies.join(", ")
                        ));
                    }
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_dag_topological_sort_and_waves() {
        let mut dag = TaskDag::new();

        dag.add_task(TaskItem {
            id: "t1".to_string(),
            title: "Task 1".to_string(),
            description: "First task".to_string(),
            dependencies: vec![],
            complexity_score: 3,
            status: TaskStatus::Pending,
            assigned_role: None,
            worktree_branch: None,
        });

        dag.add_task(TaskItem {
            id: "t2".to_string(),
            title: "Task 2".to_string(),
            description: "Second task (parallel with t3)".to_string(),
            dependencies: vec!["t1".to_string()],
            complexity_score: 4,
            status: TaskStatus::Pending,
            assigned_role: None,
            worktree_branch: None,
        });

        dag.add_task(TaskItem {
            id: "t3".to_string(),
            title: "Task 3".to_string(),
            description: "Third task (parallel with t2)".to_string(),
            dependencies: vec!["t1".to_string()],
            complexity_score: 2,
            status: TaskStatus::Pending,
            assigned_role: None,
            worktree_branch: None,
        });

        dag.add_task(TaskItem {
            id: "t4".to_string(),
            title: "Task 4".to_string(),
            description: "Final task".to_string(),
            dependencies: vec!["t2".to_string(), "t3".to_string()],
            complexity_score: 5,
            status: TaskStatus::Pending,
            assigned_role: None,
            worktree_branch: None,
        });

        let waves = dag.calculate_execution_waves().unwrap();
        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0], vec!["t1"]);
        assert!(waves[1].contains(&"t2".to_string()));
        assert!(waves[1].contains(&"t3".to_string()));
        assert_eq!(waves[2], vec!["t4"]);
    }

    #[test]
    fn test_task_dag_cycle_detection() {
        let mut dag = TaskDag::new();

        dag.add_task(TaskItem {
            id: "a".to_string(),
            title: "Task A".to_string(),
            description: "Depends on B".to_string(),
            dependencies: vec!["b".to_string()],
            complexity_score: 3,
            status: TaskStatus::Pending,
            assigned_role: None,
            worktree_branch: None,
        });

        dag.add_task(TaskItem {
            id: "b".to_string(),
            title: "Task B".to_string(),
            description: "Depends on A".to_string(),
            dependencies: vec!["a".to_string()],
            complexity_score: 3,
            status: TaskStatus::Pending,
            assigned_role: None,
            worktree_branch: None,
        });

        assert!(dag.topological_order().is_err());
    }

    #[test]
    fn test_task_dag_dynamic_split() {
        let mut dag = TaskDag::new();

        dag.add_task(TaskItem {
            id: "t1".to_string(),
            title: "Init".to_string(),
            description: "Init".to_string(),
            dependencies: vec![],
            complexity_score: 2,
            status: TaskStatus::Completed,
            assigned_role: None,
            worktree_branch: None,
        });

        dag.add_task(TaskItem {
            id: "big_task".to_string(),
            title: "Big Complex Task".to_string(),
            description: "Refactor engine".to_string(),
            dependencies: vec!["t1".to_string()],
            complexity_score: 8,
            status: TaskStatus::Pending,
            assigned_role: None,
            worktree_branch: None,
        });

        dag.add_task(TaskItem {
            id: "t3".to_string(),
            title: "Verify".to_string(),
            description: "Verify all".to_string(),
            dependencies: vec!["big_task".to_string()],
            complexity_score: 3,
            status: TaskStatus::Pending,
            assigned_role: None,
            worktree_branch: None,
        });

        let subtasks = vec![
            TaskItem {
                id: "sub_1".to_string(),
                title: "Subtask 1".to_string(),
                description: "Part 1".to_string(),
                dependencies: vec![],
                complexity_score: 4,
                status: TaskStatus::Pending,
                assigned_role: None,
                worktree_branch: None,
            },
            TaskItem {
                id: "sub_2".to_string(),
                title: "Subtask 2".to_string(),
                description: "Part 2".to_string(),
                dependencies: vec!["sub_1".to_string()],
                complexity_score: 4,
                status: TaskStatus::Pending,
                assigned_role: None,
                worktree_branch: None,
            },
        ];

        let child_ids = dag.split_task("big_task", subtasks).unwrap();
        assert_eq!(child_ids, vec!["sub_1", "sub_2"]);

        // t3 should now depend on sub_2
        assert!(dag.tasks["t3"].dependencies.contains(&"sub_2".to_string()));
        assert!(!dag.tasks["t3"]
            .dependencies
            .contains(&"big_task".to_string()));
    }
}
