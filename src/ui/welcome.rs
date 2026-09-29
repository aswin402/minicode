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
            Constraint::Min(1),               // 0: Top spacer (flexible)
            Constraint::Length(3),            // 1: Brand logo lockup (3 lines)
            Constraint::Length(1),            // 2: Spacer between logo and input dock
            Constraint::Length(input_height), // 3: Centered Dynamic Input Dock
            Constraint::Min(2),               // 4: Bottom spacer (flexible)
            Constraint::Length(1),            // 5: Bottom Edge Bar
        ])
        .split(area);

    // 1. Brand Logo Lockup (Stepped geometric pillars + typography)
    let version_str = format!("minicode v{}", env!("CARGO_PKG_VERSION"));
    let slogan = "Build better, with AI";
    let max_text_len = slogan.chars().count().max(version_str.chars().count());
    let lockup_width = (15 + max_text_len) as u16;
    let lockup_area = if vert_chunks[1].width > lockup_width {
        let offset_x = (vert_chunks[1].width - lockup_width) / 2;
        Rect {
            x: vert_chunks[1].x + offset_x,
            y: vert_chunks[1].y,
            width: lockup_width,
            height: 3,
        }
    } else {
        vert_chunks[1]
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
        Span::styled(slogan, Style::default().fg(theme.muted)),
    ]);

    let line3 = Line::from(vec![
        Span::styled("██  ", Style::default().fg(theme.brand_accent)),
        Span::raw("    "),
        Span::styled("  ██", Style::default().fg(theme.info)),
        Span::raw("   "),
        Span::styled(version_str, Style::default().fg(theme.muted)),
    ]);

    let logo_para = Paragraph::new(vec![line1, line2, line3]);
    frame.render_widget(logo_para, lockup_area);

    // 2. Centered Input Dock
    // Responsive width: 65% of screen width clamped between 42 and 74 columns
    let input_width = (vert_chunks[3].width * 65 / 100)
        .clamp(40, 74)
        .min(vert_chunks[3].width);
    let input_x = vert_chunks[3].x + (vert_chunks[3].width.saturating_sub(input_width)) / 2;
    let centered_input_rect = Rect {
        x: input_x,
        y: vert_chunks[3].y,
        width: input_width,
        height: vert_chunks[3].height,
    };
    input_dock.render(frame, centered_input_rect, theme);

    // 3. Bottom Edge Bar: path:branch (left) ... provider model (right)
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
    let mut left_spans = vec![
        Span::raw(" "),
        Span::styled(full_path_str, Style::default().fg(theme.muted)),
    ];

    let (task_count, task_mem_mb, task_cpu_pct) =
        crate::dev::registry::get_global_dev_registry().get_telemetry_snapshot();
    if task_count > 0 {
        let mem_str = if task_mem_mb < 10.0 {
            format!("{:.1}MB", task_mem_mb)
        } else {
            format!("{:.0}MB", task_mem_mb)
        };
        left_spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
        left_spans.push(Span::styled(
            format!("tasks:{} ({} · {:.1}%)", task_count, mem_str, task_cpu_pct),
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ));
        left_spans.push(Span::styled(" [F7]", Style::default().fg(theme.muted)));
    }

    let provider_display = title_case(ctx.provider);
    let model_display = format_model_name(ctx.model);
    let model_str = if provider_display.is_empty() {
        model_display
    } else {
        format!("{} {}", provider_display, model_display)
    };
    let bot_right = Span::styled(model_str, Style::default().fg(theme.muted));

    let bl_width = (left_spans
        .iter()
        .map(|s| s.content.chars().count())
        .sum::<usize>()
        + 1) as u16;
    let br_width = (bot_right.content.chars().count() + 2) as u16;

    if area.width >= bl_width + br_width {
        let bot_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(bl_width),
                Constraint::Min(1),
                Constraint::Length(br_width),
            ])
            .split(vert_chunks[5]);

        frame.render_widget(Paragraph::new(Line::from(left_spans)), bot_chunks[0]);
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

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        // Logo lockup includes slogan on line 2 and version on line 3
        assert!(rendered.contains("Build better, with AI"));
        assert!(rendered.contains("minicode v"));
        // Model is rendered in bottom-right corner
        assert!(rendered.contains("Minimax MiniMax-M2.7"));
        // Top header and ready status are removed
        assert!(!rendered.contains("AI Coding Assistant"));
        assert!(!rendered.contains("READY"));
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
