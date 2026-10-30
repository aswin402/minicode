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
    #[allow(dead_code)]
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

    /// Spawns an autonomous `minipub` subagent with custom configuration.
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
            .unwrap_or_else(|| SubagentRole::infer_from_intent(task_prompt, Some(use_worktree)));

        let fut = pool.run_subagent_with_options(role, task_prompt, config, provider, use_worktree);

        if let Some(explicit_timeout) = timeout_secs {
            match tokio::time::timeout(std::time::Duration::from_secs(explicit_timeout), fut).await
            {
                Ok(res) => res,
                Err(_) => Err(MinicodeError::Channel(format!(
                    "MiniPub subagent task timed out after {} seconds",
                    explicit_timeout
                ))),
            }
        } else {
            // Activity-heartbeat watchdog inside SubagentWorker governs timeout dynamically
            fut.await
        }
    }

    /// Concurrently executes a batch of `minipub` subagent tasks with optional Git Worktree isolation.
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

        let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
        let pool = get_global_subagent_pool(workspace_root);
        let provider = pool.get_or_create_provider().await;
        let mut worker_specs = Vec::new();
        let mut launched_descriptions = Vec::new();

        for task in tasks {
            let role = task.role;
            let prompt = task.prompt.clone();
            let timeout = task.timeout_secs.unwrap_or(
                app_cfg
                    .agent
                    .minipub
                    .inactivity_timeout_secs
                    .saturating_mul(2),
            );

            let isolate_worktree = task.isolate_worktree.unwrap_or(
                role.default_workspace_mode() == crate::agent::subagent::WorkspaceMode::Worktree,
            );

            let mut config =
                SubagentConfig::for_role_with_minipub_config(role, &app_cfg.agent.minipub)
                    .with_todos(task.todos)
                    .with_file_boundaries(task.file_boundaries);
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
                "### MiniPub Swarm Fan-Out Launched ({} worker(s) in background)\n\n",
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

        // Wait for all workers to complete using event-driven Notify wakeup
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
                pool.wait_for_change(std::time::Duration::from_millis(250))
                    .await;
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

/// Autonomous state-driven workflow router that inspects live workspace and runtime state,
/// activates tool categories, and synthesizes pre-turn context without keyword heuristics.
pub struct WorkflowRouter;

impl WorkflowRouter {
    /// Computes active `ToolCategory` set purely from live observable workspace and runtime state
    /// (without keyword heuristics on the user prompt).
    pub fn state_driven_tool_categories(
        workspace_root: &Path,
    ) -> std::collections::HashSet<crate::tools::category::ToolCategory> {
        use crate::tools::category::ToolCategory;
        let mut cats = std::collections::HashSet::new();
        let (repo_state, code_files, _) = Self::scan_repository_state(workspace_root);

        match repo_state {
            RepoState::FreshWorkspace | RepoState::DocsOnly => {
                // Expose scaffolding, skills warehouse, UI design warehouse, methodology/verification, dev task runner, and browser verification upfront
                // so the entire workflow can execute smoothly within a turn without schema churn.
                cats.insert(ToolCategory::MiniKit);
                cats.insert(ToolCategory::Vault);
                cats.insert(ToolCategory::Blocks);
                cats.insert(ToolCategory::MiniPower);
                cats.insert(ToolCategory::Dev);
                cats.insert(ToolCategory::Web);
            }
            RepoState::ExistingCodebase => {
                // Codebase exists: expose code navigation, skills warehouse, methodology/verification, dev task runner, and browser verification
                cats.insert(ToolCategory::Codegraph);
                cats.insert(ToolCategory::Search);
                cats.insert(ToolCategory::Vault);
                cats.insert(ToolCategory::MiniPower);
                cats.insert(ToolCategory::Dev);
                cats.insert(ToolCategory::Web);

                if Self::has_ui_surface(workspace_root, &code_files) {
                    cats.insert(ToolCategory::Blocks);
                }
                if crate::tools::minikit::resolve_manifest_path(workspace_root).is_some() {
                    cats.insert(ToolCategory::MiniKit);
                }
            }
        }

        if workspace_root.join(".git").exists() {
            cats.insert(ToolCategory::Git);
        }

        let (active_task_count, _, _) =
            crate::dev::get_global_dev_registry().get_telemetry_snapshot();
        let browser_live = crate::tools::browser::BrowserManager::is_live_engine_running_sync();
        if active_task_count > 0 || browser_live {
            cats.insert(ToolCategory::Dev);
            cats.insert(ToolCategory::Web);
        }

        cats
    }

    /// Checks whether the workspace contains web/UI surface files (HTML, CSS, JS/TS/JSX/TSX, Vue, Svelte, or package.json).
    fn has_ui_surface(workspace_root: &Path, code_files: &[PathBuf]) -> bool {
        if workspace_root.join("package.json").is_file()
            || workspace_root.join("index.html").is_file()
        {
            return true;
        }
        code_files.iter().any(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|ext| {
                    matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "html" | "css" | "scss" | "tsx" | "jsx" | "vue" | "svelte"
                    )
                })
                .unwrap_or(false)
        })
    }

    /// Synthesizes live context enrichment purely from observable workspace and runtime state
    /// plus data-store retrieval (BlockStore, VaultStore, Core Docs).
    /// Designed to be re-evaluated across iterations within a turn as tools mutate state.
    pub async fn enrich_state_driven_context(
        workspace_root: &Path,
        prompt: &str,
    ) -> Option<String> {
        let dev_reg = crate::dev::get_global_dev_registry();
        let (active_task_count, _, _) = dev_reg.get_telemetry_snapshot();
        let browser_live = crate::tools::browser::BrowserManager::is_live_engine_running_sync();
        let has_live_runtime = active_task_count > 0 || browser_live;

        let mut sections: Vec<String> = Vec::new();

        // 1. Live Repository State & Orchestrator Guidance
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

        // 2. When foundation is not yet established, always show available MiniKit starter stacks
        if matches!(repo_state, RepoState::FreshWorkspace | RepoState::DocsOnly) {
            if let Some(scaffolding) = Self::enrich_minikit_scaffolding(workspace_root, prompt) {
                sections.push(scaffolding);
            }
        }

        // 3. When workspace is fresh/docs-only or has a UI surface, retrieve matching palettes & components from BlockStore
        if matches!(repo_state, RepoState::FreshWorkspace | RepoState::DocsOnly)
            || Self::has_ui_surface(workspace_root, &code_files)
        {
            if let Some(ui_block) = Self::enrich_ui_design(workspace_root, prompt) {
                sections.push(ui_block);
            }
        }

        // 4. Live Runtime Services (Dev servers, timers, browser) & Reactive MiniPub Workers
        if has_live_runtime {
            if let Some(runtime_block) = Self::enrich_runtime_dev().await {
                sections.push(runtime_block);
            }
        }
        if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
            if let Some(minipub_block) = pool.format_orchestrator_context_block().await {
                sections.push(minipub_block);
            }
        }

        // 5. Core Documentation Specifications (minikit_docs/core/, docs/)
        if let Some(docs_block) = Self::enrich_core_documentation(workspace_root, prompt) {
            sections.push(docs_block);
        }

        // 6. Hermes Progressive Skills & Learned Gotchas from VaultStore
        let store = crate::vault::store::VaultStore::new(workspace_root);
        if let Some(skills_block) =
            Self::enrich_hermes_skills(workspace_root, &store, prompt, &code_files)
        {
            sections.push(skills_block);
        }

        let lower = prompt.to_ascii_lowercase();
        let relevant_gotchas = store.find_relevant_gotchas(&lower);
        if !relevant_gotchas.is_empty() {
            let mut gotcha_block = String::from("<universal_learned_gotchas>\n");
            gotcha_block.push_str(
                "  Known Pitfalls & Invariants (Learned from previous sessions across this machine):\n",
            );
            for g in relevant_gotchas.iter().take(3) {
                gotcha_block.push_str(&format!(
                    "  {}\n",
                    g.format_prompt_block().replace('\n', "\n  ")
                ));
            }
            gotcha_block.push_str(
                "  Directive: Heed these hard-won lessons to prevent repeating previous compiler and runtime errors.\n",
            );
            gotcha_block.push_str("</universal_learned_gotchas>");
            sections.push(gotcha_block);
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
            if code_files.len() >= 30 && doc_files.len() >= 30 && has_other_files {
                break;
            }
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
    /// the workspace is fresh, docs-only, or an existing software codebase, and what files are present.
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
        let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
        let active_plan = wm.read_active_plan_summary();
        match &active_plan {
            None => {
                out.push_str(
                    "  Active Plan: NONE. For any task mutating files or spanning multiple steps, initialize a right-sized plan FIRST via `create_plan` or `power_plan(topic=..., steps=[...])` (name explicit target files per step so progress tracking is deterministic).\n",
                );
            }
            Some(summary) => {
                let active_step = summary
                    .active_task
                    .as_deref()
                    .unwrap_or("final verification");
                out.push_str(&format!(
                    "  Active Plan: IN PROGRESS ({}/{} steps completed; current focus: `{}`). Maintain strictly one active `[>]` step, call `update_progress(step=\"active\", status=\"completed\")` immediately when each step's deliverable is ready, and run `power_review` + `power_verify` before `archive_plan`.\n",
                    summary.completed_tasks, summary.total_tasks, active_step
                ));
            }
        }
        if let Some((ledger_path, ledger_content)) =
            crate::agent::minipower::MiniPowerEngine::read_sdd_ledger(workspace_root)
        {
            let rel_ledger = ledger_path
                .strip_prefix(workspace_root)
                .unwrap_or(&ledger_path)
                .display();
            let recent_items: Vec<&str> = ledger_content
                .lines()
                .map(str::trim)
                .filter(|l| l.starts_with("- "))
                .rev()
                .take(4)
                .collect();
            if !recent_items.is_empty() {
                out.push_str(&format!(
                    "  MiniPower SDD Ledger (`{}`): {}\n",
                    rel_ledger,
                    recent_items
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
        }
        match state {
            RepoState::FreshWorkspace | RepoState::DocsOnly => {
                out.push_str("  Foundation: NOT ESTABLISHED (no project manifest such as package.json, Cargo.toml, pyproject.toml, go.mod or index.html exists yet).\n");
                if state == RepoState::DocsOnly {
                    out.push_str("  The documentation files listed above may already state the stack or design; read them before deciding.\n");
                }
                if crate::agent::decisions::read_for_prompt(workspace_root).is_some() {
                    out.push_str("  Recorded user decisions exist in `.minicode/decisions.md` (see <user_decisions>): build on them and do not re-ask settled questions.\n");
                }
                out.push_str("  1. Brainstorm & Clarify (`ask_user` / `power_brainstorm`): A long or detailed request is not a decision on the foundation unless it explicitly names the tech stack/framework and visual direction. Never guess or assume when you need clarity, data, or information from the user, or when you lack confidence on what to build: call `ask_user` with as many questions as needed and 4 to 6 options per choice question (recommended option first). For complex multi-module specs, run `power_brainstorm` to lock architecture invariants.\n");
                out.push_str("  2. Scaffold & Plan (`kit_stack_add` + `create_plan` / `power_plan`): Scaffold into workspace root via `kit_stack_add`, inspect relevant skills (`vault_show`), and create a right-sized plan where each step names its explicit target file(s).\n");
                out.push_str("  3. Execute (`block_search` + `update_progress` + `spawn_minipub`): Query `block_palettes` + `block_search(category=..., include_code=true)` / `block_scaffold` before hand-writing UI; call `update_progress(step=\"active\", status=\"completed\")` after each step; delegate independent modules/audits to specialist MiniPubs (`minitask(action=\"spawn_minipub\", role=...)`).\n");
                out.push_str("  4. Verify (`power_review` + `power_verify` + `archive_plan`): Verify via `minitask(action=\"start\")`, `browser_navigate`, `power_review`, and `power_verify` before `archive_plan`.\n");
            }
            RepoState::ExistingCodebase => {
                if let Some(manifest) = Self::project_manifest(workspace_root) {
                    out.push_str(&format!(
                        "  Foundation: ESTABLISHED (`{}`). Do not re-scaffold; extend the existing project.\n",
                        manifest
                    ));
                }
                out.push_str("  1. Ground first & Clarify (`ask_user` / `power_brainstorm`): inspect architecture, imports and types (`locate_symbol`, `grep_search`, `read_file`) before writing code; never guess or assume when you need clarity, domain data, or lack confidence—call `ask_user` (with as many questions as needed and 4 to 6 options per question, recommended first) or `power_brainstorm`.\n");
                out.push_str("  2. Plan & Execute (`create_plan` / `power_plan` + `update_progress`): Initialize a right-sized plan naming explicit target files before mutating code and advance live with `update_progress`; use `block_palettes`/`block_search(category=...)`/`block_scaffold` for UI; delegate independent modules to MiniPub workers (`minitask(action=\"spawn_minipub\", ...)`).\n");
                out.push_str("  3. Verify (`power_review` + `power_verify` + `archive_plan`): Run `power_review` + `power_verify` (and `browser_navigate` for UI changes) before `archive_plan`.\n");
            }
        }
        let vault = crate::vault::store::VaultStore::new(workspace_root);
        let orchestrator_skills: Vec<String> = vault
            .list_all_skills()
            .into_iter()
            .filter(|s| s.category().eq_ignore_ascii_case("orchestrator"))
            .map(|s| format!("`{}`", s.name))
            .collect();
        if !orchestrator_skills.is_empty() {
            out.push_str(&format!(
                "  MiniVault Orchestrator Skillset (decide which applies; inspect via `vault_show`, activate via `vault_load`, or customize via `vault_create`/`vault_update`/`vault_delete`): {}\n",
                orchestrator_skills.join(", ")
            ));
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

    /// Injects pure workspace-state-driven MiniBlocks UI warehouse context and
    /// combinatorial Art Direction (`<taste_art_direction>`) for UI/frontend tasks.
    pub fn enrich_ui_design(workspace_root: &Path, prompt: &str) -> Option<String> {
        let store_lock = crate::blocks::get_global_block_store();
        let store = match store_lock.read() {
            Ok(s) => s,
            Err(e) => e.into_inner(),
        };

        let stack_info = crate::blocks::seed::ProjectStackInfo::detect(workspace_root);
        let stats = store.stats();

        let mut out = String::from("<recommended_miniblocks>\n");
        if Self::project_manifest(workspace_root).is_none()
            && crate::agent::decisions::read_for_prompt(workspace_root).is_none()
        {
            out.push_str(
                "  Important: Foundation is NOT established yet. If the user did not explicitly name the tech stack and visual direction, call `ask_user` FIRST before `create_plan`, `kit_stack_add`, or `block_search`.\n",
            );
        }

        out.push_str(&format!(
            "  {}\n",
            stack_info.format_prompt_directive().replace('\n', "\n  ")
        ));
        out.push_str(&format!(
            "  Warehouse Inventory: {} components, {} color palettes, {} CSS gradients, {} page templates.\n",
            stats.total_components, stats.total_palettes, stats.total_gradients, stats.total_templates
        ));

        if let Some(fw) = stack_info.framework {
            let fw_count = stats.framework_counts.get(&fw).copied().unwrap_or(0);
            out.push_str(&format!(
                "  Active Framework Filter (`{}`): {} native components ready (plus cross-framework fallback).\n",
                fw, fw_count
            ));

            // Surface representative components for the detected workspace framework across core layout categories
            let sample_categories = [
                crate::blocks::BlockCategory::Navbar,
                crate::blocks::BlockCategory::Hero,
                crate::blocks::BlockCategory::Feature,
                crate::blocks::BlockCategory::Card,
                crate::blocks::BlockCategory::Pricing,
                crate::blocks::BlockCategory::Footer,
            ];
            let mut samples = Vec::new();
            for cat in sample_categories {
                let filter = crate::blocks::BlockSearchFilter {
                    category: Some(cat),
                    framework: Some(fw),
                    limit: 1,
                    ..Default::default()
                };
                if let Some(first) = store.search_components(&filter).into_iter().next() {
                    samples.push(first);
                }
                if samples.len() >= 4 {
                    break;
                }
            }

            if !samples.is_empty() {
                out.push_str("  Sample Warehouse Components for this Stack:\n");
                for c in &samples {
                    out.push_str(&format!(
                        "     • `{}` (category: {}, framework: {}): {}\n",
                        c.name, c.category, c.framework, c.description
                    ));
                }
            }
        }

        let palettes: Vec<_> = store.list_palettes().into_iter().take(2).collect();
        if !palettes.is_empty() {
            out.push_str(
                "  Sample Design Token Palettes (browse all with `block_palettes(query=...)`):\n",
            );
            for p in palettes {
                out.push_str(&format!(
                    "     • `{}`: --bg: {}; --surface: {}; --accent: {}; --text: {};\n",
                    p.name, p.colors[0], p.colors[1], p.colors[2], p.colors[3]
                ));
            }
        }

        out.push_str("  MiniBlocks Tool Workflow:\n");
        out.push_str("     1. Design Tokens: call `block_palettes(query=..., format=\"css\"|\"tailwind\")` and `block_gradients(query=...)` before writing root theme styles.\n");
        out.push_str("     2. Component Discovery: call `block_search(query=..., category=..., include_code=true)` (auto-detects workspace framework) before writing UI sections from scratch, or `block_get(id=...)` for full component details.\n");
        out.push_str("     3. Component Placement & Wiring: call `block_scaffold(component_name=..., category=..., palette=..., wire_to=...)` to generate a themed component and wire its import, `block_insert(component_id=..., target_file=...)` to inject code, or `block_import(component_name=..., consumer_file=..., wire=true)`.\n");
        out.push_str("     4. Warehouse Curation: use `block_save`, `block_update`, `block_delete`, and `block_stats` when saving or managing reusable components.\n");
        out.push_str("</recommended_miniblocks>");

        // Automatically deal or load the workspace's combinatorial Art Direction brief (`taste-skill` v2.1)
        let direction = crate::blocks::taste::deal_art_direction(workspace_root, prompt);
        out.push_str("\n\n");
        out.push_str(&direction.format_prompt_block());

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

    pub async fn enrich_runtime_dev() -> Option<String> {
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
            out.push_str("  Invariants: Keep dev servers & GUI browsers running across turns; close headless browsers via `browser_close` when inspection finishes; use `minitask(action=\"resources\"|\"status\"|\"stop\"|\"kill_all\")` instead of raw `ps`/`top`/`kill`/`pkill`.\n");
        }
        out.push_str("</active_dev_services>");
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

                let title_lower = title.to_ascii_lowercase();
                let content_sample_lower =
                    crate::utils::strings::truncate_chars(&content, 2000).to_ascii_lowercase();

                let name_words: std::collections::HashSet<&str> = name_stem
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .filter(|s| !s.is_empty())
                    .collect();
                let title_words: std::collections::HashSet<&str> = title_lower
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .filter(|s| !s.is_empty())
                    .collect();
                let content_words: std::collections::HashSet<&str> = content_sample_lower
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .filter(|s| !s.is_empty())
                    .collect();

                let mut score = 0;
                for token in &prompt_tokens {
                    if name_stem == *token || name_words.contains(token) {
                        score += 10;
                    }
                    if title_words.contains(token) {
                        score += 5;
                    }
                    if content_words.contains(token) {
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
    /// Dynamically scans available built-in and MiniVault skills, matches them against the prompt,
    /// active session objective (from WorkingMemory), and workspace code files (via skill `globs`
    /// and `triggers`), and injects procedural rules and traps into context without arbitrary truncations.
    fn enrich_hermes_skills(
        workspace_root: &Path,
        store: &crate::vault::store::VaultStore,
        prompt: &str,
        code_files: &[PathBuf],
    ) -> Option<String> {
        let manifest_active = store.get_manifest_active_skills();
        let project_active: Vec<String> = store
            .list_all_skills()
            .into_iter()
            .filter(|s| s.is_active_in_project)
            .map(|s| s.name)
            .collect();

        let mut exclude = manifest_active;
        exclude.extend(project_active);

        let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_root);
        let session_objective = wm.read_active_plan_summary().map(|summary| {
            let phase = summary.phase_label.trim();
            let all_titles: Vec<&str> = summary
                .tasks
                .iter()
                .filter(|t| {
                    t.status != crate::context::memory::working_memory::TaskItemStatus::Completed
                })
                .map(|t| t.title.as_str())
                .collect();
            let tasks_str = if all_titles.is_empty() {
                summary.active_task.unwrap_or_default()
            } else {
                all_titles.join(" ; ")
            };
            match (phase.is_empty(), tasks_str.is_empty()) {
                (false, false) => format!("{} — {}", phase, tasks_str),
                (false, true) => phase.to_string(),
                (true, false) => tasks_str,
                (true, true) => String::new(),
            }
        });

        store.format_dynamic_turn_skills(
            Some(prompt),
            session_objective.as_deref().filter(|s| !s.is_empty()),
            code_files,
            &exclude,
        )
    }

    /// Re-evaluates MiniVault progressive skill matching immediately after a plan (`create_plan`
    /// or `power_plan`) is created mid-turn, combining the newly created plan steps and current
    /// workspace files so relevant procedural skills are surfaced right before Step 1 execution.
    pub fn enrich_post_plan_execution_context(
        workspace_root: &Path,
        prompt: &str,
    ) -> Option<String> {
        let (_, code_files, _) = Self::scan_repository_state(workspace_root);
        let store = crate::vault::store::VaultStore::new(workspace_root);
        Self::enrich_hermes_skills(workspace_root, &store, prompt, &code_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_workflow_router_enrichment() {
        let temp = tempfile::tempdir().unwrap();
        let prompt_ui = "Build a responsive navbar component";
        let enrichment = WorkflowRouter::enrich_state_driven_context(temp.path(), prompt_ui).await;
        assert!(enrichment.is_some());
        let text = enrichment.unwrap();
        assert!(text.contains("<recommended_miniblocks>"));
        assert!(text.contains("<minikit_scaffolding_guidance>"));
        assert!(text.contains("<orchestrator_dynamic_guidance>"));
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

        let enrichment = WorkflowRouter::enrich_state_driven_context(
            temp.path(),
            "Review architecture and modular boundaries",
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
        let enrichment = WorkflowRouter::enrich_state_driven_context(
            temp.path(),
            "Create modern ui design with responsive css layout",
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
        let text = WorkflowRouter::enrich_state_driven_context(
            temp.path(),
            "Build fullstack web application",
        )
        .await
        .unwrap();
        // Plan is rendered once by PromptBuilder (<minipower_active_plan>), not here.
        assert!(!text.contains("<active_task_plan_status>"));
        // Static methodology lives in the system prompt, not in per-turn context.
        assert!(!text.contains("<minipower_autonomous_engineering_rules>"));
        assert!(text.contains("<orchestrator_dynamic_guidance>"));
    }

    #[tokio::test]
    async fn test_state_driven_transition_across_iterations() {
        use crate::tools::category::ToolCategory;
        let temp = tempfile::tempdir().unwrap();

        // Iteration 1: Fresh workspace
        let cats1 = WorkflowRouter::state_driven_tool_categories(temp.path());
        assert!(cats1.contains(&ToolCategory::MiniKit));
        assert!(cats1.contains(&ToolCategory::Blocks));
        assert!(cats1.contains(&ToolCategory::MiniPower));
        let ctx1 = WorkflowRouter::enrich_state_driven_context(
            temp.path(),
            "Build a showcase page for minicode",
        )
        .await
        .unwrap();
        assert!(ctx1.contains("Foundation: NOT ESTABLISHED"));
        assert!(ctx1.contains("<minikit_scaffolding_guidance>"));
        assert!(ctx1.contains("<recommended_miniblocks>"));

        // Iteration 2: After scaffolding creates package.json and src/App.tsx
        std::fs::write(temp.path().join("package.json"), "{\"name\":\"demo\"}").unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(
            temp.path().join("src").join("App.tsx"),
            "export default function App() {}",
        )
        .unwrap();

        let cats2 = WorkflowRouter::state_driven_tool_categories(temp.path());
        assert!(cats2.contains(&ToolCategory::Dev));
        assert!(cats2.contains(&ToolCategory::Web));
        assert!(cats2.contains(&ToolCategory::Blocks));
        assert!(cats2.contains(&ToolCategory::MiniPower));
        assert!(cats2.contains(&ToolCategory::Codegraph));

        let ctx2 = WorkflowRouter::enrich_state_driven_context(
            temp.path(),
            "Build a showcase page for minicode",
        )
        .await
        .unwrap();
        assert!(ctx2.contains("Foundation: ESTABLISHED (`package.json`)"));
        assert!(!ctx2.contains("Foundation: NOT ESTABLISHED"));
    }

    #[test]
    fn test_enrich_hermes_skills_uses_active_plan_and_files() {
        let temp = tempfile::tempdir().unwrap();
        let plan_dir = temp.path().join(".minicode").join("plan");
        std::fs::create_dir_all(&plan_dir).unwrap();
        std::fs::write(
            plan_dir.join("TASK.md"),
            "# Task Plan: Security Audit\n\n## Phase 1: OWASP Hardening\n- [ ] 1. Audit SQL injection and JWT auth middleware\n",
        )
        .unwrap();

        let store = crate::vault::store::VaultStore::new(temp.path());
        let code_files = vec![temp.path().join("src").join("auth_middleware.rs")];
        // Even with a generic prompt like "continue", the active plan objective ("OWASP Hardening — Audit SQL injection and JWT auth middleware")
        // and code_files are passed into `format_dynamic_turn_skills`.
        let enriched =
            WorkflowRouter::enrich_hermes_skills(temp.path(), &store, "continue", &code_files);
        assert!(enriched.is_some());
        let text = enriched.unwrap();
        assert!(text.contains("<auto_activated_skills>"));
    }
}
