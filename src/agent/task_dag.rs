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
}

fn default_complexity() -> u8 {
    3
}

fn default_status() -> TaskStatus {
    TaskStatus::Pending
}

/// Dependency-managed Directed Acyclic Graph (DAG) for software task execution.
#[derive(Debug, Clone, Default)]
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
    fn build_graph(
        &self,
    ) -> Result<GraphRepresentation> {
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

        let mut out = format!(
            "📊 Task DAG Execution Report: {} total (✔ {} completed | ⏳ {} in progress | ⏸ {} pending | ❌ {} failed)\n\n",
            total, completed, in_progress, pending, failed
        );

        let order = self
            .topological_order()
            .unwrap_or_else(|_| self.tasks.keys().cloned().collect());
        for (idx, id) in order.iter().enumerate() {
            if let Some(task) = self.tasks.get(id) {
                let status_icon = match task.status {
                    TaskStatus::Completed => "✔ [COMPLETED]",
                    TaskStatus::InProgress => "⏳ [IN PROGRESS]",
                    TaskStatus::Pending => "⏸ [PENDING]",
                    TaskStatus::Blocked => "🚫 [BLOCKED]",
                    TaskStatus::Failed => "❌ [FAILED]",
                };

                let deps_str = if task.dependencies.is_empty() {
                    "none".to_string()
                } else {
                    task.dependencies.join(", ")
                };

                out.push_str(&format!(
                    "{}. {} `{}` (Complexity: {}/10, Deps: {})\n   **{}**: {}\n\n",
                    idx + 1,
                    status_icon,
                    task.id,
                    task.complexity_score,
                    deps_str,
                    task.title,
                    task.description
                ));
            }
        }

        out
    }

    /// Loads the active TaskDag from `.minicode/task_dag.json` in the workspace.
    pub fn load(workspace_root: &std::path::Path) -> Result<Self> {
        let path = workspace_root.join(".minicode").join("task_dag.json");
        if !path.exists() {
            return Ok(Self::new());
        }
        let data = std::fs::read_to_string(&path).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;
        let tasks: HashMap<String, TaskItem> = serde_json::from_str(&data).unwrap_or_default();
        Ok(Self { tasks })
    }

    /// Persists the TaskDag to `.minicode/task_dag.json` in the workspace.
    pub fn save(&self, workspace_root: &std::path::Path) -> Result<()> {
        let dir = workspace_root.join(".minicode");
        std::fs::create_dir_all(&dir).map_err(|e| ToolError::FileOp {
            path: dir.display().to_string(),
            source: e,
        })?;
        let path = dir.join("task_dag.json");
        let json_str = serde_json::to_string_pretty(&self.tasks)
            .map_err(|e| ToolError::CommandExec(e.to_string()))?;
        std::fs::write(&path, json_str).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_dag_topological_order_and_execution() {
        let mut dag = TaskDag::new();
        dag.add_task(TaskItem {
            id: "task-1".to_string(),
            title: "Setup database schema".to_string(),
            description: "Create SQLite migrations".to_string(),
            dependencies: vec![],
            complexity_score: 3,
            status: TaskStatus::Pending,
        });

        dag.add_task(TaskItem {
            id: "task-2".to_string(),
            title: "Implement repository layer".to_string(),
            description: "CRUD queries for users".to_string(),
            dependencies: vec!["task-1".to_string()],
            complexity_score: 5,
            status: TaskStatus::Pending,
        });

        dag.add_task(TaskItem {
            id: "task-3".to_string(),
            title: "Implement HTTP router".to_string(),
            description: "Axum routes".to_string(),
            dependencies: vec!["task-2".to_string()],
            complexity_score: 4,
            status: TaskStatus::Pending,
        });

        let order = dag.topological_order().unwrap();
        assert_eq!(order, vec!["task-1", "task-2", "task-3"]);

        // Initially only task-1 is executable
        let next = dag.next_executable_tasks();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].id, "task-1");

        // Complete task-1
        dag.set_task_status("task-1", TaskStatus::Completed)
            .unwrap();
        let next2 = dag.next_executable_tasks();
        assert_eq!(next2.len(), 1);
        assert_eq!(next2[0].id, "task-2");
    }

    #[test]
    fn test_task_dag_circular_dependency_detected() {
        let mut dag = TaskDag::new();
        dag.add_task(TaskItem {
            id: "a".to_string(),
            title: "Task A".to_string(),
            description: "Desc A".to_string(),
            dependencies: vec!["b".to_string()],
            complexity_score: 3,
            status: TaskStatus::Pending,
        });

        dag.add_task(TaskItem {
            id: "b".to_string(),
            title: "Task B".to_string(),
            description: "Desc B".to_string(),
            dependencies: vec!["a".to_string()],
            complexity_score: 3,
            status: TaskStatus::Pending,
        });

        let res = dag.topological_order();
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Circular dependency"));
    }
}
