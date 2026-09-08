//! Interactive command and prompt dispatch logic for minicode TUI

use super::{AgentCommand, App};
use crate::ui::modal::ModalState;
use anyhow::Result;
use std::time::Instant;
use tokio::sync::mpsc;

/// Result of dispatching a command or prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    /// Continue running the interactive event loop
    Continue,
    /// Exit the application immediately
    #[allow(dead_code)]
    Exit,
}

impl<'a> App<'a> {
    /// Handles user submitted text, routing to slash commands or background agent execution
    pub async fn handle_command_or_prompt(
        &mut self,
        prompt: &str,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) -> Result<CommandAction> {
        if prompt == "/exit" || prompt == "/quit" {
            self.modal = ModalState::new_exit_confirm(&self.workspace_root);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/terminal" {
            self.pty_drawer.toggle();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/copy" || prompt.starts_with("/copy ") {
            let copy_all = prompt.contains("all");
            let text_to_copy = if copy_all {
                self.timeline.get_all_transcript_text()
            } else {
                self.timeline
                    .get_last_assistant_response()
                    .unwrap_or_default()
            };

            if text_to_copy.trim().is_empty() {
                self.timeline
                    .add_status("ℹ Nothing to copy yet".to_string());
            } else {
                let ok = crate::ui::clipboard::copy_to_clipboard(&text_to_copy);
                if ok {
                    let label = if copy_all {
                        "entire conversation"
                    } else {
                        "latest assistant response"
                    };
                    self.timeline
                        .add_status(format!("✔ Copied {} to clipboard", label));
                } else {
                    self.timeline
                        .add_status("✗ Failed to copy to clipboard".to_string());
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/new" {
            self.timeline = crate::ui::TimelineView::new();
            self.timeline
                .add_status("✨ Started a new session".to_string());
            return Ok(CommandAction::Continue);
        }

        if prompt == "/configure"
            || prompt == "/config"
            || prompt == "/setup"
            || prompt == "/keys"
            || prompt == "/key"
            || prompt == "/api"
        {
            self.modal = ModalState::new_provider_select();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/clear" {
            self.timeline.entries.clear();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/commands" {
            self.modal = ModalState::new_command_catalog();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/help" {
            self.modal = ModalState::Help;
            return Ok(CommandAction::Continue);
        }

        if prompt == "/stack" || prompt == "/stacks" {
            self.modal = ModalState::new_stack_select();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/explore" || prompt.starts_with("/explore ") {
            let query = prompt.trim_start_matches("/explore").trim();
            if query.is_empty() {
                self.modal = ModalState::new_code_explorer(&self.workspace_root);
            } else {
                let explore_prompt = format!(
                    "Surgically explore the following symbol or architecture question in the codebase using `code_explore`: {}",
                    query
                );
                self.timeline.add_user_message(prompt.to_string());
                self.is_working = true;
                self.work_start = Some(Instant::now());
                let cancel = tokio_util::sync::CancellationToken::new();
                self.cancel_token = Some(cancel.clone());
                let _ = control_tx.send(AgentCommand::Prompt(explore_prompt, Some(cancel)));
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/plan" || prompt.starts_with("/plan ") {
            let query = prompt.trim_start_matches("/plan").trim();
            let plan_prompt = if query.is_empty() {
                "Inspect the current repository architecture and generate a structured, verifiable milestone implementation plan in onpkg_docs/todo.md and onpkg_docs/implementation.md.".to_string()
            } else {
                format!("Plan and break down the following implementation into actionable verifiable tasks in onpkg_docs/todo.md: {}", query)
            };
            self.timeline.add_user_message(prompt.to_string());
            self.is_working = true;
            self.work_start = Some(Instant::now());
            let cancel = tokio_util::sync::CancellationToken::new();
            self.cancel_token = Some(cancel.clone());
            let _ = control_tx.send(AgentCommand::Prompt(plan_prompt, Some(cancel)));
            return Ok(CommandAction::Continue);
        }

        if prompt == "/goal" || prompt.starts_with("/goal ") {
            let query = prompt.trim_start_matches("/goal").trim();
            let goal_prompt = if query.is_empty() {
                "<!-- GOAL --> Execute all pending tasks in onpkg_docs/todo.md autonomously. Run verifications after each step and continue until all tasks are marked [x].".to_string()
            } else {
                format!("<!-- GOAL --> Execute the following goal autonomously to completion: {}\nUpdate onpkg_docs/todo.md, execute step-by-step, verify with tests, and do not stop until fully achieved.", query)
            };
            self.timeline.add_user_message(prompt.to_string());
            self.is_working = true;
            self.work_start = Some(Instant::now());
            let cancel = tokio_util::sync::CancellationToken::new();
            self.cancel_token = Some(cancel.clone());
            let _ = control_tx.send(AgentCommand::Prompt(goal_prompt, Some(cancel)));
            return Ok(CommandAction::Continue);
        }

        if prompt == "/diff" || prompt == "/diffs" {
            let ws = self.workspace_root.clone();
            match crate::git::GitDiffViewer::load_diffs(&ws, false).await {
                Ok(diff_files) => {
                    self.modal = ModalState::new_git_diff(diff_files, false);
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Failed to load git diff: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/review" || prompt.starts_with("/review ") {
            let staged_only = prompt
                .split_whitespace()
                .any(|w| w == "--staged" || w == "-s");
            self.timeline.add_user_message(prompt.to_string());
            self.timeline
                .add_status("🛡️ Running multi-agent adversarial code review...".to_string());

            match crate::git::GitReviewer::review_workspace(&self.workspace_root, staged_only).await
            {
                Ok(report) => {
                    let formatted = crate::git::GitReviewer::format_report(&report);
                    self.timeline
                        .entries
                        .push(crate::ui::view::TimelineEntry::AssistantMarkdown(formatted));
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Code review error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/model" || prompt == "/models" || prompt == "/provider" {
            self.modal = ModalState::new_provider_select();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/undo" {
            let backup_mgr = crate::session::backup::BackupManager::new(&self.workspace_root);
            let checkpoints = backup_mgr.list_checkpoints();
            if checkpoints.is_empty() {
                self.timeline
                    .add_status("ℹ No recorded checkpoints available to undo".to_string());
            } else {
                self.modal = ModalState::new_undo_checkpoint(checkpoints);
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/theme" || prompt == "/themes" {
            self.modal = ModalState::new_theme_select(&self.config.ui.theme);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/tokens" {
            let model_limit =
                crate::agent::models::get_model_context_limit(&self.config.provider.model);
            let card = format!(
                "📊 Token & Context Metrics:\n  • Provider: {}\n  • Active Model: {}\n  • Context Limit: {} tokens\n  • Last Turn Usage: {} tokens\n  • Compaction Threshold: {:.0}%",
                self.config.provider.default,
                self.config.provider.model,
                model_limit,
                self.last_turn_tokens,
                self.config.agent.warning_threshold * 100.0
            );
            self.timeline.add_status(card);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/streaming" {
            self.modal = ModalState::new_streaming_select(self.config.agent.streaming);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/parallel" || prompt.starts_with("/parallel ") {
            let args = prompt.strip_prefix("/parallel").unwrap_or("").trim();
            if args.is_empty() {
                let status_msg = format!(
                    "⚡ **Speculative Parallel Tool Execution Pipeline**\n\
                     • **Parallel Tools**: {}\n\
                     • **Speculative Pre-Execution**: {}\n\
                     • **Max Concurrency**: {} threads\n\
                     • **Safety Engine**: Barrier isolation (Mutating & Barrier tools serialized)\n\n\
                     *Available commands:*\n\
                     • `/parallel on` — Enable parallel read-only tool execution\n\
                     • `/parallel off` — Disable parallel execution (strictly sequential)\n\
                     • `/parallel speculative on` — Enable streaming speculative pre-execution\n\
                     • `/parallel speculative off` — Disable streaming speculative pre-execution\n\
                     • `/parallel <1-16>` — Set maximum parallel worker limit",
                    if self.config.agent.parallel_tools { "ENABLED" } else { "DISABLED" },
                    if self.config.agent.speculative_execution { "ENABLED (Streaming Overlap)" } else { "DISABLED" },
                    self.config.agent.max_parallel_tools,
                );
                self.timeline.add_status(status_msg);
            } else {
                match args.to_lowercase().as_str() {
                    "on" | "enable" | "true" | "1" => {
                        self.config.agent.parallel_tools = true;
                        self.timeline
                            .add_status("⚡ Parallel tool execution enabled.".to_string());
                    }
                    "off" | "disable" | "false" | "0" => {
                        self.config.agent.parallel_tools = false;
                        self.timeline.add_status(
                            "⚡ Parallel tool execution disabled (strictly sequential)."
                                .to_string(),
                        );
                    }
                    "speculative on" | "speculative true" => {
                        self.config.agent.speculative_execution = true;
                        self.timeline.add_status(
                            "⚡ Speculative streaming pre-execution enabled.".to_string(),
                        );
                    }
                    "speculative off" | "speculative false" => {
                        self.config.agent.speculative_execution = false;
                        self.timeline.add_status(
                            "⚡ Speculative streaming pre-execution disabled.".to_string(),
                        );
                    }
                    other => {
                        if let Ok(num) = other.parse::<usize>() {
                            let clamped = num.clamp(
                                crate::constants::MIN_PARALLEL_TOOLS,
                                crate::constants::MAX_PARALLEL_TOOLS_CAP,
                            );
                            self.config.agent.max_parallel_tools = clamped;
                            self.timeline.add_status(format!(
                                "⚡ Maximum parallel tools limit set to {} concurrent workers.",
                                clamped
                            ));
                        } else {
                            self.timeline.add_status(format!(
                                "⚠️ Invalid argument '{}'. Use `on`, `off`, `speculative on`, `speculative off`, or a number (1-16).",
                                other
                            ));
                        }
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/tx"
            || prompt.starts_with("/tx ")
            || prompt == "/transaction"
            || prompt.starts_with("/transaction ")
        {
            let remainder = if let Some(r) = prompt.strip_prefix("/transaction") {
                r.trim()
            } else {
                prompt.strip_prefix("/tx").unwrap_or("").trim()
            };

            match remainder.to_lowercase().as_str() {
                "" | "status" => {
                    match crate::session::transaction::TransactionManager::status(
                        &self.workspace_root,
                        None,
                    ) {
                        Ok(Some(receipt)) => {
                            self.timeline.add_status(receipt.format_receipt());
                        }
                        Ok(None) => {
                            self.timeline.add_status("ℹ No active workspace transaction. Workspace is in direct modification mode.\n\nUse `/tx begin <description>` to start an atomic transaction.".to_string());
                        }
                        Err(e) => {
                            self.timeline.add_status(format!(
                                "✗ Error retrieving transaction status: {}",
                                e
                            ));
                        }
                    }
                }
                "commit" => {
                    match crate::session::transaction::TransactionManager::commit(
                        &self.workspace_root,
                        None,
                    ) {
                        Ok(receipt) => {
                            self.timeline.add_status(receipt.format_receipt());
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Transaction commit failed: {}", e));
                        }
                    }
                }
                "rollback" => {
                    match crate::session::transaction::TransactionManager::rollback(
                        &self.workspace_root,
                        None,
                        Some("Manual user rollback from /tx"),
                    ) {
                        Ok(receipt) => {
                            self.timeline.add_status(receipt.format_receipt());
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Transaction rollback failed: {}", e));
                        }
                    }
                }
                other if other.starts_with("begin ") => {
                    let desc = other.strip_prefix("begin ").unwrap_or("").trim();
                    match crate::session::transaction::TransactionManager::begin(
                        &self.workspace_root,
                        desc,
                    ) {
                        Ok(manifest) => {
                            self.timeline.add_status(format!(
                                "✔ Began atomic workspace transaction '{}' for: {}\nAll subsequent file modifications will be journaled in the WAL and can be atomically rolled back.",
                                manifest.tx_id, manifest.description
                            ));
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Failed to begin transaction: {}", e));
                        }
                    }
                }
                _ => {
                    self.timeline.add_status(
                        "📦 **Workspace Transaction Commands:**\n\
                         • `/tx` — Show active transaction status and affected files\n\
                         • `/tx begin <desc>` — Start a new atomic workspace transaction\n\
                         • `/tx commit` — Commit active transaction and seal WAL journal\n\
                         • `/tx rollback` — Revert all modified/created/deleted files in transaction"
                            .to_string(),
                    );
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/dag" || prompt.starts_with("/dag ") {
            self.timeline.add_status(
                "⚡ **Dynamic Execution DAG & JSONPath Pipelining (`execute_dag`)**\n\
                 Compose multiple dependent tool calls into an atomic, wave-scheduled DAG.\n\n\
                 **Key Features:**\n\
                 • **Topological Waves**: Independent nodes execute concurrently in parallel waves\n\
                 • **JSONPath Pipelining**: Reference upstream outputs via `$node_id.path` or `${node_id.path}`\n\
                 • **Failure Isolation**: Upstream node failures automatically skip downstream dependents (`SkippedDependencyFailed`)\n\
                 • **Cycle Protection**: Strict Kahn's algorithm cycle detection\n\n\
                 **Example Tool Call Schema:**\n\
                 ```json\n\
                 {\n  \"name\": \"locate_and_read\",\n  \"nodes\": [\n    {\n      \"id\": \"locate\",\n      \"tool\": \"locate_fault\",\n      \"args\": { \"query\": \"auth_handler\" }\n    },\n    {\n      \"id\": \"read\",\n      \"tool\": \"read_file\",\n      \"args\": { \"path\": \"$locate.matches[0].path\", \"start_line\": \"$locate.matches[0].line\" },\n      \"depends_on\": [\"locate\"]\n    }\n  ]\n}\n\
                 ```".to_string()
            );
            return Ok(CommandAction::Continue);
        }

        if prompt == "/heal" || prompt.starts_with("/heal ") {
            let args = prompt.strip_prefix("/heal").unwrap_or("").trim();
            let dry_run = args == "dry" || args == "--dry-run";
            self.timeline.add_status(
                "⏳ Running workspace diagnostic triage & self-healing pass...".to_string(),
            );
            match crate::agent::self_healing::SelfHealingEngine::heal(
                &self.workspace_root,
                3,
                true,
                dry_run,
            )
            .await
            {
                Ok(report) => {
                    self.timeline
                        .add_status(report.format_summary(&self.workspace_root));
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Self-healing diagnostic error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/sandbox"
            || prompt.starts_with("/sandbox ")
            || prompt == "/sb"
            || prompt.starts_with("/sb ")
        {
            let raw_args = if let Some(stripped) = prompt.strip_prefix("/sandbox") {
                stripped.trim()
            } else {
                prompt.strip_prefix("/sb").unwrap_or("").trim()
            };

            if raw_args.is_empty() {
                let bwrap_status = if crate::sandbox::is_bwrap_available() {
                    "Bubblewrap (unprivileged namespaces) AVAILABLE"
                } else {
                    "Bubblewrap not installed (using Landlock / process isolation)"
                };
                self.timeline.add_status(format!(
                    "🛡️ **Dynamic Code Sandbox (`/sandbox`)**\n\
                     • **Active Backend**: {}\n\
                     • **Default Isolation**: Network BLOCKED, Workspace Read-Write, Ephemeral Off\n\
                     • **Usage Options**:\n\
                       - `/sandbox <cmd>`: Run command with network isolated\n\
                       - `/sandbox --net <cmd>`: Allow network access\n\
                       - `/sandbox --ro <cmd>`: Read-only workspace protection\n\
                       - `/sandbox --ephemeral <cmd>`: Discard all disk writes upon exit",
                    bwrap_status
                ));
                return Ok(CommandAction::Continue);
            }

            let mut policy = crate::sandbox::SandboxPolicy::default();
            let mut cmd_to_run = raw_args;
            loop {
                if let Some(rest) = cmd_to_run.strip_prefix("--net ") {
                    policy.allow_network = true;
                    cmd_to_run = rest.trim();
                } else if let Some(rest) = cmd_to_run.strip_prefix("--ro ") {
                    policy.read_only_workspace = true;
                    cmd_to_run = rest.trim();
                } else if let Some(rest) = cmd_to_run.strip_prefix("--ephemeral ") {
                    policy.ephemeral_overlay = true;
                    cmd_to_run = rest.trim();
                } else {
                    break;
                }
            }

            self.timeline.add_status(format!(
                "⏳ Running sandboxed command: `{}` (net: {}, ro: {}, ephemeral: {})...",
                cmd_to_run,
                policy.allow_network,
                policy.read_only_workspace,
                policy.ephemeral_overlay
            ));

            match crate::sandbox::run_sandboxed(&self.workspace_root, cmd_to_run, &policy).await {
                Ok(res) => {
                    self.timeline
                        .add_status(crate::sandbox::format_sandbox_result(&res));
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Sandbox execution error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/retrieve"
            || prompt.starts_with("/retrieve ")
            || prompt == "/hr"
            || prompt.starts_with("/hr ")
        {
            let query = if let Some(stripped) = prompt.strip_prefix("/retrieve") {
                stripped.trim()
            } else {
                prompt.strip_prefix("/hr").unwrap_or("").trim()
            };

            if query.is_empty() {
                self.timeline.add_status(
                    "ℹ **Usage**: `/retrieve <query>` (e.g. `/retrieve transaction rollback` or `/retrieve authentication`).\n\
                     Executes multi-modal fusion across AST CodeGraph, BM25, semantic vectors, wiki, and episodic memory.".to_string()
                );
                return Ok(CommandAction::Continue);
            }

            self.timeline.add_status(format!(
                "🔍 Retrieving multi-modal knowledge for: `{}`...",
                query
            ));
            match crate::context::fusion::KnowledgeFusionEngine::retrieve(
                &self.workspace_root,
                query,
                5,
                true,
                true,
                true,
            ) {
                Ok(bundle) => {
                    self.timeline
                        .add_status(crate::context::fusion::format_fused_bundle(&bundle));
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Knowledge retrieval error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/route"
            || prompt.starts_with("/route ")
            || prompt == "/ro"
            || prompt.starts_with("/ro ")
        {
            let arg = if let Some(stripped) = prompt.strip_prefix("/route") {
                stripped.trim()
            } else {
                prompt.strip_prefix("/ro").unwrap_or("").trim()
            };

            let mut router = crate::agent::router::AdaptiveModelRouter::new();

            if arg.is_empty() || arg == "status" {
                self.timeline.add_status(router.format_status_report());
            } else if arg.eq_ignore_ascii_case("auto") {
                router.set_forced_tier(None);
                self.timeline.add_status(
                    "⚡ **Model Router Mode**: Set to Adaptive Auto-Routing (Complexity-Driven)."
                        .to_string(),
                );
            } else if let Some(tier) = crate::agent::router::ModelTier::parse_tier(arg) {
                router.set_forced_tier(Some(tier));
                self.timeline.add_status(format!(
                    "🔒 **Model Router Mode**: Manually locked to `{}` tier ({}).\nRun `/route auto` to restore adaptive mode.",
                    tier.badge(),
                    tier.description()
                ));
            } else {
                let decision = router.route_turn(arg, 0, false, 0, 0);
                let mut out = format!(
                    "# 🔀 Model Routing Assessment: {}\n\n",
                    decision.tier.badge()
                );
                out.push_str(&format!("📋 **Query/Task:** `{}`\n", arg));
                out.push_str(&format!(
                    "🎯 **Recommended Tier:** `{}` ({})\n",
                    decision.tier.badge(),
                    decision.tier.description()
                ));
                out.push_str(&format!("💡 **Reasoning:** {}\n", decision.reason));
                out.push_str(&format!(
                    "💰 **Relative Cost Factor:** {:.2}x\n\n",
                    decision.estimated_cost_factor
                ));
                out.push_str(&format!(
                    "🚀 **Primary Target:** `{}` / `{}` (Priority: {}, Context: {}k)\n\n",
                    decision.primary_endpoint.provider_name,
                    decision.primary_endpoint.model_name,
                    decision.primary_endpoint.priority,
                    decision.primary_endpoint.max_context / 1000,
                ));
                if !decision.fallback_chain.is_empty() {
                    out.push_str("🛡️ **Fallback Failover Chain:**\n");
                    for (idx, fb) in decision.fallback_chain.iter().enumerate() {
                        out.push_str(&format!(
                            "{}. `{}` / `{}` (Priority: {}, Context: {}k{})\n",
                            idx + 1,
                            fb.provider_name,
                            fb.model_name,
                            fb.priority,
                            fb.max_context / 1000,
                            if fb.is_local { ", Local" } else { "" }
                        ));
                    }
                }
                self.timeline.add_status(out);
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/quarantine"
            || prompt.starts_with("/quarantine ")
            || prompt == "/q"
            || prompt.starts_with("/q ")
        {
            let arg = if let Some(stripped) = prompt.strip_prefix("/quarantine") {
                stripped.trim()
            } else {
                prompt.strip_prefix("/q").unwrap_or("").trim()
            };

            if arg.is_empty() || arg == "list" || arg == "status" {
                let store = crate::context::flaky::QuarantineManager::load(&self.workspace_root);
                self.timeline
                    .add_status(crate::context::flaky::QuarantineManager::format_report(
                        &store,
                    ));
            } else if arg == "clear" {
                match crate::context::flaky::QuarantineManager::clear(&self.workspace_root) {
                    Ok(count) => {
                        self.timeline.add_status(format!(
                            "✅ Cleared {} quarantined test(s). All tests restored to active test runs.",
                            count
                        ));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("❌ Failed to clear quarantine: {}", e));
                    }
                }
            } else if let Some(target) = arg
                .strip_prefix("remove ")
                .or_else(|| arg.strip_prefix("rm "))
            {
                let test_name = target.trim();
                match crate::context::flaky::QuarantineManager::unquarantine(
                    &self.workspace_root,
                    test_name,
                ) {
                    Ok(true) => {
                        self.timeline.add_status(format!(
                            "✅ Removed `{}` from quarantine. It will now execute in standard test suites.",
                            test_name
                        ));
                    }
                    Ok(false) => {
                        self.timeline.add_status(format!(
                            "ℹ `{}` was not found in active quarantine list.",
                            test_name
                        ));
                    }
                    Err(e) => {
                        self.timeline.add_status(format!(
                            "❌ Failed to remove `{}` from quarantine: {}",
                            test_name, e
                        ));
                    }
                }
            } else if let Some(target) = arg.strip_prefix("add ") {
                let parts: Vec<&str> = target.splitn(2, ' ').collect();
                let test_name = parts[0].trim();
                let reason = if parts.len() > 1 {
                    parts[1].trim()
                } else {
                    "Manual quarantine via /quarantine command"
                };
                match crate::context::flaky::QuarantineManager::quarantine(
                    &self.workspace_root,
                    test_name,
                    1.0,
                    crate::context::flaky::FlakySignature::Unknown,
                    reason,
                    1,
                ) {
                    Ok(_) => {
                        self.timeline.add_status(format!(
                            "🛡️ Successfully quarantined `{}`.\nReason: {}",
                            test_name, reason
                        ));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("❌ Failed to quarantine `{}`: {}", test_name, e));
                    }
                }
            } else {
                let trimmed = arg.strip_prefix("detect ").unwrap_or(arg).trim();
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                let test_name = parts.first().copied().unwrap_or("");
                let runs = parts
                    .get(1)
                    .and_then(|r| r.parse::<usize>().ok())
                    .unwrap_or(crate::constants::DEFAULT_FLAKY_RUNS);

                if test_name.is_empty() {
                    self.timeline.add_status(
                        "❌ Please specify a test name to analyze: `/quarantine detect <test_name> [runs]`"
                            .to_string(),
                    );
                } else {
                    self.timeline.add_status(format!(
                        "🔬 Initiating {} burn-in executions for test `{}`...",
                        runs, test_name
                    ));
                    match crate::context::flaky::FlakyTestDetector::execute_burn_in(
                        &self.workspace_root,
                        test_name,
                        runs,
                        crate::constants::FLAKY_TEST_TIMEOUT_SECS,
                    )
                    .await
                    {
                        Ok(report) => {
                            let mut out = report.format_markdown();
                            if report.verdict
                                == crate::context::flaky::FlakinessVerdict::FlakyIntermittent
                            {
                                let _ = crate::context::flaky::QuarantineManager::quarantine(
                                    &self.workspace_root,
                                    test_name,
                                    report.flakiness_ratio,
                                    report.signature,
                                    &format!(
                                        "Automated quarantine: {:.1}% failure variance across {} burn-in runs",
                                        report.flakiness_ratio * 100.0,
                                        report.total_runs
                                    ),
                                    report.total_runs,
                                );
                                out.push_str(&format!(
                                    "\n\n🛡️ **Auto-Quarantine Applied**: Test `{}` has been quarantined in `.minicode/quarantine.json`.",
                                    test_name
                                ));
                            }
                            self.timeline.add_status(out);
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("❌ Burn-in analysis failed: {}", e));
                        }
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/commit"
            || prompt.starts_with("/commit ")
            || prompt == "/ci"
            || prompt.starts_with("/ci ")
        {
            let arg = if let Some(stripped) = prompt.strip_prefix("/commit") {
                stripped.trim()
            } else if let Some(stripped) = prompt.strip_prefix("/ci") {
                stripped.trim()
            } else {
                ""
            };

            if arg == "help" {
                let help_msg = "📝 **Semantic Commit Synthesis & Changelog Commands**\n\n\
                    • `/commit` or `/commit preview [task hint]` — Synthesize conventional commit proposals & atomic sequence\n\
                    • `/commit now [task hint]` or `/commit -y` — Synthesize message and immediately commit working tree changes\n\
                    • `/commit changelog [version]` — Synthesize Keep-a-Changelog release draft from current diff\n\
                    • `/commit help` — Display this usage reference";
                self.timeline.add_status(help_msg.to_string());
                return Ok(CommandAction::Continue);
            }

            if arg == "changelog" || arg.starts_with("changelog ") {
                let ver = arg.strip_prefix("changelog").unwrap_or("").trim();
                let version = if ver.is_empty() { None } else { Some(ver) };
                match crate::git::commit_synth::SemanticCommitSynthesizer::synthesize(
                    &self.workspace_root,
                    None,
                    None,
                )
                .await
                {
                    Ok(mut report) => {
                        if let Some(v) = version {
                            report.changelog_markdown = crate::git::commit_synth::SemanticCommitSynthesizer::generate_changelog_draft(
                                report.unified_proposal.commit_type,
                                &report.unified_proposal.summary,
                                &report.affected_files,
                                &report.unified_proposal.scope,
                                Some(v),
                            );
                        }
                        self.timeline.add_status(format!(
                            "📜 **Keep-a-Changelog Release Draft**:\n\n```markdown\n{}\n```",
                            report.changelog_markdown
                        ));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("❌ Failed to synthesize changelog: {}", e));
                    }
                }
                return Ok(CommandAction::Continue);
            }

            if arg == "now" || arg.starts_with("now ") || arg == "-y" || arg.starts_with("-y ") {
                let task_hint = if let Some(stripped) = arg.strip_prefix("now") {
                    let s = stripped.trim();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else if let Some(stripped) = arg.strip_prefix("-y") {
                    let s = stripped.trim();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else {
                    None
                };

                match crate::git::commit_synth::SemanticCommitSynthesizer::synthesize(
                    &self.workspace_root,
                    None,
                    task_hint,
                )
                .await
                {
                    Ok(report) => {
                        let commit_msg = report.unified_proposal.format_full_message();
                        match crate::git::commit_synth::SemanticCommitSynthesizer::execute_commit(
                            &self.workspace_root,
                            &commit_msg,
                            None,
                        )
                        .await
                        {
                            Ok(res) => {
                                self.timeline.add_status(format!(
                                    "✅ **Successfully Committed Working Tree Changes**\n\n{}\n\n**Commit Message:**\n```\n{}\n```",
                                    res,
                                    commit_msg
                                ));
                            }
                            Err(e) => {
                                self.timeline
                                    .add_status(format!("❌ Commit execution failed: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("❌ Commit synthesis failed: {}", e));
                    }
                }
                return Ok(CommandAction::Continue);
            }

            // Default or /commit preview [task_hint]
            let task_hint = if let Some(stripped) = arg.strip_prefix("preview") {
                let s = stripped.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            } else if arg.is_empty() {
                None
            } else {
                Some(arg)
            };

            match crate::git::commit_synth::SemanticCommitSynthesizer::synthesize(
                &self.workspace_root,
                None,
                task_hint,
            )
            .await
            {
                Ok(report) => {
                    self.timeline.add_status(report.format_markdown());
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("❌ Commit synthesis failed: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/thinking" || prompt.starts_with("/thinking ") {
            let args = prompt.strip_prefix("/thinking").unwrap_or("").trim();
            if args.is_empty() {
                let current_budget = match self.config.provider.thinking_budget {
                    Some(b) if b >= crate::constants::MIN_THINKING_BUDGET_TOKENS => {
                        format!("{} tokens (ENABLED)", b)
                    }
                    Some(b) => format!("{} tokens (below min threshold)", b),
                    None => "Disabled".to_string(),
                };
                let effort = self
                    .config
                    .provider
                    .reasoning_effort
                    .as_deref()
                    .unwrap_or("auto");
                let status_msg = format!(
                    "🧠 **Extended Thinking / Reasoning Configuration**\n\
                     • **Provider**: {}\n\
                     • **Model**: {}\n\
                     • **Thinking Budget**: {}\n\
                     • **Reasoning Effort**: {}\n\n\
                     *Available commands:*\n\
                     • `/thinking off` — Disable extended thinking\n\
                     • `/thinking 4k` — 4,096 tokens (fast reasoning)\n\
                     • `/thinking 8k` — 8,192 tokens (balanced reasoning)\n\
                     • `/thinking 16k` — 16,000 tokens (standard extended thinking)\n\
                     • `/thinking 32k` — 32,000 tokens (deep architectural refactors)\n\
                     • `/thinking 64k` — 64,000 tokens (maximum frontier budget)",
                    self.config.provider.default,
                    self.config.provider.model,
                    current_budget,
                    effort,
                );
                self.timeline.add_status(status_msg);
            } else {
                match args.to_lowercase().as_str() {
                    "off" | "disable" | "none" | "0" => {
                        self.config.provider.thinking_budget = None;
                        self.config.provider.reasoning_effort = None;
                        self.timeline.add_status(
                            "🧠 Extended thinking / test-time reasoning disabled.".to_string(),
                        );
                    }
                    "4k" => {
                        self.config.provider.thinking_budget = Some(4096);
                        self.config.provider.reasoning_effort = Some("low".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 4,096 tokens (effort: low)."
                                .to_string(),
                        );
                    }
                    "8k" => {
                        self.config.provider.thinking_budget = Some(8192);
                        self.config.provider.reasoning_effort = Some("medium".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 8,192 tokens (effort: medium)."
                                .to_string(),
                        );
                    }
                    "16k" | "default" => {
                        self.config.provider.thinking_budget = Some(16000);
                        self.config.provider.reasoning_effort = Some("medium".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 16,000 tokens (effort: medium)."
                                .to_string(),
                        );
                    }
                    "32k" => {
                        self.config.provider.thinking_budget = Some(32000);
                        self.config.provider.reasoning_effort = Some("high".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 32,000 tokens (effort: high)."
                                .to_string(),
                        );
                    }
                    "64k" | "max" => {
                        self.config.provider.thinking_budget = Some(64000);
                        self.config.provider.reasoning_effort = Some("high".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 64,000 tokens (effort: high)."
                                .to_string(),
                        );
                    }
                    other => {
                        if let Ok(num) = other.parse::<usize>() {
                            let clamped = num.clamp(
                                crate::constants::MIN_THINKING_BUDGET_TOKENS,
                                crate::constants::MAX_THINKING_BUDGET_TOKENS,
                            );
                            let effort = if clamped <= 4096 {
                                "low"
                            } else if clamped <= 16000 {
                                "medium"
                            } else {
                                "high"
                            };
                            self.config.provider.thinking_budget = Some(clamped);
                            self.config.provider.reasoning_effort = Some(effort.to_string());
                            self.timeline.add_status(format!(
                                "🧠 Extended thinking budget set to {} tokens (effort: {}).",
                                clamped, effort
                            ));
                        } else {
                            self.timeline.add_status(format!(
                                "⚠️ Invalid thinking budget '{}'. Use off, 4k, 8k, 16k, 32k, 64k, or a number between 1024 and 64000.",
                                other
                            ));
                        }
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        let is_analysis_keyword = prompt.eq_ignore_ascii_case("analyze the project")
            || prompt.eq_ignore_ascii_case("analyze project")
            || prompt.eq_ignore_ascii_case("index codebase")
            || prompt.eq_ignore_ascii_case("generate code graph")
            || prompt.eq_ignore_ascii_case("reindex")
            || prompt.eq_ignore_ascii_case("re-index");

        if prompt == "/init" || prompt == "/index" || prompt == "/analyze" || is_analysis_keyword {
            self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/map" {
            let mut graph = crate::context::graph::CodeGraph::new();
            if let Err(e) = graph.build_graph(&self.workspace_root) {
                self.timeline
                    .add_status(format!("✗ Failed to build repo map: {}", e));
            } else {
                let repomap =
                    graph.format_repomap(&self.workspace_root, &[], self.config.agent.map_tokens);
                self.timeline
                    .add_status(format!("🗺️ AST PageRank Repository Map:\n\n{}", repomap));
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/compact" {
            let model_limit =
                crate::agent::models::get_model_context_limit(&self.config.provider.model);
            let t1 = (model_limit as f64 * crate::constants::COMPACT_TIER1_RATIO) as usize;
            let t2 = (model_limit as f64 * crate::constants::COMPACT_TIER2_RATIO) as usize;
            let t3 = (model_limit as f64 * crate::constants::COMPACT_TIER3_RATIO) as usize;
            let status_msg = format!(
                "🗜️ **Context Window Status** (`{}`)\n• **Window Limit**: {} tokens\n• **Tier 1 (Masking)**: {} tokens (60%)\n• **Tier 2 (Summary)**: {} tokens (80%)\n• **Tier 3 (Anchor)**: {} tokens (95%)\n• *Auto-compaction evaluates automatically before each turn.*",
                self.config.provider.model, model_limit, t1, t2, t3
            );
            self.timeline.add_status(status_msg);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/sessions" || prompt == "/history" {
            let store = crate::session::store::SessionStore::with_workspace(&self.workspace_root);
            match store.list_sessions_rich() {
                Ok(sessions) => {
                    let initial_summary = sessions
                        .first()
                        .and_then(|s| store.get_session_summary(&s.id).ok());
                    self.modal = ModalState::new_session_browser(sessions, initial_summary);
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Failed to list sessions: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/export" || prompt.starts_with("/export ") {
            let target_path = prompt.strip_prefix("/export").unwrap_or("").trim();
            let store = crate::session::store::SessionStore::with_workspace(&self.workspace_root);
            let session_id = match store.get_last_session_id() {
                Some(id) => id,
                None => {
                    self.timeline.add_status(
                        "ℹ No recorded sessions to export in this workspace.".to_string(),
                    );
                    return Ok(CommandAction::Continue);
                }
            };
            let export_file = if target_path.is_empty() {
                let export_dir = self.workspace_root.join(".minicode").join("exports");
                if let Err(e) = std::fs::create_dir_all(&export_dir) {
                    tracing::warn!(error = %e, "Failed to create exports directory");
                }
                export_dir.join(format!("{}.md", session_id))
            } else {
                let p = self.workspace_root.join(target_path);
                if let Some(parent) = p.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        tracing::warn!(error = %e, "Failed to create parent directory");
                    }
                }
                p
            };
            match store.export_markdown(&session_id, &export_file) {
                Ok(p) => {
                    self.timeline
                        .add_status(format!("📄 Exported session Markdown to {}", p.display()));
                }
                Err(e) => {
                    self.timeline.add_status(format!("✗ Export failed: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/save" || prompt.starts_with("/save ") {
            let target_path = prompt.strip_prefix("/save").unwrap_or("").trim();
            let export_file = if target_path.is_empty() {
                let export_dir = self.workspace_root.join(".minicode").join("exports");
                if let Err(e) = std::fs::create_dir_all(&export_dir) {
                    tracing::warn!(error = %e, "Failed to create exports directory");
                }
                export_dir.join(format!(
                    "session_{}.md",
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                ))
            } else {
                self.workspace_root.join(target_path)
            };
            let mut md = format!(
                "# minicode Session Export — {}\n\n",
                chrono::Utc::now().to_rfc3339()
            );
            for entry in &self.timeline.entries {
                match entry {
                    crate::ui::view::TimelineEntry::UserPrompt(text) => {
                        md.push_str(&format!("### 👤 User\n{}\n\n", text));
                    }
                    crate::ui::view::TimelineEntry::AssistantMarkdown(text) => {
                        md.push_str(&format!("### 🤖 Assistant\n{}\n\n", text));
                    }
                    crate::ui::view::TimelineEntry::ToolStart {
                        name,
                        command_or_path,
                    } => {
                        md.push_str(&format!(
                            "*🔧 Tool Started:* `{}` (`{}`)\n\n",
                            name, command_or_path
                        ));
                    }
                    crate::ui::view::TimelineEntry::ToolFinished {
                        name,
                        command_or_path,
                        success,
                        output,
                        ..
                    } => {
                        md.push_str(&format!(
                            "*🔧 Tool Finished:* `{}` (`{}` - {})\n```\n{}\n```\n\n",
                            name,
                            command_or_path,
                            if *success { "success" } else { "failure" },
                            output
                        ));
                    }
                    crate::ui::view::TimelineEntry::SystemStatus(text) => {
                        md.push_str(&format!("*ℹ Status:* {}\n\n", text));
                    }
                    _ => {}
                }
            }
            if let Err(e) = std::fs::write(&export_file, md) {
                self.timeline
                    .add_status(format!("✗ Failed to save session: {}", e));
            } else {
                self.timeline.add_status(format!(
                    "✔ Saved conversation export to {}",
                    export_file.display()
                ));
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/load" || prompt.starts_with("/load ") {
            let target_id = prompt.strip_prefix("/load").unwrap_or("").trim();
            let store = crate::session::store::SessionStore::with_workspace(&self.workspace_root);
            if target_id.is_empty() {
                match store.list_sessions() {
                    Ok(sessions) if !sessions.is_empty() => {
                        let mut list_msg = format!("📂 Available Sessions ({}):\n", sessions.len());
                        for s in sessions
                            .iter()
                            .take(crate::constants::SESSION_LOAD_LIST_MAX)
                        {
                            list_msg.push_str(&format!("  • {} ({})\n", s.id, s.created_at));
                        }
                        list_msg.push_str("\nUse `/load <session_id>` to view a session.");
                        self.timeline.add_status(list_msg);
                    }
                    Ok(_) => {
                        self.timeline
                            .add_status("ℹ No past sessions found".to_string());
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("✗ Failed to list sessions: {}", e));
                    }
                }
            } else {
                match store.load_session(target_id) {
                    Ok(events) => {
                        self.timeline.entries.clear();
                        self.hydrate_session(&events);
                        self.timeline.add_status(format!(
                            "✔ Loaded session '{}' with {} events",
                            target_id,
                            events.len()
                        ));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("✗ Failed to load session '{}': {}", target_id, e));
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        // Handle /retry or standard user prompt
        let prompt_to_run = if prompt == "/retry" {
            if let Some(ref last) = self.last_user_prompt {
                self.timeline
                    .add_status(format!("🔄 Retrying last prompt: \"{}\"", last));
                last.clone()
            } else {
                self.timeline
                    .add_status("ℹ No previous user prompt to retry".to_string());
                return Ok(CommandAction::Continue);
            }
        } else {
            self.last_user_prompt = Some(prompt.to_string());
            prompt.to_string()
        };

        self.timeline.add_user_message(prompt_to_run.clone());

        // Check for recognized autonomous intent to notify the user
        if let Some(m) = crate::agent::intent::match_intent(&prompt_to_run) {
            if m.confidence >= 0.85 && m.intent != crate::agent::intent::AgentIntent::GeneralQuery {
                let (icon, label) = m.intent.badge();
                self.timeline
                    .add_status(format!("{} Autonomous Intent: {}", icon, label));
            }
        }

        self.is_working = true;
        self.work_start = Some(Instant::now());

        let cancel = tokio_util::sync::CancellationToken::new();
        self.cancel_token = Some(cancel.clone());

        // Dispatch asynchronously to agent background actor
        if let Err(e) = control_tx.send(AgentCommand::Prompt(prompt_to_run, Some(cancel))) {
            tracing::error!(error = %e, "Failed to dispatch prompt to agent actor");
        }

        Ok(CommandAction::Continue)
    }
}
