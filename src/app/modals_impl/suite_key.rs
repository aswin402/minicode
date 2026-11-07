//! Key handling for MiniPower, MiniBlocks, MiniVault, MiniTask Processes/Swarm, Todo, and CodeReview modals.

use crate::app::App;
use crate::ui::modal::ModalState;
use crossterm::event::{KeyCode, KeyModifiers};

impl<'a> App<'a> {
    pub(super) async fn handle_suite_modals_key(&mut self, key: crossterm::event::KeyEvent) {
        match &mut self.modal {
            ModalState::MiniPower(state) => match key.code {
                KeyCode::Tab | KeyCode::Right => {
                    state.next_tab();
                }
                KeyCode::BackTab | KeyCode::Left => {
                    state.prev_tab();
                }
                KeyCode::Char('1') => {
                    state.active_tab = crate::ui::modals::minipower::MiniPowerTab::Pillars;
                    state.scroll_offset = 0;
                    state.selected_index = 0;
                }
                KeyCode::Char('2') => {
                    state.active_tab = crate::ui::modals::minipower::MiniPowerTab::RedFlags;
                    state.scroll_offset = 0;
                    state.selected_index = 0;
                }
                KeyCode::Char('3') => {
                    state.active_tab =
                        crate::ui::modals::minipower::MiniPowerTab::VerificationBarrier;
                    state.scroll_offset = 0;
                    state.selected_index = 0;
                }
                KeyCode::Char('4') => {
                    state.active_tab = crate::ui::modals::minipower::MiniPowerTab::PlanHierarchy;
                    state.scroll_offset = 0;
                    state.selected_index = 0;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    state.scroll_up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    state.scroll_down();
                }
                KeyCode::Char('r') | KeyCode::Char('v') | KeyCode::Enter => {
                    state.is_evaluating = true;
                    let git = crate::git::GitService::new(self.workspace_root.clone());
                    let modified_files = if git.is_git_repo().await {
                        if let Ok(st) = git.get_status().await {
                            let mut all = st.staged;
                            all.extend(st.unstaged);
                            all.sort();
                            all.dedup();
                            all
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    let report = crate::agent::verification_barrier::VerificationBarrier::verify(
                        &self.workspace_root,
                        &modified_files,
                    )
                    .await;
                    state.set_report(report);
                    state.is_evaluating = false;
                    if state.active_tab
                        != crate::ui::modals::minipower::MiniPowerTab::VerificationBarrier
                    {
                        state.active_tab =
                            crate::ui::modals::minipower::MiniPowerTab::VerificationBarrier;
                        state.scroll_offset = 0;
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::Blocks(state) => match key.code {
                KeyCode::Tab | KeyCode::Right => {
                    state.next_tab();
                }
                KeyCode::BackTab | KeyCode::Left => {
                    state.prev_tab();
                }
                KeyCode::Char('1')
                    if state.active_tab != crate::ui::modals::blocks::BlocksTab::Components
                        || state.search_query.is_empty() =>
                {
                    state.active_tab = crate::ui::modals::blocks::BlocksTab::Components;
                    state.selected_index = 0;
                    state.preview_scroll_offset = 0;
                    state.status_message = None;
                    state.refresh_filtered();
                }
                KeyCode::Char('2')
                    if state.active_tab != crate::ui::modals::blocks::BlocksTab::Components
                        || state.search_query.is_empty() =>
                {
                    state.active_tab = crate::ui::modals::blocks::BlocksTab::Palettes;
                    state.selected_index = 0;
                    state.preview_scroll_offset = 0;
                    state.status_message = None;
                    state.refresh_filtered();
                }
                KeyCode::Char('3')
                    if state.active_tab != crate::ui::modals::blocks::BlocksTab::Components
                        || state.search_query.is_empty() =>
                {
                    state.active_tab = crate::ui::modals::blocks::BlocksTab::Gradients;
                    state.selected_index = 0;
                    state.preview_scroll_offset = 0;
                    state.status_message = None;
                    state.refresh_filtered();
                }
                KeyCode::Char('4')
                    if state.active_tab != crate::ui::modals::blocks::BlocksTab::Components
                        || state.search_query.is_empty() =>
                {
                    state.active_tab = crate::ui::modals::blocks::BlocksTab::Templates;
                    state.selected_index = 0;
                    state.preview_scroll_offset = 0;
                    state.status_message = None;
                    state.refresh_filtered();
                }
                KeyCode::Up => {
                    state.select_prev();
                }
                KeyCode::Down => {
                    state.select_next();
                }
                KeyCode::PageUp => {
                    state.scroll_preview_up();
                }
                KeyCode::PageDown => {
                    state.scroll_preview_down();
                }
                KeyCode::Char('c')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        || state.active_tab != crate::ui::modals::blocks::BlocksTab::Components =>
                {
                    if let Some(code) = state.get_selected_code() {
                        let ok = crate::ui::clipboard::copy_to_clipboard(&code);
                        let msg = if ok {
                            "✔ Copied to clipboard!".to_string()
                        } else {
                            "✗ Failed to copy to clipboard".to_string()
                        };
                        state.status_message = Some(msg.clone());
                        self.timeline.add_status(msg);
                    }
                }
                KeyCode::Enter => {
                    if let Some(code) = state.get_selected_code() {
                        let _ = crate::ui::clipboard::copy_to_clipboard(&code);
                        match state.active_tab {
                            crate::ui::modals::blocks::BlocksTab::Components => {
                                if let Some(id) =
                                    state.filtered_component_ids.get(state.selected_index)
                                {
                                    let store = match crate::blocks::store::get_global_block_store()
                                        .read()
                                    {
                                        Ok(s) => s,
                                        Err(p) => p.into_inner(),
                                    };
                                    if let Some(comp) = store.get_component(id) {
                                        let ext = match comp.framework {
                                            crate::blocks::models::BlockFramework::React => "tsx",
                                            crate::blocks::models::BlockFramework::Svelte => {
                                                "svelte"
                                            }
                                            crate::blocks::models::BlockFramework::Css => "css",
                                            crate::blocks::models::BlockFramework::Scss => "scss",
                                            crate::blocks::models::BlockFramework::Tailwind
                                            | crate::blocks::models::BlockFramework::Shadcn => {
                                                "tsx"
                                            }
                                        };
                                        let clean_stem: String = comp
                                            .name
                                            .chars()
                                            .filter(|c| {
                                                c.is_alphanumeric() || *c == '-' || *c == '_'
                                            })
                                            .collect();
                                        let file_stem = if clean_stem.is_empty() {
                                            "component".to_string()
                                        } else {
                                            clean_stem
                                        };
                                        let file_name = format!("{}.{}", file_stem, ext);
                                        let target_dir = if state
                                            .workspace_root
                                            .join("src")
                                            .join("components")
                                            .exists()
                                        {
                                            state.workspace_root.join("src").join("components")
                                        } else if state.workspace_root.join("components").exists() {
                                            state.workspace_root.join("components")
                                        } else if state.workspace_root.join("src").exists() {
                                            state.workspace_root.join("src").join("components")
                                        } else {
                                            state.workspace_root.join("components")
                                        };
                                        let _ = std::fs::create_dir_all(&target_dir);
                                        let candidate_file = target_dir.join(&file_name);
                                        let validated_file =
                                            crate::sandbox::path::validate_path_in_workspace(
                                                &state.workspace_root,
                                                &candidate_file,
                                            );
                                        match validated_file {
                                            Ok(target_file) => {
                                                match std::fs::write(&target_file, &comp.code) {
                                                    Ok(_) => {
                                                        let rel = target_file
                                                            .strip_prefix(&state.workspace_root)
                                                            .unwrap_or(&target_file);
                                                        let msg = format!(
                                                            "✔ Injected {} into {}",
                                                            comp.name,
                                                            rel.display()
                                                        );
                                                        state.status_message = Some(msg.clone());
                                                        self.timeline.add_status(msg);
                                                    }
                                                    Err(e) => {
                                                        let msg = format!(
                                                            "✔ Copied {} to clipboard (File write note: {})",
                                                            comp.name, e
                                                        );
                                                        state.status_message = Some(msg.clone());
                                                        self.timeline.add_status(msg);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                let msg = format!(
                                                    "✔ Copied {} to clipboard (Security validation note: {})",
                                                    comp.name, e
                                                );
                                                state.status_message = Some(msg.clone());
                                                self.timeline.add_status(msg);
                                            }
                                        }
                                    }
                                }
                            }
                            crate::ui::modals::blocks::BlocksTab::Palettes => {
                                let msg = "✔ Copied palette CSS to clipboard!".to_string();
                                state.status_message = Some(msg.clone());
                                self.timeline.add_status(msg);
                            }
                            crate::ui::modals::blocks::BlocksTab::Gradients => {
                                let msg = "✔ Copied CSS gradient to clipboard!".to_string();
                                state.status_message = Some(msg.clone());
                                self.timeline.add_status(msg);
                            }
                            crate::ui::modals::blocks::BlocksTab::Templates => {
                                let msg = "✔ Copied template layout to clipboard!".to_string();
                                state.status_message = Some(msg.clone());
                                self.timeline.add_status(msg);
                            }
                        }
                    }
                }
                KeyCode::Esc | KeyCode::F(6) => {
                    self.modal = ModalState::None;
                }
                KeyCode::Backspace => {
                    state.handle_backspace();
                }
                KeyCode::Char(c) => {
                    state.handle_char(c);
                }
                _ => {}
            },
            ModalState::Vault(state) => match key.code {
                KeyCode::Tab | KeyCode::Right => {
                    state.next_tab();
                }
                KeyCode::BackTab | KeyCode::Left => {
                    state.prev_tab();
                }
                KeyCode::Char('1') if state.search_query.is_empty() => {
                    state.active_tab = crate::ui::modals::vault::VaultTab::All;
                    state.selected_index = 0;
                    state.preview_scroll_offset = 0;
                    state.status_message = None;
                    state.refresh_filtered();
                }
                KeyCode::Char('2') if state.search_query.is_empty() => {
                    state.active_tab = crate::ui::modals::vault::VaultTab::Project;
                    state.selected_index = 0;
                    state.preview_scroll_offset = 0;
                    state.status_message = None;
                    state.refresh_filtered();
                }
                KeyCode::Char('3') if state.search_query.is_empty() => {
                    state.active_tab = crate::ui::modals::vault::VaultTab::Global;
                    state.selected_index = 0;
                    state.preview_scroll_offset = 0;
                    state.status_message = None;
                    state.refresh_filtered();
                }
                KeyCode::Char('4') if state.search_query.is_empty() => {
                    state.active_tab = crate::ui::modals::vault::VaultTab::Builtin;
                    state.selected_index = 0;
                    state.preview_scroll_offset = 0;
                    state.status_message = None;
                    state.refresh_filtered();
                }
                KeyCode::Up => {
                    state.select_prev();
                }
                KeyCode::Down => {
                    state.select_next();
                }
                KeyCode::PageUp => {
                    state.scroll_preview_up();
                }
                KeyCode::PageDown => {
                    state.scroll_preview_down();
                }
                KeyCode::Enter => {
                    state.toggle_project_load();
                    if let Some(ref msg) = state.status_message {
                        let clean = msg.trim_start_matches(['✔', '✓', '❌', '✗', 'ℹ']).trim();
                        let (title, badge) = if clean.contains("Unloaded") {
                            ("Skill Unloaded", "Removed")
                        } else {
                            ("Skill Loaded", "Loaded")
                        };
                        self.toasts.push(
                            crate::ui::ToastKind::Skill,
                            title,
                            clean,
                            Some(badge.to_string()),
                            Some("skill:vault_modal".to_string()),
                        );
                        self.timeline.add_status(msg.clone());
                    }
                }
                KeyCode::Char(' ') if state.search_query.is_empty() => {
                    state.toggle_project_load();
                    if let Some(ref msg) = state.status_message {
                        let clean = msg.trim_start_matches(['✔', '✓', '❌', '✗', 'ℹ']).trim();
                        let (title, badge) = if clean.contains("Unloaded") {
                            ("Skill Unloaded", "Removed")
                        } else {
                            ("Skill Loaded", "Loaded")
                        };
                        self.toasts.push(
                            crate::ui::ToastKind::Skill,
                            title,
                            clean,
                            Some(badge.to_string()),
                            Some("skill:vault_modal".to_string()),
                        );
                        self.timeline.add_status(msg.clone());
                    }
                }
                KeyCode::Char('d') if state.search_query.is_empty() => {
                    state.delete_selected();
                    if let Some(ref msg) = state.status_message {
                        let clean = msg.trim_start_matches(['✔', '✓', '❌', '✗', 'ℹ']).trim();
                        self.toasts.push(
                            crate::ui::ToastKind::Skill,
                            "Skill Unloaded",
                            clean,
                            Some("Removed".to_string()),
                            Some("skill:vault_modal".to_string()),
                        );
                        self.timeline.add_status(msg.clone());
                    }
                }
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                }
                KeyCode::Backspace => {
                    state.handle_backspace();
                }
                KeyCode::Char(c) => {
                    state.handle_char(c);
                }
                _ => {}
            },
            ModalState::Processes(state) => match key.code {
                KeyCode::Tab | KeyCode::Right if !state.is_searching => {
                    state.next_tab();
                    state.refresh_live().await;
                    if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm {
                        let active_style = self.config.ui.swarm_style_enum();
                        let selected = state
                            .swarm_deck_data
                            .as_ref()
                            .map(|d| d.selected_index)
                            .unwrap_or(0);
                        state.swarm_deck_data = Some(Box::new(
                            crate::ui::modals::swarm_deck::SwarmDeckData::load(
                                &self.workspace_root,
                                selected,
                                active_style,
                            )
                            .await,
                        ));
                    }
                }
                KeyCode::BackTab | KeyCode::Left if !state.is_searching => {
                    state.prev_tab();
                    state.refresh_live().await;
                    if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm {
                        let active_style = self.config.ui.swarm_style_enum();
                        let selected = state
                            .swarm_deck_data
                            .as_ref()
                            .map(|d| d.selected_index)
                            .unwrap_or(0);
                        state.swarm_deck_data = Some(Box::new(
                            crate::ui::modals::swarm_deck::SwarmDeckData::load(
                                &self.workspace_root,
                                selected,
                                active_style,
                            )
                            .await,
                        ));
                    }
                }
                KeyCode::Char('1') if !state.is_searching => {
                    state.set_tab(crate::ui::modals::processes::ProcessesTab::All);
                    state.refresh_live().await;
                }
                KeyCode::Char('2') if !state.is_searching => {
                    state.set_tab(crate::ui::modals::processes::ProcessesTab::Servers);
                    state.refresh_live().await;
                }
                KeyCode::Char('3') if !state.is_searching => {
                    state.set_tab(crate::ui::modals::processes::ProcessesTab::Workers);
                    state.refresh_live().await;
                }
                KeyCode::Char('4') if !state.is_searching => {
                    let active_style = self.config.ui.swarm_style_enum();
                    let selected = state
                        .swarm_deck_data
                        .as_ref()
                        .map(|d| d.selected_index)
                        .unwrap_or(0);
                    state.swarm_deck_data = Some(Box::new(
                        crate::ui::modals::swarm_deck::SwarmDeckData::load(
                            &self.workspace_root,
                            selected,
                            active_style,
                        )
                        .await,
                    ));
                    state.set_tab(crate::ui::modals::processes::ProcessesTab::Swarm);
                    state.refresh_live().await;
                }
                KeyCode::Char('5') if !state.is_searching => {
                    if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm {
                        if let Some(ref data) = state.swarm_deck_data {
                            if let Some(worker) = data.selected_worker() {
                                let pid = worker.id.clone();
                                let registry = crate::dev::registry::get_global_dev_registry();
                                if let Ok(logs) = registry.logs(&pid, 500, None).await {
                                    state.set_logs(logs);
                                }
                            }
                        }
                    } else if let Some(p) = state.selected_process() {
                        let pid = p.id.clone();
                        let registry = crate::dev::registry::get_global_dev_registry();
                        if let Ok(logs) = registry.logs(&pid, 500, None).await {
                            state.set_logs(logs);
                        }
                    }
                    state.set_tab(crate::ui::modals::processes::ProcessesTab::Logs);
                }
                KeyCode::Char('6') if !state.is_searching => {
                    state.set_tab(crate::ui::modals::processes::ProcessesTab::Telemetry);
                    state.refresh_live().await;
                }
                KeyCode::Up => {
                    if state.active_tab == crate::ui::modals::processes::ProcessesTab::Logs {
                        state.scroll_logs_up(1);
                    } else if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm
                    {
                        if let Some(ref mut data) = state.swarm_deck_data {
                            data.select_prev();
                            if let Some(w) = data.selected_worker() {
                                let wid = w.id.clone();
                                let registry = crate::dev::registry::get_global_dev_registry();
                                if let Ok(logs) = registry.logs(&wid, 50, None).await {
                                    data.selected_worker_logs = logs;
                                }
                            }
                        }
                    } else {
                        state.select_prev();
                        if let Some(p) = state.selected_process() {
                            let pid = p.id.clone();
                            let registry = crate::dev::registry::get_global_dev_registry();
                            if let Ok(logs) = registry.logs(&pid, 50, None).await {
                                state.set_logs(logs);
                            }
                        }
                    }
                }
                KeyCode::Down => {
                    if state.active_tab == crate::ui::modals::processes::ProcessesTab::Logs {
                        state.scroll_logs_down(1);
                    } else if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm
                    {
                        if let Some(ref mut data) = state.swarm_deck_data {
                            data.select_next();
                            if let Some(w) = data.selected_worker() {
                                let wid = w.id.clone();
                                let registry = crate::dev::registry::get_global_dev_registry();
                                if let Ok(logs) = registry.logs(&wid, 50, None).await {
                                    data.selected_worker_logs = logs;
                                }
                            }
                        }
                    } else {
                        state.select_next();
                        if let Some(p) = state.selected_process() {
                            let pid = p.id.clone();
                            let registry = crate::dev::registry::get_global_dev_registry();
                            if let Ok(logs) = registry.logs(&pid, 50, None).await {
                                state.set_logs(logs);
                            }
                        }
                    }
                }
                KeyCode::PageUp => {
                    state.scroll_logs_up(15);
                }
                KeyCode::PageDown => {
                    state.scroll_logs_down(15);
                }
                KeyCode::Char('a')
                    if state.active_tab == crate::ui::modals::processes::ProcessesTab::Logs =>
                {
                    state.auto_scroll_logs = !state.auto_scroll_logs;
                    let msg = if state.auto_scroll_logs {
                        "Auto-scroll: ON".to_string()
                    } else {
                        "Auto-scroll: OFF".to_string()
                    };
                    state.status_message = Some(msg);
                }
                KeyCode::Char('k') | KeyCode::Char('x') if !state.is_searching => {
                    let target_id =
                        if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm {
                            state
                                .swarm_deck_data
                                .as_ref()
                                .and_then(|d| d.selected_worker().map(|w| w.id.clone()))
                        } else {
                            state.selected_process().map(|p| p.id.clone())
                        };

                    if let Some(target_id) = target_id {
                        let minipub_id = target_id
                            .as_str()
                            .strip_prefix("worker-")
                            .unwrap_or(target_id.as_str());
                        if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                            let _ = pool.kill_subagent(minipub_id).await;
                        }
                        let registry = crate::dev::registry::get_global_dev_registry();
                        match registry.stop(&target_id).await {
                            Ok(_) => {
                                let msg = format!("✔ Stopped process '{}'", target_id);
                                state.status_message = Some(msg.clone());
                                if let Some(ref mut data) = state.swarm_deck_data {
                                    data.status_message = Some(msg.clone());
                                }
                                self.timeline.add_status(msg);
                                state.refresh_live().await;
                            }
                            Err(e) => {
                                let msg =
                                    format!("✗ Failed to stop process '{}': {}", target_id, e);
                                state.status_message = Some(msg.clone());
                                if let Some(ref mut data) = state.swarm_deck_data {
                                    data.status_message = Some(msg.clone());
                                }
                                self.timeline.add_status(msg);
                            }
                        }
                    }
                }
                KeyCode::Char('r') if !state.is_searching => {
                    if let Some(p) = state.selected_process() {
                        let target_id = p.id.clone();
                        let registry = crate::dev::registry::get_global_dev_registry();
                        match registry.restart(&self.workspace_root, &target_id).await {
                            Ok(summary) => {
                                let msg = format!(
                                    "✔ Restarted process '{}' (PID: {:?})",
                                    summary.id, summary.pid
                                );
                                state.status_message = Some(msg.clone());
                                self.timeline.add_status(msg);
                                state.refresh_live().await;
                            }
                            Err(e) => {
                                let msg =
                                    format!("✗ Failed to restart process '{}': {}", target_id, e);
                                state.status_message = Some(msg.clone());
                                self.timeline.add_status(msg);
                            }
                        }
                    }
                }
                KeyCode::Char('l') | KeyCode::Enter if !state.is_searching => {
                    if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm {
                        if let Some(ref data) = state.swarm_deck_data {
                            if let Some(worker) = data.selected_worker() {
                                let pid = worker.id.clone();
                                let registry = crate::dev::registry::get_global_dev_registry();
                                if let Ok(logs) = registry.logs(&pid, 500, None).await {
                                    state.set_logs(logs);
                                }
                                state.set_tab(crate::ui::modals::processes::ProcessesTab::Logs);
                            }
                        }
                    } else if let Some(p) = state.selected_process() {
                        let pid = p.id.clone();
                        let registry = crate::dev::registry::get_global_dev_registry();
                        if let Ok(logs) = registry.logs(&pid, 500, None).await {
                            state.set_logs(logs);
                        }
                        state.set_tab(crate::ui::modals::processes::ProcessesTab::Logs);
                    }
                }
                KeyCode::Char(' ')
                    if !state.is_searching
                        && state.active_tab
                            == crate::ui::modals::processes::ProcessesTab::Swarm =>
                {
                    if let Some(ref data) = state.swarm_deck_data {
                        if let Some(worker) = data.selected_worker() {
                            let pid = worker.id.clone();
                            let registry = crate::dev::registry::get_global_dev_registry();
                            if let Ok(logs) = registry.logs(&pid, 500, None).await {
                                state.set_logs(logs);
                            }
                            state.set_tab(crate::ui::modals::processes::ProcessesTab::Logs);
                        }
                    }
                }
                KeyCode::Char('s') if !state.is_searching => {
                    if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm {
                        let next_style = if let Some(ref mut data) = state.swarm_deck_data {
                            let next = data.cycle_style();
                            data.status_message = Some(format!("Swarm style: {}", next.as_str()));
                            next
                        } else {
                            let next = self.config.ui.swarm_style_enum().next();
                            self.config.ui.swarm_style = next.as_str().to_string();
                            let _ = self.config.save(Some(&self.workspace_root));
                            state.swarm_deck_data = Some(Box::new(
                                crate::ui::modals::swarm_deck::SwarmDeckData::load(
                                    &self.workspace_root,
                                    0,
                                    next,
                                )
                                .await,
                            ));
                            return;
                        };

                        self.config.ui.swarm_style = next_style.as_str().to_string();
                        let _ = self.config.save(Some(&self.workspace_root));
                        self.timeline.add_status(format!(
                            "🎨 Swarm flight deck style set to '{}'",
                            next_style.as_str()
                        ));
                    } else if let Some(p) = state.selected_process() {
                        if let Some(ref url) = p.url {
                            let url_str = url.clone();
                            self.timeline.add_status(format!(
                                "📸 Capturing screenshot for '{}'...",
                                url_str
                            ));
                            let ws = self.workspace_root.clone();
                            match crate::tools::browser::BrowserController::navigate_and_snapshot(
                                &url_str,
                                crate::tools::browser::BrowserMode::Headless,
                                &ws,
                            )
                            .await
                            {
                                Ok(_) => {
                                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                    match crate::tools::browser::BrowserController::take_screenshot(
                                        crate::tools::browser::BrowserMode::Headless,
                                        &ws,
                                        None,
                                    )
                                    .await
                                    {
                                        Ok(res_msg) => {
                                            let msg = format!("✔ Screenshot saved: {}", res_msg);
                                            state.status_message = Some(msg.clone());
                                            self.timeline.add_status(msg);
                                        }
                                        Err(e) => {
                                            let msg = format!("✗ Screenshot failed: {}", e);
                                            state.status_message = Some(msg.clone());
                                            self.timeline.add_status(msg);
                                        }
                                    }
                                }
                                Err(e) => {
                                    let msg = format!("✗ Browser navigation failed: {}", e);
                                    state.status_message = Some(msg.clone());
                                    self.timeline.add_status(msg);
                                }
                            }
                        } else {
                            state.status_message = Some(
                                "⚠️ Selected process has no listening URL for screenshot."
                                    .to_string(),
                            );
                        }
                    }
                }
                KeyCode::Char('/') if !state.is_searching => {
                    state.is_searching = true;
                }
                KeyCode::Esc => {
                    if state.is_searching {
                        state.is_searching = false;
                        state.search_query.clear();
                        state.refresh_filtered();
                    } else if state.active_tab == crate::ui::modals::processes::ProcessesTab::Logs
                        || state.active_tab == crate::ui::modals::processes::ProcessesTab::Telemetry
                    {
                        state.set_tab(crate::ui::modals::processes::ProcessesTab::All);
                    } else {
                        self.modal = ModalState::None;
                    }
                }
                KeyCode::F(7) => {
                    self.modal = ModalState::None;
                }
                KeyCode::Backspace => {
                    state.handle_backspace();
                }
                KeyCode::Char(c) => {
                    state.handle_char(c);
                }
                _ => {}
            },
            ModalState::Todo(state) => match key.code {
                KeyCode::Tab | KeyCode::BackTab => {
                    state.toggle_pane();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    state.prev();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    state.next();
                }
                KeyCode::PageUp => {
                    state.page_up();
                }
                KeyCode::PageDown => {
                    state.page_down();
                }
                KeyCode::Home => {
                    state.first();
                }
                KeyCode::End => {
                    state.last();
                }
                KeyCode::Char(' ') | KeyCode::Char('a') => {
                    state.toggle_expand();
                }
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(8) => {
                    self.modal = ModalState::None;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::CodeReview(ref mut state) => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    state.prev();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    state.next();
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    let next_staged = !state.staged_only;
                    if let Ok(report) =
                        crate::git::GitReviewer::review_workspace(&self.workspace_root, next_staged)
                            .await
                    {
                        self.modal = ModalState::new_code_review(report, next_staged);
                    }
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    let is_staged = state.staged_only;
                    if let Ok(diff_files) =
                        crate::git::GitDiffViewer::load_diffs(&self.workspace_root, is_staged).await
                    {
                        self.modal = ModalState::new_git_diff(diff_files, is_staged);
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            _ => {}
        }
    }
}
