use crate::agent::subagent::{get_global_subagent_pool, SubAgentResult, SubagentRole};
use crate::error::{MinicodeError, Result, ToolError};
use crate::git::worktree::WorktreeManager;
use std::path::{Path, PathBuf};

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
            .unwrap_or_else(|| SubagentRole::infer_from_intent(task_prompt, None));

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

    /// Merges an isolated subagent worktree branch into the current branch and cleans up.
    #[allow(dead_code)]
    pub async fn merge_worktree(workspace_root: &Path, subagent_id: &str) -> Result<String> {
        let agent_id = crate::agent::subagent::types::AgentId(subagent_id.to_string());
        let branch_name = crate::sandbox::worktree::GitWorktreeManager::resolve_branch_for(
            workspace_root,
            &agent_id,
        );
        let mgr = WorktreeManager::new(workspace_root);
        let merge_output = mgr.merge_worktree(subagent_id).await.map_err(|e| {
            MinicodeError::Tool(ToolError::CommandExec(format!(
                "Failed to merge subagent worktree branch `{}`: {}",
                branch_name, e
            )))
        })?;

        let _ = mgr.remove_worktree(subagent_id).await;

        Ok(format!(
            "✔ Successfully integrated subagent worktree `[ID: {}]`!\n\
             • Branch `{}` merged into current active branch.\n\
             • Temporary worktree folder removed.\n\n\
             Git Merge Summary:\n{}",
            subagent_id,
            branch_name,
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
                // Expose scaffolding, skills warehouse, UI design warehouse, methodology/verification, subagent orchestration, dev task runner, and browser verification upfront
                // so the entire workflow can execute smoothly within a turn without schema churn.
                cats.insert(ToolCategory::MiniKit);
                cats.insert(ToolCategory::Vault);
                cats.insert(ToolCategory::Blocks);
                cats.insert(ToolCategory::MiniPower);
                cats.insert(ToolCategory::Agent);
                cats.insert(ToolCategory::Dev);
                cats.insert(ToolCategory::Web);
            }
            RepoState::ExistingCodebase => {
                // Codebase exists: expose code navigation, skills warehouse, methodology/verification, subagent orchestration, dev task runner, and browser verification
                cats.insert(ToolCategory::Codegraph);
                cats.insert(ToolCategory::Search);
                cats.insert(ToolCategory::Vault);
                cats.insert(ToolCategory::MiniPower);
                cats.insert(ToolCategory::Agent);
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

    /// Checks whether the workspace contains web/UI surface files via [`WorkspaceProfile`].
    fn has_ui_surface(workspace_root: &Path, code_files: &[PathBuf]) -> bool {
        crate::context::workspace_profile::WorkspaceProfile::has_ui_surface(
            workspace_root,
            code_files,
        )
    }

    /// Synthesizes live context enrichment purely from observable workspace and runtime state
    /// plus data-store retrieval (BlockStore, VaultStore, Core Docs), gated by the mathematical
    /// `PromptSignalVector` so zero-energy conversational/meta turns incur zero enrichment overhead.
    pub async fn enrich_state_driven_context(
        workspace_root: &Path,
        prompt: &str,
    ) -> Option<String> {
        Self::enrich_state_driven_context_with_baseline(workspace_root, prompt, None).await
    }

    pub async fn enrich_state_driven_context_with_baseline(
        workspace_root: &Path,
        prompt: &str,
        baseline_inlined_skills: Option<&[String]>,
    ) -> Option<String> {
        let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
        let active_plan_summary = wm.read_active_plan_summary();
        let active_plan_text = active_plan_summary.as_ref().map(|s| {
            let step = s.active_task.as_deref().unwrap_or_default();
            format!("{} {}", s.phase_label, step)
        });
        let signal = crate::context::search::intent_filter::IntentClassifier::analyze_signal(
            prompt,
            active_plan_text.as_deref(),
        );
        if !signal.has_actionable_task_intent() {
            return None;
        }

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
        sections.push(Self::enrich_orchestrator_guidance_with_affinity(
            workspace_root,
            repo_state,
            signal.has_plan_continuation_affinity(),
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
        let mut workspace_files = code_files.clone();
        workspace_files.extend(doc_files.iter().cloned());
        let _ = store.sync_project_active_skills(&workspace_files);
        if let Some(skills_block) = Self::enrich_hermes_skills_with_baseline(
            workspace_root,
            &store,
            prompt,
            &workspace_files,
            baseline_inlined_skills,
        ) {
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

    /// Dynamically scans workspace files and directories via [`WorkspaceProfile::scan`]
    /// using Syntect grammar scopes + canonical manifests instead of hardcoded arrays.
    pub fn scan_repository_state(workspace_root: &Path) -> (RepoState, Vec<PathBuf>, Vec<PathBuf>) {
        crate::context::workspace_profile::WorkspaceProfile::scan(workspace_root)
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
        Self::enrich_orchestrator_guidance_with_affinity(workspace_root, state, true)
    }

    pub fn enrich_orchestrator_guidance_with_affinity(
        workspace_root: &Path,
        state: RepoState,
        include_active_plan: bool,
    ) -> String {
        let mut out = String::from("<orchestrator_dynamic_guidance>\n");
        let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
        let active_plan = wm
            .read_active_plan_summary()
            .filter(|_| include_active_plan);
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
                out.push_str("  Foundation: NOT ESTABLISHED (no project manifest exists yet).\n");
                if state == RepoState::DocsOnly {
                    out.push_str("  The documentation files listed above may already state the stack or design; read them before deciding.\n");
                }
                if crate::agent::decisions::read_for_prompt(workspace_root).is_some() {
                    out.push_str("  Recorded user decisions exist in `.minicode/decisions.md` (see <user_decisions>): build on them without re-asking settled questions.\n");
                }
                out.push_str("  Workflow: A long or detailed request is not a decision on the foundation unless it names stack and visual direction; clarify open choices via `ask_user` / `power_brainstorm`, scaffold via `kit_stack_add`, track steps via `create_plan` / `update_progress`, and gate completion with `power_verify`.\n");
            }
            RepoState::ExistingCodebase => {
                if let Some(manifest) = Self::project_manifest(workspace_root) {
                    out.push_str(&format!(
                        "  Foundation: ESTABLISHED (`{}`). Do not re-scaffold; extend the existing project.\n",
                        manifest
                    ));
                }
                out.push_str("  Workflow: Ground first in existing symbols and imports (`locate_symbol`, `grep_search`, `read_file`), clarify ambiguities via `ask_user`, track multi-file steps via `create_plan` / `update_progress`, and verify via `power_review` + `power_verify`.\n");
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

    /// Returns the first project manifest found at the workspace root via [`WorkspaceProfile`].
    pub(crate) fn project_manifest(workspace_root: &Path) -> Option<&'static str> {
        crate::context::workspace_profile::WorkspaceProfile::project_manifest(workspace_root)
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

            // Dynamically retrieve prompt-relevant components for the detected workspace framework
            let query_opt = {
                let q = prompt.trim();
                (!q.is_empty()).then(|| q.to_string())
            };
            let mut samples = if query_opt.is_some() {
                store.search_components(&crate::blocks::BlockSearchFilter {
                    query: query_opt,
                    framework: Some(fw),
                    limit: 4,
                    ..Default::default()
                })
            } else {
                Vec::new()
            };
            if samples.len() < 4 {
                for fallback in store.search_components(&crate::blocks::BlockSearchFilter {
                    framework: Some(fw),
                    limit: 8,
                    ..Default::default()
                }) {
                    if samples.len() >= 4 {
                        break;
                    }
                    if !samples.iter().any(|s| s.id == fallback.id) {
                        samples.push(fallback);
                    }
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
            out.push_str("  Invariants: Keep dev servers & GUI browsers running across turns (NEVER stop a dev server at turn end unless requested); close headless browsers via `browser_close` when inspection finishes; use `minitask(action=\"resources\"|\"status\"|\"stop\"|\"kill_all\")` internally instead of raw `ps`/`top`/`kill`/`pkill`, and never tell the human user to invoke internal `minitask(...)` syntax in final responses.\n");
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
        Self::enrich_hermes_skills_with_baseline(workspace_root, store, prompt, code_files, None)
    }

    fn enrich_hermes_skills_with_baseline(
        workspace_root: &Path,
        store: &crate::vault::store::VaultStore,
        prompt: &str,
        code_files: &[PathBuf],
        baseline_inlined_skills: Option<&[String]>,
    ) -> Option<String> {
        let exclude: Vec<String> =
            baseline_inlined_skills
                .map(|b| b.to_vec())
                .unwrap_or_else(|| {
                    let mut ex = store.get_manifest_active_skills();
                    ex.extend(
                        store
                            .list_all_skills()
                            .into_iter()
                            .filter(|s| s.is_active_in_project)
                            .map(|s| s.name),
                    );
                    ex
                });

        let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_root);
        let raw_session_objective = wm.read_active_plan_summary().map(|summary| {
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

        let signal = crate::context::search::intent_filter::IntentClassifier::analyze_signal(
            prompt,
            raw_session_objective.as_deref(),
        );
        if !signal.has_actionable_task_intent() {
            return None;
        }

        let effective_objective = raw_session_objective
            .as_deref()
            .filter(|s| !s.is_empty() && signal.has_plan_continuation_affinity());
        let effective_files: &[PathBuf] = (signal.has_plan_continuation_affinity()
            || signal.has_code_exploration_intent())
        .then_some(code_files)
        .unwrap_or(&[]);

        store.format_dynamic_turn_skills(
            Some(prompt),
            effective_objective,
            effective_files,
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
        let (_, mut workspace_files, doc_files) = Self::scan_repository_state(workspace_root);
        workspace_files.extend(doc_files);
        let store = crate::vault::store::VaultStore::new(workspace_root);
        let _ = store.sync_project_active_skills(&workspace_files);
        Self::enrich_hermes_skills(workspace_root, &store, prompt, &workspace_files)
    }

    /// Declarative post-tool state transition router driven by `SKILL.md` `after_tools` hooks
    /// and live workspace state (replaces hardcoded tool-name `if/else` blocks in `tool_finalize`).
    pub fn enrich_post_tool_transition(
        workspace_root: &Path,
        tool_name: &str,
        prompt: &str,
    ) -> Option<String> {
        let store = crate::vault::store::VaultStore::new(workspace_root);
        let (hook_block, matched_skills) = store.format_post_tool_skills(tool_name)?;

        let mut blocks = vec![hook_block];

        let triggers_ui_warehouse = matched_skills
            .iter()
            .any(|s| s.category().eq_ignore_ascii_case("ui-styling"));
        if triggers_ui_warehouse {
            let (_, code_files, _) = Self::scan_repository_state(workspace_root);
            let has_ui = Self::has_ui_surface(workspace_root, &code_files)
                || crate::blocks::seed::ProjectStackInfo::detect(workspace_root)
                    .framework
                    .is_some();
            if has_ui {
                if let Some(ui_enrichment) = Self::enrich_ui_design(workspace_root, prompt) {
                    blocks.push(ui_enrichment);
                }
            }
        }

        let triggers_plan_execution = matched_skills
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case("executing-plans"));
        if triggers_plan_execution {
            if let Some(post_plan_skills) =
                Self::enrich_post_plan_execution_context(workspace_root, prompt)
            {
                blocks.push(post_plan_skills);
            }
        }

        Some(blocks.join("\n\n"))
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

        // Conversational prompt ("hii") must NOT leak active plan objective or code_files into skill activation
        let enriched_hii =
            WorkflowRouter::enrich_hermes_skills(temp.path(), &store, "hii", &code_files);
        assert!(enriched_hii.is_none());
    }

    #[tokio::test]
    async fn test_enrich_state_driven_context_returns_none_for_zero_energy_conversational_prompt() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("package.json"), "{\"name\":\"demo\"}").unwrap();
        std::fs::write(
            temp.path().join("todo.md"),
            "# Todo\n- [ ] Build dashboard\n",
        )
        .unwrap();
        let ctx_hii = WorkflowRouter::enrich_state_driven_context(temp.path(), "hii").await;
        assert!(ctx_hii.is_none());
    }

    #[test]
    fn test_declarative_post_tool_transition_hooks() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("package.json"),
            "{\"dependencies\":{\"react\":\"^18\"}}",
        )
        .unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/App.tsx"), "export function App() {}").unwrap();

        let post_stack = WorkflowRouter::enrich_post_tool_transition(
            temp.path(),
            "kit_stack_add",
            "build SaaS landing page",
        )
        .unwrap();
        assert!(post_stack.contains("<post_tool_skill_guidance tool=\"kit_stack_add\">"));
        assert!(post_stack.contains("<recommended_miniblocks>"));

        let post_plan = WorkflowRouter::enrich_post_tool_transition(
            temp.path(),
            "create_plan",
            "implement JWT authentication middleware",
        )
        .unwrap();
        assert!(post_plan.contains("<post_tool_skill_guidance tool=\"create_plan\">"));
        assert!(post_plan.contains("executing-plans"));
    }

    #[test]
    fn test_detect_ultrawork_ignores_meta_and_quoted_mentions() {
        assert!(crate::agent::AgentLoop::detect_ultrawork_intent(
            "ulw implement the payment webhook handler"
        ));
        assert!(crate::agent::AgentLoop::detect_ultrawork_intent(
            "do not stop until all tests pass"
        ));
        assert!(!crate::agent::AgentLoop::detect_ultrawork_intent(
            "what does ultrawork do?"
        ));
        assert!(!crate::agent::AgentLoop::detect_ultrawork_intent(
            "explain \"ulw\" mode"
        ));
    }
}
