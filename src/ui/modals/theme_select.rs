//! Theme switcher and streaming configuration modals rendering.

use crate::constants::{
    PROVIDER_SELECT_WIDTH_PCT, STREAMING_SELECT_HEIGHT_PCT, THEME_MODAL_MAX_VISIBLE,
    THEME_NAME_DISPLAY_COLS, THEME_SELECT_HEIGHT_PCT, THEME_SELECT_WIDTH_PCT,
};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::{Theme, ThemeInfo};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

pub fn render_theme_select(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    themes: &[ThemeInfo],
    selected_index: usize,
) {
    let popup_area = centered_rect(THEME_SELECT_WIDTH_PCT, THEME_SELECT_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    let max_visible = THEME_MODAL_MAX_VISIBLE;
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);
    let visible_themes = themes.iter().skip(scroll_offset).take(max_visible);

    for (idx_rel, t) in visible_themes.enumerate() {
        let idx = scroll_offset + idx_rel;
        let is_selected = idx == selected_index;

        let cursor = if is_selected { "  ❯ " } else { "    " };
        let title_style = if is_selected {
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };

        let mut header_spans = vec![
            Span::styled(
                cursor,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("[{}] ", idx + 1), Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:<width$}", t.name, width = THEME_NAME_DISPLAY_COLS),
                title_style,
            ),
        ];

        for c in &t.swatches {
            header_spans.push(Span::styled(" ■", Style::default().fg(*c)));
        }

        lines.push(Line::from(header_spans));

        let desc_style = if is_selected {
            Style::default().fg(theme.text_primary)
        } else {
            Style::default().fg(theme.muted)
        };
        lines.push(Line::from(vec![
            Span::styled("        ", Style::default()),
            Span::styled(&t.description, desc_style),
        ]));

        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![
        Span::styled(
            "  [Enter] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Apply & Save   ", Style::default().fg(theme.text_primary)),
        Span::styled("[Esc] ", Style::default().fg(theme.muted)),
        Span::styled("Cancel", Style::default().fg(theme.muted)),
    ]));

    let block = Block::default()
        .title(" Theme Switcher ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_elevated),
        )
        .style(Style::default().bg(theme.bg_elevated));

    let p = Paragraph::new(lines).block(block);
    frame.render_widget(p, popup_area);
}

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
