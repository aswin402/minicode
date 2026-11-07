//! Modal key navigation and modal action handling for minicode TUI

use super::{AgentCommand, App};
use crate::ui::modal::ModalState;
use crate::ui::Theme;
use crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::mpsc;

#[path = "modals_impl/agent_workspace_key.rs"]
mod agent_workspace_key;
#[path = "modals_impl/settings_key.rs"]
mod settings_key;
#[path = "modals_impl/suite_key.rs"]
mod suite_key;

impl<'a> App<'a> {
    /// Handles keyboard interaction within in-TUI modal dialogs
    pub async fn handle_modal_key(
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
                    crate::dev::registry::kill_all_sync();
                    self.should_exit = true;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.modal = ModalState::None;
                }
                KeyCode::Enter => {
                    if *selected_yes {
                        crate::dev::registry::kill_all_sync();
                        self.should_exit = true;
                    } else {
                        self.modal = ModalState::None;
                    }
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    crate::dev::registry::kill_all_sync();
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
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.modal = ModalState::None;
                    if let Some(sub) = self.pending_submission.take() {
                        self.input_dock.textarea = tui_textarea::TextArea::default();
                        self.input_dock.textarea.insert_str(&sub.display);
                        self.timeline.add_status("ℹ Prompt cancelled.".to_string());
                    }
                }
                KeyCode::Char(c) => {
                    input.insert(*cursor, c);
                    *cursor += 1;
                }
                KeyCode::Enter => {
                    let entered_key = input.trim().to_string();
                    if entered_key.is_empty() {
                        self.timeline.add_status(
                            "⚠️ API key cannot be empty. Please enter a valid key or press Esc to cancel."
                                .to_string(),
                        );
                        return;
                    }
                    let prov_name = provider.clone();
                    let env_name = env_var.clone();

                    self.config
                        .provider
                        .api_keys
                        .insert(prov_name.clone(), entered_key.clone());
                    std::env::set_var(&env_name, &entered_key);
                    let _ = self.config.save(Some(&self.workspace_root));
                    self.timeline
                        .add_status(format!("✔ Saved API key for '{}'", prov_name));

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
                        let default_model = self.config.get_default_model_for_provider(&prov_name);
                        self.config.provider.default = prov_name.clone();
                        self.config.provider.model = default_model.clone();

                        let prov_err = self.sync_single_provider_config(control_tx);
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
                        if let Some(sub) = self.pending_submission.take() {
                            let _ = self
                                .handle_command_or_prompt(
                                    &sub.prompt,
                                    Some(&sub.display),
                                    control_tx,
                                )
                                .await;
                        }
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
                    if let Some(sub) = self.pending_submission.take() {
                        self.input_dock.textarea = tui_textarea::TextArea::default();
                        self.input_dock.textarea.insert_str(&sub.display);
                        self.timeline.add_status("ℹ Prompt cancelled.".to_string());
                    }
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
                    if provider == "favorites" {
                        if self.config.provider.favorite_models.is_empty() {
                            self.toasts.push_status(
                                "No favorite models saved yet — press * or Ctrl+F on any model in /switch (or f in /settings) to favorite it".to_string(),
                            );
                            return;
                        }
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
                        self.modal =
                            ModalState::new_model_select("favorites".to_string(), fav_models);
                        return;
                    }

                    let is_local = self.config.is_local_provider(&provider);
                    let custom_url = self.config.get_provider_base_url(&provider);
                    let api_key = self.config.get_api_key(&provider).unwrap_or_default();

                    // If cloud provider and no key is configured, prompt with in-TUI ApiKeyInput modal
                    if !is_local && api_key.is_empty() {
                        let env_str = crate::ui::setup::SetupWizard::custom_env_var(&provider);
                        self.modal = ModalState::new_api_key_input(provider, env_str, None);
                        return;
                    }

                    // Fetch models (will use static fallbacks if offline)
                    let models_res = self
                        .model_fetcher
                        .fetch_models(&provider, &api_key, custom_url.as_deref())
                        .await;
                    let mut models = match models_res {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!(error = %e, provider = %provider, "Failed to fetch live model list");
                            Vec::new()
                        }
                    };

                    for m in &mut models {
                        if self.config.is_favorite_model(&provider, &m.id) {
                            m.description = Some("[fav]".to_string());
                        }
                    }
                    models.sort_by_key(|m| {
                        if m.description.as_deref() == Some("[fav]") {
                            0usize
                        } else {
                            1usize
                        }
                    });

                    if models.is_empty() {
                        let default_model = self.config.get_default_model_for_provider(&provider);
                        self.config.provider.default = provider.clone();
                        self.config.provider.model = default_model.clone();

                        let prov_err = self.sync_single_provider_config(control_tx);
                        let _ = self.config.save(Some(&self.workspace_root));

                        if let Some(err) = prov_err {
                            self.toasts.push_status(format!(
                                "⚠️ Switched provider to '{}' ({}), but provider reported: {}",
                                provider, default_model, err
                            ));
                        } else {
                            self.toasts.push_status(format!(
                                "✔ Switched active provider to '{}' and model to '{}'",
                                provider, default_model
                            ));
                        }
                        self.modal = ModalState::None;
                        if let Some(sub) = self.pending_submission.take() {
                            let _ = self
                                .handle_command_or_prompt(
                                    &sub.prompt,
                                    Some(&sub.display),
                                    control_tx,
                                )
                                .await;
                        }
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
                KeyCode::Tab => {
                    if self.config.provider.favorite_models.is_empty() {
                        self.toasts.push_status(
                            "No favorite models saved yet — press * or Ctrl+F to favorite a model"
                                .to_string(),
                        );
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
                        self.modal =
                            ModalState::new_model_select("favorites".to_string(), fav_models);
                    }
                }
                KeyCode::Backspace => {
                    filter.pop();
                    self.modal.update_filter();
                }
                KeyCode::Char(c)
                    if c == '*'
                        || ((c == 'f' || c == 'F')
                            && (key.modifiers.contains(KeyModifiers::CONTROL)
                                || key.modifiers.contains(KeyModifiers::ALT))) =>
                {
                    if !filtered_indices.is_empty() && *selected_index < filtered_indices.len() {
                        let real_idx = filtered_indices[*selected_index];
                        let raw_id = models[real_idx].id.clone();
                        let (target_prov, target_model) = if provider == "favorites" {
                            if let Some((p, m)) = raw_id.split_once('/') {
                                (p.to_string(), m.to_string())
                            } else {
                                (self.config.provider.default.clone(), raw_id)
                            }
                        } else {
                            (provider.clone(), raw_id)
                        };
                        let added = self
                            .config
                            .toggle_favorite_model(&target_prov, &target_model);
                        let _ = self.config.save(Some(&self.workspace_root));
                        let _ = self.config.save(None);
                        models[real_idx].description = if added {
                            Some("[fav]".to_string())
                        } else {
                            None
                        };
                        if added {
                            self.toasts.push_status(format!(
                                "Added '{}/{}' to favorite models",
                                target_prov, target_model
                            ));
                        } else {
                            self.toasts.push_status(format!(
                                "Removed '{}/{}' from favorite models",
                                target_prov, target_model
                            ));
                        }
                    }
                }
                KeyCode::Char(c) => {
                    filter.push(c);
                    self.modal.update_filter();
                }
                KeyCode::Enter => {
                    if !filtered_indices.is_empty() && *selected_index < filtered_indices.len() {
                        let real_idx = filtered_indices[*selected_index];
                        let raw_selected = models[real_idx].id.clone();
                        let (chosen_provider, selected_model) = if provider == "favorites" {
                            if let Some((p, m)) = raw_selected.split_once('/') {
                                (p.to_string(), m.to_string())
                            } else {
                                (self.config.provider.default.clone(), raw_selected)
                            }
                        } else {
                            (provider.clone(), raw_selected)
                        };
                        self.config.provider.default = chosen_provider.clone();
                        self.config.provider.model = selected_model.clone();
                        self.config
                            .provider
                            .default_models
                            .insert(chosen_provider.clone(), selected_model.clone());

                        let prov_err = self.sync_single_provider_config(control_tx);
                        let _ = self.config.save(Some(&self.workspace_root));

                        if let Some(err) = prov_err {
                            self.toasts.push_status(format!(
                                "⚠️ Switched provider to '{}' and model to '{}', but provider reported: {}",
                                chosen_provider, selected_model, err
                            ));
                        } else {
                            self.toasts.push_status(format!(
                                "✔ Switched active provider to '{}' and model to '{}'",
                                chosen_provider, selected_model
                            ));
                        }
                    }
                    self.modal = ModalState::None;
                    if let Some(sub) = self.pending_submission.take() {
                        let _ = self
                            .handle_command_or_prompt(&sub.prompt, Some(&sub.display), control_tx)
                            .await;
                    }
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
                ref animations,
                ref mut active_tab,
                ref mut theme_selected_index,
                ref mut animation_selected_index,
                ref mut todo_style_selected_index,
                ref mut swarm_style_selected_index,
                ref mut toast_style_selected_index,
                ref mut toast_symbol_selected_index,
                ref mut activity_style_selected_index,
                ref mut active_theme_id,
                ref mut active_animation_id,
                ref mut active_todo_style,
                ref mut active_swarm_style,
                ref mut active_toast_style,
                ref mut active_toast_symbols,
                ref mut active_activity_style,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Tab | KeyCode::Right => {
                    *active_tab = match *active_tab {
                        crate::ui::modals::ThemeModalTab::Themes => {
                            crate::ui::modals::ThemeModalTab::Animations
                        }
                        crate::ui::modals::ThemeModalTab::Animations => {
                            crate::ui::modals::ThemeModalTab::TodoStyles
                        }
                        crate::ui::modals::ThemeModalTab::TodoStyles => {
                            crate::ui::modals::ThemeModalTab::SwarmStyles
                        }
                        crate::ui::modals::ThemeModalTab::SwarmStyles => {
                            crate::ui::modals::ThemeModalTab::ToastStyles
                        }
                        crate::ui::modals::ThemeModalTab::ToastStyles => {
                            crate::ui::modals::ThemeModalTab::ToastSymbols
                        }
                        crate::ui::modals::ThemeModalTab::ToastSymbols => {
                            crate::ui::modals::ThemeModalTab::ActivityStyles
                        }
                        crate::ui::modals::ThemeModalTab::ActivityStyles => {
                            crate::ui::modals::ThemeModalTab::Themes
                        }
                    };
                }
                KeyCode::BackTab | KeyCode::Left => {
                    *active_tab = match *active_tab {
                        crate::ui::modals::ThemeModalTab::Themes => {
                            crate::ui::modals::ThemeModalTab::ActivityStyles
                        }
                        crate::ui::modals::ThemeModalTab::Animations => {
                            crate::ui::modals::ThemeModalTab::Themes
                        }
                        crate::ui::modals::ThemeModalTab::TodoStyles => {
                            crate::ui::modals::ThemeModalTab::Animations
                        }
                        crate::ui::modals::ThemeModalTab::SwarmStyles => {
                            crate::ui::modals::ThemeModalTab::TodoStyles
                        }
                        crate::ui::modals::ThemeModalTab::ToastStyles => {
                            crate::ui::modals::ThemeModalTab::SwarmStyles
                        }
                        crate::ui::modals::ThemeModalTab::ToastSymbols => {
                            crate::ui::modals::ThemeModalTab::ToastStyles
                        }
                        crate::ui::modals::ThemeModalTab::ActivityStyles => {
                            crate::ui::modals::ThemeModalTab::ToastSymbols
                        }
                    };
                }
                KeyCode::Char('1') => {
                    *active_tab = crate::ui::modals::ThemeModalTab::Themes;
                }
                KeyCode::Char('2') => {
                    *active_tab = crate::ui::modals::ThemeModalTab::Animations;
                }
                KeyCode::Char('3') => {
                    *active_tab = crate::ui::modals::ThemeModalTab::TodoStyles;
                }
                KeyCode::Char('4') => {
                    *active_tab = crate::ui::modals::ThemeModalTab::SwarmStyles;
                }
                KeyCode::Char('5') => {
                    *active_tab = crate::ui::modals::ThemeModalTab::ToastStyles;
                }
                KeyCode::Char('6') => {
                    *active_tab = crate::ui::modals::ThemeModalTab::ToastSymbols;
                }
                KeyCode::Char('7') => {
                    *active_tab = crate::ui::modals::ThemeModalTab::ActivityStyles;
                }
                KeyCode::Up => match *active_tab {
                    crate::ui::modals::ThemeModalTab::Themes => {
                        *theme_selected_index = theme_selected_index.saturating_sub(1);
                    }
                    crate::ui::modals::ThemeModalTab::Animations => {
                        *animation_selected_index = animation_selected_index.saturating_sub(1);
                    }
                    crate::ui::modals::ThemeModalTab::TodoStyles => {
                        *todo_style_selected_index = todo_style_selected_index.saturating_sub(1);
                    }
                    crate::ui::modals::ThemeModalTab::SwarmStyles => {
                        *swarm_style_selected_index = swarm_style_selected_index.saturating_sub(1);
                    }
                    crate::ui::modals::ThemeModalTab::ToastStyles => {
                        *toast_style_selected_index = toast_style_selected_index.saturating_sub(1);
                    }
                    crate::ui::modals::ThemeModalTab::ToastSymbols => {
                        *toast_symbol_selected_index =
                            toast_symbol_selected_index.saturating_sub(1);
                    }
                    crate::ui::modals::ThemeModalTab::ActivityStyles => {
                        *activity_style_selected_index =
                            activity_style_selected_index.saturating_sub(1);
                    }
                },
                KeyCode::Down => match *active_tab {
                    crate::ui::modals::ThemeModalTab::Themes => {
                        if *theme_selected_index + 1 < themes.len() {
                            *theme_selected_index += 1;
                        }
                    }
                    crate::ui::modals::ThemeModalTab::Animations => {
                        if *animation_selected_index + 1 < animations.len() {
                            *animation_selected_index += 1;
                        }
                    }
                    crate::ui::modals::ThemeModalTab::TodoStyles => {
                        let total = crate::ui::modals::theme_select::TODO_STYLE_OPTIONS.len();
                        if *todo_style_selected_index + 1 < total {
                            *todo_style_selected_index += 1;
                        }
                    }
                    crate::ui::modals::ThemeModalTab::SwarmStyles => {
                        let total = crate::ui::modals::theme_select::SWARM_STYLE_OPTIONS.len();
                        if *swarm_style_selected_index + 1 < total {
                            *swarm_style_selected_index += 1;
                        }
                    }
                    crate::ui::modals::ThemeModalTab::ToastStyles => {
                        let total = crate::ui::modals::theme_select::TOAST_STYLE_OPTIONS.len();
                        if *toast_style_selected_index + 1 < total {
                            *toast_style_selected_index += 1;
                        }
                    }
                    crate::ui::modals::ThemeModalTab::ToastSymbols => {
                        let total = crate::ui::modals::theme_select::TOAST_SYMBOL_OPTIONS.len();
                        if *toast_symbol_selected_index + 1 < total {
                            *toast_symbol_selected_index += 1;
                        }
                    }
                    crate::ui::modals::ThemeModalTab::ActivityStyles => {
                        let total = crate::ui::modals::theme_select::ACTIVITY_STYLE_OPTIONS.len();
                        if *activity_style_selected_index + 1 < total {
                            *activity_style_selected_index += 1;
                        }
                    }
                },
                KeyCode::Enter => {
                    match *active_tab {
                        crate::ui::modals::ThemeModalTab::Themes => {
                            if !themes.is_empty() && *theme_selected_index < themes.len() {
                                let chosen = &themes[*theme_selected_index];
                                let chosen_id = chosen.id.clone();
                                let chosen_name = chosen.name.clone();

                                // Apply live theme
                                self.theme = Theme::detect(&chosen_id);
                                self.config.ui.theme = chosen_id.clone();
                                *active_theme_id = chosen_id;

                                // Persist to configuration file (workspace and global)
                                if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                                    tracing::warn!(
                                        "Failed to save theme setting to workspace config: {}",
                                        e
                                    );
                                }
                                if let Err(e) = self.config.save(None) {
                                    tracing::warn!(
                                        "Failed to save theme setting to global config: {}",
                                        e
                                    );
                                }

                                self.toasts.push_status(format!(
                                    "✔ Active theme switched to '{}' and saved to config",
                                    chosen_name
                                ));
                            }
                        }
                        crate::ui::modals::ThemeModalTab::Animations => {
                            if !animations.is_empty()
                                && *animation_selected_index < animations.len()
                            {
                                let chosen = &animations[*animation_selected_index];
                                let chosen_id = chosen.id.to_string();
                                let chosen_name = chosen.name.to_string();

                                self.config.ui.animation = chosen_id.clone();
                                *active_animation_id = chosen_id;

                                // Persist to configuration file (workspace and global)
                                if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                                    tracing::warn!(
                                        "Failed to save animation setting to workspace config: {}",
                                        e
                                    );
                                }
                                if let Err(e) = self.config.save(None) {
                                    tracing::warn!(
                                        "Failed to save animation setting to global config: {}",
                                        e
                                    );
                                }

                                self.toasts.push_status(format!(
                                    "✔ Loading animation switched to '{}' and saved to config",
                                    chosen_name
                                ));
                            }
                        }
                        crate::ui::modals::ThemeModalTab::TodoStyles => {
                            let options = crate::ui::modals::theme_select::TODO_STYLE_OPTIONS;
                            if !options.is_empty() && *todo_style_selected_index < options.len() {
                                let chosen = &options[*todo_style_selected_index];
                                let chosen_id = chosen.id.to_string();
                                let chosen_name = chosen.name.to_string();

                                self.config.ui.todo_style = chosen_id.clone();
                                *active_todo_style = chosen_id;

                                // Persist to configuration file (workspace and global)
                                if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                                    tracing::warn!(
                                        "Failed to save todo_style setting to workspace config: {}",
                                        e
                                    );
                                }
                                if let Err(e) = self.config.save(None) {
                                    tracing::warn!(
                                        "Failed to save todo_style setting to global config: {}",
                                        e
                                    );
                                }

                                self.toasts.push_status(format!(
                                    "✔ Todo widget style switched to '{}' and saved to config",
                                    chosen_name
                                ));
                            }
                        }
                        crate::ui::modals::ThemeModalTab::SwarmStyles => {
                            let options = crate::ui::modals::theme_select::SWARM_STYLE_OPTIONS;
                            if !options.is_empty() && *swarm_style_selected_index < options.len() {
                                let chosen = &options[*swarm_style_selected_index];
                                let chosen_id = chosen.id.to_string();
                                let chosen_name = chosen.name.to_string();

                                self.config.ui.swarm_style = chosen_id.clone();
                                *active_swarm_style = chosen_id;

                                // Persist to configuration file (workspace and global)
                                if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                                    tracing::warn!(
                                        "Failed to save swarm style setting to workspace config: {}",
                                        e
                                    );
                                }
                                if let Err(e) = self.config.save(None) {
                                    tracing::warn!(
                                        "Failed to save swarm style setting to global config: {}",
                                        e
                                    );
                                }

                                self.toasts.push_status(format!(
                                    "✔ Swarm flight deck style switched to '{}' ({}) and saved to config",
                                    chosen_name, chosen.id
                                ));
                            }
                        }
                        crate::ui::modals::ThemeModalTab::ToastStyles => {
                            let options = crate::ui::modals::theme_select::TOAST_STYLE_OPTIONS;
                            if !options.is_empty() && *toast_style_selected_index < options.len() {
                                let chosen = &options[*toast_style_selected_index];
                                let chosen_id = chosen.id.to_string();
                                let chosen_name = chosen.name.to_string();

                                self.config.ui.toast_style = chosen_id.clone();
                                *active_toast_style = chosen_id.clone();

                                // Persist to configuration file (workspace and global)
                                if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                                    tracing::warn!(
                                        "Failed to save toast style setting to workspace config: {}",
                                        e
                                    );
                                }
                                if let Err(e) = self.config.save(None) {
                                    tracing::warn!(
                                        "Failed to save toast style setting to global config: {}",
                                        e
                                    );
                                }

                                self.toasts.push(
                                    crate::ui::ToastKind::Success,
                                    "Toast Style Updated",
                                    format!("Switched to \"{chosen_name}\"."),
                                    Some(chosen_id),
                                    Some("toast_style_preview".to_string()),
                                );
                            }
                        }
                        crate::ui::modals::ThemeModalTab::ToastSymbols => {
                            let options = crate::ui::modals::theme_select::TOAST_SYMBOL_OPTIONS;
                            if !options.is_empty() && *toast_symbol_selected_index < options.len() {
                                let chosen = &options[*toast_symbol_selected_index];
                                let chosen_id = chosen.id.to_string();
                                let chosen_name = chosen.name.to_string();

                                self.config.ui.toast_symbols = chosen_id.clone();
                                *active_toast_symbols = chosen_id.clone();

                                if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                                    tracing::warn!(
                                        "Failed to save toast_symbols setting to workspace config: {}",
                                        e
                                    );
                                }
                                if let Err(e) = self.config.save(None) {
                                    tracing::warn!(
                                        "Failed to save toast_symbols setting to global config: {}",
                                        e
                                    );
                                }

                                self.toasts.push(
                                    crate::ui::ToastKind::Skill,
                                    "Toast Symbols Updated",
                                    format!("Switched to \"{chosen_name}\"."),
                                    Some(chosen_id),
                                    Some("toast_symbols_preview".to_string()),
                                );
                            }
                        }
                        crate::ui::modals::ThemeModalTab::ActivityStyles => {
                            let options = crate::ui::modals::theme_select::ACTIVITY_STYLE_OPTIONS;
                            if !options.is_empty() && *activity_style_selected_index < options.len()
                            {
                                let chosen = &options[*activity_style_selected_index];
                                let chosen_id = chosen.id.to_string();
                                let chosen_name = chosen.name.to_string();

                                self.config.ui.activity_style = chosen_id.clone();
                                *active_activity_style = chosen_id.clone();

                                if let Err(e) = self.config.save(Some(&self.workspace_root)) {
                                    tracing::warn!(
                                        "Failed to save activity_style setting to workspace config: {}",
                                        e
                                    );
                                }
                                if let Err(e) = self.config.save(None) {
                                    tracing::warn!(
                                        "Failed to save activity_style setting to global config: {}",
                                        e
                                    );
                                }

                                self.toasts.push(
                                    crate::ui::ToastKind::Success,
                                    "Activity Bar Style Updated",
                                    format!("Switched to \"{chosen_name}\"."),
                                    Some(chosen_id),
                                    Some("activity_style_preview".to_string()),
                                );
                            }
                        }
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
                                let _ = control_tx.send(AgentCommand::HydrateSession {
                                    session_id: target_id.clone(),
                                    events: events.clone(),
                                });
                                self.toasts.push_status(format!(
                                    "✔ Loaded session '{}' and restored agent memory ({} events)",
                                    target_id, count
                                ));
                            }
                            Err(e) => {
                                self.toasts.push_status(format!(
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
                                self.toasts.push_status(format!(
                                    "🌿 Forked session '{}' ➔ new branch '{}'",
                                    target_id, new_id
                                ));
                                if let Ok(events) = store.load_session(&new_id) {
                                    self.timeline.entries.clear();
                                    self.hydrate_session(&events);
                                    let _ = control_tx.send(AgentCommand::HydrateSession {
                                        session_id: new_id.clone(),
                                        events,
                                    });
                                }
                            }
                            Err(e) => {
                                self.toasts
                                    .push_status(format!("✗ Failed to fork session: {}", e));
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
                                self.toasts.push_status(format!(
                                    "✔ Exported session Markdown to {}",
                                    p.display()
                                ));
                            }
                            Err(e) => {
                                self.toasts.push_status(format!("✗ Export failed: {}", e));
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
                                self.toasts
                                    .push_status(format!("✔ Deleted session '{}'", target_id));
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
                                self.toasts.push_status(format!(
                                    "ℹ Session '{}' not found on disk",
                                    target_id
                                ));
                            }
                            Err(e) => {
                                self.toasts.push_status(format!(
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
                    self.toasts
                        .push_status("✔ Streaming mode enabled (persisted to config)".to_string());
                    self.modal = ModalState::None;
                }
                KeyCode::Char('2') => {
                    self.config.agent.streaming = false;
                    self.propagate_session_config(control_tx);
                    let _ = self.config.save(Some(&self.workspace_root));
                    self.toasts
                        .push_status("✔ Streaming mode disabled (persisted to config)".to_string());
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
                            self.toasts.push_status(
                                "✔ Streaming mode enabled (persisted to config)".to_string(),
                            );
                        }
                        1 => {
                            self.config.agent.streaming = false;
                            self.propagate_session_config(control_tx);
                            let _ = self.config.save(Some(&self.workspace_root));
                            self.toasts.push_status(
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
                        if selected_cmd == "/commands" {
                            self.modal = ModalState::new_command_catalog();
                        } else {
                            let _ = self
                                .handle_command_or_prompt(
                                    selected_cmd,
                                    Some(selected_cmd),
                                    control_tx,
                                )
                                .await;
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
                            match crate::tools::minikit::scaffolder::MiniKitScaffolder::scaffold(
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
                                        "Native MiniKit stack scaffolded successfully"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        stack = %stack_name,
                                        error = %e,
                                        "Native MiniKit stack scaffolding failed"
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
            ModalState::Approval(_)
            | ModalState::Inquiry(_)
            | ModalState::WorkspaceAnalysis { .. }
            | ModalState::WorkspaceDrift { .. }
            | ModalState::ProviderSetupRequired { .. }
            | ModalState::ArchitectureAudit { .. }
            | ModalState::ContextDiagnostics { .. } => {
                self.handle_agent_and_workspace_modal_key(key, control_tx)
                    .await;
            }
            ModalState::Settings(_) | ModalState::ConfigApproval { .. } => {
                self.handle_settings_and_config_modal_key(key, control_tx)
                    .await;
            }
            ModalState::MiniPower(_)
            | ModalState::Blocks(_)
            | ModalState::Vault(_)
            | ModalState::Processes(_)
            | ModalState::Todo(_)
            | ModalState::CodeReview(_) => {
                self.handle_suite_modals_key(key).await;
            }
        }
    }
}
