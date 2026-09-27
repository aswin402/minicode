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
    _workspace_name: &str,
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
            Constraint::Length(1), // 0: Top spacer
            Constraint::Length(1), // 1: Question "Are you sure you want to quit?"
            Constraint::Length(1), // 2: Spacer
            Constraint::Length(1), // 3: Buttons row "[ ✖ Yep, Quit ]   [ ✔ Stay in Session ]"
            Constraint::Min(0),    // 4: Bottom spacer
        ])
        .split(inner_area);

    // Question
    let question_p = Paragraph::new(Line::from(vec![Span::styled(
        "Are you sure you want to quit?",
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    frame.render_widget(question_p, chunks[1]);

    // Buttons
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
    frame.render_widget(buttons_p, chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_render_exit_confirm_minimal() {
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                render_exit_confirm(f, area, &theme, "my-project", false);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(rendered.contains("Exit minicode"));
        assert!(rendered.contains("Are you sure you want to quit?"));
        assert!(rendered.contains("Yep, Quit"));
        assert!(rendered.contains("Stay in Session"));
        // Confirm workspace and session strings are removed
        assert!(!rendered.contains("Workspace:"));
        assert!(!rendered.contains("auto-saved to .minicode/history"));
    }
}
