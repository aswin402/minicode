use crate::constants::SPINNER_FRAMES;
use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Execution status of a subagent tool action item
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SubagentItemStatus {
    Running,
    Success,
    Failed(String),
}

/// An individual tool execution branch in a subagent tree
#[derive(Debug, Clone)]
pub struct SubagentTreeItem {
    pub name: String,
    pub detail: String,
    pub status: SubagentItemStatus,
}

/// An inline Crush/OpenCode style subagent tree block
#[derive(Debug, Clone)]
pub struct SubagentTreeBlock {
    pub id: String,
    pub role_name: String,
    pub task_prompt: String,
    pub items: Vec<SubagentTreeItem>,
    pub is_running: bool,
    pub is_success: bool,
    pub outcome: Option<String>,
    pub error_message: Option<String>,
    pub tokens_used: usize,
    pub duration_ms: Option<u64>,
}

/// An adaptive matrix card for large multi-agent swarms (4+ parallel workers)
#[derive(Debug, Clone)]
pub struct SwarmMatrixBlock {
    pub title: String,
    pub workers: Vec<SubagentTreeBlock>,
    pub is_expanded: bool,
    pub total_tokens: usize,
    pub is_running: bool,
    pub duration_ms: Option<u64>,
}

/// Execution status of a discrete task in a live plan block
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivePlanTaskStatus {
    Completed,
    InProgress,
    Pending,
}

/// A parsed task item in a visual Live Plan Card
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePlanTaskItem {
    pub title: String,
    pub status: LivePlanTaskStatus,
    pub sub_tasks: Vec<LivePlanTaskItem>,
}

/// An inline Oh-My-Pi / Pi style Live Plan & Execution Progress card
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePlanBlock {
    pub title: String,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub active_task: Option<String>,
    pub tasks: Vec<LivePlanTaskItem>,
}

#[derive(Debug, Clone)]
pub enum TimelineEntry {
    UserPrompt(String),
    ThoughtBlock {
        text: String,
        duration_secs: Option<f64>,
    },
    AssistantMarkdown(String),
    ToolStart {
        name: String,
        command_or_path: String,
    },
    #[allow(dead_code)]
    ToolApproved {
        name: String,
        command_or_path: String,
    },
    ToolFinished {
        #[allow(dead_code)]
        name: String,
        command_or_path: String,
        success: bool,
        output: String,
        #[allow(dead_code)]
        duration_ms: Option<u64>,
    },
    SubagentTree(SubagentTreeBlock),
    SubagentSwarm(SwarmMatrixBlock),
    #[allow(dead_code)]
    LivePlan(LivePlanBlock),
    ContextCompaction {
        tier: usize,
        turns_summarized: usize,
        tokens_before: usize,
        tokens_after: usize,
        savings_percent: usize,
    },
    SystemStatus(String),
    TurnSeparator,
}

pub struct TimelineView {
    pub entries: Vec<TimelineEntry>,
    pub scroll_offset: std::cell::Cell<u16>,
    pub auto_scroll: std::cell::Cell<bool>,
    pub max_scroll: std::cell::Cell<u16>,
    pub selection: crate::ui::selection::TimelineSelection,
    pub in_thought_mode: bool,
    pub thought_start: Option<std::time::Instant>,
    pub turn_start: Option<std::time::Instant>,
    pub thought_tag_buffer: String,
}

#[allow(dead_code)]
pub struct TimelineContext<'a> {
    pub theme: &'a Theme,
    pub is_working: bool,
    pub working_millis: u64,
    pub current_activity: Option<&'a crate::ui::animation::AgentActivity>,
    pub spinner_style: crate::ui::animation::SpinnerStyle,
    pub workspace: &'a std::path::Path,
    pub provider: &'a str,
    pub model: &'a str,
}

impl Default for TimelineView {
    fn default() -> Self {
        Self::new()
    }
}

impl TimelineView {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            scroll_offset: std::cell::Cell::new(0),
            auto_scroll: std::cell::Cell::new(true),
            max_scroll: std::cell::Cell::new(0),
            selection: crate::ui::selection::TimelineSelection::new(),
            in_thought_mode: false,
            thought_start: None,
            turn_start: None,
            thought_tag_buffer: String::new(),
        }
    }

    /// Last rendered timeline viewport height (0 before first render).
    pub fn timeline_viewport_height(&self) -> u16 {
        self.selection.timeline_area.get().height
    }

    /// Scrolls timeline up by a number of lines
    pub fn scroll_up(&self, lines: u16) {
        if self.auto_scroll.get() {
            self.scroll_offset.set(self.max_scroll.get());
            self.auto_scroll.set(false);
        }
        let current = self.scroll_offset.get();
        self.scroll_offset.set(current.saturating_sub(lines));
    }

    /// Scrolls timeline down by a number of lines
    pub fn scroll_down(&self, lines: u16) {
        if self.auto_scroll.get() {
            return;
        }
        let current = self.scroll_offset.get();
        let next = current.saturating_add(lines);
        let max = self.max_scroll.get();
        if next >= max {
            self.scroll_offset.set(max);
            self.auto_scroll.set(true);
        } else {
            self.scroll_offset.set(next);
        }
    }

    #[inline]
    fn page_scroll_step(viewport_height: u16) -> u16 {
        if viewport_height > 2 {
            viewport_height.saturating_sub(2)
        } else {
            5
        }
    }

    /// Scrolls timeline up by a page
    pub fn scroll_page_up(&self, viewport_height: u16) {
        self.scroll_up(Self::page_scroll_step(viewport_height));
    }

    /// Scrolls timeline down by a page
    pub fn scroll_page_down(&self, viewport_height: u16) {
        self.scroll_down(Self::page_scroll_step(viewport_height));
    }

    /// Jumps straight to the top of conversation history
    pub fn scroll_to_top(&self) {
        self.auto_scroll.set(false);
        self.scroll_offset.set(0);
    }

    /// Jumps straight to the bottom and resumes auto-scroll
    pub fn scroll_to_bottom(&self) {
        self.auto_scroll.set(true);
        self.scroll_offset.set(self.max_scroll.get());
    }

    /// Returns true if there are no messages or entries in the timeline.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add_user_message(&mut self, prompt: String) {
        if !self.entries.is_empty() {
            self.entries.push(TimelineEntry::TurnSeparator);
        }
        self.entries.push(TimelineEntry::UserPrompt(prompt));
        self.turn_start = Some(std::time::Instant::now());
        self.thought_start = None;
        self.in_thought_mode = false;
        self.thought_tag_buffer.clear();
        self.auto_scroll.set(true);
    }

    pub fn append_thought_delta(&mut self, delta: &str) {
        let cleaned = Self::sanitize_thought_text(delta);
        if cleaned.is_empty() {
            return;
        }
        if let Some(TimelineEntry::ThoughtBlock { text, .. }) = self.entries.last_mut() {
            text.push_str(&cleaned);
        } else {
            self.thought_start = Some(std::time::Instant::now());
            self.entries.push(TimelineEntry::ThoughtBlock {
                text: cleaned,
                duration_secs: None,
            });
        }
        if self.thought_start.is_none() {
            self.thought_start = Some(std::time::Instant::now());
        }
    }

    #[allow(dead_code)]
    pub fn add_thought_block(&mut self, text: String, duration_secs: Option<f64>) {
        let cleaned = Self::sanitize_thought_text(&text);
        if !cleaned.is_empty() {
            self.entries.push(TimelineEntry::ThoughtBlock {
                text: cleaned,
                duration_secs,
            });
        }
    }

    pub fn finalize_pending_thoughts(&mut self, _elapsed_secs: Option<f64>) {
        if !self.thought_tag_buffer.is_empty() {
            let pending = std::mem::take(&mut self.thought_tag_buffer);
            if self.in_thought_mode {
                self.append_thought_delta(&pending);
            } else {
                self.append_assistant_text(&pending);
            }
        }
        if let Some(TimelineEntry::ThoughtBlock {
            duration_secs,
            text,
        }) = self
            .entries
            .iter_mut()
            .rev()
            .find(|e| matches!(e, TimelineEntry::ThoughtBlock { .. }))
        {
            *text = Self::sanitize_thought_text(text);
            if duration_secs.is_none() || duration_secs.unwrap_or(0.0) < 0.1 {
                *duration_secs = self.thought_start.take().map(|s| s.elapsed().as_secs_f64());
            }
        }
        self.thought_start = None;
        self.in_thought_mode = false;
    }

    pub const OPEN_THOUGHT_TAGS: &'static [&'static str] = &[
        "<think>",
        "<thought>",
        "<thinking>",
        "<reasoning>",
        "<antThinking>",
        "<Think>",
        "<Thought>",
        "<Thinking>",
        "<Reasoning>",
        "<THINK>",
    ];

    pub const CLOSE_THOUGHT_TAGS: &'static [&'static str] = &[
        "</think>",
        "</thought>",
        "</thinking>",
        "</reasoning>",
        "</antThinking>",
        "</Think>",
        "</Thought>",
        "</Thinking>",
        "</Reasoning>",
        "</THINK>",
    ];

    pub const ALL_THOUGHT_TAGS: &'static [&'static str] = &[
        "<think>",
        "</think>",
        "<thought>",
        "</thought>",
        "<thinking>",
        "</thinking>",
        "<reasoning>",
        "</reasoning>",
        "<antThinking>",
        "</antThinking>",
        "<Think>",
        "</Think>",
        "<Thought>",
        "</Thought>",
        "<Thinking>",
        "</Thinking>",
        "<Reasoning>",
        "</Reasoning>",
        "<THINK>",
        "</THINK>",
    ];

    /// Strips any raw thinking XML tags (<think>, </think>, <thought>, </thought>, etc.)
    pub fn sanitize_thought_text(raw: &str) -> String {
        if !raw.contains('<') {
            return raw.to_string();
        }
        let mut s = raw.to_string();
        for tag in Self::ALL_THOUGHT_TAGS {
            if s.contains(tag) {
                s = s.replace(tag, "");
            }
        }
        s
    }

    /// Strips any stray thinking XML tags that might leak into assistant markdown
    #[inline]
    pub fn sanitize_assistant_text(raw: &str) -> String {
        Self::sanitize_thought_text(raw)
    }

    pub fn append_assistant_delta(&mut self, delta: &str) {
        const OPEN_TAGS: &[&str] = TimelineView::OPEN_THOUGHT_TAGS;
        const CLOSE_TAGS: &[&str] = TimelineView::CLOSE_THOUGHT_TAGS;

        let mut working_text = if self.thought_tag_buffer.is_empty() {
            delta.to_string()
        } else {
            let mut s = std::mem::take(&mut self.thought_tag_buffer);
            s.push_str(delta);
            s
        };

        while !working_text.is_empty() {
            if self.in_thought_mode {
                let earliest_close = CLOSE_TAGS
                    .iter()
                    .filter_map(|&tag| working_text.find(tag).map(|idx| (idx, tag.len())))
                    .min_by_key(|&(idx, _)| idx);

                if let Some((end_idx, tag_len)) = earliest_close {
                    let thought_chunk = &working_text[..end_idx];
                    if !thought_chunk.is_empty() {
                        self.append_thought_delta(thought_chunk);
                    }
                    self.in_thought_mode = false;
                    let dur = self.thought_start.take().map(|s| s.elapsed().as_secs_f64());
                    if let Some(TimelineEntry::ThoughtBlock { duration_secs, .. }) =
                        self.entries.last_mut()
                    {
                        if duration_secs.is_none() || duration_secs.unwrap_or(0.0) < 0.1 {
                            *duration_secs = dur;
                        }
                    }
                    let mut next_start = end_idx + tag_len;
                    if working_text[next_start..].starts_with("\r\n") {
                        next_start += 2;
                    } else if working_text[next_start..].starts_with('\n') {
                        next_start += 1;
                    }
                    working_text = working_text[next_start..].to_string();
                } else {
                    if let Some(pos) = working_text.rfind('<') {
                        let tail = &working_text[pos..];
                        if CLOSE_TAGS.iter().any(|tag| tag.starts_with(tail)) {
                            let thought_chunk = &working_text[..pos];
                            if !thought_chunk.is_empty() {
                                self.append_thought_delta(thought_chunk);
                            }
                            self.thought_tag_buffer = tail.to_string();
                            break;
                        }
                    }
                    self.append_thought_delta(&working_text);
                    break;
                }
            } else {
                let earliest_open = OPEN_TAGS
                    .iter()
                    .filter_map(|&tag| working_text.find(tag).map(|idx| (idx, tag.len())))
                    .min_by_key(|&(idx, _)| idx);

                if let Some((start_idx, tag_len)) = earliest_open {
                    let prefix = &working_text[..start_idx];
                    if !prefix.is_empty() {
                        self.append_assistant_text(prefix);
                    }
                    self.in_thought_mode = true;
                    self.thought_start = Some(std::time::Instant::now());
                    let mut next_start = start_idx + tag_len;
                    if working_text[next_start..].starts_with("\r\n") {
                        next_start += 2;
                    } else if working_text[next_start..].starts_with('\n') {
                        next_start += 1;
                    }
                    working_text = working_text[next_start..].to_string();
                } else {
                    if let Some(pos) = working_text.rfind('<') {
                        let tail = &working_text[pos..];
                        if OPEN_TAGS.iter().any(|tag| tag.starts_with(tail)) {
                            let prefix = &working_text[..pos];
                            if !prefix.is_empty() {
                                self.append_assistant_text(prefix);
                            }
                            self.thought_tag_buffer = tail.to_string();
                            break;
                        }
                    }
                    self.append_assistant_text(&working_text);
                    break;
                }
            }
        }
    }

    fn append_assistant_text(&mut self, text: &str) {
        let cleaned = Self::sanitize_assistant_text(text);
        if cleaned.is_empty() {
            return;
        }
        if let Some(TimelineEntry::AssistantMarkdown(ref mut existing)) = self.entries.last_mut() {
            existing.push_str(&cleaned);
        } else {
            self.entries.push(TimelineEntry::AssistantMarkdown(cleaned));
        }
    }

    pub fn add_tool_call(&mut self, name: String, args: String) {
        let display_cmd = Self::extract_cmd_display(&name, &args);
        if let Some(TimelineEntry::ToolStart {
            name: n,
            command_or_path: c,
        }) = self.entries.last()
        {
            if n == &name && c == &display_cmd {
                return;
            }
        }
        self.entries.push(TimelineEntry::ToolStart {
            name,
            command_or_path: display_cmd,
        });
    }

    pub fn finish_tool_call(
        &mut self,
        name: &str,
        success: bool,
        output: String,
        duration_ms: u64,
    ) {
        // Find corresponding tool start from the back to update in-place
        let mut found_idx = None;
        let mut display_cmd = String::new();
        for (idx, entry) in self.entries.iter().enumerate().rev() {
            if let TimelineEntry::ToolStart {
                command_or_path,
                name: n,
            } = entry
            {
                if n == name {
                    display_cmd = command_or_path.clone();
                    found_idx = Some(idx);
                    break;
                }
            }
        }

        let finished = TimelineEntry::ToolFinished {
            name: name.to_string(),
            command_or_path: display_cmd,
            success,
            output,
            duration_ms: Some(duration_ms),
        };

        if let Some(idx) = found_idx {
            self.entries[idx] = finished;
        } else {
            self.entries.push(finished);
        }
    }

    pub fn add_status(&mut self, status: String) {
        self.entries.push(TimelineEntry::SystemStatus(status));
    }

    /// Adds or updates in-place the Live Plan & Execution Progress card in the timeline
    #[allow(dead_code)]
    pub fn update_live_plan(&mut self, plan: LivePlanBlock) {
        if let Some(TimelineEntry::LivePlan(ref mut existing)) = self.entries.last_mut() {
            *existing = plan;
        } else {
            self.entries.push(TimelineEntry::LivePlan(plan));
        }
        self.auto_scroll.set(true);
    }

    pub fn add_context_compaction(
        &mut self,
        tier: usize,
        turns_summarized: usize,
        tokens_before: usize,
        tokens_after: usize,
        savings_percent: usize,
    ) {
        self.entries.push(TimelineEntry::ContextCompaction {
            tier,
            turns_summarized,
            tokens_before,
            tokens_after,
            savings_percent,
        });
    }

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
                    block.error_message = error_message;
                    block.tokens_used = tokens_used;
                    block.duration_ms = duration_ms;
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
            "success" | "done" | "ok" => SubagentItemStatus::Success,
            "failed" | "error" => SubagentItemStatus::Failed(action.to_string()),
            _ => SubagentItemStatus::Running,
        };

        for entry in self.entries.iter_mut().rev() {
            if let TimelineEntry::SubagentTree(ref mut block) = entry {
                if block.id == subagent_id {
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
        for entry in self.entries.iter_mut().rev() {
            if let TimelineEntry::SubagentTree(ref mut block) = entry {
                if block.id == subagent_id {
                    block.is_running = false;
                    block.is_success = success;
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
                tokens_used: 0,
                duration_ms: None,
            }));
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

    /// Formats an internal tool identifier into a human-readable badge title
    pub fn format_tool_title(raw_name: &str) -> String {
        match raw_name {
            "exec_cmd" | "bash" | "run_command" | "shell" | "cmd" => "Bash".to_string(),
            "read_file" | "cat" | "view_file" => "Read File".to_string(),
            "write_file" | "create_file" => "Write File".to_string(),
            "patch_file" | "edit_file" | "replace_file_content" => "Edit File".to_string(),
            "list_directory" | "list_dir" | "ls" => "List Dir".to_string(),
            "grep_search" | "search_code" | "grep" => "Grep Search".to_string(),
            "find_files" | "find_by_name" | "locate_files" => "Find Files".to_string(),
            "file_info" | "stat_file" => "File Info".to_string(),
            "make_directory" | "mkdir" => "Make Dir".to_string(),
            "remove_file" | "rm" => "Remove File".to_string(),
            "web_fetch" | "fetch_web_page" | "curl" => "Web Fetch".to_string(),
            "web_search" => "Web Search".to_string(),
            "git_status" => "Git Status".to_string(),
            "git_diff" => "Git Diff".to_string(),
            "git_commit" => "Git Commit".to_string(),
            "git_log" => "Git Log".to_string(),
            "git_branch" => "Git Branch".to_string(),
            "code_graph" | "symbol_graph" => "Code Graph".to_string(),
            "semantic_search" => "Semantic Search".to_string(),
            "explore_symbol" | "symbol_definition" => "Explore Symbol".to_string(),
            "blast_radius" => "Blast Radius".to_string(),
            "kit_doctor" | "onpkg_doctor" => "MiniKit Doctor".to_string(),
            "kit_stack_list" | "onpkg_stack_list" => "MiniKit Stack List".to_string(),
            "kit_stack_add" | "onpkg_stack_add" => "MiniKit Stack Add".to_string(),
            "kit_stack_new" | "onpkg_stack_new" => "MiniKit Stack New".to_string(),
            "kit_stack_remove" | "onpkg_stack_remove" => "MiniKit Stack Remove".to_string(),
            "kit_stack_diff" | "onpkg_stack_diff" => "MiniKit Stack Diff".to_string(),
            "kit_skill_remove" | "onpkg_skill_remove" => "MiniKit Skill Remove".to_string(),
            "kit_info" | "kit_pkg_info" | "onpkg_pkg_info" => "MiniKit Package Info".to_string(),
            "kit_add" | "kit_pkg_add" | "onpkg_pkg_add" => "MiniKit Package Add".to_string(),
            "kit_remove" | "kit_pkg_remove" | "onpkg_pkg_remove" => {
                "MiniKit Package Remove".to_string()
            }
            "spawn_subagent" => "Spawn Subagent".to_string(),
            "send_subagent_message" => "Message Subagent".to_string(),
            "begin_transaction" => "Begin Transaction".to_string(),
            "commit_transaction" => "Commit Transaction".to_string(),
            "rollback_transaction" => "Rollback Transaction".to_string(),
            other => other
                .split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// Cleans and truncates shell commands for minimal single-line badge display.
    /// Strips multiline heredoc payloads (`<< ...`), normalizes whitespace, and truncates to 70 chars.
    #[inline]
    fn truncate_header_display(s: &str, max_chars: usize) -> String {
        if s.chars().count() > max_chars {
            let mut res: String = s.chars().take(max_chars.saturating_sub(3)).collect();
            res.push_str("...");
            res
        } else {
            s.to_string()
        }
    }

    pub fn clean_command_display(cmd: &str) -> String {
        let first_line = cmd.lines().next().unwrap_or("").trim();
        // If it's a heredoc like `cat > file << 'EOF'`, cut off at `<<`
        let stripped = if let Some(idx) = first_line.find("<<") {
            first_line[..idx].trim_end()
        } else {
            first_line
        };
        let parts: Vec<&str> = stripped.split_whitespace().collect();
        let compacted = parts.join(" ");
        Self::truncate_header_display(&compacted, 70)
    }

    /// Cleans and truncates file paths for compact header display.
    pub fn clean_path_display(path: &str) -> String {
        Self::truncate_header_display(path.trim(), 70)
    }

    /// Splits a long line of text into sub-chunks of max `max_width` visual cells.
    /// Normalizes emojis/tabs first and splits on whitespace boundaries when possible.
    pub fn split_line_into_chunks(text: &str, max_width: usize) -> Vec<String> {
        let sanitized = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(text);
        let text = sanitized.as_str();
        if max_width == 0 {
            return vec![text.to_string()];
        }
        let total_width = UnicodeWidthStr::width(text);
        if total_width <= max_width {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_width = 0;

        let push_word_fitting_or_char_split =
            |word: &str,
             word_width: usize,
             current: &mut String,
             current_width: &mut usize,
             chunks: &mut Vec<String>| {
                if word_width <= max_width {
                    current.push_str(word);
                    *current_width = word_width;
                } else {
                    for ch in word.chars() {
                        let ch_w = UnicodeWidthChar::width(ch).unwrap_or(1);
                        if *current_width + ch_w > max_width && !current.is_empty() {
                            chunks.push(std::mem::take(current));
                            *current_width = 0;
                        }
                        current.push(ch);
                        *current_width += ch_w;
                    }
                }
            };

        for word in text.split(' ') {
            let word_width = UnicodeWidthStr::width(word);
            if current.is_empty() {
                push_word_fitting_or_char_split(
                    word,
                    word_width,
                    &mut current,
                    &mut current_width,
                    &mut chunks,
                );
            } else if current_width + 1 + word_width <= max_width {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_width;
            } else {
                chunks.push(std::mem::take(&mut current));
                current_width = 0;
                push_word_fitting_or_char_split(
                    word,
                    word_width,
                    &mut current,
                    &mut current_width,
                    &mut chunks,
                );
            }
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        if chunks.is_empty() {
            vec![text.to_string()]
        } else {
            chunks
        }
    }

    pub fn extract_cmd_display(name: &str, args_json: &str) -> String {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(args_json) {
            if let Some(cmd) = val
                .get("command")
                .or_else(|| val.get("cmd"))
                .and_then(|c| c.as_str())
            {
                return Self::clean_command_display(cmd);
            }
            if let Some(path) = val
                .get("path")
                .or_else(|| val.get("file_path"))
                .or_else(|| val.get("target_file"))
                .and_then(|p| p.as_str())
            {
                return Self::clean_path_display(path);
            }
            if let Some(query) = val
                .get("query")
                .or_else(|| val.get("pattern"))
                .and_then(|q| q.as_str())
            {
                return format!("\"{}\"", query);
            }
            if let Some(url) = val.get("url").and_then(|u| u.as_str()) {
                return url.to_string();
            }
            if let Some(symbol) = val
                .get("symbol")
                .or_else(|| val.get("symbol_name"))
                .and_then(|s| s.as_str())
            {
                return symbol.to_string();
            }
            if let Some(message) = val.get("message").and_then(|m| m.as_str()) {
                let first_line = message.lines().next().unwrap_or("").trim();
                return format!("\"{}\"", first_line);
            }
        }
        let trimmed = args_json.trim();
        if trimmed == "{}" || trimmed.is_empty() {
            String::new()
        } else {
            Self::clean_command_display(&format!("{}({})", name, trimmed))
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, ctx: &TimelineContext) {
        let theme = ctx.theme;
        let mut lines: Vec<Line> = Vec::new();
        let usable_width = (area.width.saturating_sub(1) as usize).max(20);

        if self.entries.is_empty() {
            return;
        }

        // 1. Top Session Header Banner (MiniCode block logo + version + model + workspace + divider line)
        // Rendered at the start of the conversation timeline before the user's first prompt.
        let has_user_prompt = self
            .entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::UserPrompt(_)));
        if has_user_prompt {
            static HOME_DIR: std::sync::OnceLock<Option<std::path::PathBuf>> =
                std::sync::OnceLock::new();
            let cached_home = HOME_DIR.get_or_init(dirs::home_dir);
            let display_path = if let Some(ref home) = cached_home {
                if let Ok(rel) = ctx.workspace.strip_prefix(home) {
                    format!("~/{}", rel.display())
                } else {
                    ctx.workspace.display().to_string()
                }
            } else {
                ctx.workspace.display().to_string()
            };

            let prov_display = if ctx.provider.is_empty() {
                String::new()
            } else {
                let mut chars = ctx.provider.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            };
            let model_summary = if prov_display.is_empty() {
                ctx.model.to_string()
            } else {
                format!("{} · {}", prov_display, ctx.model)
            };

            // Top margin so the logo block sits slightly down from the top terminal edge
            lines.push(Line::from(String::new()));
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled("████", Style::default().fg(theme.brand_accent)),
                Span::raw("    "),
                Span::styled("████", Style::default().fg(theme.info)),
                Span::raw("   "),
                Span::styled(
                    "MiniCode",
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" v{}", env!("CARGO_PKG_VERSION")),
                    Style::default().fg(theme.muted),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled("████", Style::default().fg(theme.brand_accent)),
                Span::raw("    "),
                Span::styled("████", Style::default().fg(theme.info)),
                Span::raw("   "),
                Span::styled("Build better, with AI", Style::default().fg(theme.muted)),
            ]));
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled("██  ", Style::default().fg(theme.brand_accent)),
                Span::raw("    "),
                Span::styled("  ██", Style::default().fg(theme.info)),
                Span::raw("   "),
                Span::styled(model_summary, Style::default().fg(theme.brand_accent)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("                "),
                Span::styled(display_path, Style::default().fg(theme.muted)),
            ]));
            lines.push(Line::from(String::new()));
            let divider_w = 60usize.min(area.width.saturating_sub(2) as usize);
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled("─".repeat(divider_w), Style::default().fg(theme.border)),
            ]));
            lines.push(Line::from(String::new()));
        }

        let mut last_user_prompt: Option<&str> = None;

        for entry in &self.entries {
            match entry {
                TimelineEntry::TurnSeparator => {
                    let divider_w = 60usize.min(area.width.saturating_sub(2) as usize);
                    lines.push(Line::from(String::new()));
                    lines.push(Line::from(vec![
                        Span::raw(" "),
                        Span::styled("─".repeat(divider_w), Style::default().fg(theme.border)),
                    ]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::UserPrompt(prompt) => {
                    last_user_prompt = Some(prompt.as_str());
                    let prompt_lines: Vec<&str> = prompt.lines().collect();

                    if prompt_lines.is_empty() {
                        lines.push(Line::from(vec![Span::styled(
                            "› ",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        )]));
                    } else {
                        let max_inline_lines = 4usize;
                        let total_p_lines = prompt_lines.len();
                        let shown_count = total_p_lines.min(max_inline_lines);
                        let extra_lines = total_p_lines.saturating_sub(shown_count);

                        for (idx, pline) in prompt_lines.iter().take(shown_count).enumerate() {
                            let prefix = if idx == 0 { "› " } else { "  " };
                            let prefix_style = if idx == 0 {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.muted)
                            };
                            let clean_pline =
                                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                    pline,
                                );
                            let mut row_spans = vec![
                                Span::styled(prefix, prefix_style),
                                Span::styled(
                                    clean_pline,
                                    Style::default().fg(theme.text_primary).add_modifier(
                                        if idx == 0 {
                                            Modifier::BOLD
                                        } else {
                                            Modifier::empty()
                                        },
                                    ),
                                ),
                            ];
                            if idx + 1 == shown_count && extra_lines > 0 {
                                row_spans.push(Span::styled(
                                    format!(" ... +{} lines", extra_lines),
                                    Style::default()
                                        .fg(theme.muted)
                                        .add_modifier(Modifier::ITALIC),
                                ));
                            }
                            lines.push(Line::from(row_spans));
                        }
                    }

                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::ThoughtBlock {
                    text,
                    duration_secs,
                } => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        let header = match duration_secs {
                            Some(secs) if *secs >= 0.1 => format!("• Thought for {:.1}s", secs),
                            _ => {
                                if let Some(st) = self.thought_start {
                                    format!(
                                        "• Thinking ({:.1}s)",
                                        st.elapsed().as_secs_f64().max(0.1)
                                    )
                                } else {
                                    "• Thought".to_string()
                                }
                            }
                        };

                        lines.push(Line::from(vec![Span::styled(
                            header,
                            Style::default()
                                .fg(theme.muted)
                                .add_modifier(Modifier::BOLD),
                        )]));
                        let max_t_width = (area.width.saturating_sub(4) as usize).max(20);
                        for t_line in trimmed.lines() {
                            let chunks = Self::split_line_into_chunks(t_line, max_t_width);
                            for chunk in chunks {
                                lines.push(Line::from(vec![
                                    Span::styled("  ", Style::default()),
                                    Span::styled(
                                        chunk,
                                        Style::default()
                                            .fg(theme.muted)
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]));
                            }
                        }
                        lines.push(Line::from(String::new()));
                    }
                }
                TimelineEntry::AssistantMarkdown(text) => {
                    let parsed_lines = Self::render_markdown_with_width(text, theme, usable_width);
                    lines.extend(parsed_lines);
                }
                TimelineEntry::ToolStart {
                    name,
                    command_or_path,
                } => {
                    let title = Self::format_tool_title(name);
                    let mut spans = vec![
                        Span::styled("• ", Style::default().fg(theme.brand_accent)),
                        Span::styled(
                            format!("{} ", title),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ];
                    if !command_or_path.is_empty() {
                        let clean_cmd =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                command_or_path,
                            );
                        spans.push(Span::styled(
                            clean_cmd,
                            Style::default().fg(theme.text_primary),
                        ));
                    }
                    lines.push(Line::from(spans));
                    lines.push(Line::from(vec![
                        Span::styled("  └ ", Style::default().fg(theme.muted)),
                        Span::styled(
                            "running...",
                            Style::default()
                                .fg(theme.muted)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::ToolApproved {
                    name,
                    command_or_path,
                    ..
                } => {
                    let title = Self::format_tool_title(name);
                    let clean_cmd = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                        command_or_path,
                    );
                    lines.push(Line::from(vec![
                        Span::styled(
                            "✓ ",
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("You approved ", Style::default().fg(theme.muted)),
                        Span::styled(
                            format!("{} ", title),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(clean_cmd, Style::default().fg(theme.text_primary)),
                        Span::styled(" this time", Style::default().fg(theme.muted)),
                    ]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::ToolFinished {
                    name,
                    command_or_path,
                    output,
                    success,
                    duration_ms,
                    ..
                } => {
                    let title = Self::format_tool_title(name);
                    let status_color = if *success {
                        theme.brand_accent
                    } else {
                        theme.destructive
                    };

                    let bullet = if *success { "• " } else { "✗ " };
                    let mut spans = vec![
                        Span::styled(
                            bullet,
                            Style::default()
                                .fg(status_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{} ", title),
                            Style::default()
                                .fg(status_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ];
                    if !command_or_path.is_empty() {
                        let clean_cmd =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                command_or_path,
                            );
                        spans.push(Span::styled(
                            clean_cmd,
                            Style::default().fg(theme.text_primary),
                        ));
                    }
                    lines.push(Line::from(spans));

                    // Check for inline diff block
                    let diff_marker = crate::tools::middleware::DIFF_MARKER;
                    let (diff_section, regular_output) =
                        if let Some(rest) = output.strip_prefix(diff_marker) {
                            // Split on first blank line separating diff from tool output
                            if let Some(split_pos) = rest.find("\n\n") {
                                (&rest[..split_pos + 1], rest[split_pos + 2..].trim())
                            } else {
                                (rest, "")
                            }
                        } else {
                            ("", output.trim())
                        };

                    // Render diff lines with +/- colouring
                    if !diff_section.is_empty() {
                        let diff_usable_width = area.width.saturating_sub(2) as usize;
                        let max_content_width = diff_usable_width.saturating_sub(6).max(20);
                        let mut diff_line_count = 0;
                        for diff_line in diff_section.lines() {
                            if diff_line.starts_with("---") || diff_line.starts_with("+++") {
                                let chunks =
                                    Self::split_line_into_chunks(diff_line, max_content_width);
                                for (c_idx, chunk) in chunks.into_iter().enumerate() {
                                    let pfx = if c_idx == 0 { "  │ " } else { "  │   " };
                                    lines.push(Line::from(vec![
                                        Span::styled(pfx, Style::default().fg(theme.muted)),
                                        Span::styled(
                                            chunk,
                                            Style::default()
                                                .fg(theme.muted)
                                                .add_modifier(Modifier::ITALIC),
                                        ),
                                    ]));
                                }
                            } else if let Some((marker, color, rest)) = diff_line
                                .strip_prefix("+ ")
                                .map(|r| ("+", theme.success, r))
                                .or_else(|| {
                                    diff_line
                                        .strip_prefix("- ")
                                        .map(|r| ("-", theme.destructive, r))
                                })
                            {
                                let chunks = Self::split_line_into_chunks(rest, max_content_width);
                                for (c_idx, chunk) in chunks.into_iter().enumerate() {
                                    if c_idx == 0 {
                                        lines.push(Line::from(vec![
                                            Span::styled("  │ ", Style::default().fg(theme.muted)),
                                            Span::styled(
                                                marker,
                                                Style::default()
                                                    .fg(color)
                                                    .add_modifier(Modifier::BOLD),
                                            ),
                                            Span::styled(
                                                format!(" {}", chunk),
                                                Style::default().fg(color),
                                            ),
                                        ]));
                                    } else {
                                        lines.push(Line::from(vec![
                                            Span::styled(
                                                "  │   ",
                                                Style::default().fg(theme.muted),
                                            ),
                                            Span::styled(chunk, Style::default().fg(color)),
                                        ]));
                                    }
                                }
                                diff_line_count += 1;
                            } else if let Some(rest) = diff_line.strip_prefix("  ") {
                                let chunks = Self::split_line_into_chunks(rest, max_content_width);
                                for chunk in chunks {
                                    lines.push(Line::from(vec![
                                        Span::styled("  │   ", Style::default().fg(theme.muted)),
                                        Span::styled(chunk, Style::default().fg(theme.muted)),
                                    ]));
                                }
                            }
                            if diff_line_count > crate::constants::UI_MAX_TOOL_OUTPUT_LINES {
                                let remaining = diff_section
                                    .lines()
                                    .filter(|l| l.starts_with("+ ") || l.starts_with("- "))
                                    .count()
                                    .saturating_sub(diff_line_count);
                                if remaining > 0 {
                                    lines.push(Line::from(vec![
                                        Span::styled("  │ ", Style::default().fg(theme.muted)),
                                        Span::styled(
                                            format!("... +{} diff lines (folded)", remaining),
                                            Style::default().fg(theme.border),
                                        ),
                                    ]));
                                }
                                break;
                            }
                        }
                    }

                    // Render regular tool output (summary after diff)
                    let trimmed = regular_output;
                    if !trimmed.is_empty() {
                        let out_usable_width = area.width.saturating_sub(2) as usize;
                        let max_content_width = out_usable_width.saturating_sub(6).max(20);
                        let max_lines = if *success {
                            crate::constants::UI_MAX_TOOL_OUTPUT_LINES
                        } else {
                            4
                        };
                        for raw_out_line in trimmed.lines().take(max_lines) {
                            let out_line = if !*success && raw_out_line.contains("Raw arguments") {
                                "[Arguments payload truncated: model exceeded single-call capacity]"
                            } else {
                                raw_out_line
                            };

                            let line_color = if !*success {
                                theme.destructive
                            } else if out_line.starts_with('+') {
                                theme.success
                            } else if out_line.starts_with('-') {
                                theme.destructive
                            } else if out_line.starts_with("##")
                                || out_line.starts_with("test result:")
                            {
                                theme.info
                            } else {
                                theme.muted
                            };

                            let border_color = if *success {
                                theme.muted
                            } else {
                                theme.destructive
                            };

                            let chunks = Self::split_line_into_chunks(out_line, max_content_width);
                            for (c_idx, chunk) in chunks.into_iter().enumerate() {
                                let pfx = if c_idx == 0 { "  │ " } else { "  │   " };
                                lines.push(Line::from(vec![
                                    Span::styled(pfx, Style::default().fg(border_color)),
                                    Span::styled(chunk, Style::default().fg(line_color)),
                                ]));
                            }
                        }
                        if trimmed.lines().count() > max_lines {
                            let remaining = trimmed.lines().count().saturating_sub(max_lines);
                            lines.push(Line::from(vec![
                                Span::styled(
                                    "  │ ",
                                    Style::default().fg(if *success {
                                        theme.muted
                                    } else {
                                        theme.destructive
                                    }),
                                ),
                                Span::styled(
                                    format!("... +{} lines (output folded)", remaining),
                                    Style::default().fg(theme.border),
                                ),
                            ]));
                        }
                    }

                    let dur_str = duration_ms
                        .filter(|d| *d > 0)
                        .map(|d| format!(" ({}ms)", d))
                        .unwrap_or_default();
                    let (status_glyph, glyph_color, status_text) = if *success {
                        ("✓", theme.success, "completed")
                    } else {
                        ("✗", theme.destructive, "failed")
                    };

                    let foot_border = if *success {
                        theme.muted
                    } else {
                        theme.destructive
                    };
                    lines.push(Line::from(vec![
                        Span::styled("  └ ", Style::default().fg(foot_border)),
                        Span::styled(
                            format!("{} ", status_glyph),
                            Style::default()
                                .fg(glyph_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{}{}", status_text, dur_str),
                            Style::default().fg(if *success {
                                theme.muted
                            } else {
                                theme.destructive
                            }),
                        ),
                    ]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::SubagentTree(block) => {
                    let role_color = theme.role_accent_color(&block.role_name);
                    let (status_bullet, status_color, header_suffix) = if block.is_running {
                        ("◉", role_color, "".to_string())
                    } else if block.is_success {
                        let dur = if let Some(ms) = block.duration_ms {
                            format!(
                                " ({:.1}s • {} tokens)",
                                (ms as f64) / 1000.0,
                                block.tokens_used
                            )
                        } else {
                            format!(" ({} tokens)", block.tokens_used)
                        };
                        ("✓", theme.success, dur)
                    } else {
                        let dur = if let Some(ms) = block.duration_ms {
                            format!(
                                " (Failed in {:.1}s • {} tokens)",
                                (ms as f64) / 1000.0,
                                block.tokens_used
                            )
                        } else {
                            format!(" (Failed • {} tokens)", block.tokens_used)
                        };
                        ("✗", theme.destructive, dur)
                    };

                    // Line 1: ◉ [Researcher: researcher-1]
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} ", status_bullet),
                            Style::default()
                                .fg(status_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("[{}: {}]", block.role_name, block.id),
                            Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(header_suffix, Style::default().fg(theme.muted)),
                    ]));

                    // Empty line
                    lines.push(Line::from(String::new()));

                    // Task Badge Line
                    let prompt_trimmed = block.task_prompt.trim();
                    let prompt_lines: Vec<&str> = prompt_trimmed.lines().collect();

                    if let Some(first_line) = prompt_lines.first() {
                        let clean_first =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                first_line,
                            );
                        lines.push(Line::from(vec![
                            Span::styled("   ", Style::default()),
                            Span::styled(
                                " Task ",
                                Style::default()
                                    .fg(theme.bg_primary)
                                    .bg(role_color)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" ", Style::default()),
                            Span::styled(clean_first, Style::default().fg(theme.text_primary)),
                        ]));

                        for rest_line in prompt_lines.iter().skip(1) {
                            let clean_rest =
                                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                    rest_line,
                                );
                            lines.push(Line::from(vec![
                                Span::styled("         ", Style::default()),
                                Span::styled(clean_rest, Style::default().fg(theme.text_primary)),
                            ]));
                        }
                    }

                    // Tree Branches
                    let item_count = block.items.len();
                    for (idx, item) in block.items.iter().enumerate() {
                        let is_last = idx == item_count - 1;
                        let branch = if is_last {
                            "  ╰─── "
                        } else {
                            "  ├─── "
                        };

                        let frame_idx = ((ctx.working_millis / crate::constants::SPINNER_FRAME_MS)
                            as usize)
                            % SPINNER_FRAMES.len();
                        let spinner = SPINNER_FRAMES[frame_idx];

                        let (item_bullet, item_color) = match &item.status {
                            SubagentItemStatus::Running => (spinner, theme.warning),
                            SubagentItemStatus::Success => ("✓", theme.success),
                            SubagentItemStatus::Failed(_) => ("✗", theme.destructive),
                        };

                        let clean_detail =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                &item.detail,
                            );
                        lines.push(Line::from(vec![
                            Span::styled(branch, Style::default().fg(theme.border)),
                            Span::styled(
                                format!("{} ", item_bullet),
                                Style::default().fg(item_color).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{} ", item.name),
                                Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(clean_detail, Style::default().fg(theme.muted)),
                        ]));
                    }

                    // Outcome / Error
                    if let Some(ref outcome) = block.outcome {
                        let clean_outcome =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                outcome.trim(),
                            );
                        lines.push(Line::from(String::new()));
                        lines.push(Line::from(vec![
                            Span::styled(
                                "  Outcome: ",
                                Style::default()
                                    .fg(theme.success)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(clean_outcome, Style::default().fg(theme.text_primary)),
                        ]));
                    } else if let Some(ref err) = block.error_message {
                        let clean_err =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                err.trim(),
                            );
                        lines.push(Line::from(String::new()));
                        lines.push(Line::from(vec![
                            Span::styled(
                                "  Error: ",
                                Style::default()
                                    .fg(theme.destructive)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(clean_err, Style::default().fg(theme.destructive)),
                        ]));
                    }

                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::SubagentSwarm(swarm) => {
                    let frame_idx = ((ctx.working_millis / crate::constants::SPINNER_FRAME_MS)
                        as usize)
                        % SPINNER_FRAMES.len();
                    let spinner = SPINNER_FRAMES[frame_idx];

                    let top_bullet = if swarm.is_running { spinner } else { "✓" };
                    let bullet_color = if swarm.is_running {
                        theme.warning
                    } else {
                        theme.success
                    };

                    let elapsed_str = if let Some(ms) = swarm.duration_ms {
                        format!(" • {:.1}s", (ms as f64) / 1000.0)
                    } else {
                        "".to_string()
                    };

                    let header_title = format!(
                        "┌─ {} {} ({} Workers • {} tokens{}) ──",
                        top_bullet,
                        swarm.title,
                        swarm.workers.len(),
                        swarm.total_tokens,
                        elapsed_str
                    );

                    lines.push(Line::from(vec![Span::styled(
                        header_title,
                        Style::default()
                            .fg(bullet_color)
                            .add_modifier(Modifier::BOLD),
                    )]));

                    if !swarm.is_expanded {
                        // Collapsed Matrix Summary
                        for worker in &swarm.workers {
                            let role_color = theme.role_accent_color(&worker.role_name);
                            let (w_bullet, w_color) = if worker.is_running {
                                ("◉", role_color)
                            } else if worker.is_success {
                                ("✓", theme.success)
                            } else {
                                ("✗", theme.destructive)
                            };

                            let last_action = worker
                                .items
                                .last()
                                .map(|it| {
                                    crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                        &format!("{} {}", it.name, it.detail),
                                    )
                                })
                                .unwrap_or_else(|| "Initializing...".to_string());

                            lines.push(Line::from(vec![
                                Span::styled("│ ", Style::default().fg(theme.border)),
                                Span::styled(
                                    format!("{} ", w_bullet),
                                    Style::default().fg(w_color).add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(
                                    format!("[{}] ", worker.role_name),
                                    Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(
                                    format!("{}: ", worker.id),
                                    Style::default().fg(theme.text_primary),
                                ),
                                Span::styled(
                                    format!("{} ", last_action),
                                    Style::default().fg(theme.muted),
                                ),
                                Span::styled(
                                    format!("({} tok)", worker.tokens_used),
                                    Style::default().fg(theme.border),
                                ),
                            ]));
                        }

                        lines.push(Line::from(vec![Span::styled(
                            "│ ",
                            Style::default().fg(theme.border),
                        )]));
                        lines.push(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(theme.border)),
                            Span::styled(
                                "[Space] Expand Full Tree  •  [k] Cancel Worker",
                                Style::default().fg(theme.muted),
                            ),
                        ]));
                    } else {
                        // Expanded Full Tree View inside Swarm Card
                        for worker in &swarm.workers {
                            let role_color = theme.role_accent_color(&worker.role_name);
                            let (w_bullet, w_color) = if worker.is_running {
                                ("◉", role_color)
                            } else if worker.is_success {
                                ("✓", theme.success)
                            } else {
                                ("✗", theme.destructive)
                            };

                            lines.push(Line::from(vec![
                                Span::styled("│ ", Style::default().fg(theme.border)),
                                Span::styled(
                                    format!("▾ {} ", w_bullet),
                                    Style::default().fg(w_color).add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(
                                    format!("[{}: {}]", worker.role_name, worker.id),
                                    Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                                ),
                            ]));

                            lines.push(Line::from(vec![
                                Span::styled("│ ", Style::default().fg(theme.border)),
                                Span::styled("   Task: ", Style::default().fg(role_color)),
                                Span::styled(
                                    crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                        worker.task_prompt.lines().next().unwrap_or(""),
                                    ),
                                    Style::default().fg(theme.text_primary),
                                ),
                            ]));

                            let item_count = worker.items.len();
                            for (idx, item) in worker.items.iter().enumerate() {
                                let is_last = idx == item_count - 1;
                                let branch = if is_last {
                                    "   ╰─── "
                                } else {
                                    "   ├─── "
                                };
                                let (i_bullet, i_color) = match &item.status {
                                    SubagentItemStatus::Running => (spinner, theme.warning),
                                    SubagentItemStatus::Success => ("✓", theme.success),
                                    SubagentItemStatus::Failed(_) => ("✗", theme.destructive),
                                };

                                lines.push(Line::from(vec![
                                    Span::styled("│ ", Style::default().fg(theme.border)),
                                    Span::styled(branch, Style::default().fg(theme.border)),
                                    Span::styled(
                                        format!("{} ", i_bullet),
                                        Style::default().fg(i_color),
                                    ),
                                    Span::styled(
                                        format!("{} ", item.name),
                                        Style::default()
                                            .fg(role_color)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&item.detail),
                                        Style::default().fg(theme.muted),
                                    ),
                                ]));
                            }

                            lines.push(Line::from(vec![Span::styled(
                                "│ ",
                                Style::default().fg(theme.border),
                            )]));
                        }

                        lines.push(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(theme.border)),
                            Span::styled(
                                "[Space] Collapse to Matrix  •  [k] Cancel Worker",
                                Style::default().fg(theme.muted),
                            ),
                        ]));
                    }

                    let bottom_border =
                        format!("└{}", "─".repeat((area.width as usize).saturating_sub(2)));
                    lines.push(Line::from(vec![Span::styled(
                        bottom_border,
                        Style::default().fg(theme.border),
                    )]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::ContextCompaction {
                    tier,
                    turns_summarized,
                    tokens_before,
                    tokens_after,
                    savings_percent,
                } => {
                    let tier_label = match tier {
                        1 => "Tier 1: Masking",
                        2 => "Tier 2: Turn Summary",
                        3 => "Tier 3: Memory Anchor",
                        _ => "Compacted",
                    };
                    let before_k = if *tokens_before >= 1000 {
                        format!("{:.1}k", *tokens_before as f64 / 1000.0)
                    } else {
                        tokens_before.to_string()
                    };
                    let after_k = if *tokens_after >= 1000 {
                        format!("{:.1}k", *tokens_after as f64 / 1000.0)
                    } else {
                        tokens_after.to_string()
                    };
                    lines.push(Line::from(vec![
                        Span::styled("• ", Style::default().fg(theme.brand_accent)),
                        Span::styled(
                            format!(
                                "Context Auto-Compacted [{}]: {} turns summarized, {} → {} tokens ({}% saved)",
                                tier_label, turns_summarized, before_k, after_k, savings_percent
                            ),
                            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::LivePlan(plan) => {
                    let header_text = format!(
                        "┌── Plan Progress: {}/{} Tasks Completed ──",
                        plan.completed_tasks, plan.total_tasks
                    );
                    let header_len = UnicodeWidthStr::width(header_text.as_str());
                    let right_padding = (area.width as usize)
                        .saturating_sub(header_len)
                        .saturating_sub(1);
                    let full_header = format!("{}{}", header_text, "─".repeat(right_padding));

                    lines.push(Line::from(vec![Span::styled(
                        full_header,
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )]));

                    for task in &plan.tasks {
                        let (bullet, bullet_color, text_color, modifier) = match task.status {
                            LivePlanTaskStatus::Completed => {
                                ("✓", theme.success, theme.text_primary, Modifier::empty())
                            }
                            LivePlanTaskStatus::InProgress => {
                                ("▸", theme.info, theme.brand_accent, Modifier::BOLD)
                            }
                            LivePlanTaskStatus::Pending => {
                                ("○", theme.muted, theme.muted, Modifier::empty())
                            }
                        };

                        let clean_title =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                &task.title,
                            );
                        lines.push(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(theme.border)),
                            Span::styled(
                                format!(" {} ", bullet),
                                Style::default()
                                    .fg(bullet_color)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                clean_title,
                                Style::default().fg(text_color).add_modifier(modifier),
                            ),
                        ]));
                    }

                    let bottom_border =
                        format!("└{}", "─".repeat((area.width as usize).saturating_sub(2)));
                    lines.push(Line::from(vec![Span::styled(
                        bottom_border,
                        Style::default().fg(theme.border),
                    )]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::SystemStatus(status) => {
                    for raw_line in status.lines() {
                        let normalized =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                raw_line,
                            );
                        let trimmed = normalized.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let prefixes = [
                            ("✓ ", theme.success),
                            ("✗ ", theme.destructive),
                            ("! ", theme.warning),
                            ("▲ ", theme.warning),
                            ("→ ", theme.brand_accent),
                            ("▸ ", theme.brand_accent),
                            ("• ", theme.brand_accent),
                            ("◆ ", theme.brand_accent),
                            ("● ", theme.brand_accent),
                        ];

                        let mut matched = false;
                        for (prefix, color) in prefixes {
                            if let Some(rest) = trimmed.strip_prefix(prefix) {
                                lines.push(Line::from(vec![
                                    Span::styled(
                                        prefix,
                                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        rest.to_string(),
                                        Style::default().fg(theme.text_primary),
                                    ),
                                ]));
                                matched = true;
                                break;
                            }
                        }

                        if !matched {
                            let lower = trimmed.to_lowercase();
                            if lower.starts_with("error") || lower.contains("error:") {
                                lines.push(Line::from(vec![
                                    Span::styled(
                                        "✗ ",
                                        Style::default()
                                            .fg(theme.destructive)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        trimmed.to_string(),
                                        Style::default().fg(theme.text_primary),
                                    ),
                                ]));
                            } else if lower.contains("rate limit") || lower.contains("retrying") {
                                lines.push(Line::from(vec![
                                    Span::styled(
                                        "! ",
                                        Style::default()
                                            .fg(theme.warning)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        trimmed.to_string(),
                                        Style::default().fg(theme.text_primary),
                                    ),
                                ]));
                            } else {
                                lines.push(Line::from(vec![
                                    Span::styled("• ", Style::default().fg(theme.brand_accent)),
                                    Span::styled(
                                        trimmed.to_string(),
                                        Style::default().fg(theme.text_primary),
                                    ),
                                ]));
                            }
                        }
                    }
                }
            }
        }

        let lines = Self::wrap_lines_to_width(lines, usable_width);

        let total_lines = lines.len().min(u16::MAX as usize) as u16;
        let viewport_height = area.height;
        let max_scroll = total_lines.saturating_sub(viewport_height);
        self.max_scroll.set(max_scroll);
        let scroll = if self.auto_scroll.get() {
            self.scroll_offset.set(max_scroll);
            max_scroll
        } else {
            let s = self.scroll_offset.get().min(max_scroll);
            self.scroll_offset.set(s);
            s
        };

        self.selection.timeline_area.set(area);
        if self.selection.is_selecting.get()
            || self.selection.has_selection()
            || self.selection.cached_plain_lines.borrow().len() != lines.len()
        {
            self.selection.cache_plain_lines(&lines);
        }
        let lines = self.selection.apply_highlight(lines, theme);

        // Slice directly to the visible viewport and pad every row to `area.width`
        // so differential buffer updates on scroll are 100% deterministic.
        let start_idx = (scroll as usize).min(lines.len());
        let end_idx = (start_idx + viewport_height as usize).min(lines.len());
        let full_w = area.width as usize;
        let clear_style = Style::default().bg(theme.bg_primary).fg(theme.text_primary);
        let mut visible_lines: Vec<Line> = Vec::with_capacity(viewport_height as usize);

        for mut line in lines.into_iter().skip(start_idx).take(end_idx - start_idx) {
            let line_w: usize = line
                .spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            if line_w < full_w {
                line.spans
                    .push(Span::styled(" ".repeat(full_w - line_w), clear_style));
            }
            visible_lines.push(line);
        }
        while visible_lines.len() < viewport_height as usize {
            visible_lines.push(Line::from(vec![Span::styled(
                " ".repeat(full_w),
                clear_style,
            )]));
        }

        // ── Minimal Sticky Top Running/Last Prompt Header ───────────────────
        // When the timeline is scrolled down past the top header (`scroll > 6`),
        // pin the active/last user prompt to the top of the viewport (`rows 0..sticky_h`)
        // using the clean minimal `› ` style (no box background) with up to 2 lines
        // and `... +N lines` indicator.
        if let Some(prompt_text) = last_user_prompt {
            if scroll > 6 && viewport_height >= 12 && full_w >= 24 {
                let raw_lines: Vec<&str> = prompt_text.lines().collect();
                let mut wrapped_prompt_lines: Vec<String> = Vec::new();
                let usable_w = full_w.saturating_sub(4).max(12);
                for raw in &raw_lines {
                    if raw.is_empty() {
                        wrapped_prompt_lines.push(String::new());
                        continue;
                    }
                    let mut cur = String::new();
                    let mut cur_w = 0usize;
                    for ch in raw.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                        if cur_w + cw > usable_w && !cur.is_empty() {
                            wrapped_prompt_lines.push(cur);
                            cur = String::new();
                            cur_w = 0;
                        }
                        cur.push(ch);
                        cur_w += cw;
                    }
                    if !cur.is_empty() {
                        wrapped_prompt_lines.push(cur);
                    }
                }
                let max_sticky_lines = 2usize;
                let total_p_lines = wrapped_prompt_lines.len();
                let shown_p_lines = total_p_lines.min(max_sticky_lines);
                let hidden_p_lines = total_p_lines.saturating_sub(shown_p_lines);

                let sticky_base = Style::default().bg(theme.bg_primary).fg(theme.text_primary);
                let mut sticky_rows: Vec<Line<'static>> = Vec::with_capacity(4);

                for (idx, pline) in wrapped_prompt_lines.iter().take(shown_p_lines).enumerate() {
                    let prefix_str = if idx == 0 { "› " } else { "  " };
                    let prefix_style = if idx == 0 {
                        Style::default()
                            .bg(theme.bg_primary)
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        sticky_base
                    };
                    let mut row_spans = vec![
                        Span::styled(prefix_str, prefix_style),
                        Span::styled(
                            pline.clone(),
                            sticky_base.add_modifier(if idx == 0 {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                        ),
                    ];
                    let is_last_shown = idx + 1 == shown_p_lines;
                    if is_last_shown && hidden_p_lines > 0 {
                        let suffix = format!(" ... +{} lines", hidden_p_lines);
                        let cur_len = 2
                            + UnicodeWidthStr::width(pline.as_str())
                            + UnicodeWidthStr::width(suffix.as_str());
                        if cur_len <= full_w {
                            row_spans.push(Span::styled(
                                suffix,
                                Style::default()
                                    .bg(theme.bg_primary)
                                    .fg(theme.muted)
                                    .add_modifier(Modifier::ITALIC),
                            ));
                        }
                    }
                    let used_w: usize = row_spans
                        .iter()
                        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                        .sum();
                    if used_w < full_w {
                        row_spans.push(Span::styled(" ".repeat(full_w - used_w), sticky_base));
                    }
                    sticky_rows.push(Line::from(row_spans));
                }

                // Subtle half-width divider row separating sticky prompt from scrolling timeline
                let div_w = 60usize.min(full_w.saturating_sub(1));
                let rem_w = full_w.saturating_sub(1 + div_w);
                sticky_rows.push(Line::from(vec![
                    Span::styled(" ", sticky_base),
                    Span::styled(
                        "─".repeat(div_w),
                        Style::default().bg(theme.bg_primary).fg(theme.border),
                    ),
                    Span::styled(" ".repeat(rem_w), sticky_base),
                ]));

                for (r_idx, s_line) in sticky_rows.into_iter().enumerate() {
                    if r_idx < visible_lines.len() {
                        visible_lines[r_idx] = s_line;
                    }
                }
            }
        }

        // ── Floating `↓ Back to bottom · End` Pill When Scrolled Up ─────────
        // Only display when genuinely scrolled above the bottom (`scroll < max_scroll`)
        // and not actively clicking/selecting text in the timeline.
        if !self.auto_scroll.get()
            && scroll < max_scroll
            && max_scroll > 0
            && !self.selection.is_selecting.get()
            && !self.has_selection()
            && viewport_height >= 14
            && full_w >= 32
            && !visible_lines.is_empty()
        {
            let pill_text = " ↓ Back to bottom · End ";
            let pill_style = Style::default()
                .bg(theme.bg_elevated)
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD);
            let last_row_idx = visible_lines.len() - 1;
            let overlaid = Self::overlay_centered_pill(
                &visible_lines[last_row_idx],
                pill_text,
                pill_style,
                clear_style,
                full_w,
            );
            visible_lines[last_row_idx] = overlaid;
        }

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(theme.bg_primary));

        let paragraph = Paragraph::new(visible_lines).block(block);

        frame.render_widget(paragraph, area);
    }

    /// Overlays a centered pill onto an existing rendered line without erasing text on the left or right of the pill.
    fn overlay_centered_pill(
        line: &Line<'_>,
        pill_text: &'static str,
        pill_style: Style,
        clear_style: Style,
        full_w: usize,
    ) -> Line<'static> {
        use unicode_width::UnicodeWidthChar;
        let pill_w = UnicodeWidthStr::width(pill_text);
        let pill_start = full_w.saturating_sub(pill_w) / 2;
        let pill_end = pill_start + pill_w;

        let mut left_spans: Vec<Span<'static>> = Vec::new();
        let mut right_spans: Vec<Span<'static>> = Vec::new();
        let mut cur_col = 0usize;
        let mut left_col_end = 0usize;
        let mut right_col_start: Option<usize> = None;

        for span in &line.spans {
            let mut left_buf = String::new();
            let mut right_buf = String::new();

            for ch in span.content.chars() {
                let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
                let ch_start = cur_col;
                let ch_end = cur_col + cw;
                cur_col = ch_end;

                if ch_end <= pill_start {
                    left_buf.push(ch);
                    left_col_end = ch_end;
                } else if ch_start >= pill_end {
                    if right_col_start.is_none() {
                        right_col_start = Some(ch_start);
                    }
                    right_buf.push(ch);
                }
            }

            if !left_buf.is_empty() {
                left_spans.push(Span::styled(left_buf, span.style));
            }
            if !right_buf.is_empty() {
                right_spans.push(Span::styled(right_buf, span.style));
            }
        }

        if left_col_end < pill_start {
            left_spans.push(Span::styled(
                " ".repeat(pill_start - left_col_end),
                clear_style,
            ));
        }

        let mut out_spans = left_spans;
        out_spans.push(Span::styled(pill_text, pill_style));

        if let Some(r_start) = right_col_start {
            if r_start > pill_end {
                out_spans.push(Span::styled(" ".repeat(r_start - pill_end), clear_style));
            }
            out_spans.extend(right_spans);
        } else if full_w > pill_end {
            out_spans.push(Span::styled(" ".repeat(full_w - pill_end), clear_style));
        }

        Line::from(out_spans)
    }

    /// Computes the hanging-indentation continuation prefix `(prefix_str, style, width)`
    /// for wrapped continuation lines so bullet lists, numbered lists, code blocks,
    /// and indented paragraphs maintain clean column alignment when wrapped.
    fn continuation_prefix_for_line(line: &Line<'_>) -> (String, Style, usize) {
        let first_span = match line.spans.first() {
            Some(s) => s,
            None => return (String::new(), Style::default(), 0),
        };
        let first_text = first_span.content.as_ref();

        if first_text == " › " {
            return ("   ".to_string(), first_span.style, 3);
        }
        if first_text == "  │ " || first_text == "  │   " {
            let w = UnicodeWidthStr::width(first_text);
            return (first_text.to_string(), first_span.style, w);
        }
        if first_text == "│ " {
            if let Some(second) = line.spans.get(1) {
                let s2 = second.content.as_ref();
                if s2 == " ✓ " || s2 == " ▸ " || s2 == " ○ " || s2 == " ✔ " || s2 == " ▶ "
                {
                    return ("│    ".to_string(), first_span.style, 5);
                }
            }
            return ("│ ".to_string(), first_span.style, 2);
        }
        if first_text == "• "
            || first_text == "› "
            || first_text == "✓ "
            || first_text == "✗ "
            || first_text == "! "
            || first_text == "→ "
            || first_text == "▸ "
        {
            return ("  ".to_string(), Style::default(), 2);
        }
        if first_text == "  - " || first_text == "  • " || first_text == "    - " {
            let w = UnicodeWidthStr::width(first_text);
            return (" ".repeat(w), Style::default(), w);
        }
        if first_text.ends_with(". ")
            && first_text.len() <= 6
            && first_text[..first_text.len() - 2]
                .chars()
                .all(|c| c.is_ascii_digit())
        {
            let w = first_text.len();
            return (" ".repeat(w), Style::default(), w);
        }

        // Preserve leading space indentation (up to 8 spaces) without allocating a String
        let leading_spaces = line
            .spans
            .iter()
            .flat_map(|s| s.content.chars())
            .take_while(|&c| c == ' ')
            .take(9)
            .count();
        if leading_spaces > 0 && leading_spaces <= 8 {
            return (" ".repeat(leading_spaces), Style::default(), leading_spaces);
        }

        (String::new(), Style::default(), 0)
    }

    /// Pre-wraps lines to the given visual column width so that each `Line`
    /// maps 1:1 to a rendered terminal row. Groups contiguous non-whitespace characters
    /// across adjacent `Span`s into atomic words and preserves hanging indentation.
    pub fn wrap_lines_to_width<'a>(lines: Vec<Line<'a>>, max_width: usize) -> Vec<Line<'a>> {
        let max_width = max_width.max(10);
        let mut wrapped: Vec<Line<'a>> = Vec::with_capacity(lines.len());

        struct StyledRun {
            chars: Vec<(char, Style)>,
            is_ws: bool,
            width: usize,
        }

        for line in lines {
            let total_width: usize = line
                .spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            let has_newline = line.spans.iter().any(|s| s.content.contains('\n'));

            if total_width <= max_width && !has_newline {
                wrapped.push(line);
                continue;
            }

            let (cont_pfx, cont_style, cont_w) = if max_width >= 20 {
                Self::continuation_prefix_for_line(&line)
            } else {
                (String::new(), Style::default(), 0)
            };
            let (cont_pfx, cont_w) = if cont_w * 2 >= max_width {
                (String::new(), 0)
            } else {
                (cont_pfx, cont_w)
            };

            // Split logical sub-lines on '\n' while preserving per-character styles
            let mut logical_sublines: Vec<Vec<(char, Style)>> = vec![Vec::new()];
            for span in line.spans {
                let style = span.style;
                for ch in span.content.chars() {
                    if ch == '\n' {
                        logical_sublines.push(Vec::new());
                    } else if ch == '\t' {
                        if let Some(last_sub) = logical_sublines.last_mut() {
                            for _ in 0..4 {
                                last_sub.push((' ', style));
                            }
                        }
                    } else if !ch.is_control() {
                        if let Some(last_sub) = logical_sublines.last_mut() {
                            last_sub.push((ch, style));
                        }
                    }
                }
            }

            for sub_chars in logical_sublines {
                if sub_chars.is_empty() {
                    wrapped.push(Line::from(String::new()));
                    continue;
                }

                // Group contiguous whitespace vs non-whitespace characters across spans
                let mut runs: Vec<StyledRun> = Vec::new();
                for (ch, style) in sub_chars {
                    let is_ws = ch.is_whitespace();
                    let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if ch_w == 0 {
                        continue;
                    }
                    if let Some(last) = runs.last_mut() {
                        if last.is_ws == is_ws {
                            last.chars.push((ch, style));
                            last.width += ch_w;
                            continue;
                        }
                    }
                    runs.push(StyledRun {
                        chars: vec![(ch, style)],
                        is_ws,
                        width: ch_w,
                    });
                }

                let mut cur_line_spans: Vec<Span<'a>> = Vec::new();
                let mut cur_line_width: usize = 0;
                let mut is_first_visual_line = true;

                let start_continuation_line = |spans: &mut Vec<Span<'a>>, width: &mut usize| {
                    spans.clear();
                    if cont_w > 0 {
                        spans.push(Span::styled(cont_pfx.clone(), cont_style));
                        *width = cont_w;
                    } else {
                        *width = 0;
                    }
                };

                for run in runs {
                    if run.is_ws {
                        let min_prefix_w = if is_first_visual_line { 0 } else { cont_w };
                        if !is_first_visual_line && cur_line_width <= min_prefix_w {
                            // Ignore leading whitespace right after a wrap on continuation lines
                            continue;
                        } else if cur_line_width + run.width <= max_width {
                            for (ch, st) in run.chars {
                                let mut buf = [0u8; 4];
                                Self::push_span_token(
                                    &mut cur_line_spans,
                                    ch.encode_utf8(&mut buf),
                                    st,
                                );
                            }
                            cur_line_width += run.width;
                        } else {
                            wrapped.push(Line::from(std::mem::take(&mut cur_line_spans)));
                            is_first_visual_line = false;
                            start_continuation_line(&mut cur_line_spans, &mut cur_line_width);
                        }
                    } else {
                        // Atomic multi-span word unit
                        let avail_on_fresh_line = max_width.saturating_sub(cont_w);
                        if cur_line_width + run.width <= max_width {
                            for (ch, st) in run.chars {
                                let mut buf = [0u8; 4];
                                Self::push_span_token(
                                    &mut cur_line_spans,
                                    ch.encode_utf8(&mut buf),
                                    st,
                                );
                            }
                            cur_line_width += run.width;
                        } else if run.width > avail_on_fresh_line {
                            // Word itself exceeds available line width: break by character
                            for (ch, st) in run.chars {
                                let ch_w = UnicodeWidthChar::width(ch).unwrap_or(1);
                                if cur_line_width + ch_w > max_width && cur_line_width > 0 {
                                    wrapped.push(Line::from(std::mem::take(&mut cur_line_spans)));
                                    is_first_visual_line = false;
                                    start_continuation_line(
                                        &mut cur_line_spans,
                                        &mut cur_line_width,
                                    );
                                }
                                let mut buf = [0u8; 4];
                                Self::push_span_token(
                                    &mut cur_line_spans,
                                    ch.encode_utf8(&mut buf),
                                    st,
                                );
                                cur_line_width += ch_w;
                            }
                        } else {
                            // Wrap entire word unit onto next continuation line
                            if !cur_line_spans.is_empty() {
                                wrapped.push(Line::from(std::mem::take(&mut cur_line_spans)));
                            }
                            is_first_visual_line = false;
                            start_continuation_line(&mut cur_line_spans, &mut cur_line_width);
                            for (ch, st) in run.chars {
                                let mut buf = [0u8; 4];
                                Self::push_span_token(
                                    &mut cur_line_spans,
                                    ch.encode_utf8(&mut buf),
                                    st,
                                );
                            }
                            cur_line_width += run.width;
                        }
                    }
                }

                if !cur_line_spans.is_empty() {
                    wrapped.push(Line::from(cur_line_spans));
                }
            }
        }

        wrapped
    }

    fn push_span_token<'a>(spans: &mut Vec<Span<'a>>, text: &str, style: Style) {
        if let Some(last) = spans.last_mut() {
            if last.style == style {
                last.content.to_mut().push_str(text);
                return;
            }
        }
        spans.push(Span::styled(text.to_string(), style));
    }

    /// Renders assistant Markdown text into highlighted Ratatui lines constrained to `max_width`
    fn render_markdown_with_width<'a>(
        text: &'a str,
        theme: &'a Theme,
        max_width: usize,
    ) -> Vec<Line<'a>> {
        crate::ui::markdown::MarkdownRenderer::render_with_width(text, theme, max_width)
    }

    /// Renders assistant Markdown text into highlighted Ratatui lines
    #[allow(dead_code)]
    fn render_markdown<'a>(text: &'a str, theme: &'a Theme) -> Vec<Line<'a>> {
        crate::ui::markdown::MarkdownRenderer::render(text, theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_scrolling_and_auto_scroll_resumption() {
        let view = TimelineView::new();
        view.max_scroll.set(50);
        view.auto_scroll.set(true);

        // 1. Scrolling up disables auto_scroll and steps backward from max_scroll
        view.scroll_up(10);
        assert!(!view.auto_scroll.get());
        assert_eq!(view.scroll_offset.get(), 40);

        // 2. Further scroll up
        view.scroll_up(20);
        assert_eq!(view.scroll_offset.get(), 20);

        // 3. Scroll to top
        view.scroll_to_top();
        assert_eq!(view.scroll_offset.get(), 0);
        assert!(!view.auto_scroll.get());

        // 4. Scroll down
        view.scroll_down(30);
        assert_eq!(view.scroll_offset.get(), 30);
        assert!(!view.auto_scroll.get());

        // 5. Scroll down to or beyond max_scroll re-enables auto_scroll
        view.scroll_down(30);
        assert_eq!(view.scroll_offset.get(), 50);
        assert!(view.auto_scroll.get());

        // 6. Scroll to bottom
        view.scroll_up(15);
        assert!(!view.auto_scroll.get());
        view.scroll_to_bottom();
        assert_eq!(view.scroll_offset.get(), 50);
        assert!(view.auto_scroll.get());
    }

    #[test]
    fn test_timeline_mouse_selection_and_copy() {
        let view = TimelineView::new();
        view.selection.timeline_area.set(Rect::new(0, 0, 80, 24));
        *view.selection.cached_plain_lines.borrow_mut() = vec![
            "Line zero hello world".to_string(),
            "Line one minicode assistant".to_string(),
            "Line two testing auto copy".to_string(),
        ];

        // 1. Single line mouse drag selection
        view.handle_mouse_down(5, 0); // "zero" starts around index 5
        view.handle_mouse_drag(9, 0);
        let extracted = view.handle_mouse_up(9, 0);
        assert_eq!(extracted, Some("zero".to_string()));

        // 2. Multi-line mouse drag selection
        view.handle_mouse_down(5, 0);
        view.handle_mouse_drag(8, 1);
        let multi_extracted = view.handle_mouse_up(8, 1);
        assert!(multi_extracted.is_some());
        let text = multi_extracted.unwrap();
        assert!(text.contains("zero hello world\nLine one"));
    }

    #[test]
    fn test_apply_selection_to_line() {
        let theme = Theme::aura_dark();
        let original_line = Line::from(vec![
            Span::raw("Hello "),
            Span::raw("minicode "),
            Span::raw("world"),
        ]);

        // Select "minicode" (columns 6..14, excluding the space at col 14)
        let highlighted = crate::ui::selection::TimelineSelection::apply_selection_to_line(
            original_line,
            6,
            14,
            &theme,
        );
        assert_eq!(highlighted.spans.len(), 4);
        assert_eq!(highlighted.spans[0].content, "Hello ");
        assert_eq!(highlighted.spans[1].content, "minicode");
        assert_eq!(highlighted.spans[2].content, " ");
        assert_eq!(highlighted.spans[3].content, "world");
        assert!(highlighted.spans[1]
            .style
            .add_modifier
            .contains(Modifier::REVERSED));
    }

    #[test]
    fn test_wrap_lines_to_width() {
        // 1. Short line remains unchanged
        let lines = vec![Line::from("Hello world")];
        let wrapped = TimelineView::wrap_lines_to_width(lines, 40);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].spans[0].content, "Hello world");

        // 2. Long line wraps at word boundary
        let long_line = vec![Line::from("The quick brown fox jumps over the lazy dog")];
        let wrapped = TimelineView::wrap_lines_to_width(long_line, 15);
        assert!(wrapped.len() >= 3);
        for l in &wrapped {
            let line_text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(UnicodeWidthStr::width(line_text.as_str()) <= 15);
        }

        // 3. Very long continuous token wraps by character
        let unbroken = vec![Line::from("abcdefghijklmnopqrstuvwxyz0123456789")];
        let wrapped = TimelineView::wrap_lines_to_width(unbroken, 10);
        assert_eq!(wrapped.len(), 4);
        for l in &wrapped {
            let line_text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(UnicodeWidthStr::width(line_text.as_str()) <= 10);
        }
    }

    #[test]
    fn test_paragraph_rendering_alignment() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut view = TimelineView::new();
        view.entries
            .push(TimelineEntry::UserPrompt("read src/main.rs".to_string()));
        view.entries.push(TimelineEntry::ToolFinished {
            name: "read_file".to_string(),
            command_or_path: "src/main.rs".to_string(),
            output: "line 1\nline 2\nline 3\nline 4\nline 5".to_string(),
            success: true,
            duration_ms: Some(15),
        });
        view.entries.push(TimelineEntry::AssistantMarkdown(
            "This is a longer assistant message that explains what the file contains and will definitely wrap across several lines on a standard width terminal window so we can see how wrapping affects selection.".to_string(),
        ));

        for i in 0..25 {
            view.entries.push(TimelineEntry::SystemStatus(format!(
                "Status entry number {}",
                i
            )));
        }

        let theme = Theme::aura_dark();
        let ctx = TimelineContext {
            theme: &theme,
            is_working: false,
            working_millis: 0,
            current_activity: None,
            spinner_style: crate::ui::animation::SpinnerStyle::default(),
            workspace: std::path::Path::new("."),
            provider: "test",
            model: "test",
        };

        // Render with auto_scroll=true (which will scroll down)
        terminal
            .draw(|f| {
                view.render(f, Rect::new(0, 0, 80, 20), &ctx);
            })
            .unwrap();

        let initial_scroll = view.scroll_offset.get();
        assert!(initial_scroll > 0);

        // Get the plain text displayed at screen row 10
        let buf = terminal.backend().buffer();
        let mut row_10_text = String::new();
        for x in 0..20 {
            row_10_text.push(buf[(x, 10)].symbol().chars().next().unwrap_or(' '));
        }

        // Simulate clicking and dragging on screen row 10
        view.handle_mouse_down(0, 10);
        view.handle_mouse_drag(20, 10);
        let extracted = view.handle_mouse_up(20, 10);
        assert_eq!(extracted, Some(row_10_text.clone()));

        // Re-render to verify that row 10 has the selection style applied
        terminal
            .draw(|f| {
                view.render(f, Rect::new(0, 0, 80, 20), &ctx);
            })
            .unwrap();

        let buf2 = terminal.backend().buffer();
        // Row 10 must be highlighted with REVERSED modifier
        assert!(buf2[(0, 10)].modifier.contains(Modifier::REVERSED));
        // Adjacent rows must NOT be highlighted
        assert!(!buf2[(0, 9)].modifier.contains(Modifier::REVERSED));
        assert!(!buf2[(0, 11)].modifier.contains(Modifier::REVERSED));

        // Test manual scroll alignment: scroll up 5 lines
        view.scroll_up(5);
        terminal
            .draw(|f| {
                view.render(f, Rect::new(0, 0, 80, 20), &ctx);
            })
            .unwrap();

        let buf3 = terminal.backend().buffer();
        let mut scrolled_row_5_text = String::new();
        for x in 0..15 {
            scrolled_row_5_text.push(buf3[(x, 5)].symbol().chars().next().unwrap_or(' '));
        }

        view.handle_mouse_down(0, 5);
        view.handle_mouse_drag(15, 5);
        let scrolled_extracted = view.handle_mouse_up(15, 5);
        assert_eq!(scrolled_extracted, Some(scrolled_row_5_text));
    }

    #[test]
    fn test_minimax_think_tag_parsing_single_chunk() {
        let mut view = TimelineView::new();
        view.add_user_message("hii".to_string());
        let raw = "<think>\nThinking about greeting.\n</think>\n\nHey! How can I help you?";
        view.append_assistant_delta(raw);
        view.finalize_pending_thoughts(Some(1.5));

        let mut has_thought = false;
        let mut has_assistant = false;
        for entry in &view.entries {
            match entry {
                TimelineEntry::ThoughtBlock { text, .. } => {
                    assert!(text.contains("Thinking about greeting."));
                    assert!(!text.contains("<think>"));
                    assert!(!text.contains("</think>"));
                    has_thought = true;
                }
                TimelineEntry::AssistantMarkdown(text) => {
                    assert!(text.contains("Hey! How can I help you?"));
                    assert!(!text.contains("<think>"));
                    assert!(!text.contains("</think>"));
                    has_assistant = true;
                }
                _ => {}
            }
        }
        assert!(has_thought, "Expected a ThoughtBlock for <think>");
        assert!(has_assistant, "Expected AssistantMarkdown without <think>");
    }

    #[test]
    fn test_minimax_think_tag_parsing_split_chunks() {
        let mut view = TimelineView::new();
        view.add_user_message("hii".to_string());
        view.append_assistant_delta("<th");
        view.append_assistant_delta("ink>\nStep 1 reasoning.\n</th");
        view.append_assistant_delta("ink>\nHello world!");
        view.finalize_pending_thoughts(Some(2.0));

        let mut has_thought = false;
        let mut has_assistant = false;
        for entry in &view.entries {
            match entry {
                TimelineEntry::ThoughtBlock { text, .. } => {
                    assert!(text.contains("Step 1 reasoning."));
                    assert!(!text.contains("<think>"));
                    assert!(!text.contains("</think>"));
                    has_thought = true;
                }
                TimelineEntry::AssistantMarkdown(text) => {
                    assert!(text.contains("Hello world!"));
                    assert!(!text.contains("<think>"));
                    assert!(!text.contains("</think>"));
                    has_assistant = true;
                }
                _ => {}
            }
        }
        assert!(has_thought, "Expected ThoughtBlock from split chunks");
        assert!(
            has_assistant,
            "Expected clean AssistantMarkdown from split chunks"
        );
    }

    #[test]
    fn test_format_tool_title() {
        assert_eq!(TimelineView::format_tool_title("exec_cmd"), "Bash");
        assert_eq!(TimelineView::format_tool_title("read_file"), "Read File");
        assert_eq!(TimelineView::format_tool_title("patch_file"), "Edit File");
        assert_eq!(
            TimelineView::format_tool_title("list_directory"),
            "List Dir"
        );
        assert_eq!(
            TimelineView::format_tool_title("grep_search"),
            "Grep Search"
        );
        assert_eq!(
            TimelineView::format_tool_title("custom_inspect_node"),
            "Custom Inspect Node"
        );
    }

    #[test]
    fn test_extract_cmd_display() {
        assert_eq!(
            TimelineView::extract_cmd_display("exec_cmd", r#"{"command": "cargo test"}"#),
            "cargo test"
        );
        assert_eq!(
            TimelineView::extract_cmd_display("read_file", r#"{"path": "src/main.rs"}"#),
            "src/main.rs"
        );
        assert_eq!(
            TimelineView::extract_cmd_display("grep_search", r#"{"query": "required_height"}"#),
            "\"required_height\""
        );
        assert_eq!(TimelineView::extract_cmd_display("git_status", "{}"), "");
    }

    #[test]
    fn test_clean_command_display_heredoc() {
        let cmd = "cat > /path/to/Login.tsx << 'ENDOFFILE'\n'use client';\nexport default function() {}\nENDOFFILE";
        assert_eq!(
            TimelineView::clean_command_display(cmd),
            "cat > /path/to/Login.tsx"
        );
    }

    #[test]
    fn test_clean_command_display_truncation() {
        let cmd = "npm run very-long-command-name-that-keeps-going-and-going-and-exceeds-seventy-characters-easily";
        let cleaned = TimelineView::clean_command_display(cmd);
        assert!(cleaned.len() <= 70);
        assert!(cleaned.ends_with("..."));
    }

    #[test]
    fn test_split_line_into_chunks() {
        let line = "export interface Customer { id: string; name: string; email: string; }";
        let chunks = TimelineView::split_line_into_chunks(line, 30);
        assert!(chunks.len() > 1);
        for c in chunks {
            assert!(unicode_width::UnicodeWidthStr::width(c.as_str()) <= 30);
        }
    }

    #[test]
    fn test_finish_tool_call_updates_in_place() {
        let mut view = TimelineView::new();
        view.add_tool_call(
            "exec_cmd".to_string(),
            r#"{"command": "cargo check"}"#.to_string(),
        );

        assert_eq!(view.entries.len(), 1);
        match &view.entries[0] {
            TimelineEntry::ToolStart {
                name,
                command_or_path,
            } => {
                assert_eq!(name, "exec_cmd");
                assert_eq!(command_or_path, "cargo check");
            }
            _ => panic!("Expected ToolStart"),
        }

        view.finish_tool_call("exec_cmd", true, "Finished dev profile".to_string(), 120);

        // Crucial test: entries.len() MUST remain 1 (no duplicate Running + Ran entries!)
        assert_eq!(view.entries.len(), 1);
        match &view.entries[0] {
            TimelineEntry::ToolFinished {
                name,
                command_or_path,
                success,
                output,
                duration_ms,
            } => {
                assert_eq!(name, "exec_cmd");
                assert_eq!(command_or_path, "cargo check");
                assert!(*success);
                assert!(output.contains("Finished dev profile"));
                assert_eq!(*duration_ms, Some(120));
            }
            _ => panic!("Expected ToolFinished"),
        }
    }

    #[test]
    fn test_live_plan_update_and_transcript() {
        let mut view = TimelineView::new();
        let plan1 = LivePlanBlock {
            title: "Plan Progress".to_string(),
            total_tasks: 3,
            completed_tasks: 1,
            active_task: Some("T2: Implementation".to_string()),
            tasks: vec![
                LivePlanTaskItem {
                    title: "T1: Design".to_string(),
                    status: LivePlanTaskStatus::Completed,
                    sub_tasks: Vec::new(),
                },
                LivePlanTaskItem {
                    title: "T2: Implementation".to_string(),
                    status: LivePlanTaskStatus::InProgress,
                    sub_tasks: Vec::new(),
                },
                LivePlanTaskItem {
                    title: "T3: Verification".to_string(),
                    status: LivePlanTaskStatus::Pending,
                    sub_tasks: Vec::new(),
                },
            ],
        };

        view.update_live_plan(plan1);
        assert_eq!(view.entries.len(), 1);

        // Update in-place
        let plan2 = LivePlanBlock {
            title: "Plan Progress".to_string(),
            total_tasks: 3,
            completed_tasks: 2,
            active_task: Some("T3: Verification".to_string()),
            tasks: vec![
                LivePlanTaskItem {
                    title: "T1: Design".to_string(),
                    status: LivePlanTaskStatus::Completed,
                    sub_tasks: Vec::new(),
                },
                LivePlanTaskItem {
                    title: "T2: Implementation".to_string(),
                    status: LivePlanTaskStatus::Completed,
                    sub_tasks: Vec::new(),
                },
                LivePlanTaskItem {
                    title: "T3: Verification".to_string(),
                    status: LivePlanTaskStatus::InProgress,
                    sub_tasks: Vec::new(),
                },
            ],
        };

        view.update_live_plan(plan2);
        // Crucial test: in-place update must not duplicate entries
        assert_eq!(view.entries.len(), 1);

        let transcript = view.get_all_transcript_text();
        assert!(transcript.contains("### Plan Progress: 2/3 Tasks Completed"));
        assert!(transcript.contains("- [x] T1: Design"));
        assert!(transcript.contains("- [x] T2: Implementation"));
        assert!(transcript.contains("- [/] T3: Verification"));
    }

    #[test]
    fn test_wrap_lines_preserves_multi_span_atomic_words_and_hanging_indent() {
        let theme = crate::ui::theme::Theme::tokyo_night();
        let md = "1. Configured Obsidian-Slate (`#090B12`) with `badge`, `button`, and `table` components.";
        let rendered = crate::ui::markdown::MarkdownRenderer::render_with_width(md, &theme, 44);
        let wrapped = TimelineView::wrap_lines_to_width(rendered, 44);

        let plain_lines: Vec<String> = wrapped
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .filter(|s| !s.trim().is_empty())
            .collect();

        assert!(plain_lines.len() >= 2);
        // Continuation lines of a numbered list ("1. ") must have 3-space hanging indent
        for cont in &plain_lines[1..] {
            assert!(
                cont.starts_with("   "),
                "Expected 3-space hanging indent on continuation line, got: {:?}",
                cont
            );
            // Punctuation attached to inline code must never wrap onto a line by itself
            let trimmed = cont.trim_start();
            assert!(
                !trimmed.starts_with(")") && !trimmed.starts_with(","),
                "Punctuation should stay attached to inline code word, got: {:?}",
                cont
            );
        }
        // Check that `(#090B12)` stayed together on a single line
        assert!(
            plain_lines.iter().any(|l| l.contains("(#090B12)")),
            "Expected (#090B12) to stay intact as a single word unit across spans: {:?}",
            plain_lines
        );
    }

    #[test]
    fn test_wide_table_and_scroll_alignment_in_timeline_render() {
        use ratatui::{backend::TestBackend, layout::Rect, Terminal};

        let theme = crate::ui::theme::Theme::tokyo_night();
        let mut view = TimelineView::new();
        let md = "| Area | Implementation |\n|---|---|\n| **Design System** | Configured Obsidian-Slate (`#090B12`) + Indigo (`#6366F1`) + Emerald (`#10B981`) |\n| **Verification** | `npm run build` succeeded with 0 TypeScript or Vite bundle errors |\n";
        view.append_assistant_text(md);

        let backend = TestBackend::new(72, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 72, 12);

        for scroll_pos in [0u16, 1, 2, 3] {
            view.scroll_offset.set(scroll_pos);
            view.auto_scroll.set(false);
            terminal
                .draw(|frame| {
                    let ctx = TimelineContext {
                        theme: &theme,
                        is_working: false,
                        working_millis: 0,
                        current_activity: None,
                        spinner_style: crate::ui::animation::SpinnerStyle::from_id("braille"),
                        workspace: std::path::Path::new("/tmp"),
                        provider: "openrouter",
                        model: "qwen",
                    };
                    view.render(frame, area, &ctx);
                })
                .unwrap();

            let buf = terminal.backend().buffer();
            for y in 0..area.height {
                let mut row_str = String::new();
                for x in 0..area.width {
                    row_str.push_str(buf[(x, y)].symbol());
                }
                if row_str.contains('│') {
                    let trimmed = row_str.trim();
                    assert!(
                        trimmed.starts_with('│') && trimmed.ends_with('│'),
                        "Table row at scroll={} y={} has misaligned borders: {:?}",
                        scroll_pos,
                        y,
                        row_str
                    );
                }
            }
        }
    }

    #[test]
    fn test_clicking_bottom_line_does_not_show_back_to_bottom_pill() {
        use ratatui::{backend::TestBackend, layout::Rect, Terminal};

        let theme = crate::ui::theme::Theme::tokyo_night();
        let mut view = TimelineView::new();
        for i in 0..30 {
            view.append_assistant_text(&format!("Line {}\n", i));
        }
        view.append_assistant_text("Dev server still running at http://127.0.0.1:4177.\n");

        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 16);

        let draw_once = |v: &TimelineView, term: &mut Terminal<TestBackend>| {
            term.draw(|frame| {
                let ctx = TimelineContext {
                    theme: &theme,
                    is_working: false,
                    working_millis: 0,
                    current_activity: None,
                    spinner_style: crate::ui::animation::SpinnerStyle::from_id("braille"),
                    workspace: std::path::Path::new("/tmp"),
                    provider: "minimax",
                    model: "MiniMax-M3.1-Flash",
                };
                v.render(frame, area, &ctx);
            })
            .unwrap();
        };

        // 1. Initial render at bottom
        draw_once(&view, &mut terminal);
        assert!(view.auto_scroll.get());

        // 2. Click on the last line (where the URL is) while at the bottom
        view.handle_mouse_down(32, 15);
        let _ = view.handle_mouse_up(32, 15);
        draw_once(&view, &mut terminal);

        let buf = terminal.backend().buffer();
        let mut all_text = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                all_text.push_str(buf[(x, y)].symbol());
            }
            all_text.push('\n');
        }
        assert!(
            !all_text.contains("Back to bottom"),
            "Clicking at the bottom of the timeline must not trigger 'Back to bottom' pill"
        );
        assert!(
            all_text.contains("http://127.0.0.1:4177"),
            "The URL on the last line must remain visible when clicked"
        );
    }
}
