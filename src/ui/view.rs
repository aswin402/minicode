use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

#[path = "view_impl/entry_renderers.rs"]
mod entry_renderers;
#[path = "view_impl/subagents_and_selection.rs"]
mod subagents_and_selection;
#[path = "view_impl/wrapping_and_format.rs"]
mod wrapping_and_format;

#[cfg(test)]
#[path = "view_impl/tests.rs"]
mod tests;

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

    pub(super) fn append_assistant_text(&mut self, text: &str) {
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

        let mut wrapped_lines: Vec<Line> = Vec::with_capacity(self.entries.len() * 4);
        let mut prompt_anchors: Vec<(usize, &str)> = Vec::new();

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
                    if !lines.is_empty() {
                        wrapped_lines.extend(Self::wrap_lines_to_width(
                            std::mem::take(&mut lines),
                            usable_width,
                        ));
                    }
                    prompt_anchors.push((wrapped_lines.len(), prompt.as_str()));
                    let prompt_lines: Vec<&str> = prompt.lines().collect();

                    if prompt_lines.is_empty() {
                        lines.push(Line::from(vec![Span::styled(
                            "› ",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        )]));
                    } else {
                        for (idx, pline) in prompt_lines.iter().enumerate() {
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
                            let row_spans = vec![
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
                            12
                        };
                        let mut after_panic = false;
                        for raw_out_line in trimmed.lines().take(max_lines) {
                            let out_line = if !*success && raw_out_line.contains("Raw arguments") {
                                "[Arguments payload truncated: model exceeded single-call capacity]"
                            } else {
                                raw_out_line
                            };
                            let trimmed_line = out_line.trim();
                            let is_panic_header = trimmed_line.contains("panicked at");

                            let line_style = if trimmed_line.starts_with("left:")
                                || (out_line.starts_with('-') && !out_line.starts_with("---"))
                            {
                                Style::default().fg(theme.destructive)
                            } else if trimmed_line.starts_with("right:")
                                || (out_line.starts_with('+') && !out_line.starts_with("+++"))
                            {
                                Style::default().fg(theme.success)
                            } else if is_panic_header
                                || after_panic
                                || trimmed_line.starts_with("assertion ")
                                || trimmed_line.starts_with("error[")
                                || trimmed_line.starts_with("error:")
                                || trimmed_line.starts_with("✗")
                            {
                                Style::default()
                                    .fg(theme.destructive)
                                    .add_modifier(Modifier::BOLD)
                            } else if trimmed_line.starts_with("warning:")
                                || trimmed_line.starts_with("⚠")
                            {
                                Style::default().fg(theme.warning)
                            } else if trimmed_line.starts_with('✔') || trimmed_line.starts_with('✓')
                            {
                                Style::default()
                                    .fg(theme.success)
                                    .add_modifier(Modifier::BOLD)
                            } else if trimmed_line.starts_with("libs compiled")
                                || trimmed_line.starts_with("disk used")
                                || trimmed_line.starts_with("binary size")
                                || out_line.starts_with("##")
                                || trimmed_line.starts_with("test result:")
                            {
                                Style::default().fg(theme.info)
                            } else if !*success {
                                Style::default().fg(theme.destructive)
                            } else {
                                Style::default().fg(theme.muted)
                            };
                            after_panic = is_panic_header;

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
                                    Span::styled(chunk, line_style),
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
                    Self::render_subagent_tree_entry(block, ctx, theme, &mut lines);
                }
                TimelineEntry::SubagentSwarm(swarm) => {
                    Self::render_subagent_swarm_entry(swarm, ctx, theme, area.width, &mut lines);
                }
                TimelineEntry::ContextCompaction {
                    tier,
                    turns_summarized,
                    tokens_before,
                    tokens_after,
                    savings_percent,
                } => {
                    Self::render_context_compaction_entry(
                        *tier,
                        *turns_summarized,
                        *tokens_before,
                        *tokens_after,
                        *savings_percent,
                        theme,
                        &mut lines,
                    );
                }
                TimelineEntry::LivePlan(plan) => {
                    Self::render_live_plan_entry(plan, theme, area.width, &mut lines);
                }
                TimelineEntry::SystemStatus(status) => {
                    Self::render_system_status_entry(status, theme, &mut lines);
                }
            }
        }

        if !lines.is_empty() {
            wrapped_lines.extend(Self::wrap_lines_to_width(lines, usable_width));
        }
        let lines = wrapped_lines;

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

        // ── Scroll-Aware Stacked Sticky Top Prompt Header ───────────────────
        // Finds the active prompt section for the current `scroll` position:
        // - When `scroll_row <= prompt_1_row` (above/at Prompt 1): no sticky prompt is shown.
        // - When in Prompt 1's section (`prompt_1_row < scroll_row <= prompt_2_row`): Prompt 1 is sticky.
        // - When in Prompt 2's section (`prompt_2_row < scroll_row <= prompt_3_row`): Prompt 2 is sticky.
        // - When in Prompt 3's section (`scroll_row > prompt_3_row`): Prompt 3 is sticky.
        // Also smoothly pushes up the sticky header when the next prompt approaches the top edge.
        let scroll_row = scroll as usize;
        let active_anchor_idx = prompt_anchors
            .iter()
            .rposition(|&(prompt_row, _)| scroll_row > prompt_row);

        if let Some(anchor_idx) = active_anchor_idx {
            let (_, prompt_text) = prompt_anchors[anchor_idx];
            let next_prompt_row = prompt_anchors.get(anchor_idx + 1).map(|&(r, _)| r);
            if viewport_height >= 12 && full_w >= 24 {
                let raw_lines: Vec<&str> = prompt_text.lines().collect();
                let mut wrapped_prompt_lines: Vec<String> = Vec::new();
                let usable_w = full_w.saturating_sub(4).max(12);
                for raw in &raw_lines {
                    if raw.is_empty() {
                        wrapped_prompt_lines.push(String::new());
                        continue;
                    }
                    for chunk in Self::split_line_into_chunks(raw, usable_w) {
                        wrapped_prompt_lines.push(chunk);
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

                let max_allowed_rows = next_prompt_row
                    .map(|nr| nr.saturating_sub(scroll_row))
                    .unwrap_or(sticky_rows.len());

                for (r_idx, s_line) in sticky_rows.into_iter().take(max_allowed_rows).enumerate() {
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
}
