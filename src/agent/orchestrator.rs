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
pub(crate) fn has_any_word(text: &str, words: &[&str]) -> bool {
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

        // 0. Dynamic Repository State & File Discovery (purely filesystem-derived).
        // Guidance below depends only on observable workspace state, never on
        // keyword matching against the user's prompt. Static methodology rules live
        // once in STATIC_SYSTEM_PROMPT; the active plan is injected once by
        // PromptBuilder::build_recency_context (<minipower_active_plan>).
        let (repo_state, code_files, doc_files) = Self::scan_repository_state(workspace_root);
        sections.push(Self::enrich_repository_state(
            workspace_root,
            repo_state,
            &code_files,
            &doc_files,
        ));
        sections.push(Self::enrich_orchestrator_guidance(
            workspace_root,
            repo_state,
        ));

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

        let state = if !code_files.is_empty() || Self::project_manifest(workspace_root).is_some() {
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

    /// Injects workflow guidance derived purely from observable workspace state.
    ///
    /// No keyword matching against the user's prompt happens here: whether the
    /// request already settles the foundation (stack, visual direction) is a
    /// judgement the model makes by reading the request, guided by the
    /// Reversibility Rule in `STATIC_SYSTEM_PROMPT`.
    pub fn enrich_orchestrator_guidance(workspace_root: &Path, state: RepoState) -> String {
        let mut out = String::from("<orchestrator_dynamic_guidance>\n");
        match state {
            RepoState::FreshWorkspace | RepoState::DocsOnly => {
                out.push_str("  Foundation: NOT ESTABLISHED (no project manifest such as package.json, Cargo.toml, pyproject.toml, go.mod or index.html exists yet).\n");
                if state == RepoState::DocsOnly {
                    out.push_str("  The documentation files listed above may already state the stack or design; read them before deciding.\n");
                }
                if crate::agent::decisions::read_for_prompt(workspace_root).is_some() {
                    out.push_str("  Recorded user decisions exist (see <user_decisions>): build on them and do not re-ask settled questions. Ask only about decisions still open.\n");
                }
                out.push_str(
                    "  Resolve the foundation before writing project files (Reversibility Rule):\n",
                );
                out.push_str("  1. Identify which foundation decisions the request (and any docs) actually state: framework/stack, architecture, visual direction for UI work, scope, and any content/data only the user has. A long or detailed request is not a decision; a named stack is.\n");
                out.push_str("  2. Unstated decisions that are expensive to reverse once files exist belong to the user: ask them all in ONE `ask_user` call, recommended option first.\n");
                out.push_str("  3. If every foundation decision is stated, or the deliverable is trivially a single file (one script, one standalone page), do not ask: state your assumption in one sentence and continue.\n");
                out.push_str("  Then build on it:\n");
                out.push_str("  4. Scaffold with `kit_stack_add` into the workspace root (`kit_stack_list` shows stacks; `static-website` covers plain HTML/CSS/JS) instead of hand-writing boilerplate.\n");
                out.push_str("  5. Multi-step work: `create_plan` with 4-8 verifiable steps, then `update_progress` as each step finishes.\n");
                out.push_str("  6. UI work: choose tokens with `block_palettes` and components with `block_search` before writing CSS or markup from scratch.\n");
                out.push_str("  7. Larger projects: after the foundation is decided, record specs in `minikit_docs/core/` (prd.md, design.md, architecture.md) if they help.\n");
                out.push_str("  8. Verify by running it (`minitask` start or `exec_cmd`) and inspecting the result (`browser_navigate`, tests).\n");
            }
            RepoState::ExistingCodebase => {
                if let Some(manifest) = Self::project_manifest(workspace_root) {
                    out.push_str(&format!(
                        "  Foundation: ESTABLISHED (`{}`). Do not re-scaffold; extend the existing project.\n",
                        manifest
                    ));
                }
                out.push_str("  1. Ground first: inspect architecture, imports and types (`locate_symbol`, `grep_search`, `read_file`) before writing code.\n");
                out.push_str("  Before your first edit, check: which decisions does this change make that the request does not state (storage or data model, architecture, API shape, visual direction, content)? Any that is costly to undo or a matter of the user's taste is theirs: ask in one `ask_user` round. Only cheap, reversible details may be assumed, and say so.\n");
                out.push_str("  2. Respect existing patterns, styling and conventions; make surgical changes.\n");
                out.push_str("  3. Multi-step work: `create_plan`, then `update_progress` as steps finish.\n");
                out.push_str("  4. Dependencies: use the project's package manager via `exec_cmd`. UI components: `block_search`.\n");
                out.push_str("  5. Decisions the code does not settle and that are costly to undo still belong to the user (`ask_user`, one batched round): new architecture or data model, API/spec changes, design direction, new heavy dependencies, destructive changes, missing data only they have. For large multi-step work, get the approach or plan approved before the first edit.\n");
            }
        }
        out.push_str("</orchestrator_dynamic_guidance>");
        out
    }

    /// Returns the first project manifest found at the workspace root, if any.
    ///
    /// A manifest marks an established project foundation (scaffolded or
    /// hand-initialized), independent of how many source files exist yet.
    pub(crate) fn project_manifest(workspace_root: &Path) -> Option<&'static str> {
        const MANIFESTS: &[&str] = &[
            "package.json",
            "Cargo.toml",
            "pyproject.toml",
            "requirements.txt",
            "go.mod",
            "deno.json",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "Gemfile",
            "composer.json",
            "pubspec.yaml",
            "mix.exs",
            "CMakeLists.txt",
            "Makefile",
            "minikit.json",
            "index.html",
        ];
        MANIFESTS
            .iter()
            .copied()
            .find(|m| workspace_root.join(m).is_file())
    }

    fn enrich_ui_design(_workspace_root: &Path, prompt: &str) -> Option<String> {
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

        // Palette retrieval: query the palette store with the prompt's own salient
        // tokens (same retrieval used for components above) rather than a fixed
        // list of theme words. Palettes are suggestions; the model decides.
        let mut palettes = Vec::new();
        for token in &prompt_tokens {
            for p in store.search_palettes(token) {
                if !palettes
                    .iter()
                    .any(|existing: &&crate::blocks::BlockPalette| existing.name == p.name)
                {
                    palettes.push(p);
                }
            }
            if palettes.len() >= 2 {
                break;
            }
        }

        let mut out = String::from("<recommended_miniblocks>\n");
        out.push_str(
            "  MiniBlocks warehouse matches for this request (retrieved, not mandatory):\n",
        );

        if !components.is_empty() {
            out.push_str("  Components:\n");
            for c in components.iter().take(3) {
                out.push_str(&format!(
                    "     • `{}` ({}, {:?}): {}\n",
                    c.name, c.category, c.framework, c.description
                ));
            }
            if matched_queries.len() > 1 {
                out.push_str(&format!(
                    "     Other matched terms: {}. Query them with `block_search`.\n",
                    matched_queries[1..].join(", ")
                ));
            }
            out.push_str("     Use `block_get` for source, or `block_insert` / `block_scaffold` to place them.\n");
        } else {
            out.push_str("  Components: none matched directly; try `block_search` with a section name (hero, navbar, pricing, footer).\n");
        }

        if !palettes.is_empty() {
            out.push_str("  Palettes:\n");
            for p in palettes.iter().take(2) {
                out.push_str(&format!(
                    "     • `{}`: --bg: {}; --surface: {}; --accent: {}; --text: {};\n",
                    p.name, p.colors[0], p.colors[1], p.colors[2], p.colors[3]
                ));
            }
        } else {
            out.push_str("  Palettes: none matched directly; browse with `block_palettes`.\n");
        }

        out.push_str("</recommended_miniblocks>");
        Some(out)
    }

    fn enrich_minikit_scaffolding(_workspace_root: &Path, _prompt: &str) -> Option<String> {
        let mut out = String::from("<minikit_scaffolding_guidance>\n");
        out.push_str("  Available starter stacks (`kit_stack_add(stack_name=...)` scaffolds one into the workspace root in a single call):\n");
        let stacks = crate::tools::minikit::stacks::builtin::builtin_stacks();
        for s in &stacks {
            out.push_str(&format!(
                "     • `{}` ({}): {}\n",
                s.name, s.runtime, s.description
            ));
        }
        out.push_str("     • Remote GitHub stacks: `gh:owner/repo`\n");
        out.push_str("  After scaffolding, add libraries with the project's package manager via `exec_cmd`; MiniKit is for templates only.\n");
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
        out.push_str(&format!(
            "  1. Gate 1 (Intent Anchor): Define the goal and bite-sized milestones. Record progress in `{}/core/todo.md` using `create_plan` or direct file edits.\n",
            docs_name
        ));
        out.push_str(&format!(
            "  2. Gate 2 (Architecture & Discovery): Consult relevant specs in `{}/core/` and verify dependencies before making changes. Use `kit_stack_add` for project scaffolding and native package managers via `exec_cmd` for packages.\n",
            docs_name
        ));
        out.push_str("  3. Gate 3 (TDD Implementation): Write or update tests FIRST. Verify failure (Red), then implement minimal code, then verify green.\n");
        out.push_str("  4. Gate 4 (Verification Barrier): Execute compiler/test checks (`cargo test -j 1 ...`, `npm test`, etc.) to confirm 0 errors before concluding.\n");
        out.push_str(
            "  5. Command Invariant: Run all builds, tests, and dev servers with `exec_cmd` or `minitask`.\n",
        );

        out.push_str("  Stack-specific pitfalls (layout wrappers, lucide brand icons, GSAP scroll flow) are covered in the Self-Healing protocol.\n");

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
                    if crate::utils::strings::truncate_chars(&content, 2000)
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

    #[test]
    fn test_enrich_orchestrator_guidance_is_state_based() {
        let temp = tempfile::tempdir().unwrap();

        // Fresh workspace: foundation not established, reversibility guidance, no keyword verdicts.
        let fresh =
            WorkflowRouter::enrich_orchestrator_guidance(temp.path(), RepoState::FreshWorkspace);
        assert!(fresh.contains("Foundation: NOT ESTABLISHED"));
        assert!(fresh.contains("`ask_user`"));
        assert!(fresh.contains("`kit_stack_add`"));
        assert!(fresh.contains("A long or detailed request is not a decision"));
        assert!(!fresh.contains("full autonomy"));
        assert!(!fresh.contains("MUST"));

        // Docs-only workspace additionally points at the existing docs.
        let docs = WorkflowRouter::enrich_orchestrator_guidance(temp.path(), RepoState::DocsOnly);
        assert!(docs.contains("documentation files listed above"));

        // Existing project with a manifest: foundation established, no re-scaffold.
        std::fs::write(temp.path().join("package.json"), "{}").unwrap();
        let existing =
            WorkflowRouter::enrich_orchestrator_guidance(temp.path(), RepoState::ExistingCodebase);
        assert!(existing.contains("Foundation: ESTABLISHED (`package.json`)"));
        assert!(existing.contains("Ground first"));
        assert!(!existing.contains("NOT ESTABLISHED"));
    }

    #[test]
    fn test_scan_repository_state_manifest_only_is_existing() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("README.md"), "# Hello").unwrap();
        std::fs::write(temp.path().join("package.json"), "{\"name\":\"x\"}").unwrap();
        let (state, _, _) = WorkflowRouter::scan_repository_state(temp.path());
        assert_eq!(state, RepoState::ExistingCodebase);
        assert_eq!(
            WorkflowRouter::project_manifest(temp.path()),
            Some("package.json")
        );
    }

    #[tokio::test]
    async fn test_enrich_context_has_no_duplicate_plan_or_methodology_dump() {
        let temp = tempfile::tempdir().unwrap();
        let wm = crate::context::memory::working_memory::WorkingMemory::new(temp.path());
        wm.init_plan(
            "Feature",
            &["Step 1: a".to_string(), "Step 2: b".to_string()],
        )
        .unwrap();
        let text = WorkflowRouter::enrich_context(
            temp.path(),
            "Build fullstack web application",
            WorkflowArchetype::Standard,
        )
        .await
        .unwrap();
        // Plan is rendered once by PromptBuilder (<minipower_active_plan>), not here.
        assert!(!text.contains("<active_task_plan_status>"));
        // Static methodology lives in the system prompt, not in per-turn context.
        assert!(!text.contains("<minipower_autonomous_engineering_rules>"));
        assert!(text.contains("<orchestrator_dynamic_guidance>"));
    }
}
