//! Turn checkpoint reversal modal rendering.

use crate::constants::{
    CHECKPOINT_FILES_PREVIEW, CHECKPOINT_PROMPT_MAX_CHARS, CHECKPOINT_PROMPT_PREVIEW_CHARS,
    UNDO_CHECKPOINT_HEIGHT_PCT, UNDO_CHECKPOINT_MAX_VISIBLE, UNDO_CHECKPOINT_WIDTH_PCT,
};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::{compute_scroll_offset, TurnCheckpointInfo};
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn render_undo_checkpoint(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    checkpoints: &[TurnCheckpointInfo],
    selected_index: usize,
) {
    let popup_area = centered_rect(UNDO_CHECKPOINT_WIDTH_PCT, UNDO_CHECKPOINT_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    let total = checkpoints.len();
    let max_visible = UNDO_CHECKPOINT_MAX_VISIBLE;
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);
    let visible_checkpoints = checkpoints.iter().skip(scroll_offset).take(max_visible);

    for (idx_rel, cp) in visible_checkpoints.enumerate() {
        let idx = scroll_offset + idx_rel;
        let is_selected = idx == selected_index;
        let is_last = idx == total.saturating_sub(1);

        let node_sym = if is_selected {
            "  ◉─ "
        } else {
            "  ○─ "
        };
        let node_style = if is_selected {
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };

        let turn_badge = if cp.is_latest {
            format!("[Turn {}] (Latest)", cp.turn_id)
        } else {
            format!("[Turn {}]", cp.turn_id)
        };

        let badge_style = if is_selected {
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };

        lines.push(Line::from(vec![
            Span::styled(node_sym, node_style),
            Span::styled(turn_badge, badge_style),
        ]));

        let prompt_style = if is_selected {
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        let prompt_display = if cp.prompt.chars().count() > CHECKPOINT_PROMPT_MAX_CHARS {
            format!(
                "{}...",
                cp.prompt
                    .chars()
                    .take(CHECKPOINT_PROMPT_PREVIEW_CHARS)
                    .collect::<String>()
            )
        } else {
            cp.prompt.clone()
        };

        lines.push(Line::from(vec![
            Span::styled(
                "  │   ",
                if is_last {
                    Style::default().fg(theme.bg_elevated)
                } else {
                    Style::default().fg(theme.muted)
                },
            ),
            Span::styled(format!("\"{}\"", prompt_display), prompt_style),
        ]));

        let file_summary = if cp.files.is_empty() {
            "0 files modified".to_string()
        } else {
            let files_str: Vec<&str> = cp
                .files
                .iter()
                .map(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(p)
                })
                .take(CHECKPOINT_FILES_PREVIEW)
                .collect();
            let more = if cp.files.len() > CHECKPOINT_FILES_PREVIEW {
                format!(" +{} more", cp.files.len() - CHECKPOINT_FILES_PREVIEW)
            } else {
                String::new()
            };
            format!(
                "{} file(s) ({}{})",
                cp.files.len(),
                files_str.join(", "),
                more
            )
        };

        let meta_text = format!("└─ {} • {}", cp.time_ago, file_summary);
        lines.push(Line::from(vec![
            Span::styled(
                "  │   ",
                if is_last {
                    Style::default().fg(theme.bg_elevated)
                } else {
                    Style::default().fg(theme.muted)
                },
            ),
            Span::styled(
                meta_text,
                Style::default().fg(if is_selected {
                    theme.brand_accent
                } else {
                    theme.muted
                }),
            ),
        ]));

        if !is_last {
            lines.push(Line::from(vec![Span::styled(
                "  │",
                Style::default().fg(theme.muted),
            )]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "  [Enter] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Revert to selected checkpoint   ",
            Style::default().fg(theme.text_primary),
        ),
        Span::styled("[Esc] ", Style::default().fg(theme.muted)),
        Span::styled("Cancel", Style::default().fg(theme.muted)),
    ]));

    let block = Block::default()
        .title(" Undo to Checkpoint ")
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
