use crate::ui::diff_viewer::DiffViewer;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// The outcome of an interactive permission approval request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalResponse {
    Accept,
    Reject,
    AllowSession,
    CustomFeedback(String),
}

/// Menu option entries for the approval modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOption {
    Accept,
    Reject,
    AllowSession,
    TypeFeedback,
}

impl ApprovalOption {
    pub fn all() -> &'static [ApprovalOption] {
        &[
            ApprovalOption::Accept,
            ApprovalOption::Reject,
            ApprovalOption::AllowSession,
            ApprovalOption::TypeFeedback,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ApprovalOption::Accept => "Accept & Apply (Execute action)",
            ApprovalOption::Reject => "Reject (Decline this action)",
            ApprovalOption::AllowSession => {
                "Allow for this Session (Auto-approve subsequent turns)"
            }
            ApprovalOption::TypeFeedback => "Type Feedback / Custom Instructions (Guide agent)",
        }
    }

    pub fn key_hint(&self) -> &'static str {
        match self {
            ApprovalOption::Accept => "[1]",
            ApprovalOption::Reject => "[2]",
            ApprovalOption::AllowSession => "[3]",
            ApprovalOption::TypeFeedback => "[4]",
        }
    }
}

/// State tracking an active permission approval modal in the TUI.
#[derive(Debug, Clone)]
pub struct ApprovalModalState {
    #[allow(dead_code)]
    pub turn_id: usize,
    #[allow(dead_code)]
    pub tool_id: String,
    #[allow(dead_code)]
    pub tool_name: String,
    pub target_description: String,
    pub diff_preview: Vec<Line<'static>>,
    pub selected_index: usize,
    pub is_typing_feedback: bool,
    pub feedback_input: String,
}

impl ApprovalModalState {
    pub fn new(
        turn_id: usize,
        tool_id: String,
        tool_name: String,
        target_description: String,
        diff_lines: Vec<Line<'static>>,
    ) -> Self {
        Self {
            turn_id,
            tool_id,
            tool_name,
            target_description,
            diff_preview: diff_lines,
            selected_index: 0,
            is_typing_feedback: false,
            feedback_input: String::new(),
        }
    }

    pub fn from_tool_call(
        turn_id: usize,
        tool_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        theme: &Theme,
    ) -> Self {
        let target_desc = match tool_name {
            "patch_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("Patch file: {}", path)
            }
            "write_file" => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("Write new file: {}", path)
            }
            "exec_cmd" => {
                let cmd = args
                    .get("cmd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("command");
                format!("Execute command: `{}`", cmd)
            }
            _ => format!("Execute tool: {}", tool_name),
        };

        let diff_lines = if tool_name == "patch_file" {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let search = args
                .get("search_block")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let replace = args
                .get("replace_block")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            DiffViewer::render_patch_args(path, search, replace, theme, 12)
        } else if tool_name == "write_file" {
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            DiffViewer::render_diff("", content, theme, 12)
        } else if tool_name == "exec_cmd" {
            let cmd = args.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
            vec![Line::from(vec![
                Span::styled(
                    "Command to run: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(cmd.to_string(), Style::default().fg(theme.warning)),
            ])]
        } else {
            vec![Line::from(vec![Span::styled(
                format!("Arguments: {}", args),
                Style::default().fg(theme.muted),
            )])]
        };

        Self::new(
            turn_id,
            tool_id.to_string(),
            tool_name.to_string(),
            target_desc,
            diff_lines,
        )
    }

    pub fn next_option(&mut self) {
        if !self.is_typing_feedback {
            let count = ApprovalOption::all().len();
            self.selected_index = (self.selected_index + 1) % count;
        }
    }

    pub fn prev_option(&mut self) {
        if !self.is_typing_feedback {
            let count = ApprovalOption::all().len();
            self.selected_index = (self.selected_index + count - 1) % count;
        }
    }

    pub fn select_by_number(&mut self, num: usize) {
        if num >= 1 && num <= ApprovalOption::all().len() {
            self.selected_index = num - 1;
            if self.selected_index == 3 {
                self.is_typing_feedback = true;
            }
        }
    }

    pub fn handle_char(&mut self, c: char) {
        if self.is_typing_feedback {
            self.feedback_input.push(c);
        } else {
            match c {
                'j' | 'J' => self.next_option(),
                'k' | 'K' => self.prev_option(),
                '1' => self.select_by_number(1),
                '2' => self.select_by_number(2),
                '3' => self.select_by_number(3),
                '4' => self.select_by_number(4),
                _ => {}
            }
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.is_typing_feedback {
            self.feedback_input.pop();
        }
    }

    pub fn confirm_selection(&mut self) -> Option<ApprovalResponse> {
        if self.is_typing_feedback {
            let feedback = self.feedback_input.trim().to_string();
            if !feedback.is_empty() {
                Some(ApprovalResponse::CustomFeedback(feedback))
            } else {
                Some(ApprovalResponse::Reject)
            }
        } else {
            match self.selected_index {
                0 => Some(ApprovalResponse::Accept),
                1 => Some(ApprovalResponse::Reject),
                2 => Some(ApprovalResponse::AllowSession),
                3 => {
                    self.is_typing_feedback = true;
                    None // Await user typed input
                }
                _ => Some(ApprovalResponse::Reject),
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let popup_area = centered_rect(75, 70, area);
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" 🛡️ Action Permission Required ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.brand_accent))
            .style(Style::default().bg(theme.bg_elevated));

        let inner_area = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Target description banner
                Constraint::Min(4),    // Diff preview / arguments viewport
                Constraint::Length(7), // Menu options list
                Constraint::Length(3), // Custom feedback input box (if active)
            ])
            .split(inner_area);

        // 1. Target Banner
        let header_p = Paragraph::new(vec![Line::from(vec![
            Span::styled("Action: ", Style::default().fg(theme.muted)),
            Span::styled(
                &self.target_description,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ])]);
        frame.render_widget(header_p, chunks[0]);

        // 2. Diff Preview
        let diff_block = Block::default()
            .title(" Proposed Changes ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .style(Style::default().bg(theme.bg_primary));
        let diff_p = Paragraph::new(self.diff_preview.clone()).block(diff_block);
        frame.render_widget(diff_p, chunks[1]);

        // 3. Selectable Options Menu
        let options = ApprovalOption::all();
        let mut option_lines = Vec::new();

        for (idx, opt) in options.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let cursor = if is_selected { " ❯ " } else { "   " };

            let key_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };

            let label_style = if is_selected {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };

            option_lines.push(Line::from(vec![
                Span::styled(
                    cursor,
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{} ", opt.key_hint()), key_style),
                Span::styled(opt.label(), label_style),
            ]));
        }

        let options_block = Block::default()
            .title(" Select Option (↑/↓ or 1-4, Enter to confirm) ")
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme.border));
        let options_p = Paragraph::new(option_lines).block(options_block);
        frame.render_widget(options_p, chunks[2]);

        // 4. Custom Feedback Box
        if self.is_typing_feedback {
            let feedback_block = Block::default()
                .title(" Type Instructions for Agent & Press Enter ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.success))
                .style(Style::default().bg(theme.bg_primary));
            let feedback_p =
                Paragraph::new(format!("❯ {}█", self.feedback_input)).block(feedback_block);
            frame.render_widget(feedback_p, chunks[3]);
        }
    }
}

/// Helper function to create a centered rect rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let margin_y = 100_u16.saturating_sub(percent_y) / 2;
    let margin_x = 100_u16.saturating_sub(percent_x) / 2;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(margin_y),
            Constraint::Percentage(percent_y.min(100)),
            Constraint::Percentage(margin_y),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(margin_x),
            Constraint::Percentage(percent_x.min(100)),
            Constraint::Percentage(margin_x),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_modal_navigation_and_selection() {
        let mut state = ApprovalModalState::new(
            1,
            "call_1".to_string(),
            "patch_file".to_string(),
            "Patch main.rs".to_string(),
            vec![],
        );

        assert_eq!(state.selected_index, 0);
        state.next_option();
        assert_eq!(state.selected_index, 1);
        state.next_option();
        assert_eq!(state.selected_index, 2);
        state.prev_option();
        assert_eq!(state.selected_index, 1);

        // Select by number
        state.select_by_number(3);
        assert_eq!(state.selected_index, 2);
        assert_eq!(
            state.confirm_selection(),
            Some(ApprovalResponse::AllowSession)
        );
    }

    #[test]
    fn test_approval_modal_custom_feedback_typing() {
        let mut state = ApprovalModalState::new(
            1,
            "call_1".to_string(),
            "exec_cmd".to_string(),
            "Run rm -rf".to_string(),
            vec![],
        );

        state.select_by_number(4);
        assert!(state.is_typing_feedback);

        state.handle_char('D');
        state.handle_char('o');
        state.handle_char('n');
        state.handle_char('t');
        assert_eq!(state.feedback_input, "Dont");

        let response = state.confirm_selection();
        assert_eq!(
            response,
            Some(ApprovalResponse::CustomFeedback("Dont".to_string()))
        );
    }
}
