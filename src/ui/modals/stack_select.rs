//! Native onpkg stack selection wizard modal rendering.

use crate::constants::{STACK_PREVIEW_MAX_FILES, STACK_SELECT_HEIGHT_PCT, STACK_SELECT_WIDTH_PCT};
use crate::tools::onpkg::stacks::Stack;
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

pub fn render_stack_select(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    stacks: &[Stack],
    filtered_indices: &[usize],
    selected_index: usize,
    filter: &str,
) {
    let popup_area = centered_rect(STACK_SELECT_WIDTH_PCT, STACK_SELECT_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let outer_block = Block::default()
        .title(" 📦 Native onpkg Stack Wizard ")
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

    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(crate::constants::MODAL_SEARCH_INPUT_HEIGHT), // Filter search input
            Constraint::Min(6),                                              // 2-column main area
        ])
        .split(inner_area);

    // Search box
    let search_text = format!(" Filter: {}█", filter);
    let search_box = Paragraph::new(search_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border))
                .title(" Search Stacks by name / tech / runtime "),
        )
        .style(Style::default().fg(theme.text_primary));
    frame.render_widget(search_box, v_chunks[0]);

    // Split middle area horizontally (List vs Preview)
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(crate::constants::MODAL_SPLIT_PRIMARY_PERCENT),
            Constraint::Percentage(crate::constants::MODAL_SPLIT_SECONDARY_PERCENT),
        ])
        .split(v_chunks[1]);

    let max_visible = (h_chunks[0].height.saturating_sub(2) as usize).max(1);
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);

    // Left: Stacks List with viewport scrolling
    let items: Vec<ListItem> = filtered_indices
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
        .map(|(visual_idx, &real_idx)| {
            let s = &stacks[real_idx];
            let is_selected = visual_idx == selected_index;
            let prefix = if is_selected { " › " } else { "   " };

            let runtime_badge = format!(" [{}]", s.runtime);
            let file_info = format!(" ({} files)", s.files.len());

            let spans = vec![
                Span::raw(prefix),
                Span::styled(
                    &s.name,
                    if is_selected {
                        Style::default()
                            .fg(theme.bg_primary)
                            .bg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    runtime_badge,
                    if is_selected {
                        Style::default().fg(theme.bg_primary).bg(theme.brand_accent)
                    } else {
                        Style::default().fg(theme.brand_accent)
                    },
                ),
                Span::styled(
                    file_info,
                    if is_selected {
                        Style::default().fg(theme.bg_primary).bg(theme.brand_accent)
                    } else {
                        Style::default().fg(theme.muted)
                    },
                ),
            ];

            let item_style = if is_selected {
                Style::default().bg(theme.brand_accent)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(spans)).style(item_style)
        })
        .collect();

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(format!(" Stacks ({}) ", filtered_indices.len()));
    let list = List::new(items).block(list_block);
    frame.render_widget(list, h_chunks[0]);

    // Right: Preview of Selected Stack
    let preview_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(" Stack Preview ");

    let mut preview_lines = Vec::new();
    if !filtered_indices.is_empty() && selected_index < filtered_indices.len() {
        let s = &stacks[filtered_indices[selected_index]];

        preview_lines.push(Line::from(vec![
            Span::styled("📦 ", Style::default()),
            Span::styled(
                &s.name,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  (Runtime: {})", s.runtime),
                Style::default().fg(theme.success),
            ),
        ]));
        preview_lines.push(Line::from(""));
        preview_lines.push(Line::from(vec![
            Span::styled("📝 ", Style::default()),
            Span::styled(&s.description, Style::default().fg(theme.text_primary)),
        ]));
        preview_lines.push(Line::from(""));

        let pkgs_str = if s.packages.is_empty() {
            "none".to_string()
        } else {
            s.packages.join(", ")
        };
        preview_lines.push(Line::from(vec![
            Span::styled(
                "⚡ Packages: ",
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(pkgs_str, Style::default().fg(theme.text_primary)),
        ]));
        preview_lines.push(Line::from(""));

        preview_lines.push(Line::from(vec![Span::styled(
            format!("📁 File Structure ({} files):", s.files.len()),
            Style::default().fg(theme.brand_accent),
        )]));

        for f in s.files.iter().take(STACK_PREVIEW_MAX_FILES) {
            preview_lines.push(Line::from(vec![
                Span::styled("  ├── ", Style::default().fg(theme.muted)),
                Span::styled(&f.path, Style::default().fg(theme.text_primary)),
            ]));
        }
        if s.files.len() > STACK_PREVIEW_MAX_FILES {
            preview_lines.push(Line::from(vec![Span::styled(
                format!(
                    "  ╰── ... and {} more files",
                    s.files.len() - STACK_PREVIEW_MAX_FILES
                ),
                Style::default().fg(theme.muted),
            )]));
        }
    } else {
        preview_lines.push(Line::from(Span::styled(
            "No stack selected",
            Style::default().fg(theme.muted),
        )));
    }

    let preview_p = Paragraph::new(preview_lines).block(preview_block);
    frame.render_widget(preview_p, h_chunks[1]);
}
