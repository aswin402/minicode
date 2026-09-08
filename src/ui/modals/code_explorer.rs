//! Surgical AST CodeGraph explorer modal rendering.

use crate::constants::{
    CODE_EXPLORER_HEIGHT_PCT, CODE_EXPLORER_WIDTH_PCT, EXPLORER_BADGE_DISPLAY_COLS,
};
use crate::context::explorer::CodeExploreMatch;
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

#[allow(clippy::too_many_arguments)]
pub fn render_code_explorer(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    symbols: &[CodeExploreMatch],
    filtered_indices: &[usize],
    selected_index: usize,
    filter: &str,
    active_tab: usize,
) {
    let popup_area = centered_rect(CODE_EXPLORER_WIDTH_PCT, CODE_EXPLORER_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let outer_block = Block::default()
        .title(" 🧭 CodeGraph Surgical Explorer (Ctrl+E) ")
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
            Constraint::Length(crate::constants::MODAL_SEARCH_INPUT_HEIGHT), // Search box
            Constraint::Min(5), // Main content (2 cols)
        ])
        .split(inner_area);

    // Search input
    let search_text = format!(" Filter symbols/layers: {}█", filter);
    let search_box = Paragraph::new(search_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border)),
        )
        .style(Style::default().fg(theme.text_primary));
    frame.render_widget(search_box, v_chunks[0]);

    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35), // Left: Symbols list
            Constraint::Percentage(65), // Right: Details & Call graph
        ])
        .split(v_chunks[1]);

    let max_visible = (h_chunks[0].height.saturating_sub(2) as usize).max(1);
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);

    // Left: Symbols list with viewport scrolling
    let items: Vec<ListItem> = filtered_indices
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
        .map(|(display_idx, &real_idx)| {
            let sym = &symbols[real_idx];
            let is_selected = display_idx == selected_index;

            let spans = vec![
                Span::styled(
                    format!(
                        "{:<width$} ",
                        sym.layer.badge(),
                        width = EXPLORER_BADGE_DISPLAY_COLS
                    ),
                    Style::default().fg(theme.brand_accent),
                ),
                Span::styled(
                    &sym.symbol_name,
                    if is_selected {
                        Style::default()
                            .fg(theme.bg_primary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD)
                    },
                ),
                Span::styled(format!(" [{}]", sym.kind), Style::default().fg(theme.muted)),
            ];

            let item_style = if is_selected {
                Style::default().bg(theme.brand_accent).fg(theme.bg_primary)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(spans)).style(item_style)
        })
        .collect();

    let list_title = format!(" Symbols ({}) ", filtered_indices.len());
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(list_title);
    let list = List::new(items).block(list_block);
    frame.render_widget(list, h_chunks[0]);

    // Right: Detail view with tabs
    let detail_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(" Surgical AST Inspection ");

    let mut detail_lines = Vec::new();

    if let Some(&real_idx) = filtered_indices.get(selected_index) {
        let sym = &symbols[real_idx];

        // Header line
        detail_lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", sym.layer.badge()),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                &sym.symbol_name,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " ({}) in {}:{}-{}",
                    sym.kind, sym.file_path, sym.line_range.0, sym.line_range.1
                ),
                Style::default().fg(theme.muted),
            ),
        ]));

        // Signature
        detail_lines.push(Line::from(vec![
            Span::styled("Signature: ", Style::default().fg(theme.warning)),
            Span::styled(&sym.signature, Style::default().fg(theme.text_primary)),
        ]));

        if let Some(doc) = &sym.doc_comment {
            detail_lines.push(Line::from(vec![
                Span::styled("Doc: ", Style::default().fg(theme.muted)),
                Span::styled(doc.trim(), Style::default().fg(theme.muted)),
            ]));
        }
        detail_lines.push(Line::from(""));

        // Tab selector bar
        let tab_titles = [
            "[ 1. Source Definition ]".to_string(),
            format!("[ 2. Callers ({}) ]", sym.callers.len()),
            format!("[ 3. Callees ({}) ]", sym.callees.len()),
            format!("[ 4. Blast Radius ({}) ]", sym.blast_radius_files.len()),
        ];

        let mut tab_spans = Vec::new();
        for (t_idx, t_title) in tab_titles.iter().enumerate() {
            let is_active_tab = t_idx == active_tab;
            tab_spans.push(Span::styled(
                format!("{} ", t_title),
                if is_active_tab {
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(theme.muted)
                },
            ));
        }
        detail_lines.push(Line::from(tab_spans));
        detail_lines.push(Line::from(""));

        // Tab contents
        match active_tab {
            0 => {
                if let Some(src) = &sym.source_code {
                    let start_line = sym.line_range.0;
                    for (i, src_line) in src.lines().enumerate() {
                        detail_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:>4} │ ", start_line + i),
                                Style::default().fg(theme.muted),
                            ),
                            Span::styled(src_line, Style::default().fg(theme.text_primary)),
                        ]));
                    }
                } else {
                    detail_lines.push(Line::from(Span::styled(
                        " (Source code not loaded) ",
                        Style::default().fg(theme.muted),
                    )));
                }
            }
            1 => {
                if sym.callers.is_empty() {
                    detail_lines.push(Line::from(Span::styled(
                        " No incoming callers found (Root entrypoint / public symbol).",
                        Style::default().fg(theme.muted),
                    )));
                } else {
                    for c in &sym.callers {
                        detail_lines.push(Line::from(vec![
                            Span::styled("  ← ", Style::default().fg(theme.success)),
                            Span::styled(
                                &c.name,
                                Style::default()
                                    .fg(theme.text_primary)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(" in {}:{}", c.file_path, c.line),
                                Style::default().fg(theme.muted),
                            ),
                        ]));
                    }
                }
            }
            2 => {
                if sym.callees.is_empty() {
                    detail_lines.push(Line::from(Span::styled(
                        " No outgoing calls found (Leaf function).",
                        Style::default().fg(theme.muted),
                    )));
                } else {
                    for c in &sym.callees {
                        detail_lines.push(Line::from(vec![
                            Span::styled("  → ", Style::default().fg(theme.brand_accent)),
                            Span::styled(
                                &c.name,
                                Style::default()
                                    .fg(theme.text_primary)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(" in {}:{}", c.file_path, c.line),
                                Style::default().fg(theme.muted),
                            ),
                        ]));
                    }
                }
            }
            _ => {
                if sym.blast_radius_files.is_empty() {
                    detail_lines.push(Line::from(Span::styled(
                        " Isolated module (No direct downstream dependents).",
                        Style::default().fg(theme.success),
                    )));
                } else {
                    detail_lines.push(Line::from(Span::styled(
                        "Files directly dependent on this symbol:",
                        Style::default().fg(theme.warning),
                    )));
                    for file in &sym.blast_radius_files {
                        detail_lines.push(Line::from(vec![
                            Span::styled("  • ", Style::default().fg(theme.destructive)),
                            Span::styled(file, Style::default().fg(theme.text_primary)),
                        ]));
                    }
                }
            }
        }
    } else {
        detail_lines.push(Line::from(Span::styled(
            " No matching AST symbols found. ",
            Style::default().fg(theme.muted),
        )));
    }

    let detail_p = Paragraph::new(detail_lines).block(detail_block);
    frame.render_widget(detail_p, h_chunks[1]);
}
