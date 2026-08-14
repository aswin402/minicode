use crate::ui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
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
}

impl<'a> InputDock<'a> {
    pub fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Implement {feature} or ask a question...");
        textarea.set_cursor_line_style(Style::default());
        Self { textarea }
    }

    /// Returns matching slash command candidates if user is typing a slash command
    pub fn matching_slash_command(&self) -> Option<&'static SlashCommand> {
        let lines = self.textarea.lines();
        if let Some(first_line) = lines.first() {
            let trimmed = first_line.trim();
            if trimmed.starts_with('/') && !trimmed.contains(' ') {
                for cmd in SLASH_COMMANDS {
                    if cmd.name.starts_with(trimmed) {
                        return Some(cmd);
                    }
                }
            }
        }
        None
    }

    /// Autocompletes active slash command when Tab is pressed
    pub fn autocomplete_slash(&mut self) -> bool {
        if let Some(cmd) = self.matching_slash_command() {
            self.textarea = TextArea::new(vec![cmd.name.to_string()]);
            self.textarea
                .set_placeholder_text("Implement {feature} or ask a question...");
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
                if !trimmed.is_empty() {
                    self.textarea = TextArea::default();
                    self.textarea
                        .set_placeholder_text("Implement {feature} or ask a question...");
                    Some(trimmed)
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
                None
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut cloned = self.textarea.clone();
        cloned.set_style(Style::default().fg(theme.text_primary).bg(theme.bg_input));
        cloned.set_cursor_style(
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.brand_accent),
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border).bg(theme.bg_input))
            .style(Style::default().bg(theme.bg_input))
            .title(" › ");

        cloned.set_block(block);
        frame.render_widget(&cloned, area);
    }

    /// Renders the slash command autocomplete suggestion line (matching screenshot)
    pub fn render_autocomplete_hint(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if let Some(cmd) = self.matching_slash_command() {
            let line = Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    cmd.name,
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(cmd.description, Style::default().fg(theme.muted)),
            ]);

            let p = Paragraph::new(line).style(Style::default().bg(theme.bg_primary));
            frame.render_widget(p, area);
        }
    }
}
