//! Key handling for Approval, Inquiry (`ask_user`), Workspace Analysis/Drift, Architecture Audit, and Context Diagnostics modals.

use crate::app::{AgentCommand, App, PendingSubmission};
use crate::ui::modal::ModalState;
use crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::mpsc;

impl<'a> App<'a> {
    pub(super) async fn handle_agent_and_workspace_modal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
        match &mut self.modal {
            ModalState::Approval(ref mut approval_state) => match key.code {
                KeyCode::Esc => {
                    let pending_tool_id = approval_state.tool_id.clone();
                    self.resolve_approval(
                        &pending_tool_id,
                        crate::agent::types::ApprovalDecision::Reject,
                    );
                    self.modal = ModalState::None;
                    if let Some(token) = self.cancel_token.take() {
                        token.cancel();
                    }
                    self.is_working = false;
                    self.current_activity = None;
                    let elapsed_secs = self.work_start.take().map(|s| s.elapsed().as_secs_f64());
                    self.timeline.finalize_pending_thoughts(elapsed_secs);
                    self.timeline
                        .add_status("⏹ Action & turn cancelled by user (Esc)".to_string());
                    self.timeline.auto_scroll.set(true);
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
                                self.timeline.add_user_message(feedback.clone());
                                self.timeline.auto_scroll.set(true);
                                self.modal = ModalState::None;

                                self.dispatch_agent_prompt(
                                    feedback,
                                    crate::ui::AgentActivity::Thinking,
                                    control_tx,
                                );
                            }
                        }
                    }
                }
                _ => {}
            },
            ModalState::Inquiry(ref mut inq_state) => match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let resp = inq_state.cancel();
                    let tid = inq_state.tool_id.clone();
                    self.resolve_inquiry(&tid, resp);
                    self.modal = ModalState::None;
                }
                KeyCode::Esc => {
                    if inq_state.is_typing_custom && inq_state.total_options_count() > 0 {
                        inq_state.is_typing_custom = false;
                    } else {
                        let resp = inq_state.cancel();
                        let tid = inq_state.tool_id.clone();
                        self.resolve_inquiry(&tid, resp);
                        self.modal = ModalState::None;
                    }
                }
                KeyCode::Up => {
                    if !inq_state.is_typing_custom {
                        let is_rank = inq_state.current_question().is_some_and(|q| {
                            q.input_type == crate::agent::inquiry::InquiryInputType::Rank
                        });
                        if is_rank
                            && (key.modifiers.contains(KeyModifiers::SHIFT)
                                || key.modifiers.contains(KeyModifiers::ALT))
                        {
                            inq_state.move_rank_up();
                        } else {
                            inq_state.prev_option();
                        }
                    }
                }
                KeyCode::Down => {
                    if !inq_state.is_typing_custom {
                        let is_rank = inq_state.current_question().is_some_and(|q| {
                            q.input_type == crate::agent::inquiry::InquiryInputType::Rank
                        });
                        if is_rank
                            && (key.modifiers.contains(KeyModifiers::SHIFT)
                                || key.modifiers.contains(KeyModifiers::ALT))
                        {
                            inq_state.move_rank_down();
                        } else {
                            inq_state.next_option();
                        }
                    }
                }
                KeyCode::Tab => {
                    if !inq_state.is_typing_custom {
                        if inq_state.request.questions.len() > 1 {
                            inq_state.next_question_tab();
                        } else {
                            inq_state.next_option();
                        }
                    }
                }
                KeyCode::BackTab => {
                    if !inq_state.is_typing_custom {
                        if inq_state.request.questions.len() > 1 {
                            inq_state.prev_question_tab();
                        } else {
                            inq_state.prev_option();
                        }
                    }
                }
                KeyCode::Left => {
                    if inq_state.is_typing_custom {
                        inq_state.cursor_left();
                    } else if inq_state.current_question().is_some_and(|q| {
                        q.input_type == crate::agent::inquiry::InquiryInputType::Slider
                    }) {
                        inq_state.slider_decrement();
                    } else if inq_state.request.questions.len() > 1 {
                        inq_state.prev_question_tab();
                    }
                }
                KeyCode::Right => {
                    if inq_state.is_typing_custom {
                        inq_state.cursor_right();
                    } else if inq_state.current_question().is_some_and(|q| {
                        q.input_type == crate::agent::inquiry::InquiryInputType::Slider
                    }) {
                        inq_state.slider_increment();
                    } else if inq_state.request.questions.len() > 1 {
                        inq_state.next_question_tab();
                    }
                }
                KeyCode::PageUp => {
                    if !inq_state.is_typing_custom {
                        inq_state.selected_option_idx = 0;
                    }
                }
                KeyCode::PageDown => {
                    if !inq_state.is_typing_custom {
                        let total = inq_state.total_options_count();
                        if total > 0 {
                            inq_state.selected_option_idx = total - 1;
                        }
                    }
                }
                KeyCode::Home => {
                    if inq_state.is_typing_custom {
                        inq_state.cursor_home();
                    } else {
                        inq_state.selected_option_idx = 0;
                    }
                }
                KeyCode::End => {
                    if inq_state.is_typing_custom {
                        inq_state.cursor_end();
                    } else {
                        let total = inq_state.total_options_count();
                        if total > 0 {
                            inq_state.selected_option_idx = total - 1;
                        }
                    }
                }
                KeyCode::Delete => {
                    if inq_state.is_typing_custom {
                        inq_state.handle_delete();
                    }
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if inq_state.is_typing_custom {
                        inq_state.cursor_home();
                    }
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if inq_state.is_typing_custom {
                        inq_state.cursor_end();
                    }
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if inq_state.is_typing_custom {
                        inq_state.clear_line();
                    }
                }
                KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if inq_state.is_typing_custom {
                        inq_state.kill_to_end();
                    }
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if inq_state.is_typing_custom {
                        inq_state.handle_delete();
                    }
                }
                KeyCode::Char(c) => {
                    if inq_state.is_typing_custom {
                        inq_state.handle_char(c);
                    } else if c == ']' && inq_state.request.questions.len() > 1 {
                        inq_state.next_question_tab();
                    } else if c == '[' && inq_state.request.questions.len() > 1 {
                        inq_state.prev_question_tab();
                    } else if inq_state.current_question().is_some_and(|q| {
                        q.input_type == crate::agent::inquiry::InquiryInputType::Slider
                    }) && matches!(c, 'h' | 'H' | '-')
                    {
                        inq_state.slider_decrement();
                    } else if inq_state.current_question().is_some_and(|q| {
                        q.input_type == crate::agent::inquiry::InquiryInputType::Slider
                    }) && matches!(c, 'l' | 'L' | '+' | '=')
                    {
                        inq_state.slider_increment();
                    } else if inq_state.current_question().is_some_and(|q| {
                        q.input_type == crate::agent::inquiry::InquiryInputType::Slider
                    }) && ('0'..='9').contains(&c)
                    {
                        inq_state.set_slider_value((c as u8 - b'0') as i64);
                    } else if inq_state.current_question().is_some_and(|q| {
                        q.input_type == crate::agent::inquiry::InquiryInputType::Confirm
                    }) && matches!(c, 'y' | 'Y' | 'n' | 'N')
                    {
                        inq_state.selected_option_idx = if matches!(c, 'y' | 'Y') { 0 } else { 1 };
                        if let Some(resp) = inq_state.confirm_selection() {
                            let tid = inq_state.tool_id.clone();
                            self.resolve_inquiry(&tid, resp);
                            self.modal = ModalState::None;
                        }
                    } else if ('1'..='9').contains(&c) {
                        let num = (c as u8 - b'1') as usize;
                        let (is_multi, is_rank, options_len) = inq_state
                            .current_question()
                            .map(|q| {
                                (
                                    q.is_multi_select,
                                    q.input_type == crate::agent::inquiry::InquiryInputType::Rank,
                                    q.options.len(),
                                )
                            })
                            .unwrap_or((false, false, 0));
                        if num < options_len {
                            inq_state.selected_option_idx = num;
                            if is_multi {
                                inq_state.toggle_multi();
                            } else if !is_rank {
                                if let Some(resp) = inq_state.confirm_selection() {
                                    let tid = inq_state.tool_id.clone();
                                    self.resolve_inquiry(&tid, resp);
                                    self.modal = ModalState::None;
                                }
                            }
                        }
                    } else if c == ' ' {
                        inq_state.toggle_multi();
                    } else if c == 'J'
                        && inq_state.current_question().is_some_and(|q| {
                            q.input_type == crate::agent::inquiry::InquiryInputType::Rank
                        })
                    {
                        inq_state.move_rank_down();
                    } else if c == 'K'
                        && inq_state.current_question().is_some_and(|q| {
                            q.input_type == crate::agent::inquiry::InquiryInputType::Rank
                        })
                    {
                        inq_state.move_rank_up();
                    } else if c == 'j' || c == 'J' {
                        inq_state.next_option();
                    } else if c == 'k' || c == 'K' {
                        inq_state.prev_option();
                    } else if c == 'o' || c == 'O' || c == 'w' || c == 'W' {
                        if let Some(q) = inq_state.current_question() {
                            if q.allow_custom {
                                inq_state.selected_option_idx = q.options.len();
                                inq_state.is_typing_custom = true;
                            }
                        }
                    }
                }
                KeyCode::Backspace => {
                    if inq_state.is_typing_custom {
                        inq_state.handle_backspace();
                    }
                }
                KeyCode::Enter => {
                    if let Some(resp) = inq_state.confirm_selection() {
                        let tid = inq_state.tool_id.clone();
                        self.resolve_inquiry(&tid, resp);
                        self.modal = ModalState::None;
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
                    self.execute_workspace_analysis_action(act, is_indexed_flag, control_tx);
                }
            }
            ModalState::WorkspaceDrift {
                workspace_path: _,
                modified_count: _,
                added_count: _,
                removed_count: _,
                selected_index,
            } => {
                let mut triggered_action: Option<usize> = None;
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                        triggered_action = Some(2); // Skip
                    }
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                        *selected_index = selected_index.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                        if *selected_index < 2 {
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
                    KeyCode::Enter => {
                        triggered_action = Some(*selected_index);
                    }
                    _ => {}
                }

                if let Some(act) = triggered_action {
                    self.modal = ModalState::None;
                    self.execute_workspace_drift_action(act, control_tx);
                }
            }
            ModalState::ProviderSetupRequired {
                provider_name,
                pending_prompt_preview: _,
                selected_index,
            } => {
                let prov = provider_name.clone();
                let mut triggered_action: Option<usize> = None;
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                        triggered_action = Some(1); // Cancel
                    }
                    KeyCode::Up
                    | KeyCode::Char('k')
                    | KeyCode::Char('K')
                    | KeyCode::Down
                    | KeyCode::Char('j')
                    | KeyCode::Char('J')
                    | KeyCode::Tab => {
                        *selected_index = (*selected_index + 1) % 2;
                    }
                    KeyCode::Char('1') => {
                        triggered_action = Some(0);
                    }
                    KeyCode::Char('2') => {
                        triggered_action = Some(1);
                    }
                    KeyCode::Enter => {
                        triggered_action = Some(*selected_index);
                    }
                    _ => {}
                }

                if let Some(act) = triggered_action {
                    self.modal = ModalState::None;
                    self.execute_provider_setup_action(act, &prov);
                }
            }
            ModalState::ArchitectureAudit {
                report,
                active_tab,
                selected_index,
                scroll_offset,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Tab => {
                    *active_tab = (*active_tab + 1) % 3;
                    *selected_index = 0;
                    *scroll_offset = 0;
                }
                KeyCode::BackTab => {
                    *active_tab = if *active_tab == 0 { 2 } else { *active_tab - 1 };
                    *selected_index = 0;
                    *scroll_offset = 0;
                }
                KeyCode::Char('1') => {
                    *active_tab = 0;
                    *selected_index = 0;
                    *scroll_offset = 0;
                }
                KeyCode::Char('2') => {
                    *active_tab = 1;
                    *selected_index = 0;
                    *scroll_offset = 0;
                }
                KeyCode::Char('3') => {
                    *active_tab = 2;
                    *selected_index = 0;
                    *scroll_offset = 0;
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                    *selected_index = selected_index.saturating_sub(1);
                    if *selected_index < *scroll_offset {
                        *scroll_offset = *selected_index;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                    let total_items = match *active_tab {
                        0 => report.layer_violations.len(),
                        1 => report.coupling_metrics.len(),
                        2 => report.circular_cycles.len(),
                        _ => 0,
                    };
                    if total_items > 0 && *selected_index + 1 < total_items {
                        *selected_index += 1;
                        if *selected_index >= *scroll_offset + 12 {
                            *scroll_offset = selected_index.saturating_sub(11);
                        }
                    }
                }
                _ => {}
            },
            ModalState::ContextDiagnostics {
                data,
                active_tab,
                scroll_offset,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Tab => {
                    *active_tab = (*active_tab + 1) % 3;
                    *scroll_offset = 0;
                }
                KeyCode::BackTab => {
                    *active_tab = if *active_tab == 0 { 2 } else { *active_tab - 1 };
                    *scroll_offset = 0;
                }
                KeyCode::Char('1') => {
                    *active_tab = 0;
                    *scroll_offset = 0;
                }
                KeyCode::Char('2') => {
                    *active_tab = 1;
                    *scroll_offset = 0;
                }
                KeyCode::Char('3') => {
                    *active_tab = 2;
                    *scroll_offset = 0;
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                    *scroll_offset = scroll_offset.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                    *scroll_offset = scroll_offset.saturating_add(1);
                }
                KeyCode::PageUp => {
                    *scroll_offset = scroll_offset.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    *scroll_offset = scroll_offset.saturating_add(10);
                }
                KeyCode::Home => {
                    *scroll_offset = 0;
                }
                KeyCode::Char('p') | KeyCode::Char('P') => {
                    let mut prog_mem =
                        crate::context::memory::progressive_memory::ProgressiveMemory::load(
                            &self.workspace_root,
                        );
                    let before =
                        prog_mem.l1_session_anchors.len() + prog_mem.l2_project_facts.len();
                    prog_mem.prune_decayed(0.15);
                    let pruned = before.saturating_sub(
                        prog_mem.l1_session_anchors.len() + prog_mem.l2_project_facts.len(),
                    );
                    let _ = prog_mem.save(&self.workspace_root);

                    crate::context::budget::ccr_cache::CcrCache::clear();

                    let used_tokens =
                        Self::estimate_context_tokens(self.last_turn_tokens, &self.timeline);
                    let cumulative = self.cumulative_tokens;
                    let cached = self.last_turn_cached_tokens;
                    let msg_count = self.timeline.entries.len();
                    **data = crate::ui::modals::context_diagnostics::ContextDiagnosticsData::gather(
                        &self.workspace_root,
                        &self.config,
                        used_tokens,
                        cumulative,
                        cached,
                        msg_count,
                    );
                    self.timeline.add_status(format!(
                        "🧹 Pruned {} decayed memory entries & flushed observation CCR cache",
                        pruned
                    ));
                }
                _ => {}
            },
            _ => {}
        }
    }
    /// Executes the selected workspace analysis action from the interactive modal
    fn execute_workspace_analysis_action(
        &mut self,
        action_index: usize,
        is_indexed: bool,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
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
                    self.session_skipped_indexing = true;
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

        // Resume any pending submission
        if let Some(sub) = self.pending_submission.take() {
            self.dispatch_pending_prompt(sub, control_tx);
        }
    }

    /// Executes the selected workspace drift action and resumes any pending submission
    fn execute_workspace_drift_action(
        &mut self,
        action_index: usize,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
        self.modal = ModalState::None;
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
                            .add_status(format!("✗ Full rebuild failed: {}", e));
                    }
                }
            }
            _ => {
                // Skip for this session
                self.session_skipped_drift = true;
                self.timeline.add_status(
                    "⏩ Workspace drift sync skipped. Continuing with existing cached graph."
                        .to_string(),
                );
            }
        }

        // Resume any pending submission
        if let Some(sub) = self.pending_submission.take() {
            self.dispatch_pending_prompt(sub, control_tx);
        }
    }

    /// Executes the selected action when provider setup is required
    fn execute_provider_setup_action(&mut self, action_index: usize, provider_name: &str) {
        self.modal = ModalState::None;
        match action_index {
            0 => {
                // Launch setup / API key prompt
                let env_var = crate::ui::setup::SetupWizard::custom_env_var(provider_name);
                self.modal =
                    ModalState::new_api_key_input(provider_name.to_string(), env_var, None);
            }
            _ => {
                // Cancel: restore pending submission into input dock textarea
                if let Some(sub) = self.pending_submission.take() {
                    self.input_dock.textarea = tui_textarea::TextArea::default();
                    self.input_dock.textarea.insert_str(&sub.display);
                    self.timeline.add_status("ℹ Prompt cancelled.".to_string());
                }
            }
        }
    }

    /// Dispatches a pending submission to the agent actor
    fn dispatch_pending_prompt(
        &mut self,
        sub: PendingSubmission,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
        self.timeline.add_user_message(sub.prompt.clone());
        if self.last_turn_tokens == 0 {
            self.last_turn_tokens = sub.prompt.len().max(4) / 4;
        }
        self.dispatch_agent_prompt(sub.prompt, crate::ui::AgentActivity::Thinking, control_tx);
    }
}
