//! Post-tool execution pipeline (`finalize_executed_tool`) for `AgentLoop`.

use super::AgentLoop;
use crate::agent::provider::CompletionOptions;
use crate::agent::stuck_detector::BreakerAction;
use crate::agent::types::{AgentEvent, Message, ToolCall};
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
                let manifest_skills: Vec<String> =
                    std::fs::read_to_string(self.workspace_root.join("minikit.json"))
                        .ok()
                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                        .and_then(|val| {
                            val.get("active_skills")
                                .and_then(|arr| arr.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(str::to_string))
                                        .collect::<Vec<_>>()
                                })
                        })
                        .unwrap_or_default();
                let stack_skills = if !manifest_skills.is_empty() {
                    manifest_skills
                } else {
                    crate::vault::store::VaultStore::new(&self.workspace_root)
                        .resolve_turn_active_skills(
                            user_prompt,
                            Some("kit_stack_add"),
                            &Vec::<String>::new(),
                        )
                };
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
                    let has_explicit_query = extra_query.is_some_and(|q| !q.trim().is_empty());
                    let search_target = extra_query.filter(|q| !q.is_empty()).unwrap_or(cat_str);
                    let matched_tools = crate::tools::category::search_individual_tools(
                        search_target,
                        mcp_tools_by_server,
                    );
                    let mut already_active_cats = Vec::new();
                    if !(has_explicit_query && !matched_tools.is_empty()) {
                        for part in cat_str
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                        {
                            if let Ok(cat) = part.parse::<crate::tools::category::ToolCategory>() {
                                if !active_categories.insert(cat) {
                                    already_active_cats.push(part.to_string());
                                }
                            }
                        }
                    }
                    for matched in matched_tools {
                        active_tool_names.insert(matched.name);
                    }
                    if !already_active_cats.is_empty() {
                        tool_result.output.push_str(&format!(
                            "\n[Note: Category '{}' was already active in context — avoid redundant `activate_tools` calls.]",
                            already_active_cats.join(", ")
                        ));
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
        if tool_result.success && crate::tools::meta::is_file_mutation(&tool_call.name) {
            had_mutation = true;
            let _ =
                std::fs::remove_file(self.workspace_root.join(".minicode/plan/last_verify.json"));
            let _ =
                std::fs::remove_file(self.workspace_root.join(".minicode/plan/last_review.json"));
            if let Some(path_str) = crate::tools::param::opt_path(&tool_call.arguments) {
                let p_owned = path_str.to_string();
                let is_internal_minicode =
                    p_owned.starts_with(".minicode/") || p_owned.contains("/.minicode/");
                if !is_internal_minicode {
                    self.record_single_file_fingerprint(&p_owned);
                    if !turn_files_modified.contains(&p_owned) {
                        turn_files_modified.push(p_owned.clone());
                    }
                }
                let (created, modified) = if file_existed_before {
                    (Vec::new(), vec![p_owned.clone()])
                } else {
                    (vec![p_owned.clone()], Vec::new())
                };
                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                    &self.workspace_root,
                );
                wm.invalidate_verification_evidence_on_code_mutation(std::slice::from_ref(
                    &p_owned,
                ));
                let reopened =
                    wm.reopen_completed_plan_on_code_mutation(std::slice::from_ref(&p_owned));
                let reconciled = if inject_step_grounding && !reopened {
                    wm.reconcile_preceding_tasks_on_step_transition(&[p_owned])
                } else {
                    0
                };
                if reopened || reconciled > 0 {
                    total_reconciled += reconciled;
                    self.emit_current_plan(Some(turn_id), event_sender);
                    plan_emitted = true;
                }
                if let Some(ref mut ledger) = self.intent_ledger {
                    let mut updated =
                        ledger.update_from_file_activity(&created, &modified, turn_id);
                    if reopened || reconciled > 0 {
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
            if tool_call.name == "archive_plan" {
                for prev in &mut self.messages {
                    if prev.role == crate::agent::types::Role::Tool {
                        prev.images = None;
                    }
                }
            }
            let wm =
                crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
            let persistence_path = self
                .workspace_root
                .join(&self.config.agent.intent.persistence_file);
            if let Ok(disk_ledger) =
                crate::context::memory::intent::IntentLedger::load_from_disk(&persistence_path)
            {
                self.intent_ledger = Some(disk_ledger);
            }
            if let Some(ref mut ledger) = self.intent_ledger {
                let synced = ledger.sync_with_working_memory(&wm, turn_id);
                if synced > 0 {
                    *items_progressed_this_turn = true;
                    let _ = ledger.save_to_disk(&persistence_path);
                }
            }
        }

        if tool_result.success
            && (tool_call.name == "create_plan" || tool_call.name == "power_plan")
        {
            let persistence_path = self
                .workspace_root
                .join(&self.config.agent.intent.persistence_file);
            if let Ok(disk_ledger) =
                crate::context::memory::intent::IntentLedger::load_from_disk(&persistence_path)
            {
                self.intent_ledger = Some(disk_ledger);
            }
            *tools = self.rebuild_active_tools(
                user_prompt,
                active_categories,
                mcp_tools_by_server,
                active_mcp_servers,
                active_tool_names,
            );
            self.emit_current_plan(Some(turn_id), event_sender);
            plan_emitted = true;
        } else if !crate::tools::is_read_only(&tool_call.name)
            && (!crate::tools::meta::is_file_mutation(&tool_call.name)
                || crate::tools::param::opt_path(&tool_call.arguments).is_none())
        {
            let newly_changed = self.sync_workspace_mutations(turn_files_modified);
            let is_scaffold_tool = tool_result.success
                && matches!(
                    tool_call.name.as_str(),
                    "kit_stack_add" | "block_scaffold" | "block_insert"
                );
            if !newly_changed.is_empty() || is_scaffold_tool {
                had_mutation = true;
                let _ = std::fs::remove_file(
                    self.workspace_root.join(".minicode/plan/last_verify.json"),
                );
                let _ = std::fs::remove_file(
                    self.workspace_root.join(".minicode/plan/last_review.json"),
                );
                if !newly_changed.is_empty() {
                    self.compactor
                        .record_external_files_modified(&newly_changed);
                    crate::context::graph::CodeGraph::invalidate_cache();
                    crate::agent::verification_barrier::SessionQualityTelemetry::record_external_workspace_mutations(
                        &self.workspace_root,
                        &newly_changed,
                    );
                    for rel_p in &newly_changed {
                        self.record_file_access(rel_p);
                        let mod_ev = AgentEvent::FileModified {
                            turn_id,
                            path: rel_p.clone(),
                            action: "modified".to_string(),
                            backup: String::new(),
                        };
                        let _ = self.session_store.append_event(&self.session_id, &mod_ev);
                        let _ = event_sender.send(mod_ev);
                    }
                }
                let wm = crate::context::memory::working_memory::WorkingMemory::new(
                    &self.workspace_root,
                );
                let mut reopened_extra = false;
                if !is_scaffold_tool && !newly_changed.is_empty() {
                    wm.invalidate_verification_evidence_on_code_mutation(&newly_changed);
                    reopened_extra = wm.reopen_completed_plan_on_code_mutation(&newly_changed);
                }
                let auto_reconciled_extra = if reopened_extra {
                    0
                } else {
                    wm.reconcile_preceding_tasks_on_step_transition(&newly_changed)
                };
                if reopened_extra || auto_reconciled_extra > 0 {
                    total_reconciled += auto_reconciled_extra;
                    self.emit_current_plan(Some(turn_id), event_sender);
                    plan_emitted = true;
                }
                if !newly_changed.is_empty() {
                    if let Some(ref mut ledger) = self.intent_ledger {
                        let mut updated =
                            ledger.update_from_file_activity(&[], &newly_changed, turn_id);
                        if reopened_extra || auto_reconciled_extra > 0 {
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
            }
        }

        let subagent_modified = crate::agent::subagent::relay::get_global_relay_store()
            .drain_modified_files(&self.workspace_root);
        if !subagent_modified.is_empty() {
            had_mutation = true;
            let _ =
                std::fs::remove_file(self.workspace_root.join(".minicode/plan/last_verify.json"));
            let _ =
                std::fs::remove_file(self.workspace_root.join(".minicode/plan/last_review.json"));
            self.compactor
                .record_external_files_modified(&subagent_modified);
            crate::context::graph::CodeGraph::invalidate_cache();
            crate::agent::verification_barrier::SessionQualityTelemetry::record_external_workspace_mutations(
                &self.workspace_root,
                &subagent_modified,
            );
            for p in &subagent_modified {
                let is_internal_minicode = p.starts_with(".minicode/") || p.contains("/.minicode/");
                if !is_internal_minicode && !turn_files_modified.contains(p) {
                    turn_files_modified.push(p.clone());
                }
            }
            let wm =
                crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
            wm.invalidate_verification_evidence_on_code_mutation(&subagent_modified);
            let reopened_sub = wm.reopen_completed_plan_on_code_mutation(&subagent_modified);
            let reconciled = if reopened_sub {
                0
            } else {
                wm.reconcile_preceding_tasks_on_step_transition(&subagent_modified)
            };
            if reopened_sub || reconciled > 0 {
                total_reconciled += reconciled;
                self.emit_current_plan(Some(turn_id), event_sender);
                plan_emitted = true;
            }
            if let Some(ref mut ledger) = self.intent_ledger {
                let mut updated =
                    ledger.update_from_file_activity(&[], &subagent_modified, turn_id);
                if reopened_sub || reconciled > 0 {
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

        let effective_verify_ok =
            crate::context::memory::working_memory::WorkingMemory::is_effective_tool_verification_success(
                &tool_call.name,
                &tool_call.arguments,
                tool_result.success,
                &tool_result.output,
            );

        if effective_verify_ok && tool_call.name != "update_progress" {
            let wm =
                crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
            let sub_reconciled = wm.reconcile_verification_substep_on_tool_output(
                &tool_call.name,
                &tool_call.arguments,
                tool_result.success,
                &tool_result.output,
            );
            if sub_reconciled > 0 {
                total_reconciled += sub_reconciled;
                *items_progressed_this_turn = true;
                self.emit_current_plan(Some(turn_id), event_sender);
                plan_emitted = true;
            }
            wm.record_verification_tool_activity_with_output(
                &tool_call.name,
                &tool_call.arguments,
                tool_result.success,
                &tool_result.output,
            );
        }

        if tool_result.success
            && !crate::tools::concurrency::classify_tool(&tool_call.name).is_read_only()
        {
            let vault_store = crate::vault::store::VaultStore::new(&self.workspace_root);
            let _ = vault_store.sync_project_active_skills(turn_files_modified);
            if tool_call.name == "kit_stack_add" {
                let mut auto_cats = Vec::new();
                if crate::blocks::seed::ProjectStackInfo::detect(&self.workspace_root)
                    .framework
                    .is_some()
                {
                    if active_categories.insert(crate::tools::category::ToolCategory::Blocks) {
                        auto_cats.push("blocks");
                    }
                    if active_categories.insert(crate::tools::category::ToolCategory::Web) {
                        auto_cats.push("web");
                    }
                    if active_categories.insert(crate::tools::category::ToolCategory::Dev) {
                        auto_cats.push("dev");
                    }
                }
                *tools = self.rebuild_active_tools(
                    user_prompt,
                    active_categories,
                    mcp_tools_by_server,
                    active_mcp_servers,
                    active_tool_names,
                );
                if !auto_cats.is_empty() {
                    tool_result.output.push_str(&format!(
                        "\n[Tool Registry Telemetry: Auto-activated '{}' tool categories for scaffolded UI stack (`block_*`, `minitask` & `browser_*` are ALREADY active — do NOT call `activate_tools` for them).]",
                        auto_cats.join("', '")
                    ));
                }
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

        if tool_result.success {
            if let Some(transition_context) =
                crate::agent::orchestrator::WorkflowRouter::enrich_post_tool_transition(
                    &self.workspace_root,
                    &tool_call.name,
                    user_prompt,
                )
            {
                output_for_llm.push_str("\n\n");
                output_for_llm.push_str(&transition_context);
            }
        }

        if inject_step_grounding {
            let suffix = self.build_step_grounding_suffix(
                &tool_call.name,
                &tool_call.arguments,
                effective_verify_ok,
                total_reconciled,
            );
            if !suffix.is_empty() {
                output_for_llm.push_str(&suffix);
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
                        if seen_image_tools >= 4 {
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

        let is_noop_progress_call =
            tool_call.name == "update_progress" && tool_result.output.contains("(auto-reconciled");
        if !plan_emitted
            && tool_result.success
            && !is_noop_progress_call
            && (tool_call.name.contains("plan")
                || tool_call.name == "update_progress"
                || matches!(tool_call.name.as_str(), "power_verify" | "minipower_verify")
                || tool_call.arguments.to_string().contains("todo.md"))
        {
            self.emit_current_plan(Some(turn_id), event_sender);
        }

        self.session_active_categories.clone_from(active_categories);
        self.session_active_mcp_servers
            .clone_from(active_mcp_servers);
        self.session_active_tool_names.clone_from(active_tool_names);

        turn_tool_results.push(tool_result);
        Ok((had_mutation, total_reconciled))
    }

    /// Builds the unified active-step readiness and `update_progress` grounding suffix shared by
    /// both sequential (`finalize_executed_tool`) and parallel (`ExecutionStage::Parallel`) stages.
    pub(crate) fn build_step_grounding_suffix(
        &self,
        last_tool_name: &str,
        last_tool_args: &serde_json::Value,
        last_tool_ok: bool,
        total_reconciled: usize,
    ) -> String {
        if matches!(
            last_tool_name,
            "update_progress" | "create_plan" | "power_plan" | "archive_plan"
        ) {
            return String::new();
        }

        use crate::context::memory::working_memory::{ActiveStepReadiness, WorkingMemory};
        let wm = WorkingMemory::new(&self.workspace_root);
        let has_ui_framework = crate::blocks::seed::ProjectStackInfo::detect(&self.workspace_root)
            .framework
            .is_some();
        let miniblocks_hint = if has_ui_framework {
            " Use `block_search`/`block_scaffold` for UI."
        } else {
            ""
        };
        let readiness = wm.active_step_readiness();
        let static_verify_just_passed = last_tool_ok
            && (last_tool_name == "power_verify"
                || (last_tool_name == "exec_cmd"
                    && last_tool_args
                        .get("command")
                        .or_else(|| last_tool_args.get("cmd"))
                        .and_then(|v| v.as_str())
                        .is_some_and(WorkingMemory::step_requires_static_build_or_lint)));
        let browser_qa_just_ran = last_tool_ok
            && WorkingMemory::tool_performs_browser_qa_check(last_tool_name, last_tool_args);

        if total_reconciled > 0 {
            let rem_hint = wm.format_remaining_tasks_hint();
            if let Some(summary) = wm.read_active_plan_summary() {
                if let Some(ref next_active) = summary.active_task {
                    let next_step_num = summary.completed_tasks.saturating_add(1);
                    return match readiness {
                        ActiveStepReadiness::DeliverablesReady => format!(
                            "\n\n{}\n[Plan Catch-Up: Preceding step(s) 1..={} marked [x] ({}/{} completed — do NOT re-update them). MANDATORY STEP CHECKPOINT: Primary deliverables for active step #{} (\"{}\") are on disk — finish any remaining wiring/imports for step #{} first if needed, otherwise call `update_progress(step=\"active\", status=\"completed\")` NOW BEFORE creating or editing files for step #{}!{}]",
                            rem_hint,
                            summary.completed_tasks,
                            summary.completed_tasks,
                            summary.total_tasks,
                            next_step_num,
                            next_active,
                            next_step_num,
                            next_step_num.saturating_add(1),
                            miniblocks_hint
                        ),
                        ActiveStepReadiness::AwaitingVerification => format!(
                            "\n\n{}\n[Plan Catch-Up: Preceding step(s) 1..={} marked [x] ({}/{} completed — do NOT re-update them). Active step [>] is now #{}: \"{}\" — file deliverables are on disk; complete its runtime build/lint/browser verification checks, then call `update_progress(step=\"active\", status=\"completed\")`.{}]",
                            rem_hint,
                            summary.completed_tasks,
                            summary.completed_tasks,
                            summary.total_tasks,
                            next_step_num,
                            next_active,
                            miniblocks_hint
                        ),
                        ActiveStepReadiness::InProgress => format!(
                            "\n\n{}\n[Plan Catch-Up: Preceding step(s) 1..={} marked [x] ({}/{} completed — do NOT re-update them). Active step [>] is now #{}: \"{}\". Finish this step's remaining work and call `update_progress(step=\"active\", status=\"completed\")` immediately before starting the next step.{}]",
                            rem_hint,
                            summary.completed_tasks,
                            summary.completed_tasks,
                            summary.total_tasks,
                            next_step_num,
                            next_active,
                            miniblocks_hint
                        ),
                    };
                } else if summary.completed_tasks == summary.total_tasks && summary.total_tasks > 0
                {
                    return format!(
                        "\n\n{}\n[Plan Catch-Up: All {} tasks are marked [x] (100% — do NOT call `update_progress`). Next: run `power_review` + `power_verify`, then `archive_plan`.]",
                        rem_hint,
                        summary.total_tasks
                    );
                }
            }
        } else if let Some(summary) = wm.read_active_plan_summary() {
            if let Some(ref active) = summary.active_task {
                let active_step_num = summary.completed_tasks.saturating_add(1);
                if readiness != ActiveStepReadiness::DeliverablesReady
                    && static_verify_just_passed
                    && WorkingMemory::step_requires_static_build_or_lint(active)
                    && !WorkingMemory::step_requires_browser_verification(active)
                {
                    return format!(
                        "\n\n[Verification Check Passed ({}/{}): Static verification command succeeded for active step #{}: \"{}\". If all checks in this step passed, call `update_progress(step=\"active\", status=\"completed\")` NOW before moving to the next step!{}]",
                        active_step_num,
                        summary.total_tasks,
                        active_step_num,
                        active,
                        miniblocks_hint
                    );
                }
                if readiness != ActiveStepReadiness::DeliverablesReady
                    && browser_qa_just_ran
                    && WorkingMemory::step_requires_browser_verification(active)
                {
                    return format!(
                        "\n\n[Browser QA Check Completed ({}/{}): Browser inspection ran for active step #{}: \"{}\". If all layout/viewport/interactive checks required by this step passed with zero defects, call `update_progress(step=\"active\", status=\"completed\")` NOW before moving to the next step; otherwise fix any detected issues first!{}]",
                        active_step_num,
                        summary.total_tasks,
                        active_step_num,
                        active,
                        miniblocks_hint
                    );
                }
                return match readiness {
                    ActiveStepReadiness::DeliverablesReady => format!(
                        "\n\n[MANDATORY STEP CHECKPOINT ({}/{}): Primary deliverables for active step #{} (\"{}\") are on disk. If step #{} still needs any remaining wiring/imports/data edits, finish them first; otherwise call `update_progress(step=\"active\", status=\"completed\")` NOW BEFORE creating or editing files for step #{} (never defer `update_progress` calls to the end)!{}]",
                        active_step_num,
                        summary.total_tasks,
                        active_step_num,
                        active,
                        active_step_num,
                        active_step_num.saturating_add(1),
                        miniblocks_hint
                    ),
                    ActiveStepReadiness::AwaitingVerification => format!(
                        "\n\n[Active Step ({}/{}): [>] \"{}\". File deliverables are on disk, and this step also requires runtime build/lint/browser verification — run the verification checks in this step now, then call `update_progress(step=\"active\", status=\"completed\")` once verified.{}]",
                        active_step_num,
                        summary.total_tasks,
                        active,
                        miniblocks_hint
                    ),
                    ActiveStepReadiness::InProgress => format!(
                        "\n\n[Active Step ({}/{}): [>] \"{}\". Complete this step's remaining work and call `update_progress(step=\"active\", status=\"completed\")` immediately once this step is finished BEFORE starting the next step (never defer `update_progress` calls to the end).{}]",
                        active_step_num,
                        summary.total_tasks,
                        active,
                        miniblocks_hint
                    ),
                };
            }
        }

        String::new()
    }
}
