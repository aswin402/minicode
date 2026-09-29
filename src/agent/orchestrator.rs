use crate::agent::subagent::{
    get_global_scratchpad, get_global_subagent_pool, SubAgentResult, SubagentConfig, SubagentRole,
    SubagentTaskSpec,
};
use crate::error::{MinicodeError, Result, ToolError};
use crate::git::worktree::WorktreeManager;
use std::path::Path;

/// Result of an individual subagent fanout worker.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct FanoutWorkerOutcome {
    pub id: String,
    pub role: String,
    pub isolate_worktree: bool,
    pub success: bool,
    pub result: SubAgentResult,
    pub merged: bool,
    pub error: Option<String>,
}

/// Orchestrator for delegating tasks to concurrent or isolated subagents.
pub struct MultiAgentOrchestrator;

impl MultiAgentOrchestrator {
    /// Spawns an autonomous subagent for the given task and returns its structured result.
    pub async fn delegate(
        workspace_root: &Path,
        task_prompt: &str,
        use_worktree: bool,
        timeout_secs: Option<u64>,
    ) -> Result<SubAgentResult> {
        Self::delegate_with_config(
            workspace_root,
            task_prompt,
            use_worktree,
            timeout_secs,
            None,
        )
        .await
    }

    /// Spawns an autonomous subagent with custom configuration.
    pub async fn delegate_with_config(
        workspace_root: &Path,
        task_prompt: &str,
        use_worktree: bool,
        timeout_secs: Option<u64>,
        config: Option<crate::agent::subagent::SubagentConfig>,
    ) -> Result<SubAgentResult> {
        let pool = get_global_subagent_pool(workspace_root);
        let provider = pool.get_or_create_provider().await;
        let role = config
            .as_ref()
            .map(|c| c.role)
            .unwrap_or(SubagentRole::Scout);
        let timeout = timeout_secs.unwrap_or(crate::constants::DEFAULT_SUBAGENT_TIMEOUT_SECS);

        let fut = pool.run_subagent_with_options(role, task_prompt, config, provider, use_worktree);

        match tokio::time::timeout(std::time::Duration::from_secs(timeout), fut).await {
            Ok(res) => res,
            Err(_) => Err(MinicodeError::Channel(format!(
                "Subagent task timed out after {} seconds",
                timeout
            ))),
        }
    }

    /// Concurrently executes a batch of subagent tasks with optional Git Worktree isolation.
    #[allow(dead_code)]
    pub async fn fanout_tasks(
        workspace_root: &Path,
        tasks: Vec<SubagentTaskSpec>,
        wait_for_completion: bool,
        auto_merge: bool,
    ) -> Result<String> {
        if tasks.is_empty() {
            return Ok("No subagent tasks specified for fan-out.".to_string());
        }

        let pool = get_global_subagent_pool(workspace_root);
        let provider = pool.get_or_create_provider().await;
        let mut worker_specs = Vec::new();
        let mut launched_descriptions = Vec::new();

        for task in tasks {
            let role = task.role;
            let prompt = task.prompt.clone();
            let timeout = task.timeout_secs.unwrap_or(120);

            let isolate_worktree = task.isolate_worktree.unwrap_or(
                role.default_workspace_mode() == crate::agent::subagent::WorkspaceMode::Worktree,
            );

            let mut config = SubagentConfig::for_role(role);
            if let Some(m) = task.model {
                config.model = Some(m);
            }

            let task_id = pool
                .spawn_background_worker(
                    role,
                    &prompt,
                    Some(config),
                    std::sync::Arc::clone(&provider),
                    isolate_worktree,
                )
                .await?;

            launched_descriptions.push((
                task_id.clone(),
                role.badge().to_string(),
                isolate_worktree,
                prompt.clone(),
            ));

            worker_specs.push((task_id, role, isolate_worktree, timeout));
        }

        if !wait_for_completion {
            let mut out = format!(
                "### Subagent Swarm Fan-Out Launched ({} worker(s) in background)\n\n",
                launched_descriptions.len()
            );
            for (id, badge, isolate, prompt) in launched_descriptions {
                let mode = if isolate {
                    format!("Git Worktree: subagent/{}", id)
                } else {
                    "Shared Read-Only".to_string()
                };
                let short_prompt = crate::utils::truncate_ellipsis(&prompt, 60);
                out.push_str(&format!(
                    "- `{}` (**{}**) [{}]: \"{}\"\n",
                    id, badge, mode, short_prompt
                ));
            }
            out.push_str("\nWorkers are running concurrently in background. Press `Ctrl+S` or run `/swarm` to monitor live telemetry in the Activity Drawer.\n");
            return Ok(out);
        }

        // Wait for all workers to complete
        let scratchpad = get_global_scratchpad();
        let mut completed_results = Vec::new();
        let worktree_mgr = WorktreeManager::new(workspace_root);

        for (id, role, isolate_worktree, timeout_secs) in worker_specs {
            let start = std::time::Instant::now();
            let max_dur = std::time::Duration::from_secs(timeout_secs);

            let mut final_info = None;
            while start.elapsed() < max_dur {
                if let Some(info) = pool.get_subagent(&id).await {
                    if !matches!(
                        info.state,
                        crate::agent::subagent::types::SubagentState::Running
                            | crate::agent::subagent::types::SubagentState::Starting
                            | crate::agent::subagent::types::SubagentState::WaitingForInput
                    ) {
                        final_info = Some(info);
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }

            let info = match final_info {
                Some(i) => Some(i),
                None => pool.get_subagent(&id).await,
            };

            let summary = scratchpad
                .read_entry(&format!("subagent_{}", id))
                .or_else(|| scratchpad.read_entry(&format!("subagent/{}", id)))
                .map(|e| e.content)
                .or_else(|| info.as_ref().and_then(|i| i.final_summary.clone()))
                .unwrap_or_else(|| "Subagent completed without producing a summary.".to_string());

            let success = info
                .as_ref()
                .map(|i| {
                    matches!(
                        i.state,
                        crate::agent::subagent::types::SubagentState::Completed
                    )
                })
                .unwrap_or(false);

            let tokens_used = info.as_ref().map(|i| i.tokens_used).unwrap_or(0);
            let turns_executed = info.as_ref().map(|i| i.turns_executed).unwrap_or(0);

            let mut merged = false;
            if auto_merge
                && isolate_worktree
                && success
                && worktree_mgr.merge_worktree(&id).await.is_ok()
            {
                let _ = worktree_mgr.remove_worktree(&id).await;
                merged = true;
            }

            let sub_res = SubAgentResult {
                id: id.clone(),
                task_id: id.clone(),
                role,
                success,

                final_summary: summary.clone(),
                tokens_used,
                turns_executed,
                files_inspected: Vec::new(),
                files_modified: Vec::new(),
                worktree_branch: if isolate_worktree {
                    Some(format!("subagent/{}", id))
                } else {
                    None
                },
            };

            completed_results.push(FanoutWorkerOutcome {
                id: id.clone(),
                role: role.badge().to_string(),
                isolate_worktree,
                success,
                result: sub_res,
                merged,
                error: if success { None } else { Some(summary) },
            });
        }

        Ok(Self::format_fanout_summary(&completed_results))
    }

    /// Formats a clean markdown report summarizing fan-out results
    #[allow(dead_code)]
    pub fn format_fanout_summary(results: &[FanoutWorkerOutcome]) -> String {
        let mut out = format!(
            "### Subagent Swarm Fan-Out Completed ({} worker(s) finished)\n\n",
            results.len()
        );
        out.push_str("| Worker ID | Role | Environment | Status | Tokens | Files Modified |\n");
        out.push_str("| :--- | :--- | :--- | :---: | :---: | :--- |\n");

        for item in results {
            let env = if item.isolate_worktree {
                if item.merged {
                    format!("`subagent/{}` *(merged)*", item.id)
                } else {
                    format!("`subagent/{}`", item.id)
                }
            } else {
                "Shared (Read-Only)".to_string()
            };

            let status = if item.success && item.result.success {
                "✔ Success"
            } else {
                "✗ Failed"
            };
            let modified = if item.result.files_modified.is_empty() {
                "None".to_string()
            } else {
                item.result.files_modified.join(", ")
            };

            out.push_str(&format!(
                "| `{}` | **{}** | {} | {} | {} | {} |\n",
                item.id, item.role, env, status, item.result.tokens_used, modified
            ));
        }

        out.push_str("\n#### Executive Summaries & Findings:\n\n");
        for item in results {
            out.push_str(&format!("##### `{}` — {}\n", item.id, item.role));
            if let Some(ref e) = item.error {
                out.push_str(&format!("**Error:** {}\n\n", e));
            } else {
                let summary_text = if item.result.final_summary.trim().is_empty() {
                    "(No summary output emitted)"
                } else {
                    item.result.final_summary.trim()
                };
                out.push_str(&format!("{}\n\n", summary_text));
            }
        }

        out
    }

    /// Merges an isolated subagent worktree branch into the current branch and cleans up.
    #[allow(dead_code)]
    pub async fn merge_worktree(workspace_root: &Path, subagent_id: &str) -> Result<String> {
        let mgr = WorktreeManager::new(workspace_root);
        let merge_output = mgr.merge_worktree(subagent_id).await.map_err(|e| {
            MinicodeError::Tool(ToolError::CommandExec(format!(
                "Failed to merge subagent worktree branch `subagent/{}`: {}",
                subagent_id, e
            )))
        })?;

        let _ = mgr.remove_worktree(subagent_id).await;

        Ok(format!(
            "✔ Successfully integrated subagent worktree `[ID: {}]`!\n\
             • Branch `subagent/{}` merged into current active branch.\n\
             • Temporary worktree folder `.minicode/worktrees/{}` removed.\n\n\
             Git Merge Summary:\n{}",
            subagent_id,
            subagent_id,
            subagent_id,
            merge_output.trim()
        ))
    }

    /// Formats the SubAgentResult into a clean markdown summary for parent LLM tool results.
    pub fn format_result(result: &SubAgentResult) -> String {
        let status_emoji = if result.success { "✔" } else { "✗" };
        let mut out = format!(
            "{} SubAgent Task [{}] Finished (Tokens: {})\n",
            status_emoji, result.task_id, result.tokens_used
        );

        if let Some(ref branch) = result.worktree_branch {
            out.push_str(&format!("Branch: {}\n", branch));
        }

        if !result.files_modified.is_empty() {
            out.push_str(&format!(
                "Files Modified ({}):\n  • {}\n",
                result.files_modified.len(),
                result.files_modified.join("\n  • ")
            ));
        }

        out.push_str(&format!(
            "\nOutcome / Findings:\n{}",
            result.final_summary.trim()
        ));
        out
    }
}

/// Execution archetype detected from natural language user prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowArchetype {
    /// UI/frontend development, styling, components, or layout creation.
    UiDesign,
    /// AST slicing, code tracing, architecture exploration, or impact analysis.
    CodeExploration,
    /// Dev servers, background daemons, port monitoring, or runtime logs.
    RuntimeDev,
    /// Complex multi-step feature engineering, refactoring, or TDD workflows.
    MultiPhaseEngineering,
    /// MiniVault skills management (search, show, load, create, update, import_url).
    VaultSkills,
    /// Standard or single-step task (bugfix, direct file edit, general question).
    Standard,
}

impl WorkflowArchetype {
    /// Returns the recommended tool categories for this execution archetype.
    pub fn recommended_categories(&self) -> Vec<crate::tools::category::ToolCategory> {
        use crate::tools::category::ToolCategory;
        match self {
            Self::UiDesign => vec![ToolCategory::Blocks, ToolCategory::MiniKit],
            Self::CodeExploration => vec![
                ToolCategory::Codegraph,
                ToolCategory::Search,
                ToolCategory::Memory,
            ],
            Self::RuntimeDev => vec![ToolCategory::Dev, ToolCategory::Exec],
            Self::MultiPhaseEngineering => vec![
                ToolCategory::MiniPower,
                ToolCategory::Memory,
                ToolCategory::Files,
                ToolCategory::Exec,
            ],
            Self::VaultSkills => vec![ToolCategory::Vault, ToolCategory::MiniKit],
            Self::Standard => vec![],
        }
    }
}

/// Autonomous workflow router that analyzes natural language prompts,
/// classifies execution archetypes, activates tool categories, and synthesizes
/// specialized pre-turn context without requiring manual slash commands.
pub struct WorkflowRouter;

impl WorkflowRouter {
    /// Classifies user prompt into an execution archetype.
    pub fn classify(prompt: &str) -> WorkflowArchetype {
        let lower = prompt.to_ascii_lowercase();

        // 0. MiniVault Skills Management Archetype
        if lower.contains("add skill")
            || lower.contains("load skill")
            || lower.contains("install skill")
            || lower.contains("import skill")
            || lower.contains("create skill")
            || lower.contains("update skill")
            || lower.contains("delete skill")
            || lower.contains("remove skill")
            || lower.contains("search skill")
            || lower.contains("list skill")
            || lower.contains("show skill")
            || lower.contains("mini_vault")
            || lower.contains("minivault")
            || lower.contains("skill bundle")
            || lower.contains("skill bundles")
            || lower.contains("load bundle")
            || lower.contains("install bundle")
            || lower.contains("create bundle")
            || lower.contains("list bundles")
            || lower.contains("list bundle")
            || lower.contains("bundle list")
            || lower.contains("fullstack bundle")
            || lower.contains("bundled skills")
            || lower.contains("bundled skill")
            || lower.contains("ingest source")
            || lower.contains("bookmark")
            || lower.contains("learn from website")
            || lower.contains("learn from repo")
            || lower.contains("learn website")
            || lower.contains("learn repo")
            || lower.contains("gotchas")
            || lower.contains("compiler traps")
            || lower.contains("self improve")
            || lower.contains("vault_ingest")
            || lower.contains("vault_gotchas")
            || ((lower.contains("http://") || lower.contains("https://"))
                && (lower.contains("learn")
                    || lower.contains("ingest")
                    || lower.contains("doc")
                    || lower.contains("bookmark")
                    || lower.contains("save link")
                    || lower.contains("reference")))
            || (crate::utils::has_word(&lower, "skill")
                && (lower.contains("internet")
                    || lower.contains("github")
                    || lower.contains("http")
                    || lower.contains("project")
                    || lower.contains("global")))
            || (crate::utils::has_word(&lower, "bundle")
                && (lower.contains("vault")
                    || lower.contains("stack")
                    || lower.contains("load")
                    || lower.contains("install")
                    || lower.contains("skills")))
        {
            return WorkflowArchetype::VaultSkills;
        }

        // 1. CodeGraph, AST Slicing & Architecture Exploration Archetype
        // Prioritized when prompt specifically asks to locate, trace, or inspect symbols/callers
        if lower.contains("blast radius")
            || lower.contains("code graph")
            || lower.contains("codegraph")
            || lower.contains("code_explore")
            || lower.contains("diff_impact")
            || lower.contains("code_explain")
            || lower.contains("call trace")
            || lower.contains("trace flow")
            || lower.contains("who calls")
            || lower.contains("call hierarchy")
            || lower.contains("dependency graph")
            || lower.contains("repo map")
            || lower.contains("repomap")
            || lower.contains("impact of")
            || crate::utils::has_word(&lower, "callers")
            || crate::utils::has_word(&lower, "callees")
            || (crate::utils::has_word(&lower, "where")
                && (crate::utils::has_word(&lower, "defined")
                    || crate::utils::has_word(&lower, "located")))
            || (crate::utils::has_word(&lower, "trace")
                && (crate::utils::has_word(&lower, "call")
                    || crate::utils::has_word(&lower, "function")
                    || crate::utils::has_word(&lower, "symbol")
                    || crate::utils::has_word(&lower, "flow")))
        {
            return WorkflowArchetype::CodeExploration;
        }

        // 2. Runtime Dev & Process Orchestration Archetype
        if lower.contains("dev server")
            || lower.contains("run server")
            || lower.contains("start server")
            || lower.contains("launch server")
            || lower.contains("restart server")
            || lower.contains("kill server")
            || lower.contains("stop server")
            || lower.contains("background process")
            || lower.contains("background task")
            || lower.contains("manage task")
            || lower.contains("manage tasks")
            || lower.contains("minitask")
            || lower.contains("listen on port")
            || lower.contains("port ")
            || lower.starts_with("serve")
            || lower.contains(" serve ")
            || lower.contains("how much resource")
            || lower.contains("resource usage")
            || lower.contains("resources used")
            || lower.contains("resources taking")
            || lower.contains("ram usage")
            || lower.contains("cpu usage")
            || lower.contains("stop that")
            || lower.contains("kill that")
            || lower == "stop it"
            || lower == "kill it"
            || lower == "stop"
            || lower == "kill"
            || lower.contains("close browser")
            || lower.contains("stop browser")
            || lower.contains("kill browser")
            || lower.contains("close website")
            || lower.contains("stop website")
            || lower.contains("kill website")
            || lower.contains("whats happening")
            || lower.contains("what's happening")
            || lower.contains("what is happening")
            || lower.contains("what is running")
            || lower.contains("what's running")
            || lower.contains("whats running")
            || lower.contains("what are the tasks doing")
            || lower.contains("how are the tasks doing")
            || lower.contains("how are background processes doing")
            || lower.contains("how are the background processes doing")
            || lower.contains("status of tasks")
            || lower.contains("status of task")
            || lower.contains("task status")
            || lower.contains("process status")
            || lower.contains("is the server running")
            || lower.contains("is the server still running")
            || lower.contains("is server running")
            || lower.contains("any background tasks")
            || lower.contains("any background task")
            || lower.contains("any processes running")
            || lower.contains("check background tasks")
            || lower.contains("check background task")
            || lower.contains("check background processes")
            || lower.contains("check tasks")
            || lower.contains("check processes")
            || lower.contains("active processes")
            || lower.contains("active tasks")
            || lower.contains("scheduled task")
            || lower.contains("scheduled tasks")
            || lower.contains("periodic check")
            || lower.contains("one-shot timer")
            || crate::utils::has_word(&lower, "daemon")
            || (crate::utils::has_word(&lower, "server")
                && (crate::utils::has_word(&lower, "start")
                    || crate::utils::has_word(&lower, "run")
                    || crate::utils::has_word(&lower, "status")
                    || crate::utils::has_word(&lower, "logs")
                    || crate::utils::has_word(&lower, "kill")
                    || crate::utils::has_word(&lower, "stop")))
        {
            return WorkflowArchetype::RuntimeDev;
        }

        // 3. Multi-Phase Engineering, Planning & TDD Archetype
        if lower.contains("power plan")
            || lower.contains("superpower")
            || lower.contains("minipower")
            || lower.contains("acceptance criteria")
            || lower.contains("end-to-end")
            || lower.contains("verification barrier")
            || (crate::utils::has_word(&lower, "implement")
                && (crate::utils::has_word(&lower, "feature")
                    || crate::utils::has_word(&lower, "module")
                    || crate::utils::has_word(&lower, "system")
                    || crate::utils::has_word(&lower, "service")
                    || crate::utils::has_word(&lower, "api")
                    || crate::utils::has_word(&lower, "with")))
            || (crate::utils::has_word(&lower, "refactor")
                && (crate::utils::has_word(&lower, "codebase")
                    || crate::utils::has_word(&lower, "layer")
                    || crate::utils::has_word(&lower, "architecture")
                    || crate::utils::has_word(&lower, "system")
                    || crate::utils::has_word(&lower, "module")))
            || (crate::utils::has_word(&lower, "build")
                && (crate::utils::has_word(&lower, "app")
                    || crate::utils::has_word(&lower, "fullstack")
                    || crate::utils::has_word(&lower, "feature")
                    || crate::utils::has_word(&lower, "system")))
            || (crate::utils::has_word(&lower, "plan")
                && crate::utils::has_word(&lower, "milestones"))
            || crate::utils::has_word(&lower, "tdd")
            || crate::utils::has_word(&lower, "milestone")
        {
            return WorkflowArchetype::MultiPhaseEngineering;
        }

        // 4. UI Design & Component Warehouse Archetype
        let is_explicit_ui = lower.contains("landing page")
            || lower.contains("landing")
            || lower.contains("sidebar")
            || lower.contains("navbar")
            || lower.contains("hero section")
            || lower.contains("ui design")
            || lower.contains("design token")
            || lower.contains("tailwind")
            || lower.contains("palette")
            || lower.contains("gradient")
            || lower.contains("dark mode")
            || lower.contains("light mode")
            || lower.contains("wireframe")
            || crate::utils::has_word(&lower, "ui")
            || crate::utils::has_word(&lower, "frontend")
            || crate::utils::has_word(&lower, "css")
            || crate::utils::has_word(&lower, "styling");

        let has_ui_component_term = crate::utils::has_word(&lower, "component")
            || crate::utils::has_word(&lower, "components")
            || crate::utils::has_word(&lower, "button")
            || crate::utils::has_word(&lower, "modal")
            || crate::utils::has_word(&lower, "card")
            || crate::utils::has_word(&lower, "dialog");

        // Disambiguate data structure / database terms from UI components
        let has_data_backend_term = lower.contains("database")
            || lower.contains("hash table")
            || lower.contains("sql table")
            || lower.contains("pricing calculation")
            || lower.contains("pricing algorithm");

        if (is_explicit_ui || has_ui_component_term) && !has_data_backend_term {
            return WorkflowArchetype::UiDesign;
        }

        WorkflowArchetype::Standard
    }

    /// Synthesizes contextual enrichment block according to the detected archetype.
    pub async fn enrich_context(
        workspace_root: &Path,
        prompt: &str,
        archetype: WorkflowArchetype,
    ) -> Option<String> {
        let base_enrichment = match archetype {
            WorkflowArchetype::UiDesign => Self::enrich_ui_design(workspace_root, prompt),
            WorkflowArchetype::CodeExploration => Self::enrich_code_exploration(prompt),
            WorkflowArchetype::RuntimeDev => Self::enrich_runtime_dev().await,
            WorkflowArchetype::MultiPhaseEngineering => {
                Self::enrich_multiphase_engineering(workspace_root, prompt)
            }
            WorkflowArchetype::VaultSkills => Self::enrich_vault_skills(workspace_root, prompt),
            WorkflowArchetype::Standard => None,
        };

        // For engineering workflows (MultiPhaseEngineering or Standard), check if there are
        // highly relevant universal gotchas learned from past sessions for the active prompt/stack
        if matches!(
            archetype,
            WorkflowArchetype::MultiPhaseEngineering | WorkflowArchetype::Standard
        ) {
            let store = crate::vault::store::VaultStore::new(workspace_root);
            let lower = prompt.to_ascii_lowercase();
            let relevant_gotchas = store.find_relevant_gotchas(&lower);
            if !relevant_gotchas.is_empty() {
                let mut gotcha_block = String::from("\n<universal_learned_gotchas>\n");
                gotcha_block.push_str("  Known Pitfalls & Invariants (Learned from previous sessions across this machine):\n");
                for g in relevant_gotchas.iter().take(3) {
                    gotcha_block.push_str(&format!(
                        "  {}\n",
                        g.format_prompt_block().replace('\n', "\n  ")
                    ));
                }
                gotcha_block.push_str("  Directive: Heed these hard-won lessons to prevent repeating previous compiler and runtime errors.\n");
                gotcha_block.push_str("</universal_learned_gotchas>");

                return match base_enrichment {
                    Some(mut b) => {
                        b.push_str(&gotcha_block);
                        Some(b)
                    }
                    None => Some(gotcha_block),
                };
            }
        }

        base_enrichment
    }

    fn is_empty_workspace(workspace: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(workspace) else {
            return false;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }
            return false;
        }
        true
    }

    fn enrich_ui_design(workspace_root: &Path, prompt: &str) -> Option<String> {
        let store_lock = crate::blocks::get_global_block_store();
        let store = match store_lock.read() {
            Ok(s) => s,
            Err(e) => e.into_inner(),
        };

        let lower = prompt.to_ascii_lowercase();
        let candidates = [
            "navbar",
            "hero",
            "card",
            "pricing",
            "testimonial",
            "feature",
            "faq",
            "footer",
            "sidebar",
            "form",
            "modal",
            "table",
            "button",
            "input",
            "tabs",
            "accordion",
            "avatar",
            "badge",
            "dropdown",
            "slider",
            "cta",
            "auth",
            "dashboard",
        ];

        let matched_queries: Vec<&str> = candidates
            .iter()
            .copied()
            .filter(|&c| crate::utils::has_word(&lower, c))
            .collect();

        let primary_query = matched_queries.first().copied();

        let filter = crate::blocks::BlockSearchFilter {
            query: primary_query.map(|q| q.to_string()),
            limit: 4,
            ..Default::default()
        };

        let components = store.search_components(&filter);
        let palettes = store.list_palettes();

        let mut out = String::from("<recommended_miniblocks>\n");
        out.push_str("  Autonomous UI Warehouse Guidance:\n");
        out.push_str("  1. Do not code UI elements, styles, or color palettes from scratch when warehouse blocks match.\n");

        if !components.is_empty() {
            out.push_str("  2. Matching verified components in MiniBlocks warehouse:\n");
            for c in components.iter().take(3) {
                out.push_str(&format!(
                    "     • `{}` ({}, {:?}): {}\n",
                    c.name, c.category, c.framework, c.description
                ));
            }
            if matched_queries.len() > 1 {
                out.push_str(&format!(
                    "     Tip: User prompt also requested other UI blocks ({}). Query them with `block_search`.\n",
                    matched_queries[1..].join(", ")
                ));
            }
            out.push_str("     Action: Use `block_insert` or `block_scaffold` to insert them directly into project source.\n");
        } else {
            out.push_str("  2. Search warehouse with `block_search` to find pre-built components for your stack.\n");
        }

        if !palettes.is_empty() {
            out.push_str("  3. Available color palette tokens:\n");
            for p in palettes.iter().take(2) {
                out.push_str(&format!(
                    "     • `{}` (Colors: [Background: {}, Surface: {}, Accent: {}, Text: {}])\n",
                    p.name, p.colors[0], p.colors[1], p.colors[2], p.colors[3]
                ));
            }
            out.push_str("     Action: Pass palette name to `block_scaffold(palette=\"...\")` for automated theme tokens.\n");
        }

        // Modular anti-monolith & dev server rules
        out.push_str("  • Modular Code Architecture Invariant:\n");
        out.push_str("    NEVER write massive monolithic files (>300 lines or >12KB) in a single tool call to avoid token truncation and JSON EOF errors.\n");
        out.push_str("    Decompose frontend applications into separate modular files (`index.html`, `styles.css`, `app.js` or components) from turn 1.\n");

        let core_dir = crate::tools::minikit::resolve_core_docs_dir(workspace_root);
        if !core_dir.exists() && Self::is_empty_workspace(workspace_root) {
            out.push_str("  • Bootstrap & Scaffolding Invariant:\n");
            out.push_str("    This workspace is empty. Run `kit_sync` or ensure `minikit_docs/core/` (prd.md, design.md, todo.md) is initialized to anchor architecture and tasks before writing code.\n");
        }

        out.push_str("  • Development Server Invariant:\n");
        out.push_str("    To run and test the web application (e.g. `python3 -m http.server 8080 &`), use `exec_cmd` or `minitask(action=\"start\")`.\n");

        out.push_str("</recommended_miniblocks>");
        Some(out)
    }

    fn enrich_code_exploration(prompt: &str) -> Option<String> {
        let mut symbols = Vec::new();

        // 1. Backtick extraction `foo`
        let mut parts = prompt.split('`');
        let _ = parts.next();
        while let Some(candidate) = parts.next() {
            let trimmed = candidate.trim();
            if !trimmed.is_empty() && trimmed.len() <= 64 && !trimmed.contains(' ') {
                symbols.push(trimmed.to_string());
            }
            let _ = parts.next();
        }

        // 2. Snake_case or CamelCase words if no backticks found
        if symbols.is_empty() {
            for word in prompt.split_whitespace() {
                let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                let is_snake = clean.contains('_') && clean.len() >= 3 && clean.len() <= 50;
                let is_camel = clean.len() >= 4
                    && clean
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                    && clean.chars().skip(1).any(|c| c.is_lowercase())
                    && clean.chars().skip(2).any(|c| c.is_uppercase());

                if is_snake || is_camel {
                    symbols.push(clean.to_string());
                }
                if symbols.len() >= 3 {
                    break;
                }
            }
        }

        let mut out = String::from("<code_exploration_guidance>\n");
        out.push_str("  Autonomous AST & CodeGraph Guidance:\n");
        if !symbols.is_empty() {
            out.push_str(&format!(
                "  Candidate symbol(s) detected in user prompt: {}\n",
                symbols
                    .iter()
                    .map(|s| format!("`{}`", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            out.push_str("  1. Use `code_explore(symbol=\"...\")` or `code_explain(symbol=\"...\")` for fast surgical AST inspection without loading full files.\n");
        } else {
            out.push_str("  1. Use `code_explore(query=\"...\")` to locate definitions, callers, callees, and layer boundaries across the repository.\n");
        }
        out.push_str("  2. Use `diff_impact` before or after modifications to verify architectural layer boundaries and blast radius.\n");
        out.push_str("</code_exploration_guidance>");
        Some(out)
    }

    async fn enrich_runtime_dev() -> Option<String> {
        let dev_reg = crate::dev::get_global_dev_registry();
        let procs = dev_reg.list().await;
        let (active_count, rss_mb, cpu_pct) = dev_reg.get_telemetry_snapshot();
        let browser_live = crate::tools::browser::BrowserManager::is_live_engine_running_sync();

        let mut out = String::from("<active_dev_services>\n");
        if procs.is_empty() && !browser_live {
            out.push_str("  Managed Runtime Daemons & Tasks: None currently active.\n");
            out.push_str("  Autonomous Action: Use `minitask(action=\"start\")` to launch dev servers, or `minitask(action=\"schedule\")` for recurring tasks and timers.\n");
            out.push_str("  Directive: When the user asks what is happening or checks tasks, clearly inform them that no background processes or scheduled watchers are currently running.\n");
        } else {
            out.push_str(&format!(
                "  Supervised Tasks: {} active (Total RAM: {:.1}MB, CPU: {:.1}%)\n",
                active_count, rss_mb, cpu_pct
            ));
            if browser_live {
                out.push_str("  Browser: Live browser session is running.\n");
            }
            for p in &procs {
                let port_str = if p.ports.is_empty() {
                    String::new()
                } else {
                    format!(
                        " [Ports: {}]",
                        p.ports
                            .iter()
                            .map(|pt| pt.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                let sched_str = if let Some(ref sched) = p.schedule_info {
                    if sched.is_one_shot {
                        format!(" [Timer: {}s, Runs: {}]", sched.interval_secs, sched.iteration_count)
                    } else {
                        let max_str = sched.max_iterations.map(|m| format!("/{}", m)).unwrap_or_default();
                        format!(" [Schedule: Every {}s, Runs: {}{}]", sched.interval_secs, sched.iteration_count, max_str)
                    }
                } else {
                    String::new()
                };
                let pid_str = p
                    .pid
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "N/A".to_string());
                out.push_str(&format!(
                    "  • [{}] `{}` (PID: {}, Status: {}{}{}, RAM: {:.1}MB, CPU: {:.1}%)\n",
                    p.id, p.name, pid_str, p.status, port_str, sched_str, p.memory_rss_mb, p.cpu_percent
                ));

                if let Ok(recent_logs) = dev_reg.logs(&p.id, 3, None).await {
                    if !recent_logs.is_empty() {
                        out.push_str("    Recent Output:\n");
                        for line in recent_logs {
                            out.push_str(&format!("      │ {}\n", line.trim_end()));
                        }
                    }
                }
            }
            out.push_str("  Status & Inspection Invariant:\n");
            out.push_str("  • When the user asks what is happening, checks tasks, or requests telemetry, directly explain the current state of active services, scheduled tasks, and recent outputs above. Use `minitask(action=\"resources\")` or `minitask(action=\"status\")` if further live details are needed.\n");
            out.push_str("  Resource / Telemetry Invariant:\n");
            out.push_str("  • If the user asks about resource usage or telemetry, ALWAYS use `minitask(action=\"resources\")` or `minitask(action=\"status\")`. DO NOT execute raw shell commands like `ps`, `top`, or `grep`.\n");
            out.push_str("  Termination & Teardown Invariant:\n");
            out.push_str("  • If the user asks to stop, kill, or close the server, task, or website, ALWAYS call `minitask(action=\"stop\", id=\"...\")` (or `minitask(action=\"kill_all\")`) AND `browser_close` to terminate both the server process tree and browser engine. DO NOT run raw `pkill` or `kill` commands.\n");
        }
        out.push_str("</active_dev_services>");
        Some(out)
    }

    fn enrich_multiphase_engineering(workspace_root: &Path, _prompt: &str) -> Option<String> {
        let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_root);
        let docs_name = docs_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(crate::constants::MINIKIT_DOCS_DIR);

        let mut out = String::from("<autonomous_engineering_guidance>\n");
        out.push_str("  Autonomous 4-Gate Methodology Activated:\n");
        out.push_str(&format!(
            "  1. Gate 1 (Intent Anchor): Define the goal and bite-sized milestones. Record progress in `{}/core/todo.md` using `create_plan` or direct file edits.\n",
            docs_name
        ));
        out.push_str(&format!(
            "  2. Gate 2 (Architecture & Discovery): Consult relevant specs in `{}/core/` and verify dependencies before making changes.\n",
            docs_name
        ));
        out.push_str("  3. Gate 3 (TDD Implementation): Write or update tests FIRST. Verify failure (Red), then implement minimal code, then verify green.\n");
        out.push_str("  4. Gate 4 (Verification Barrier): Execute compiler/test checks (`cargo test -j 1 ...`, `npm test`, etc.) to confirm 0 errors before concluding.\n");
        out.push_str("  5. Anti-Monolith Invariant: Write modular, cleanly scoped files under 300 lines rather than giant single files to prevent JSON EOF parsing cutoffs.\n");
        out.push_str(
            "  6. Command Invariant: Run all builds, tests, and dev servers with `exec_cmd`.\n",
        );
        out.push_str("</autonomous_engineering_guidance>");
        Some(out)
    }

    fn enrich_vault_skills(workspace_root: &Path, prompt: &str) -> Option<String> {
        let store = crate::vault::store::VaultStore::new(workspace_root);
        let all_skills = store.list_all_skills();
        let all_bundles = store.list_all_bundles();
        let lower = prompt.to_ascii_lowercase();

        let matched_skills: Vec<_> = all_skills
            .into_iter()
            .filter(|s| s.matches_prompt(&lower))
            .take(6)
            .collect();

        let matched_bundles: Vec<_> = all_bundles
            .into_iter()
            .filter(|b| b.matches_query(&lower))
            .take(3)
            .collect();

        let mut out = String::from("<minivault_skills_guidance>\n");
        out.push_str("  MiniVault Tool Suite: `vault_search`, `vault_show`, `vault_load`, `vault_unload`, `vault_create`, `vault_update`, `vault_delete`, `vault_import_url`, `vault_bundle_list`, `vault_bundle_load`, `vault_bundle_create`, `vault_ingest_source`, `vault_gotchas_list`.\n");

        if !matched_bundles.is_empty() {
            out.push_str("  Matched Skill Bundles in MiniVault:\n");
            for b in &matched_bundles {
                out.push_str(&format!(
                    "  • Bundle `{}` ({} skills: {}): {}\n",
                    b.name,
                    b.skills.len(),
                    b.skills.join(", "),
                    b.description
                ));
            }
            out.push_str("  Autonomous Action: Call `vault_bundle_load(name)` to install this entire stack into the project.\n");
        }

        let has_matched_skills = !matched_skills.is_empty();
        if has_matched_skills {
            let (docs, workflows): (Vec<_>, Vec<_>) = matched_skills
                .into_iter()
                .partition(|s| s.kind() == crate::vault::models::SkillKind::Reference);

            if !docs.is_empty() {
                out.push_str("  Matched Technical Reference Guides & Invariants ([Doc]):\n");
                for d in &docs {
                    let status = if d.is_active_in_project {
                        "Active in Project"
                    } else {
                        "Available in Vault"
                    };
                    out.push_str(&format!(
                        "  • [Doc] `{}` [{:?} | {}]: {}\n",
                        d.name, d.scope, status, d.description
                    ));
                }
            }

            if !workflows.is_empty() {
                out.push_str("  Matched Operational Engineering Methodologies ([Skill]):\n");
                for w in &workflows {
                    let status = if w.is_active_in_project {
                        "Active in Project"
                    } else {
                        "Available in Vault"
                    };
                    out.push_str(&format!(
                        "  • [Skill] `{}` [{:?} | {}]: {}\n",
                        w.name, w.scope, status, w.description
                    ));
                }
            }

            out.push_str("  Autonomous Action: Call `vault_load(name)` to install into active project, or `vault_show(name)` to inspect full rules.\n");
        }

        let matched_gotchas = store.find_relevant_gotchas(&lower);
        if !matched_gotchas.is_empty() {
            out.push_str(
                "  Universal Machine Gotchas & Compiler Traps (Learned from previous sessions):\n",
            );
            for g in &matched_gotchas {
                out.push_str(&format!(
                    "  {}\n",
                    g.format_prompt_block().replace('\n', "\n  ")
                ));
            }
            out.push_str("  Autonomous Action: Call `vault_gotchas_list()` to inspect full negative knowledge.\n");
        }

        let matched_sources = store.search_sources(&lower);
        if !matched_sources.is_empty() {
            out.push_str("  Bookmarked External Knowledge Sources:\n");
            for s in matched_sources.iter().take(3) {
                out.push_str(&format!(
                    "  • [{}] `{}`: {} ({})\n",
                    s.kind.badge(),
                    s.title,
                    s.summary,
                    s.uri
                ));
            }
            out.push_str("  Autonomous Action: Call `vault_ingest_source(uri)` to bookmark additional docs or repos.\n");
        }

        if matched_bundles.is_empty()
            && !has_matched_skills
            && matched_gotchas.is_empty()
            && matched_sources.is_empty()
        {
            out.push_str("  Autonomous Action: Call `vault_bundle_list()` for curated stacks, `vault_search(query)` to find skills, `vault_ingest_source(uri)` to bookmark URLs/repos, or `vault_import_url(url)` to download skills.\n");
        }
        out.push_str("</minivault_skills_guidance>");
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_router_classify_vault_skills() {
        assert_eq!(
            WorkflowRouter::classify("Add skill from internet: https://example.com/skill.md"),
            WorkflowArchetype::VaultSkills
        );
        assert_eq!(
            WorkflowRouter::classify("Load skill tailwind-v4 into this project"),
            WorkflowArchetype::VaultSkills
        );
        assert_eq!(
            WorkflowRouter::classify("Search skills for react in minivault"),
            WorkflowArchetype::VaultSkills
        );
        assert_eq!(
            WorkflowRouter::classify("Load fullstack-nextjs skill bundle"),
            WorkflowArchetype::VaultSkills
        );
        assert_eq!(
            WorkflowRouter::classify("Show available skill bundles in minivault"),
            WorkflowArchetype::VaultSkills
        );
        assert_eq!(
            WorkflowRouter::classify("Ingest source from https://docs.rs/tokio into minivault"),
            WorkflowArchetype::VaultSkills
        );
        assert_eq!(
            WorkflowRouter::classify("Show learned compiler gotchas and traps"),
            WorkflowArchetype::VaultSkills
        );
        assert_eq!(
            WorkflowRouter::classify("Learn from repo at /path/to/my-repo"),
            WorkflowArchetype::VaultSkills
        );
    }

    #[test]
    fn test_workflow_router_classify_ui_design() {
        assert_eq!(
            WorkflowRouter::classify(
                "Create a modern landing page with a pricing card in dark mode"
            ),
            WorkflowArchetype::UiDesign
        );
        assert_eq!(
            WorkflowRouter::classify("Style the sidebar and buttons with tailwind"),
            WorkflowArchetype::UiDesign
        );
    }

    #[test]
    fn test_workflow_router_classify_runtime_dev() {
        assert_eq!(
            WorkflowRouter::classify("Start the dev server on port 3000"),
            WorkflowArchetype::RuntimeDev
        );
        assert_eq!(
            WorkflowRouter::classify("Check the server logs with minitask"),
            WorkflowArchetype::RuntimeDev
        );
        assert_eq!(
            WorkflowRouter::classify("what's happening with the background tasks?"),
            WorkflowArchetype::RuntimeDev
        );
        assert_eq!(
            WorkflowRouter::classify("whats happening"),
            WorkflowArchetype::RuntimeDev
        );
        assert_eq!(
            WorkflowRouter::classify("what is running"),
            WorkflowArchetype::RuntimeDev
        );
        assert_eq!(
            WorkflowRouter::classify("status of tasks"),
            WorkflowArchetype::RuntimeDev
        );
        assert_eq!(
            WorkflowRouter::classify("is the server running?"),
            WorkflowArchetype::RuntimeDev
        );
        assert_eq!(
            WorkflowRouter::classify("how are background processes doing"),
            WorkflowArchetype::RuntimeDev
        );
    }

    #[test]
    fn test_workflow_router_classify_code_exploration() {
        assert_eq!(
            WorkflowRouter::classify("Where is `process_payment` defined and who calls it?"),
            WorkflowArchetype::CodeExploration
        );
        assert_eq!(
            WorkflowRouter::classify("Analyze the blast radius and call hierarchy of this struct"),
            WorkflowArchetype::CodeExploration
        );
    }

    #[test]
    fn test_workflow_router_classify_multiphase_engineering() {
        assert_eq!(
            WorkflowRouter::classify("Implement user authentication feature with tests"),
            WorkflowArchetype::MultiPhaseEngineering
        );
        assert_eq!(
            WorkflowRouter::classify(
                "Refactor the database architecture layer with acceptance criteria"
            ),
            WorkflowArchetype::MultiPhaseEngineering
        );
    }

    #[test]
    fn test_workflow_router_classify_standard() {
        assert_eq!(
            WorkflowRouter::classify("Fix typo on line 42 in main.rs"),
            WorkflowArchetype::Standard
        );
    }

    #[tokio::test]
    async fn test_workflow_router_enrichment() {
        let temp = tempfile::tempdir().unwrap();
        let prompt_ui = "Build a responsive navbar component";
        let enrichment =
            WorkflowRouter::enrich_context(temp.path(), prompt_ui, WorkflowArchetype::UiDesign)
                .await;
        assert!(enrichment.is_some());
        let text = enrichment.unwrap();
        assert!(text.contains("<recommended_miniblocks>"));

        let prompt_ast = "Trace callers of `execute_turn`";
        let enrichment_ast = WorkflowRouter::enrich_context(
            temp.path(),
            prompt_ast,
            WorkflowArchetype::CodeExploration,
        )
        .await;
        assert!(enrichment_ast.is_some());
        let text_ast = enrichment_ast.unwrap();
        assert!(text_ast.contains("<code_exploration_guidance>"));
        assert!(text_ast.contains("`execute_turn`"));

        let enrichment_eng = WorkflowRouter::enrich_context(
            temp.path(),
            "Implement feature X",
            WorkflowArchetype::MultiPhaseEngineering,
        )
        .await;
        assert!(enrichment_eng.is_some());
        let text_eng = enrichment_eng.unwrap();
        assert!(text_eng.contains("<autonomous_engineering_guidance>"));
    }
}
