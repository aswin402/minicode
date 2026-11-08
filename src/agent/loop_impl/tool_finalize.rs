//! Post-tool execution pipeline (`finalize_executed_tool`) for `AgentLoop`.

use super::AgentLoop;
use crate::agent::provider::CompletionOptions;
use crate::agent::stuck_detector::BreakerAction;
use crate::agent::types::{AgentEvent, Message, ToolCall};
use crate::constants::FILE_MODIFYING_TOOLS;
use crate::error::Result;
use std::collections::HashMap;
use tokio::sync::mpsc;

impl AgentLoop {
    /// Unified post-tool processing for both parallel and sequential execution stages:
    /// runs middleware, donut truncation, anti-thrashing circuit breaker, dynamic tool
    /// schema activation, `AgentEvent::ToolResult` persistence, WorkingMemory/IntentLedger
    /// reconciliation, learned gotcha injection, observation pruning, and micro-compaction.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn finalize_executed_tool(
        &mut self,
        tool_call: ToolCall,
        tool_result: crate::agent::types::ToolResult,
        file_existed_before: bool,
        file_before: Option<String>,
        turn_id: usize,
        user_prompt: &str,
        used_tokens_est: usize,
        event_sender: &mpsc::UnboundedSender<AgentEvent>,
        options: &mut CompletionOptions,
        tools: &mut Vec<crate::agent::provider::ToolSchema>,
        active_categories: &mut std::collections::HashSet<crate::tools::category::ToolCategory>,
        mcp_tools_by_server: &HashMap<String, Vec<crate::agent::provider::ToolSchema>>,
        active_mcp_servers: &mut std::collections::HashSet<String>,
        active_tool_names: &mut std::collections::HashSet<String>,
        turn_files_modified: &mut Vec<String>,
        items_progressed_this_turn: &mut bool,
        circuit_tripped: &mut bool,
        turn_tool_results: &mut Vec<crate::agent::types::ToolResult>,
        inject_step_grounding: bool,
    ) -> Result<(bool, usize)> {
        let mut tool_result = self.tool_pipeline.run(
            tool_result,
            &tool_call.name,
            &self.workspace_root,
            &tool_call.arguments,
            file_before.as_deref(),
        );
        if tool_result.success
            && matches!(
                tool_call.name.as_str(),
                "kit_stack_add"
                    | "kit_skill_install"
                    | "kit_skill_remove"
                    | "create_skill"
                    | "vault_load"
                    | "vault_unload"
                    | "vault_bundle_load"
                    | "vault_create"
                    | "vault_update"
                    | "vault_patch"
                    | "vault_delete"
                    | "vault_import_url"
            )
        {
            let refreshed = crate::agent::prompt::PromptBuilder::build_static_system_prompt(
                &self.workspace_root,
                None,
            );
            if let Some(first_msg) = self.messages.first_mut() {
                if first_msg.role == crate::agent::types::Role::System {
                    first_msg.content = refreshed.clone();
                }
            }
            options.system_instruction = Some(refreshed);
            if matches!(
                tool_call.name.as_str(),
                "kit_stack_add" | "minikit_stack_add" | "onpkg_stack_add"
            ) {
                let stack_skills = crate::vault::store::VaultStore::new(&self.workspace_root)
                    .resolve_turn_active_skills(user_prompt, None, &Vec::<String>::new());
                if !stack_skills.is_empty() {
                    let skill_event = AgentEvent::SkillActivity {
                        turn_id,
                        action: "using".to_string(),
                        skills: stack_skills,
                        detail: Some("Auto-activated from scaffolded stack".to_string()),
                    };
                    let _ = self
                        .session_store
                        .append_event(&self.session_id, &skill_event);
                    let _ = event_sender.send(skill_event);
                }
            }
        }

        if tool_result.success {
            const SKILL_TOOL_ACTIVITY_TABLE: &[(&[&str], &str, &[&str])] = &[
                (
                    &[
                        "vault_show",
                        "vault_skill_show",
                        "skill_show",
                        "kit_skill_show",
                        "onpkg_skill_show",
                    ],
                    "using",
                    &["name", "skill_name"],
                ),
                (
                    &[
                        "vault_load",
                        "vault_skill_load",
                        "skill_load",
                        "kit_skill_install",
                        "onpkg_skill_install",
                        "vault_bundle_load",
                        "vault_ingest",
                        "vault_source_ingest",
                        "vault_import_url",
                    ],
                    "loaded",
                    &["name", "skill_name", "bundle", "uri", "url"],
                ),
                (
                    &[
                        "vault_create",
                        "vault_skill_create",
                        "skill_create",
                        "create_skill",
                        "vault_update",
                        "vault_skill_update",
                        "skill_update",
                        "vault_patch",
                        "vault_skill_patch",
                        "skill_patch",
                        "vault_gotcha_add",
                    ],
                    "saved",
                    &["name", "trigger"],
                ),
                (
                    &[
                        "vault_unload",
                        "kit_skill_remove",
                        "onpkg_skill_remove",
                        "vault_bundle_unload",
                        "vault_delete",
                    ],
                    "unloaded",
                    &["name", "skill_name", "bundle"],
                ),
            ];

            if let Some((_, action, arg_keys)) = SKILL_TOOL_ACTIVITY_TABLE
                .iter()
                .find(|(tools, _, _)| tools.contains(&tool_call.name.as_str()))
            {
                let target = arg_keys
                    .iter()
                    .find_map(|k| tool_call.arguments.get(*k).and_then(|v| v.as_str()))
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(tool_call.name.as_str());
                let detail = tool_call
                    .arguments
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|fp| format!("Read Level-3 resource `{fp}`"));
                let skill_event = AgentEvent::SkillActivity {
                    turn_id,
                    action: (*action).to_string(),
                    skills: vec![target.to_string()],
                    detail,
                };
                let _ = self
                    .session_store
                    .append_event(&self.session_id, &skill_event);
                let _ = event_sender.send(skill_event);
            }
        }

        // === Smart Donut Truncator (Phase 88, 115 + Dynamic Context-Remaining Scaling) ===
        let ctx_limit = self.compactor.model_token_limit();
        tool_result.output =
            crate::context::donut::SmartDonutTruncator::truncate_for_remaining_context(
                &tool_result.output,
                ctx_limit,
                used_tokens_est,
            )
            .content;
        if tool_result.success {
            self.compactor
                .record_tool_file_ops(&tool_call.name, &tool_call.arguments);
        }

        // === Algorithmic Stuck Detector & Anti-Thrashing Circuit Breaker (Phase 112) ===
        match self
            .stuck_detector
            .check(&tool_call.name, &tool_call.arguments, tool_result.success)
        {
            BreakerAction::Pass => {}
            BreakerAction::Warning(intervention) => {
                tracing::warn!(
                    tool = %tool_call.name,
                    "Anti-thrashing stuck detector warning injected"
                );
                tool_result.output.push_str("\n\n");
                tool_result.output.push_str(&intervention);
            }
            BreakerAction::Trip { reason, loop_type } => {
                tracing::warn!(
                    tool = %tool_call.name,
                    pattern = %loop_type.pattern_name(),
                    "Anti-thrashing circuit breaker intervention injected"
                );
                tool_result.output.push_str("\n\n");
                tool_result.output.push_str(&reason);

                let trip_event = AgentEvent::AntiThrashTripped {
                    turn_id,
                    pattern: loop_type.pattern_name().to_string(),
                    target: loop_type.target_file(),
                    failures: loop_type.failures(),
                    intervention: reason,
                };
                if let Err(e) = self
                    .session_store
                    .append_event(&self.session_id, &trip_event)
                {
                    tracing::warn!("Failed to persist AntiThrashTripped event: {}", e);
                }
                let _ = event_sender.send(trip_event);
                *circuit_tripped = true;
            }
        }

        // Dynamic tool schema activation (activate_tools, lazy invocation, or near-miss lookup)
        if tool_call.name == "activate_tools" && tool_result.success {
            let cat_arg = crate::tools::param::get_str_with_aliases(
                &tool_call.arguments,
                &["category", "query", "search", "tools", "tool"],
            );
            let extra_query = crate::tools::param::opt_str(&tool_call.arguments, "query");
            if let Some(cat_str) = cat_arg {
                if cat_str == "all" {
                    active_categories.extend(crate::tools::category::ToolCategory::ALL);
                    active_mcp_servers.extend(mcp_tools_by_server.keys().cloned());
                } else if cat_str == "mcp" {
                    active_mcp_servers.extend(mcp_tools_by_server.keys().cloned());
                } else if extra_query.is_none()
                    && cat_str
                        .parse::<crate::tools::category::ToolCategory>()
                        .is_ok()
                {
                    if let Ok(cat) = cat_str.parse::<crate::tools::category::ToolCategory>() {
                        active_categories.insert(cat);
                    }
                } else if extra_query.is_none()
                    && (cat_str.starts_with("mcp:") || mcp_tools_by_server.contains_key(cat_str))
                {
                    let server = cat_str.strip_prefix("mcp:").unwrap_or(cat_str);
                    active_mcp_servers.insert(server.to_string());
                } else {
                    for part in cat_str
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                    {
                        if let Ok(cat) = part.parse::<crate::tools::category::ToolCategory>() {
                            active_categories.insert(cat);
                        }
                    }
                    let search_target = extra_query.filter(|q| !q.is_empty()).unwrap_or(cat_str);
                    for matched in crate::tools::category::search_individual_tools(
                        search_target,
                        mcp_tools_by_server,
                    ) {
                        active_tool_names.insert(matched.name);
                    }
                }
                *tools = self.rebuild_active_tools(
                    user_prompt,
                    active_categories,
                    mcp_tools_by_server,
                    active_mcp_servers,
                    active_tool_names,
                );
                tool_result.output.push_str(&format!(
                    "\n[Tool Registry Telemetry: {} Active Tool Schemas in Context / {} Built-in Domain Tools]",
                    tools.len(),
                    crate::constants::TOTAL_TOOL_COUNT
                ));
                tracing::info!(
                    "Dynamically activated '{}'; active schemas count now {}",
                    cat_str,
                    tools.len()
                );
            }
        } else if tool_result.success
            && !tools.iter().any(|t| t.name == tool_call.name)
            && active_tool_names.insert(tool_call.name.clone())
        {
            *tools = self.rebuild_active_tools(
                user_prompt,
                active_categories,
                mcp_tools_by_server,
                active_mcp_servers,
                active_tool_names,
            );
        } else if !tool_result.success
            && tool_result.output.contains("Tool '")
            && tool_result.output.contains("not found")
        {
            let mut added = false;
            for matched in crate::tools::category::search_individual_tools(
                &tool_call.name,
                mcp_tools_by_server,
            ) {
                if active_tool_names.insert(matched.name) {
                    added = true;
                }
            }
            if added {
                *tools = self.rebuild_active_tools(
                    user_prompt,
                    active_categories,
                    mcp_tools_by_server,
                    active_mcp_servers,
                    active_tool_names,
                );
            }
        }

        let res_event = AgentEvent::ToolResult {
            turn_id,
            tool_id: tool_result.tool_id.clone(),
            tool: tool_result.tool_name.clone(),
            success: tool_result.success,
            output: tool_result.display_output().to_string(),
            duration_ms: tool_result.duration_ms,
        };
        if let Err(e) = self
            .session_store
            .append_event(&self.session_id, &res_event)
        {
            tracing::warn!("Failed to persist ToolResult event: {}", e);
        }
        event_sender.send(res_event)?;

        let mut had_mutation = false;
        let mut total_reconciled = 0usize;
        let mut plan_emitted = false;
        let is_noop_patch = tool_result.output.starts_with("No changes needed in ");
        if tool_result.success
            && FILE_MODIFYING_TOOLS.contains(&tool_call.name.as_str())
            && !is_noop_patch
        {
            had_mutation = true;
            let _ =
                std::fs::remove_file(self.workspace_root.join(".minicode/plan/last_verify.json"));
            if let Some(path_str) = crate::tools::param::opt_path(&tool_call.arguments) {
                let p_owned = path_str.to_string();
                let is_internal_minicode =
                    p_owned.starts_with(".minicode/") || p_owned.contains("/.minicode/");
                if !is_internal_minicode && !turn_files_modified.contains(&p_owned) {
                    turn_files_modified.push(p_owned.clone());
                }
                let (created, modified) = if file_existed_before {
                    (Vec::new(), vec![p_owned.clone()])
                } else {
                    (vec![p_owned.clone()], Vec::new())
                };
                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                    &self.workspace_root,
                );
                let reconciled = wm.reconcile_workspace_tasks(&[p_owned]);
                if reconciled > 0 {
                    total_reconciled += reconciled;
                    self.emit_current_plan(Some(turn_id), event_sender);
                    plan_emitted = true;
                }
                if let Some(ref mut ledger) = self.intent_ledger {
                    let mut updated =
                        ledger.update_from_file_activity(&created, &modified, turn_id);
                    if reconciled > 0 {
                        updated += ledger.sync_with_working_memory(&wm, turn_id);
                    }
                    if updated > 0 {
                        *items_progressed_this_turn = true;
                    }
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    if let Err(e) = ledger.save_to_disk(&persistence_path) {
                        tracing::warn!(
                            error = %e,
                            path = %persistence_path.display(),
                            "Failed to persist intent ledger after tool execution"
                        );
                    }
                }
            } else if tool_call.name == "write_file" {
                self.emit_current_plan(Some(turn_id), event_sender);
                plan_emitted = true;
            }
        } else if tool_result.success
            && (tool_call.name == "update_progress" || tool_call.name == "archive_plan")
        {
            let wm =
                crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
            if let Some(ref mut ledger) = self.intent_ledger {
                let synced = ledger.sync_with_working_memory(&wm, turn_id);
                if synced > 0 {
                    *items_progressed_this_turn = true;
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    let _ = ledger.save_to_disk(&persistence_path);
                }
            }
        }

        if tool_result.success
            && (tool_call.name == "create_plan" || tool_call.name == "power_plan")
        {
            self.emit_current_plan(Some(turn_id), event_sender);
            plan_emitted = true;
        } else if tool_result.success
            && (tool_call.name == "kit_stack_add"
                || tool_call.name == "block_scaffold"
                || tool_call.name == "block_insert")
        {
            had_mutation = true;
            let _ =
                std::fs::remove_file(self.workspace_root.join(".minicode/plan/last_verify.json"));
            let wm =
                crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
            let auto_reconciled_extra = wm.reconcile_workspace_tasks(&[]);
            if auto_reconciled_extra > 0 {
                total_reconciled += auto_reconciled_extra;
                self.emit_current_plan(Some(turn_id), event_sender);
                plan_emitted = true;
            }
        }

        let subagent_modified = crate::agent::subagent::relay::get_global_relay_store()
            .drain_modified_files(&self.workspace_root);
        if !subagent_modified.is_empty() {
            had_mutation = true;
            let _ =
                std::fs::remove_file(self.workspace_root.join(".minicode/plan/last_verify.json"));
            for p in &subagent_modified {
                let is_internal_minicode = p.starts_with(".minicode/") || p.contains("/.minicode/");
                if !is_internal_minicode && !turn_files_modified.contains(p) {
                    turn_files_modified.push(p.clone());
                }
            }
            let wm =
                crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
            let reconciled = wm.reconcile_workspace_tasks(&subagent_modified);
            if reconciled > 0 {
                total_reconciled += reconciled;
                self.emit_current_plan(Some(turn_id), event_sender);
                plan_emitted = true;
            }
            if let Some(ref mut ledger) = self.intent_ledger {
                let mut updated =
                    ledger.update_from_file_activity(&[], &subagent_modified, turn_id);
                if reconciled > 0 {
                    updated += ledger.sync_with_working_memory(&wm, turn_id);
                }
                if updated > 0 {
                    *items_progressed_this_turn = true;
                }
                let persistence_path = self
                    .workspace_root
                    .join(&self.config.agent.intent.persistence_file);
                let _ = ledger.save_to_disk(&persistence_path);
            }
        }

        if tool_result.success
            && !crate::tools::concurrency::classify_tool(&tool_call.name).is_read_only()
        {
            let vault_store = crate::vault::store::VaultStore::new(&self.workspace_root);
            let _ = vault_store.sync_project_active_skills(turn_files_modified);
            if tool_call.name == "kit_stack_add" {
                if crate::blocks::seed::ProjectStackInfo::detect(&self.workspace_root)
                    .framework
                    .is_some()
                {
                    active_categories.insert(crate::tools::category::ToolCategory::Web);
                }
                *tools = self.rebuild_active_tools(
                    user_prompt,
                    active_categories,
                    mcp_tools_by_server,
                    active_mcp_servers,
                    active_tool_names,
                );
                tracing::info!(
                    tool = %tool_call.name,
                    "Dynamic system instruction and active tool schemas refreshed following stack scaffold"
                );
            }
        }

        // Append tool result message for LLM context with smart observation pruning (JSON + Logs)
        let mut output_for_llm = crate::context::budget::ObservationPruner::prune_for_llm(
            &tool_call.name,
            &tool_result.output,
        );
        if tool_result.success {
            if crate::context::dedup::FetchCallTracker::should_invalidate_local_reads(
                &tool_call.name,
            ) {
                self.fetch_tracker.invalidate_local_reads();
            } else {
                output_for_llm = self.fetch_tracker.record_and_annotate(
                    &tool_call.name,
                    &tool_call.arguments,
                    output_for_llm,
                );
            }
            self.fetch_tracker
                .record_preloaded_screenshot(&tool_call.name, &tool_call.arguments);
        } else if tool_result.output.trim().len() >= 16 {
            let store = crate::vault::store::VaultStore::new(&self.workspace_root);
            let matched_gotchas = store.find_relevant_gotchas(&tool_result.output);
            if !matched_gotchas.is_empty() {
                output_for_llm.push_str("\n\n<universal_learned_gotchas>\n");
                output_for_llm
                    .push_str("  Matching Pitfalls & Verified Fixes from Universal Vault:\n");
                for g in matched_gotchas.iter().take(2) {
                    output_for_llm.push_str(&format!(
                        "  {}\n",
                        g.format_prompt_block().replace('\n', "\n  ")
                    ));
                }
                output_for_llm.push_str("</universal_learned_gotchas>");
            }
        }

        if tool_result.success && tool_call.name == "kit_stack_add" {
            if let Some(ui_enrichment) =
                crate::agent::orchestrator::WorkflowRouter::enrich_ui_design(
                    &self.workspace_root,
                    "",
                )
            {
                output_for_llm.push_str("\n\n");
                output_for_llm.push_str(&ui_enrichment);
            }
        }

        if tool_result.success
            && (tool_call.name == "create_plan" || tool_call.name == "power_plan")
        {
            if let Some(post_plan_skills) =
                crate::agent::orchestrator::WorkflowRouter::enrich_post_plan_execution_context(
                    &self.workspace_root,
                    user_prompt,
                )
            {
                output_for_llm.push_str("\n\n");
                output_for_llm.push_str(&post_plan_skills);
            }
        }

        if inject_step_grounding && tool_call.name != "update_progress" {
            let wm =
                crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
            let has_ui_framework =
                crate::blocks::seed::ProjectStackInfo::detect(&self.workspace_root)
                    .framework
                    .is_some();
            let miniblocks_hint = if has_ui_framework {
                " Use `block_search`/`block_scaffold` for UI."
            } else {
                ""
            };
            if total_reconciled > 0 {
                let rem_hint = wm.format_remaining_tasks_hint();
                if let Some(summary) = wm.read_active_plan_summary() {
                    if let Some(ref next_active) = summary.active_task {
                        output_for_llm.push_str(&format!(
                            "\n\n{}\n[Plan Auto-Advancement: Steps 1..={} are ALREADY marked [x] ({}/{} completed — DO NOT call `update_progress` on them). Active step [>] is now #{}: \"{}\". Focus ONLY on this step.{}]",
                            rem_hint,
                            summary.completed_tasks,
                            summary.completed_tasks,
                            summary.total_tasks,
                            summary.completed_tasks.saturating_add(1),
                            next_active,
                            miniblocks_hint
                        ));
                    } else if summary.completed_tasks == summary.total_tasks
                        && summary.total_tasks > 0
                    {
                        output_for_llm.push_str(&format!(
                            "\n\n{}\n[Plan Auto-Advancement: All {} tasks are ALREADY marked [x] (100% — DO NOT call `update_progress`). Next: run `power_review` + `power_verify`, then `archive_plan`.]",
                            rem_hint,
                            summary.total_tasks
                        ));
                    }
                }
            } else if let Some(summary) = wm.read_active_plan_summary() {
                if let Some(ref active) = summary.active_task {
                    output_for_llm.push_str(&format!(
                        "\n\n[Active Step ({}/{}): [>] \"{}\". Continue implementing the remaining deliverables for this step (plan status auto-advances when all step deliverables exist on disk).{}]",
                        summary.completed_tasks.saturating_add(1),
                        summary.total_tasks,
                        active,
                        miniblocks_hint
                    ));
                }
            }
        }

        let is_fetch_blocked = output_for_llm.starts_with("<fetch-blocked");
        let mut tool_msg =
            Message::tool_result(tool_call.id, tool_call.name.clone(), output_for_llm);
        if tool_result.success && !is_fetch_blocked {
            let tool_images = crate::agent::subagent::worker::extract_tool_image_attachments(
                &self.workspace_root,
                &tool_call.name,
                &tool_call.arguments,
            );
            if !tool_images.is_empty() {
                tool_msg = tool_msg.with_images(tool_images);
                let mut seen_image_tools = 0usize;
                for prev in self.messages.iter_mut().rev() {
                    if prev.role == crate::agent::types::Role::Tool
                        && prev.images.as_ref().is_some_and(|imgs| !imgs.is_empty())
                    {
                        seen_image_tools += 1;
                        if seen_image_tools >= 6 {
                            prev.images = None;
                        }
                    }
                }
            }
        }
        self.messages.push(tool_msg);

        if inject_step_grounding
            && tool_result.success
            && crate::context::budget::micro_compact::is_mutation_tool(&tool_call.name)
        {
            let micro_metrics =
                crate::context::budget::MicroCompactor::compact_messages(&mut self.messages, 1);
            if micro_metrics.tokens_saved_estimate > 0 {
                tracing::info!(
                    superseded_reads = micro_metrics.superseded_reads_compacted,
                    duplicate_reads = micro_metrics.duplicate_reads_compacted,
                    mutation_echoes = micro_metrics.mutation_echoes_compacted,
                    search_results = micro_metrics.search_results_compacted,
                    tokens_saved = micro_metrics.tokens_saved_estimate,
                    "Applied semantic micro-compaction after file mutation"
                );
            }
        }

        if !plan_emitted
            && tool_result.success
            && (tool_call.name.contains("plan")
                || tool_call.name == "update_progress"
                || tool_call.arguments.to_string().contains("todo.md"))
        {
            self.emit_current_plan(Some(turn_id), event_sender);
        }

        turn_tool_results.push(tool_result);
        Ok((had_mutation, total_reconciled))
    }
}
