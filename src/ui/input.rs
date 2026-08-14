use crate::ui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;
use tui_textarea::TextArea;

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

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        // Only process KeyPress / KeyRepeat events (ignore KeyRelease)
        if key.kind == KeyEventKind::Release {
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
}
