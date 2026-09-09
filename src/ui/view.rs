use crate::constants::SPINNER_FRAMES;
use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

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

    /// Scrolls timeline up by a page
    pub fn scroll_page_up(&self, viewport_height: u16) {
        let step = if viewport_height > 2 {
            viewport_height.saturating_sub(2)
        } else {
            5
        };
        self.scroll_up(step);
    }

    /// Scrolls timeline down by a page
    pub fn scroll_page_down(&self, viewport_height: u16) {
        let step = if viewport_height > 2 {
            viewport_height.saturating_sub(2)
        } else {
            5
        };
        self.scroll_down(step);
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
        if self.thought_start.is_none() {
            self.thought_start = self.turn_start.or_else(|| Some(std::time::Instant::now()));
        }
        if let Some(TimelineEntry::ThoughtBlock { text, .. }) = self.entries.last_mut() {
            text.push_str(&cleaned);
        } else {
            self.entries.push(TimelineEntry::ThoughtBlock {
                text: cleaned,
                duration_secs: None,
            });
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

    pub fn finalize_pending_thoughts(&mut self, elapsed_secs: Option<f64>) {
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
                *duration_secs = elapsed_secs
                    .or_else(|| self.thought_start.take().map(|s| s.elapsed().as_secs_f64()))
                    .or_else(|| self.turn_start.map(|s| s.elapsed().as_secs_f64()));
            }
        }
        self.in_thought_mode = false;
    }

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
        let mut s = raw.to_string();
        for tag in Self::ALL_THOUGHT_TAGS {
            s = s.replace(tag, "");
        }
        s
    }

    /// Strips any stray thinking XML tags that might leak into assistant markdown
    pub fn sanitize_assistant_text(raw: &str) -> String {
        let mut s = raw.to_string();
        for tag in Self::ALL_THOUGHT_TAGS {
            s = s.replace(tag, "");
        }
        s
    }

    pub fn append_assistant_delta(&mut self, delta: &str) {
        const OPEN_TAGS: &[&str] = &[
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
        const CLOSE_TAGS: &[&str] = &[
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
                    let dur = self
                        .thought_start
                        .take()
                        .or(self.turn_start)
                        .map(|s| s.elapsed().as_secs_f64());
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
                    self.thought_start =
                        self.turn_start.or_else(|| Some(std::time::Instant::now()));
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
                _ => {}
            }
        }
        out
    }

    /// Handles mouse button press to begin text selection
    pub fn handle_mouse_down(&self, col: u16, row: u16) {
        let area = self.selection.timeline_area.get();
        if col >= area.x && col < area.right() && row >= area.y && row < area.bottom() {
            self.selection
                .handle_mouse_down(col, row, self.scroll_offset.get());
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
            "onpkg_doctor" => "onpkg Doctor".to_string(),
            "onpkg_stack_list" => "onpkg Stack List".to_string(),
            "onpkg_stack_add" => "onpkg Stack Add".to_string(),
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

    fn extract_cmd_display(name: &str, args_json: &str) -> String {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(args_json) {
            if let Some(cmd) = val
                .get("command")
                .or_else(|| val.get("cmd"))
                .and_then(|c| c.as_str())
            {
                return cmd.to_string();
            }
            if let Some(path) = val
                .get("path")
                .or_else(|| val.get("file_path"))
                .or_else(|| val.get("target_file"))
                .and_then(|p| p.as_str())
            {
                return path.to_string();
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
            format!("{}({})", name, trimmed)
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, ctx: &TimelineContext) {
        let theme = ctx.theme;
        let mut lines: Vec<Line> = Vec::new();

        if self.entries.is_empty() {
            return;
        }

        for entry in &self.entries {
            match entry {
                TimelineEntry::TurnSeparator => {
                    lines.push(Line::from(String::new()));
                    lines.push(Line::from(vec![Span::styled(
                        "─".repeat(area.width.saturating_sub(2) as usize),
                        Style::default().fg(theme.border),
                    )]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::UserPrompt(prompt) => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "› ",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            prompt,
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::ThoughtBlock {
                    text,
                    duration_secs,
                } => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        let dur_display = match duration_secs {
                            Some(secs) if *secs >= 0.1 => *secs,
                            _ => ((ctx.working_millis as f64) / 1000.0).max(0.1),
                        };
                        let header = format!("• Thought for {:.1}s", dur_display);

                        lines.push(Line::from(vec![Span::styled(
                            header,
                            Style::default()
                                .fg(theme.muted)
                                .add_modifier(Modifier::BOLD),
                        )]));
                        for t_line in trimmed.lines() {
                            lines.push(Line::from(vec![
                                Span::styled("  ", Style::default()),
                                Span::styled(
                                    t_line,
                                    Style::default()
                                        .fg(theme.muted)
                                        .add_modifier(Modifier::ITALIC),
                                ),
                            ]));
                        }
                        lines.push(Line::from(String::new()));
                    }
                }
                TimelineEntry::AssistantMarkdown(text) => {
                    let parsed_lines = Self::render_markdown(text, theme);
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
                        spans.push(Span::styled(
                            command_or_path,
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
                    lines.push(Line::from(vec![
                        Span::styled(
                            "✔ ",
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
                        Span::styled(command_or_path, Style::default().fg(theme.text_primary)),
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

                    let mut spans = vec![
                        Span::styled("• ", Style::default().fg(status_color)),
                        Span::styled(
                            format!("{} ", title),
                            Style::default()
                                .fg(status_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ];
                    if !command_or_path.is_empty() {
                        spans.push(Span::styled(
                            command_or_path,
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
                        let mut diff_line_count = 0;
                        for diff_line in diff_section.lines() {
                            if diff_line.starts_with("---") || diff_line.starts_with("+++") {
                                lines.push(Line::from(vec![
                                    Span::styled("  │ ", Style::default().fg(theme.muted)),
                                    Span::styled(
                                        diff_line,
                                        Style::default()
                                            .fg(theme.muted)
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]));
                            } else if let Some(rest) = diff_line.strip_prefix("+ ") {
                                lines.push(Line::from(vec![
                                    Span::styled("  │ ", Style::default().fg(theme.muted)),
                                    Span::styled(
                                        "+",
                                        Style::default()
                                            .fg(theme.success)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        format!(" {}", rest),
                                        Style::default().fg(theme.success),
                                    ),
                                ]));
                                diff_line_count += 1;
                            } else if let Some(rest) = diff_line.strip_prefix("- ") {
                                lines.push(Line::from(vec![
                                    Span::styled("  │ ", Style::default().fg(theme.muted)),
                                    Span::styled(
                                        "-",
                                        Style::default()
                                            .fg(theme.destructive)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        format!(" {}", rest),
                                        Style::default().fg(theme.destructive),
                                    ),
                                ]));
                                diff_line_count += 1;
                            } else if let Some(rest) = diff_line.strip_prefix("  ") {
                                lines.push(Line::from(vec![
                                    Span::styled("  │   ", Style::default().fg(theme.muted)),
                                    Span::styled(
                                        rest.to_string(),
                                        Style::default().fg(theme.muted),
                                    ),
                                ]));
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
                        for out_line in trimmed
                            .lines()
                            .take(crate::constants::UI_MAX_TOOL_OUTPUT_LINES)
                        {
                            let line_color = if out_line.starts_with('+') {
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

                            lines.push(Line::from(vec![
                                Span::styled("  │ ", Style::default().fg(theme.muted)),
                                Span::styled(out_line, Style::default().fg(line_color)),
                            ]));
                        }
                        if trimmed.lines().count() > crate::constants::UI_MAX_TOOL_OUTPUT_LINES {
                            let remaining = trimmed
                                .lines()
                                .count()
                                .saturating_sub(crate::constants::UI_MAX_TOOL_OUTPUT_LINES);
                            lines.push(Line::from(vec![
                                Span::styled("  │ ", Style::default().fg(theme.muted)),
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

                    lines.push(Line::from(vec![
                        Span::styled("  └ ", Style::default().fg(theme.muted)),
                        Span::styled(
                            format!("{} ", status_glyph),
                            Style::default()
                                .fg(glyph_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{}{}", status_text, dur_str),
                            Style::default().fg(theme.muted),
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
                        ("✔", theme.success, dur)
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
                            Span::styled(*first_line, Style::default().fg(theme.text_primary)),
                        ]));

                        for rest_line in prompt_lines.iter().skip(1) {
                            lines.push(Line::from(vec![
                                Span::styled("         ", Style::default()),
                                Span::styled(*rest_line, Style::default().fg(theme.text_primary)),
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
                            SubagentItemStatus::Success => ("✔", theme.success),
                            SubagentItemStatus::Failed(_) => ("✗", theme.destructive),
                        };

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
                            Span::styled(&item.detail, Style::default().fg(theme.muted)),
                        ]));
                    }

                    // Outcome / Error
                    if let Some(ref outcome) = block.outcome {
                        lines.push(Line::from(String::new()));
                        lines.push(Line::from(vec![
                            Span::styled(
                                "  Outcome: ",
                                Style::default()
                                    .fg(theme.success)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(outcome.trim(), Style::default().fg(theme.text_primary)),
                        ]));
                    } else if let Some(ref err) = block.error_message {
                        lines.push(Line::from(String::new()));
                        lines.push(Line::from(vec![
                            Span::styled(
                                "  Error: ",
                                Style::default()
                                    .fg(theme.destructive)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(err.trim(), Style::default().fg(theme.destructive)),
                        ]));
                    }

                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::SubagentSwarm(swarm) => {
                    let frame_idx = ((ctx.working_millis / crate::constants::SPINNER_FRAME_MS)
                        as usize)
                        % SPINNER_FRAMES.len();
                    let spinner = SPINNER_FRAMES[frame_idx];

                    let top_bullet = if swarm.is_running { spinner } else { "✔" };
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
                                ("✔", theme.success)
                            } else {
                                ("✗", theme.destructive)
                            };

                            let last_action = worker
                                .items
                                .last()
                                .map(|it| format!("{} {}", it.name, it.detail))
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
                                ("✔", theme.success)
                            } else {
                                ("✗", theme.destructive)
                            };

                            lines.push(Line::from(vec![
                                Span::styled("│ ", Style::default().fg(theme.border)),
                                Span::styled(
                                    format!("▼ {} ", w_bullet),
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
                                    worker.task_prompt.lines().next().unwrap_or(""),
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
                                    SubagentItemStatus::Success => ("✔", theme.success),
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
                                    Span::styled(&item.detail, Style::default().fg(theme.muted)),
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
                        Span::styled("🗜️ ", Style::default().fg(theme.brand_accent)),
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
                TimelineEntry::SystemStatus(status) => {
                    let trimmed = status.trim_start();
                    let prefixes = [
                        ("✔ ", theme.success),
                        ("✓ ", theme.success),
                        ("✗ ", theme.destructive),
                        ("✖ ", theme.destructive),
                        ("ℹ ", theme.info),
                        ("⏹ ", theme.warning),
                        ("• ", theme.brand_accent),
                        ("✨ ", theme.brand_accent),
                        ("🛡️ ", theme.info),
                        ("🛡 ", theme.info),
                        ("🗑️ ", theme.destructive),
                        ("🗑 ", theme.destructive),
                        ("● ", theme.brand_accent),
                        ("⚠ ", theme.warning),
                    ];

                    let mut matched = false;
                    for (prefix, color) in prefixes {
                        if let Some(rest) = trimmed.strip_prefix(prefix) {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    prefix,
                                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(rest, Style::default().fg(theme.text_primary)),
                            ]));
                            matched = true;
                            break;
                        }
                    }

                    if !matched {
                        lines.push(Line::from(vec![
                            Span::styled("• ", Style::default().fg(theme.brand_accent)),
                            Span::styled(status, Style::default().fg(theme.text_primary)),
                        ]));
                    }
                }
            }
        }

        self.selection.timeline_area.set(area);
        self.selection.cache_plain_lines(&lines);
        let lines = self.selection.apply_highlight(lines, theme);

        let total_lines = Self::visual_row_count(&lines, area.width);
        let viewport_height = area.height;
        let max_scroll = total_lines.saturating_sub(viewport_height);
        self.max_scroll.set(max_scroll);
        let scroll = if self.auto_scroll.get() {
            self.scroll_offset.set(max_scroll);
            max_scroll
        } else {
            self.scroll_offset.get().min(max_scroll)
        };

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(theme.bg_primary));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));

        frame.render_widget(paragraph, area);
    }

    /// Counts visually rendered rows for `lines` at the given wrap width.
    ///
    /// ratatui's Paragraph::wrap expands long logical lines into multiple
    /// visual rows by wrapping on whitespace word boundaries. We simulate
    /// this word-wrapping to ensure scroll bounds never underestimate height,
    /// which would otherwise push streaming content off-screen.
    fn visual_row_count(lines: &[Line<'_>], width: u16) -> u16 {
        let usable = usize::from(width.max(1));
        let total: usize = lines
            .iter()
            .map(|line| {
                let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                if text.is_empty() {
                    return 1;
                }
                let mut rows: usize = 0;
                for sub_line in text.split('\n') {
                    if sub_line.is_empty() {
                        rows += 1;
                        continue;
                    }
                    let mut line_rows: usize = 1;
                    let mut current_row_width: usize = 0;

                    for word in sub_line.split(' ') {
                        let word_width = UnicodeWidthStr::width(word);
                        if current_row_width == 0 {
                            if word_width > usable {
                                let extra = (word_width.saturating_sub(1)) / usable;
                                line_rows += extra;
                                current_row_width = word_width % usable;
                                if current_row_width == 0 {
                                    current_row_width = usable;
                                }
                            } else {
                                current_row_width = word_width;
                            }
                        } else if current_row_width + 1 + word_width <= usable {
                            current_row_width += 1 + word_width;
                        } else {
                            line_rows += 1;
                            if word_width > usable {
                                let extra = (word_width.saturating_sub(1)) / usable;
                                line_rows += extra;
                                current_row_width = word_width % usable;
                                if current_row_width == 0 {
                                    current_row_width = usable;
                                }
                            } else {
                                current_row_width = word_width;
                            }
                        }
                    }
                    rows += line_rows;
                }
                rows
            })
            .sum();
        total.min(u16::MAX as usize) as u16
    }

    /// Renders assistant Markdown text into highlighted Ratatui lines
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
}
