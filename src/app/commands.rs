//! Interactive command and prompt dispatch logic for minicode TUI

use super::{AgentCommand, App};
use crate::ui::modal::ModalState;
use anyhow::Result;
use tokio::sync::mpsc;

#[path = "commands_impl/dev_and_git_commands.rs"]
mod dev_and_git_commands;
#[path = "commands_impl/goal_and_session.rs"]
mod goal_and_session;
#[path = "commands_impl/suite_commands.rs"]
mod suite_commands;

#[cfg(test)]
#[path = "commands_impl/tests.rs"]
mod tests;

#[allow(unused_imports)]
pub use goal_and_session::{format_ledger_timeline, parse_goal_command, GoalSubcommand};

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
        display_prompt: Option<&str>,
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

        if prompt == "/subagents" || prompt == "/swarm" || prompt == "/workers" {
            self.subagent_drawer.toggle();
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
                self.add_timeline_status("ℹ Nothing to copy yet".to_string());
            } else {
                let ok = crate::ui::clipboard::copy_to_clipboard(&text_to_copy);
                if ok {
                    let label = if copy_all {
                        "entire conversation"
                    } else {
                        "latest assistant response"
                    };
                    self.add_timeline_status(format!("✔ Copied {} to clipboard", label));
                } else {
                    self.add_timeline_status("✗ Failed to copy to clipboard".to_string());
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/new" {
            self.timeline = crate::ui::TimelineView::new();
            let _ = control_tx.send(AgentCommand::NewSession);
            self.add_timeline_status("✨ Started a new session".to_string());
            return Ok(CommandAction::Continue);
        }

        let prompt_trimmed = prompt.trim();
        let prompt_lower = prompt_trimmed.to_lowercase();
        if prompt_lower == "/settings"
            || prompt_lower.starts_with("/settings ")
            || prompt_lower == "/config"
            || prompt_lower.starts_with("/config ")
            || prompt_lower == "/preferences"
            || prompt_lower.starts_with("/preferences ")
        {
            let remainder = if let Some(r) = prompt_trimmed.strip_prefix("/settings") {
                r.trim()
            } else if let Some(r) = prompt_trimmed.strip_prefix("/config") {
                r.trim()
            } else {
                prompt_trimmed
                    .strip_prefix("/preferences")
                    .unwrap_or("")
                    .trim()
            };

            if remainder.is_empty() {
                self.modal = ModalState::new_settings(&self.config, &self.workspace_root);
            } else {
                let parts: Vec<&str> = remainder.split_whitespace().collect();
                match parts.as_slice() {
                    ["model", prov, model] => {
                        self.config
                            .provider
                            .default_models
                            .insert((*prov).to_string(), (*model).to_string());
                        if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                            tracing::warn!(error = %e, "Failed to persist default model setting");
                            self.add_timeline_status(format!(
                                "✗ Failed to save configuration: {}",
                                e
                            ));
                        } else {
                            self.add_timeline_status(format!(
                                "✔ Default model for provider '{}' set to '{}'",
                                prov, model
                            ));
                        }
                    }
                    ["auto_approve", val] => {
                        let enabled = matches!(val.to_lowercase().as_str(), "on" | "true" | "1");
                        self.config.agent.auto_approve = enabled;
                        if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                            tracing::warn!(error = %e, "Failed to persist auto-approve setting");
                            self.add_timeline_status(format!(
                                "✗ Failed to save configuration: {}",
                                e
                            ));
                        } else {
                            self.add_timeline_status(format!(
                                "✔ Auto-approve {}",
                                if enabled { "enabled" } else { "disabled" }
                            ));
                        }
                    }
                    ["streaming", val] => {
                        let enabled = matches!(
                            val.to_lowercase().as_str(),
                            "on" | "true" | "1" | "enabled" | "enable"
                        );
                        self.config.agent.streaming = enabled;
                        let _ = self.config.save(Some(&self.workspace_root));
                        let _ = self.config.save(None);
                        self.add_timeline_status(format!(
                            "✔ Streaming {}",
                            if enabled { "enabled" } else { "disabled" }
                        ));
                    }
                    ["effort" | "temp" | "level", val] => {
                        self.config.provider.apply_effort_level(val);
                        let _ = self.config.save(Some(&self.workspace_root));
                        let _ = self.config.save(None);
                        let _ = self.sync_orchestrator_config(control_tx);
                        self.add_timeline_status(format!(
                            "✔ Effort / Temp level set to '{}' for {}/{}",
                            self.config.provider.effective_level_label(),
                            self.config.provider.default,
                            self.config.provider.model
                        ));
                    }
                    ["thinking", tokens_str] => {
                        let max_out = crate::agent::models::get_model_output_limit(
                            &self.config.provider.model,
                        );
                        let budget = match tokens_str.to_lowercase().as_str() {
                            "off" | "none" | "0" | "low" => 0,
                            "medium" | "med" => (max_out / 4).max(1024),
                            "high" => ((max_out * 3) / 4).max(4096),
                            "max" => max_out,
                            other => {
                                if let Some(k) = other.strip_suffix('k') {
                                    k.parse::<usize>().unwrap_or(0) * 1024
                                } else {
                                    other.parse::<usize>().unwrap_or(0)
                                }
                            }
                        };
                        self.config.provider.thinking_budget =
                            if budget == 0 { None } else { Some(budget) };
                        if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                            tracing::warn!(error = %e, "Failed to persist thinking budget setting");
                            self.add_timeline_status(format!(
                                "✗ Failed to save configuration: {}",
                                e
                            ));
                        } else {
                            self.add_timeline_status(format!(
                                "✔ Thinking budget set to {} tokens",
                                budget
                            ));
                        }
                    }
                    _ => {
                        self.modal = ModalState::new_settings(&self.config, &self.workspace_root);
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/configure"
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

        if prompt == "/commands" || prompt == "/command" {
            self.modal = ModalState::new_command_catalog();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/help" {
            self.modal = ModalState::Help;
            return Ok(CommandAction::Continue);
        }

        if let Some(action) = self
            .handle_suite_slash_command(prompt, prompt_trimmed, &prompt_lower, control_tx)
            .await?
        {
            return Ok(action);
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
                self.dispatch_agent_prompt(
                    explore_prompt,
                    crate::ui::AgentActivity::RepoResearch {
                        target: query.to_string(),
                    },
                    control_tx,
                );
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/todo"
            || prompt == "/plan"
            || prompt.starts_with("/todo ")
            || prompt.starts_with("/plan ")
        {
            self.modal = ModalState::new_todo(&self.workspace_root);
            return Ok(CommandAction::Continue);
        }

        if let Some(action) = self
            .handle_goal_and_session_slash_command(prompt, control_tx)
            .await?
        {
            return Ok(action);
        }

        if prompt == "/diff" || prompt == "/diffs" {
            let ws = self.workspace_root.clone();
            match crate::git::GitDiffViewer::load_diffs(&ws, false).await {
                Ok(diff_files) => {
                    self.modal = ModalState::new_git_diff(diff_files, false);
                }
                Err(e) => {
                    self.add_timeline_status(format!("✗ Failed to load git diff: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/review" || prompt.starts_with("/review ") {
            let staged_only = prompt
                .split_whitespace()
                .any(|w| w == "--staged" || w == "-s");

            match crate::git::GitReviewer::review_workspace(&self.workspace_root, staged_only).await
            {
                Ok(report) => {
                    self.modal = ModalState::new_code_review(report, staged_only);
                }
                Err(e) => {
                    self.add_timeline_status(format!("✗ Code review error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt_lower == "/model"
            || prompt_lower == "/models"
            || prompt_lower == "/provider"
            || prompt_lower == "/switch"
            || prompt_lower.starts_with("/switch ")
            || prompt_lower.starts_with("/model ")
            || prompt_lower == "/fav"
            || prompt_lower == "/favorites"
            || prompt_lower.starts_with("/fav ")
            || prompt_lower.starts_with("/favorites ")
        {
            let is_fav_cmd = prompt_lower == "/fav"
                || prompt_lower == "/favorites"
                || prompt_lower.starts_with("/fav ")
                || prompt_lower.starts_with("/favorites ");
            let arg = if let Some(r) = prompt_trimmed.strip_prefix("/favorites") {
                r.trim()
            } else if let Some(r) = prompt_trimmed.strip_prefix("/fav") {
                r.trim()
            } else if let Some(r) = prompt_trimmed.strip_prefix("/switch") {
                r.trim()
            } else if let Some(r) = prompt_trimmed.strip_prefix("/model") {
                r.trim()
            } else {
                ""
            };
            let arg_lower = arg.to_lowercase();

            if is_fav_cmd && (arg_lower == "add" || arg_lower.starts_with("add ")) {
                let target = arg.strip_prefix("add").unwrap_or("").trim();
                let (prov, mdl) = if target.is_empty() {
                    (
                        self.config.provider.default.clone(),
                        self.config.provider.model.clone(),
                    )
                } else if let Some((p, m)) = target.split_once('/') {
                    (p.trim().to_string(), m.trim().to_string())
                } else {
                    (
                        self.config.provider.default.clone(),
                        target.trim().to_string(),
                    )
                };
                if !self.config.is_favorite_model(&prov, &mdl) {
                    self.config.toggle_favorite_model(&prov, &mdl);
                    let _ = self.config.save(Some(&self.workspace_root));
                    let _ = self.config.save(None);
                }
                self.toasts
                    .push_status(format!("Saved '{}/{}' to favorite models", prov, mdl));
                return Ok(CommandAction::Continue);
            }

            if is_fav_cmd
                && (arg_lower == "rm"
                    || arg_lower == "remove"
                    || arg_lower.starts_with("rm ")
                    || arg_lower.starts_with("remove "))
            {
                let target = arg
                    .strip_prefix("remove")
                    .or_else(|| arg.strip_prefix("rm"))
                    .unwrap_or("")
                    .trim();
                let (prov, mdl) = if target.is_empty() {
                    (
                        self.config.provider.default.clone(),
                        self.config.provider.model.clone(),
                    )
                } else if let Some((p, m)) = target.split_once('/') {
                    (p.trim().to_string(), m.trim().to_string())
                } else {
                    (
                        self.config.provider.default.clone(),
                        target.trim().to_string(),
                    )
                };
                if self.config.is_favorite_model(&prov, &mdl) {
                    self.config.toggle_favorite_model(&prov, &mdl);
                    let _ = self.config.save(Some(&self.workspace_root));
                    let _ = self.config.save(None);
                    self.toasts
                        .push_status(format!("Removed '{}/{}' from favorite models", prov, mdl));
                } else {
                    self.toasts
                        .push_status(format!("'{}/{}' is not in favorite models", prov, mdl));
                }
                return Ok(CommandAction::Continue);
            }

            if is_fav_cmd || arg_lower == "fav" || arg_lower == "favorites" {
                if self.config.provider.favorite_models.is_empty() {
                    self.toasts.push_status(
                        "No favorite models saved yet — use `/fav add` or press `*` / `Ctrl+F` in `/switch`".to_string(),
                    );
                    self.modal = ModalState::new_provider_select();
                } else {
                    let fav_models: Vec<crate::agent::models::ModelInfo> = self
                        .config
                        .provider
                        .favorite_models
                        .iter()
                        .map(|slot| {
                            let full_id = format!("{}/{}", slot.provider, slot.model);
                            crate::agent::models::ModelInfo {
                                id: full_id.clone(),
                                name: full_id,
                                description: Some("[fav]".to_string()),
                                context_length: Some(
                                    crate::agent::models::get_model_context_limit(&slot.model),
                                ),
                                is_free: false,
                            }
                        })
                        .collect();
                    self.modal = ModalState::new_model_select("favorites".to_string(), fav_models);
                }
                return Ok(CommandAction::Continue);
            }

            if !arg.is_empty() {
                if let Some((prov, mdl)) = arg.split_once('/') {
                    let prov = prov.trim().to_string();
                    let mdl = mdl.trim().to_string();
                    if !prov.is_empty() && !mdl.is_empty() {
                        self.config.provider.default = prov.clone();
                        self.config.provider.model = mdl.clone();
                        self.config
                            .provider
                            .default_models
                            .insert(prov.clone(), mdl.clone());
                        let _ = self.sync_single_provider_config(control_tx);
                        let _ = self.config.save(Some(&self.workspace_root));
                        self.toasts.push_status(format!(
                            "✔ Switched active provider to '{}' and model to '{}'",
                            prov, mdl
                        ));
                        return Ok(CommandAction::Continue);
                    }
                }
            }

            self.modal = ModalState::new_provider_select();
            return Ok(CommandAction::Continue);
        }

        if prompt_lower == "/temp"
            || prompt_lower == "/effort"
            || prompt_lower == "/temperature"
            || prompt_lower.starts_with("/temp ")
            || prompt_lower.starts_with("/effort ")
            || prompt_lower.starts_with("/temperature ")
        {
            let arg = if let Some(r) = prompt_trimmed.strip_prefix("/temperature") {
                r.trim()
            } else if let Some(r) = prompt_trimmed.strip_prefix("/effort") {
                r.trim()
            } else {
                prompt_trimmed.strip_prefix("/temp").unwrap_or("").trim()
            };

            if arg.is_empty() {
                let next_level = self.config.provider.cycle_effort_level();
                let _ = self.config.save(Some(&self.workspace_root));
                let _ = self.config.save(None);
                let _ = self.sync_orchestrator_config(control_tx);
                self.toasts.push_status(format!(
                    "Effort / Temp level set to '{}' ({}/{})",
                    next_level, self.config.provider.default, self.config.provider.model
                ));
            } else {
                let lower = arg.to_lowercase();
                if matches!(
                    lower.as_str(),
                    "low" | "l" | "medium" | "med" | "m" | "high" | "h"
                ) {
                    self.config.provider.apply_effort_level(&lower);
                } else if let Ok(val) = lower.parse::<f32>() {
                    self.config.provider.temperature = val.clamp(0.0, 2.0);
                    self.config.provider.reasoning_effort = None;
                } else {
                    self.toasts.push_status(
                        "Usage: `/temp [low|medium|high|<0.0..2.0>]` (or press Shift+Tab / Alt+T)"
                            .to_string(),
                    );
                    return Ok(CommandAction::Continue);
                }
                let _ = self.config.save(Some(&self.workspace_root));
                let _ = self.config.save(None);
                let _ = self.sync_orchestrator_config(control_tx);
                self.toasts.push_status(format!(
                    "Effort / Temp level set to '{}' ({}/{})",
                    self.config.provider.effective_level_label(),
                    self.config.provider.default,
                    self.config.provider.model
                ));
            }
            return Ok(CommandAction::Continue);
        }

        if prompt_lower == "/caveman" || prompt_lower.starts_with("/caveman ") {
            let mut settings_state = crate::ui::modals::settings::SettingsModalState::from_config(
                &self.config,
                &self.workspace_root,
            );
            settings_state.active_tab = crate::ui::modals::settings::SettingsTab::Autonomy;
            settings_state.selected_index = 1;
            self.modal = ModalState::Settings(settings_state);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/undo" {
            let backup_mgr = crate::session::backup::BackupManager::new(&self.workspace_root);
            let checkpoints = backup_mgr.list_checkpoints();
            self.modal = ModalState::new_undo_checkpoint(checkpoints);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/theme" || prompt == "/themes" {
            self.open_theme_modal(None);
            return Ok(CommandAction::Continue);
        }

        if prompt_lower == "/menu"
            || prompt_lower.starts_with("/menu ")
            || prompt_lower == "/menu-style"
            || prompt_lower.starts_with("/menu-style ")
        {
            let arg = prompt_trimmed
                .strip_prefix("/menu-style")
                .or_else(|| prompt_trimmed.strip_prefix("/menu"))
                .unwrap_or("")
                .trim();
            if arg.is_empty() {
                self.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::MenuStyles));
            } else {
                let norm = arg.to_ascii_lowercase().replace('-', "_");
                if let Some(spec) = crate::ui::intellisense::MENU_STYLE_SPECS.iter().find(|s| {
                    s.id == norm
                        || s.aliases.contains(&norm.as_str())
                        || s.name.to_ascii_lowercase().contains(&norm)
                }) {
                    self.config.ui.menu_style = spec.id.to_string();
                    self.input_dock.menu_style = spec.style;
                    let _ = self.config.save(Some(&self.workspace_root));
                    let _ = self.config.save(None);
                    self.toasts.push(
                        crate::ui::ToastKind::Success,
                        "Intellisense Menu Style Updated",
                        format!("Switched to \"{}\".", spec.name),
                        Some(spec.id.to_string()),
                        Some("menu_style_preview".to_string()),
                    );
                } else {
                    let valid_ids = crate::ui::intellisense::MENU_STYLE_SPECS
                        .iter()
                        .map(|s| s.id)
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.add_timeline_status(format!(
                        "! Unknown menu style '{}'. Available: {}",
                        arg, valid_ids
                    ));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/activity-style" || prompt.starts_with("/activity-style ") {
            let arg = prompt.strip_prefix("/activity-style").unwrap_or("").trim();
            if arg.is_empty() {
                self.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::ActivityStyles));
            } else {
                let lower = arg.to_lowercase();
                if let Some(opt) = crate::ui::modals::theme_select::ACTIVITY_STYLE_OPTIONS
                    .iter()
                    .find(|o| o.id == lower || o.name.to_lowercase().contains(&lower))
                {
                    self.config.ui.activity_style = opt.id.to_string();
                    let _ = self.config.save(Some(&self.workspace_root));
                    let _ = self.config.save(None);
                    self.toasts.push(
                        crate::ui::ToastKind::Success,
                        "Activity Bar Style Updated",
                        format!("Switched to \"{}\".", opt.name),
                        Some(opt.id.to_string()),
                        Some("activity_style_preview".to_string()),
                    );
                } else {
                    let valid_ids = crate::ui::modals::theme_select::ACTIVITY_STYLE_OPTIONS
                        .iter()
                        .map(|o| o.id)
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.add_timeline_status(format!(
                        "! Unknown activity bar style '{}'. Available: {}",
                        arg, valid_ids
                    ));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/toast-style" || prompt.starts_with("/toast-style ") {
            let arg = prompt.strip_prefix("/toast-style").unwrap_or("").trim();
            if arg.is_empty() {
                self.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::ToastStyles));
            } else {
                let lower = arg.to_lowercase();
                if let Some(opt) = crate::ui::modals::theme_select::TOAST_STYLE_OPTIONS
                    .iter()
                    .find(|o| o.id == lower || o.name.to_lowercase().contains(&lower))
                {
                    self.config.ui.toast_style = opt.id.to_string();
                    let _ = self.config.save(Some(&self.workspace_root));
                    let _ = self.config.save(None);
                    self.toasts.push(
                        crate::ui::ToastKind::Success,
                        "Toast Style Updated",
                        format!("Switched to \"{}\".", opt.name),
                        Some(opt.id.to_string()),
                        Some("toast_style_preview".to_string()),
                    );
                } else {
                    let valid_ids = crate::ui::modals::theme_select::TOAST_STYLE_OPTIONS
                        .iter()
                        .map(|o| o.id)
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.add_timeline_status(format!(
                        "⚠️ Unknown toast style '{}'. Available: {}",
                        arg, valid_ids
                    ));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/toast-symbols"
            || prompt.starts_with("/toast-symbols ")
            || prompt == "/toast-symbol"
            || prompt.starts_with("/toast-symbol ")
        {
            let arg = prompt
                .strip_prefix("/toast-symbols")
                .or_else(|| prompt.strip_prefix("/toast-symbol"))
                .unwrap_or("")
                .trim();
            if arg.is_empty() {
                self.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::ToastSymbols));
            } else {
                let lower = arg.to_lowercase();
                if let Some(opt) = crate::ui::modals::theme_select::TOAST_SYMBOL_OPTIONS
                    .iter()
                    .find(|o| o.id == lower || o.name.to_lowercase().contains(&lower))
                {
                    self.config.ui.toast_symbols = opt.id.to_string();
                    let _ = self.config.save(Some(&self.workspace_root));
                    let _ = self.config.save(None);
                    self.toasts.push(
                        crate::ui::ToastKind::Skill,
                        "Toast Symbols Updated",
                        format!("Switched to \"{}\".", opt.name),
                        Some(opt.id.to_string()),
                        Some("toast_symbols_preview".to_string()),
                    );
                } else {
                    let valid_ids = crate::ui::modals::theme_select::TOAST_SYMBOL_OPTIONS
                        .iter()
                        .map(|o| o.id)
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.add_timeline_status(format!(
                        "! Unknown toast symbol theme '{}'. Available: {}",
                        arg, valid_ids
                    ));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/swarm-style" || prompt.starts_with("/swarm-style ") {
            let arg = prompt.strip_prefix("/swarm-style").unwrap_or("").trim();
            if arg.is_empty() {
                self.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::SwarmStyles));
            } else {
                let lower = arg.to_lowercase();
                if let Some(opt) = crate::ui::modals::theme_select::SWARM_STYLE_OPTIONS
                    .iter()
                    .find(|o| o.id == lower || o.name.to_lowercase().contains(&lower))
                {
                    self.config.ui.swarm_style = opt.id.to_string();
                    let _ = self.config.save(Some(&self.workspace_root));
                    let _ = self.config.save(None);
                    self.add_timeline_status(format!(
                        "✔ Swarm flight deck style switched to '{}' ({})",
                        opt.name, opt.id
                    ));
                } else {
                    self.add_timeline_status(format!(
                        "⚠️ Unknown style '{}'. Available: stylish, gitgraph, modern, minimal, cockpit",
                        arg
                    ));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/todo-style" || prompt.starts_with("/todo-style ") {
            let arg = prompt.strip_prefix("/todo-style").unwrap_or("").trim();
            if arg.is_empty() {
                self.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::TodoStyles));
            } else {
                let lower = arg.to_lowercase();
                if let Some(opt) = crate::ui::modals::theme_select::TODO_STYLE_OPTIONS
                    .iter()
                    .find(|o| o.id == lower || o.name.to_lowercase().contains(&lower))
                {
                    self.config.ui.todo_style = opt.id.to_string();
                    let _ = self.config.save(Some(&self.workspace_root));
                    let _ = self.config.save(None);
                    self.add_timeline_status(format!(
                        "✔ Todo widget style switched to '{}' ({})",
                        opt.name, opt.id
                    ));
                } else {
                    self.add_timeline_status(format!(
                        "⚠️ Unknown style '{}'. Available: tree, card, rail, minimal",
                        arg
                    ));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/context"
            || prompt == "/ctx"
            || prompt == "/kv"
            || prompt == "/cache"
            || prompt == "/tokens"
            || prompt == "/compact"
        {
            let data = crate::ui::modals::context_diagnostics::ContextDiagnosticsData::gather(
                &self.workspace_root,
                &self.config,
                self.current_context_tokens(),
                self.cumulative_tokens,
                self.last_turn_cached_tokens,
                self.timeline.entries.len(),
            );
            self.modal = ModalState::new_context_diagnostics(data);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/streaming" {
            self.modal = ModalState::new_streaming_select(self.config.agent.streaming);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/parallel" || prompt.starts_with("/parallel ") {
            let args = prompt.strip_prefix("/parallel").unwrap_or("").trim();
            if args.is_empty() {
                let mut settings_state =
                    crate::ui::modals::settings::SettingsModalState::from_config(
                        &self.config,
                        &self.workspace_root,
                    );
                settings_state.active_tab = crate::ui::modals::settings::SettingsTab::Autonomy;
                self.modal = ModalState::Settings(settings_state);
            } else {
                match args.to_lowercase().as_str() {
                    "on" | "enable" | "true" | "1" => {
                        self.config.agent.parallel_tools = true;
                        self.add_timeline_status("✔ Parallel tool execution enabled.".to_string());
                    }
                    "off" | "disable" | "false" | "0" => {
                        self.config.agent.parallel_tools = false;
                        self.add_timeline_status(
                            "✔ Parallel tool execution disabled (strictly sequential).".to_string(),
                        );
                    }
                    "speculative on" | "speculative true" => {
                        self.config.agent.speculative_execution = true;
                        self.add_timeline_status(
                            "✔ Speculative streaming pre-execution enabled.".to_string(),
                        );
                    }
                    "speculative off" | "speculative false" => {
                        self.config.agent.speculative_execution = false;
                        self.add_timeline_status(
                            "✔ Speculative streaming pre-execution disabled.".to_string(),
                        );
                    }
                    other => {
                        if let Ok(num) = other.parse::<usize>() {
                            let clamped = num.clamp(
                                crate::constants::MIN_PARALLEL_TOOLS,
                                crate::constants::MAX_PARALLEL_TOOLS_CAP,
                            );
                            self.config.agent.max_parallel_tools = clamped;
                            self.add_timeline_status(format!(
                                "✔ Maximum parallel tools limit set to {} concurrent workers.",
                                clamped
                            ));
                        } else {
                            self.add_timeline_status(format!(
                                "⚠️ Invalid argument '{}'. Use `on`, `off`, `speculative on`, `speculative off`, or a number (1-16).",
                                other
                            ));
                        }
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if let Some(action) = self.handle_dev_and_git_slash_command(prompt).await? {
            return Ok(action);
        }

        if prompt == "/thinking" || prompt.starts_with("/thinking ") {
            let args = prompt.strip_prefix("/thinking").unwrap_or("").trim();
            if args.is_empty() {
                let mut settings_state =
                    crate::ui::modals::settings::SettingsModalState::from_config(
                        &self.config,
                        &self.workspace_root,
                    );
                settings_state.active_tab = crate::ui::modals::settings::SettingsTab::Autonomy;
                self.modal = ModalState::Settings(settings_state);
            } else {
                match args.to_lowercase().as_str() {
                    "off" | "disable" | "none" | "0" => {
                        self.config.provider.thinking_budget = None;
                        self.config.provider.reasoning_effort = None;
                        self.add_timeline_status(
                            "✔ Extended thinking / test-time reasoning disabled.".to_string(),
                        );
                    }
                    "4k" => {
                        self.config.provider.thinking_budget = Some(4096);
                        self.config.provider.reasoning_effort = Some("low".to_string());
                        self.add_timeline_status(
                            "✔ Extended thinking budget set to 4,096 tokens (effort: low)."
                                .to_string(),
                        );
                    }
                    "8k" => {
                        self.config.provider.thinking_budget = Some(8192);
                        self.config.provider.reasoning_effort = Some("medium".to_string());
                        self.add_timeline_status(
                            "✔ Extended thinking budget set to 8,192 tokens (effort: medium)."
                                .to_string(),
                        );
                    }
                    "16k" | "default" => {
                        self.config.provider.thinking_budget = Some(16000);
                        self.config.provider.reasoning_effort = Some("medium".to_string());
                        self.add_timeline_status(
                            "✔ Extended thinking budget set to 16,000 tokens (effort: medium)."
                                .to_string(),
                        );
                    }
                    "32k" => {
                        self.config.provider.thinking_budget = Some(32000);
                        self.config.provider.reasoning_effort = Some("high".to_string());
                        self.add_timeline_status(
                            "✔ Extended thinking budget set to 32,000 tokens (effort: high)."
                                .to_string(),
                        );
                    }
                    "64k" | "max" => {
                        self.config.provider.thinking_budget = Some(64000);
                        self.config.provider.reasoning_effort = Some("high".to_string());
                        self.add_timeline_status(
                            "✔ Extended thinking budget set to 64,000 tokens (effort: high)."
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
                            self.add_timeline_status(format!(
                                "✔ Extended thinking budget set to {} tokens (effort: {}).",
                                clamped, effort
                            ));
                        } else {
                            self.add_timeline_status(format!(
                                "⚠️ Invalid thinking budget '{}'. Use off, 4k, 8k, 16k, 32k, 64k, or a number between 1024 and 64000.",
                                other
                            ));
                        }
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/init"
            || prompt == "/index"
            || prompt == "/analyze"
            || prompt == "/map"
            || crate::agent::intent::is_repo_map_analysis_command(prompt)
        {
            self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/arch" || prompt == "/architecture" {
            match crate::context::governance::ArchitectureGovernor::scan_workspace(
                &self.workspace_root,
            ) {
                Ok(report) => {
                    self.modal = ModalState::new_architecture_audit(report);
                }
                Err(e) => {
                    self.add_timeline_status(format!("✗ Failed to audit architecture: {}", e));
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

        let message_to_display = display_prompt.unwrap_or(&prompt_to_run);

        // ====================================================================
        // GATE 1: JIT Provider & Model Authentication Gate
        // If the user hasn't configured a key or selected an active provider,
        // intercept the prompt, preserve it, and display the Setup modal.
        // ====================================================================
        if self.config.provider.default.is_empty() {
            self.pending_submission = Some(crate::app::PendingSubmission {
                prompt: prompt_to_run.clone(),
                display: message_to_display.to_string(),
            });
            self.modal = ModalState::new_provider_setup_required("", &prompt_to_run);
            return Ok(CommandAction::Continue);
        }

        let is_provider_configured = if self.config.is_local_provider(&self.config.provider.default)
        {
            true
        } else {
            match self.config.get_api_key(&self.config.provider.default) {
                Ok(key) => !key.trim().is_empty(),
                Err(_) => false,
            }
        };

        if !is_provider_configured {
            self.pending_submission = Some(crate::app::PendingSubmission {
                prompt: prompt_to_run.clone(),
                display: message_to_display.to_string(),
            });
            self.modal = ModalState::new_provider_setup_required(
                &self.config.provider.default,
                &prompt_to_run,
            );
            return Ok(CommandAction::Continue);
        }

        // ====================================================================
        // GATE 2: JIT Repository CRUD & Codebase Modification Gate
        // On new repos, only prompt for full indexing when the user requests
        // code modifications, file edits, or repository mutations. General
        // questions (e.g. "what is a mutex?", "/help") execute immediately.
        // On existing repos with drift, ask permission before sync/rebuild.
        // ====================================================================
        let matched_intent = crate::agent::intent::match_intent(&prompt_to_run);
        let is_crud = crate::agent::intent::is_repository_crud_intent(
            &prompt_to_run,
            matched_intent.as_ref(),
        );

        if is_crud && !self.is_working {
            let (repo_state, _, _) =
                crate::agent::orchestrator::WorkflowRouter::scan_repository_state(
                    &self.workspace_root,
                );

            // Only prompt for CodeGraph indexing if the repository actually contains source code
            if repo_state == crate::agent::orchestrator::RepoState::ExistingCodebase {
                let graph_file = self.workspace_root.join(".minicode").join("graph.json");
                if !graph_file.exists() && !self.session_skipped_indexing {
                    self.pending_submission = Some(crate::app::PendingSubmission {
                        prompt: prompt_to_run.clone(),
                        display: message_to_display.to_string(),
                    });
                    self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
                    return Ok(CommandAction::Continue);
                } else if graph_file.exists() && !self.session_skipped_drift {
                    let mut graph = crate::context::graph::CodeGraph::new();
                    if graph.load_cached(&self.workspace_root) {
                        if let Ok(drift) = graph.check_drift(&self.workspace_root) {
                            if drift.is_stale {
                                self.pending_submission = Some(crate::app::PendingSubmission {
                                    prompt: prompt_to_run.clone(),
                                    display: message_to_display.to_string(),
                                });
                                self.modal =
                                    ModalState::new_workspace_drift(&self.workspace_root, &drift);
                                return Ok(CommandAction::Continue);
                            } else if drift.total_drift()
                                >= crate::constants::DEFAULT_DRIFT_MINOR_SYNC_THRESHOLD
                            {
                                tracing::info!(
                                    "Seamlessly updating minor code graph drift ({} files changed)",
                                    drift.total_drift()
                                );
                                if graph.incremental_update(&self.workspace_root).is_ok() {
                                    let _ = graph.save_to_disk(&self.workspace_root);
                                }
                            }
                        }
                    } else {
                        // Corrupted or unparseable graph.json: propose fresh workspace re-analysis
                        tracing::warn!(
                            "Existing .minicode/graph.json corrupted or unreadable; proposing workspace re-analysis"
                        );
                        self.pending_submission = Some(crate::app::PendingSubmission {
                            prompt: prompt_to_run.clone(),
                            display: message_to_display.to_string(),
                        });
                        self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
                        return Ok(CommandAction::Continue);
                    }
                }
            }
        }

        // Display the user's actual full prompt in the TUI timeline (expanding any text paste placeholders, while keeping [img N] clean)
        let timeline_display =
            if let Some((before_xml, _)) = prompt_to_run.split_once("\n\n<attached_images>") {
                before_xml.to_string()
            } else {
                prompt_to_run.to_string()
            };
        self.timeline.add_user_message(timeline_display);

        // Check for recognized autonomous intent to notify the user
        if let Some(m) = matched_intent {
            if m.confidence >= 0.85 && m.intent != crate::agent::intent::AgentIntent::GeneralQuery {
                let (icon, label) = m.intent.badge();
                self.timeline
                    .add_status(format!("{} Autonomous Intent: {}", icon, label));
            }
        }

        if self.last_turn_tokens == 0 {
            self.last_turn_tokens = prompt_to_run.len().max(4) / 4;
        }
        self.dispatch_agent_prompt(
            prompt_to_run,
            crate::ui::AgentActivity::Thinking,
            control_tx,
        );

        Ok(CommandAction::Continue)
    }
}
