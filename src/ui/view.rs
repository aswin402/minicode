use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

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
}

pub struct TimelineContext<'a> {
    pub theme: &'a Theme,
    pub is_working: bool,
    pub working_millis: u64,
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
        }
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

    pub fn add_user_message(&mut self, prompt: String) {
        if !self.entries.is_empty() {
            self.entries.push(TimelineEntry::TurnSeparator);
        }
        self.entries.push(TimelineEntry::UserPrompt(prompt));
        self.turn_start = Some(std::time::Instant::now());
        self.thought_start = None;
        self.in_thought_mode = false;
    }

    pub fn append_thought_delta(&mut self, delta: &str) {
        if self.thought_start.is_none() {
            self.thought_start = self.turn_start.or_else(|| Some(std::time::Instant::now()));
        }
        if let Some(TimelineEntry::ThoughtBlock { text, .. }) = self.entries.last_mut() {
            text.push_str(delta);
        } else {
            self.entries.push(TimelineEntry::ThoughtBlock {
                text: delta.to_string(),
                duration_secs: None,
            });
        }
    }

    #[allow(dead_code)]
    pub fn add_thought_block(&mut self, text: String, duration_secs: Option<f64>) {
        self.entries.push(TimelineEntry::ThoughtBlock {
            text,
            duration_secs,
        });
    }

    pub fn finalize_pending_thoughts(&mut self, elapsed_secs: Option<f64>) {
        if let Some(TimelineEntry::ThoughtBlock { duration_secs, .. }) = self.entries.last_mut() {
            if duration_secs.is_none() || duration_secs.unwrap_or(0.0) < 0.1 {
                *duration_secs = elapsed_secs
                    .or_else(|| self.thought_start.take().map(|s| s.elapsed().as_secs_f64()))
                    .or_else(|| self.turn_start.map(|s| s.elapsed().as_secs_f64()));
            }
        }
        self.in_thought_mode = false;
    }

    pub fn append_assistant_delta(&mut self, delta: &str) {
        let mut text = delta;

        while !text.is_empty() {
            if self.in_thought_mode {
                if let Some(end_idx) = text.find("</thought>") {
                    let thought_chunk = &text[..end_idx];
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
                    text = &text[end_idx + "</thought>".len()..];
                } else {
                    self.append_thought_delta(text);
                    break;
                }
            } else if let Some(start_idx) = text.find("<thought>") {
                let prefix = &text[..start_idx];
                if !prefix.is_empty() {
                    self.append_assistant_text(prefix);
                }
                self.in_thought_mode = true;
                self.thought_start = self.turn_start.or_else(|| Some(std::time::Instant::now()));
                text = &text[start_idx + "<thought>".len()..];
            } else {
                self.append_assistant_text(text);
                break;
            }
        }
    }

    fn append_assistant_text(&mut self, text: &str) {
        if let Some(TimelineEntry::AssistantMarkdown(ref mut existing)) = self.entries.last_mut() {
            existing.push_str(text);
        } else {
            self.entries
                .push(TimelineEntry::AssistantMarkdown(text.to_string()));
        }
    }

    pub fn add_tool_call(&mut self, name: String, args: String) {
        let display_cmd = Self::extract_cmd_display(&name, &args);
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
        // Find corresponding tool start
        let mut display_cmd = String::new();
        for entry in self.entries.iter().rev() {
            if let TimelineEntry::ToolStart {
                command_or_path,
                name: n,
            } = entry
            {
                if n == name {
                    display_cmd = command_or_path.clone();
                    break;
                }
            }
        }

        self.entries.push(TimelineEntry::ToolFinished {
            name: name.to_string(),
            command_or_path: display_cmd,
            success,
            output,
            duration_ms: Some(duration_ms),
        });
    }

    pub fn add_status(&mut self, status: String) {
        self.entries.push(TimelineEntry::SystemStatus(status));
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
        if col >= area.x && col < area.x + area.width && row >= area.y && row < area.y + area.height
        {
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

    fn extract_cmd_display(name: &str, args_json: &str) -> String {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(args_json) {
            if let Some(cmd) = val.get("command").and_then(|c| c.as_str()) {
                return cmd.to_string();
            }
            if let Some(path) = val.get("path").and_then(|p| p.as_str()) {
                return path.to_string();
            }
            if let Some(query) = val.get("query").and_then(|q| q.as_str()) {
                return format!("query: {}", query);
            }
            if let Some(url) = val.get("url").and_then(|u| u.as_str()) {
                return url.to_string();
            }
        }
        format!("{}({})", name, args_json)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, ctx: &TimelineContext) {
        let theme = ctx.theme;
        let mut lines: Vec<Line> = Vec::new();

        if self.entries.is_empty() {
            let display_path = if let Some(ref home) = dirs::home_dir() {
                if let Ok(rel) = ctx.workspace.strip_prefix(home) {
                    format!("~/{}", rel.display())
                } else {
                    ctx.workspace.display().to_string()
                }
            } else {
                ctx.workspace.display().to_string()
            };

            // 3D Isometric Block (Retro-Futuristic) minicode Wordmark
            lines.push(Line::from(String::new()));
            lines.push(Line::from(vec![Span::styled(
                crate::constants::ASCII_WORDMARK_LINES[0],
                Style::default().fg(theme.brand_accent),
            )]));

            lines.push(Line::from(vec![
                Span::styled(
                    crate::constants::ASCII_WORDMARK_LINES[1],
                    Style::default().fg(theme.brand_accent),
                ),
                Span::styled(
                    format!("  v{}", env!("CARGO_PKG_VERSION")),
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            lines.push(Line::from(vec![Span::styled(
                crate::constants::ASCII_WORDMARK_LINES[2],
                Style::default().fg(theme.highlight),
            )]));

            lines.push(Line::from(vec![Span::styled(
                crate::constants::ASCII_WORDMARK_LINES[3],
                Style::default().fg(theme.success),
            )]));

            lines.push(Line::from(String::new()));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("{} | {}", ctx.provider, ctx.model),
                    Style::default().fg(theme.warning),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(display_path, Style::default().fg(theme.info)),
            ]));

            if let Some(branch) = crate::ui::status::StatusWidgets::get_git_branch(ctx.workspace) {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        format!("git: {}", branch),
                        Style::default().fg(theme.success),
                    ),
                ]));
            }
            lines.push(Line::from(String::new()));
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
                    command_or_path, ..
                } => {
                    lines.push(Line::from(vec![
                        Span::styled("• Running ", Style::default().fg(theme.muted)),
                        Span::styled(command_or_path, Style::default().fg(theme.text_primary)),
                    ]));
                }
                TimelineEntry::ToolApproved {
                    command_or_path, ..
                } => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "✔ ",
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "You approved minicode to run ",
                            Style::default().fg(theme.muted),
                        ),
                        Span::styled(command_or_path, Style::default().fg(theme.text_primary)),
                        Span::styled(" this time", Style::default().fg(theme.muted)),
                    ]));
                }
                TimelineEntry::ToolFinished {
                    command_or_path,
                    output,
                    success,
                    ..
                } => {
                    let verb = if *success { "Ran" } else { "Failed" };
                    let verb_color = if *success {
                        theme.muted
                    } else {
                        theme.destructive
                    };

                    lines.push(Line::from(vec![
                        Span::styled(format!("• {} ", verb), Style::default().fg(verb_color)),
                        Span::styled(command_or_path, Style::default().fg(theme.warning)),
                    ]));

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
                                    Span::styled("  ", Style::default()),
                                    Span::styled(
                                        diff_line,
                                        Style::default()
                                            .fg(theme.muted)
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]));
                            } else if let Some(rest) = diff_line.strip_prefix("+ ") {
                                lines.push(Line::from(vec![
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
                                    Span::styled("  ", Style::default()),
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
                                    lines.push(Line::from(vec![Span::styled(
                                        format!("    ... +{} diff lines (folded)", remaining),
                                        Style::default().fg(theme.border),
                                    )]));
                                }
                                break;
                            }
                        }
                    }

                    // Render regular tool output (summary after diff)
                    let trimmed = regular_output;
                    if trimmed.is_empty() && diff_section.is_empty() {
                        lines.push(Line::from(vec![Span::styled(
                            "  └ (no output)",
                            Style::default().fg(theme.muted),
                        )]));
                    } else if !trimmed.is_empty() {
                        let mut first = true;
                        for out_line in trimmed
                            .lines()
                            .take(crate::constants::UI_MAX_TOOL_OUTPUT_LINES)
                        {
                            let prefix = if first { "  └ " } else { "    " };
                            first = false;

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
                                Span::styled(prefix, Style::default().fg(theme.muted)),
                                Span::styled(out_line, Style::default().fg(line_color)),
                            ]));
                        }
                        if trimmed.lines().count() > crate::constants::UI_MAX_TOOL_OUTPUT_LINES {
                            let remaining = trimmed.lines().count()
                                - crate::constants::UI_MAX_TOOL_OUTPUT_LINES;
                            lines.push(Line::from(vec![Span::styled(
                                format!("    ... +{} lines (output folded)", remaining),
                                Style::default().fg(theme.border),
                            )]));
                        }
                    }
                    lines.push(Line::from(String::new()));
                }
                TimelineEntry::SystemStatus(status) => {
                    lines.push(Line::from(vec![
                        Span::styled("• ", Style::default().fg(theme.brand_accent)),
                        Span::styled(status, Style::default().fg(theme.text_primary)),
                    ]));
                }
            }
        }

        // Live working / thinking status spinner at bottom if running
        if ctx.is_working {
            let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_idx = ((ctx.working_millis / 80) as usize) % spinner_frames.len();
            let spinner = spinner_frames[frame_idx];
            let elapsed_secs = (ctx.working_millis as f64) / 1000.0;

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} Thinking ", spinner),
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({:.1}s • esc to interrupt)", elapsed_secs),
                    Style::default().fg(theme.muted),
                ),
            ]));
        }

        self.selection.timeline_area.set(area);
        self.selection.cache_plain_lines(&lines);
        let lines = self.selection.apply_highlight(lines, theme);

        let total_lines = lines.len() as u16;
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
}
