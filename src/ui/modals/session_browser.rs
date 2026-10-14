//! Session browser and history inspection modal rendering.

use crate::constants::{
    SESSION_BROWSER_MAX_HEIGHT, SESSION_BROWSER_MAX_WIDTH, SESSION_BROWSER_MIN_HEIGHT,
    SESSION_BROWSER_MIN_WIDTH, SESSION_LIST_ITEM_HEIGHT, SESSION_TIME_AGO_COLS,
};
use crate::session::store::{truncate_display, SessionMetadata, SessionSummary};
use crate::ui::layout_utils::centered_rect_exact;
use crate::ui::modals::common::{compute_scroll_offset, format_time_ago};
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

pub fn render_session_browser(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    sessions: &[SessionMetadata],
    selected_index: usize,
    _cached_summary: Option<&SessionSummary>,
) {
    let width =
        (area.width.saturating_sub(4)).clamp(SESSION_BROWSER_MIN_WIDTH, SESSION_BROWSER_MAX_WIDTH);
    let height = (area.height.saturating_sub(4))
        .clamp(SESSION_BROWSER_MIN_HEIGHT, SESSION_BROWSER_MAX_HEIGHT);
    let popup_area = centered_rect_exact(width, height, area);
    frame.render_widget(Clear, popup_area);

    let total_sessions = sessions.len();
    let index_badge = if total_sessions > 0 {
        format!(" [{}/{}] ", selected_index + 1, total_sessions)
    } else {
        " [0/0] ".to_string()
    };

    let outer_block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                " Resume Session ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(index_badge, Style::default().fg(theme.muted)),
        ]))
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.brand_accent))
        .padding(Padding::new(1, 1, 1, 1))
        .style(Style::default().bg(theme.bg_elevated));

    let inner_area = outer_block.inner(popup_area);
    frame.render_widget(outer_block, popup_area);

    if sessions.is_empty() {
        let empty = Paragraph::new(Line::from(vec![Span::styled(
            "No past sessions found in this workspace.",
            Style::default().fg(theme.muted),
        )]))
        .alignment(Alignment::Center);
        frame.render_widget(empty, inner_area);
    } else {
        let available_height = inner_area.height as usize;
        let max_visible = (available_height / SESSION_LIST_ITEM_HEIGHT).max(1);
        let scroll_offset = compute_scroll_offset(selected_index, max_visible);
        let visible_sessions = sessions
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(max_visible);

        let total_cols = inner_area.width as usize;
        let prefix_cols = 3usize; // " › " or "   "
        let time_cols = SESSION_TIME_AGO_COLS;
        let gap_cols = 2usize;
        let title_col_budget = total_cols.saturating_sub(prefix_cols + time_cols + gap_cols);

        let items: Vec<ListItem> = visible_sessions
            .map(|(i, s)| {
                let is_selected = i == selected_index;
                let time_ago = format_time_ago(&s.created_at);

                let raw_title = if !s.preview.is_empty() {
                    s.preview.replace('\n', " ")
                } else {
                    "Untitled session".to_string()
                };

                let truncated_title =
                    truncate_display(raw_title.trim(), title_col_budget.max(8), "…");
                let title_w = UnicodeWidthStr::width(truncated_title.as_str());
                let pad_spaces = title_col_budget.saturating_sub(title_w) + gap_cols;

                let line = if is_selected {
                    Line::from(vec![
                        Span::styled(
                            " › ",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            truncated_title,
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" ".repeat(pad_spaces)),
                        Span::styled(
                            format!("{:>width$}", time_ago, width = time_cols),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw("   "),
                        Span::styled(truncated_title, Style::default().fg(theme.text_primary)),
                        Span::raw(" ".repeat(pad_spaces)),
                        Span::styled(
                            format!("{:>width$}", time_ago, width = time_cols),
                            Style::default().fg(theme.muted),
                        ),
                    ])
                };

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_session_browser_minimal_single_line_layout() {
        let theme = Theme::aura_dark();
        let sessions = vec![
            SessionMetadata {
                id: "20261013T093113Z-baa7ebca".to_string(),
                created_at: "2026-10-13T09:31:13Z".to_string(),
                workspace: "/tmp/ws".to_string(),
                event_count: 659,
                preview: "create a landing page for our minicode".to_string(),
                path: "/tmp/s1.jsonl".to_string(),
            },
            SessionMetadata {
                id: "20261012T080000Z-11223344".to_string(),
                created_at: "2026-10-12T08:00:00Z".to_string(),
                workspace: "/tmp/ws".to_string(),
                event_count: 42,
                preview: "fix authentication middleware".to_string(),
                path: "/tmp/s2.jsonl".to_string(),
            },
        ];

        let backend = TestBackend::new(90, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                render_session_browser(f, f.area(), &theme, &sessions, 0, None);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut all_text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                all_text.push_str(buffer[(x, y)].symbol());
            }
            all_text.push('\n');
        }

        assert!(all_text.contains("Resume Session"));
        assert!(!all_text.contains("📜"));
        assert!(!all_text.contains("events"));
        assert!(!all_text.contains("id:"));
        assert!(all_text.contains("create a landing page for our minicode"));
        assert!(all_text.contains("fix authentication middleware"));
    }
}
