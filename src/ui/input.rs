use crate::ui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use tui_textarea::TextArea;

/// Categories for organizing palette commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    All,
    System,
    Intelligence,
    Tools,
}

impl CommandCategory {
    pub fn all() -> &'static [CommandCategory] {
        &[
            CommandCategory::All,
            CommandCategory::System,
            CommandCategory::Intelligence,
            CommandCategory::Tools,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            CommandCategory::All => "All",
            CommandCategory::System => "System",
            CommandCategory::Intelligence => "Intelligence",
            CommandCategory::Tools => "Tools",
        }
    }
}

/// A command item displayed in the floating spotlight palette.
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub slash_name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub category: CommandCategory,
    pub shortcut: Option<&'static str>,
}

pub const PALETTE_COMMANDS: &[PaletteCommand] = &[
    // System Commands
    PaletteCommand {
        slash_name: "/new",
        title: "New Session",
        description: "Start fresh session & reset conversation",
        category: CommandCategory::System,
        shortcut: Some("ctrl+n"),
    },
    PaletteCommand {
        slash_name: "/model",
        title: "Switch Model",
        description: "Choose AI model or reasoning effort",
        category: CommandCategory::System,
        shortcut: Some("ctrl+l"),
    },
    PaletteCommand {
        slash_name: "/configure",
        title: "Configure Providers",
        description: "Interactive API key & endpoint manager",
        category: CommandCategory::System,
        shortcut: Some("F2"),
    },
    PaletteCommand {
        slash_name: "/provider",
        title: "Switch Provider",
        description: "Select active LLM provider (OpenRouter, Groq, Ollama, etc.)",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/theme",
        title: "Theme Selector",
        description: "Switch TUI color theme palette",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/sessions",
        title: "Session History",
        description: "Browse & reload past workspace sessions",
        category: CommandCategory::System,
        shortcut: Some("ctrl+h"),
    },
    PaletteCommand {
        slash_name: "/tokens",
        title: "Token Breakdown",
        description: "Display detailed token usage & context stats",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/clear",
        title: "Clear Timeline",
        description: "Clear active conversation timeline messages",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/help",
        title: "Help & Shortcuts",
        description: "Interactive keyboard shortcuts cheatsheet",
        category: CommandCategory::System,
        shortcut: Some("F1"),
    },
    PaletteCommand {
        slash_name: "/exit",
        title: "Exit minicode",
        description: "Quit minicode interactive session cleanly",
        category: CommandCategory::System,
        shortcut: Some("ctrl+c"),
    },
    // Intelligence / Code Graph Commands
    PaletteCommand {
        slash_name: "/index",
        title: "Repository Index",
        description: "Scan AST symbols & build PageRank code graph",
        category: CommandCategory::Intelligence,
        shortcut: Some("F5"),
    },
    PaletteCommand {
        slash_name: "/review",
        title: "Code Review",
        description: "Run git diff impact analysis & security audit",
        category: CommandCategory::Intelligence,
        shortcut: Some("ctrl+r"),
    },
    PaletteCommand {
        slash_name: "/explore",
        title: "Code Explorer",
        description: "Surgically explore AST symbols, call graph & blast radius",
        category: CommandCategory::Intelligence,
        shortcut: Some("ctrl+e"),
    },
    PaletteCommand {
        slash_name: "/compact",
        title: "Compact Context",
        description: "Manually compact conversation context tokens",
        category: CommandCategory::Intelligence,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/plan",
        title: "Plan Feature",
        description: "Break complex task into verifiable milestones",
        category: CommandCategory::Intelligence,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/goal",
        title: "Autonomous Goal",
        description: "Run self-directed loop until complete",
        category: CommandCategory::Intelligence,
        shortcut: None,
    },
    // Tools Commands
    PaletteCommand {
        slash_name: "/diff",
        title: "Git Diff Viewer",
        description: "Interactive split/unified git diff viewer & staging",
        category: CommandCategory::Tools,
        shortcut: Some("ctrl+d"),
    },
    PaletteCommand {
        slash_name: "/terminal",
        title: "Toggle Terminal",
        description: "Toggle embedded interactive terminal drawer",
        category: CommandCategory::Tools,
        shortcut: Some("ctrl+t"),
    },
    PaletteCommand {
        slash_name: "/stack",
        title: "Scaffold Stack",
        description: "Interactive onpkg multi-runtime stack wizard",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/undo",
        title: "Undo Changes",
        description: "Revert file modifications from previous turn",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/retry",
        title: "Retry Prompt",
        description: "Re-submit previous prompt to agent",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/export",
        title: "Export Session",
        description: "Export conversation trajectory to markdown",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/copy",
        title: "Copy Transcript",
        description: "Copy latest response or whole transcript to clipboard",
        category: CommandCategory::Tools,
        shortcut: None,
    },
];

pub struct InputDock<'a> {
    pub textarea: TextArea<'a>,
    pub slash_selected_index: usize,
    pub category_index: usize,
}

impl<'a> Default for InputDock<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> InputDock<'a> {
    pub fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Ask minicode to do anything...");
        textarea.set_cursor_line_style(Style::default());
        Self {
            textarea,
            slash_selected_index: 0,
            category_index: 0,
        }
    }

    /// Checks if the input starts with '/' and is actively triggering the command palette
    pub fn has_active_slash_query(&self) -> bool {
        let lines = self.textarea.lines();
        if let Some(first_line) = lines.first() {
            let trimmed = first_line.trim();
            trimmed.starts_with('/') && !trimmed.contains(' ')
        } else {
            false
        }
    }

    /// Returns matching slash command candidates filtered by search query and active category
    pub fn matching_palette_commands(&self) -> Vec<&'static PaletteCommand> {
        let lines = self.textarea.lines();
        let query = if let Some(first_line) = lines.first() {
            let trimmed = first_line.trim();
            if let Some(stripped) = trimmed.strip_prefix('/') {
                stripped.to_lowercase()
            } else {
                return Vec::new();
            }
        } else {
            return Vec::new();
        };

        let selected_category =
            CommandCategory::all()[self.category_index % CommandCategory::all().len()];

        PALETTE_COMMANDS
            .iter()
            .filter(|cmd| {
                // Category filter
                let matches_category = match selected_category {
                    CommandCategory::All => true,
                    cat => cmd.category == cat,
                };

                if !matches_category {
                    return false;
                }

                // Query filter
                if query.is_empty() {
                    return true;
                }

                cmd.title.to_lowercase().contains(&query)
                    || cmd
                        .slash_name
                        .trim_start_matches('/')
                        .to_lowercase()
                        .contains(&query)
                    || cmd.description.to_lowercase().contains(&query)
                    || cmd
                        .shortcut
                        .is_some_and(|s| s.to_lowercase().contains(&query))
            })
            .collect()
    }

    /// Returns the currently selected palette command candidate
    pub fn selected_palette_command(&self) -> Option<&'static PaletteCommand> {
        let matches = self.matching_palette_commands();
        if matches.is_empty() {
            None
        } else {
            let idx = self
                .slash_selected_index
                .min(matches.len().saturating_sub(1));
            Some(matches[idx])
        }
    }

    /// Cycles the active category (Tab / BackTab)
    pub fn cycle_category(&mut self, forward: bool) {
        let total = CommandCategory::all().len();
        if forward {
            self.category_index = (self.category_index + 1) % total;
        } else {
            self.category_index = (self.category_index + total - 1) % total;
        }
        self.slash_selected_index = 0;
    }

    /// Autocompletes active slash command when Tab or Enter is pressed on recommendation
    #[allow(dead_code)]
    pub fn autocomplete_slash(&mut self) -> bool {
        if let Some(cmd) = self.selected_palette_command() {
            let mut ta = TextArea::new(vec![cmd.slash_name.to_string()]);
            ta.set_placeholder_text("Ask minicode to do anything...");
            ta.set_cursor_line_style(Style::default());
            self.textarea = ta;
            self.slash_selected_index = 0;
            true
        } else {
            false
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        // Only process KeyPress / KeyRepeat events (ignore KeyRelease)
        if key.kind == KeyEventKind::Release {
            return None;
        }

        let is_slash_open = self.has_active_slash_query();
        let matching = self.matching_palette_commands();

        // Handle Escape to dismiss command palette immediately
        if is_slash_open && key.code == KeyCode::Esc {
            let mut ta = TextArea::default();
            ta.set_placeholder_text("Ask minicode to do anything...");
            ta.set_cursor_line_style(Style::default());
            self.textarea = ta;
            self.slash_selected_index = 0;
            return None;
        }

        // Handle Up/Down arrow navigation across palette commands
        if is_slash_open && !matching.is_empty() {
            if key.code == KeyCode::Up {
                self.slash_selected_index = self.slash_selected_index.saturating_sub(1);
                return None;
            }
            if key.code == KeyCode::Down {
                if self.slash_selected_index + 1 < matching.len() {
                    self.slash_selected_index += 1;
                }
                return None;
            }
        }

        // Handle Tab to cycle categories or autocomplete
        if is_slash_open && key.code == KeyCode::Tab {
            self.cycle_category(true);
            return None;
        }
        if is_slash_open && key.code == KeyCode::BackTab {
            self.cycle_category(false);
            return None;
        }

        match (key.code, key.modifiers) {
            // Submit prompt on Enter (Enter alone or Shift+Enter)
            (KeyCode::Enter, m)
                if !m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::ALT) =>
            {
                let text = self.textarea.lines().join("\n");
                let trimmed = text.trim().to_string();

                // If user typed an exact or prefix slash command with palette open,
                // resolve to the highlighted slash command
                let final_prompt = if is_slash_open && !matching.is_empty() {
                    if let Some(cmd) = self.selected_palette_command() {
                        cmd.slash_name.to_string()
                    } else {
                        trimmed
                    }
                } else {
                    trimmed
                };

                if !final_prompt.is_empty() {
                    let mut ta = TextArea::default();
                    ta.set_placeholder_text("Ask minicode to do anything...");
                    ta.set_cursor_line_style(Style::default());
                    self.textarea = ta;
                    self.slash_selected_index = 0;
                    Some(final_prompt)
                } else {
                    None
                }
            }
            // Insert newline on Ctrl+J
            (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                self.textarea.insert_newline();
                None
            }
            // Insert newline on Alt+Enter or Ctrl+Enter
            (KeyCode::Enter, m)
                if m.contains(KeyModifiers::ALT) || m.contains(KeyModifiers::CONTROL) =>
            {
                self.textarea.insert_newline();
                None
            }
            _ => {
                self.textarea.input(key);
                let new_matches = self.matching_palette_commands();
                if self.slash_selected_index >= new_matches.len() {
                    self.slash_selected_index = new_matches.len().saturating_sub(1);
                }
                None
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.border).bg(theme.bg_input))
            .style(Style::default().bg(theme.bg_input));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Subdivide inner area to render "› " prompt and text editor inline
        let input_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // "› "
                Constraint::Min(1),    // TextArea input
            ])
            .split(inner_area);

        let prompt_span = Span::styled(
            "› ",
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_input)
                .add_modifier(Modifier::BOLD),
        );
        let prompt_widget = Paragraph::new(Line::from(vec![prompt_span]))
            .style(Style::default().bg(theme.bg_input));
        frame.render_widget(prompt_widget, input_chunks[0]);

        let mut cloned = self.textarea.clone();
        cloned.set_style(Style::default().fg(theme.text_primary).bg(theme.bg_input));
        cloned.set_cursor_style(Style::default().fg(theme.bg_primary).bg(theme.brand_accent));
        // Explicitly disable underline on cursor line
        cloned.set_cursor_line_style(Style::default().bg(theme.bg_input));
        cloned.set_placeholder_style(Style::default().fg(theme.muted).bg(theme.bg_input));
        cloned.set_placeholder_text("Ask minicode to do anything...");
        cloned.set_block(
            Block::default()
                .borders(Borders::NONE)
                .style(Style::default().bg(theme.bg_input)),
        );

        frame.render_widget(&cloned, input_chunks[1]);
    }

    /// Renders the Clean Floating Spotlight Command Palette (Option 1)
    pub fn render_slash_palette(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.has_active_slash_query() {
            return;
        }

        let matches = self.matching_palette_commands();

        // Modal dimensions (responsive spotlight centered on screen)
        let width = (area.width * 64 / 100).clamp(62, 78);
        let height = 10_u16;

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 3; // Position in upper-middle
        let popup_area = Rect::new(x, y, width.min(area.width), height.min(area.height));

        frame.render_widget(Clear, popup_area);

        // Build Title Bar with Category Radio Tabs on right
        let current_cat_idx = self.category_index % CommandCategory::all().len();
        let mut title_spans = vec![
            Span::styled(
                " ⌘ Commands ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("[Tab] ", Style::default().fg(theme.muted)),
        ];

        for (idx, cat) in CommandCategory::all().iter().enumerate() {
            let is_active = idx == current_cat_idx;
            if is_active {
                title_spans.push(Span::styled(
                    format!("◉ {} ", cat.label()),
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                title_spans.push(Span::styled(
                    format!("○ {} ", cat.label()),
                    Style::default().fg(theme.muted),
                ));
            }
        }
        title_spans.push(Span::raw(" "));

        let outer_block = Block::default()
            .title(Line::from(title_spans))
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.brand_accent))
            .style(Style::default().bg(theme.bg_elevated));

        let inner_area = outer_block.inner(popup_area);
        frame.render_widget(outer_block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Search row: › /search█
                Constraint::Length(1), // Divider
                Constraint::Min(4),    // Commands list
            ])
            .split(inner_area);

        // 1. Search Query Row
        let typed_text = self.textarea.lines().first().cloned().unwrap_or_default();
        let search_line = Line::from(vec![
            Span::styled(
                "  › ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if typed_text.is_empty() {
                    "/"
                } else {
                    &typed_text
                },
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(theme.brand_accent)),
        ]);
        frame.render_widget(Paragraph::new(search_line), chunks[0]);

        // Top Divider
        let divider = Paragraph::new(Line::from(vec![Span::styled(
            "─".repeat(inner_area.width as usize),
            Style::default().fg(theme.border),
        )]));
        frame.render_widget(divider, chunks[1]);

        // 2. Command Items List
        let list_height = chunks[2].height as usize;
        let selected_idx = self
            .slash_selected_index
            .min(matches.len().saturating_sub(1));

        // Viewport windowing calculation for smooth scrolling
        let scroll_offset = if selected_idx >= list_height {
            selected_idx.saturating_sub(list_height - 1)
        } else {
            0
        };

        let mut item_lines = Vec::new();
        let inner_width = inner_area.width as usize;

        if matches.is_empty() {
            item_lines.push(Line::from(vec![Span::styled(
                "   No matching commands found",
                Style::default().fg(theme.muted),
            )]));
        } else {
            for (i, cmd) in matches
                .iter()
                .skip(scroll_offset)
                .take(list_height)
                .enumerate()
            {
                let actual_idx = scroll_offset + i;
                let is_selected = actual_idx == selected_idx;
                let shortcut_str = cmd.shortcut.unwrap_or("");

                let prefix = if is_selected { " ❯ " } else { "   " };
                let left_content = format!("{}{}", prefix, cmd.title);

                // Right-aligned shortcut badge
                let avail_space =
                    inner_width.saturating_sub(left_content.len() + shortcut_str.len() + 2);
                let padding = " ".repeat(avail_space);

                if is_selected {
                    let line_str = format!("{}{}{}", left_content, padding, shortcut_str);
                    let full_padded = format!("{:<width$}", line_str, width = inner_width);
                    item_lines.push(Line::from(vec![Span::styled(
                        full_padded,
                        Style::default()
                            .bg(theme.brand_accent)
                            .fg(theme.bg_primary)
                            .add_modifier(Modifier::BOLD),
                    )]));
                } else {
                    let mut spans = vec![
                        Span::styled(left_content, Style::default().fg(theme.text_primary)),
                        Span::raw(padding),
                    ];
                    if !shortcut_str.is_empty() {
                        spans.push(Span::styled(shortcut_str, Style::default().fg(theme.muted)));
                    }
                    spans.push(Span::raw(" "));
                    item_lines.push(Line::from(spans));
                }
            }
        }

        let list_p = Paragraph::new(item_lines);
        frame.render_widget(list_p, chunks[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matching_palette_commands_filtering() {
        let mut dock = InputDock::new();
        dock.textarea.insert_str("/mod");
        let matches = dock.matching_palette_commands();
        assert!(matches.iter().any(|c| c.title == "Switch Model"));

        let mut dock2 = InputDock::new();
        dock2.textarea.insert_str("/rev");
        let matches2 = dock2.matching_palette_commands();
        assert!(matches2.iter().any(|c| c.title == "Code Review"));
    }

    #[test]
    fn test_category_cycling() {
        let mut dock = InputDock::new();
        assert_eq!(dock.category_index, 0);
        dock.cycle_category(true);
        assert_eq!(dock.category_index, 1);
        dock.cycle_category(false);
        assert_eq!(dock.category_index, 0);
    }
}
