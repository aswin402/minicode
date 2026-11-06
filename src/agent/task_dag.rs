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
    #[serde(default, alias = "role")]
    pub assigned_role: Option<String>,
    #[serde(default)]
    pub worktree_branch: Option<String>,
}

/// Optional execution metadata for a DAG task node (`file_boundaries`, `check_cmd`, `category`, `load_skills`, `worker_id`, `max_turns`, `token_budget`, `timeout_secs`, `todos`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskExecMeta {
    #[serde(default)]
    pub file_boundaries: Vec<String>,
    #[serde(default)]
    pub check_cmd: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub load_skills: Vec<String>,
    #[serde(default)]
    pub worker_id: Option<String>,
    #[serde(default, alias = "max_iterations")]
    pub max_turns: Option<usize>,
    #[serde(default)]
    pub token_budget: Option<usize>,
    #[serde(default, alias = "duration_seconds")]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub todos: Vec<String>,
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
    #[serde(default)]
    pub task_meta: HashMap<String, TaskExecMeta>,
}

impl TaskDag {
    /// Creates a new empty Task DAG.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            task_meta: HashMap::new(),
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

    /// Deserializes a `TaskItem` and its optional `TaskExecMeta` from a JSON object and adds it to the DAG.
    pub fn add_task_from_json(&mut self, item: &serde_json::Value) -> Result<()> {
        let task: TaskItem =
            serde_json::from_value(item.clone()).map_err(|e| ToolError::InvalidArguments {
                name: "task_dag".to_string(),
                reason: format!("Invalid task schema: {}", e),
            })?;
        let task_id = task.id.clone();
        self.add_task(task);
        if let Ok(meta) = serde_json::from_value::<TaskExecMeta>(item.clone()) {
            if !meta.file_boundaries.is_empty()
                || meta.check_cmd.is_some()
                || meta.category.is_some()
                || !meta.load_skills.is_empty()
                || meta.worker_id.is_some()
                || meta.max_turns.is_some()
                || meta.token_budget.is_some()
                || meta.timeout_secs.is_some()
                || !meta.todos.is_empty()
            {
                self.task_meta.insert(task_id, meta);
            }
        }
        Ok(())
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
                if task.dependencies.iter().any(|d| d == parent_id) {
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
        let (mut completed, mut in_progress, mut pending, mut failed) = (0, 0, 0, 0);
        for t in self.tasks.values() {
            match t.status {
                TaskStatus::Completed => completed += 1,
                TaskStatus::InProgress => in_progress += 1,
                TaskStatus::Pending => pending += 1,
                TaskStatus::Failed => failed += 1,
                TaskStatus::Blocked => {}
            }
        }

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

            out.push_str("Topological Task Order:\n");
            for id in waves.iter().flatten() {
                if let Some(t) = self.tasks.get(id) {
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

    /// Propagates `TaskStatus::Blocked` to any `Pending` task whose upstream dependency has
    /// `Failed` or `Blocked` status, iterating to a fixed point. Returns the number of newly blocked tasks.
    pub fn propagate_blocked_to_fixed_point(&mut self) -> usize {
        let mut newly_blocked_total = 0;
        loop {
            let mut to_block = Vec::new();
            for (id, task) in &self.tasks {
                if task.status == TaskStatus::Pending {
                    let has_failed_or_blocked_dep = task.dependencies.iter().any(|dep_id| {
                        self.tasks.get(dep_id).is_some_and(|dep| {
                            matches!(dep.status, TaskStatus::Failed | TaskStatus::Blocked)
                        })
                    });
                    if has_failed_or_blocked_dep {
                        to_block.push(id.clone());
                    }
                }
            }
            if to_block.is_empty() {
                break;
            }
            newly_blocked_total += to_block.len();
            for id in to_block {
                if let Some(t) = self.tasks.get_mut(&id) {
                    t.status = TaskStatus::Blocked;
                }
            }
        }
        newly_blocked_total
    }

    /// Dispatches all currently unblocked (`next_executable_tasks`) tasks onto `SubagentPool`,
    /// marking them `TaskStatus::InProgress` and recording their assigned `worker_id`.
    pub async fn spawn_ready_tasks_on_pool(
        &mut self,
        workspace_root: &std::path::Path,
        pool: &crate::agent::subagent::SubagentPool,
    ) -> Result<Vec<(String, String, crate::agent::subagent::SubagentRole)>> {
        let mut ready_ids: Vec<String> = self
            .next_executable_tasks()
            .into_iter()
            .map(|t| t.id.clone())
            .collect();
        ready_ids.sort();
        if ready_ids.is_empty() {
            return Ok(Vec::new());
        }

        let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
        let provider = pool.get_or_create_provider().await;
        let mut spawned = Vec::with_capacity(ready_ids.len());

        for task_id in ready_ids {
            let Some(task) = self.tasks.get(&task_id).cloned() else {
                continue;
            };
            let meta = self.task_meta.get(&task_id).cloned().unwrap_or_default();
            let role_str = task
                .assigned_role
                .as_deref()
                .or(meta.category.as_deref())
                .unwrap_or("coder");
            let role = crate::agent::subagent::SubagentRole::from_str_loose(role_str);

            let dep_context = if task.dependencies.is_empty() {
                String::new()
            } else {
                let refs: Vec<String> = task
                    .dependencies
                    .iter()
                    .map(|d| format!("$RESULT[{}]", d))
                    .collect();
                format!(
                    "\n\n### Completed Upstream Dependency Outputs\n{}\n",
                    refs.join("\n")
                )
            };

            let prompt = format!(
                "### Task `{}`: {}\n\n{}{}",
                task.id, task.title, task.description, dep_context
            );

            let mut sub_cfg =
                crate::agent::subagent::types::SubagentConfig::for_role_with_minipub_config(
                    role,
                    &app_cfg.agent.minipub,
                )
                .with_todos(meta.todos.clone())
                .with_file_boundaries(meta.file_boundaries.clone())
                .with_explicit_max_turns(meta.max_turns)
                .with_explicit_token_budget(meta.token_budget)
                .with_category(meta.category.clone())
                .with_load_skills(meta.load_skills.clone())
                .with_until_loop(meta.check_cmd.clone(), None)
                .with_result_alias(Some(task_id.clone()))
                .with_dynamic_scope(&prompt);

            // Also incorporate DAG complexity_score (1..=10) when explicit max_turns/token_budget are not set
            if meta.max_turns.is_none() && task.complexity_score > 3 {
                let complexity_turn_bonus = (task.complexity_score.saturating_sub(3) as usize) * 2;
                sub_cfg.max_turns = (sub_cfg.max_turns + complexity_turn_bonus).clamp(
                    crate::constants::MIN_MINIPUB_TURNS,
                    crate::constants::MAX_MINIPUB_TURNS,
                );
                if meta.token_budget.is_none() {
                    let tokens_per_turn =
                        role.default_token_budget() / role.default_max_turns().max(1);
                    sub_cfg.token_budget = sub_cfg
                        .token_budget
                        .max(sub_cfg.max_turns.saturating_mul(tokens_per_turn))
                        .clamp(
                            crate::constants::MIN_MINIPUB_TOKEN_BUDGET,
                            crate::constants::MAX_MINIPUB_TOKEN_BUDGET,
                        );
                }
            }
            if let Some(to) = meta.timeout_secs {
                sub_cfg.inactivity_timeout_secs = to.clamp(
                    crate::constants::MIN_MINIPUB_INACTIVITY_TIMEOUT_SECS,
                    crate::constants::MAX_MINIPUB_INACTIVITY_TIMEOUT_SECS,
                );
            }
            let isolate_wt = role.should_isolate_worktree(&prompt, &meta.file_boundaries, None);

            let worker_id = pool
                .spawn_background_worker(
                    role,
                    &prompt,
                    Some(sub_cfg),
                    std::sync::Arc::clone(&provider),
                    isolate_wt,
                )
                .await?;

            if let Some(t) = self.tasks.get_mut(&task_id) {
                t.status = TaskStatus::InProgress;
            }
            self.task_meta.entry(task_id.clone()).or_default().worker_id = Some(worker_id.clone());
            spawned.push((task_id, worker_id, role));
        }

        self.save(workspace_root)?;
        Ok(spawned)
    }

    /// Event-driven reactive DAG wave conductor.
    /// Dynamically spawns unblocked tasks (`in_degree == 0`) onto `SubagentPool`, listens on
    /// `pool.wait_for_change` (`tokio::sync::Notify`), updates `.minicode/task_dag.json` as each
    /// worker finishes, and immediately unlocks dependent tasks without waiting for slower sibling tasks.
    pub async fn execute_dag_on_pool(
        workspace_root: &std::path::Path,
        run_in_background: bool,
        max_concurrency: Option<usize>,
    ) -> Result<String> {
        let mut dag = Self::load(workspace_root)?;
        if dag.tasks.is_empty() {
            return Ok(
                "ℹ No active Task DAG found in workspace. Pass `tasks` to `execute_task_dag` or call `create_task_dag` first."
                    .to_string(),
            );
        }
        let topo_order = dag.topological_order()?;
        let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
        if let Some(mc) = max_concurrency {
            pool.set_max_parallel(mc);
        }

        dag.propagate_blocked_to_fixed_point();
        let initial_spawned = dag.spawn_ready_tasks_on_pool(workspace_root, &pool).await?;

        let wave_id = format!(
            "swarm-dag-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                % 100_000
        );
        let dev_reg = crate::dev::registry::get_global_dev_registry();
        let wave_handle = dev_reg
            .register_swarm_process(
                crate::dev::models::DevProcessId::from(wave_id.clone()),
                format!("Task DAG Wave ({} tasks)", topo_order.len()),
                format!("execute_task_dag: {}", topo_order.join(" ➔ ")),
                workspace_root.to_path_buf(),
                std::process::id(),
                0,
                None,
            )
            .await;
        wave_handle
            .append_log(format!(
                "▶ Task DAG Wave `{}` started ({} total tasks, {} initial unblocked workers).",
                wave_id,
                topo_order.len(),
                initial_spawned.len()
            ))
            .await;
        for (tid, wid, role) in &initial_spawned {
            wave_handle
                .append_log(format!(
                    "  • Dispatched `{}` ➔ Worker `{}` ({})",
                    tid,
                    wid,
                    role.badge()
                ))
                .await;
        }

        if run_in_background {
            let ws_owned = workspace_root.to_path_buf();
            tokio::spawn(async move {
                let _ = Self::drive_reactive_dag_loop(&ws_owned, Some(wave_handle)).await;
            });

            let spawned_lines: Vec<String> = initial_spawned
                .iter()
                .map(|(tid, wid, role)| {
                    format!("• Task `{}` ➔ Worker `{}` ({})", tid, wid, role.badge())
                })
                .collect();
            return Ok(format!(
                "✔ Reactive Async Task DAG Conductor (`{}`) started in background (`{} total task(s)`, Topological Order: `{}`).\n\
                 ### Initial Unblocked Wave Dispatched ({} worker(s)):\n{}\n\n\
                 • Downstream dependent tasks will automatically spawn on `SubagentPool` the instant their upstream dependencies complete.\n\
                 • Completed worker reports will arrive via Boomerang notifications, or await with `minitask(action=\"await\")` / `manage_subagents(action=\"await\")`.",
                wave_id,
                topo_order.len(),
                topo_order.join(" ➔ "),
                initial_spawned.len(),
                if spawned_lines.is_empty() {
                    "• (No pending unblocked tasks)".to_string()
                } else {
                    spawned_lines.join("\n")
                }
            ));
        }

        let summaries = Self::drive_reactive_dag_loop(workspace_root, Some(wave_handle)).await?;
        let final_dag = Self::load(workspace_root)?;
        let mut out = format!(
            "✔ Reactive Task DAG Execution Finished (`{}` tasks):\n\n{}\n",
            topo_order.len(),
            final_dag.generate_report()
        );
        if !summaries.is_empty() {
            out.push_str("### Task Worker Outcomes:\n");
            for tid in &topo_order {
                if let Some((wid, summary)) = summaries.get(tid) {
                    let excerpt = crate::utils::strings::truncate_chars(summary.trim(), 500);
                    out.push_str(&format!(
                        "• **`{}`** (Worker `{}`): {}\n",
                        tid, wid, excerpt
                    ));
                }
            }
        }
        Ok(out)
    }

    async fn drive_reactive_dag_loop(
        workspace_root: &std::path::Path,
        wave_handle: Option<std::sync::Arc<crate::dev::process::DevProcessHandle>>,
    ) -> Result<HashMap<String, (String, String)>> {
        let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
        let mut summaries: HashMap<String, (String, String)> = HashMap::new();

        loop {
            let mut dag = Self::load(workspace_root)?;
            let mut changed = false;

            // 1. Check in-progress tasks against SubagentPool worker states
            let in_progress_ids: Vec<(String, String)> = dag
                .tasks
                .values()
                .filter(|t| t.status == TaskStatus::InProgress)
                .filter_map(|t| {
                    dag.task_meta
                        .get(&t.id)
                        .and_then(|m| m.worker_id.clone())
                        .map(|wid| (t.id.clone(), wid))
                })
                .collect();

            for (tid, wid) in &in_progress_ids {
                if let Some(info) = pool.get_subagent(wid).await {
                    if !info.state.is_active() {
                        let new_status = if matches!(
                            info.state,
                            crate::agent::subagent::SubagentState::Completed
                        ) {
                            TaskStatus::Completed
                        } else {
                            TaskStatus::Failed
                        };
                        if let Some(t) = dag.tasks.get_mut(tid) {
                            t.status = new_status;
                            changed = true;
                        }
                        if let Some(ref handle) = wave_handle {
                            handle
                                .append_log(format!(
                                    "  {} Task `{}` (Worker `{}`) ➔ {:?}",
                                    if new_status == TaskStatus::Completed {
                                        "✔"
                                    } else {
                                        "✗"
                                    },
                                    tid,
                                    wid,
                                    new_status
                                ))
                                .await;
                        }
                        summaries.insert(
                            tid.clone(),
                            (wid.clone(), info.final_summary.unwrap_or_default()),
                        );
                    }
                }
            }

            // 2. Propagate blocked status if any upstream task failed
            if dag.propagate_blocked_to_fixed_point() > 0 {
                changed = true;
            }
            if changed {
                dag.save(workspace_root)?;
            }

            // 3. Immediately spawn any newly unblocked downstream tasks (streaming topological readiness)
            let newly_spawned = dag.spawn_ready_tasks_on_pool(workspace_root, &pool).await?;
            if let Some(ref handle) = wave_handle {
                for (tid, wid, role) in &newly_spawned {
                    handle
                        .append_log(format!(
                            "  • Unblocked & dispatched `{}` ➔ Worker `{}` ({})",
                            tid,
                            wid,
                            role.badge()
                        ))
                        .await;
                }
            }

            // 4. Check termination: no tasks InProgress and no tasks ready to execute
            let still_in_progress = dag
                .tasks
                .values()
                .any(|t| t.status == TaskStatus::InProgress);
            if !still_in_progress
                && newly_spawned.is_empty()
                && dag.next_executable_tasks().is_empty()
            {
                if let Some(ref handle) = wave_handle {
                    let any_failed = dag
                        .tasks
                        .values()
                        .any(|t| matches!(t.status, TaskStatus::Failed | TaskStatus::Blocked));
                    let exit_code = if any_failed { 1 } else { 0 };
                    handle
                        .append_log(format!(
                            "🏁 Task DAG Wave completed ({} task outcomes recorded).",
                            summaries.len()
                        ))
                        .await;
                    handle
                        .update_status(crate::dev::models::DevProcessStatus::Exited(Some(
                            exit_code,
                        )))
                        .await;
                }
                break;
            }

            // 5. Event-driven wait on SubagentPool's completion Notify
            pool.wait_for_change(std::time::Duration::from_millis(200))
                .await;
        }

        Ok(summaries)
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

    #[test]
    fn test_task_dag_json_exec_meta_and_blocked_propagation() {
        let mut dag = TaskDag::new();
        dag.add_task_from_json(&serde_json::json!({
            "id": "t1",
            "title": "Audit API",
            "description": "Inspect API routes",
            "role": "scout",
            "file_boundaries": ["src/api"],
            "check_cmd": "cargo check -j 1"
        }))
        .unwrap();
        dag.add_task_from_json(&serde_json::json!({
            "id": "t2",
            "title": "Implement Handler",
            "description": "Build handler",
            "dependencies": ["t1"],
            "role": "coder"
        }))
        .unwrap();
        dag.add_task_from_json(&serde_json::json!({
            "id": "t3",
            "title": "Verify Handler",
            "description": "Test handler",
            "dependencies": ["t2"],
            "role": "tester"
        }))
        .unwrap();

        assert_eq!(dag.tasks["t1"].assigned_role.as_deref(), Some("scout"));
        assert_eq!(
            dag.task_meta["t1"].file_boundaries,
            vec!["src/api".to_string()]
        );
        assert_eq!(
            dag.task_meta["t1"].check_cmd.as_deref(),
            Some("cargo check -j 1")
        );

        // Mark t1 Failed and verify fixed-point propagation blocks both t2 and t3
        dag.set_task_status("t1", TaskStatus::Failed).unwrap();
        let blocked_count = dag.propagate_blocked_to_fixed_point();
        assert_eq!(blocked_count, 2);
        assert_eq!(dag.tasks["t2"].status, TaskStatus::Blocked);
        assert_eq!(dag.tasks["t3"].status, TaskStatus::Blocked);
    }
}
