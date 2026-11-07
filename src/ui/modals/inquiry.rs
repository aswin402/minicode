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

use unicode_width::UnicodeWidthStr;

fn requires_text_input(q: &InquiryQuestion) -> bool {
    match q.input_type {
        InquiryInputType::Text | InquiryInputType::Secret => true,
        InquiryInputType::Slider => false,
        InquiryInputType::Choice | InquiryInputType::Confirm | InquiryInputType::Rank => {
            q.options.is_empty()
        }
    }
}

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
    pub answers_by_idx: Vec<Option<InquiryAnswer>>,
    pub rank_orders: Vec<Vec<usize>>,
    pub slider_values: Vec<i64>,
    pub selected_multi_indices: Vec<usize>,
    pub is_typing_custom: bool,
    pub custom_input_buffer: String,
    pub custom_cursor: usize,
}

impl InquiryModalState {
    pub fn from_request(turn_id: usize, tool_id: &str, request: InquiryRequest) -> Self {
        let is_text_or_secret = request.questions.first().is_some_and(requires_text_input);
        let q_len = request.questions.len();
        let rank_orders = request
            .questions
            .iter()
            .map(|q| (0..q.options.len()).collect())
            .collect();
        let slider_values = request
            .questions
            .iter()
            .map(InquiryQuestion::initial_slider_value)
            .collect();

        Self {
            turn_id,
            tool_id: tool_id.to_string(),
            request,
            current_question_idx: 0,
            selected_option_idx: 0,
            answers: Vec::new(),
            answers_by_idx: vec![None; q_len],
            rank_orders,
            slider_values,
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
            if q.input_type == InquiryInputType::Slider {
                return 0;
            }
            let mut count = q.options.len();
            if q.allow_custom
                && matches!(
                    q.input_type,
                    InquiryInputType::Choice | InquiryInputType::Confirm
                )
            {
                count += 1; // "Other / Custom write-in"
            }
            count
        } else {
            0
        }
    }

    /// Switches to another question tab in non-linear order, restoring any draft/recorded answer.
    pub fn switch_to_question(&mut self, idx: usize) {
        if idx >= self.request.questions.len() {
            return;
        }
        self.current_question_idx = idx;
        self.selected_option_idx = 0;
        self.selected_multi_indices.clear();
        self.custom_input_buffer.clear();
        self.custom_cursor = 0;

        let q_opt = self.request.questions.get(idx).cloned();
        if let Some(q) = q_opt {
            self.is_typing_custom = requires_text_input(&q);
            if let Some(Some(prev)) = self.answers_by_idx.get(idx).cloned() {
                if let Some(custom) = prev.custom_text {
                    self.custom_input_buffer = custom;
                    self.custom_cursor = self.custom_input_buffer.len();
                    if q.allow_custom
                        && matches!(
                            q.input_type,
                            InquiryInputType::Choice | InquiryInputType::Confirm
                        )
                    {
                        self.selected_option_idx = q.options.len();
                    }
                } else if q.is_multi_select {
                    for (opt_i, opt) in q.options.iter().enumerate() {
                        if prev.selected_options.iter().any(|s| s == opt.resolved_id()) {
                            self.selected_multi_indices.push(opt_i);
                        }
                    }
                } else if matches!(
                    q.input_type,
                    InquiryInputType::Choice | InquiryInputType::Confirm
                ) {
                    if let Some(first_sel) = prev.selected_options.first() {
                        if let Some(pos) =
                            q.options.iter().position(|o| o.resolved_id() == first_sel)
                        {
                            self.selected_option_idx = pos;
                        }
                    }
                }
            }
        }
    }

    /// Jump to the next question tab (wrapping around).
    pub fn next_question_tab(&mut self) {
        let len = self.request.questions.len();
        if len > 1 {
            let next = (self.current_question_idx + 1) % len;
            self.switch_to_question(next);
        }
    }

    /// Jump to the previous question tab (wrapping around).
    pub fn prev_question_tab(&mut self) {
        let len = self.request.questions.len();
        if len > 1 {
            let prev = if self.current_question_idx == 0 {
                len - 1
            } else {
                self.current_question_idx - 1
            };
            self.switch_to_question(prev);
        }
    }

    /// Returns the current ordered option indices for a `Rank` question.
    pub fn current_rank_order(&self) -> &[usize] {
        self.rank_orders
            .get(self.current_question_idx)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Moves the currently highlighted option higher in priority (`Rank` mode).
    pub fn move_rank_up(&mut self) {
        if let Some(order) = self.rank_orders.get_mut(self.current_question_idx) {
            if self.selected_option_idx > 0 && self.selected_option_idx < order.len() {
                order.swap(self.selected_option_idx, self.selected_option_idx - 1);
                self.selected_option_idx -= 1;
            }
        }
    }

    /// Moves the currently highlighted option lower in priority (`Rank` mode).
    pub fn move_rank_down(&mut self) {
        if let Some(order) = self.rank_orders.get_mut(self.current_question_idx) {
            if self.selected_option_idx + 1 < order.len() {
                order.swap(self.selected_option_idx, self.selected_option_idx + 1);
                self.selected_option_idx += 1;
            }
        }
    }

    /// Returns the current slider numeric value for the active question.
    pub fn current_slider_value(&self) -> i64 {
        self.slider_values
            .get(self.current_question_idx)
            .copied()
            .or_else(|| {
                self.current_question()
                    .map(InquiryQuestion::initial_slider_value)
            })
            .unwrap_or(5)
    }

    /// Decrements the slider value by `step` within `[min, max]`.
    pub fn slider_decrement(&mut self) {
        let Some(bounds) = self.current_question().map(InquiryQuestion::slider_bounds) else {
            return;
        };
        let (min, max, step) = bounds;
        if let Some(val) = self.slider_values.get_mut(self.current_question_idx) {
            *val = (*val - step).clamp(min, max);
        }
    }

    /// Increments the slider value by `step` within `[min, max]`.
    pub fn slider_increment(&mut self) {
        let Some(bounds) = self.current_question().map(InquiryQuestion::slider_bounds) else {
            return;
        };
        let (min, max, step) = bounds;
        if let Some(val) = self.slider_values.get_mut(self.current_question_idx) {
            *val = (*val + step).clamp(min, max);
        }
    }

    /// Sets the slider value directly (clamped to `[min, max]`).
    pub fn set_slider_value(&mut self, new_val: i64) {
        let Some(bounds) = self.current_question().map(InquiryQuestion::slider_bounds) else {
            return;
        };
        let (min, max, _) = bounds;
        if let Some(val) = self.slider_values.get_mut(self.current_question_idx) {
            *val = new_val.clamp(min, max);
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
        let prev = self.custom_cursor;
        self.cursor_left();
        if self.custom_cursor < prev {
            self.custom_input_buffer.remove(self.custom_cursor);
        }
    }

    pub fn cursor_left(&mut self) {
        if self.custom_cursor > 0 && self.custom_cursor <= self.custom_input_buffer.len() {
            if let Some((idx, _)) = self.custom_input_buffer[..self.custom_cursor]
                .char_indices()
                .next_back()
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

    pub fn handle_delete(&mut self) {
        if self.custom_cursor < self.custom_input_buffer.len() {
            if let Some((_, c)) = self.custom_input_buffer[self.custom_cursor..]
                .char_indices()
                .next()
            {
                self.custom_input_buffer
                    .drain(self.custom_cursor..self.custom_cursor + c.len_utf8());
            }
        }
    }

    pub fn cursor_home(&mut self) {
        self.custom_cursor = 0;
    }

    pub fn cursor_end(&mut self) {
        self.custom_cursor = self.custom_input_buffer.len();
    }

    pub fn clear_line(&mut self) {
        self.custom_input_buffer.clear();
        self.custom_cursor = 0;
    }

    pub fn kill_to_end(&mut self) {
        if self.custom_cursor < self.custom_input_buffer.len() {
            self.custom_input_buffer.truncate(self.custom_cursor);
        }
    }

    pub fn confirm_selection(&mut self) -> Option<InquiryResponse> {
        let q = match self.current_question() {
            Some(q) => q.clone(),
            None => return Some(self.build_response(false)),
        };

        let is_custom_option = q.allow_custom
            && matches!(
                q.input_type,
                InquiryInputType::Choice | InquiryInputType::Confirm
            )
            && self.selected_option_idx == q.options.len();
        let is_text_or_secret = requires_text_input(&q);

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
                question_id: q.resolved_id().to_string(),
                selected_options: vec![],
                custom_text: Some(final_val),
                masked: q.input_type == InquiryInputType::Secret,
            }
        } else if q.input_type == InquiryInputType::Slider {
            let val = self.current_slider_value();
            InquiryAnswer {
                question_id: q.resolved_id().to_string(),
                selected_options: vec![val.to_string()],
                custom_text: None,
                masked: false,
            }
        } else if q.input_type == InquiryInputType::Rank {
            let ordered: Vec<String> = self
                .current_rank_order()
                .iter()
                .filter_map(|&opt_idx| q.options.get(opt_idx).map(|o| o.resolved_id().to_string()))
                .collect();
            InquiryAnswer {
                question_id: q.resolved_id().to_string(),
                selected_options: ordered,
                custom_text: None,
                masked: false,
            }
        } else if q.is_multi_select {
            let mut selected: Vec<String> = self
                .selected_multi_indices
                .iter()
                .filter_map(|&idx| q.options.get(idx).map(|o| o.resolved_id().to_string()))
                .collect();
            // If user did not toggle any boxes with Space, use currently highlighted option as fallback
            if selected.is_empty() {
                if let Some(opt) = q.options.get(self.selected_option_idx) {
                    selected.push(opt.resolved_id().to_string());
                }
            }
            InquiryAnswer {
                question_id: q.resolved_id().to_string(),
                selected_options: selected,
                custom_text: None,
                masked: false,
            }
        } else {
            let selected = q
                .options
                .get(self.selected_option_idx)
                .map(|o| o.resolved_id().to_string())
                .unwrap_or_else(|| "default".to_string());
            InquiryAnswer {
                question_id: q.resolved_id().to_string(),
                selected_options: vec![selected],
                custom_text: None,
                masked: false,
            }
        };

        if let Some(slot) = self.answers_by_idx.get_mut(self.current_question_idx) {
            *slot = Some(answer);
        }
        self.answers = self
            .answers_by_idx
            .iter()
            .filter_map(|a| a.clone())
            .collect();

        let total_q = self.request.questions.len();
        let next_unanswered = ((self.current_question_idx + 1)..total_q)
            .chain(0..self.current_question_idx)
            .find(|&idx| self.answers_by_idx.get(idx).is_some_and(Option::is_none));

        if let Some(next_idx) = next_unanswered {
            self.switch_to_question(next_idx);
            None
        } else {
            self.current_question_idx = total_q;
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

/// Wraps styled spans into visual lines where each returned `Line` is guaranteed to have
/// a display width `<= max_width` (1 `Line` == 1 terminal row). Continuation lines are
/// prefixed with `hanging_indent`.
fn wrap_styled_spans(
    spans: Vec<(String, Style)>,
    max_width: usize,
    hanging_indent: &str,
) -> Vec<Line<'static>> {
    let max_width = max_width.max(1);
    let indent_width = UnicodeWidthStr::width(hanging_indent).min(max_width.saturating_sub(1));
    let effective_indent: String = if indent_width > 0 {
        hanging_indent
            .chars()
            .take_while(|_| true)
            .collect::<String>()
    } else {
        String::new()
    };

    let mut out: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width: usize = 0;
    let mut is_continuation = false;

    let flush_line = |out: &mut Vec<Line<'static>>,
                      current_spans: &mut Vec<Span<'static>>,
                      current_width: &mut usize,
                      is_continuation: &mut bool| {
        // Trim trailing whitespace from the last span on the line
        while let Some(last) = current_spans.last_mut() {
            let trimmed = last.content.trim_end_matches(' ').to_string();
            if trimmed.is_empty() {
                current_spans.pop();
            } else {
                *last = Span::styled(trimmed, last.style);
                break;
            }
        }
        out.push(Line::from(std::mem::take(current_spans)));
        *is_continuation = true;
        if indent_width > 0 {
            current_spans.push(Span::raw(effective_indent.clone()));
            *current_width = indent_width;
        } else {
            *current_width = 0;
        }
    };

    for (text, style) in spans {
        for (line_idx, raw_segment) in text.split('\n').enumerate() {
            if line_idx > 0 {
                flush_line(
                    &mut out,
                    &mut current_spans,
                    &mut current_width,
                    &mut is_continuation,
                );
            }

            // Tokenize into alternating runs of whitespace and non-whitespace
            let mut chars = raw_segment.chars().peekable();
            while let Some(&ch) = chars.peek() {
                let is_space = ch.is_whitespace();
                let mut token = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() == is_space {
                        token.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }

                if is_space {
                    let min_prefix = if is_continuation { indent_width } else { 0 };
                    if is_continuation && current_width <= min_prefix {
                        continue;
                    }
                    let space_w = UnicodeWidthStr::width(token.as_str());
                    if current_width + space_w <= max_width {
                        current_spans.push(Span::styled(token, style));
                        current_width += space_w;
                    } else {
                        flush_line(
                            &mut out,
                            &mut current_spans,
                            &mut current_width,
                            &mut is_continuation,
                        );
                    }
                } else {
                    let word_w = UnicodeWidthStr::width(token.as_str());
                    let min_prefix = if is_continuation { indent_width } else { 0 };
                    if current_width + word_w > max_width && current_width > min_prefix {
                        flush_line(
                            &mut out,
                            &mut current_spans,
                            &mut current_width,
                            &mut is_continuation,
                        );
                    }

                    if current_width + word_w <= max_width {
                        current_spans.push(Span::styled(token, style));
                        current_width += word_w;
                    } else {
                        // Word itself is wider than remaining line budget; split by character width
                        let mut chunk = String::new();
                        let mut chunk_w = 0;
                        for c in token.chars() {
                            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                            if current_width + chunk_w + cw > max_width
                                && (current_width + chunk_w > 0)
                            {
                                if !chunk.is_empty() {
                                    current_spans
                                        .push(Span::styled(std::mem::take(&mut chunk), style));
                                    current_width += chunk_w;
                                    chunk_w = 0;
                                }
                                flush_line(
                                    &mut out,
                                    &mut current_spans,
                                    &mut current_width,
                                    &mut is_continuation,
                                );
                            }
                            chunk.push(c);
                            chunk_w += cw;
                        }
                        if !chunk.is_empty() {
                            current_spans.push(Span::styled(chunk, style));
                            current_width += chunk_w;
                        }
                    }
                }
            }
        }
    }

    if !current_spans.is_empty() || out.is_empty() {
        out.push(Line::from(current_spans));
    }

    out
}

/// Builds a width-responsive footer keybind string that never clips `[Enter] Confirm`.
fn build_inquiry_footer_text(state: &InquiryModalState, max_width: usize) -> &'static str {
    let has_multi_q = state.request.questions.len() > 1;
    let candidates: &[&str] = if state.is_typing_custom {
        &[
            "[Enter] Submit  [Esc] Back  [Home/End] Jump  [Ctrl+U] Clear",
            "[Enter] Submit  [Esc] Back  [Ctrl+U] Clear",
            "[Enter] Submit  [Esc] Back",
        ]
    } else {
        match state.current_question().map(|q| q.input_type) {
            Some(InquiryInputType::Slider) => {
                if has_multi_q {
                    &[
                        "[←/→/h/l] Adjust  [1-9] Set  [Tab] Next Q  [Enter] Confirm  [Esc] Cancel",
                        "[←/→] Adjust  [1-9] Set  [Tab] Next Q  [Enter] Confirm",
                        "[←/→] Adjust  [Tab] Next  [Enter] Confirm",
                    ]
                } else {
                    &[
                        "[←/→/h/l] Adjust  [1-9] Set  [Enter] Confirm  [Esc] Cancel",
                        "[←/→] Adjust  [1-9] Set  [Enter] Confirm",
                        "[←/→] Adjust  [Enter] Confirm",
                    ]
                }
            }
            Some(InquiryInputType::Rank) => {
                if has_multi_q {
                    &[
                        "[↑/↓/j/k] Select  [Shift+↑/↓ or K/J] Reorder  [Tab] Next Q  [Enter] Confirm",
                        "[↑/↓] Select  [Shift+↑/↓] Reorder  [Tab] Next Q  [Enter] Confirm",
                        "[↑/↓] Select  [K/J] Reorder  [Enter] Confirm",
                    ]
                } else {
                    &[
                        "[↑/↓/j/k] Select  [Shift+↑/↓ or K/J] Reorder  [Enter] Confirm  [Esc] Cancel",
                        "[↑/↓] Select  [Shift+↑/↓] Reorder  [Enter] Confirm",
                        "[↑/↓] Select  [K/J] Reorder  [Enter] Confirm",
                    ]
                }
            }
            Some(InquiryInputType::Confirm) => {
                if has_multi_q {
                    &[
                        "[↑/↓] Navigate  [y/n] Yes/No  [w] Write-in  [Tab] Next Q  [Enter] Confirm",
                        "[↑/↓] Move  [y/n] Yes/No  [w] Custom  [Tab] Next  [Enter] Confirm",
                        "[y/n] Yes/No  [w] Custom  [Tab] Next  [Enter] Confirm",
                    ]
                } else {
                    &[
                        "[↑/↓] Navigate  [y/n] Yes/No  [w] Write-in  [Enter] Confirm  [Esc] Cancel",
                        "[↑/↓] Move  [y/n] Yes/No  [w] Write-in  [Enter] Confirm",
                        "[y/n] Yes/No  [w] Custom  [Enter] Confirm",
                    ]
                }
            }
            _ => {
                if has_multi_q {
                    &[
                        "[↑/↓/j/k] Navigate  [1-9] Pick  [w] Write-in  [Tab] Next Q  [Enter] Confirm",
                        "[↑/↓] Move  [1-9] Pick  [w] Write-in  [Tab] Next Q  [Enter] Confirm",
                        "[↑/↓] Move  [1-9] Pick  [w] Custom  [Tab] Next  [Enter] Confirm",
                        "[↑/↓] Move  [1-9] Pick  [w] Custom  [Enter] Confirm",
                    ]
                } else {
                    &[
                        "[↑/↓/j/k] Navigate  [1-9] Pick  [w] Write-in  [Enter] Confirm  [Esc] Cancel",
                        "[↑/↓] Navigate  [1-9] Pick  [w] Write-in  [Enter] Confirm",
                        "[↑/↓] Move  [1-9] Pick  [w] Custom  [Enter] Confirm",
                    ]
                }
            }
        }
    };

    candidates
        .iter()
        .copied()
        .find(|s| UnicodeWidthStr::width(*s) <= max_width)
        .unwrap_or_else(|| candidates.last().copied().unwrap_or("[Enter] Confirm"))
}

/// Renders the Option 1 Progressive Stepper modal in Ratatui.
pub fn render_inquiry_modal(
    frame: &mut Frame,
    state: &InquiryModalState,
    theme: &Theme,
    area: Rect,
) {
    if area.width < 10 || area.height < 6 {
        return;
    }

    // Responsive width: expand up to 96 columns on wider terminals, fit gracefully on narrow ones.
    let max_modal_w = area.width.saturating_sub(2).max(1);
    let preferred_w = area.width.saturating_sub(4).clamp(60, 96);
    let width = preferred_w
        .min(max_modal_w)
        .max(area.width.min(48).min(max_modal_w));
    let inner_width = width.saturating_sub(4).max(1) as usize;
    // Reserve 2 columns on the right of the options body for the scrollbar track when needed.
    let body_wrap_width = inner_width.saturating_sub(2).max(1);

    let mut header_lines: Vec<Line<'static>> = Vec::new();

    // Contextual description
    if let Some(desc) = &state.request.description {
        let wrapped_desc = wrap_styled_spans(
            vec![(desc.clone(), Style::default().fg(theme.muted))],
            inner_width,
            "",
        );
        header_lines.extend(wrapped_desc);
        header_lines.push(Line::from(""));
    }

    // Non-linear question tab bar when multiple questions exist ("See all questions at once / Answer in any order")
    if state.request.questions.len() > 1 {
        let mut tab_spans: Vec<(String, Style)> = Vec::new();
        for (idx, q) in state.request.questions.iter().enumerate() {
            let short_label = q
                .header
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| q.resolved_id());
            let is_active = idx == state.current_question_idx;
            let answered = state.answers_by_idx.get(idx).and_then(|a| a.as_ref());

            let (icon, style) = if is_active {
                (
                    "●",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                )
            } else if answered.is_some() {
                (
                    "✓",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("○", Style::default().fg(theme.muted))
            };

            let tab_text = if let Some(ans) = answered {
                let val = ans.display_value();
                let preview: String = val.chars().take(12).collect();
                format!(" [{} {}. {}: {}] ", icon, idx + 1, short_label, preview)
            } else {
                format!(" [{} {}. {}] ", icon, idx + 1, short_label)
            };
            tab_spans.push((tab_text, style));
        }
        let wrapped_tabs = wrap_styled_spans(tab_spans, inner_width, " ");
        header_lines.extend(wrapped_tabs);
        header_lines.push(Line::from(""));
    }

    let mut body_lines: Vec<Line<'static>> = Vec::new();
    let mut selected_start_row: usize = 0;
    let mut selected_end_row: usize = 1;

    // Active question
    if let Some(q) = state.current_question() {
        let step_badge = format!(
            "[{}/{}] ",
            state.current_question_idx + 1,
            state.request.questions.len()
        );
        let category = q.header.as_deref().unwrap_or("Question");

        let wrapped_question = wrap_styled_spans(
            vec![
                (
                    step_badge,
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                (
                    format!("{}: ", category),
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                (
                    q.question.clone(),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ],
            inner_width,
            "",
        );
        header_lines.extend(wrapped_question);
        header_lines.push(Line::from(""));

        // Choice / Confirm options (with Pros/Cons trade-off cards)
        if matches!(
            q.input_type,
            InquiryInputType::Choice | InquiryInputType::Confirm
        ) && !q.options.is_empty()
        {
            for (idx, opt) in q.options.iter().enumerate() {
                let is_selected = idx == state.selected_option_idx;
                let opt_start_row = body_lines.len();
                let cursor = if is_selected { "❯ " } else { "  " };
                let num_key = if q.input_type == InquiryInputType::Confirm && idx == 0 {
                    "[y] ".to_string()
                } else if q.input_type == InquiryInputType::Confirm && idx == 1 {
                    "[n] ".to_string()
                } else if idx < 9 {
                    format!("[{}] ", idx + 1)
                } else {
                    "    ".to_string()
                };

                let check = if q.is_multi_select {
                    if state.selected_multi_indices.contains(&idx) {
                        "[✓] "
                    } else {
                        "[ ] "
                    }
                } else {
                    ""
                };

                let mut spans: Vec<(String, Style)> = vec![
                    (
                        cursor.to_string(),
                        if is_selected {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                    (num_key, Style::default().fg(theme.muted)),
                    (
                        check.to_string(),
                        Style::default().fg(if is_selected {
                            theme.brand_accent
                        } else {
                            theme.muted
                        }),
                    ),
                    (
                        opt.label.clone(),
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
                    spans.push((
                        " (Recommended)".to_string(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                if let Some(desc) = &opt.description {
                    spans.push((format!(" — {}", desc), Style::default().fg(theme.muted)));
                }

                // Hanging indent of 6 spaces aligns wrapped description lines cleanly with the option label and pros/cons.
                let wrapped_opt = wrap_styled_spans(spans, body_wrap_width, "      ");
                body_lines.extend(wrapped_opt);

                // Render Pros/Cons trade-off cards when present
                if !opt.pros.is_empty() || !opt.cons.is_empty() {
                    for pro in &opt.pros {
                        let wrapped_pro = wrap_styled_spans(
                            vec![
                                ("      ".to_string(), Style::default()),
                                (
                                    "+ ".to_string(),
                                    Style::default()
                                        .fg(Color::Green)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                (pro.clone(), Style::default().fg(Color::Green)),
                            ],
                            body_wrap_width,
                            "        ",
                        );
                        body_lines.extend(wrapped_pro);
                    }
                    for con in &opt.cons {
                        let wrapped_con = wrap_styled_spans(
                            vec![
                                ("      ".to_string(), Style::default()),
                                (
                                    "- ".to_string(),
                                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                                ),
                                (con.clone(), Style::default().fg(Color::Red)),
                            ],
                            body_wrap_width,
                            "        ",
                        );
                        body_lines.extend(wrapped_con);
                    }
                }

                if is_selected && !state.is_typing_custom {
                    selected_start_row = opt_start_row;
                    selected_end_row = body_lines.len();
                }
            }

            if q.allow_custom {
                let custom_idx = q.options.len();
                let is_selected = custom_idx == state.selected_option_idx;
                let opt_start_row = body_lines.len();
                let cursor = if is_selected { "❯ " } else { "  " };
                let wrapped_custom = wrap_styled_spans(
                    vec![
                        (
                            cursor.to_string(),
                            if is_selected {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default()
                            },
                        ),
                        ("[w] ".to_string(), Style::default().fg(theme.muted)),
                        (
                            "Write-in custom option / answer...".to_string(),
                            if is_selected {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.muted)
                            },
                        ),
                    ],
                    body_wrap_width,
                    "      ",
                );
                body_lines.extend(wrapped_custom);
                if is_selected && !state.is_typing_custom {
                    selected_start_row = opt_start_row;
                    selected_end_row = body_lines.len();
                }
            }
        } else if q.input_type == InquiryInputType::Rank && !q.options.is_empty() {
            let order = state.current_rank_order();
            for (rank_pos, &opt_idx) in order.iter().enumerate() {
                if let Some(opt) = q.options.get(opt_idx) {
                    let is_selected = rank_pos == state.selected_option_idx;
                    let opt_start_row = body_lines.len();
                    let cursor = if is_selected { "❯ " } else { "  " };
                    let mut spans: Vec<(String, Style)> = vec![
                        (
                            cursor.to_string(),
                            if is_selected {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default()
                            },
                        ),
                        (
                            format!("#{} ", rank_pos + 1),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        (
                            opt.label.clone(),
                            if is_selected {
                                Style::default()
                                    .fg(theme.text_primary)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.text_primary)
                            },
                        ),
                    ];
                    if let Some(desc) = &opt.description {
                        spans.push((format!(" — {}", desc), Style::default().fg(theme.muted)));
                    }
                    let wrapped_rank = wrap_styled_spans(spans, body_wrap_width, "     ");
                    body_lines.extend(wrapped_rank);
                    if is_selected {
                        selected_start_row = opt_start_row;
                        selected_end_row = body_lines.len();
                    }
                }
            }
        } else if q.input_type == InquiryInputType::Slider {
            selected_start_row = body_lines.len();
            let (min, max, step) = q.slider_bounds();
            let val = state.current_slider_value();
            let bar_width: usize = 24.min(body_wrap_width.saturating_sub(16).max(8));
            let span_range = (max - min).max(1) as f64;
            let ratio = ((val - min) as f64 / span_range).clamp(0.0, 1.0);
            let knob_pos = ((ratio * (bar_width.saturating_sub(1) as f64)).round() as usize)
                .min(bar_width.saturating_sub(1));
            let filled = "━".repeat(knob_pos);
            let empty = "─".repeat(bar_width.saturating_sub(knob_pos + 1));

            body_lines.push(Line::from(vec![
                Span::styled("  ◄ ", Style::default().fg(theme.muted)),
                Span::styled(filled, Style::default().fg(theme.brand_accent)),
                Span::styled(
                    "●",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(empty, Style::default().fg(theme.muted)),
                Span::styled(" ►  ", Style::default().fg(theme.muted)),
                Span::styled(
                    format!("{}", val),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  (range: {}..={}, step: {})", min, max, step),
                    Style::default().fg(theme.muted),
                ),
            ]));
            selected_end_row = body_lines.len();
        }

        // In-card input box
        if state.is_typing_custom || requires_text_input(q) {
            body_lines.push(Line::from(""));
            selected_start_row = body_lines.len();
            let input_title = if q.input_type == InquiryInputType::Secret {
                "Enter Secret / API Key (Masked)"
            } else {
                "Enter Custom Answer"
            };
            let box_width = body_wrap_width
                .saturating_sub(2)
                .min(64)
                .max(input_title.len() + 6);
            let top_fill = box_width.saturating_sub(input_title.len() + 5);
            body_lines.push(Line::from(Span::styled(
                format!("┌─ {} {}┐", input_title, "─".repeat(top_fill)),
                Style::default().fg(theme.info),
            )));

            if state.custom_input_buffer.is_empty() && q.input_type != InquiryInputType::Secret {
                let default_hint;
                let placeholder = if let Some(ph) = &q.placeholder {
                    ph.as_str()
                } else if let Some(def) = &q.default_value {
                    default_hint = format!("(default: {})", def);
                    default_hint.as_str()
                } else {
                    "Type answer here..."
                };
                let placeholder_budget = box_width.saturating_sub(7);
                let ph_chars: String = placeholder.chars().take(placeholder_budget).collect();
                let pad_len =
                    placeholder_budget.saturating_sub(UnicodeWidthStr::width(ph_chars.as_str()));

                body_lines.push(Line::from(vec![
                    Span::styled("│ ❯ ", Style::default().fg(theme.brand_accent)),
                    Span::styled("█", Style::default().fg(theme.highlight)),
                    Span::styled(
                        format!(" {}{}", ph_chars, " ".repeat(pad_len)),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled("│", Style::default().fg(theme.info)),
                ]));
            } else {
                let raw_text = if q.input_type == InquiryInputType::Secret {
                    "•".repeat(state.custom_input_buffer.chars().count())
                } else {
                    state.custom_input_buffer.clone()
                };
                let char_cursor = state
                    .custom_input_buffer
                    .get(..state.custom_cursor.min(state.custom_input_buffer.len()))
                    .map(|s| s.chars().count())
                    .unwrap_or(0);

                let content_max = box_width.saturating_sub(6);
                let window_start = if char_cursor >= content_max {
                    char_cursor + 1 - content_max
                } else {
                    0
                };
                let visible_chars: Vec<char> = raw_text
                    .chars()
                    .skip(window_start)
                    .take(content_max)
                    .collect();
                let cursor_rel = char_cursor
                    .saturating_sub(window_start)
                    .min(visible_chars.len());

                let prefix: String = visible_chars[..cursor_rel].iter().collect();
                let suffix: String = visible_chars[cursor_rel..].iter().collect();
                let display_cols = UnicodeWidthStr::width(prefix.as_str())
                    + UnicodeWidthStr::width(suffix.as_str());
                let pad_len = content_max.saturating_sub(display_cols);

                let text_style = Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD);

                body_lines.push(Line::from(vec![
                    Span::styled("│ ❯ ", Style::default().fg(theme.brand_accent)),
                    Span::styled(prefix, text_style),
                    Span::styled("█", Style::default().fg(theme.highlight)),
                    Span::styled(suffix, text_style),
                    Span::styled(
                        format!("{}│", " ".repeat(pad_len)),
                        Style::default().fg(theme.info),
                    ),
                ]));
            }

            body_lines.push(Line::from(Span::styled(
                format!("└{}┘", "─".repeat(box_width.saturating_sub(2))),
                Style::default().fg(theme.info),
            )));
            selected_end_row = body_lines.len();
        }
    }

    // Dynamically size modal height to fit content when possible, clamping to terminal bounds.
    let max_modal_h = area
        .height
        .saturating_sub(2)
        .max(area.height.min(12))
        .max(1);
    let min_modal_h = 14.min(max_modal_h);
    // +2 for borders, +2 for scroll-status/spacer row and footer row
    let ideal_modal_h = (header_lines.len() + body_lines.len() + 4) as u16;
    let height = ideal_modal_h.clamp(min_modal_h, max_modal_h);

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(Span::styled(
            format!(" ? {} ", state.request.title),
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
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let footer_text = build_inquiry_footer_text(state, inner.width as usize);
    let footer_h: u16 = 1;
    let avail_above_footer = inner.height.saturating_sub(footer_h) as usize;

    // Ensure the scrollable options body gets at least half of the modal on short terminals.
    let header_h = if header_lines.len() + 4 <= avail_above_footer {
        header_lines.len()
    } else {
        avail_above_footer.saturating_sub(5).min(header_lines.len())
    };

    if header_h > 0 {
        let header_rect = Rect::new(inner.x, inner.y, inner.width, header_h as u16);
        let header_slice: Vec<Line<'static>> = header_lines.into_iter().take(header_h).collect();
        frame.render_widget(Paragraph::new(header_slice), header_rect);
    }

    let remaining_after_header = avail_above_footer.saturating_sub(header_h);
    let total_body_rows = body_lines.len();
    // Reserve 1 status row above the footer when scrolling is active (or as breathing room when it fits).
    let reserve_status_row = usize::from(remaining_after_header > 2);
    let body_viewport_h = remaining_after_header.saturating_sub(reserve_status_row);

    let scroll_y = if total_body_rows <= body_viewport_h || body_viewport_h == 0 {
        0
    } else {
        // Keep the entire selected option block (label + wrapped desc + pros/cons) visible,
        // with 1 row of lookahead cushion below when possible.
        let effective_end = (selected_end_row + 1).min(total_body_rows);
        let min_scroll_for_end = effective_end.saturating_sub(body_viewport_h);
        let max_scroll = total_body_rows.saturating_sub(body_viewport_h);
        min_scroll_for_end.min(selected_start_row).min(max_scroll)
    };

    if body_viewport_h > 0 {
        let body_rect = Rect::new(
            inner.x,
            inner.y + header_h as u16,
            inner.width,
            body_viewport_h as u16,
        );
        let body_paragraph = Paragraph::new(body_lines).scroll((scroll_y as u16, 0));
        frame.render_widget(body_paragraph, body_rect);

        // Render vertical scrollbar on the right edge when content overflows the viewport.
        if total_body_rows > body_viewport_h && inner.width > 4 {
            let max_scroll = total_body_rows.saturating_sub(body_viewport_h).max(1);
            let thumb_h =
                ((body_viewport_h * body_viewport_h) / total_body_rows).clamp(1, body_viewport_h);
            let track_span = body_viewport_h.saturating_sub(thumb_h);
            let thumb_top = (scroll_y * track_span) / max_scroll;

            let bar_x = inner.x + inner.width.saturating_sub(1);
            for row_offset in 0..body_viewport_h {
                let is_thumb = row_offset >= thumb_top && row_offset < thumb_top + thumb_h;
                let (glyph, style) = if is_thumb {
                    (
                        "┃",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("│", Style::default().fg(theme.muted))
                };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(glyph, style))),
                    Rect::new(bar_x, body_rect.y + row_offset as u16, 1, 1),
                );
            }
        }
    }

    // Render scroll status / position indicator row when options overflow the viewport.
    if reserve_status_row > 0 && total_body_rows > body_viewport_h {
        let hidden_above = scroll_y;
        let hidden_below = total_body_rows.saturating_sub(scroll_y + body_viewport_h);
        let total_opts = state.total_options_count();
        let current_opt = (state.selected_option_idx + 1).min(total_opts.max(1));

        let up_hint = if hidden_above > 0 {
            format!("▲ {} more above", hidden_above)
        } else {
            "▲ top".to_string()
        };
        let down_hint = if hidden_below > 0 {
            format!("▼ {} more below", hidden_below)
        } else {
            "▼ bottom".to_string()
        };

        let status_line = Line::from(vec![
            Span::styled(
                up_hint,
                Style::default().fg(if hidden_above > 0 {
                    theme.brand_accent
                } else {
                    theme.muted
                }),
            ),
            Span::styled(
                format!("   •   Option {}/{}   •   ", current_opt, total_opts.max(1)),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                down_hint,
                Style::default().fg(if hidden_below > 0 {
                    theme.brand_accent
                } else {
                    theme.muted
                }),
            ),
        ]);
        let status_y = inner.y + (header_h + body_viewport_h) as u16;
        frame.render_widget(
            Paragraph::new(status_line).alignment(Alignment::Center),
            Rect::new(inner.x, status_y, inner.width, 1),
        );
    }

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
                    min_value: None,
                    max_value: None,
                    step: None,
                    options: vec![
                        InquiryOption {
                            id: "react".to_string(),
                            label: "React 19".to_string(),
                            description: Some("Vite + Tailwind".to_string()),
                            recommended: true,
                            pros: vec!["Fast HMR".to_string()],
                            cons: vec!["SPA only".to_string()],
                        },
                        InquiryOption {
                            id: "vue".to_string(),
                            label: "Vue 3".to_string(),
                            description: Some("Nuxt / Pinia".to_string()),
                            recommended: false,
                            pros: Vec::new(),
                            cons: Vec::new(),
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
                    min_value: None,
                    max_value: None,
                    step: None,
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
            min_value: None,
            max_value: None,
            step: None,
            options: vec![
                InquiryOption {
                    id: "fast".to_string(),
                    label: "Fast".to_string(),
                    description: None,
                    recommended: false,
                    pros: Vec::new(),
                    cons: Vec::new(),
                },
                InquiryOption {
                    id: "secure".to_string(),
                    label: "Secure".to_string(),
                    description: None,
                    recommended: true,
                    pros: Vec::new(),
                    cons: Vec::new(),
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

    #[test]
    fn test_buffer_delete_home_end_clear_kill() {
        let req = dummy_request();
        let mut state = InquiryModalState::from_request(1, "tool-edit", req);
        state.is_typing_custom = true;
        for c in "hello world".chars() {
            state.handle_char(c);
        }
        assert_eq!(state.custom_input_buffer, "hello world");
        assert_eq!(state.custom_cursor, 11);

        state.cursor_home();
        assert_eq!(state.custom_cursor, 0);

        // Delete 'h'
        state.handle_delete();
        assert_eq!(state.custom_input_buffer, "ello world");
        assert_eq!(state.custom_cursor, 0);

        // Move to ' '
        for _ in 0..4 {
            state.cursor_right();
        }
        assert_eq!(state.custom_cursor, 4);

        // Kill to end removes " world"
        state.kill_to_end();
        assert_eq!(state.custom_input_buffer, "ello");

        state.cursor_end();
        assert_eq!(state.custom_cursor, 4);

        state.clear_line();
        assert_eq!(state.custom_input_buffer, "");
        assert_eq!(state.custom_cursor, 0);
    }

    #[test]
    fn test_multi_select_fallback_to_highlighted() {
        let req = InquiryRequest {
            inquiry_id: "inq_multi".to_string(),
            title: "Multi test".to_string(),
            description: None,
            questions: vec![InquiryQuestion {
                id: "features".to_string(),
                question: "Select features".to_string(),
                header: None,
                input_type: InquiryInputType::Choice,
                is_multi_select: true,
                allow_custom: false,
                placeholder: None,
                default_value: None,
                min_value: None,
                max_value: None,
                step: None,
                options: vec![
                    InquiryOption {
                        id: "auth".to_string(),
                        label: "Authentication".to_string(),
                        description: None,
                        recommended: false,
                        pros: Vec::new(),
                        cons: Vec::new(),
                    },
                    InquiryOption {
                        id: "db".to_string(),
                        label: "Database".to_string(),
                        description: None,
                        recommended: false,
                        pros: Vec::new(),
                        cons: Vec::new(),
                    },
                ],
            }],
        };

        let mut state = InquiryModalState::from_request(1, "tool-multi", req);
        state.selected_option_idx = 1; // Highlight "db"
                                       // User didn't toggle Space, just pressed Enter
        let resp = state.confirm_selection().unwrap();
        assert_eq!(resp.answers.len(), 1);
        assert_eq!(resp.answers[0].selected_options, vec!["db"]);
    }

    #[test]
    fn test_non_linear_tab_navigation_and_rank_slider_modes() {
        let req = InquiryRequest {
            inquiry_id: "inq_octto".to_string(),
            title: "Architecture & Priorities".to_string(),
            description: None,
            questions: vec![
                InquiryQuestion {
                    id: "priorities".to_string(),
                    question: "Rank your priorities".to_string(),
                    header: Some("Priorities".to_string()),
                    input_type: InquiryInputType::Rank,
                    is_multi_select: false,
                    allow_custom: false,
                    placeholder: None,
                    default_value: None,
                    min_value: None,
                    max_value: None,
                    step: None,
                    options: vec![
                        InquiryOption {
                            id: "latency".to_string(),
                            label: "Low Latency".to_string(),
                            description: None,
                            recommended: false,
                            pros: Vec::new(),
                            cons: Vec::new(),
                        },
                        InquiryOption {
                            id: "cost".to_string(),
                            label: "Low Cost".to_string(),
                            description: None,
                            recommended: false,
                            pros: Vec::new(),
                            cons: Vec::new(),
                        },
                    ],
                },
                InquiryQuestion {
                    id: "concurrency".to_string(),
                    question: "Target concurrency".to_string(),
                    header: Some("Scale".to_string()),
                    input_type: InquiryInputType::Slider,
                    is_multi_select: false,
                    allow_custom: false,
                    placeholder: None,
                    default_value: Some("4".to_string()),
                    min_value: Some(1),
                    max_value: Some(8),
                    step: Some(2),
                    options: Vec::new(),
                },
            ],
        };

        let mut state = InquiryModalState::from_request(1, "tool-octto", req);
        // Jump to question 2 first (non-linear order!)
        state.next_question_tab();
        assert_eq!(state.current_question_idx, 1);
        assert_eq!(state.current_slider_value(), 4);
        state.slider_increment();
        assert_eq!(state.current_slider_value(), 6);

        // Confirm question 2 -> automatically jumps back to unanswered question 1 (idx 0)
        assert!(state.confirm_selection().is_none());
        assert_eq!(state.current_question_idx, 0);

        // In question 1 (Rank mode), move option 1 ("cost") up above option 0 ("latency")
        state.next_option();
        assert_eq!(state.selected_option_idx, 1);
        state.move_rank_up();
        assert_eq!(state.selected_option_idx, 0);

        let final_resp = state
            .confirm_selection()
            .expect("All questions answered now");
        assert_eq!(final_resp.answers.len(), 2);
        assert_eq!(
            final_resp.answers[0].selected_options,
            vec!["cost".to_string(), "latency".to_string()]
        );
        assert_eq!(
            final_resp.answers[1].selected_options,
            vec!["6".to_string()]
        );
    }

    #[test]
    fn test_inquiry_modal_responsive_scroll_and_no_footer_clipping() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let req = InquiryRequest {
            inquiry_id: "inq-landing".to_string(),
            title: "minicode landing page foundation".to_string(),
            description: Some(
                "Workspace is empty (no manifest). Need foundation + a couple of product-owned decisions before scaffolding."
                    .to_string(),
            ),
            questions: vec![
                InquiryQuestion {
                    id: "stack".to_string(),
                    question: "Which foundation should the minicode landing page be built on?"
                        .to_string(),
                    header: Some("Stack".to_string()),
                    input_type: InquiryInputType::Choice,
                    is_multi_select: false,
                    allow_custom: true,
                    placeholder: None,
                    default_value: None,
                    min_value: None,
                    max_value: None,
                    step: None,
                    options: vec![
                        InquiryOption {
                            id: "static-website".to_string(),
                            label: "static-website (HTML/CSS/JS)".to_string(),
                            description: Some(
                                "Plain HTML/CSS/JS. Fastest to load, zero build step, ideal for a single premium marketing page with hand-crafted CSS."
                                    .to_string(),
                            ),
                            recommended: true,
                            pros: vec![
                                "Zero deps, no build wait".to_string(),
                                "Full control over CSS art direction".to_string(),
                                "Easiest to deploy anywhere".to_string(),
                            ],
                            cons: Vec::new(),
                        },
                        InquiryOption {
                            id: "react-vite".to_string(),
                            label: "react-vite".to_string(),
                            description: Some(
                                "React 19 + Vite 8 + Tailwind 4 + Lucide. Best if this will grow into a multi-page marketing site."
                                    .to_string(),
                            ),
                            recommended: false,
                            pros: vec![
                                "Component reuse".to_string(),
                                "Tailwind utility styling".to_string(),
                                "Easy to extend later".to_string(),
                            ],
                            cons: Vec::new(),
                        },
                        InquiryOption {
                            id: "next-template".to_string(),
                            label: "next-template".to_string(),
                            description: Some(
                                "Next.js 16 App Router + Tailwind 4. Best for SEO metadata and eventual blog/docs routes."
                                    .to_string(),
                            ),
                            recommended: false,
                            pros: vec![
                                "SSR + metadata".to_string(),
                                "Route-based docs later".to_string(),
                                "Heavier build".to_string(),
                            ],
                            cons: Vec::new(),
                        },
                        InquiryOption {
                            id: "react-vite-gsap".to_string(),
                            label: "react-vite-gsap".to_string(),
                            description: Some(
                                "React + Vite + Tailwind + GSAP + Framer Motion + Lenis smooth scroll."
                                    .to_string(),
                            ),
                            recommended: false,
                            pros: vec![
                                "Built-in scroll choreography".to_string(),
                                "Smooth inertia scrolling".to_string(),
                            ],
                            cons: vec!["Larger JS bundle".to_string()],
                        },
                    ],
                },
                InquiryQuestion {
                    id: "cta".to_string(),
                    question: "Which CTA links?".to_string(),
                    header: Some("Install/CTA links".to_string()),
                    input_type: InquiryInputType::Text,
                    is_multi_select: false,
                    allow_custom: true,
                    placeholder: None,
                    default_value: None,
                    min_value: None,
                    max_value: None,
                    step: None,
                    options: Vec::new(),
                },
                InquiryQuestion {
                    id: "palette".to_string(),
                    question: "Which palette?".to_string(),
                    header: Some("Palette".to_string()),
                    input_type: InquiryInputType::Text,
                    is_multi_select: false,
                    allow_custom: true,
                    placeholder: None,
                    default_value: None,
                    min_value: None,
                    max_value: None,
                    step: None,
                    options: Vec::new(),
                },
            ],
        };

        let mut state = InquiryModalState::from_request(1, "tool-ui", req);
        let theme = Theme::default();
        let backend = TestBackend::new(80, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let render_to_string =
            |term: &mut Terminal<TestBackend>, st: &InquiryModalState| -> String {
                term.draw(|f| {
                    let area = f.area();
                    render_inquiry_modal(f, st, &theme, area);
                })
                .expect("draw");
                let buf = term.backend().buffer();
                let mut out = String::new();
                for y in 0..buf.area.height {
                    for x in 0..buf.area.width {
                        out.push_str(buf[(x, y)].symbol());
                    }
                    out.push('\n');
                }
                out
            };

        // 1. Initial render: footer must show full "[Enter] Confirm" (never clipped to "[Enter] Conf")
        let screen0 = render_to_string(&mut terminal, &state);
        assert!(
            screen0.contains("[Enter] Confirm"),
            "Footer was clipped on 80x28 screen:\n{}",
            screen0
        );

        // 2. Navigate down to Option 4 (index 3: react-vite-gsap)
        state.next_option(); // idx 1
        state.next_option(); // idx 2
        state.next_option(); // idx 3 (react-vite-gsap)
        assert_eq!(state.selected_option_idx, 3);
        let screen3 = render_to_string(&mut terminal, &state);
        assert!(
            screen3.contains("[1/3] Stack:"),
            "Pinned question header should remain visible when scrolled:\n{}",
            screen3
        );
        assert!(
            screen3.contains("Lenis smooth scroll"),
            "Option 4 wrapped description should be fully visible when Option 4 is selected:\n{}",
            screen3
        );
        assert!(
            screen3.contains("Built-in scroll choreography")
                && screen3.contains("Larger JS bundle"),
            "Option 4 pros and cons should be fully scrolled into view:\n{}",
            screen3
        );

        // 3. Navigate down to [w] Write-in (index 4) and activate custom input box
        state.next_option(); // idx 4 ([w] Write-in)
        let screen4 = render_to_string(&mut terminal, &state);
        assert!(
            screen4.contains("Write-in custom option / answer..."),
            "Write-in option must be visible when selected:\n{}",
            screen4
        );

        state.is_typing_custom = true;
        let screen_custom = render_to_string(&mut terminal, &state);
        assert!(
            screen_custom.contains("Enter Custom Answer"),
            "Custom answer input box must be scrolled into view when typing custom:\n{}",
            screen_custom
        );
    }
}
