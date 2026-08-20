use crate::ui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use tui_textarea::TextArea;

pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "/model",
        description: "choose what model and reasoning effort to use",
    },
    SlashCommand {
        name: "/provider",
        description: "select or switch active LLM provider",
    },
    SlashCommand {
        name: "/undo",
        description: "revert files modified in the previous turn",
    },
    SlashCommand {
        name: "/retry",
        description: "re-submit the last prompt to the agent",
    },
    SlashCommand {
        name: "/save",
        description: "export conversation history to a file",
    },
    SlashCommand {
        name: "/load",
        description: "load and display past session history",
    },
    SlashCommand {
        name: "/map",
        description: "render AST PageRank repository map",
    },
    SlashCommand {
        name: "/compact",
        description: "manually compact conversation context tokens",
    },
    SlashCommand {
        name: "/tokens",
        description: "display detailed token usage breakdown",
    },
    SlashCommand {
        name: "/terminal",
        description: "toggle interactive embedded terminal drawer (Ctrl+T)",
    },
    SlashCommand {
        name: "/copy",
        description: "copy latest response or entire transcript to clipboard (/copy [all])",
    },
    SlashCommand {
        name: "/clear",
        description: "clear conversation timeline",
    },
    SlashCommand {
        name: "/help",
        description: "display help and keyboard shortcuts",
    },
    SlashCommand {
        name: "/exit",
        description: "quit minicode interactive session",
    },
];

pub struct InputDock<'a> {
    pub textarea: TextArea<'a>,
    pub slash_selected_index: usize,
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
        }
    }

    /// Returns matching slash command candidates if user is typing a slash command
    pub fn matching_slash_commands(&self) -> Vec<&'static SlashCommand> {
        let lines = self.textarea.lines();
        if let Some(first_line) = lines.first() {
            let trimmed = first_line.trim();
            if trimmed.starts_with('/') && !trimmed.contains(' ') {
                return SLASH_COMMANDS
                    .iter()
                    .filter(|cmd| cmd.name.starts_with(trimmed))
                    .collect();
            }
        }
        Vec::new()
    }

    /// Returns the currently selected slash command candidate
    pub fn selected_slash_command(&self) -> Option<&'static SlashCommand> {
        let matches = self.matching_slash_commands();
        if matches.is_empty() {
            None
        } else {
            let idx = self
                .slash_selected_index
                .min(matches.len().saturating_sub(1));
            Some(matches[idx])
        }
    }

    /// Autocompletes active slash command when Tab or Enter is pressed on recommendation
    pub fn autocomplete_slash(&mut self) -> bool {
        if let Some(cmd) = self.selected_slash_command() {
            let mut ta = TextArea::new(vec![cmd.name.to_string()]);
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

        let matching = self.matching_slash_commands();
        let has_recommendations = !matching.is_empty();

        // Handle Up/Down arrow navigation across recommendations
        if has_recommendations {
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

        // Handle Tab for autocomplete
        if key.code == KeyCode::Tab && self.autocomplete_slash() {
            return None;
        }

        match (key.code, key.modifiers) {
            // Submit prompt on Enter (Enter alone or Shift+Enter)
            (KeyCode::Enter, m)
                if !m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::ALT) =>
            {
                let text = self.textarea.lines().join("\n");
                let trimmed = text.trim().to_string();

                // If user typed an exact or prefix slash command with recommendations open,
                // resolve to the highlighted slash command
                let final_prompt =
                    if has_recommendations && trimmed.starts_with('/') && !trimmed.contains(' ') {
                        if let Some(cmd) = self.selected_slash_command() {
                            cmd.name.to_string()
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
                let new_matches = self.matching_slash_commands();
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
        let input_chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([
                ratatui::layout::Constraint::Length(2), // "› "
                ratatui::layout::Constraint::Min(1),    // TextArea input
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

    /// Renders the slash command autocomplete suggestion rows (with Up/Down arrow selection)
    pub fn render_autocomplete_hint(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let matches = self.matching_slash_commands();
        if matches.is_empty() {
            return;
        }

        let max_display = (area.height as usize).min(matches.len());
        let selected_idx = self
            .slash_selected_index
            .min(matches.len().saturating_sub(1));

        let mut lines = Vec::new();
        for (i, cmd) in matches.iter().take(max_display).enumerate() {
            let is_selected = i == selected_idx;
            let prefix = if is_selected { " › " } else { "   " };

            let cmd_style = if is_selected {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.success)
            };

            let desc_style = if is_selected {
                Style::default().fg(theme.text_primary)
            } else {
                Style::default().fg(theme.muted)
            };

            let row_style = if is_selected {
                Style::default().bg(theme.bg_elevated)
            } else {
                Style::default().bg(theme.bg_primary)
            };

            let mut line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.brand_accent)),
                Span::styled(format!("{:<12}", cmd.name), cmd_style),
                Span::styled(cmd.description, desc_style),
            ]);
            line.style = row_style;
            lines.push(line);
        }

        let p = Paragraph::new(lines).style(Style::default().bg(theme.bg_primary));
        frame.render_widget(p, area);
    }
}
