//! Provider selection and model picker modal rendering.

use crate::constants::{PROVIDER_SELECT_HEIGHT_PCT, PROVIDER_SELECT_WIDTH_PCT};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

pub fn render_provider_select(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    providers: &[String],
    selected_index: usize,
) {
    render_provider_select_with_active(frame, area, theme, providers, selected_index, None, None);
}

pub fn render_provider_select_with_active(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    providers: &[String],
    selected_index: usize,
    active_provider: Option<&str>,
    parent_label: Option<&str>,
) {
    let popup_area = centered_rect(PROVIDER_SELECT_WIDTH_PCT, PROVIDER_SELECT_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Select Provider ")
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
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner_area);

    let max_visible = (chunks[0].height as usize).max(1);
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);

    let items: Vec<ListItem> = providers
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
        .map(|(i, p)| {
            let is_selected = i == selected_index;
            let is_active = active_provider.is_some_and(|ap| ap.eq_ignore_ascii_case(p));
            let prefix = if is_selected { " › " } else { "   " };
            let style = if is_selected {
                Style::default()
                    .fg(theme.bg_primary)
                    .bg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let is_local = p == "ollama" || p == "lmstudio" || p == "vllm" || p == "localhost";
            let suffix = if p == "favorites" {
                " [Saved Favorite Models across Providers]"
            } else if is_local {
                " [Localhost - Key Optional]"
            } else {
                ""
            };
            let mut spans = vec![Span::styled(format!("{}{}{}", prefix, p, suffix), style)];
            if is_active {
                spans.push(Span::styled(
                    " [active]",
                    if is_selected {
                        style
                    } else {
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD)
                    },
                ));
            }
            ListItem::new(Line::from(spans)).style(if is_selected {
                Style::default().bg(theme.brand_accent)
            } else {
                Style::default()
            })
        })
        .collect();

    let list = List::new(items).highlight_style(Style::default().bg(theme.brand_accent));
    frame.render_widget(list, chunks[0]);

    let back_hint = match parent_label {
        Some(parent) => format!("← back to {}", parent),
        None => "← back".to_string(),
    };
    let footer = Line::from(vec![
        Span::styled("  ↑/↓ ", Style::default().fg(theme.brand_accent)),
        Span::styled("navigate   ", Style::default().fg(theme.muted)),
        Span::styled("Enter ", Style::default().fg(theme.brand_accent)),
        Span::styled("browse models   ", Style::default().fg(theme.muted)),
        Span::styled("Esc/Bksp ", Style::default().fg(theme.brand_accent)),
        Span::styled(back_hint, Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer), chunks[1]);
}

// Re-export for backward compatibility
#[allow(unused_imports)]
pub use super::model_select::render_model_select;
