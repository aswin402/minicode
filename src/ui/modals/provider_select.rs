//! Provider selection and model picker modal rendering.

use crate::constants::{PROVIDER_SELECT_HEIGHT_PCT, PROVIDER_SELECT_WIDTH_PCT};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};
use ratatui::Frame;

pub fn render_provider_select(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    providers: &[String],
    selected_index: usize,
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
    let max_visible = (inner_area.height as usize).max(1);
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);

    let items: Vec<ListItem> = providers
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
        .map(|(i, p)| {
            let is_selected = i == selected_index;
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
            let suffix = if is_local {
                " [Localhost - Key Optional]"
            } else {
                ""
            };
            ListItem::new(format!("{}{}{}", prefix, p, suffix)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(theme.brand_accent));

    frame.render_widget(list, popup_area);
}

// Re-export for backward compatibility
#[allow(unused_imports)]
pub use super::model_select::render_model_select;
