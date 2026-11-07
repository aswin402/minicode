//! Subagent tree/swarm state management, transcript export, and mouse selection handlers for `TimelineView`.

use super::{
    LivePlanTaskStatus, SubagentItemStatus, SubagentTreeBlock, SubagentTreeItem, SwarmMatrixBlock,
    TimelineEntry, TimelineView,
};

impl TimelineView {
    /// Adds a subagent tree block to the timeline
    #[allow(dead_code)]
    pub fn add_subagent_tree(&mut self, block: SubagentTreeBlock) {
        self.entries.push(TimelineEntry::SubagentTree(block));
    }

    /// Appends or updates a tool execution item on an active subagent tree block
    #[allow(dead_code)]
    pub fn update_subagent_tree_item(&mut self, id: &str, item: SubagentTreeItem) {
        for entry in self.entries.iter_mut().rev() {
            if let TimelineEntry::SubagentTree(ref mut block) = entry {
                if block.id == id {
                    block.items.push(item);
                    return;
                }
            }
        }
    }

    /// Completes a subagent tree block with outcome/error status
    #[allow(dead_code)]
    pub fn complete_subagent_tree(
        &mut self,
        id: &str,
        is_success: bool,
        outcome: Option<String>,
        error_message: Option<String>,
        tokens_used: usize,
        duration_ms: Option<u64>,
    ) {
        for entry in self.entries.iter_mut().rev() {
            if let TimelineEntry::SubagentTree(ref mut block) = entry {
                if block.id == id {
                    block.is_running = false;
                    block.is_success = is_success;
                    block.outcome = outcome;
                    block.error_message = error_message.clone();
                    block.tokens_used = tokens_used;
                    block.duration_ms = duration_ms;
                    for item in block.items.iter_mut() {
                        if matches!(item.status, SubagentItemStatus::Running) {
                            item.status = if is_success {
                                SubagentItemStatus::Success
                            } else {
                                SubagentItemStatus::Failed(
                                    error_message.clone().unwrap_or_default(),
                                )
                            };
                        }
                    }
                    return;
                }
            }
        }
    }

    /// Adds a multi-worker subagent swarm matrix block to the timeline
    #[allow(dead_code)]
    pub fn add_subagent_swarm(&mut self, swarm: SwarmMatrixBlock) {
        self.entries.push(TimelineEntry::SubagentSwarm(swarm));
    }

    /// Toggles expanded/collapsed state of the active swarm card
    #[allow(dead_code)]
    pub fn toggle_subagent_swarm(&mut self) {
        for entry in self.entries.iter_mut().rev() {
            if let TimelineEntry::SubagentSwarm(ref mut swarm) = entry {
                swarm.is_expanded = !swarm.is_expanded;
                return;
            }
        }
    }

    /// Ingests a SubagentProgress event and updates or creates a SubagentTree block.
    pub fn handle_subagent_progress(
        &mut self,
        subagent_id: &str,
        role: &str,
        action: &str,
        status: &str,
    ) {
        let item_status = match status.to_lowercase().as_str() {
            "success" | "done" | "ok" | "spawned" | "started" | "completed" => {
                SubagentItemStatus::Success
            }
            "failed" | "error" => SubagentItemStatus::Failed(action.to_string()),
            _ => SubagentItemStatus::Running,
        };

        for entry in self.entries.iter_mut().rev() {
            if let TimelineEntry::SubagentTree(ref mut block) = entry {
                if block.id == subagent_id {
                    for prev_item in block.items.iter_mut() {
                        if matches!(prev_item.status, SubagentItemStatus::Running) {
                            prev_item.status = SubagentItemStatus::Success;
                        }
                    }
                    block.items.push(SubagentTreeItem {
                        name: role.to_string(),
                        detail: action.to_string(),
                        status: item_status,
                    });
                    return;
                }
            }
        }

        self.entries
            .push(TimelineEntry::SubagentTree(SubagentTreeBlock {
                id: subagent_id.to_string(),
                role_name: role.to_string(),
                task_prompt: action.to_string(),
                items: vec![SubagentTreeItem {
                    name: role.to_string(),
                    detail: action.to_string(),
                    status: item_status,
                }],
                is_running: true,
                is_success: false,
                outcome: None,
                error_message: None,
                tokens_used: 0,
                duration_ms: None,
            }));
    }

    /// Ingests a SubagentCompleted event and finalizes the matching SubagentTree block.
    pub fn handle_subagent_completed(
        &mut self,
        subagent_id: &str,
        role: &str,
        success: bool,
        summary: &str,
    ) {
        let (pool_tokens, pool_duration_ms) =
            crate::agent::subagent::try_get_global_subagent_pool()
                .and_then(|pool| {
                    pool.snapshot_subagents()
                        .into_iter()
                        .find(|w| w.id == subagent_id)
                })
                .map(|w| {
                    let dur_ms = w
                        .finished_at_secs
                        .map(|fin| fin.saturating_sub(w.started_at_secs).saturating_mul(1000))
                        .or_else(|| Some(w.elapsed_secs().saturating_mul(1000)));
                    (w.tokens_used, dur_ms)
                })
                .unwrap_or((0, None));

        for entry in self.entries.iter_mut().rev() {
            if let TimelineEntry::SubagentTree(ref mut block) = entry {
                if block.id == subagent_id {
                    block.is_running = false;
                    block.is_success = success;
                    if pool_tokens > 0 {
                        block.tokens_used = pool_tokens;
                    }
                    if pool_duration_ms.is_some() {
                        block.duration_ms = pool_duration_ms;
                    }
                    for item in block.items.iter_mut() {
                        if matches!(item.status, SubagentItemStatus::Running) {
                            item.status = if success {
                                SubagentItemStatus::Success
                            } else {
                                SubagentItemStatus::Failed(summary.to_string())
                            };
                        }
                    }
                    if success {
                        block.outcome = Some(summary.to_string());
                    } else {
                        block.error_message = Some(summary.to_string());
                    }
                    return;
                }
            }
        }

        self.entries
            .push(TimelineEntry::SubagentTree(SubagentTreeBlock {
                id: subagent_id.to_string(),
                role_name: role.to_string(),
                task_prompt: summary.to_string(),
                items: vec![],
                is_running: false,
                is_success: success,
                outcome: if success {
                    Some(summary.to_string())
                } else {
                    None
                },
                error_message: if !success {
                    Some(summary.to_string())
                } else {
                    None
                },
                tokens_used: pool_tokens,
                duration_ms: pool_duration_ms,
            }));
    }

    /// Finalizes any dangling open timeline entries (`Thought`, `ToolStart`, or completed `SubagentTree`s)
    /// when a turn ends or interrupts so loading spinners never remain stuck in the TUI.
    pub fn finalize_open_entries_on_turn_end(&mut self) {
        self.finalize_pending_thoughts(None);

        let snapshots = crate::agent::subagent::try_get_global_subagent_pool()
            .map(|pool| pool.snapshot_subagents())
            .unwrap_or_default();

        for entry in self.entries.iter_mut() {
            match entry {
                TimelineEntry::ToolStart {
                    name,
                    command_or_path,
                } => {
                    *entry = TimelineEntry::ToolFinished {
                        name: name.clone(),
                        command_or_path: command_or_path.clone(),
                        output: String::new(),
                        success: true,
                        duration_ms: None,
                    };
                }
                TimelineEntry::SubagentTree(ref mut block) if block.is_running => {
                    if let Some(w) = snapshots.iter().find(|w| w.id == block.id) {
                        if !w.state.is_active() {
                            let is_success =
                                matches!(w.state, crate::agent::subagent::SubagentState::Completed);
                            block.is_running = false;
                            block.is_success = is_success;
                            if w.tokens_used > 0 {
                                block.tokens_used = w.tokens_used;
                            }
                            if block.duration_ms.is_none() {
                                block.duration_ms = Some(w.elapsed_secs().saturating_mul(1000));
                            }
                            for item in block.items.iter_mut() {
                                if matches!(item.status, SubagentItemStatus::Running) {
                                    item.status = if is_success {
                                        SubagentItemStatus::Success
                                    } else {
                                        SubagentItemStatus::Failed(
                                            w.final_summary.clone().unwrap_or_default(),
                                        )
                                    };
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Gets the most recent assistant response text for copying
    pub fn get_last_assistant_response(&self) -> Option<String> {
        for entry in self.entries.iter().rev() {
            if let TimelineEntry::AssistantMarkdown(ref text) = entry {
                return Some(text.clone());
            }
        }
        None
    }

    /// Gets the entire conversation transcript as markdown
    pub fn get_all_transcript_text(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            match entry {
                TimelineEntry::UserPrompt(prompt) => {
                    out.push_str(&format!("## User\n{}\n\n", prompt));
                }
                TimelineEntry::AssistantMarkdown(text) => {
                    out.push_str(&format!("## Assistant\n{}\n\n", text));
                }
                TimelineEntry::ToolFinished {
                    name,
                    command_or_path,
                    output,
                    success,
                    ..
                } => {
                    let status = if *success { "success" } else { "failed" };
                    out.push_str(&format!(
                        "### Tool: {} ({}) [{}]\n```\n{}\n```\n\n",
                        name,
                        command_or_path,
                        status,
                        output.trim()
                    ));
                }
                TimelineEntry::SubagentTree(block) => {
                    let status = if block.is_success {
                        "Success"
                    } else if block.is_running {
                        "Running"
                    } else {
                        "Failed"
                    };
                    out.push_str(&format!(
                        "### Subagent: {} ({}) [{}]\nPrompt: {}\n",
                        block.id,
                        block.role_name,
                        status,
                        block.task_prompt.trim()
                    ));
                    for it in &block.items {
                        out.push_str(&format!("- {} {}\n", it.name, it.detail));
                    }
                    if let Some(ref outcome) = block.outcome {
                        out.push_str(&format!("Outcome: {}\n", outcome));
                    }
                    if let Some(ref err) = block.error_message {
                        out.push_str(&format!("Error: {}\n", err));
                    }
                    out.push('\n');
                }
                TimelineEntry::SubagentSwarm(swarm) => {
                    out.push_str(&format!(
                        "### Subagent Swarm: {} ({} workers)\n",
                        swarm.title,
                        swarm.workers.len()
                    ));
                    for w in &swarm.workers {
                        out.push_str(&format!(
                            "- [{}] {}: {} tokens\n",
                            w.role_name, w.id, w.tokens_used
                        ));
                    }
                    out.push('\n');
                }
                TimelineEntry::LivePlan(plan) => {
                    out.push_str(&format!(
                        "### Plan Progress: {}/{} Tasks Completed\n",
                        plan.completed_tasks, plan.total_tasks
                    ));
                    for t in &plan.tasks {
                        let mark = match t.status {
                            LivePlanTaskStatus::Completed => "[x]",
                            LivePlanTaskStatus::InProgress => "[/]",
                            LivePlanTaskStatus::Pending => "[ ]",
                        };
                        out.push_str(&format!("- {} {}\n", mark, t.title));
                    }
                    out.push('\n');
                }
                _ => {}
            }
        }
        out
    }

    /// Handles mouse button press to begin text selection (or click the floating `Back to bottom` pill)
    pub fn handle_mouse_down(&self, col: u16, row: u16) {
        let area = self.selection.timeline_area.get();
        if col >= area.x && col < area.right() && row >= area.y && row < area.bottom() {
            let scroll = self.scroll_offset.get();
            let max_scroll = self.max_scroll.get();

            // Check if the user clicked the floating `↓ Back to bottom · End` pill on the last row when scrolled up
            if !self.auto_scroll.get()
                && scroll < max_scroll
                && max_scroll > 0
                && area.height >= 14
                && area.width >= 32
                && row == area.bottom().saturating_sub(1)
            {
                let pill_text = " ↓ Back to bottom · End ";
                let pill_w = unicode_width::UnicodeWidthStr::width(pill_text) as u16;
                let left_pad = area.width.saturating_sub(pill_w) / 2;
                let pill_start_x = area.x + left_pad;
                let pill_end_x = pill_start_x + pill_w;
                if col >= pill_start_x && col < pill_end_x {
                    self.clear_selection();
                    self.scroll_to_bottom();
                    return;
                }
            }

            // Only disable auto_scroll if the user is actually scrolled up above the bottom;
            // clicking or selecting text at the bottom must never trigger the "Back to bottom" overlay.
            if scroll < max_scroll {
                self.auto_scroll.set(false);
            }
            self.selection.handle_mouse_down(col, row, scroll);
        } else {
            self.clear_selection();
        }
    }

    /// Handles mouse drag to expand text selection range
    pub fn handle_mouse_drag(&self, col: u16, row: u16) {
        self.selection
            .handle_mouse_drag(col, row, self.scroll_offset.get());
    }

    /// Handles mouse button release: completes selection and auto-copies to system clipboard
    pub fn handle_mouse_up(&self, col: u16, row: u16) -> Option<String> {
        self.selection
            .handle_mouse_up(col, row, self.scroll_offset.get())
    }

    /// Returns whether there is an active visual text selection
    pub fn has_selection(&self) -> bool {
        self.selection.has_selection()
    }

    /// Clears any active visual text selection
    pub fn clear_selection(&self) {
        self.selection.clear();
    }

    /// Extracts the plain string contents of the selected text region
    #[allow(dead_code)]
    pub fn extract_selected_text(&self) -> Option<String> {
        self.selection.extract_selected_text()
    }
}
