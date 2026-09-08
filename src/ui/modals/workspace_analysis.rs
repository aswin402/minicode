//! Workspace index analysis and repository scan modal rendering.

use crate::constants::{WORKSPACE_ANALYSIS_HEIGHT, WORKSPACE_ANALYSIS_WIDTH};
use crate::ui::layout_utils::centered_rect_exact;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

#[allow(clippy::too_many_arguments)]
pub fn render_workspace_analysis(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    workspace_path: &str,
    is_indexed: bool,
    cached_symbols_count: usize,
    cached_files_count: usize,
    selected_index: usize,
) {
    let popup_area = centered_rect_exact(WORKSPACE_ANALYSIS_WIDTH, WORKSPACE_ANALYSIS_HEIGHT, area);
    frame.render_widget(Clear, popup_area);

    let title = format!(" ✨ minicode v{} ", env!("CARGO_PKG_VERSION"));
    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_elevated),
        )
        .style(Style::default().bg(theme.bg_elevated));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header info
            Constraint::Length(1), // Spacer
            Constraint::Min(3),    // Options
        ])
        .split(inner);

    let cache_status_span = if is_indexed {
        Span::styled(
            format!(
                "Indexed ({} syms, {} files)",
                cached_symbols_count, cached_files_count
            ),
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "Unindexed",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        )
    };

    let header_line = Line::from(vec![
        Span::raw(" 📁 "),
        Span::styled(workspace_path, Style::default().fg(theme.text_primary)),
        Span::styled("  │  🔍 Cache: ", Style::default().fg(theme.muted)),
        cache_status_span,
    ]);
    frame.render_widget(Paragraph::new(header_line), chunks[0]);

    let options: Vec<(&str, &str, &str)> = if !is_indexed {
        vec![
            (
                "[1] ⚡ Quick Index",
                "AST symbols & PageRank graph",
                "[ENTER]",
            ),
            (
                "[2] 🧠 Deep Scan",
                "Symbol graph & architecture scan",
                "[2]",
            ),
            ("[3] ⏩ Skip", "Instant lightweight chat", "[ESC]"),
        ]
    } else {
        vec![
            (
                "[1] ⚡ Incremental Sync",
                "Scan modified files only (~5ms)",
                "[ENTER]",
            ),
            ("[2] 🔄 Full Rebuild", "Cold re-index from scratch", "[2]"),
            (
                "[3] 🗺️ View Repo Map",
                "Display AST centrality hierarchy",
                "[3]",
            ),
            ("[4] ⏩ Close", "Resume chat without re-indexing", "[ESC]"),
        ]
    };

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (tag, desc, key))| {
            let is_selected = i == selected_index;
            let prefix = if is_selected { "  ❯ " } else { "    " };

            let line = if is_selected {
                Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        *tag,
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  ({})", desc),
                        Style::default().fg(theme.text_primary),
                    ),
                    Span::styled(
                        format!("  {:>8}", key),
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(*tag, Style::default().fg(theme.text_primary)),
                    Span::styled(format!("  ({})", desc), Style::default().fg(theme.muted)),
                    Span::styled(format!("  {:>8}", key), Style::default().fg(theme.muted)),
                ])
            };

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[2]);
}
