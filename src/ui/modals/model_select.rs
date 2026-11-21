//! Model picker modal rendering with search filtering and context size badges.

use crate::agent::models::ModelInfo;
use crate::constants::{
    MODAL_SEARCH_INPUT_HEIGHT, MODEL_SELECT_HEIGHT_PCT, MODEL_SELECT_WIDTH_PCT,
};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

/// Renders the model picker modal with search query filtering, pagination, and status badges.
#[allow(clippy::too_many_arguments)]
pub fn render_model_select(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    provider: &str,
    models: &[ModelInfo],
    filtered_indices: &[usize],
    selected_index: usize,
    filter: &str,
    loading: bool,
) {
    render_model_select_with_active(
        frame,
        area,
        theme,
        provider,
        models,
        filtered_indices,
        selected_index,
        filter,
        loading,
        None,
        None,
        None,
    );
}

/// Renders the model picker modal with active-model highlight and Back navigation hints.
#[allow(clippy::too_many_arguments)]
pub fn render_model_select_with_active(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    provider: &str,
    models: &[ModelInfo],
    filtered_indices: &[usize],
    selected_index: usize,
    filter: &str,
    loading: bool,
    active_provider: Option<&str>,
    active_model: Option<&str>,
    parent_label: Option<&str>,
) {
    let popup_area = centered_rect(MODEL_SELECT_WIDTH_PCT, MODEL_SELECT_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let outer_block = Block::default()
        .title(format!(" Select Model ({}) ", provider))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_elevated),
        )
        .style(Style::default().bg(theme.bg_elevated));

    let inner_area = outer_block.inner(popup_area);
    frame.render_widget(outer_block, popup_area);

    if loading {
        let loading_p = Paragraph::new(format!("Fetching live models from {} API...", provider))
            .style(Style::default().fg(theme.warning))
            .alignment(Alignment::Center);
        frame.render_widget(loading_p, inner_area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(MODAL_SEARCH_INPUT_HEIGHT), // Search / filter input
            Constraint::Min(4),                            // Models list
            Constraint::Length(1),                         // Footer hints
        ])
        .split(inner_area);

    // Search box
    let search_text = format!(" Search: {}█", filter);
    let search_box = Paragraph::new(search_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border))
                .title(" Filter Models  —  [* / Ctrl+F] Favorite  ·  [Tab] Favorites "),
        )
        .style(Style::default().fg(theme.text_primary));
    frame.render_widget(search_box, chunks[0]);

    // Models list with viewport scrolling
    let max_visible = chunks[1].height as usize;
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);

    let items: Vec<ListItem> = filtered_indices
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
        .map(|(visual_idx, &real_idx)| {
            let m = &models[real_idx];
            let is_selected = visual_idx == selected_index;
            let prefix = if is_selected { " › " } else { "   " };

            let is_active = match (active_provider, active_model) {
                (Some(ap), Some(am)) => {
                    if provider == "favorites" {
                        m.id.split_once('/')
                            .is_some_and(|(p, mdl)| p.eq_ignore_ascii_case(ap) && mdl == am)
                    } else {
                        provider.eq_ignore_ascii_case(ap) && m.id == am
                    }
                }
                _ => false,
            };

            let mut spans = vec![
                Span::raw(prefix),
                Span::styled(
                    &m.id,
                    if is_selected {
                        Style::default()
                            .fg(theme.bg_primary)
                            .bg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
            ];

            if is_active {
                spans.push(Span::styled(
                    " [active]",
                    if is_selected {
                        Style::default()
                            .fg(theme.bg_primary)
                            .bg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD)
                    },
                ));
            }

            let is_fav = provider == "favorites"
                || m.description
                    .as_deref()
                    .map(|d| d.contains("[fav]"))
                    .unwrap_or(false);
            if is_fav {
                spans.push(Span::styled(
                    " [fav]",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            if m.is_free {
                spans.push(Span::styled(
                    " [FREE]",
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            if let Some(ctx) = m.context_length {
                spans.push(Span::styled(
                    format!(" ({}k ctx)", ctx / 1000),
                    Style::default().fg(theme.muted),
                ));
            }

            let item_style = if is_selected {
                Style::default().bg(theme.brand_accent)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(spans)).style(item_style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[1]);

    let back_hint = match parent_label {
        Some(parent) => format!("← back to {}", parent),
        None => "← back".to_string(),
    };
    let footer = Line::from(vec![
        Span::styled("  ↑/↓ ", Style::default().fg(theme.brand_accent)),
        Span::styled("navigate   ", Style::default().fg(theme.muted)),
        Span::styled("Enter ", Style::default().fg(theme.brand_accent)),
        Span::styled("select (stay)   ", Style::default().fg(theme.muted)),
        Span::styled("*/Ctrl+F ", Style::default().fg(theme.brand_accent)),
        Span::styled("fav   ", Style::default().fg(theme.muted)),
        Span::styled("Esc ", Style::default().fg(theme.brand_accent)),
        Span::styled(back_hint, Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer), chunks[2]);
}
