use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::collections::VecDeque;

/// A bounded scrollable terminal drawer embedded at the bottom of the TUI.
#[derive(Debug)]
pub struct PtyDrawer {
    pub is_open: bool,
    pub input_buffer: String,
    pub history_lines: VecDeque<String>,
    pub max_history: usize,
    #[allow(dead_code)]
    pub scroll_offset: usize,
}

impl Default for PtyDrawer {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyDrawer {
    pub fn new() -> Self {
        let mut history = VecDeque::new();
        history
            .push_back("⚡ Embedded Terminal Drawer initialized. Type commands below.".to_string());
        history.push_back("👉 Press Enter to run, Ctrl+T or Esc to close.".to_string());

        Self {
            is_open: false,
            input_buffer: String::new(),
            history_lines: history,
            max_history: 1000,
            scroll_offset: 0,
        }
    }

    /// Toggles the open/closed state of the terminal drawer.
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    /// Appends output lines into the scrollable history buffer.
    pub fn append_output(&mut self, line: impl Into<String>) {
        if self.history_lines.len() >= self.max_history {
            self.history_lines.pop_front();
        }
        self.history_lines.push_back(line.into());
    }

    /// Clears the terminal output buffer.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.history_lines.clear();
        self.history_lines
            .push_back("⚡ Terminal buffer cleared.".to_string());
    }

    /// Handles a character keystroke into the command line buffer.
    pub fn handle_char(&mut self, c: char) {
        self.input_buffer.push(c);
    }

    /// Handles a backspace keystroke.
    pub fn handle_backspace(&mut self) {
        self.input_buffer.pop();
    }

    /// Submits the current command line and returns the string command if non-empty.
    pub fn submit_command(&mut self) -> Option<String> {
        let trimmed = self.input_buffer.trim().to_string();
        if trimmed.is_empty() {
            return None;
        }

        self.append_output(format!("$ {}", trimmed));
        self.input_buffer.clear();
        Some(trimmed)
    }

    /// Renders the terminal drawer overlaid at the bottom 40% of the screen.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.is_open {
            return;
        }

        // Allocate bottom 40% of viewport
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        let drawer_area = chunks[1];
        frame.render_widget(Clear, drawer_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(97, 255, 202))) // Mint Green
            .title(Span::styled(
                " [ ⚡ Terminal Drawer (Ctrl+T to hide) ] ",
                Style::default()
                    .fg(Color::Rgb(255, 202, 97))
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(drawer_area);
        frame.render_widget(block, drawer_area);

        // Split inner into output history and input row
        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        // 1. Output History Paragraph
        let visible_lines: Vec<Line> = self
            .history_lines
            .iter()
            .map(|l| {
                if l.starts_with('$') {
                    Line::from(vec![
                        Span::styled(
                            "$ ",
                            Style::default()
                                .fg(Color::Rgb(97, 255, 202))
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            l.trim_start_matches("$ "),
                            Style::default().fg(Color::White),
                        ),
                    ])
                } else if l.contains("error") || l.contains("Error") || l.contains("FAIL") {
                    Line::from(Span::styled(
                        l.as_str(),
                        Style::default().fg(Color::Rgb(255, 103, 103)),
                    ))
                } else {
                    Line::from(Span::styled(
                        l.as_str(),
                        Style::default().fg(Color::Rgb(170, 170, 170)),
                    ))
                }
            })
            .collect();

        let num_lines = visible_lines.len();
        let height = inner_chunks[0].height as usize;
        let scroll_y = if num_lines > height {
            (num_lines - height) as u16
        } else {
            0
        };

        let output_paragraph = Paragraph::new(visible_lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll_y, 0));

        frame.render_widget(output_paragraph, inner_chunks[0]);

        // 2. Input Prompt Row
        let prompt_line = Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default()
                    .fg(Color::Rgb(130, 226, 255))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(&self.input_buffer, Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::Rgb(97, 255, 202))),
        ]);

        frame.render_widget(Paragraph::new(prompt_line), inner_chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_drawer_toggle_and_input() {
        let mut drawer = PtyDrawer::new();
        assert!(!drawer.is_open);

        drawer.toggle();
        assert!(drawer.is_open);

        drawer.handle_char('l');
        drawer.handle_char('s');
        drawer.handle_char(' ');
        drawer.handle_char('-');
        drawer.handle_char('l');
        drawer.handle_char('a');
        assert_eq!(drawer.input_buffer, "ls -la");

        drawer.handle_backspace();
        assert_eq!(drawer.input_buffer, "ls -l");

        let cmd = drawer.submit_command();
        assert_eq!(cmd, Some("ls -l".to_string()));
        assert_eq!(drawer.input_buffer, "");
        assert!(drawer.history_lines.back().unwrap().contains("$ ls -l"));
    }
}
