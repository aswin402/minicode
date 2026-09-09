//! Minimalist Zen Spotlight welcome screen rendering.
//!
//! Renders the initial start screen when the conversation timeline is empty, featuring:
//! - Top header with version and title
//! - Centered brand logo lockup (Option A: Stepped geometric pillars + typography)
//! - Centered hero input dock
//! - Live status & model readiness strip
//! - Bottom workspace and git context

use crate::ui::input::InputDock;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::path::Path;

/// Context parameters required for rendering the welcome screen.
pub struct WelcomeContext<'a> {
    pub workspace: &'a Path,
    pub provider: &'a str,
    pub model: &'a str,
}

/// Helper formatting provider name to readable Title Case.
fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Helper normalizing model name for clean header display.
fn format_model_name(model: &str) -> String {
    // If model contains slash (e.g. openrouter/anthropic/claude-3.7), pick the leaf
    let name = model.split('/').next_back().unwrap_or(model);
    // Replace hyphens with spaces if uppercase letters aren't already present
    if name.chars().any(|c| c.is_ascii_uppercase()) {
        name.to_string()
    } else {
        name.split('-')
            .map(title_case)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Renders the Zen Spotlight welcome screen.
pub fn render_welcome_screen(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    ctx: &WelcomeContext<'_>,
    input_dock: &InputDock,
) -> Rect {
    // 1. Fill entire background with theme's primary background
    let bg_block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(theme.bg_primary));
    frame.render_widget(bg_block, area);

    let input_height = input_dock.required_height();

    // Fallback for extremely constrained terminal dimensions
    if area.height < 10 || area.width < 30 {
        let fallback_input = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(input_height) / 2,
            width: area.width,
            height: input_height.min(area.height),
        };
        input_dock.render(frame, fallback_input, theme);
        return fallback_input;
    }

    // Vertical layout hierarchy
    let vert_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),            // 0: Top Header
            Constraint::Min(1),               // 1: Top spacer (flexible)
            Constraint::Length(3),            // 2: Brand logo lockup (3 lines)
            Constraint::Length(1),            // 3: Spacer between logo and input dock
            Constraint::Length(input_height), // 4: Centered Dynamic Input Dock
            Constraint::Length(1),            // 5: Spacer between input and status
            Constraint::Length(1),            // 6: Status strip (● READY | Model)
            Constraint::Min(2),               // 7: Bottom spacer (flexible)
            Constraint::Length(1),            // 8: Bottom Edge Bar
        ])
        .split(area);

    // 1. Top Header: "MiniCode vX.X.X" (left) ... "AI Coding Assistant" (right)
    let version_str = format!("MiniCode v{}", env!("CARGO_PKG_VERSION"));
    let top_left = Span::styled(
        version_str,
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    );
    let top_right = Span::styled("AI Coding Assistant", Style::default().fg(theme.muted));

    let tl_width = (top_left.content.chars().count() + 2) as u16;
    let tr_width = (top_right.content.chars().count() + 2) as u16;

    if area.width >= tl_width + tr_width {
        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(tl_width),
                Constraint::Min(1),
                Constraint::Length(tr_width),
            ])
            .split(vert_chunks[0]);

        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::raw(" "), top_left])),
            top_chunks[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![top_right, Span::raw(" ")])).alignment(Alignment::Right),
            top_chunks[2],
        );
    }

    // 2. Brand Logo Lockup (Option A: Stepped geometric pillars + typography)
    // Logo block width:
    // Left pillar: 4 chars
    // Gap: 4 chars
    // Right pillar: 4 chars
    // Gap to text: 3 chars
    // "Build better, with AI": 21 chars
    // Total lockup width = 12 + 3 + 21 = 36 chars.
    let lockup_width = 36_u16;
    let lockup_area = if vert_chunks[2].width > lockup_width {
        let offset_x = (vert_chunks[2].width - lockup_width) / 2;
        Rect {
            x: vert_chunks[2].x + offset_x,
            y: vert_chunks[2].y,
            width: lockup_width,
            height: 3,
        }
    } else {
        vert_chunks[2]
    };

    let line1 = Line::from(vec![
        Span::styled("████", Style::default().fg(theme.brand_accent)),
        Span::raw("    "),
        Span::styled("████", Style::default().fg(theme.info)),
        Span::raw("   "),
        Span::styled(
            "MiniCode",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let line2 = Line::from(vec![
        Span::styled("████", Style::default().fg(theme.brand_accent)),
        Span::raw("    "),
        Span::styled("████", Style::default().fg(theme.info)),
        Span::raw("   "),
        Span::styled("Build better, with AI", Style::default().fg(theme.muted)),
    ]);

    let line3 = Line::from(vec![
        Span::styled("██  ", Style::default().fg(theme.brand_accent)),
        Span::raw("    "),
        Span::styled("  ██", Style::default().fg(theme.info)),
    ]);

    let logo_para = Paragraph::new(vec![line1, line2, line3]);
    frame.render_widget(logo_para, lockup_area);

    // 3. Centered Input Dock
    // Responsive width: 65% of screen width clamped between 42 and 74 columns
    let input_width = (vert_chunks[4].width * 65 / 100)
        .clamp(40, 74)
        .min(vert_chunks[4].width);
    let input_x = vert_chunks[4].x + (vert_chunks[4].width.saturating_sub(input_width)) / 2;
    let centered_input_rect = Rect {
        x: input_x,
        y: vert_chunks[4].y,
        width: input_width,
        height: vert_chunks[4].height,
    };
    input_dock.render(frame, centered_input_rect, theme);

    // 4. Live Status Strip (● READY | Provider Model)
    let provider_display = title_case(ctx.provider);
    let model_display = format_model_name(ctx.model);
    let status_spans = vec![
        Span::styled("● ", Style::default().fg(theme.success)),
        Span::styled(
            "READY",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} {}", provider_display, model_display),
            Style::default().fg(theme.text_primary),
        ),
    ];
    let status_para = Paragraph::new(Line::from(status_spans)).alignment(Alignment::Center);
    frame.render_widget(status_para, vert_chunks[6]);

    // 5. Bottom Edge Bar: path:branch (left) ... version (right)
    let display_path = if let Some(ref home) = dirs::home_dir() {
        if let Ok(rel) = ctx.workspace.strip_prefix(home) {
            format!("~/{}", rel.display())
        } else {
            ctx.workspace.display().to_string()
        }
    } else {
        ctx.workspace.display().to_string()
    };

    let git_info = crate::ui::status::StatusWidgets::get_git_branch(ctx.workspace)
        .map(|b| format!(":{}", b))
        .unwrap_or_default();

    let full_path_str = format!("{}{}", display_path, git_info);
    let bot_left = Span::styled(full_path_str, Style::default().fg(theme.muted));
    let bot_right = Span::styled(
        format!("v{}", env!("CARGO_PKG_VERSION")),
        Style::default().fg(theme.muted),
    );

    let bl_width = (bot_left.content.chars().count() + 2) as u16;
    let br_width = (bot_right.content.chars().count() + 2) as u16;

    if area.width >= bl_width + br_width {
        let bot_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(bl_width),
                Constraint::Min(1),
                Constraint::Length(br_width),
            ])
            .split(vert_chunks[8]);

        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::raw(" "), bot_left])),
            bot_chunks[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![bot_right, Span::raw(" ")])).alignment(Alignment::Right),
            bot_chunks[2],
        );
    }

    centered_input_rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::tempdir;

    #[test]
    fn test_render_welcome_screen_standard_size() {
        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let dir = tempdir().unwrap();
        let input_dock = InputDock::new();
        let ctx = WelcomeContext {
            workspace: dir.path(),
            provider: "minimax",
            model: "MiniMax-M2.7",
        };

        terminal
            .draw(|f| {
                let area = f.area();
                let rect = render_welcome_screen(f, area, &theme, &ctx, &input_dock);
                assert!(rect.width > 0);
                assert!(rect.height == 3);
            })
            .unwrap();
    }

    #[test]
    fn test_render_welcome_screen_compact_size() {
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let dir = tempdir().unwrap();
        let input_dock = InputDock::new();
        let ctx = WelcomeContext {
            workspace: dir.path(),
            provider: "anthropic",
            model: "claude-3-7-sonnet",
        };

        terminal
            .draw(|f| {
                let area = f.area();
                let rect = render_welcome_screen(f, area, &theme, &ctx, &input_dock);
                assert!(rect.width > 0);
                assert!(rect.height == 3);
            })
            .unwrap();
    }

    #[test]
    fn test_render_welcome_screen_tiny_size() {
        let theme = Theme::default();
        let backend = TestBackend::new(25, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        let dir = tempdir().unwrap();
        let input_dock = InputDock::new();
        let ctx = WelcomeContext {
            workspace: dir.path(),
            provider: "openai",
            model: "gpt-4o",
        };

        terminal
            .draw(|f| {
                let area = f.area();
                let rect = render_welcome_screen(f, area, &theme, &ctx, &input_dock);
                assert!(rect.width > 0);
            })
            .unwrap();
    }
}
