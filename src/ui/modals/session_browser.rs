//! Session browser and history inspection modal rendering.

use crate::constants::{
    SESSION_BROWSER_MAX_HEIGHT, SESSION_BROWSER_MAX_WIDTH, SESSION_BROWSER_MIN_HEIGHT,
    SESSION_BROWSER_MIN_WIDTH, SESSION_ID_DISPLAY_COLS, SESSION_LIST_ITEM_HEIGHT,
    SESSION_TIME_AGO_COLS,
};
use crate::session::store::{truncate_display, SessionMetadata, SessionSummary};
use crate::ui::layout_utils::centered_rect_exact;
use crate::ui::modals::common::{compute_scroll_offset, format_time_ago};
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

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
                " 📜 Session History ",
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

        let items: Vec<ListItem> = visible_sessions
            .map(|(i, s)| {
                let is_selected = i == selected_index;
                let time_ago = format_time_ago(&s.created_at);

                // Determine preview title (prompt or clean fallback)
                let preview_title = if !s.preview.is_empty() {
                    s.preview.replace('\n', " ")
                } else {
                    format!("Session {}", s.id)
                };

                let event_text = if s.event_count == 0 {
                    "0 events".to_string()
                } else if s.event_count == 1 {
                    "1 event".to_string()
                } else {
                    format!("{} events", s.event_count)
                };

                let id_short = truncate_display(&s.id, SESSION_ID_DISPLAY_COLS, "…");

                let (line1, line2) = if is_selected {
                    let time_style = Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD);
                    let title_style = Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD);
                    let l1 = Line::from(vec![
                        Span::styled(
                            " › ",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<width$} ", time_ago, width = SESSION_TIME_AGO_COLS),
                            time_style,
                        ),
                        Span::styled(preview_title, title_style),
                    ]);

                    let l2 = Line::from(vec![
                        Span::raw("     "),
                        Span::styled("● ", Style::default().fg(theme.success)),
                        Span::styled(
                            format!("{}  •  id: {}", event_text, id_short),
                            Style::default().fg(theme.muted),
                        ),
                    ]);
                    (l1, l2)
                } else {
                    let time_style = Style::default().fg(theme.muted);
                    let title_style = Style::default().fg(theme.text_primary);
                    let l1 = Line::from(vec![
                        Span::raw("   "),
                        Span::styled(
                            format!("{:<width$} ", time_ago, width = SESSION_TIME_AGO_COLS),
                            time_style,
                        ),
                        Span::styled(preview_title, title_style),
                    ]);

                    let l2 = Line::from(vec![
                        Span::raw("     "),
                        Span::styled("● ", Style::default().fg(theme.muted)),
                        Span::styled(
                            format!("{}  •  id: {}", event_text, id_short),
                            Style::default().fg(theme.muted),
                        ),
                    ]);
                    (l1, l2)
                };

                ListItem::new(vec![line1, line2])
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner_area);
    }
}
