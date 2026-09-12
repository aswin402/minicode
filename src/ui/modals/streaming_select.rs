//! LLM response streaming configuration modal rendering.

use crate::constants::{PROVIDER_SELECT_WIDTH_PCT, STREAMING_SELECT_HEIGHT_PCT};
use crate::ui::layout_utils::centered_rect;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

/// Renders the LLM streaming toggle modal allowing user to enable/disable real-time token streaming.
pub fn render_streaming_select(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    selected_index: usize,
    current_streaming: bool,
) {
    let popup_area = centered_rect(
        PROVIDER_SELECT_WIDTH_PCT.max(52),
        STREAMING_SELECT_HEIGHT_PCT,
        area,
    );
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " ⚡ LLM Response Streaming ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )]))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_elevated),
        )
        .style(Style::default().bg(theme.bg_elevated));

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Subtitle
            Constraint::Min(4),    // Items list
            Constraint::Length(1), // Key hints footer
        ])
        .split(inner_area);

    let active_idx = if current_streaming { 0 } else { 1 };
    let options = [
        (
            "Enable Streaming",
            "Token-by-token live streaming (default)",
        ),
        (
            "Disable Streaming",
            "Wait for full response before rendering",
        ),
        ("Cancel / Back", "Keep current setting and return"),
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (label, desc))| {
            let is_selected = i == selected_index;
            let is_active = i == active_idx;
            let prefix = if is_selected { " › " } else { "   " };
            let marker = if i == 2 {
                ""
            } else if is_active {
                " [Active ◉]"
            } else {
                " [○]"
            };

            let item_style = if is_selected {
                Style::default()
                    .fg(theme.bg_primary)
                    .bg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            let desc_style = if is_selected {
                Style::default().fg(theme.bg_primary)
            } else {
                Style::default().fg(theme.muted)
            };

            let line1 = Line::from(vec![
                Span::styled(format!("{}{:<20}", prefix, label), item_style),
                Span::styled(
                    marker,
                    if is_active && !is_selected {
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        item_style
                    },
                ),
            ]);
            let line2 = Line::from(vec![Span::styled(format!("     {}", desc), desc_style)]);

            ListItem::new(vec![line1, line2]).style(if is_selected {
                Style::default().bg(theme.brand_accent)
            } else {
                Style::default().bg(theme.bg_elevated)
            })
        })
        .collect();

    let subtitle = Line::from(vec![Span::styled(
        " Select streaming behavior for assistant turns:",
        Style::default().fg(theme.muted),
    )]);
    frame.render_widget(Paragraph::new(subtitle), chunks[0]);

    let list = List::new(items)
        .style(Style::default().bg(theme.bg_elevated))
        .block(Block::default().borders(Borders::NONE));
    frame.render_widget(list, chunks[1]);

    let footer = Line::from(vec![
        Span::styled(
            "  ↑/↓/Tab: ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", Style::default().fg(theme.muted)),
        Span::styled(
            "Enter/1/2: ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Select  ", Style::default().fg(theme.muted)),
        Span::styled(
            "Esc: ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer), chunks[2]);
}
