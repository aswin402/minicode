//! Modal key navigation and modal action handling for minicode TUI

use super::{AgentCommand, App};
use crate::ui::modal::ModalState;
use crate::ui::Theme;
use crossterm::event::{KeyCode, KeyModifiers};
use std::time::Instant;
use tokio::sync::mpsc;

impl<'a> App<'a> {
    /// Handles keyboard interaction within in-TUI modal dialogs
    pub(crate) async fn handle_modal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
        match &mut self.modal {
            ModalState::None => {}
            ModalState::ExitConfirm { selected_yes, .. } => match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    *selected_yes = !*selected_yes;
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.should_exit = true;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.modal = ModalState::None;
                }
                KeyCode::Enter => {
                    if *selected_yes {
                        self.should_exit = true;
                    } else {
                        self.modal = ModalState::None;
                    }
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_exit = true;
                }
                _ => {}
            },
            ModalState::ApiKeyInput {
                provider,
                env_var,
                input,
                cursor,
            } => match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::new_provider_select();
                }
                KeyCode::Left => {
                    *cursor = cursor.saturating_sub(1);
                }
                KeyCode::Right => {
                    if *cursor < input.len() {
                        *cursor += 1;
                    }
                }
                KeyCode::Backspace => {
                    if *cursor > 0 && *cursor <= input.len() {
                        input.remove(*cursor - 1);
                        *cursor -= 1;
                    }
                }
                KeyCode::Char(c) => {
                    input.insert(*cursor, c);
                    *cursor += 1;
                }
                KeyCode::Enter => {
                    let entered_key = input.trim().to_string();
                    let prov_name = provider.clone();
                    let env_name = env_var.clone();

                    if !entered_key.is_empty() {
                        self.config
                            .provider
                            .api_keys
                            .insert(prov_name.clone(), entered_key.clone());
                        std::env::set_var(&env_name, &entered_key);
                        let _ = self.config.save(Some(&self.workspace_root));
                        self.timeline
                            .add_status(format!("✔ Saved API key for '{}'", prov_name));
                    }

                    let custom_url = self.config.get_provider_base_url(&prov_name);
                    let models_res = self
                        .model_fetcher
                        .fetch_models(&prov_name, &entered_key, custom_url.as_deref())
                        .await;
                    let models = match models_res {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!(error = %e, provider = %prov_name, "Failed to fetch live model list");
                            Vec::new()
                        }
                    };

                    if models.is_empty() {
                        let default_model =
                            crate::config::Config::get_default_model_for_provider(&prov_name)
                                .to_string();
                        self.config.provider.default = prov_name.clone();
                        self.config.provider.model = default_model.clone();

                        let key_res = self.config.get_api_key(&prov_name);
                        let (new_prov, prov_err) =
                            crate::agent::provider::create_provider_or_fallback(
                                &prov_name,
                                key_res,
                                custom_url.as_deref(),
                            );
                        let _ = control_tx.send(AgentCommand::UpdateConfig {
                            config: Box::new(self.config.clone()),
                            provider: new_prov,
                        });
                        let _ = self.config.save(Some(&self.workspace_root));

                        if let Some(err) = prov_err {
                            self.timeline.add_status(format!(
                                "⚠️ Switched provider to '{}' ({}), but provider reported: {}",
                                prov_name, default_model, err
                            ));
                        } else {
                            self.timeline.add_status(format!(
                                "✔ Switched active provider to '{}' and model to '{}'",
                                prov_name, default_model
                            ));
                        }
                        self.modal = ModalState::None;
                    } else {
                        self.modal = ModalState::new_model_select(prov_name, models);
                    }
                }
                _ => {}
            },
            ModalState::ProviderSelect {
                providers,
                selected_index,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < providers.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Enter => {
                    let provider = providers[*selected_index].clone();
                    let is_local = self.config.is_local_provider(&provider);
                    let custom_url = self.config.get_provider_base_url(&provider);
                    let api_key = self.config.get_api_key(&provider).unwrap_or_default();

                    // If cloud provider and no key is configured, prompt with in-TUI ApiKeyInput modal
                    if !is_local && api_key.is_empty() {
                        let env_var = match provider.as_str() {
                            "gemini" | "google" => "GEMINI_API_KEY",
                            "anthropic" | "claude" => "ANTHROPIC_API_KEY",
                            "openrouter" => "OPENROUTER_API_KEY",
                            "openai" => "OPENAI_API_KEY",
                            "deepseek" => "DEEPSEEK_API_KEY",
                            "groq" => "GROQ_API_KEY",
                            "together" => "TOGETHER_API_KEY",
                            "minimax" => "MINIMAX_API_KEY",
                            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => "ZHIPU_API_KEY",
                            "mistral" => "MISTRAL_API_KEY",
                            _ => "",
                        };
                        let env_str = if env_var.is_empty() {
                            format!(
                                "{}_API_KEY",
                                provider.to_uppercase().replace(['-', '.'], "_")
                            )
                        } else {
                            env_var.to_string()
                        };
                        self.modal = ModalState::new_api_key_input(provider, env_str, None);
                        return;
                    }

                    // Fetch models (will use static fallbacks if offline)
                    let models_res = self
                        .model_fetcher
                        .fetch_models(&provider, &api_key, custom_url.as_deref())
                        .await;
                    let models = match models_res {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!(error = %e, provider = %provider, "Failed to fetch live model list");
                            Vec::new()
                        }
                    };

                    if models.is_empty() {
                        let default_model =
                            crate::config::Config::get_default_model_for_provider(&provider)
                                .to_string();
                        self.config.provider.default = provider.clone();
                        self.config.provider.model = default_model.clone();

                        let key_res = self.config.get_api_key(&provider);
                        let (new_prov, prov_err) =
                            crate::agent::provider::create_provider_or_fallback(
                                &provider,
                                key_res,
                                custom_url.as_deref(),
                            );
                        let _ = control_tx.send(AgentCommand::UpdateConfig {
                            config: Box::new(self.config.clone()),
                            provider: new_prov,
                        });
                        let _ = self.config.save(Some(&self.workspace_root));

                        if let Some(err) = prov_err {
                            self.timeline.add_status(format!(
                                "⚠️ Switched provider to '{}' ({}), but provider reported: {}",
                                provider, default_model, err
                            ));
                        } else {
                            self.timeline.add_status(format!(
                                "✔ Switched active provider to '{}' and model to '{}'",
                                provider, default_model
                            ));
                        }
                        self.modal = ModalState::None;
                    } else {
                        self.modal = ModalState::new_model_select(provider, models);
                    }
                }
                _ => {}
            },
            ModalState::ModelSelect {
                provider,
                models,
                filtered_indices,
                selected_index,
                filter,
                ..
            } => match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::new_provider_select();
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < filtered_indices.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Backspace => {
                    filter.pop();
                    self.modal.update_filter();
                }
                KeyCode::Char(c) => {
                    filter.push(c);
                    self.modal.update_filter();
                }
                KeyCode::Enter => {
                    if !filtered_indices.is_empty() && *selected_index < filtered_indices.len() {
                        let real_idx = filtered_indices[*selected_index];
                        let selected_model = models[real_idx].id.clone();
                        self.config.provider.default = provider.clone();
                        self.config.provider.model = selected_model.clone();

                        let custom_url = self
                            .config
                            .get_provider_base_url(&self.config.provider.default);
                        let key_res = self.config.get_api_key(&self.config.provider.default);
                        let (new_prov, prov_err) =
                            crate::agent::provider::create_provider_or_fallback(
                                &self.config.provider.default,
                                key_res,
                                custom_url.as_deref(),
                            );

                        let _ = control_tx.send(AgentCommand::UpdateConfig {
                            config: Box::new(self.config.clone()),
                            provider: new_prov,
                        });
                        let _ = self.config.save(Some(&self.workspace_root));

                        if let Some(err) = prov_err {
                            self.timeline.add_status(format!(
                                "⚠️ Switched provider to '{}' and model to '{}', but provider reported: {}",
                                provider, selected_model, err
                            ));
                        } else {
                            self.timeline.add_status(format!(
                                "✔ Switched active provider to '{}' and model to '{}'",
                                provider, selected_model
                            ));
                        }
                    }
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::UndoCheckpoint {
                ref checkpoints,
                ref mut selected_index,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < checkpoints.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Enter => {
                    if !checkpoints.is_empty() && *selected_index < checkpoints.len() {
                        let target_checkpoint = checkpoints[*selected_index].clone();
                        let target_turn_id = target_checkpoint.turn_id;
                        let target_prompt = target_checkpoint.prompt.clone();

                        match crate::session::undo::rollback_to_checkpoint(
                            &self.workspace_root,
                            target_turn_id,
                        ) {
                            Ok(res) => {
                                let backup_mgr = crate::session::backup::BackupManager::new(
                                    &self.workspace_root,
                                );
                                let message_index = backup_mgr
                                    .load_turn_manifest(target_turn_id)
                                    .map(|m| m.message_index)
                                    .unwrap_or(0);

                                let _ = control_tx.send(AgentCommand::Rollback {
                                    target_turn_id,
                                    message_index,
                                });

                                self.timeline.add_status(format!(
                                    "✔ Reverted workspace & conversation to checkpoint: \"{}\" (Turn #{}) [{} file(s) restored, {} deleted]",
                                    target_prompt, target_turn_id, res.restored_count, res.deleted_count
                                ));
                            }
                            Err(e) => {
                                self.timeline.add_status(format!("✗ Undo failed: {}", e));
                            }
                        }
                    }
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::ThemeSelect {
                ref themes,
                ref mut selected_index,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < themes.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Enter => {
                    if !themes.is_empty() && *selected_index < themes.len() {
                        let chosen = &themes[*selected_index];
                        let chosen_id = chosen.id.clone();
                        let chosen_name = chosen.name.clone();

                        // Apply live theme
                        self.theme = Theme::detect(&chosen_id);
                        self.config.ui.theme = chosen_id;

                        // Persist to configuration file
                        if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                            tracing::warn!("Failed to save theme setting to config: {}", e);
                        }

                        self.timeline.add_status(format!(
                            "✔ Active theme switched to '{}' and saved to config",
                            chosen_name
                        ));
                    }
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::SessionBrowser {
                ref mut sessions,
                ref mut selected_index,
                ref mut cached_summary,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected_index > 0 {
                        *selected_index -= 1;
                        let store = crate::session::store::SessionStore::with_workspace(
                            &self.workspace_root,
                        );
                        if let Some(target) = sessions.get(*selected_index) {
                            *cached_summary = store.get_session_summary(&target.id).ok();
                        }
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if *selected_index + 1 < sessions.len() {
                        *selected_index += 1;
                        let store = crate::session::store::SessionStore::with_workspace(
                            &self.workspace_root,
                        );
                        if let Some(target) = sessions.get(*selected_index) {
                            *cached_summary = store.get_session_summary(&target.id).ok();
                        }
                    }
                }
                KeyCode::Enter => {
                    if !sessions.is_empty() && *selected_index < sessions.len() {
                        let target_id = sessions[*selected_index].id.clone();
                        let store = crate::session::store::SessionStore::with_workspace(
                            &self.workspace_root,
                        );
                        match store.load_session(&target_id) {
                            Ok(events) => {
                                let count = events.len();
                                self.timeline.entries.clear();
                                self.hydrate_session(&events);
                                self.timeline.add_status(format!(
                                    "✔ Loaded session '{}' ({} events)",
                                    target_id, count
                                ));
                            }
                            Err(e) => {
                                self.timeline.add_status(format!(
                                    "✗ Failed to load session '{}': {}",
                                    target_id, e
                                ));
                            }
                        }
                    }
                    self.modal = ModalState::None;
                }
                KeyCode::Char('f') => {
                    if !sessions.is_empty() && *selected_index < sessions.len() {
                        let target_id = sessions[*selected_index].id.clone();
                        let store = crate::session::store::SessionStore::with_workspace(
                            &self.workspace_root,
                        );
                        match store.fork_session(&target_id, &self.workspace_root) {
                            Ok(new_id) => {
                                self.timeline.add_status(format!(
                                    "🌿 Forked session '{}' ➔ new branch '{}'",
                                    target_id, new_id
                                ));
                                if let Ok(events) = store.load_session(&new_id) {
                                    self.timeline.entries.clear();
                                    self.hydrate_session(&events);
                                }
                            }
                            Err(e) => {
                                self.timeline
                                    .add_status(format!("✗ Failed to fork session: {}", e));
                            }
                        }
                    }
                    self.modal = ModalState::None;
                }
                KeyCode::Char('e') => {
                    if !sessions.is_empty() && *selected_index < sessions.len() {
                        let target_id = sessions[*selected_index].id.clone();
                        let store = crate::session::store::SessionStore::with_workspace(
                            &self.workspace_root,
                        );
                        let export_dir = self.workspace_root.join(".minicode").join("exports");
                        let _ = std::fs::create_dir_all(&export_dir);
                        let export_file = export_dir.join(format!("{}.md", target_id));
                        match store.export_markdown(&target_id, &export_file) {
                            Ok(p) => {
                                self.timeline.add_status(format!(
                                    "📄 Exported session Markdown to {}",
                                    p.display()
                                ));
                            }
                            Err(e) => {
                                self.timeline.add_status(format!("✗ Export failed: {}", e));
                            }
                        }
                    }
                    self.modal = ModalState::None;
                }
                KeyCode::Char('d') => {
                    if !sessions.is_empty() && *selected_index < sessions.len() {
                        let target_id = sessions[*selected_index].id.clone();
                        let store = crate::session::store::SessionStore::with_workspace(
                            &self.workspace_root,
                        );
                        match store.delete_session(&target_id) {
                            Ok(true) => {
                                self.timeline
                                    .add_status(format!("🗑️ Deleted session '{}'", target_id));
                                sessions.remove(*selected_index);
                                if *selected_index >= sessions.len() && *selected_index > 0 {
                                    *selected_index = sessions.len() - 1;
                                }
                                if let Some(target) = sessions.get(*selected_index) {
                                    *cached_summary = store.get_session_summary(&target.id).ok();
                                } else {
                                    *cached_summary = None;
                                }
                            }
                            Ok(false) => {
                                self.timeline.add_status(format!(
                                    "ℹ Session '{}' not found on disk",
                                    target_id
                                ));
                            }
                            Err(e) => {
                                self.timeline.add_status(format!(
                                    "✗ Failed to delete session '{}': {}",
                                    target_id, e
                                ));
                            }
                        }
                    }
                }
                _ => {}
            },
            ModalState::Help => {
                if key.code == KeyCode::Esc
                    || key.code == KeyCode::Enter
                    || key.code == KeyCode::Char('q')
                {
                    self.modal = ModalState::None;
                }
            }
            ModalState::StreamingSelect {
                selected_index,
                current_streaming: _,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::BackTab => {
                    if *selected_index == 0 {
                        *selected_index = 2;
                    } else {
                        *selected_index = selected_index.saturating_sub(1);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Tab => {
                    if *selected_index >= 2 {
                        *selected_index = 0;
                    } else {
                        *selected_index += 1;
                    }
                }
                KeyCode::Home => {
                    *selected_index = 0;
                }
                KeyCode::End => {
                    *selected_index = 2;
                }
                KeyCode::Char('1') => {
                    self.config.agent.streaming = true;
                    self.propagate_session_config(control_tx);
                    let _ = self.config.save(Some(&self.workspace_root));
                    self.timeline
                        .add_status("✔ Streaming mode enabled (persisted to config)".to_string());
                    self.modal = ModalState::None;
                }
                KeyCode::Char('2') => {
                    self.config.agent.streaming = false;
                    self.propagate_session_config(control_tx);
                    let _ = self.config.save(Some(&self.workspace_root));
                    self.timeline
                        .add_status("✔ Streaming mode disabled (persisted to config)".to_string());
                    self.modal = ModalState::None;
                }
                KeyCode::Char('3') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Enter => {
                    match *selected_index {
                        0 => {
                            self.config.agent.streaming = true;
                            self.propagate_session_config(control_tx);
                            let _ = self.config.save(Some(&self.workspace_root));
                            self.timeline.add_status(
                                "✔ Streaming mode enabled (persisted to config)".to_string(),
                            );
                        }
                        1 => {
                            self.config.agent.streaming = false;
                            self.propagate_session_config(control_tx);
                            let _ = self.config.save(Some(&self.workspace_root));
                            self.timeline.add_status(
                                "✔ Streaming mode disabled (persisted to config)".to_string(),
                            );
                        }
                        2 => {}
                        _ => {}
                    }
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::CommandCatalog {
                ref filtered_indices,
                ref mut selected_index,
                ref mut filter,
            } => match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < filtered_indices.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Backspace => {
                    filter.pop();
                    self.modal.update_filter();
                }
                KeyCode::Char(c) => {
                    filter.push(c);
                    self.modal.update_filter();
                }
                KeyCode::Enter => {
                    if !filtered_indices.is_empty() && *selected_index < filtered_indices.len() {
                        let real_idx = filtered_indices[*selected_index];
                        let selected_cmd = crate::ui::modal::COMMAND_CATALOG_ITEMS[real_idx].name;
                        self.modal = ModalState::None;

                        match selected_cmd {
                            "/stack" => {
                                self.modal = ModalState::new_stack_select();
                            }
                            "/model" | "/provider" => {
                                self.modal = ModalState::new_provider_select();
                            }
                            "/thinking" => {
                                let current_budget = match self.config.provider.thinking_budget {
                                    Some(b)
                                        if b >= crate::constants::MIN_THINKING_BUDGET_TOKENS =>
                                    {
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
                                     *Run `/thinking [off | 4k | 8k | 16k | 32k | 64k]` in input dock to adjust.*",
                                    self.config.provider.default,
                                    self.config.provider.model,
                                    current_budget,
                                    effort,
                                );
                                self.timeline.add_status(status_msg);
                            }
                            "/theme" => {
                                self.modal = ModalState::new_theme_select(&self.config.ui.theme);
                            }
                            "/explore" => {
                                self.modal = ModalState::new_code_explorer(&self.workspace_root);
                            }
                            "/diff" => {
                                let ws = self.workspace_root.clone();
                                match crate::git::GitDiffViewer::load_diffs(&ws, false).await {
                                    Ok(diff_files) => {
                                        self.modal = ModalState::new_git_diff(diff_files, false);
                                    }
                                    Err(e) => {
                                        self.timeline.add_status(format!(
                                            "✗ Failed to load git diff: {}",
                                            e
                                        ));
                                    }
                                }
                            }
                            "/history" | "/sessions" => {
                                let store = crate::session::store::SessionStore::with_workspace(
                                    &self.workspace_root,
                                );
                                match store.list_sessions_rich() {
                                    Ok(sessions) => {
                                        let initial_summary = sessions
                                            .first()
                                            .and_then(|s| store.get_session_summary(&s.id).ok());
                                        self.modal = ModalState::new_session_browser(
                                            sessions,
                                            initial_summary,
                                        );
                                    }
                                    Err(e) => {
                                        self.timeline.add_status(format!(
                                            "✗ Failed to load sessions: {}",
                                            e
                                        ));
                                    }
                                }
                            }
                            "/undo" => {
                                let backup_mgr = crate::session::backup::BackupManager::new(
                                    &self.workspace_root,
                                );
                                let checkpoints = backup_mgr.list_checkpoints();
                                if checkpoints.is_empty() {
                                    self.timeline.add_status(
                                        "ℹ No recorded checkpoints available to undo".to_string(),
                                    );
                                } else {
                                    self.modal = ModalState::new_undo_checkpoint(checkpoints);
                                }
                            }
                            "/help" => {
                                self.modal = ModalState::Help;
                            }
                            "/terminal" => {
                                self.pty_drawer.toggle();
                            }
                            "/streaming" => {
                                self.modal =
                                    ModalState::new_streaming_select(self.config.agent.streaming);
                            }
                            "/parallel" => {
                                let status_msg = format!(
                                    "⚡ **Speculative Parallel Tool Execution Pipeline**\n\
                                     • **Parallel Tools**: {}\n\
                                     • **Speculative Pre-Execution**: {}\n\
                                     • **Max Concurrency**: {} threads\n\
                                     • **Safety Engine**: Barrier isolation (Mutating & Barrier tools serialized)\n\n\
                                     *Usage:*\n\
                                     • `/parallel on` — Enable parallel tool execution\n\
                                     • `/parallel off` — Disable parallel tool execution\n\
                                     • `/parallel speculative on|off` — Toggle streaming pre-execution\n\
                                     • `/parallel <1-16>` — Set max parallel workers",
                                    if self.config.agent.parallel_tools { "ENABLED" } else { "DISABLED" },
                                    if self.config.agent.speculative_execution { "ENABLED" } else { "DISABLED" },
                                    self.config.agent.max_parallel_tools,
                                );
                                self.timeline.add_status(status_msg);
                                self.modal = ModalState::None;
                            }
                            "/tx" | "/transaction" => {
                                match crate::session::transaction::TransactionManager::status(
                                    &self.workspace_root,
                                    None,
                                ) {
                                    Ok(Some(receipt)) => {
                                        self.timeline.add_status(receipt.format_receipt());
                                    }
                                    Ok(None) => {
                                        self.timeline.add_status("ℹ No active workspace transaction. Use `/tx begin <desc>` or agent tool `begin_transaction` to start one.".to_string());
                                    }
                                    Err(e) => {
                                        self.timeline
                                            .add_status(format!("✗ Transaction error: {}", e));
                                    }
                                }
                                self.modal = ModalState::None;
                            }
                            "/dag" => {
                                self.timeline.add_status(
                                    "⚡ **Dynamic Execution DAG & JSONPath Pipelining (`execute_dag`)**\n\
                                     Compose multiple dependent tool calls into an atomic, wave-scheduled DAG.\n\n\
                                     • Use agent tool `execute_dag` to execute compound tool graphs in a single turn.\n\
                                     • Reference upstream outputs with `$node_id.field` or `${node_id.field}`.\n\
                                     • Dependent tasks are automatically skipped if upstream tasks fail.".to_string()
                                );
                                self.modal = ModalState::None;
                            }
                            "/heal" => {
                                self.timeline.add_status(
                                    "⏳ Running workspace diagnostic triage & self-healing pass..."
                                        .to_string(),
                                );
                                match crate::agent::self_healing::SelfHealingEngine::heal(
                                    &self.workspace_root,
                                    3,
                                    true,
                                    false,
                                )
                                .await
                                {
                                    Ok(report) => {
                                        self.timeline.add_status(
                                            report.format_summary(&self.workspace_root),
                                        );
                                    }
                                    Err(e) => {
                                        self.timeline.add_status(format!(
                                            "✗ Self-healing diagnostic error: {}",
                                            e
                                        ));
                                    }
                                }
                                self.modal = ModalState::None;
                            }
                            "/sandbox" => {
                                let bwrap_status = if crate::sandbox::is_bwrap_available() {
                                    "Bubblewrap (unprivileged namespaces) AVAILABLE"
                                } else {
                                    "Bubblewrap not installed (using Landlock / process isolation)"
                                };
                                self.timeline.add_status(format!(
                                    "🛡️ **Dynamic Code Sandbox (`/sandbox`)**\n\
                                     • **Active Backend**: {}\n\
                                     • **Usage**: Type `/sandbox <command>` or `/sandbox --ephemeral <command>` in prompt.",
                                    bwrap_status
                                ));
                                self.modal = ModalState::None;
                            }
                            "/retrieve" => {
                                self.timeline.add_status(
                                    "ℹ **Usage**: Type `/retrieve <query>` in prompt to execute multi-modal knowledge fusion across CodeGraph, vectors, wiki, and memory.".to_string(),
                                );
                                self.modal = ModalState::None;
                            }
                            "/route" => {
                                let router = crate::agent::router::AdaptiveModelRouter::new();
                                self.timeline.add_status(router.format_status_report());
                                self.modal = ModalState::None;
                            }
                            "/quarantine" => {
                                let store = crate::context::flaky::QuarantineManager::load(
                                    &self.workspace_root,
                                );
                                self.timeline.add_status(
                                    crate::context::flaky::QuarantineManager::format_report(&store),
                                );
                                self.modal = ModalState::None;
                            }
                            "/commit" => {
                                match crate::git::commit_synth::SemanticCommitSynthesizer::synthesize(&self.workspace_root, None, None).await {
                                    Ok(report) => {
                                        self.timeline.add_status(report.format_markdown());
                                    }
                                    Err(e) => {
                                        self.timeline.add_status(format!("❌ Commit synthesis failed: {}", e));
                                    }
                                }
                                self.modal = ModalState::None;
                            }
                            other => {
                                self.input_dock.textarea = tui_textarea::TextArea::default();
                                self.input_dock.textarea.insert_str(other);
                                self.input_dock.textarea.insert_char(' ');
                            }
                        }
                    } else {
                        self.modal = ModalState::None;
                    }
                }
                _ => {}
            },
            ModalState::StackSelect {
                ref stacks,
                ref filtered_indices,
                ref mut selected_index,
                ref mut filter,
            } => match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < filtered_indices.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Backspace => {
                    filter.pop();
                    self.modal.update_filter();
                }
                KeyCode::Char(c) => {
                    filter.push(c);
                    self.modal.update_filter();
                }
                KeyCode::Enter => {
                    if !filtered_indices.is_empty() && *selected_index < filtered_indices.len() {
                        let real_idx = filtered_indices[*selected_index];
                        let selected_stack = &stacks[real_idx];
                        let stack_name = selected_stack.name.clone();

                        self.timeline
                            .add_status(format!("🚀 Scaffolding stack `{}`...", stack_name));

                        let ws = self.workspace_root.clone();
                        tokio::spawn(async move {
                            match crate::tools::onpkg::scaffolder::OnpkgScaffolder::scaffold(
                                &ws,
                                &stack_name,
                                None,
                                false,
                            )
                            .await
                            {
                                Ok(msg) => {
                                    tracing::info!(
                                        stack = %stack_name,
                                        output = %msg,
                                        "Native onpkg stack scaffolded successfully"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        stack = %stack_name,
                                        error = %e,
                                        "Native onpkg stack scaffolding failed"
                                    );
                                }
                            }
                        });
                    }
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::CodeExplorer {
                ref filtered_indices,
                ref mut selected_index,
                ref mut filter,
                ref mut active_tab,
                ..
            } => match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < filtered_indices.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Tab => {
                    *active_tab = (*active_tab + 1) % 4;
                }
                KeyCode::BackTab => {
                    *active_tab = if *active_tab == 0 { 3 } else { *active_tab - 1 };
                }
                KeyCode::Backspace => {
                    filter.pop();
                    self.modal.update_filter();
                }
                KeyCode::Char(c) => {
                    filter.push(c);
                    self.modal.update_filter();
                }
                _ => {}
            },
            ModalState::GitDiff {
                ref diff_files,
                ref mut selected_file_index,
                ref mut scroll_offset,
                ref mut staged_view,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_file_index = selected_file_index.saturating_sub(1);
                    *scroll_offset = 0;
                }
                KeyCode::Down => {
                    if *selected_file_index + 1 < diff_files.len() {
                        *selected_file_index += 1;
                        *scroll_offset = 0;
                    }
                }
                KeyCode::Char('j') => {
                    *scroll_offset = scroll_offset.saturating_add(3);
                }
                KeyCode::Char('k') => {
                    *scroll_offset = scroll_offset.saturating_sub(3);
                }
                KeyCode::PageDown => {
                    *scroll_offset = scroll_offset.saturating_add(10);
                }
                KeyCode::PageUp => {
                    *scroll_offset = scroll_offset.saturating_sub(10);
                }
                KeyCode::Tab => {
                    let next_staged = !*staged_view;
                    let ws = self.workspace_root.clone();
                    match crate::git::GitDiffViewer::load_diffs(&ws, next_staged).await {
                        Ok(new_diffs) => {
                            self.modal = ModalState::new_git_diff(new_diffs, next_staged);
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Failed to reload git diffs: {}", e));
                        }
                    }
                }
                KeyCode::Char('s') => {
                    if !diff_files.is_empty() && *selected_file_index < diff_files.len() {
                        let path = diff_files[*selected_file_index].path.clone();
                        let is_staged = *staged_view;
                        let git = crate::git::GitService::new(self.workspace_root.clone());
                        if is_staged {
                            if let Err(e) =
                                git.unstage_files(Some(std::slice::from_ref(&path))).await
                            {
                                self.timeline
                                    .add_status(format!("✗ Failed to unstage {}: {}", path, e));
                            } else {
                                self.timeline.add_status(format!("✔ Unstaged {}", path));
                            }
                        } else {
                            if let Err(e) = git.stage_files(Some(std::slice::from_ref(&path))).await
                            {
                                self.timeline
                                    .add_status(format!("✗ Failed to stage {}: {}", path, e));
                            } else {
                                self.timeline.add_status(format!("✔ Staged {}", path));
                            }
                        }
                        // Reload diffs
                        let ws = self.workspace_root.clone();
                        if let Ok(new_diffs) =
                            crate::git::GitDiffViewer::load_diffs(&ws, is_staged).await
                        {
                            self.modal = ModalState::new_git_diff(new_diffs, is_staged);
                        }
                    }
                }
                KeyCode::Char('r') => {
                    let is_staged = *staged_view;
                    self.modal = ModalState::None;
                    self.timeline.add_status(
                        "🛡️ Running multi-agent code review on git diff...".to_string(),
                    );
                    match crate::git::GitReviewer::review_workspace(&self.workspace_root, is_staged)
                        .await
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
                }
                _ => {}
            },
            ModalState::Approval(ref mut approval_state) => match key.code {
                KeyCode::Esc => {
                    let pending_tool_id = approval_state.tool_id.clone();
                    self.resolve_approval(
                        &pending_tool_id,
                        crate::agent::types::ApprovalDecision::Reject,
                    );
                    self.modal = ModalState::None;
                    self.timeline
                        .add_status("ℹ Action cancelled by user".to_string());
                }
                KeyCode::Up => {
                    approval_state.prev_option();
                }
                KeyCode::Down => {
                    approval_state.next_option();
                }
                KeyCode::Backspace => {
                    approval_state.handle_backspace();
                }
                KeyCode::Char(c) => {
                    let pending_tool_id = approval_state.tool_id.clone();
                    if let Some(resp) = approval_state.handle_char(c) {
                        match resp {
                            crate::ui::approval::ApprovalResponse::Accept => {
                                self.resolve_approval(
                                    &pending_tool_id,
                                    crate::agent::types::ApprovalDecision::Approve,
                                );
                                self.timeline
                                    .add_status("✔ Action approved & applied".to_string());
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::Reject => {
                                self.resolve_approval(
                                    &pending_tool_id,
                                    crate::agent::types::ApprovalDecision::Reject,
                                );
                                self.timeline
                                    .add_status("✗ Action rejected by user".to_string());
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::AllowSession => {
                                self.config.agent.auto_approve = true;
                                self.resolve_approval(
                                    &pending_tool_id,
                                    crate::agent::types::ApprovalDecision::Approve,
                                );
                                self.timeline.add_status(
                                    "✔ Auto-approve enabled for this session".to_string(),
                                );
                                self.propagate_session_config(control_tx);
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::CustomFeedback(_) => {}
                        }
                    }
                }
                KeyCode::Enter => {
                    let pending_tool_id = approval_state.tool_id.clone();
                    if let Some(resp) = approval_state.confirm_selection() {
                        match resp {
                            crate::ui::approval::ApprovalResponse::Accept => {
                                self.resolve_approval(
                                    &pending_tool_id,
                                    crate::agent::types::ApprovalDecision::Approve,
                                );
                                self.timeline
                                    .add_status("✔ Action approved & applied".to_string());
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::Reject => {
                                self.resolve_approval(
                                    &pending_tool_id,
                                    crate::agent::types::ApprovalDecision::Reject,
                                );
                                self.timeline
                                    .add_status("✗ Action rejected by user".to_string());
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::AllowSession => {
                                self.config.agent.auto_approve = true;
                                self.resolve_approval(
                                    &pending_tool_id,
                                    crate::agent::types::ApprovalDecision::Approve,
                                );
                                self.timeline.add_status(
                                    "✔ Auto-approve enabled for this session".to_string(),
                                );
                                self.propagate_session_config(control_tx);
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::CustomFeedback(feedback) => {
                                // The old turn's pending gate must not hang.
                                self.resolve_approval(
                                    &pending_tool_id,
                                    crate::agent::types::ApprovalDecision::Reject,
                                );
                                self.timeline
                                    .add_status(format!("💬 User feedback sent: \"{}\"", feedback));
                                self.modal = ModalState::None;

                                let token = tokio_util::sync::CancellationToken::new();
                                self.cancel_token = Some(token.clone());
                                self.is_working = true;
                                self.work_start = Some(Instant::now());
                                let _ =
                                    control_tx.send(AgentCommand::Prompt(feedback, Some(token)));
                            }
                        }
                    }
                }
                _ => {}
            },
            ModalState::WorkspaceAnalysis {
                workspace_path: _,
                is_indexed,
                cached_symbols_count: _,
                cached_files_count: _,
                selected_index,
            } => {
                let max_index = if !*is_indexed { 2 } else { 3 };
                let is_indexed_flag = *is_indexed;
                let mut triggered_action: Option<usize> = None;

                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                        triggered_action = Some(if !is_indexed_flag { 2 } else { 3 });
                    }
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                        *selected_index = selected_index.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                        if *selected_index < max_index {
                            *selected_index += 1;
                        }
                    }
                    KeyCode::Char('1') => {
                        triggered_action = Some(0);
                    }
                    KeyCode::Char('2') => {
                        triggered_action = Some(1);
                    }
                    KeyCode::Char('3') => {
                        triggered_action = Some(2);
                    }
                    KeyCode::Char('4') if is_indexed_flag => {
                        triggered_action = Some(3);
                    }
                    KeyCode::Enter => {
                        triggered_action = Some(*selected_index);
                    }
                    _ => {}
                }

                if let Some(act) = triggered_action {
                    self.modal = ModalState::None;
                    self.execute_workspace_analysis_action(act, is_indexed_flag);
                }
            }
        }
    }

    /// Executes the selected workspace analysis action from the interactive modal
    fn execute_workspace_analysis_action(&mut self, action_index: usize, is_indexed: bool) {
        self.modal = ModalState::None;
        if !is_indexed {
            match action_index {
                0 => {
                    // Quick Index
                    let mut graph = crate::context::graph::CodeGraph::new();
                    match graph.build_graph(&self.workspace_root) {
                        Ok(_) => {
                            let sym_count = graph.symbol_nodes().count();
                            let file_count = graph.file_count();
                            self.timeline.add_status(format!(
                                "✔ Workspace indexed: {} symbols across {} files (saved to .minicode/graph.json)",
                                sym_count, file_count
                            ));
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Failed to index workspace: {}", e));
                        }
                    }
                }
                1 => {
                    // Deep Scan
                    let mut graph = crate::context::graph::CodeGraph::new();
                    let _ = graph.build_graph(&self.workspace_root);
                    let sym_count = graph.symbol_nodes().count();
                    match crate::context::governance::ArchitectureGovernor::scan_workspace(
                        &self.workspace_root,
                    ) {
                        Ok(report) => {
                            let card = format!(
                                "🧠 Deep Analysis Complete:\n  • Symbols: {}\n  • Architecture Health: {}/100 ({} files, {} LOC)\n  • Violations: {} | Circular Cycles: {}\n  • Snapshot: .minicode/graph.json",
                                sym_count,
                                report.health_score,
                                report.total_files,
                                report.total_loc,
                                report.layer_violations.len(),
                                report.circular_cycles.len()
                            );
                            self.timeline.add_status(card);
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Deep scan encountered error: {}", e));
                        }
                    }
                }
                _ => {
                    // Skip
                    self.timeline.add_status(
                        "⏩ Workspace analysis skipped. Running in lightweight instant mode. Type /init anytime to analyze.".to_string(),
                    );
                }
            }
        } else {
            match action_index {
                0 => {
                    // Incremental Sync
                    let mut graph = crate::context::graph::CodeGraph::new();
                    if graph.load_cached(&self.workspace_root) {
                        match graph.incremental_update(&self.workspace_root) {
                            Ok(stats) => {
                                let _ = graph.save_to_disk(&self.workspace_root);
                                let sym_count = graph.symbol_nodes().count();
                                self.timeline.add_status(format!(
                                    "✔ Incremental graph sync complete ({} files scanned, {} reparsed, {} removed) — {} active symbols",
                                    stats.files_scanned, stats.files_reparsed, stats.files_removed, sym_count
                                ));
                            }
                            Err(e) => {
                                self.timeline
                                    .add_status(format!("✗ Incremental update failed: {}", e));
                            }
                        }
                    } else {
                        // Cold build fallback
                        match graph.build_graph(&self.workspace_root) {
                            Ok(_) => {
                                let sym_count = graph.symbol_nodes().count();
                                self.timeline.add_status(format!(
                                    "✔ Code graph rebuilt: {} symbols (saved to .minicode/graph.json)",
                                    sym_count
                                ));
                            }
                            Err(e) => {
                                self.timeline
                                    .add_status(format!("✗ Failed to build graph: {}", e));
                            }
                        }
                    }
                }
                1 => {
                    // Full Rebuild
                    let mut graph = crate::context::graph::CodeGraph::new();
                    match graph.full_rebuild(&self.workspace_root) {
                        Ok(_) => {
                            let _ = graph.save_to_disk(&self.workspace_root);
                            let sym_count = graph.symbol_nodes().count();
                            let file_count = graph.file_count();
                            self.timeline.add_status(format!(
                                "🔄 Full cold re-index complete: {} symbols across {} files (saved to .minicode/graph.json)",
                                sym_count, file_count
                            ));
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Failed full re-index: {}", e));
                        }
                    }
                }
                2 => {
                    // View Repo Map
                    let mut graph = crate::context::graph::CodeGraph::new();
                    if let Err(e) = graph.build_graph(&self.workspace_root) {
                        self.timeline
                            .add_status(format!("✗ Failed to build repo map: {}", e));
                    } else {
                        let repomap = graph.format_repomap(
                            &self.workspace_root,
                            &[],
                            self.config.agent.map_tokens,
                        );
                        self.timeline
                            .add_status(format!("🗺️ AST PageRank Repository Map:\n\n{}", repomap));
                    }
                }
                _ => {
                    // Close
                    self.timeline
                        .add_status("ℹ Analysis menu closed.".to_string());
                }
            }
        }
    }
}
