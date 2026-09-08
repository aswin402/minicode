//! Exit confirmation dialog modal rendering.

use crate::constants::{EXIT_CONFIRM_MODAL_HEIGHT, EXIT_CONFIRM_MODAL_WIDTH};
use crate::ui::layout_utils::centered_rect_exact;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn render_exit_confirm(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    workspace_name: &str,
    selected_yes: bool,
) {
    let width = EXIT_CONFIRM_MODAL_WIDTH.min(area.width.saturating_sub(2));
    let height = EXIT_CONFIRM_MODAL_HEIGHT.min(area.height.saturating_sub(2));
    let popup_area = centered_rect_exact(width, height, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " ⏻ Exit minicode ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )]))
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.brand_accent))
        .style(Style::default().bg(theme.bg_elevated));

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 0: ● Workspace: name (vX.X.X)
            Constraint::Length(1), // 1: ● Session: auto-saved to .minicode/history
            Constraint::Length(1), // 2: Question "Are you sure you want to quit?"
            Constraint::Length(1), // 3: Spacer
            Constraint::Length(1), // 4: Buttons row "[ ✖ Yep, Quit ]   [ ✔ Stay in Session ]"
        ])
        .split(inner_area);

    // 0: Workspace info
    let ws_line = Line::from(vec![
        Span::styled(" ● ", Style::default().fg(theme.success)),
        Span::styled("Workspace: ", Style::default().fg(theme.muted)),
        Span::styled(
            workspace_name,
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" (v{})", env!("CARGO_PKG_VERSION")),
            Style::default().fg(theme.muted),
        ),
    ]);
    frame.render_widget(Paragraph::new(ws_line), chunks[0]);

    // 1: Session persistence info
    let sess_line = Line::from(vec![
        Span::styled(" ● ", Style::default().fg(theme.info)),
        Span::styled("Session: ", Style::default().fg(theme.muted)),
        Span::styled(
            "auto-saved to .minicode/history",
            Style::default().fg(theme.muted),
        ),
    ]);
    frame.render_widget(Paragraph::new(sess_line), chunks[1]);

    // 2: Question
    let question_p = Paragraph::new(Line::from(vec![Span::styled(
        "Are you sure you want to quit?",
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    frame.render_widget(question_p, chunks[2]);

    // 4: Buttons
    let yep_style = if selected_yes {
        Style::default()
            .bg(theme.destructive)
            .fg(theme.bg_primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(theme.bg_primary).fg(theme.text_primary)
    };

    let nope_style = if !selected_yes {
        Style::default()
            .bg(theme.brand_accent)
            .fg(theme.bg_primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(theme.bg_primary).fg(theme.text_primary)
    };

    let yep_spans = if selected_yes {
        vec![Span::styled("  ✖ Yep, Quit (Y)  ", yep_style)]
    } else {
        vec![
            Span::styled("  ✖ ", yep_style),
            Span::styled("Y", yep_style.add_modifier(Modifier::UNDERLINED)),
            Span::styled("ep, Quit  ", yep_style),
        ]
    };

    let nope_spans = if !selected_yes {
        vec![Span::styled("  ✔ Stay in Session (N)  ", nope_style)]
    } else {
        vec![
            Span::styled("  ✔ Stay in Session (", nope_style),
            Span::styled("N", nope_style.add_modifier(Modifier::UNDERLINED)),
            Span::styled(")  ", nope_style),
        ]
    };

    let mut buttons_line = Vec::new();
    buttons_line.extend(yep_spans);
    buttons_line.push(Span::raw("   "));
    buttons_line.extend(nope_spans);

    let buttons_p = Paragraph::new(Line::from(buttons_line)).alignment(Alignment::Center);
    frame.render_widget(buttons_p, chunks[4]);
}
