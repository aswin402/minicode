//! Minimalist "Gum / GitHub CLI" Progressive Stepper Modal for user inquiries.

use crate::agent::inquiry::{
    InquiryAnswer, InquiryInputType, InquiryQuestion, InquiryRequest, InquiryResponse,
};
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// State for the in-TUI interactive inquiry modal.
#[derive(Debug, Clone)]
pub struct InquiryModalState {
    #[allow(dead_code)]
    pub turn_id: usize,
    pub tool_id: String,
    pub request: InquiryRequest,
    pub current_question_idx: usize,
    pub selected_option_idx: usize,
    pub answers: Vec<InquiryAnswer>,
    pub selected_multi_indices: Vec<usize>,
    pub is_typing_custom: bool,
    pub custom_input_buffer: String,
    pub custom_cursor: usize,
}

impl InquiryModalState {
    pub fn from_request(turn_id: usize, tool_id: &str, request: InquiryRequest) -> Self {
        let is_text_or_secret = request.questions.first().is_some_and(|q| {
            q.input_type == InquiryInputType::Text
                || q.input_type == InquiryInputType::Secret
                || q.options.is_empty()
        });

        Self {
            turn_id,
            tool_id: tool_id.to_string(),
            request,
            current_question_idx: 0,
            selected_option_idx: 0,
            answers: Vec::new(),
            selected_multi_indices: Vec::new(),
            is_typing_custom: is_text_or_secret,
            custom_input_buffer: String::new(),
            custom_cursor: 0,
        }
    }

    pub fn current_question(&self) -> Option<&InquiryQuestion> {
        self.request.questions.get(self.current_question_idx)
    }

    pub fn total_options_count(&self) -> usize {
        if let Some(q) = self.current_question() {
            let mut count = q.options.len();
            if q.allow_custom && q.input_type == InquiryInputType::Choice {
                count += 1; // "Other / Custom write-in"
            }
            count
        } else {
            0
        }
    }

    pub fn next_option(&mut self) {
        let total = self.total_options_count();
        if total > 0 {
            self.selected_option_idx = (self.selected_option_idx + 1) % total;
        }
    }

    pub fn prev_option(&mut self) {
        let total = self.total_options_count();
        if total > 0 {
            if self.selected_option_idx == 0 {
                self.selected_option_idx = total.saturating_sub(1);
            } else {
                self.selected_option_idx -= 1;
            }
        }
    }

    pub fn toggle_multi(&mut self) {
        if let Some(q) = self.current_question() {
            if q.is_multi_select {
                if let Some(pos) = self
                    .selected_multi_indices
                    .iter()
                    .position(|&i| i == self.selected_option_idx)
                {
                    self.selected_multi_indices.remove(pos);
                } else {
                    self.selected_multi_indices.push(self.selected_option_idx);
                }
            }
        }
    }

    pub fn handle_char(&mut self, c: char) {
        if self.custom_cursor > self.custom_input_buffer.len() {
            self.custom_cursor = self.custom_input_buffer.len();
        }
        self.custom_input_buffer.insert(self.custom_cursor, c);
        self.custom_cursor += c.len_utf8();
    }

    pub fn handle_backspace(&mut self) {
        if self.custom_cursor > 0 && self.custom_cursor <= self.custom_input_buffer.len() {
            if let Some((idx, _)) = self.custom_input_buffer[..self.custom_cursor]
                .char_indices()
                .last()
            {
                self.custom_input_buffer.remove(idx);
                self.custom_cursor = idx;
            }
        }
    }

    pub fn cursor_left(&mut self) {
        if self.custom_cursor > 0 && self.custom_cursor <= self.custom_input_buffer.len() {
            if let Some((idx, _)) = self.custom_input_buffer[..self.custom_cursor]
                .char_indices()
                .last()
            {
                self.custom_cursor = idx;
            }
        }
    }

    pub fn cursor_right(&mut self) {
        if self.custom_cursor < self.custom_input_buffer.len() {
            if let Some((idx, c)) = self.custom_input_buffer[self.custom_cursor..]
                .char_indices()
                .next()
            {
                self.custom_cursor += idx + c.len_utf8();
            }
        }
    }

    pub fn confirm_selection(&mut self) -> Option<InquiryResponse> {
        let q = match self.current_question() {
            Some(q) => q.clone(),
            None => return Some(self.build_response(false)),
        };

        let is_custom_option = q.allow_custom && self.selected_option_idx == q.options.len();
        let is_text_or_secret =
            q.input_type == InquiryInputType::Text || q.input_type == InquiryInputType::Secret;

        if (is_custom_option || is_text_or_secret) && !self.is_typing_custom {
            self.is_typing_custom = true;
            return None;
        }

        // Record answer
        let answer = if self.is_typing_custom || is_text_or_secret {
            let val = self.custom_input_buffer.trim().to_string();
            let final_val = if val.is_empty() {
                q.default_value
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string())
            } else {
                val
            };
            InquiryAnswer {
                question_id: q.id.clone(),
                selected_options: vec![],
                custom_text: Some(final_val),
                masked: q.input_type == InquiryInputType::Secret,
            }
        } else if q.is_multi_select {
            let selected: Vec<String> = self
                .selected_multi_indices
                .iter()
                .filter_map(|&idx| q.options.get(idx).map(|o| o.id.clone()))
                .collect();
            InquiryAnswer {
                question_id: q.id.clone(),
                selected_options: selected,
                custom_text: None,
                masked: false,
            }
        } else {
            let selected = q
                .options
                .get(self.selected_option_idx)
                .map(|o| o.id.clone())
                .unwrap_or_else(|| "default".to_string());
            InquiryAnswer {
                question_id: q.id.clone(),
                selected_options: vec![selected],
                custom_text: None,
                masked: false,
            }
        };

        self.answers.push(answer);
        self.current_question_idx += 1;
        self.selected_option_idx = 0;
        self.selected_multi_indices.clear();
        self.custom_input_buffer.clear();
        self.custom_cursor = 0;

        if let Some(next_q) = self.current_question() {
            self.is_typing_custom = next_q.input_type == InquiryInputType::Text
                || next_q.input_type == InquiryInputType::Secret
                || next_q.options.is_empty();
            None
        } else {
            Some(self.build_response(false))
        }
    }

    pub fn cancel(&self) -> InquiryResponse {
        self.build_response(true)
    }

    fn build_response(&self, cancelled: bool) -> InquiryResponse {
        InquiryResponse {
            inquiry_id: self.tool_id.clone(),
            answers: self.answers.clone(),
            cancelled,
        }
    }
}

/// Renders the Option 1 Progressive Stepper modal in Ratatui.
pub fn render_inquiry_modal(
    frame: &mut Frame,
    state: &InquiryModalState,
    theme: &Theme,
    area: Rect,
) {
    let width = 74.min(area.width.saturating_sub(4));
    let height = 24.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(Span::styled(
            format!(" ❓ {} ", state.request.title),
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(block, modal_area);

    let inner = Rect::new(
        modal_area.x + 2,
        modal_area.y + 1,
        modal_area.width.saturating_sub(4),
        modal_area.height.saturating_sub(2),
    );

    let mut lines = Vec::new();

    // Contextual description
    if let Some(desc) = &state.request.description {
        lines.push(Line::from(Span::styled(
            desc,
            Style::default().fg(theme.muted),
        )));
        lines.push(Line::from(""));
    }

    // Collapsed summary of previously answered questions
    for (idx, ans) in state.answers.iter().enumerate() {
        if let Some(q) = state.request.questions.get(idx) {
            let label = q.header.as_deref().unwrap_or(&q.id);
            lines.push(Line::from(vec![
                Span::styled(
                    "✔ ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}: ", label),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(ans.display_value(), Style::default().fg(theme.info)),
            ]));
        }
    }

    if !state.answers.is_empty() {
        lines.push(Line::from(""));
    }

    // Active question
    if let Some(q) = state.current_question() {
        let step_badge = format!(
            "[{}/{}] ",
            state.current_question_idx + 1,
            state.request.questions.len()
        );
        let category = q.header.as_deref().unwrap_or("Question");

        lines.push(Line::from(vec![
            Span::styled(
                step_badge,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}: ", category),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                &q.question,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        // Choice options
        if q.input_type == InquiryInputType::Choice && !q.options.is_empty() {
            for (idx, opt) in q.options.iter().enumerate() {
                let is_selected = idx == state.selected_option_idx;
                let cursor = if is_selected { "❯ " } else { "  " };
                let num_key = if idx < 9 {
                    format!("[{}] ", idx + 1)
                } else {
                    "    ".to_string()
                };

                let check = if q.is_multi_select {
                    if state.selected_multi_indices.contains(&idx) {
                        "[✔] "
                    } else {
                        "[ ] "
                    }
                } else {
                    ""
                };

                let mut spans = vec![
                    Span::styled(
                        cursor,
                        if is_selected {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                    Span::styled(num_key, Style::default().fg(theme.muted)),
                    Span::styled(
                        check,
                        Style::default().fg(if is_selected {
                            theme.brand_accent
                        } else {
                            theme.muted
                        }),
                    ),
                    Span::styled(
                        &opt.label,
                        if is_selected {
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.text_primary)
                        },
                    ),
                ];

                if opt.recommended {
                    spans.push(Span::styled(
                        " (Recommended)",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                if let Some(desc) = &opt.description {
                    spans.push(Span::styled(
                        format!(" — {}", desc),
                        Style::default().fg(theme.muted),
                    ));
                }

                lines.push(Line::from(spans));
            }

            if q.allow_custom {
                let custom_idx = q.options.len();
                let is_selected = custom_idx == state.selected_option_idx;
                let cursor = if is_selected { "❯ " } else { "  " };
                lines.push(Line::from(vec![
                    Span::styled(
                        cursor,
                        if is_selected {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                    Span::styled("[o] ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "Other / Custom write-in...",
                        if is_selected {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.muted)
                        },
                    ),
                ]));
            }
        }

        // In-card input box
        if state.is_typing_custom
            || q.input_type == InquiryInputType::Text
            || q.input_type == InquiryInputType::Secret
        {
            lines.push(Line::from(""));
            let input_title = if q.input_type == InquiryInputType::Secret {
                "Enter Secret / API Key (Masked)"
            } else {
                "Enter Custom Answer"
            };
            let box_width = (inner.width as usize)
                .saturating_sub(2)
                .min(52)
                .max(input_title.len() + 6);
            let top_fill = box_width.saturating_sub(input_title.len() + 5);
            lines.push(Line::from(Span::styled(
                format!("┌─ {} {}┐", input_title, "─".repeat(top_fill)),
                Style::default().fg(theme.info),
            )));

            let display_text = if q.input_type == InquiryInputType::Secret {
                "•".repeat(state.custom_input_buffer.len())
            } else if state.custom_input_buffer.is_empty() {
                q.placeholder
                    .clone()
                    .unwrap_or_else(|| "Type answer here...".to_string())
            } else {
                state.custom_input_buffer.clone()
            };

            let text_style = if state.custom_input_buffer.is_empty()
                && q.input_type != InquiryInputType::Secret
            {
                Style::default().fg(theme.muted)
            } else {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            };

            lines.push(Line::from(vec![
                Span::styled("│ ❯ ", Style::default().fg(theme.brand_accent)),
                Span::styled(display_text, text_style),
                Span::styled(" █", Style::default().fg(theme.highlight)),
            ]));
            lines.push(Line::from(Span::styled(
                format!("└{}┘", "─".repeat(box_width.saturating_sub(2))),
                Style::default().fg(theme.info),
            )));
        }
    }

    // Footer instructions
    let footer_text = if state.is_typing_custom {
        "[Enter] Submit  [Esc] Back to options"
    } else {
        "[↑/↓] Navigate  [1-9] Fast pick  [Space] Toggle  [Enter] Confirm  [Esc] Cancel"
    };

    let p = Paragraph::new(lines);
    frame.render_widget(
        p,
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        ),
    );

    let footer = Paragraph::new(Line::from(Span::styled(
        footer_text,
        Style::default().fg(theme.muted),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(
        footer,
        Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1.min(inner.height),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::inquiry::*;

    fn dummy_request() -> InquiryRequest {
        InquiryRequest {
            inquiry_id: "inq-1".to_string(),
            title: "Website Scaffolding".to_string(),
            description: Some("Choose your preferred stack.".to_string()),
            questions: vec![
                InquiryQuestion {
                    id: "framework".to_string(),
                    question: "Choose UI framework".to_string(),
                    header: Some("Stack".to_string()),
                    input_type: InquiryInputType::Choice,
                    is_multi_select: false,
                    allow_custom: true,
                    placeholder: None,
                    default_value: None,
                    options: vec![
                        InquiryOption {
                            id: "react".to_string(),
                            label: "React 19".to_string(),
                            description: Some("Vite + Tailwind".to_string()),
                            recommended: true,
                        },
                        InquiryOption {
                            id: "vue".to_string(),
                            label: "Vue 3".to_string(),
                            description: Some("Nuxt / Pinia".to_string()),
                            recommended: false,
                        },
                    ],
                },
                InquiryQuestion {
                    id: "api_key".to_string(),
                    question: "Enter Anthropic API Key".to_string(),
                    header: Some("Credentials".to_string()),
                    input_type: InquiryInputType::Secret,
                    is_multi_select: false,
                    allow_custom: false,
                    placeholder: Some("sk-ant-api03-...".to_string()),
                    default_value: None,
                    options: vec![],
                },
            ],
        }
    }

    #[test]
    fn test_modal_stepper_progression() {
        let req = dummy_request();
        let mut state = InquiryModalState::from_request(1, "tool-1", req);

        assert_eq!(state.current_question_idx, 0);
        assert_eq!(state.selected_option_idx, 0);

        // Select first option (React)
        let resp = state.confirm_selection();
        assert!(
            resp.is_none(),
            "Should advance to next question, not finish"
        );
        assert_eq!(state.current_question_idx, 1);
        assert_eq!(state.answers.len(), 1);
        assert_eq!(state.answers[0].selected_options, vec!["react".to_string()]);

        // Second question is Secret: typing custom text
        state.handle_char('s');
        state.handle_char('k');
        state.handle_char('-');
        assert_eq!(state.custom_input_buffer, "sk-");

        let finish = state.confirm_selection();
        assert!(finish.is_some(), "Should complete all questions");
        let final_resp = finish.unwrap();
        assert_eq!(final_resp.answers.len(), 2);
        assert_eq!(final_resp.answers[1].custom_text, Some("sk-".to_string()));
        assert!(final_resp.answers[1].masked);
    }

    #[test]
    fn test_navigation_and_cycling() {
        let req = dummy_request();
        let mut state = InquiryModalState::from_request(1, "tool-nav", req);

        // 2 options + 1 custom write-in = 3 total options
        assert_eq!(state.total_options_count(), 3);
        assert_eq!(state.selected_option_idx, 0);

        state.next_option();
        assert_eq!(state.selected_option_idx, 1);

        state.next_option();
        assert_eq!(state.selected_option_idx, 2);

        // Wrap around
        state.next_option();
        assert_eq!(state.selected_option_idx, 0);

        // Wrap backward
        state.prev_option();
        assert_eq!(state.selected_option_idx, 2);

        state.prev_option();
        assert_eq!(state.selected_option_idx, 1);
    }

    #[test]
    fn test_multiselect_toggle() {
        let multi_q = InquiryQuestion {
            id: "tags".to_string(),
            question: "Select tags".to_string(),
            header: Some("Metadata".to_string()),
            input_type: InquiryInputType::Choice,
            is_multi_select: true,
            allow_custom: false,
            placeholder: None,
            default_value: None,
            options: vec![
                InquiryOption {
                    id: "fast".to_string(),
                    label: "Fast".to_string(),
                    description: None,
                    recommended: false,
                },
                InquiryOption {
                    id: "secure".to_string(),
                    label: "Secure".to_string(),
                    description: None,
                    recommended: true,
                },
            ],
        };

        let req = InquiryRequest {
            inquiry_id: "inq-multi".to_string(),
            title: "Multi Select".to_string(),
            description: None,
            questions: vec![multi_q],
        };

        let mut state = InquiryModalState::from_request(1, "tool-multi", req);
        assert!(state.selected_multi_indices.is_empty());

        // Toggle option 0
        state.toggle_multi();
        assert_eq!(state.selected_multi_indices, vec![0]);

        // Move to option 1 and toggle
        state.next_option();
        state.toggle_multi();
        assert_eq!(state.selected_multi_indices, vec![0, 1]);

        // Toggle option 1 again to deselect
        state.toggle_multi();
        assert_eq!(state.selected_multi_indices, vec![0]);

        let resp = state.confirm_selection();
        assert!(resp.is_some());
        let res = resp.unwrap();
        assert_eq!(res.answers.len(), 1);
        assert_eq!(res.answers[0].selected_options, vec!["fast".to_string()]);
    }

    #[test]
    fn test_backspace_and_custom_writein() {
        let req = dummy_request();
        let mut state = InquiryModalState::from_request(1, "tool-writein", req);

        // Select "Other / Custom write-in" (idx 2)
        state.selected_option_idx = 2;
        let resp = state.confirm_selection();
        assert!(resp.is_none());
        assert!(state.is_typing_custom);

        state.handle_char('H');
        state.handle_char('e');
        state.handle_char('l');
        state.handle_char('l');
        state.handle_char('o');
        assert_eq!(state.custom_input_buffer, "Hello");

        state.handle_backspace();
        assert_eq!(state.custom_input_buffer, "Hell");

        state.handle_backspace();
        assert_eq!(state.custom_input_buffer, "Hel");

        state.handle_char('p');
        assert_eq!(state.custom_input_buffer, "Help");
    }

    #[test]
    fn test_cancel_inquiry() {
        let req = dummy_request();
        let state = InquiryModalState::from_request(1, "tool-cancel", req);
        let resp = state.cancel();
        assert!(resp.cancelled);
        assert_eq!(resp.inquiry_id, "tool-cancel");
    }

    #[test]
    fn test_render_inquiry_modal_without_panic() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::aura_dark();

        let req = dummy_request();
        let state = InquiryModalState::from_request(1, "tool-render", req);

        terminal
            .draw(|f| {
                let area = f.area();
                render_inquiry_modal(f, &state, &theme, area);
            })
            .unwrap();

        // Also test rendering when typing custom text and masked secret
        let mut state_typing = state.clone();
        state_typing.is_typing_custom = true;
        state_typing.custom_input_buffer = "my_custom_value".to_string();

        terminal
            .draw(|f| {
                let area = f.area();
                render_inquiry_modal(f, &state_typing, &theme, area);
            })
            .unwrap();
    }

    #[test]
    fn test_cursor_navigation_in_buffer() {
        let req = dummy_request();
        let mut state = InquiryModalState::from_request(1, "tool-cursor", req);
        state.is_typing_custom = true;
        state.handle_char('a');
        state.handle_char('b');
        state.handle_char('c');
        assert_eq!(state.custom_cursor, 3);

        state.cursor_left();
        assert_eq!(state.custom_cursor, 2);
        state.cursor_left();
        assert_eq!(state.custom_cursor, 1);
        state.cursor_left();
        assert_eq!(state.custom_cursor, 0);
        state.cursor_left(); // bounded
        assert_eq!(state.custom_cursor, 0);

        state.cursor_right();
        assert_eq!(state.custom_cursor, 1);
        state.cursor_right();
        assert_eq!(state.custom_cursor, 2);
        state.cursor_right();
        assert_eq!(state.custom_cursor, 3);
        state.cursor_right(); // bounded
        assert_eq!(state.custom_cursor, 3);
    }
}
