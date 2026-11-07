//! Dev, verification, routing, sandbox, and git slash command handlers (`/tx`, `/dag`, `/heal`, `/sandbox`, `/retrieve`, `/route`, `/quarantine`, `/commit`).

use super::{App, CommandAction};
use anyhow::Result;

impl<'a> App<'a> {
    pub(super) async fn handle_dev_and_git_slash_command(
        &mut self,
        prompt: &str,
    ) -> Result<Option<CommandAction>> {
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
            return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
        }

        Ok(None)
    }
}
