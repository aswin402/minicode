//! Help and keyboard shortcuts modal rendering.

use crate::constants::{HELP_HEIGHT_PCT, HELP_WIDTH_PCT};
use crate::ui::layout_utils::centered_rect;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn render_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let popup_area = centered_rect(HELP_WIDTH_PCT, HELP_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from(vec![Span::styled(
            "⚡ minicode Help & Keyboard Shortcuts",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  /model     ", Style::default().fg(theme.success)),
            Span::styled(
                "Choose LLM model & provider interactively",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  /theme     ", Style::default().fg(theme.success)),
            Span::styled(
                "Switch TUI color theme palette interactively",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  /undo      ", Style::default().fg(theme.success)),
            Span::styled(
                "Revert all file modifications from previous turn",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  /sessions  ", Style::default().fg(theme.success)),
            Span::styled(
                "Browse & reload past workspace session history",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  /copy      ", Style::default().fg(theme.success)),
            Span::styled(
                "Copy latest AI response to clipboard (/copy all for whole chat)",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  /clear     ", Style::default().fg(theme.success)),
            Span::styled(
                "Clear conversation timeline display",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  /exit      ", Style::default().fg(theme.success)),
            Span::styled(
                "Quit minicode interactive session",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Shift+Drag ", Style::default().fg(theme.warning)),
            Span::styled(
                "Select and copy text in terminal directly",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Enter      ", Style::default().fg(theme.warning)),
            Span::styled(
                "Submit prompt or confirm action",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+J     ", Style::default().fg(theme.warning)),
            Span::styled(
                "Insert newline (multi-line prompt)",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  PgUp / PgDn", Style::default().fg(theme.warning)),
            Span::styled(
                "Scroll timeline (or Shift+↑/↓, Mouse wheel)",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Home / End ", Style::default().fg(theme.warning)),
            Span::styled(
                "Scroll directly to top / bottom of timeline",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+T     ", Style::default().fg(theme.warning)),
            Span::styled(
                "Toggle embedded PTY terminal drawer",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Esc        ", Style::default().fg(theme.warning)),
            Span::styled(
                "Interrupt running execution / close modal",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press Esc or Enter to close",
            Style::default().fg(theme.muted),
        )]),
    ];

    let block = Block::default()
        .title(" minicode Help ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_elevated),
        )
        .style(Style::default().bg(theme.bg_elevated));

    let p = Paragraph::new(help_text).block(block);
    frame.render_widget(p, popup_area);
}
