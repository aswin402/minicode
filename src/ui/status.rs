use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::path::Path;

pub struct StatusWidgets;

impl StatusWidgets {
    pub fn get_git_branch(workspace: &Path) -> Option<String> {
        let output = std::process::Command::new("git")
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .current_dir(workspace)
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !branch.is_empty() {
                    return Some(branch);
                }
            }
        }
        None
    }

    pub fn render_bottom_bar(
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        workspace: &Path,
        model: &str,
    ) {
        let home_dir = dirs::home_dir();
        let display_path = if let Some(ref home) = home_dir {
            if let Ok(rel) = workspace.strip_prefix(home) {
                format!("~/{}", rel.display())
            } else {
                workspace.display().to_string()
            }
        } else {
            workspace.display().to_string()
        };

        let mut footer_spans = vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                model,
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::default().fg(theme.muted)),
            Span::styled(display_path, Style::default().fg(theme.info)),
        ];

        if let Some(branch) = Self::get_git_branch(workspace) {
            footer_spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
            footer_spans.push(Span::styled(
                format!("git:{}", branch),
                Style::default().fg(theme.success),
            ));
        }

        let footer_line = Line::from(footer_spans);

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(theme.bg_primary));

        let paragraph = Paragraph::new(footer_line).block(block);
        frame.render_widget(paragraph, area);
    }
}
