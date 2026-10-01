//! Domain models, task specifications, and DAG validation for multi-agent swarms.
//!
//! Provides dynamic, non-hardcoded task specifications with DAG dependency resolution,
//! topological wave calculation, artifact passing definitions, and execution state tracking.

use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

use crate::agent::subagent::types::WorkspaceMode;

/// Errors that can occur during swarm planning, scheduling, or execution.
#[allow(dead_code)]
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SwarmError {
    #[error("Swarm planning failed: {0}")]
    PlanningFailed(String),

    #[error("Circular dependency detected in swarm DAG involving task '{0}'")]
    CycleDetected(String),

    #[error("Task '{task_id}' references nonexistent dependency '{dep_id}'")]
    MissingDependency { task_id: String, dep_id: String },

    #[error("Duplicate task ID '{0}' found in swarm plan")]
    DuplicateTaskId(String),

    #[error("Swarm plan contains no tasks")]
    EmptyPlan,

    #[error("Task '{task_id}' failed: {reason}")]
    WorkerFailed { task_id: String, reason: String },

    #[error("Verification command '{command}' failed for task '{task_id}' with exit code {exit_code}: {stderr}")]
    VerificationFailed {
        task_id: String,
        command: String,
        exit_code: i32,
        stderr: String,
    },

    #[error("Merge arbitration failed: {0}")]
    ArbitrationFailed(String),

    #[error("Swarm execution was cancelled")]
    Cancelled,

    #[error("Message payload exceeds 800 characters (actual: {0})")]
    MessagePayloadTooLarge(usize),

    #[error(
        "Message quota exceeded: worker '{0}' has reached the limit of 3 messages for this wave"
    )]
    MessageQuotaExceeded(String),

    #[error("Invalid recipient: worker cannot send a message to itself")]
    SelfMessageNotAllowed,

    #[error("IO or filesystem error: {0}")]
    IoError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl From<std::io::Error> for SwarmError {
    fn from(err: std::io::Error) -> Self {
        SwarmError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for SwarmError {
    fn from(err: serde_json::Error) -> Self {
        SwarmError::SerializationError(err.to_string())
    }
}

/// Lifecycle status of an individual task node in the swarm DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwarmTaskStatus {
    /// Task is waiting for upstream dependencies to complete.
    Pending,
    /// Dependencies are satisfied; task is queued and ready to be scheduled.
    Ready,
    /// Task is currently running in an isolated worker workspace.
    Running,
    /// Worker completed; executing automated verification check.
    Verifying,
    /// Worker finished successfully and passed verification.
    Completed,
    /// Worker exited with error or failed verification.
    Failed,
    /// Task was blocked because an upstream dependency failed.
    Blocked,
    /// Task was cancelled by user or swarm abort.
    Cancelled,
}

impl SwarmTaskStatus {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Pending => "○ Pending",
            Self::Ready => "▶ Ready",
            Self::Running => "● Running",
            Self::Verifying => "🔍 Verifying",
            Self::Completed => "✔ Completed",
            Self::Failed => "✖ Failed",
            Self::Blocked => "⊘ Blocked",
            Self::Cancelled => "⊝ Cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Blocked | Self::Cancelled
        )
    }
}

impl fmt::Display for SwarmTaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.badge())
    }
}

/// Specification for an individual worker task within a dynamic swarm plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmTaskSpec {
    /// Unique identifier for this task (e.g. "t1_api_backend", "t2_frontend_ui").
    pub id: String,

    /// Short human-readable title.
    pub title: String,

    /// Specialized, dynamically synthesized role title (e.g. "FastAPI Database Architect", "WebGL Shader Specialist").
    #[serde(default = "default_role_title")]
    pub role_title: String,

    /// Synthesized system instructions & guidelines tailored to this worker.
    #[serde(default)]
    pub instructions: String,

    /// The exact task prompt / feature requirements for this worker.
    pub prompt: String,

    /// Relative file path boundaries / globs this worker is assigned to touch (prevents clobbering).
    #[serde(default)]
    pub file_boundaries: Vec<String>,

    /// Task IDs that MUST successfully complete before this task can run.
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Automated command to verify correctness (e.g. "cargo test", "pytest tests/test_api.py", "npm test").
    #[serde(default)]
    pub check_command: Option<String>,

    /// Relative file paths or artifacts this task produces for downstream workers to consume.
    #[serde(default)]
    pub expected_artifacts: Vec<String>,

    /// Workspace isolation mode (defaults to Worktree for independent mutating work).
    #[serde(default)]
    pub workspace_mode: Option<WorkspaceMode>,

    /// Maximum tool call iterations allowed for this worker turn.
    #[serde(default)]
    pub max_iterations: Option<usize>,
}

fn default_role_title() -> String {
    "Autonomous Software Engineer".to_string()
}

/// Specification for an entire multi-agent swarm execution plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmPlan {
    /// Unique identifier for this swarm run.
    pub id: String,

    /// High-level title for the swarm objective.
    pub title: String,

    /// The original user objective or requirement.
    pub objective: String,

    /// List of task specifications forming the execution DAG.
    pub tasks: Vec<SwarmTaskSpec>,

    /// ISO timestamp when the plan was created.
    #[serde(default = "default_timestamp")]
    pub created_at: String,

    /// Additional metadata (e.g. detected stack, target branch, user flags).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

type SwarmGraphRepresentation = (
    DiGraph<String, ()>,
    HashMap<String, NodeIndex>,
    HashMap<NodeIndex, String>,
);

impl SwarmPlan {
    /// Validates the plan for uniqueness of IDs, existence of all dependencies, and absence of cycles.
    pub fn validate(&self) -> Result<(), SwarmError> {
        if self.tasks.is_empty() {
            return Err(SwarmError::EmptyPlan);
        }

        let mut seen_ids = HashSet::new();
        for task in &self.tasks {
            if !seen_ids.insert(&task.id) {
                return Err(SwarmError::DuplicateTaskId(task.id.clone()));
            }
        }

        for task in &self.tasks {
            for dep in &task.dependencies {
                if !seen_ids.contains(dep) {
                    return Err(SwarmError::MissingDependency {
                        task_id: task.id.clone(),
                        dep_id: dep.clone(),
                    });
                }
            }
        }

        // Verify DAG is acyclic via toposort
        self.topological_order()?;

        Ok(())
    }

    /// Builds a petgraph DiGraph for cycle detection and topological sorting.
    fn build_graph(&self) -> Result<SwarmGraphRepresentation, SwarmError> {
        let mut graph = DiGraph::new();
        let mut id_to_node = HashMap::new();
        let mut node_to_id = HashMap::new();

        for task in &self.tasks {
            let idx = graph.add_node(task.id.clone());
            id_to_node.insert(task.id.clone(), idx);
            node_to_id.insert(idx, task.id.clone());
        }

        for task in &self.tasks {
            let target_node = id_to_node[&task.id];
            for dep in &task.dependencies {
                if let Some(&source_node) = id_to_node.get(dep) {
                    // Directed edge: source (upstream dependency) -> target (downstream dependent)
                    graph.add_edge(source_node, target_node, ());
                } else {
                    return Err(SwarmError::MissingDependency {
                        task_id: task.id.clone(),
                        dep_id: dep.clone(),
                    });
                }
            }
        }

        Ok((graph, id_to_node, node_to_id))
    }

    /// Returns the topological execution order of tasks if no cycles exist.
    pub fn topological_order(&self) -> Result<Vec<String>, SwarmError> {
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
                Err(SwarmError::CycleDetected(node_id.clone()))
            }
        }
    }

    /// Calculates parallel execution waves (sets of independent tasks that can run concurrently).
    pub fn calculate_waves(&self) -> Result<Vec<Vec<String>>, SwarmError> {
        let topo = self.topological_order()?;
        let mut waves: Vec<Vec<String>> = Vec::new();
        let mut task_to_wave: HashMap<String, usize> = HashMap::new();

        let task_map: HashMap<&str, &SwarmTaskSpec> =
            self.tasks.iter().map(|t| (t.id.as_str(), t)).collect();

        for task_id in &topo {
            let task = task_map[task_id.as_str()];
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

    /// Returns a specific task specification by ID.
    pub fn get_task(&self, task_id: &str) -> Option<&SwarmTaskSpec> {
        self.tasks.iter().find(|t| t.id == task_id)
    }
}

/// Recorded outcome of an individual worker task after completion and verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTaskOutcome {
    pub task_id: String,
    pub role_title: String,
    pub status: SwarmTaskStatus,
    pub duration_ms: u64,
    pub tokens_used: usize,
    pub files_modified: Vec<String>,
    pub worktree_path: Option<PathBuf>,
    pub branch_name: Option<String>,
    pub verification_command: Option<String>,
    pub verification_passed: bool,
    pub summary: String,
    pub error: Option<String>,
    pub generated_artifacts: HashMap<String, String>, // path -> preview/content
}

/// Overall live tracking state for a swarm run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmExecutionState {
    pub swarm_id: String,
    pub title: String,
    pub objective: String,
    pub task_statuses: HashMap<String, SwarmTaskStatus>,
    pub outcomes: HashMap<String, SwarmTaskOutcome>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub total_duration_ms: Option<u64>,
    pub total_tokens: usize,
    pub is_running: bool,
    pub auto_merged: bool,
}

impl SwarmExecutionState {
    pub fn new(plan: &SwarmPlan) -> Self {
        let mut statuses = HashMap::new();
        for task in &plan.tasks {
            let initial_status = if task.dependencies.is_empty() {
                SwarmTaskStatus::Ready
            } else {
                SwarmTaskStatus::Pending
            };
            statuses.insert(task.id.clone(), initial_status);
        }

        Self {
            swarm_id: plan.id.clone(),
            title: plan.title.clone(),
            objective: plan.objective.clone(),
            task_statuses: statuses,
            outcomes: HashMap::new(),
            start_time: chrono::Utc::now().to_rfc3339(),
            end_time: None,
            total_duration_ms: None,
            total_tokens: 0,
            is_running: true,
            auto_merged: false,
        }
    }

    /// Checks if all dependencies for a pending task are Completed, and if so marks it Ready.
    pub fn evaluate_ready_tasks(&mut self, plan: &SwarmPlan) -> Vec<String> {
        let mut newly_ready = Vec::new();

        for task in &plan.tasks {
            if self.task_statuses.get(&task.id) == Some(&SwarmTaskStatus::Pending) {
                let all_deps_completed = task.dependencies.iter().all(|dep_id| {
                    self.task_statuses.get(dep_id) == Some(&SwarmTaskStatus::Completed)
                });

                let any_dep_failed = task.dependencies.iter().any(|dep_id| {
                    matches!(
                        self.task_statuses.get(dep_id),
                        Some(SwarmTaskStatus::Failed)
                            | Some(SwarmTaskStatus::Blocked)
                            | Some(SwarmTaskStatus::Cancelled)
                    )
                });

                if any_dep_failed {
                    self.task_statuses
                        .insert(task.id.clone(), SwarmTaskStatus::Blocked);
                } else if all_deps_completed {
                    self.task_statuses
                        .insert(task.id.clone(), SwarmTaskStatus::Ready);
                    newly_ready.push(task.id.clone());
                }
            }
        }

        newly_ready
    }

    /// Returns true if all tasks have reached a terminal state.
    pub fn is_complete(&self) -> bool {
        self.task_statuses.values().all(|s| s.is_terminal())
    }

    /// Counts tasks by status.
    pub fn count_by_status(&self, status: SwarmTaskStatus) -> usize {
        self.task_statuses
            .values()
            .filter(|&&s| s == status)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_task(id: &str, deps: Vec<&str>) -> SwarmTaskSpec {
        SwarmTaskSpec {
            id: id.to_string(),
            title: format!("Task {}", id),
            role_title: "Engineer".to_string(),
            instructions: "Do work".to_string(),
            prompt: format!("Execute {}", id),
            file_boundaries: vec![],
            dependencies: deps.into_iter().map(String::from).collect(),
            check_command: None,
            expected_artifacts: vec![],
            workspace_mode: None,
            max_iterations: None,
        }
    }

    #[test]
    fn test_swarm_plan_validation_and_waves() {
        let plan = SwarmPlan {
            id: "test-swarm-1".to_string(),
            title: "Test Full Stack Swarm".to_string(),
            objective: "Build an app".to_string(),
            tasks: vec![
                make_test_task("t1_backend", vec![]),
                make_test_task("t2_database", vec![]),
                make_test_task("t3_frontend", vec!["t1_backend", "t2_database"]),
                make_test_task("t4_e2e_tests", vec!["t3_frontend"]),
            ],
            created_at: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        assert!(plan.validate().is_ok());

        let waves = plan
            .calculate_waves()
            .expect("wave calculation should succeed");
        assert_eq!(waves.len(), 3);
        // Wave 0: t1 and t2 (independent)
        assert_eq!(waves[0].len(), 2);
        assert!(waves[0].contains(&"t1_backend".to_string()));
        assert!(waves[0].contains(&"t2_database".to_string()));

        // Wave 1: t3 (depends on t1 and t2)
        assert_eq!(waves[1], vec!["t3_frontend".to_string()]);

        // Wave 2: t4 (depends on t3)
        assert_eq!(waves[2], vec!["t4_e2e_tests".to_string()]);
    }

    #[test]
    fn test_swarm_plan_cycle_detection() {
        let plan = SwarmPlan {
            id: "cycle-swarm".to_string(),
            title: "Cycle Test".to_string(),
            objective: "Fail cycle".to_string(),
            tasks: vec![
                make_test_task("t1", vec!["t2"]),
                make_test_task("t2", vec!["t1"]),
            ],
            created_at: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        let err = plan.validate().expect_err("cycle must fail validation");
        match err {
            SwarmError::CycleDetected(node) => {
                assert!(node == "t1" || node == "t2");
            }
            other => panic!("expected CycleDetected, got {:?}", other),
        }
    }

    #[test]
    fn test_swarm_plan_missing_dependency() {
        let plan = SwarmPlan {
            id: "missing-dep".to_string(),
            title: "Missing Dep".to_string(),
            objective: "Fail missing dep".to_string(),
            tasks: vec![make_test_task("t1", vec!["nonexistent_task"])],
            created_at: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        let err = plan
            .validate()
            .expect_err("missing dep must fail validation");
        match err {
            SwarmError::MissingDependency { task_id, dep_id } => {
                assert_eq!(task_id, "t1");
                assert_eq!(dep_id, "nonexistent_task");
            }
            other => panic!("expected MissingDependency, got {:?}", other),
        }
    }

    #[test]
    fn test_swarm_execution_state_advancement() {
        let plan = SwarmPlan {
            id: "advancement-test".to_string(),
            title: "Advancement".to_string(),
            objective: "Advancement".to_string(),
            tasks: vec![
                make_test_task("t1", vec![]),
                make_test_task("t2", vec!["t1"]),
            ],
            created_at: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        let mut state = SwarmExecutionState::new(&plan);
        assert_eq!(state.task_statuses.get("t1"), Some(&SwarmTaskStatus::Ready));
        assert_eq!(
            state.task_statuses.get("t2"),
            Some(&SwarmTaskStatus::Pending)
        );

        // Complete t1
        state
            .task_statuses
            .insert("t1".to_string(), SwarmTaskStatus::Completed);
        let unblocked = state.evaluate_ready_tasks(&plan);

        assert_eq!(unblocked, vec!["t2".to_string()]);
        assert_eq!(state.task_statuses.get("t2"), Some(&SwarmTaskStatus::Ready));
    }
}
