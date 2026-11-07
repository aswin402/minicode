//! Key handling for Settings and ConfigApproval modals.

use crate::app::{AgentCommand, App};
use crate::ui::modal::ModalState;
use crate::ui::Theme;
use crossterm::event::KeyCode;
use tokio::sync::mpsc;

impl<'a> App<'a> {
    pub(super) async fn handle_settings_and_config_modal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
        match &mut self.modal {
            ModalState::Settings(state) => {
                // If currently in Model Selection drilldown:
                if state.selecting_model_for_provider.is_some() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Backspace => {
                            state.selecting_model_for_provider = None;
                            state.provider_models.clear();
                            state.model_selected_index = 0;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            state.prev_item();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            state.next_item();
                        }
                        KeyCode::Tab => {
                            state.next_tab();
                        }
                        KeyCode::BackTab => {
                            state.prev_tab();
                        }
                        KeyCode::Char('m') => {
                            state.next_routing_target();
                        }
                        KeyCode::Char('M') => {
                            state.prev_routing_target();
                        }
                        KeyCode::Char('a') => {
                            self.config.sync_all_minipubs_to_orchestrator();
                            state.sync_from_config(&self.config);
                            if state.save_to_workspace {
                                let _ = self.config.save(Some(&self.workspace_root));
                            } else {
                                let _ = self.config.save(None);
                            }
                            let _ = self.sync_orchestrator_config(control_tx);
                            self.toasts.push_status(
                                "Synced all 14 MiniPubs to Global (Orchestrator) 4-slot chain"
                                    .to_string(),
                            );
                        }
                        KeyCode::Char('c') | KeyCode::Delete => {
                            let (target_slug, target_label) = state.current_routing_target();
                            if target_slug == "global" {
                                if self.config.provider.fallback_1.is_some()
                                    || self.config.provider.fallback_2.is_some()
                                {
                                    self.config.provider.fallback_1 = None;
                                    self.config.provider.fallback_2 = None;
                                    self.toasts.push_status(
                                        "Cleared Global 1st & 2nd Fallbacks".to_string(),
                                    );
                                } else {
                                    self.config.provider.global_last_fallback = None;
                                    self.toasts
                                        .push_status("Cleared Global Last Fallback".to_string());
                                }
                            } else {
                                self.config.clear_minipub_role_override(target_slug);
                                self.toasts.push_status(format!(
                                    "Reset '{}' to inherit Global 4-slot chain",
                                    target_label
                                ));
                            }
                            state.sync_from_config(&self.config);
                            if state.save_to_workspace {
                                let _ = self.config.save(Some(&self.workspace_root));
                            } else {
                                let _ = self.config.save(None);
                            }
                            let _ = self.sync_orchestrator_config(control_tx);
                        }
                        KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Char('*') => {
                            if !state.provider_models.is_empty()
                                && state.model_selected_index < state.provider_models.len()
                            {
                                let prov = state
                                    .selecting_model_for_provider
                                    .clone()
                                    .unwrap_or_default();
                                let chosen_model =
                                    state.provider_models[state.model_selected_index].id.clone();
                                let added = self.config.toggle_favorite_model(&prov, &chosen_model);
                                state.sync_from_config(&self.config);
                                if state.save_to_workspace {
                                    let _ = self.config.save(Some(&self.workspace_root));
                                } else {
                                    let _ = self.config.save(None);
                                }
                                if added {
                                    self.toasts.push_status(format!(
                                        "Added '{}/{}' to favorite models",
                                        prov, chosen_model
                                    ));
                                } else {
                                    self.toasts.push_status(format!(
                                        "Removed '{}/{}' from favorite models",
                                        prov, chosen_model
                                    ));
                                }
                            }
                        }
                        KeyCode::Enter
                        | KeyCode::Char('1')
                        | KeyCode::Char('2')
                        | KeyCode::Char('3')
                        | KeyCode::Char('4') => {
                            if !state.provider_models.is_empty()
                                && state.model_selected_index < state.provider_models.len()
                            {
                                let close_drilldown = matches!(key.code, KeyCode::Enter);
                                let slot_num: usize = match key.code {
                                    KeyCode::Char('1') => 1,
                                    KeyCode::Char('2') => 2,
                                    KeyCode::Char('3') => 3,
                                    KeyCode::Char('4') => 4,
                                    _ => state.pending_slot.unwrap_or(1) as usize,
                                };
                                let prov = state
                                    .selecting_model_for_provider
                                    .clone()
                                    .unwrap_or_default();
                                let chosen_model =
                                    state.provider_models[state.model_selected_index].id.clone();
                                let (target_slug, target_label) = state.current_routing_target();

                                let slot =
                                    crate::config::ModelSlot::new(&prov, chosen_model.clone());
                                self.config.set_slot_for_target(
                                    target_slug,
                                    slot_num,
                                    Some(slot.clone()),
                                );
                                state.sync_from_config(&self.config);

                                // Save to target config layer
                                if state.save_to_workspace {
                                    let ws_minicode_dir = self
                                        .workspace_root
                                        .join(crate::constants::WORKSPACE_DIR_NAME);
                                    let _ = std::fs::create_dir_all(&ws_minicode_dir);
                                    let _ = self.config.save(Some(&self.workspace_root));
                                    if target_slug == "global" && slot_num == 1 {
                                        let _ = crate::config::Config::save_workspace_preference(
                                            &self.workspace_root,
                                            &prov,
                                            &chosen_model,
                                        );
                                    }
                                } else {
                                    let _ = self.config.save(None);
                                }

                                if close_drilldown {
                                    state.clear_drilldown();
                                }

                                // Update live agent provider runtime with full 4-slot chain
                                let prov_err = self.sync_orchestrator_config(control_tx);

                                let slot_name = match slot_num {
                                    1 => "Primary",
                                    2 => "1st Fallback",
                                    3 => "2nd Fallback",
                                    _ => "Global Last Fallback",
                                };
                                let scope_display = if slot_num == 4 {
                                    "All Agents (Shared)"
                                } else {
                                    target_label
                                };

                                if let Some(err) = prov_err {
                                    self.toasts.push_status(format!(
                                        "Set {} for {} to '{}/{}' (note: {})",
                                        slot_name, scope_display, prov, chosen_model, err
                                    ));
                                } else {
                                    self.toasts.push_status(format!(
                                        "Set {} for {} to '{}/{}'",
                                        slot_name, scope_display, prov, chosen_model
                                    ));
                                }
                            }
                        }
                        _ => {}
                    }
                    return;
                }

                // Standard Settings Modal Key Handling:
                let in_provider_picker = state.active_tab
                    == crate::ui::modals::settings::SettingsTab::Providers
                    || state.selecting_provider_for_target;

                match key.code {
                    KeyCode::Esc | KeyCode::Backspace if state.selecting_provider_for_target => {
                        state.clear_drilldown();
                    }
                    KeyCode::Tab | KeyCode::Right => {
                        state.next_tab();
                    }
                    KeyCode::BackTab | KeyCode::Left => {
                        state.prev_tab();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.prev_item();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.next_item();
                    }
                    KeyCode::Char('m') if in_provider_picker => {
                        state.next_routing_target();
                    }
                    KeyCode::Char('M') if in_provider_picker => {
                        state.prev_routing_target();
                    }
                    KeyCode::Char('a')
                        if matches!(
                            state.active_tab,
                            crate::ui::modals::settings::SettingsTab::Providers
                                | crate::ui::modals::settings::SettingsTab::Fallbacks
                                | crate::ui::modals::settings::SettingsTab::MiniPubs
                        ) =>
                    {
                        self.config.sync_all_minipubs_to_orchestrator();
                        state.sync_from_config(&self.config);
                        if state.save_to_workspace {
                            let _ = self.config.save(Some(&self.workspace_root));
                        } else {
                            let _ = self.config.save(None);
                        }
                        let _ = self.sync_orchestrator_config(control_tx);
                        self.toasts.push_status(
                            "Synced all 14 MiniPubs to Global (Orchestrator) 4-slot chain"
                                .to_string(),
                        );
                    }
                    KeyCode::Char('c') | KeyCode::Delete
                        if matches!(
                            state.active_tab,
                            crate::ui::modals::settings::SettingsTab::Providers
                                | crate::ui::modals::settings::SettingsTab::Fallbacks
                                | crate::ui::modals::settings::SettingsTab::MiniPubs
                        ) =>
                    {
                        if state.active_tab == crate::ui::modals::settings::SettingsTab::Fallbacks
                            && !state.selecting_provider_for_target
                        {
                            match state.selected_index {
                                1 => {
                                    self.config.provider.fallback_1 = None;
                                    self.toasts
                                        .push_status("Cleared Global 1st Fallback".to_string());
                                }
                                2 => {
                                    self.config.provider.fallback_2 = None;
                                    self.toasts
                                        .push_status("Cleared Global 2nd Fallback".to_string());
                                }
                                3 => {
                                    self.config.provider.global_last_fallback = None;
                                    self.toasts
                                        .push_status("Cleared Global Last Fallback".to_string());
                                }
                                _ => {
                                    self.config.provider.fallback_1 = None;
                                    self.config.provider.fallback_2 = None;
                                    self.toasts.push_status(
                                        "Cleared Global 1st & 2nd Fallbacks".to_string(),
                                    );
                                }
                            }
                        } else if state.active_tab
                            == crate::ui::modals::settings::SettingsTab::MiniPubs
                            && !state.selecting_provider_for_target
                        {
                            if state.selected_index >= 4 {
                                let mp_idx = state.selected_index - 3;
                                if let Some((slug, label)) =
                                    crate::ui::modals::settings::ROUTING_TARGETS
                                        .get(mp_idx)
                                        .copied()
                                {
                                    self.config.clear_minipub_role_override(slug);
                                    self.toasts.push_status(format!(
                                        "Reset '{}' to inherit Global 4-slot chain",
                                        label
                                    ));
                                }
                            }
                        } else {
                            let (target_slug, target_label) = state.current_routing_target();
                            if target_slug == "global" {
                                if self.config.provider.fallback_1.is_some()
                                    || self.config.provider.fallback_2.is_some()
                                {
                                    self.config.provider.fallback_1 = None;
                                    self.config.provider.fallback_2 = None;
                                    self.toasts.push_status(
                                        "Cleared Global 1st & 2nd Fallbacks".to_string(),
                                    );
                                } else {
                                    self.config.provider.global_last_fallback = None;
                                    self.toasts
                                        .push_status("Cleared Global Last Fallback".to_string());
                                }
                            } else {
                                self.config.clear_minipub_role_override(target_slug);
                                self.toasts.push_status(format!(
                                    "Reset '{}' to inherit Global 4-slot chain",
                                    target_label
                                ));
                            }
                        }
                        state.sync_from_config(&self.config);
                        if state.save_to_workspace {
                            let _ = self.config.save(Some(&self.workspace_root));
                        } else {
                            let _ = self.config.save(None);
                        }
                        let _ = self.sync_orchestrator_config(control_tx);
                    }
                    KeyCode::Char('1')
                    | KeyCode::Char('2')
                    | KeyCode::Char('3')
                    | KeyCode::Char('4')
                        if in_provider_picker =>
                    {
                        if state.selected_index < crate::constants::SUPPORTED_PROVIDERS.len() {
                            let prov = crate::constants::SUPPORTED_PROVIDERS[state.selected_index];
                            let model = self.config.get_default_model_for_provider(prov);
                            let slot_num = match key.code {
                                KeyCode::Char('2') => 2,
                                KeyCode::Char('3') => 3,
                                KeyCode::Char('4') => 4,
                                _ => 1,
                            };
                            let (target_slug, target_label) = state.current_routing_target();
                            let slot = crate::config::ModelSlot::new(prov, model.clone());
                            self.config
                                .set_slot_for_target(target_slug, slot_num, Some(slot));
                            state.sync_from_config(&self.config);
                            if state.save_to_workspace {
                                let _ = self.config.save(Some(&self.workspace_root));
                            } else {
                                let _ = self.config.save(None);
                            }
                            let _ = self.sync_orchestrator_config(control_tx);
                            let slot_name = match slot_num {
                                1 => "Primary",
                                2 => "1st Fallback",
                                3 => "2nd Fallback",
                                _ => "Global Last Fallback",
                            };
                            let scope_display = if slot_num == 4 {
                                "All Agents (Shared)"
                            } else {
                                target_label
                            };
                            self.toasts.push_status(format!(
                                "Set {} for {} to '{}/{}'",
                                slot_name, scope_display, prov, model
                            ));
                        }
                    }
                    KeyCode::Char(' ') if in_provider_picker => {
                        if state.selected_index < crate::constants::SUPPORTED_PROVIDERS.len() {
                            let prov = crate::constants::SUPPORTED_PROVIDERS[state.selected_index];
                            let model = self.config.get_default_model_for_provider(prov);
                            let slot_num = state.pending_slot.unwrap_or(1) as usize;
                            let (target_slug, target_label) = state.current_routing_target();
                            let slot = crate::config::ModelSlot::new(prov, model.clone());
                            self.config
                                .set_slot_for_target(target_slug, slot_num, Some(slot));
                            state.sync_from_config(&self.config);
                            self.toasts.push_status(format!(
                                "Set Slot {} for {} to '{}/{}'",
                                slot_num, target_label, prov, model
                            ));
                        }
                    }
                    KeyCode::Enter if in_provider_picker => {
                        if state.selected_index < crate::constants::SUPPORTED_PROVIDERS.len() {
                            let prov = crate::constants::SUPPORTED_PROVIDERS[state.selected_index];
                            let prov_str = prov.to_string();

                            // Fetch live models with fallback to static catalog
                            let api_key = self.config.get_api_key(prov).unwrap_or_default();
                            let custom_url = self.config.get_provider_base_url(prov);
                            let live_models = self
                                .model_fetcher
                                .fetch_models(prov, &api_key, custom_url.as_deref())
                                .await
                                .unwrap_or_default();

                            let models = if !live_models.is_empty() {
                                live_models
                            } else {
                                let fb = crate::tools::registry::agent_tools::config_tools::fallback_models_for_provider(prov);
                                fb.into_iter()
                                    .map(|item| crate::agent::models::ModelInfo {
                                        id: item.id,
                                        name: item.name,
                                        description: None,
                                        context_length: Some(item.context_length),
                                        is_free: false,
                                    })
                                    .collect()
                            };

                            let current_default = self.config.get_default_model_for_provider(prov);
                            let def_idx = models
                                .iter()
                                .position(|m| m.id == current_default || m.name == current_default)
                                .unwrap_or(0);

                            state.selecting_model_for_provider = Some(prov_str);
                            state.provider_models = models;
                            state.model_selected_index = def_idx;
                        }
                    }
                    KeyCode::Char(' ') | KeyCode::Enter => match state.active_tab {
                        crate::ui::modals::settings::SettingsTab::Providers => {}
                        crate::ui::modals::settings::SettingsTab::Fallbacks => {
                            if state.selected_index < 4 {
                                state.routing_target_idx = 0;
                                state.pending_slot = Some((state.selected_index + 1) as u8);
                                state.return_selected_index = state.selected_index;
                                state.selecting_provider_for_target = true;
                                state.selected_index = 0;
                            } else {
                                self.config.sync_all_minipubs_to_orchestrator();
                                state.sync_from_config(&self.config);
                                self.toasts.push_status(
                                    "Synced all 14 MiniPubs to Global 4-slot chain".to_string(),
                                );
                            }
                        }
                        crate::ui::modals::settings::SettingsTab::MiniPubs => {
                            match state.selected_index {
                                0 => {
                                    state.minipub_max_parallel = match state.minipub_max_parallel {
                                        1 => 2,
                                        2 => 4,
                                        4 => 6,
                                        6 => 8,
                                        8 => 12,
                                        12 => 16,
                                        _ => 1,
                                    };
                                }
                                1 => {
                                    state.minipub_inactivity_timeout_secs =
                                        match state.minipub_inactivity_timeout_secs {
                                            120 => 300,
                                            300 => 600,
                                            600 => 900,
                                            900 => 1800,
                                            1800 => 3600,
                                            _ => 120,
                                        };
                                }
                                2 => {
                                    state.minipub_worktree_isolation = match state
                                        .minipub_worktree_isolation
                                        .to_lowercase()
                                        .as_str()
                                    {
                                        "auto" => "always".to_string(),
                                        "always" => "never".to_string(),
                                        _ => "auto".to_string(),
                                    };
                                }
                                3 => {
                                    self.config.sync_all_minipubs_to_orchestrator();
                                    state.sync_from_config(&self.config);
                                    self.toasts.push_status(
                                        "Synced all 14 MiniPubs to Global 4-slot chain".to_string(),
                                    );
                                }
                                idx if idx >= 4 => {
                                    state.routing_target_idx = idx - 3;
                                    state.pending_slot = None;
                                    state.return_selected_index = idx;
                                    state.selecting_provider_for_target = true;
                                    state.selected_index = 0;
                                }
                                _ => {}
                            }
                        }
                        crate::ui::modals::settings::SettingsTab::Workspace => {
                            state.save_to_workspace = !state.save_to_workspace;
                        }
                        crate::ui::modals::settings::SettingsTab::Autonomy => {
                            let max_out =
                                crate::agent::models::get_model_output_limit(&state.active_model);
                            let q1 = (max_out / 4).max(1024);
                            let q2 = (max_out / 2).max(2048);
                            let q3 = ((max_out * 3) / 4).max(4096);
                            match state.selected_index {
                                0 => {
                                    state.streaming = !state.streaming;
                                }
                                1 => {
                                    state.caveman_mode = !state.caveman_mode;
                                }
                                2 => {
                                    state.caveman_level =
                                        crate::agent::caveman::CavemanLevel::from_str_loose(
                                            &state.caveman_level,
                                        )
                                        .cycle_next()
                                        .as_str()
                                        .to_string();
                                }
                                3 => {
                                    state.effort_level =
                                        match state.effort_level.to_lowercase().as_str() {
                                            "low" => {
                                                state.thinking_budget = q1;
                                                "medium".to_string()
                                            }
                                            "medium" => {
                                                state.thinking_budget = q3;
                                                "high".to_string()
                                            }
                                            _ => {
                                                state.thinking_budget = 0;
                                                "low".to_string()
                                            }
                                        };
                                }
                                4 => {
                                    state.auto_approve = !state.auto_approve;
                                }
                                5 => {
                                    state.thinking_budget = if state.thinking_budget == 0 {
                                        q1
                                    } else if state.thinking_budget < q2 {
                                        q2
                                    } else if state.thinking_budget < q3 {
                                        q3
                                    } else if state.thinking_budget < max_out {
                                        max_out
                                    } else {
                                        0
                                    };
                                }
                                6 => {
                                    state.approval_policy =
                                        match state.approval_policy.to_lowercase().as_str() {
                                            "strict" => "prompt".to_string(),
                                            "prompt" => "permissive".to_string(),
                                            _ => "strict".to_string(),
                                        };
                                }
                                7 => {
                                    state.stream_idle_timeout_secs =
                                        match state.stream_idle_timeout_secs {
                                            120 => 300,
                                            300 => 600,
                                            600 => 900,
                                            900 => 1800,
                                            1800 => 3600,
                                            _ => 120,
                                        };
                                }
                                _ => {}
                            }
                        }
                        crate::ui::modals::settings::SettingsTab::Probes => {
                            state.probing = true;
                            let results = crate::tools::registry::agent_tools::config_tools::test_all_provider_connections(&self.config).await;
                            state.probe_results = Some(results);
                            state.probing = false;
                        }
                        crate::ui::modals::settings::SettingsTab::Browser => {
                            match state.selected_index {
                                0 => {
                                    state.browser_default_engine =
                                        match state.browser_default_engine.as_deref() {
                                            None => Some("obscura".to_string()),
                                            Some("obscura") => Some("firefox".to_string()),
                                            Some("firefox") => Some("chrome".to_string()),
                                            _ => None,
                                        };
                                }
                                1 => {
                                    if !state.browser_headless_priority.is_empty() {
                                        state.browser_headless_priority.rotate_left(1);
                                    }
                                }
                                2 => {
                                    if !state.browser_gui_priority.is_empty() {
                                        state.browser_gui_priority.rotate_left(1);
                                    }
                                }
                                3 => {
                                    state.browser_default_engine = None;
                                    state.browser_headless_priority = vec![
                                        "obscura".to_string(),
                                        "chrome".to_string(),
                                        "firefox".to_string(),
                                    ];
                                    state.browser_gui_priority = vec![
                                        "chrome".to_string(),
                                        "obscura".to_string(),
                                        "firefox".to_string(),
                                    ];
                                }
                                _ => {}
                            }
                        }
                    },
                    KeyCode::Esc | KeyCode::Char('q') => {
                        let old_provider = self.config.provider.default.clone();
                        let old_model = self.config.provider.model.clone();
                        let old_fb1 = self.config.provider.fallback_1.clone();
                        let old_fb2 = self.config.provider.fallback_2.clone();
                        let old_gfb = self.config.provider.global_last_fallback.clone();
                        let old_streaming = self.config.agent.streaming;
                        let old_caveman = self.config.agent.caveman_mode;
                        let old_caveman_level = self.config.agent.caveman_level.clone();
                        let old_effort = self.config.provider.effective_level_label();
                        let old_auto_approve = self.config.agent.auto_approve;
                        let old_thinking = self.config.provider.thinking_budget;
                        let old_policy = self.config.agent.approval_policy.clone();
                        let old_minipub = self.config.agent.minipub.clone();
                        let old_stream_idle = self.config.provider.stream_idle_timeout_secs;
                        let old_browser = self.config.browser.clone();

                        self.config.provider.default = state.active_provider.clone();
                        self.config.provider.model = state.active_model.clone();
                        self.config.provider.fallback_1 = state.fallback_1.clone();
                        self.config.provider.fallback_2 = state.fallback_2.clone();
                        self.config.provider.global_last_fallback =
                            state.global_last_fallback.clone();
                        self.config.agent.streaming = state.streaming;
                        self.config.agent.caveman_mode = state.caveman_mode;
                        self.config.agent.caveman_level = state.caveman_level.clone();
                        let flag_path =
                            crate::agent::caveman::default_flag_path(&self.workspace_root);
                        let level_flag = if state.caveman_mode {
                            crate::agent::caveman::CavemanLevel::from_str_loose(
                                &self.config.agent.caveman_level,
                            )
                        } else {
                            crate::agent::caveman::CavemanLevel::Off
                        };
                        let _ = crate::agent::caveman::write_flag_safe(&flag_path, level_flag);
                        if old_effort != state.effort_level {
                            self.config.provider.apply_effort_level(&state.effort_level);
                        }
                        self.config.agent.auto_approve = state.auto_approve;
                        self.config.provider.thinking_budget = if state.thinking_budget == 0 {
                            None
                        } else {
                            Some(state.thinking_budget)
                        };
                        self.config.agent.approval_policy = state.approval_policy.clone();
                        self.config.agent.minipub.max_parallel = state.minipub_max_parallel;
                        self.config.agent.minipub.inactivity_timeout_secs =
                            state.minipub_inactivity_timeout_secs;
                        self.config.agent.minipub.worktree_isolation =
                            state.minipub_worktree_isolation.clone();
                        self.config.agent.minipub.sync_with_orchestrator =
                            state.minipub_sync_with_orchestrator;
                        self.config.agent.minipub.default_chain =
                            state.minipub_default_chain.clone();
                        self.config.agent.minipub.role_models = state.minipub_role_models.clone();
                        self.config.provider.stream_idle_timeout_secs =
                            Some(state.stream_idle_timeout_secs);
                        self.config.sync_runtime_timeouts();
                        self.config.browser.default_engine = state.browser_default_engine.clone();
                        self.config.browser.headless_priority =
                            state.browser_headless_priority.clone();
                        self.config.browser.gui_priority = state.browser_gui_priority.clone();

                        if state.save_to_workspace {
                            let ws_minicode_dir = self
                                .workspace_root
                                .join(crate::constants::WORKSPACE_DIR_NAME);
                            let _ = std::fs::create_dir_all(&ws_minicode_dir);
                            let _ = self.config.save(Some(&self.workspace_root));
                            let _ = crate::config::Config::save_workspace_preference(
                                &self.workspace_root,
                                &self.config.provider.default,
                                &self.config.provider.model,
                            );
                        } else {
                            let _ = self.config.save(None);
                        }

                        let config_changed = old_provider != self.config.provider.default
                            || old_model != self.config.provider.model
                            || old_fb1 != self.config.provider.fallback_1
                            || old_fb2 != self.config.provider.fallback_2
                            || old_gfb != self.config.provider.global_last_fallback
                            || old_streaming != self.config.agent.streaming
                            || old_caveman != self.config.agent.caveman_mode
                            || old_caveman_level != self.config.agent.caveman_level
                            || old_effort != state.effort_level
                            || old_auto_approve != self.config.agent.auto_approve
                            || old_thinking != self.config.provider.thinking_budget
                            || old_policy != self.config.agent.approval_policy
                            || old_minipub != self.config.agent.minipub
                            || old_stream_idle != self.config.provider.stream_idle_timeout_secs
                            || old_browser != self.config.browser;

                        if config_changed {
                            let _ = self.sync_orchestrator_config(control_tx);
                        }

                        self.toasts
                            .push_status("✔ Settings saved successfully".to_string());
                        self.modal = ModalState::None;
                    }
                    _ => {}
                }
            }
            ModalState::ConfigApproval {
                proposal,
                selected_index,
            } => {
                let prop = proposal.clone();
                let mut apply = false;
                let mut decline = false;

                match key.code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                        *selected_index = if *selected_index == 0 { 1 } else { 0 };
                    }
                    KeyCode::Char('1') => {
                        apply = true;
                    }
                    KeyCode::Char('2') | KeyCode::Esc => {
                        decline = true;
                    }
                    KeyCode::Enter => {
                        if *selected_index == 0 {
                            apply = true;
                        } else {
                            decline = true;
                        }
                    }
                    _ => {}
                }

                if apply {
                    self.modal = ModalState::None;
                    self.apply_config_proposal_and_notify(&prop, control_tx);
                } else if decline {
                    self.timeline
                        .add_status("Config change proposal declined.".to_string());
                    self.modal = ModalState::None;
                }
            }
            _ => {}
        }
    }

    /// Applies a configuration change proposal approved by the user and updates the running actor.
    fn apply_config_proposal_and_notify(
        &mut self,
        proposal: &crate::tools::registry::agent_tools::config_tools::ConfigChangeProposal,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
        match crate::tools::registry::agent_tools::config_tools::apply_proposal(
            proposal,
            &self.workspace_root,
        ) {
            Ok(_) => {
                if let Some(ref p) = proposal.provider {
                    self.config.provider.default = p.clone();
                }
                if let Some(ref m) = proposal.model {
                    self.config.provider.model = m.clone();
                }
                if let Some(aa) = proposal.auto_approve {
                    self.config.agent.auto_approve = aa;
                }
                if let Some(tb) = proposal.thinking_budget {
                    self.config.provider.thinking_budget = if tb == 0 { None } else { Some(tb) };
                }
                if let Some(ref th) = proposal.theme {
                    self.config.ui.theme = th.clone();
                    self.theme = Theme::detect(th);
                }

                let _ = self.sync_single_provider_config(control_tx);

                self.timeline.add_status(
                    "✔ Configuration change proposal applied successfully.".to_string(),
                );
            }
            Err(e) => {
                self.timeline
                    .add_status(format!("✗ Failed to apply config proposal: {}", e));
            }
        }
    }
}
