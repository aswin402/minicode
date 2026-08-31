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
            ApprovalOption::Accept => "✔ Approve (Execute action)",
            ApprovalOption::Reject => "✖ Reject (Cancel action)",
            ApprovalOption::AllowSession => "⚡ Always Allow for this Session",
            ApprovalOption::TypeFeedback => "💬 Type Instructions / Custom Feedback",
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
    pub tool_id: String,
    pub tool_name: String,
    pub target_description: String,
    #[allow(dead_code)]
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
        let (target_desc, diff_lines) = match tool_name {
            "exec_cmd" => {
                let cmd = args
                    .get("command")
                    .or_else(|| args.get("cmd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("command");
                let desc = format!("$ {}", cmd);
                (desc, vec![])
            }
            "patch_file" => {
                let path = args
                    .get("path")
                    .or_else(|| args.get("file_path"))
                    .or_else(|| args.get("target_file"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let search = args
                    .get("search_block")
                    .or_else(|| args.get("search"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let replace = args
                    .get("replace_block")
                    .or_else(|| args.get("replace"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let search_lines = search.lines().count();
                let replace_lines = replace.lines().count();
                let desc = format!("{} (+{}/-{} lines)", path, replace_lines, search_lines);
                let diff = DiffViewer::render_patch_args(path, search, replace, theme, 6);
                (desc, diff)
            }
            "write_file" => {
                let path = args
                    .get("path")
                    .or_else(|| args.get("file_path"))
                    .or_else(|| args.get("target_file"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let content = args
                    .get("content")
                    .or_else(|| args.get("code_content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let size_kb = (content.len() as f64) / 1024.0;
                let desc = if size_kb >= 1.0 {
                    format!("{} ({:.1} KB)", path, size_kb)
                } else {
                    format!("{} ({} B)", path, content.len())
                };
                let diff = DiffViewer::render_diff("", content, theme, 6);
                (desc, diff)
            }
            _ => {
                let desc = if let Some(obj) = args.as_object() {
                    if let Some(cmd) = obj
                        .get("command")
                        .or_else(|| obj.get("cmd"))
                        .and_then(|v| v.as_str())
                    {
                        format!("$ {}", cmd)
                    } else if let Some(path) = obj
                        .get("path")
                        .or_else(|| obj.get("file_path"))
                        .and_then(|v| v.as_str())
                    {
                        path.to_string()
                    } else if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
                        url.to_string()
                    } else {
                        tool_name.to_string()
                    }
                } else {
                    tool_name.to_string()
                };
                (desc, vec![])
            }
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

    pub fn handle_char(&mut self, c: char) -> Option<ApprovalResponse> {
        if self.is_typing_feedback {
            self.feedback_input.push(c);
            None
        } else {
            match c {
                'y' | 'Y' => Some(ApprovalResponse::Accept),
                'n' | 'N' => Some(ApprovalResponse::Reject),
                'a' | 'A' => Some(ApprovalResponse::AllowSession),
                'f' | 'F' => {
                    self.select_by_number(4);
                    None
                }
                'j' | 'J' => {
                    self.next_option();
                    None
                }
                'k' | 'K' => {
                    self.prev_option();
                    None
                }
                '1' => {
                    self.select_by_number(1);
                    None
                }
                '2' => {
                    self.select_by_number(2);
                    None
                }
                '3' => {
                    self.select_by_number(3);
                    None
                }
                '4' => {
                    self.select_by_number(4);
                    None
                }
                _ => None,
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
        let width = (area.width.saturating_sub(4)).clamp(48, 62);
        let height = if self.is_typing_feedback { 12 } else { 8 };

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width.min(area.width), height.min(area.height));

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" ⚡ Permission Required ")
            .title_alignment(Alignment::Left)
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.brand_accent))
            .style(Style::default().bg(theme.bg_elevated));

        let inner_area = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let constraints = if self.is_typing_feedback {
            vec![
                Constraint::Length(1), // Header (action/command)
                Constraint::Length(1), // Divider
                Constraint::Length(4), // Options
                Constraint::Length(1), // Spacer
                Constraint::Length(3), // Feedback Box
            ]
        } else {
            vec![
                Constraint::Length(1), // Header (action/command)
                Constraint::Length(1), // Divider
                Constraint::Length(4), // Options
            ]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner_area);

        // 1. Target Banner / Command line (truncated to fit box with ellipsis)
        let max_content_width = (inner_area.width as usize).saturating_sub(12);
        let display_target = if self.target_description.chars().count() > max_content_width {
            let mut s: String = self
                .target_description
                .chars()
                .take(max_content_width.saturating_sub(1))
                .collect();
            s.push('…');
            s
        } else {
            self.target_description.clone()
        };

        let header_p = match self.tool_name.as_str() {
            "exec_cmd" => Paragraph::new(Line::from(vec![
                Span::styled(
                    "  ▶ Run: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    display_target,
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            "patch_file" => Paragraph::new(Line::from(vec![
                Span::styled(
                    "  📝 Patch: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    display_target,
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            "write_file" => Paragraph::new(Line::from(vec![
                Span::styled(
                    "  📄 Write: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    display_target,
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            _ => Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("  🔧 {}: ", self.tool_name),
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    display_target,
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
        };
        frame.render_widget(header_p, chunks[0]);

        // Divider line
        let divider = Paragraph::new(Line::from(vec![Span::styled(
            "─".repeat(inner_area.width as usize),
            Style::default().fg(theme.border),
        )]));
        frame.render_widget(divider, chunks[1]);

        // 2. Selectable Options Menu (Pill-Highlighted)
        let options = ApprovalOption::all();
        let mut option_lines = Vec::new();

        for (idx, opt) in options.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let full_width = inner_area.width as usize;

            if is_selected {
                let text = format!(" ❯ {} {}", opt.key_hint(), opt.label());
                let padded = format!("{:<width$}", text, width = full_width);
                option_lines.push(Line::from(vec![Span::styled(
                    padded,
                    Style::default()
                        .bg(theme.brand_accent)
                        .fg(theme.bg_primary)
                        .add_modifier(Modifier::BOLD),
                )]));
            } else {
                let text = format!("   {} {}", opt.key_hint(), opt.label());
                let padded = format!("{:<width$}", text, width = full_width);
                option_lines.push(Line::from(vec![Span::styled(
                    padded,
                    Style::default().fg(theme.muted),
                )]));
            }
        }

        let options_p = Paragraph::new(option_lines);
        frame.render_widget(options_p, chunks[2]);

        // 3. Custom Feedback Box
        if self.is_typing_feedback && chunks.len() >= 5 {
            let feedback_block = Block::default()
                .title(" Instructions (Enter to submit) ")
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(theme.brand_accent))
                .style(Style::default().bg(theme.bg_primary));
            let feedback_p =
                Paragraph::new(format!("❯ {}█", self.feedback_input)).block(feedback_block);
            frame.render_widget(feedback_p, chunks[4]);
        }
    }
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

    #[test]
    fn test_approval_modal_direct_char_hotkeys() {
        let mut state = ApprovalModalState::new(
            1,
            "call_1".to_string(),
            "exec_cmd".to_string(),
            "$ cargo test".to_string(),
            vec![],
        );

        assert_eq!(state.handle_char('y'), Some(ApprovalResponse::Accept));
        assert_eq!(state.handle_char('n'), Some(ApprovalResponse::Reject));
        assert_eq!(state.handle_char('a'), Some(ApprovalResponse::AllowSession));
    }
}
