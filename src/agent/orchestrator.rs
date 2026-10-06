use crate::agent::subagent::{
    get_global_scratchpad, get_global_subagent_pool, SubAgentResult, SubagentConfig, SubagentRole,
    SubagentTaskSpec,
};
use crate::error::{MinicodeError, Result, ToolError};
use crate::git::worktree::WorktreeManager;
use std::path::{Path, PathBuf};

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
    /// MiniKit project scaffolding, starter stacks, dependencies, and package management.
    MiniKitScaffolding,
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

/// Dynamic repository classification based on filesystem discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoState {
    /// Completely fresh workspace with no source code or documentation
    FreshWorkspace,
    /// Workspace containing documentation, markdown, or configs, but no application source code
    DocsOnly,
    /// Active software codebase with parseable source code files (.rs, .ts, .js, .py, etc.)
    ExistingCodebase,
}

impl WorkflowArchetype {
    /// Returns the recommended tool categories for this execution archetype.
    pub fn recommended_categories(&self) -> Vec<crate::tools::category::ToolCategory> {
        use crate::tools::category::ToolCategory;
        match self {
            Self::UiDesign => vec![
                ToolCategory::Blocks,
                ToolCategory::MiniKit,
                ToolCategory::Web,
                ToolCategory::Files,
                ToolCategory::Agent,
                ToolCategory::Dev,
            ],
            Self::MiniKitScaffolding => vec![
                ToolCategory::MiniKit,
                ToolCategory::Files,
                ToolCategory::Exec,
                ToolCategory::Blocks,
                ToolCategory::Memory,
                ToolCategory::Agent,
                ToolCategory::Web,
            ],
            Self::CodeExploration => vec![
                ToolCategory::Codegraph,
                ToolCategory::Search,
                ToolCategory::Memory,
                ToolCategory::Files,
            ],
            Self::RuntimeDev => vec![
                ToolCategory::Dev,
                ToolCategory::Exec,
                ToolCategory::Web,
                ToolCategory::Files,
            ],
            Self::MultiPhaseEngineering => vec![
                ToolCategory::MiniPower,
                ToolCategory::Memory,
                ToolCategory::Files,
                ToolCategory::Exec,
                ToolCategory::MiniKit,
                ToolCategory::Blocks,
                ToolCategory::Agent,
                ToolCategory::Web,
                ToolCategory::Dev,
            ],
            Self::VaultSkills => vec![
                ToolCategory::Vault,
                ToolCategory::MiniKit,
                ToolCategory::Files,
                ToolCategory::Web,
            ],
            Self::Standard => vec![],
        }
    }
}

#[inline]
fn contains_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|&p| text.contains(p))
}

#[inline]
fn has_any_word(text: &str, words: &[&str]) -> bool {
    words.iter().any(|&w| crate::utils::has_word(text, w))
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
        let has_vault_term =
            contains_any(&lower, &["vault", "minivault", "gotcha", "compiler trap"]);

        let has_skill_action = has_any_word(&lower, &["skill", "skills"])
            && contains_any(
                &lower,
                &[
                    "add", "load", "install", "import", "create", "update", "delete", "remove",
                    "search", "list", "show", "internet", "github", "project", "global",
                ],
            );

        let has_bundle_action = has_any_word(&lower, &["bundle", "bundles"])
            && contains_any(
                &lower,
                &[
                    "skill",
                    "load",
                    "install",
                    "create",
                    "list",
                    "show",
                    "stack",
                    "fullstack",
                ],
            );

        let has_learn_ingest = (contains_any(&lower, &["http://", "https://", "website"])
            || crate::utils::has_word(&lower, "repo"))
            && contains_any(
                &lower,
                &["learn", "ingest", "bookmark", "save link", "reference"],
            );

        if has_vault_term || has_skill_action || has_bundle_action || has_learn_ingest {
            return WorkflowArchetype::VaultSkills;
        }

        // 1. MiniKit Architecture Stacks, Scaffolding & Dependencies Archetype
        let has_minikit_term = contains_any(
            &lower,
            &[
                "minikit",
                "kit ",
                "kit_",
                "onpkg",
                "scaffold",
                "starter stack",
                "stack template",
                "add dep",
                "install dep",
                "cargo add",
                "npm install",
                "bun add",
                "yarn add",
                "pip install",
                "drift",
                "self-heal",
            ],
        );
        let has_kit_action = has_any_word(&lower, &["stack", "package", "dependency"])
            && contains_any(
                &lower,
                &["add", "install", "scaffold", "list", "show", "diff"],
            );

        if has_minikit_term || has_kit_action {
            return WorkflowArchetype::MiniKitScaffolding;
        }

        // 2. UI Design, Landing Pages & Component Warehouse Archetype
        let is_explicit_ui = contains_any(
            &lower,
            &[
                "landing page",
                "landing",
                "sidebar",
                "navbar",
                "hero section",
                "ui design",
                "design token",
                "tailwind",
                "palette",
                "gradient",
                "dark mode",
                "light mode",
                "wireframe",
            ],
        ) || has_any_word(&lower, &["ui", "frontend", "css", "styling"]);

        let has_ui_component_term = has_any_word(
            &lower,
            &[
                "component",
                "components",
                "button",
                "modal",
                "card",
                "dialog",
            ],
        );

        // Disambiguate data structure / database terms from UI components
        let has_data_backend_term = contains_any(
            &lower,
            &[
                "database",
                "hash table",
                "sql table",
                "pricing calculation",
                "pricing algorithm",
            ],
        );

        let has_explicit_plan_request = contains_any(
            &lower,
            &[
                "detailed plan",
                "power plan",
                "superpower",
                "acceptance criteria",
                "verification barrier",
            ],
        );

        if (is_explicit_ui || has_ui_component_term)
            && !has_data_backend_term
            && !has_explicit_plan_request
        {
            return WorkflowArchetype::UiDesign;
        }

        // 3. CodeGraph, AST Slicing & Architecture Exploration Archetype
        let has_graph_keyword = contains_any(
            &lower,
            &[
                "blast radius",
                "code graph",
                "codegraph",
                "call hierarchy",
                "dependency graph",
                "repo map",
                "repomap",
                "code_explore",
                "diff_impact",
                "code_explain",
                "code_trace",
            ],
        );

        let has_trace_query = contains_any(
            &lower,
            &["who calls", "call trace", "trace flow", "impact of"],
        ) || has_any_word(&lower, &["callers", "callees"])
            || (crate::utils::has_word(&lower, "where")
                && (crate::utils::has_word(&lower, "defined")
                    || crate::utils::has_word(&lower, "located")))
            || (crate::utils::has_word(&lower, "trace")
                && (crate::utils::has_word(&lower, "call")
                    || crate::utils::has_word(&lower, "function")
                    || crate::utils::has_word(&lower, "symbol")
                    || crate::utils::has_word(&lower, "flow")));

        if has_graph_keyword || has_trace_query {
            return WorkflowArchetype::CodeExploration;
        }

        // 4. Runtime Dev & Process Orchestration Archetype
        let has_process_term = contains_any(
            &lower,
            &[
                "dev server",
                "minitask",
                "background process",
                "background task",
                "scheduled task",
                "one-shot timer",
                "periodic check",
                "listen on port",
            ],
        ) || crate::utils::has_word(&lower, "daemon")
            || lower.starts_with("serve ")
            || lower.contains(" port ");

        let has_server_action = crate::utils::has_word(&lower, "server")
            && has_any_word(
                &lower,
                &[
                    "start", "run", "launch", "serve", "status", "logs", "kill", "stop", "restart",
                ],
            );

        let has_resource_query = contains_any(
            &lower,
            &[
                "resource usage",
                "resources used",
                "resources taking",
                "how much resource",
                "ram usage",
                "cpu usage",
            ],
        );

        let has_status_inspection = contains_any(
            &lower,
            &[
                "whats happening",
                "what's happening",
                "what is happening",
                "what is running",
                "what's running",
                "whats running",
                "status of task",
                "task status",
                "process status",
                "is the server running",
                "is server running",
                "check background",
                "active processes",
                "active tasks",
            ],
        ) || (contains_any(&lower, &["how are", "what are"])
            && contains_any(&lower, &["background processes", "tasks"]));

        let has_teardown_command = matches!(lower.trim(), "stop" | "kill" | "stop it" | "kill it")
            || contains_any(
                &lower,
                &[
                    "stop that",
                    "kill that",
                    "close browser",
                    "stop browser",
                    "kill browser",
                    "close website",
                    "stop website",
                    "kill website",
                    "manage task",
                ],
            );

        if has_process_term
            || has_server_action
            || has_resource_query
            || has_status_inspection
            || has_teardown_command
        {
            return WorkflowArchetype::RuntimeDev;
        }

        // 5. Multi-Phase Engineering, Planning & TDD Archetype
        let has_planning_token = contains_any(
            &lower,
            &[
                "power plan",
                "superpower",
                "minipower",
                "acceptance criteria",
                "verification barrier",
                "detailed plan",
            ],
        ) || has_any_word(&lower, &["tdd", "milestone", "milestones"]);

        let has_construction_intent = has_any_word(
            &lower,
            &["implement", "build", "create", "make", "refactor"],
        ) && (has_any_word(
            &lower,
            &[
                "feature",
                "module",
                "system",
                "service",
                "api",
                "architecture",
                "codebase",
                "layer",
                "website",
                "portfolio",
                "webapp",
                "app",
                "application",
                "fullstack",
            ],
        ) || (crate::utils::has_word(&lower, "project")
            && !crate::utils::has_word(&lower, "into")));

        if has_planning_token || has_construction_intent {
            return WorkflowArchetype::MultiPhaseEngineering;
        }

        WorkflowArchetype::Standard
    }

    /// Synthesizes contextual enrichment block according to the detected archetype.
    pub async fn enrich_context(
        workspace_root: &Path,
        prompt: &str,
        archetype: WorkflowArchetype,
    ) -> Option<String> {
        let dev_reg = crate::dev::get_global_dev_registry();
        let (active_task_count, _, _) = dev_reg.get_telemetry_snapshot();
        let browser_live = crate::tools::browser::BrowserManager::is_live_engine_running_sync();
        let has_live_runtime = active_task_count > 0 || browser_live;

        let mut sections: Vec<String> = Vec::new();

        // 0. Dynamic Repository State & File Discovery
        let (repo_state, code_files, doc_files) = Self::scan_repository_state(workspace_root);
        sections.push(Self::enrich_repository_state(
            workspace_root,
            repo_state,
            &code_files,
            &doc_files,
        ));
        sections.push(Self::enrich_orchestrator_guidance(
            workspace_root,
            prompt,
            repo_state,
        ));
        sections.push(Self::enrich_minipower_rules_and_freedom(
            workspace_root,
            prompt,
        ));

        // 1. Dynamic Plan Status Grounding (Oh My Pi / SWE-Agent Task Reconciler)
        if let Some(plan_block) = Self::enrich_plan_status(workspace_root) {
            sections.push(plan_block);
        }

        // 2. Base archetype enrichment
        let base_enrichment = match archetype {
            WorkflowArchetype::UiDesign => Self::enrich_ui_design(workspace_root, prompt),
            WorkflowArchetype::MiniKitScaffolding => {
                Self::enrich_minikit_scaffolding(workspace_root, prompt)
            }
            WorkflowArchetype::CodeExploration => Self::enrich_code_exploration(prompt),
            WorkflowArchetype::RuntimeDev => Self::enrich_runtime_dev().await,
            WorkflowArchetype::MultiPhaseEngineering => {
                Self::enrich_multiphase_engineering(workspace_root, prompt)
            }
            WorkflowArchetype::VaultSkills => Self::enrich_vault_skills(workspace_root, prompt),
            WorkflowArchetype::Standard => None,
        };

        // State-Grounded Runtime Injection:
        // If background tasks or browser session are actively running and archetype was not RuntimeDev,
        // prepend the live dev services block so the agent is ALWAYS aware of its active runtime environment.
        if has_live_runtime && archetype != WorkflowArchetype::RuntimeDev {
            if let Some(runtime_block) = Self::enrich_runtime_dev().await {
                sections.push(runtime_block);
            }
        }

        if let Some(base) = base_enrichment {
            sections.push(base);
        }

        // 3. Dynamic Core Documentation Specifications (PRD, Specs, Architecture, Design)
        if let Some(docs_block) = Self::enrich_core_documentation(workspace_root, prompt) {
            sections.push(docs_block);
        }

        // 4. Hermes Progressive Skills Enrichment (Auto-activate when relevant)
        if archetype != WorkflowArchetype::VaultSkills {
            if let Some(skills_block) = Self::enrich_hermes_skills(workspace_root, prompt) {
                sections.push(skills_block);
            }
        }

        // 5. Universal learned gotchas for engineering workflows
        if matches!(
            archetype,
            WorkflowArchetype::MultiPhaseEngineering | WorkflowArchetype::Standard
        ) {
            let store = crate::vault::store::VaultStore::new(workspace_root);
            let lower = prompt.to_ascii_lowercase();
            let relevant_gotchas = store.find_relevant_gotchas(&lower);
            if !relevant_gotchas.is_empty() {
                let mut gotcha_block = String::from("<universal_learned_gotchas>\n");
                gotcha_block.push_str("  Known Pitfalls & Invariants (Learned from previous sessions across this machine):\n");
                for g in relevant_gotchas.iter().take(3) {
                    gotcha_block.push_str(&format!(
                        "  {}\n",
                        g.format_prompt_block().replace('\n', "\n  ")
                    ));
                }
                gotcha_block.push_str("  Directive: Heed these hard-won lessons to prevent repeating previous compiler and runtime errors.\n");
                gotcha_block.push_str("</universal_learned_gotchas>");
                sections.push(gotcha_block);
            }
        }

        if sections.is_empty() {
            None
        } else {
            Some(sections.join("\n\n"))
        }
    }

    /// Extracts live plan progress from working memory to ensure the agent is aware of
    /// pending, in-progress, and completed tasks at each step.
    fn enrich_plan_status(workspace_root: &Path) -> Option<String> {
        let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_root);
        if !wm.has_active_plan() {
            return None;
        }

        let tasks = wm.read_parsed_tasks();
        if tasks.is_empty() {
            return None;
        }

        let total = tasks.len();
        let completed = tasks
            .iter()
            .filter(|t| {
                t.status == crate::context::memory::working_memory::TaskItemStatus::Completed
            })
            .count();
        let in_progress = tasks.iter().find(|t| {
            t.status == crate::context::memory::working_memory::TaskItemStatus::InProgress
        });

        let mut out = String::from("<active_task_plan_status>\n");
        out.push_str(&format!(
            "  Execution Plan State: {}/{} tasks completed\n",
            completed, total
        ));
        if let Some(active) = in_progress {
            out.push_str(&format!(
                "  ► Currently In Progress: \"{}\"\n",
                active.title
            ));
        }
        out.push_str("  Task Checklist:\n");
        for t in &tasks {
            let mark = match t.status {
                crate::context::memory::working_memory::TaskItemStatus::Completed => "[x]",
                crate::context::memory::working_memory::TaskItemStatus::InProgress => "[>]",
                crate::context::memory::working_memory::TaskItemStatus::Pending => "[ ]",
            };
            out.push_str(&format!("    • {} {}\n", mark, t.title));
        }
        out.push_str("  Plan Reconciliation Invariant (Oh My Pi / SWE-Agent Standard):\n");
        out.push_str("  • Keep focus strictly on the in-progress step.\n");
        out.push_str("  • Upon completing a task's code or verification, call `update_progress` immediately so no completed steps linger as `[>]`.\n");
        out.push_str("  • When the final task is verified, call `update_progress(step=\"active\", status=\"completed\")` to achieve 100% plan completion.\n");
        out.push_str("  • Never batch all `update_progress` calls together at the end of a multi-step turn; advance tasks atomically as you execute.\n");
        out.push_str("</active_task_plan_status>");
        Some(out)
    }

    /// Dynamically scans workspace files and directories to detect if this is a fresh workspace,
    /// docs-only workspace, or existing software codebase, without hardcoded names or rigid assumptions.
    pub fn scan_repository_state(workspace_root: &Path) -> (RepoState, Vec<PathBuf>, Vec<PathBuf>) {
        if !workspace_root.exists() {
            return (RepoState::FreshWorkspace, Vec::new(), Vec::new());
        }

        let walker = ignore::WalkBuilder::new(workspace_root)
            .hidden(true)
            .git_ignore(true)
            .max_depth(Some(4))
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    "node_modules"
                        | "target"
                        | "dist"
                        | "build"
                        | ".next"
                        | "vendor"
                        | ".venv"
                        | "venv"
                        | "__pycache__"
                        | ".git"
                        | ".minicode"
                )
            })
            .build();

        let mut code_files = Vec::new();
        let mut doc_files = Vec::new();
        let mut has_other_files = false;

        const CODE_EXTS: &[&str] = &[
            "rs", "ts", "tsx", "js", "jsx", "py", "go", "c", "cpp", "cc", "cxx", "h", "hpp",
            "java", "kt", "kts", "swift", "rb", "php", "cs", "scala", "vue", "svelte", "html",
            "css", "scss", "sass", "less", "sh", "bash", "zsh", "sql",
        ];
        const DOC_EXTS: &[&str] = &["md", "markdown", "mdown", "txt", "rst", "adoc", "org"];

        for result in walker.flatten() {
            if result.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                let path = result.path();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if name.starts_with('.') {
                    continue;
                }
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_ascii_lowercase();
                    if CODE_EXTS.contains(&ext_lower.as_str()) {
                        if code_files.len() < 30 {
                            code_files.push(path.to_path_buf());
                        }
                    } else if DOC_EXTS.contains(&ext_lower.as_str()) {
                        if doc_files.len() < 30 {
                            doc_files.push(path.to_path_buf());
                        }
                    } else {
                        has_other_files = true;
                    }
                } else {
                    has_other_files = true;
                }
            }
        }

        let state = if !code_files.is_empty() {
            RepoState::ExistingCodebase
        } else if !doc_files.is_empty() || has_other_files {
            RepoState::DocsOnly
        } else {
            RepoState::FreshWorkspace
        };

        (state, code_files, doc_files)
    }

    /// Injects dynamic repository state analysis into context so the agent knows whether
    /// the workspace is fresh, docs-only, or an existing codebase, and what files are present.
    pub fn enrich_repository_state(
        workspace_root: &Path,
        state: RepoState,
        code_files: &[PathBuf],
        doc_files: &[PathBuf],
    ) -> String {
        let mut out = String::from("<workspace_repository_state>\n");
        match state {
            RepoState::FreshWorkspace => {
                out.push_str("  Status: Fresh Workspace (Zero prior code or docs files)\n");
                out.push_str("  AST Indexing: Bypassed (no code to index).\n");
                out.push_str("  Directive: If asked to build/create a project, enter Inception Workflow: create core specifications in `minikit_docs/core/` and scaffold via `kit_stack_add`.\n");
            }
            RepoState::DocsOnly => {
                out.push_str("  Status: Documentation / Non-Coded Workspace (No application source code files)\n");
                out.push_str("  AST Indexing: Bypassed (no code to index).\n");
                out.push_str("  Discovered Documentation Files in Workspace:\n");
                for d in doc_files.iter().take(6) {
                    let rel = d.strip_prefix(workspace_root).unwrap_or(d);
                    out.push_str(&format!("    • `{}`\n", rel.display()));
                }
                out.push_str("  Directive: Ground implementations in these existing docs before scaffolding.\n");
            }
            RepoState::ExistingCodebase => {
                out.push_str(&format!(
                    "  Status: Existing Software Codebase ({} source files detected)\n",
                    code_files.len()
                ));
                out.push_str("  Key Source Files:\n");
                for c in code_files.iter().take(6) {
                    let rel = c.strip_prefix(workspace_root).unwrap_or(c);
                    out.push_str(&format!("    • `{}`\n", rel.display()));
                }
                if !doc_files.is_empty() {
                    out.push_str("  Documentation Files:\n");
                    for d in doc_files.iter().take(4) {
                        let rel = d.strip_prefix(workspace_root).unwrap_or(d);
                        out.push_str(&format!("    • `{}`\n", rel.display()));
                    }
                }
                out.push_str("  Directive: Ground changes in existing architecture and symbols (`locate_symbol`, `grep_search`). Maintain existing conventions.\n");
            }
        }
        out.push_str("</workspace_repository_state>");
        out
    }

    /// Injects dynamic orchestrator workflow guidance for fresh project inception,
    /// specification synthesis, information completeness evaluation, ask_user gating,
    /// and web research.
    pub fn enrich_orchestrator_guidance(
        _workspace_root: &Path,
        prompt: &str,
        state: RepoState,
    ) -> String {
        let mut out = String::from("<orchestrator_dynamic_guidance>\n");
        let lower = prompt.to_ascii_lowercase();

        let is_creation_intent = has_any_word(
            &lower,
            &[
                "create",
                "build",
                "make",
                "scaffold",
                "develop",
                "setup",
                "bootstrap",
                "start",
            ],
        ) && has_any_word(
            &lower,
            &[
                "project", "app", "website", "landing", "page", "service", "api", "tool", "stack",
                "system",
            ],
        );

        let has_explicit_stack = {
            let direct_stack_declarations = [
                "use react",
                "using react",
                "with react",
                "in react",
                "react app",
                "react project",
                "react + vite",
                "react/vite",
                "use vite",
                "using vite",
                "with vite",
                "use vue",
                "using vue",
                "with vue",
                "in vue",
                "vue app",
                "vue project",
                "use svelte",
                "using svelte",
                "with svelte",
                "in svelte",
                "svelte app",
                "use nextjs",
                "using nextjs",
                "with nextjs",
                "in nextjs",
                "use next.js",
                "using next.js",
                "with next.js",
                "in next.js",
                "nextjs app",
                "next.js app",
                "use vanilla",
                "using vanilla",
                "vanilla html",
                "pure html",
                "static html",
                "plain html",
                "vanilla js",
                "use tailwind",
                "using tailwind",
                "with tailwind",
                "use fastapi",
                "using fastapi",
                "with fastapi",
                "in fastapi",
                "use flask",
                "using flask",
                "use django",
                "using django",
                "use express",
                "using express",
                "use hono",
                "using hono",
                "use actix",
                "using actix",
                "use axum",
                "using axum",
                "use astro",
                "using astro",
                "in rust",
                "using rust",
                "stack:",
                "tech stack:",
                "framework:",
            ];
            contains_any(&lower, &direct_stack_declarations)
        };
        let has_explicit_theme_or_style = has_any_word(
            &lower,
            &[
                "dark",
                "light",
                "minimal",
                "futuristic",
                "cyberpunk",
                "modern",
                "neon",
                "retro",
                "monochrome",
                "gradient",
                "glassmorphism",
                "aesthetic",
                "palette",
                "theme",
            ],
        );
        let is_rich_prompt = has_explicit_stack && has_explicit_theme_or_style;

        match state {
            RepoState::FreshWorkspace | RepoState::DocsOnly => {
                if is_creation_intent || state == RepoState::FreshWorkspace {
                    out.push_str("  Autonomous Inception & Specification Architecture:\n");
                    out.push_str("  1. Specification Synthesis First:\n");
                    out.push_str("     Before creating application code files, establish core project specifications in `minikit_docs/core/`:\n");
                    out.push_str("     • `prd.md`: Product Requirements Document (Core purpose, user stories, success metrics, constraints, non-goals)\n");
                    out.push_str("     • `design.md`: Visual Design Specification (Theme tokens, typography, layout hierarchy, components, breakpoints)\n");
                    out.push_str("     • `architecture.md`: Clean Architecture & Dependency Contract (Module boundaries, data flow, state management)\n");
                    out.push_str("     • `spec.md`: Technical Invariants & Protocol Specification (APIs, contracts, error boundaries)\n");
                    out.push_str("     • `todo.md`: Initialized via `create_plan` with verifiable bite-sized milestones.\n");

                    if is_rich_prompt {
                        out.push_str(
                            "  2. Information Completeness: HIGH (Rich Specification Provided):\n",
                        );
                        out.push_str("     The user provided initial stack/theme directives, but critical design nuances, layout priorities, and component details remain open.\n");
                        out.push_str("     • Include Step 1 in your active plan: \"Clarify key feature priorities and theme nuances with user via ask_user\".\n");
                        out.push_str("     • Call `ask_user` to present targeted options (e.g. layout structure, specific cyber aesthetic accents, interactive component priorities) before scaffolding or writing application code.\n");
                        out.push_str("     • Once confirmed, Synthesize detailed `.md` core files directly in `minikit_docs/core/`, then scaffold using `kit_stack_add` and implement modular components (<250 lines per file).\n");
                    } else {
                        out.push_str(
                            "  2. Information Completeness: UNDERSPECIFIED (Interactive Clarification Required):\n",
                        );
                        out.push_str("     Target tech stack, design theme, or architectural bounds are not fully settled by the user.\n");
                        out.push_str("     • Include Step 1 in your active plan: \"Clarify tech stack, design aesthetic, and scope with user via ask_user\".\n");
                        out.push_str("     • YOU MUST CALL `ask_user` ON TURN 1 to present 2-3 structured choices for:\n");
                        out.push_str("       • Tech Stack & Framework (e.g. React+Vite+Tailwind, Modern Vanilla HTML5/CSS3/ES6, Next.js, FastAPI)\n");
                        out.push_str("       • Visual Theme & Aesthetic (e.g. Dark Modern Futuristic Neon, Clean Minimalist Monochrome, High-Contrast Light)\n");
                        out.push_str("       • Scope & Key Features\n");
                        out.push_str("     DO NOT write code or create project files before asking! Once the user answers, create the detailed `.md` files in `minikit_docs/core/` and advance to the next step.\n");
                    }

                    out.push_str(
                        "  3. Two-Tier Planning & Progressive Step-by-Step Task Advancement:\n",
                    );
                    out.push_str("     • Tier 1 (Core Roadmap): `minikit_docs/core/todo.md` tracks high-level strategic milestones (viewed in /todo modal).\n");
                    out.push_str("     • Tier 2 (Active MiniPower Step Plan): `.minicode/plan/task_plan.md` tracks atomic 2-5 min tactical execution steps (viewed live in TUI dock).\n");
                    out.push_str("     • Call `create_plan` with 4-8 focused, bite-sized steps. Step 1 should be interactive clarification via `ask_user`.\n");
                    out.push_str("     • Execute sequentially: work on Step 1, call `update_progress(step=\"1\", status=\"completed\")` immediately to advance to Step 2, and repeat.\n");
                    out.push_str("     • NEVER batch all `update_progress` calls together at the very end of your turn! The live TUI dock displays your progress in real-time.\n");
                    out.push_str("     • Conclude the turn by verifying the final task and marking it completed.\n");
                    out.push_str("  4. Starter Stacks & Anti-Monolith Invariant:\n");
                    out.push_str("     • Scaffolding: Call `kit_stack_add` to scaffold starter project templates in 1 tool call rather than hand-authoring files from scratch.\n");
                    out.push_str("     • NEVER write massive monolithic single files (>250-300 lines or >10KB). Decompose HTML, CSS, and JS into modular files to avoid JSON token truncation cutoffs.\n");
                    out.push_str("     • UI Design: Call `block_palettes` for color tokens and `block_search` / `block_scaffold` for pre-built components.\n");
                    out.push_str("  5. Research on Doubt / Uncertainty:\n");
                    out.push_str("     If you have any doubt regarding modern library APIs, framework compatibility, or best practices, call `search_web` or `fetch_or_browse` to ground yourself before writing specifications or code.\n");
                }
            }
            RepoState::ExistingCodebase => {
                out.push_str("  Existing Codebase Engineering Architecture:\n");
                out.push_str("  1. Grounding & CodeGraph: Inspect existing architecture, imports, and types (`locate_symbol`, `grep_search`, `read_file`) before writing code.\n");
                out.push_str("  2. Invariants: Respect existing project patterns, styling, and coding conventions.\n");
                out.push_str("  3. Living Specs: Inspect available `.md` documentation and update `minikit_docs/core/todo.md` via `create_plan` or `update_progress` as tasks complete.\n");
                out.push_str("  4. Research: If troubleshooting unfamiliar libraries or legacy patterns, use `search_web` to look up official documentation.\n");
                out.push_str("  5. MiniKit & Blocks: If adding new features, dependencies, or UI elements, leverage `kit_info`, `kit_add`, and MiniBlocks (`block_search`) to maintain modularity.\n");
            }
        }

        out.push_str("</orchestrator_dynamic_guidance>");
        out
    }

    /// MiniPower Methodology & Orchestrator Freedom Directive
    /// Grounding the orchestrator with MiniPower's 6 core engineering pillars,
    /// anti-rationalization guardrails ("Red Flags"), and complete tool autonomy.
    fn enrich_minipower_rules_and_freedom(_workspace_root: &Path, _prompt: &str) -> String {
        let mut out = String::from("<minipower_autonomous_engineering_rules>\n");
        out.push_str("  MINIPOWER CORE METHODOLOGY & AGENT FREEDOM CONTRACT:\n");
        out.push_str("  1. The 6 Engineering Pillars:\n");
        out.push_str("     • Socratic Brainstorming: Clarify intent, surface trade-offs, and chunk specs before touching code (`power_brainstorm` or `ask_user`).\n");
        out.push_str("     • Git Worktree Isolation: Protect the main branch by running complex tasks in isolated branches (`power_worktree_task`).\n");
        out.push_str("     • Bite-Sized Planning: Atomic 2-5 min tasks with concrete acceptance tests (`power_plan` or `create_plan` + `todo.md`).\n");
        out.push_str("     • Two-Stage Subagent Review: Automated Stage 1 (Spec Compliance) and Stage 2 (Code Quality) (`power_review`).\n");
        out.push_str("     • Strict Red/Green TDD: Write failing tests first, make them green, refactor cleanly (`exec_cmd`).\n");
        out.push_str("     • Evidence Before Assertions: Zero unverified claims; verified by exit code 0 (`power_verify`).\n\n");

        out.push_str("  2. Anti-Rationalization Guardrails (\"Red Flags\" Reality Table):\n");
        for (excuse, reality) in
            crate::agent::minipower::MiniPowerEngine::anti_rationalization_table()
        {
            out.push_str(&format!("     • \"{}\" ➔ {}\n", excuse, reality));
        }

        out.push_str("\n  3. 4-Gate Pre-Completion Verification Barrier:\n");
        out.push_str(
            "     Never declare a feature or phase complete without satisfying all 4 gates:\n",
        );
        out.push_str("     • Gate 1 (Compiler & Syntax): Clean build with exit code 0 (e.g. `cargo check`, `tsc --noEmit`).\n");
        out.push_str("     • Gate 2 (Test Suite & Regression): Running tests pass (e.g. `cargo test`, `npm test`, `pytest`).\n");
        out.push_str("     • Gate 3 (Structural Integrity): Zero merge conflict markers (`<<<<<<<`), no dead imports or broken links.\n");
        out.push_str("     • Gate 4 (Diff & Secret Protection): Zero leaked API keys, tokens, or credentials in git diff.\n");
        out.push_str(
            "     Invoke `power_verify` or run checks via `exec_cmd` before claiming success!\n\n",
        );

        out.push_str("  4. Tool Freedom & Ecosystem Synergy (202 Native Tools Available):\n");
        out.push_str("     You have full freedom to choose and combine the highest-leverage tools for any task:\n");
        out.push_str("     • MiniKit: `kit_stack_add` (scaffold templates), `kit_add` (install dependencies), `kit_sync` (reconcile manifest).\n");
        out.push_str("     • File System Safety: `write_file(path, content, overwrite=true, append=true)` — Safe-overwrite guard active. Set `overwrite=true` to replace or `append=true` for chunked writes. For large multi-section files (>250 lines), use modular files, `append=true`, or scripted builder assembly (`python3 scripts/build.py` or `cat << 'EOF' >> file` via `exec_cmd`).\n");
        out.push_str("     • MiniBlocks: `block_search`, `block_palettes`, `block_scaffold` — NEVER write 1,000+ line monolithic CSS/JS files! Decompose into modular components (<250 lines per file) to prevent JSON token truncation (`EOF while parsing a string`).\n");
        out.push_str("     • MiniTask Vault: `minitask(action=\"start\")` to run dev servers/daemons, `minitask(action=\"status\")` / `minitask(action=\"resources\")` for telemetry, `minitask(action=\"stop\")` for teardown.\n");
        out.push_str("     • Browser Automation: `browser_navigate`, `browser_screenshot`, `browser_snapshot`, `browser_close`. Inspect the returned `[DOM & Visual Health Observation]` telemetry (stylesheets count, computed font, serif detection) to verify visual rendering.\n");
        out.push_str("     • CodeGraph AST: `code_explore`, `blast_radius`, `locate_symbol`, `diff_impact` for architectural navigation.\n");
        out.push_str("     • Working Memory: `create_plan`, `update_progress` — call `update_progress` after each step so the user and live TUI stay in sync.\n");
        out.push_str("     • MiniPower Execution: `power_status`, `power_brainstorm`, `power_plan`, `power_review`, `power_verify`, `power_worktree_task`.\n");
        out.push_str("     • Interactive Inquiry & Socratic Inception: `ask_user` — When initiating features, apps, or UI designs, make Step 1 of your plan an interactive consultation via `ask_user` to align on user preferences, feature priorities, and aesthetic nuances before writing code.\n");

        out.push_str("</minipower_autonomous_engineering_rules>");
        out
    }

    #[allow(dead_code)]
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

        // Extract meaningful candidate tokens from the prompt (length >= 3, skipping common noise/action words)
        let stop_words: std::collections::HashSet<&'static str> = [
            "the",
            "and",
            "for",
            "with",
            "from",
            "create",
            "make",
            "build",
            "style",
            "page",
            "modern",
            "responsive",
            "using",
            "into",
            "this",
            "that",
            "user",
            "app",
            "application",
            "mode",
            "dark",
            "light",
            "give",
            "detailed",
            "plan",
        ]
        .into_iter()
        .collect();

        let prompt_tokens: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .map(|s| s.trim())
            .filter(|s| s.len() >= 3 && !stop_words.contains(s))
            .collect();

        // Query components for the most salient tokens or overall prompt
        let mut components = Vec::new();
        let mut matched_queries = Vec::new();

        for token in &prompt_tokens {
            let filter = crate::blocks::BlockSearchFilter {
                query: Some((*token).to_string()),
                limit: 3,
                ..Default::default()
            };
            let results = store.search_components(&filter);
            if !results.is_empty() {
                matched_queries.push(*token);
                for r in results {
                    if !components.iter().any(
                        |existing: &crate::blocks::store::BlockSearchResult| existing.id == r.id,
                    ) {
                        components.push(r);
                    }
                    if components.len() >= 4 {
                        break;
                    }
                }
            }
            if components.len() >= 4 {
                break;
            }
        }

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
        out.push_str("  • Interactive Clarification First (`ask_user`):\n");
        out.push_str("    If the user prompt is broad, open-ended, or has multiple design directions or tech stacks (e.g. 'Build a web dashboard', 'Create an application', 'Add an analytics panel'):\n");
        out.push_str("    YOU MUST CALL `ask_user` ON TURN 1 to confirm tech stack, color theme, and key sections before writing code!\n");
        out.push_str("  • Instant Starter Stack Scaffolding (`kit_stack_add`):\n");
        out.push_str("    To build a modern frontend, use `kit_stack_add` (e.g. `kit_stack_add(stack_name=\"react-vite-gsap\")` or `kit_stack_add(stack_name=\"static-website\")`) to scaffold the full project in 1 call rather than hand-authoring files.\n");
        out.push_str("  • Strict Modular Code Architecture Contract (Anti-Monolith Invariant):\n");
        out.push_str("    NEVER write massive monolithic files (>250-300 lines or >10KB) in a single tool call to avoid token truncation and JSON EOF errors.\n");
        out.push_str(
            "    Decompose frontend applications into separate modular files from turn 1:\n",
        );
        out.push_str("    • `index.html`: Semantic HTML skeleton only (<150 lines), linking to `styles.css` and `app.js`. NO massive inline <style> or <script> tags!\n");
        out.push_str("    • `styles.css`: CSS variables, design tokens, typography, responsive layout (<250 lines).\n");
        out.push_str("    • Modular JS: Split into focused modules (`app.js`, `components.js`, `api.js`, `theme.js`, <150 lines each).\n");

        let core_dir = crate::tools::minikit::resolve_core_docs_dir(workspace_root);
        if !core_dir.join("todo.md").exists() && !workspace_root.join("todo.md").exists() {
            out.push_str("  • Task Planning Invariant:\n");
            out.push_str("    When embarking on a new feature or multi-step work, use `create_plan` to structure tasks into `todo.md` with verification checks before modifying code.\n");
        }

        out.push_str("  • UI Component Warehouse Invariant:\n");
        out.push_str("    ALWAYS query MiniBlocks (`block_search`, `block_palettes`) BEFORE creating UI components or themes from scratch.\n");
        out.push_str("    Use `block_insert` or `block_scaffold` to insert verified, production-grade components into the project rather than hand-coding JSX/TSX from scratch.\n");

        out.push_str("  • Development Server Invariant:\n");
        out.push_str("    To run and test the web application, use `minitask(action=\"start\")` or `exec_cmd`. Do NOT launch multiple competing servers. Cleanly close test servers and browser sessions when finished.\n");

        out.push_str("</recommended_miniblocks>");
        Some(out)
    }

    fn enrich_minikit_scaffolding(_workspace_root: &Path, _prompt: &str) -> Option<String> {
        let mut out = String::from("<minikit_scaffolding_guidance>\n");
        out.push_str("  MiniKit Architecture, Starter Stacks & Dependency Engine Activated:\n");
        out.push_str("  1. Interactive Clarification First (`ask_user`):\n");
        out.push_str("     If user requirements or framework preferences are open-ended (e.g. choice of React vs Next.js vs static HTML vs FastAPI), call `ask_user` on Turn 1 to confirm before creating files.\n");
        out.push_str("  2. Instant Starter Stacks (`kit_stack_add`):\n");
        out.push_str("     Never manually author 15 boilerplate files from scratch! Inspect available stacks with `kit_stack_list`:\n");
        let stacks = crate::tools::minikit::stacks::builtin::builtin_stacks();
        for s in &stacks {
            out.push_str(&format!(
                "     • `{}` ({}): {}\n",
                s.name, s.runtime, s.description
            ));
        }
        out.push_str("     • Remote GitHub stacks: `gh:owner/repo` (e.g. `gh:shadcn-ui/ui`)\n");
        out.push_str("     Run `kit_stack_add(stack_name=\"...\")` to scaffold with automated dependency installation in 1 tool call.\n");
        out.push_str("  3. Safe Dependency Management (`kit_info` + `kit_add`):\n");
        out.push_str("     When adding libraries (e.g. `zod`, `gsap`, `axum`), call `kit_info(name)` to inspect packages and `kit_add(name, is_dev)` to update manifests safely without breaking lockfiles.\n");
        out.push_str("  4. Strict Modular Code Architecture Contract:\n");
        out.push_str("     Write modular, cleanly scoped files under 250-300 lines rather than giant single files to prevent JSON EOF parsing cutoffs. Strictly separate HTML, CSS, and JS.\n");
        out.push_str("</minikit_scaffolding_guidance>");
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
                        format!(
                            " [Timer: {}s, Runs: {}]",
                            sched.interval_secs, sched.iteration_count
                        )
                    } else {
                        let max_str = sched
                            .max_iterations
                            .map(|m| format!("/{}", m))
                            .unwrap_or_default();
                        format!(
                            " [Schedule: Every {}s, Runs: {}{}]",
                            sched.interval_secs, sched.iteration_count, max_str
                        )
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
                    p.id,
                    p.name,
                    pid_str,
                    p.status,
                    port_str,
                    sched_str,
                    p.memory_rss_mb,
                    p.cpu_percent
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
        out.push_str("  • Interactive Clarification First (`ask_user`): If user requirements, architecture, or tech stack choices are underspecified, call `ask_user` on Turn 1 before writing code.\n");
        out.push_str(&format!(
            "  1. Gate 1 (Intent Anchor): Define the goal and bite-sized milestones. Record progress in `{}/core/todo.md` using `create_plan` or direct file edits.\n",
            docs_name
        ));
        out.push_str(&format!(
            "  2. Gate 2 (Architecture & Discovery): Consult relevant specs in `{}/core/` and verify dependencies before making changes. Use `kit_stack_add` for project scaffolding and `kit_add` for packages.\n",
            docs_name
        ));
        out.push_str("  3. Gate 3 (TDD Implementation): Write or update tests FIRST. Verify failure (Red), then implement minimal code, then verify green.\n");
        out.push_str("  4. Gate 4 (Verification Barrier): Execute compiler/test checks (`cargo test -j 1 ...`, `npm test`, etc.) to confirm 0 errors before concluding.\n");
        out.push_str("  5. Anti-Monolith Invariant: Write modular, cleanly scoped files under 250-300 lines rather than giant single files to prevent JSON EOF parsing cutoffs. Strictly separate HTML, CSS, and JS.\n");
        out.push_str(
            "  6. Command Invariant: Run all builds, tests, and dev servers with `exec_cmd` or `minitask`.\n",
        );

        let core_dir = crate::tools::minikit::resolve_core_docs_dir(workspace_root);
        if !core_dir.join("todo.md").exists() && !workspace_root.join("todo.md").exists() {
            out.push_str("  • Task Planning: For multi-step tasks or new features, use `create_plan` to anchor bite-sized milestones before creating source files.\n");
        }
        out.push_str("  • UI Invariant: For frontend/UI tasks, query MiniBlocks (`block_search`, `block_palettes`) before writing components from scratch.\n");
        out.push_str("  • Plan Progress Invariant: Update tasks via `update_progress` as you complete each milestone to keep the Live Execution Plan in sync.\n");
        out.push_str("  • Layout Anti-Collision Invariant: Starter stacks (e.g. `react-vite-gsap`, `next-template`) wrap pages inside a root layout (e.g. `src/layouts/RootLayout.tsx`) that already renders `<Navbar />` and `<Footer />`. Inspect `RootLayout.tsx` first! NEVER duplicate `<Navigation />` or `<Footer />` inside page components (`HomePage.tsx`).\n");
        out.push_str("  • Brand Icons Invariant: Modern `lucide-react` does NOT export brand icons (`Github`, `Twitter`, `Discord`). Always render brand icons as inline `<svg>` elements or use standard generic icons (`Code`, `Globe`, `Share2`).\n");
        out.push_str("  • Smooth Scroll Flow: When combining GSAP ScrollTrigger with smooth scroll (Lenis), avoid hardcoded rigid heights (`h-screen`, `min-h-screen`) across multiple sequential sections without flow spacing. Let natural content flow govern section heights.\n");

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

    /// Dynamically discovers and extracts relevant project specifications from `.md` documentation files
    /// in the workspace (`minikit_docs/core/`, `onpkg_docs/core/`, `docs/`, etc.) without hardcoding names.
    fn enrich_core_documentation(workspace_root: &Path, prompt: &str) -> Option<String> {
        let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_root);
        let mut candidate_dirs: Vec<PathBuf> = Vec::new();
        let core_dir = docs_dir.join("core");
        if core_dir.is_dir() {
            candidate_dirs.push(core_dir);
        }
        if docs_dir.is_dir() && docs_dir != workspace_root {
            candidate_dirs.push(docs_dir);
        }
        let root_docs = workspace_root.join("docs");
        if root_docs.is_dir() {
            candidate_dirs.push(root_docs);
        }

        if candidate_dirs.is_empty() {
            return None;
        }

        let mut discovered_docs: Vec<(String, String, String, usize)> = Vec::new(); // (filename, title, excerpt, score)
        let lower_prompt = prompt.to_ascii_lowercase();
        let prompt_tokens: Vec<&str> = lower_prompt
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .map(|s| s.trim())
            .filter(|s| s.len() >= 3)
            .collect();

        for dir in candidate_dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if !name.ends_with(".md")
                    || name.eq_ignore_ascii_case("todo.md")
                    || name.eq_ignore_ascii_case("notes.md")
                {
                    continue;
                }

                // Avoid duplicates across multiple dirs
                if discovered_docs
                    .iter()
                    .any(|(n, _, _, _)| n.eq_ignore_ascii_case(name))
                {
                    continue;
                }

                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if content.trim().is_empty() || content.len() > 150_000 {
                    continue;
                }

                let title = content
                    .lines()
                    .find(|l| l.trim_start().starts_with('#'))
                    .map(|l| l.trim().trim_start_matches('#').trim().to_string())
                    .unwrap_or_else(|| name.to_string());

                let excerpt_lines: Vec<&str> = content.lines().take(25).collect();
                let excerpt = excerpt_lines.join("\n");

                let name_stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();

                let mut score = 0;
                for token in &prompt_tokens {
                    if name_stem.contains(token) {
                        score += 10;
                    }
                    if title.to_ascii_lowercase().contains(token) {
                        score += 5;
                    }
                    if content[..content.len().min(2000)]
                        .to_ascii_lowercase()
                        .contains(token)
                    {
                        score += 1;
                    }
                }

                discovered_docs.push((name.to_string(), title, excerpt, score));
            }
        }

        if discovered_docs.is_empty() {
            return None;
        }

        discovered_docs.sort_by(|a, b| b.3.cmp(&a.3));

        let mut out = String::from("<project_core_specifications>\n");
        out.push_str("  Available Project Documentation Specifications:\n");
        for (name, title, _, score) in discovered_docs.iter().take(6) {
            let score_note = if *score > 0 { " [matches prompt]" } else { "" };
            out.push_str(&format!("  • `{}`: {}{}\n", name, title, score_note));
        }

        if let Some((top_name, top_title, top_excerpt, top_score)) = discovered_docs.first() {
            if *top_score > 0 || discovered_docs.len() <= 3 {
                out.push_str(&format!(
                    "\n  Grounding Excerpt from `{}` ({}):\n",
                    top_name, top_title
                ));
                for line in top_excerpt.lines().take(20) {
                    out.push_str(&format!("    {}\n", line));
                }
            }
        }

        out.push_str("  Directive: Ground implementations, architecture, and constraints in these core specifications.\n");
        out.push_str("</project_core_specifications>");
        Some(out)
    }

    /// Hermes Agent progressive procedural knowledge activation:
    /// Dynamically scans available built-in and MiniVault skills, matches them against the prompt
    /// and project stack, and injects procedural rules and traps into context.
    fn enrich_hermes_skills(workspace_root: &Path, prompt: &str) -> Option<String> {
        let lower_prompt = prompt.to_ascii_lowercase();

        let builtins = crate::tools::minikit::builtin_skills::get_all_builtin_skills();
        let store = crate::vault::store::VaultStore::new(workspace_root);
        let vault_skills = store.list_all_skills();
        let stack_info = crate::blocks::seed::ProjectStackInfo::detect(workspace_root);

        let mut matched_skills: Vec<(String, String, String, usize)> = Vec::new();

        for b in builtins {
            let mut score = 0;
            let b_name_lower = b.name.to_ascii_lowercase();
            let b_desc_lower = b.description.to_ascii_lowercase();

            if crate::utils::has_word(&lower_prompt, &b_name_lower) {
                score += 15;
            }

            for glob in b.globs {
                let clean_glob = glob.trim_start_matches("*.").to_ascii_lowercase();
                if lower_prompt.contains(&clean_glob) {
                    score += 5;
                }
            }

            for word in &[
                "design",
                "ui",
                "ux",
                "frontend",
                "animation",
                "react",
                "next",
                "api",
                "fastapi",
                "rust",
                "style",
                "theme",
                "layout",
                "component",
            ] {
                if crate::utils::has_word(&lower_prompt, word)
                    && (b_name_lower.contains(word) || b_desc_lower.contains(word))
                {
                    score += 5;
                }
            }

            let has_react = stack_info.framework == Some(crate::blocks::BlockFramework::React);
            if (b_name_lower.contains("react") && has_react)
                || (b_name_lower.contains("tailwind") && stack_info.has_tailwind)
            {
                score += 2;
            }

            if score >= 5 {
                let excerpt_lines: Vec<&str> = b.content.lines().take(25).collect();
                matched_skills.push((
                    b.name.to_string(),
                    b.description.to_string(),
                    excerpt_lines.join("\n"),
                    score,
                ));
            }
        }

        for s in vault_skills {
            if s.is_active_in_project {
                continue;
            }
            let mut score = 0;
            let s_name_lower = s.name.to_ascii_lowercase();
            let s_desc_lower = s.description.to_ascii_lowercase();

            if crate::utils::has_word(&lower_prompt, &s_name_lower)
                || crate::utils::has_word(&lower_prompt, &s_desc_lower)
            {
                score += 15;
            }

            for trigger in &s.frontmatter.triggers {
                let trig_lower = trigger.to_ascii_lowercase();
                if lower_prompt.contains(&trig_lower) {
                    score += 8;
                }
            }

            if score >= 5 {
                let excerpt_lines: Vec<&str> = s.instructions.lines().take(25).collect();
                matched_skills.push((s.name, s.description, excerpt_lines.join("\n"), score));
            }
        }

        if matched_skills.is_empty() {
            return None;
        }

        matched_skills.sort_by(|a, b| b.3.cmp(&a.3));

        let mut out = String::from("<auto_activated_skills>\n");
        out.push_str("  Hermes Progressive Skill Intelligence Activated:\n");
        out.push_str(
            "  The following domain skills automatically matched your prompt and project stack:\n",
        );

        for (name, desc, excerpt, _) in matched_skills.iter().take(2) {
            out.push_str(&format!("  • Skill `{}`: {}\n", name, desc));
            out.push_str("    Key Guidelines & Traps:\n");
            for line in excerpt.lines().take(20) {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("---") {
                    out.push_str(&format!("      {}\n", trimmed));
                }
            }
        }
        out.push_str(
            "  Directive: Adhere strictly to these procedural skill guidelines for this task.\n",
        );
        out.push_str("</auto_activated_skills>");
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
        assert_eq!(
            WorkflowRouter::classify("make an website for syed and give detailed plan"),
            WorkflowArchetype::MultiPhaseEngineering
        );
        assert_eq!(
            WorkflowRouter::classify("create a portfolio website for my projects"),
            WorkflowArchetype::MultiPhaseEngineering
        );
        assert_eq!(
            WorkflowRouter::classify("build a website with vite and react"),
            WorkflowArchetype::MultiPhaseEngineering
        );
    }

    #[test]
    fn test_workflow_router_classify_minikit_scaffolding() {
        assert_eq!(
            WorkflowRouter::classify("Scaffold a new react-vite-gsap stack"),
            WorkflowArchetype::MiniKitScaffolding
        );
        assert_eq!(
            WorkflowRouter::classify("Add dependency zod using kit"),
            WorkflowArchetype::MiniKitScaffolding
        );
        assert_eq!(
            WorkflowRouter::classify("Check stack template diff and self-heal drift"),
            WorkflowArchetype::MiniKitScaffolding
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

        let enrichment_kit = WorkflowRouter::enrich_context(
            temp.path(),
            "Scaffold react-vite stack",
            WorkflowArchetype::MiniKitScaffolding,
        )
        .await;
        let text_kit = enrichment_kit.unwrap();
        assert!(text_kit.contains("<minikit_scaffolding_guidance>"));
    }

    #[tokio::test]
    async fn test_workflow_router_enrich_plan_status() {
        let temp = tempfile::tempdir().unwrap();
        let wm = crate::context::memory::working_memory::WorkingMemory::new(temp.path());
        wm.init_plan(
            "Test Feature",
            &[
                "Step 1: Create auth.rs".to_string(),
                "Step 2: Create routes.rs".to_string(),
                "Step 3: Run cargo test".to_string(),
            ],
        )
        .unwrap();

        // Initially 0/3 completed, step 1 in progress
        let enrichment = WorkflowRouter::enrich_context(
            temp.path(),
            "What is next?",
            WorkflowArchetype::Standard,
        )
        .await;
        assert!(enrichment.is_some());
        let text = enrichment.unwrap();
        assert!(text.contains("<active_task_plan_status>"));
        assert!(text.contains("0/3 tasks completed"));
        assert!(text.contains("[>] Step 1: Create auth.rs"));

        // Mark step 1 completed
        wm.update_progress("1", "completed").unwrap();
        let enrichment2 =
            WorkflowRouter::enrich_context(temp.path(), "Continue", WorkflowArchetype::Standard)
                .await;
        assert!(enrichment2.is_some());
        let text2 = enrichment2.unwrap();
        assert!(text2.contains("1/3 tasks completed"));
        assert!(text2.contains("[x] Step 1: Create auth.rs"));
        assert!(text2.contains("[>] Step 2: Create routes.rs"));
    }

    #[tokio::test]
    async fn test_workflow_router_enrich_core_docs() {
        let temp = tempfile::tempdir().unwrap();
        let core_dir = temp.path().join("minikit_docs").join("core");
        std::fs::create_dir_all(&core_dir).unwrap();
        std::fs::write(
            core_dir.join("architecture.md"),
            "# System Architecture Specification\nStrict modular boundaries with zero circular dependencies.\n",
        )
        .unwrap();

        let enrichment = WorkflowRouter::enrich_context(
            temp.path(),
            "Review architecture and modular boundaries",
            WorkflowArchetype::Standard,
        )
        .await;
        assert!(enrichment.is_some());
        let text = enrichment.unwrap();
        assert!(text.contains("<project_core_specifications>"));
        assert!(text.contains("architecture.md"));
        assert!(text.contains("System Architecture Specification"));
    }

    #[tokio::test]
    async fn test_workflow_router_enrich_hermes_skills() {
        let temp = tempfile::tempdir().unwrap();
        let enrichment = WorkflowRouter::enrich_context(
            temp.path(),
            "Create modern ui design with responsive css layout",
            WorkflowArchetype::Standard,
        )
        .await;
        assert!(enrichment.is_some());
        let text = enrichment.unwrap();
        assert!(text.contains("<auto_activated_skills>"));
        assert!(text.contains("Hermes Progressive Skill Intelligence Activated"));
    }

    #[test]
    fn test_scan_repository_state_fresh_docs_codebase() {
        let temp_fresh = tempfile::tempdir().unwrap();
        let (state_fresh, _, _) = WorkflowRouter::scan_repository_state(temp_fresh.path());
        assert_eq!(state_fresh, RepoState::FreshWorkspace);

        let temp_docs = tempfile::tempdir().unwrap();
        std::fs::write(temp_docs.path().join("README.md"), "# Hello").unwrap();
        let (state_docs, _, doc_files) = WorkflowRouter::scan_repository_state(temp_docs.path());
        assert_eq!(state_docs, RepoState::DocsOnly);
        assert_eq!(doc_files.len(), 1);

        let temp_code = tempfile::tempdir().unwrap();
        let src = temp_code.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        let (state_code, code_files, _) = WorkflowRouter::scan_repository_state(temp_code.path());
        assert_eq!(state_code, RepoState::ExistingCodebase);
        assert_eq!(code_files.len(), 1);
    }

    #[tokio::test]
    async fn test_enrich_orchestrator_guidance_fresh_vs_existing() {
        let temp = tempfile::tempdir().unwrap();

        // 1. Fresh repo with underspecified prompt -> requires ask_user
        let guidance_underspec = WorkflowRouter::enrich_orchestrator_guidance(
            temp.path(),
            "build a website",
            RepoState::FreshWorkspace,
        );
        assert!(guidance_underspec.contains("Autonomous Inception & Specification Architecture"));
        assert!(guidance_underspec.contains("Information Completeness: UNDERSPECIFIED"));
        assert!(guidance_underspec.contains("YOU MUST CALL `ask_user` ON TURN 1"));

        // 2. Fresh repo with rich prompt -> proceeds with spec synthesis directly
        let guidance_rich = WorkflowRouter::enrich_orchestrator_guidance(
            temp.path(),
            "Create a modern dark futuristic landing page with react and vite including hero section, swarm dag, council review, and 150+ developer tools",
            RepoState::FreshWorkspace,
        );
        assert!(
            guidance_rich.contains("Information Completeness: HIGH (Rich Specification Provided)")
        );
        assert!(guidance_rich.contains("Synthesize detailed `.md` core files directly"));

        // 3. Fresh repo with long descriptive product prompt but no explicit target build stack -> requires ask_user
        let guidance_long_no_stack = WorkflowRouter::enrich_orchestrator_guidance(
            temp.path(),
            "Create a premium, modern landing page for minicode, an autonomous AI coding agent built for developers who want fast, reliable, secure software development. The overall design should feel cutting-edge, technical, minimal, and futuristic.",
            RepoState::FreshWorkspace,
        );
        assert!(guidance_long_no_stack.contains("Information Completeness: UNDERSPECIFIED"));
        assert!(guidance_long_no_stack.contains("YOU MUST CALL `ask_user` ON TURN 1"));

        // 4. Existing codebase -> maintenance & grounding
        let guidance_existing = WorkflowRouter::enrich_orchestrator_guidance(
            temp.path(),
            "refactor auth module",
            RepoState::ExistingCodebase,
        );
        assert!(guidance_existing.contains("Existing Codebase Engineering Architecture"));
        assert!(guidance_existing.contains("Inspect existing architecture"));
    }

    #[tokio::test]
    async fn test_enrich_minipower_rules_and_freedom() {
        let temp = tempfile::tempdir().unwrap();
        let enrichment = WorkflowRouter::enrich_context(
            temp.path(),
            "Build fullstack web application",
            WorkflowArchetype::Standard,
        )
        .await;
        assert!(enrichment.is_some());
        let text = enrichment.unwrap();
        assert!(text.contains("<minipower_autonomous_engineering_rules>"));
        assert!(text.contains("MINIPOWER CORE METHODOLOGY & AGENT FREEDOM CONTRACT"));
        assert!(text.contains("The 6 Engineering Pillars"));
        assert!(text.contains("Anti-Rationalization Guardrails"));
        assert!(text.contains("4-Gate Pre-Completion Verification Barrier"));
        assert!(text.contains("Tool Freedom & Ecosystem Synergy (202 Native Tools Available)"));
    }
}
