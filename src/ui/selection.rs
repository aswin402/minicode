use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::cell::{Cell, RefCell};

/// Manages mouse drag text selection, visual highlighting, and clipboard copying for the TUI timeline
#[derive(Debug, Default)]
pub struct TimelineSelection {
    pub start: Cell<Option<(u16, u16)>>,
    pub end: Cell<Option<(u16, u16)>>,
    pub is_selecting: Cell<bool>,
    pub timeline_area: Cell<Rect>,
    pub cached_plain_lines: RefCell<Vec<String>>,
}

impl TimelineSelection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles mouse button press to begin text selection
    pub fn handle_mouse_down(&self, col: u16, row: u16, scroll_offset: u16) {
        let area = self.timeline_area.get();
        let char_col = col.saturating_sub(area.x);
        let line_idx = row.saturating_sub(area.y).saturating_add(scroll_offset);
        self.start.set(Some((char_col, line_idx)));
        self.end.set(Some((char_col, line_idx)));
        self.is_selecting.set(true);
    }

    /// Handles mouse drag to expand text selection range
    pub fn handle_mouse_drag(&self, col: u16, row: u16, scroll_offset: u16) {
        if !self.is_selecting.get() {
            return;
        }
        let area = self.timeline_area.get();
        let char_col = col.saturating_sub(area.x);
        let line_idx = row.saturating_sub(area.y).saturating_add(scroll_offset);
        self.end.set(Some((char_col, line_idx)));
    }

    /// Handles mouse button release: completes selection and auto-copies to system clipboard
    pub fn handle_mouse_up(&self, col: u16, row: u16, scroll_offset: u16) -> Option<String> {
        if !self.is_selecting.get() {
            return None;
        }
        self.handle_mouse_drag(col, row, scroll_offset);
        self.is_selecting.set(false);

        let extracted = self.extract_selected_text();
        if let Some(ref text) = extracted {
            if !text.trim().is_empty() {
                crate::ui::clipboard::copy_to_clipboard(text);
            }
        }
        extracted
    }

    /// Clears any active visual text selection
    pub fn clear(&self) {
        self.start.set(None);
        self.end.set(None);
        self.is_selecting.set(false);
    }

    /// Caches rendered plain lines for selection extraction
    pub fn cache_plain_lines(&self, lines: &[Line]) {
        let mut cache = self.cached_plain_lines.borrow_mut();
        cache.clear();
        for l in lines {
            let mut line_str = String::new();
            for s in &l.spans {
                line_str.push_str(&s.content);
            }
            cache.push(line_str);
        }
    }

    /// Returns whether there is an active selection
    pub fn has_selection(&self) -> bool {
        self.start.get().is_some()
    }

    /// Extracts the plain string contents of the selected text region
    pub fn extract_selected_text(&self) -> Option<String> {
        let start = self.start.get()?;
        let end = self.end.get()?;

        if start == end {
            return None;
        }

        let ((c1, r1), (c2, r2)) = if start.1 < end.1 || (start.1 == end.1 && start.0 <= end.0) {
            (start, end)
        } else {
            (end, start)
        };

        let plain_lines = self.cached_plain_lines.borrow();
        if plain_lines.is_empty() {
            return None;
        }

        let mut result = String::new();
        for r in r1..=r2 {
            if (r as usize) < plain_lines.len() {
                let line = &plain_lines[r as usize];
                let char_count = line.chars().count();

                if r1 == r2 {
                    let start_c = (c1 as usize).min(char_count);
                    let end_c = (c2 as usize).min(char_count);
                    if start_c < end_c {
                        let sub: String =
                            line.chars().skip(start_c).take(end_c - start_c).collect();
                        result.push_str(&sub);
                    }
                } else if r == r1 {
                    let start_c = (c1 as usize).min(char_count);
                    let sub: String = line.chars().skip(start_c).collect();
                    result.push_str(&sub);
                    result.push('\n');
                } else if r == r2 {
                    let end_c = (c2 as usize).min(char_count);
                    let sub: String = line.chars().take(end_c).collect();
                    result.push_str(&sub);
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Slices a Line's spans and applies the visual selection highlight style across a character column range
    pub fn apply_selection_to_line<'a>(
        line: Line<'a>,
        sel_start: usize,
        sel_end: usize,
        theme: &'a Theme,
    ) -> Line<'a> {
        if sel_start >= sel_end {
            return line;
        }

        let mut new_spans = Vec::new();
        let mut current_col = 0;

        let selection_style = Style::default()
            .bg(theme.border)
            .fg(theme.text_primary)
            .add_modifier(Modifier::REVERSED);

        for span in line.spans {
            let span_len = span.content.chars().count();
            let span_end = current_col + span_len;

            if span_end <= sel_start || current_col >= sel_end {
                // Span is completely outside selection
                new_spans.push(span);
            } else {
                // Span overlaps with selection
                let chars: Vec<char> = span.content.chars().collect();
                let overlap_start = sel_start.saturating_sub(current_col).min(span_len);
                let overlap_end = (sel_end - current_col).min(span_len);

                // 1. Prefix before selection
                if overlap_start > 0 {
                    let prefix: String = chars[..overlap_start].iter().collect();
                    new_spans.push(Span::styled(prefix, span.style));
                }

                // 2. Selected portion
                if overlap_start < overlap_end {
                    let selected: String = chars[overlap_start..overlap_end].iter().collect();
                    new_spans.push(Span::styled(selected, selection_style));
                }

                // 3. Suffix after selection
                if overlap_end < span_len {
                    let suffix: String = chars[overlap_end..].iter().collect();
                    new_spans.push(Span::styled(suffix, span.style));
                }
            }

            current_col = span_end;
        }

        Line::from(new_spans)
    }

    /// Applies visual selection highlight across a list of rendered lines
    pub fn apply_highlight<'a>(&self, lines: Vec<Line<'a>>, theme: &'a Theme) -> Vec<Line<'a>> {
        if let (Some(start), Some(end)) = (self.start.get(), self.end.get()) {
            if start != end {
                let ((c1, r1), (c2, r2)) =
                    if start.1 < end.1 || (start.1 == end.1 && start.0 <= end.0) {
                        (start, end)
                    } else {
                        (end, start)
                    };

                let mut highlighted_lines = Vec::with_capacity(lines.len());
                for (idx, line) in lines.into_iter().enumerate() {
                    let line_idx = idx as u16;
                    if line_idx >= r1 && line_idx <= r2 {
                        let (sel_start, sel_end) = if r1 == r2 {
                            (c1 as usize, c2 as usize)
                        } else if line_idx == r1 {
                            (c1 as usize, usize::MAX)
                        } else if line_idx == r2 {
                            (0, c2 as usize)
                        } else {
                            (0, usize::MAX)
                        };
                        highlighted_lines.push(Self::apply_selection_to_line(
                            line, sel_start, sel_end, theme,
                        ));
                    } else {
                        highlighted_lines.push(line);
                    }
                }
                return highlighted_lines;
            }
        }
        lines
    }
}
