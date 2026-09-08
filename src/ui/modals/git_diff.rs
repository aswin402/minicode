//! Interactive git diff inspection modal rendering.

use crate::constants::{GIT_DIFF_HEIGHT_PCT, GIT_DIFF_WIDTH_PCT};
use crate::git::GitDiffFile;
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

pub fn render_git_diff(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    diff_files: &[GitDiffFile],
    selected_file_index: usize,
    scroll_offset: usize,
    staged_view: bool,
) {
    let modal_area = centered_rect(GIT_DIFF_WIDTH_PCT, GIT_DIFF_HEIGHT_PCT, area);
    frame.render_widget(Clear, modal_area);

    let title = if staged_view {
        " 🔍 Staged Git Changes (--cached) "
    } else {
        " 🔍 Working Tree Git Changes (unstaged) "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )
        .title(title);
    frame.render_widget(block, modal_area);

    let inner = modal_area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(32), // Files list
            Constraint::Percentage(68), // Diff viewer
        ])
        .split(inner);

    let max_visible = (h_chunks[0].height.saturating_sub(2) as usize).max(1);
    let file_scroll_offset = compute_scroll_offset(selected_file_index, max_visible);

    // Left: Files list with viewport scrolling
    let items: Vec<ListItem> = diff_files
        .iter()
        .enumerate()
        .skip(file_scroll_offset)
        .take(max_visible)
        .map(|(idx, f)| {
            let is_selected = idx == selected_file_index;
            let prefix = if is_selected { " › " } else { "   " };
            let status_badge = format!("[{}] ", f.status_char);
            let stats = format!(" +{} -{}", f.additions, f.deletions);

            let badge_style = match f.status_char {
                'A' => Style::default().fg(theme.success),
                'D' => Style::default().fg(theme.destructive),
                'M' => Style::default().fg(theme.warning),
                _ => Style::default().fg(theme.muted),
            };

            let spans = vec![
                Span::raw(prefix),
                Span::styled(status_badge, badge_style),
                Span::styled(
                    &f.path,
                    if is_selected {
                        Style::default()
                            .fg(theme.bg_primary)
                            .bg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(stats, Style::default().fg(theme.muted)),
            ];

            let item_style = if is_selected {
                Style::default().bg(theme.brand_accent).fg(theme.bg_primary)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(spans)).style(item_style)
        })
        .collect();

    let files_title = format!(" Modified Files ({}) ", diff_files.len());
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(files_title);
    let list = List::new(items).block(list_block);
    frame.render_widget(list, h_chunks[0]);

    // Right: Diff content
    let diff_title = if !diff_files.is_empty() && selected_file_index < diff_files.len() {
        format!(" Diff: {} ", diff_files[selected_file_index].path)
    } else {
        " Diff Preview ".to_string()
    };

    let preview_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(diff_title);

    let mut diff_lines = Vec::new();
    if !diff_files.is_empty() && selected_file_index < diff_files.len() {
        let cur_f = &diff_files[selected_file_index];
        if cur_f.lines.is_empty() {
            diff_lines.push(Line::from(Span::styled(
                " (No line diffs recorded) ",
                Style::default().fg(theme.muted),
            )));
        } else {
            for l in cur_f.lines.iter().skip(scroll_offset) {
                let (style, prefix_char) = match l.tag {
                    '+' => (Style::default().fg(theme.success), "+ "),
                    '-' => (Style::default().fg(theme.destructive), "- "),
                    '@' => (
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                        "  ",
                    ),
                    _ => (Style::default().fg(theme.text_primary), "  "),
                };

                let lineno_str = match (l.old_lineno, l.new_lineno) {
                    (Some(o), Some(n)) => format!("{:>4} {:>4} │ ", o, n),
                    (Some(o), None) => format!("{:>4}      │ ", o),
                    (None, Some(n)) => format!("     {:>4} │ ", n),
                    (None, None) => "          │ ".to_string(),
                };

                diff_lines.push(Line::from(vec![
                    Span::styled(lineno_str, Style::default().fg(theme.muted)),
                    Span::styled(prefix_char, style),
                    Span::styled(&l.content, style),
                ]));
            }
        }
    } else {
        diff_lines.push(Line::from(Span::styled(
            " Working tree is clean — no diffs found. ",
            Style::default().fg(theme.muted),
        )));
    }

    let diff_p = Paragraph::new(diff_lines).block(preview_block);
    frame.render_widget(diff_p, h_chunks[1]);
}
