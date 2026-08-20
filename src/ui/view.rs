use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Debug, Clone)]
pub enum TimelineEntry {
    UserPrompt(String),
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
}

pub struct TimelineContext<'a> {
    pub theme: &'a Theme,
    pub is_working: bool,
    pub working_secs: u64,
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
    }

    pub fn append_assistant_delta(&mut self, delta: &str) {
        if let Some(TimelineEntry::AssistantMarkdown(ref mut text)) = self.entries.last_mut() {
            text.push_str(delta);
        } else {
            self.entries
                .push(TimelineEntry::AssistantMarkdown(delta.to_string()));
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

                    let trimmed = output.trim();
                    if trimmed.is_empty() {
                        lines.push(Line::from(vec![Span::styled(
                            "  └ (no output)",
                            Style::default().fg(theme.muted),
                        )]));
                    } else {
                        let mut first = true;
                        for out_line in trimmed
                            .lines()
                            .take(crate::constants::UI_MAX_TOOL_OUTPUT_LINES)
                        {
                            let prefix = if first { "  └ " } else { "    " };
                            first = false;

                            // Colorize diff lines or test outputs
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

        // Live working status spinner at bottom if running
        if ctx.is_working {
            lines.push(Line::from(vec![
                Span::styled(
                    "• Working ",
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({}s • esc to interrupt)", ctx.working_secs),
                    Style::default().fg(theme.muted),
                ),
            ]));
        }

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
        let mut lines = Vec::new();

        for raw_line in text.lines() {
            let line = raw_line.trim_end();

            if let Some(rest) = line.strip_prefix("### ") {
                lines.push(Line::from(vec![
                    Span::styled(
                        "### ",
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        rest,
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            } else if let Some(rest) = line.strip_prefix("## ") {
                lines.push(Line::from(vec![
                    Span::styled(
                        "## ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        rest,
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            } else if let Some(rest) = line.strip_prefix("# ") {
                lines.push(Line::from(vec![
                    Span::styled(
                        "# ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        rest,
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            } else if let Some(rest) = line.strip_prefix("• ") {
                lines.push(Line::from(vec![
                    Span::styled("• ", Style::default().fg(theme.brand_accent)),
                    Span::styled(rest, Style::default().fg(theme.text_primary)),
                ]));
            } else if let Some(rest) = line.strip_prefix("* ") {
                lines.push(Line::from(vec![
                    Span::styled("• ", Style::default().fg(theme.brand_accent)),
                    Span::styled(rest, Style::default().fg(theme.text_primary)),
                ]));
            } else if let Some(rest) = line.strip_prefix("- ") {
                lines.push(Line::from(vec![
                    Span::styled("• ", Style::default().fg(theme.brand_accent)),
                    Span::styled(rest, Style::default().fg(theme.text_primary)),
                ]));
            } else if let Some(rest) = line.strip_prefix("  - ") {
                lines.push(Line::from(vec![
                    Span::styled("    - ", Style::default().fg(theme.muted)),
                    Span::styled(rest, Style::default().fg(theme.text_primary)),
                ]));
            } else if let Some(rest) = line.strip_prefix("  * ") {
                lines.push(Line::from(vec![
                    Span::styled("    - ", Style::default().fg(theme.muted)),
                    Span::styled(rest, Style::default().fg(theme.text_primary)),
                ]));
            } else if let Some(rest) = line.strip_prefix("  • ") {
                lines.push(Line::from(vec![
                    Span::styled("    - ", Style::default().fg(theme.muted)),
                    Span::styled(rest, Style::default().fg(theme.text_primary)),
                ]));
            } else if line.starts_with("```") {
                lines.push(Line::from(vec![Span::styled(
                    line,
                    Style::default().fg(theme.muted),
                )]));
            } else if line.is_empty() {
                lines.push(Line::from(String::new()));
            } else {
                lines.push(Line::from(vec![Span::styled(
                    line,
                    Style::default().fg(theme.text_primary),
                )]));
            }
        }

        lines
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
}
