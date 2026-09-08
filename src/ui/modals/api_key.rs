//! API key input configuration modal rendering.

use crate::ui::layout_utils::centered_rect;
use crate::ui::theme::Theme;
use crate::utils::strings::mask_secret;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn render_api_key_input(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    provider: &str,
    env_var: &str,
    input: &str,
) {
    let popup_area = centered_rect(65, 30, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(format!(" Configure API Key: {} ", provider))
        .title_alignment(Alignment::Center)
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
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(inner);

    let hint_line = Line::from(vec![
        Span::styled("Environment variable: ", Style::default().fg(theme.muted)),
        Span::styled(
            env_var,
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  (Enter to save, Esc to cancel)",
            Style::default().fg(theme.muted),
        ),
    ]);
    frame.render_widget(Paragraph::new(hint_line), chunks[0]);

    let display_text = if input.is_empty() {
        Span::styled(
            "Paste or type API key here...",
            Style::default().fg(theme.muted),
        )
    } else {
        let masked = mask_secret(input, 4);
        Span::styled(masked, Style::default().fg(theme.text_primary))
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.brand_accent));
    frame.render_widget(Paragraph::new(display_text).block(input_block), chunks[1]);
}
