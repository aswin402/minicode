use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::agent::subagent::types::SubagentState;

/// An interactive, expandable bottom activity drawer rendering live subagent swarm telemetry
#[derive(Debug)]
#[allow(dead_code)]
pub struct SubagentDrawer {
    pub is_open: bool,
    pub selected_index: usize,
    #[allow(dead_code)]
    pub scroll_offset: usize,
    pub inspect_mode: bool,
}

impl Default for SubagentDrawer {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl SubagentDrawer {
    pub fn new() -> Self {
        Self {
            is_open: false,
            selected_index: 0,
            scroll_offset: 0,
            inspect_mode: false,
        }
    }

    /// Toggles the drawer open/closed state
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    pub fn open(&mut self) {
        self.is_open = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.inspect_mode = false;
    }

    /// Selects next subagent worker
    pub fn next(&mut self, total: usize) {
        if total == 0 {
            self.selected_index = 0;
            return;
        }
        if self.selected_index + 1 < total {
            self.selected_index += 1;
        } else {
            self.selected_index = 0;
        }
    }

    /// Selects previous subagent worker
    pub fn previous(&mut self, total: usize) {
        if total == 0 {
            self.selected_index = 0;
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = total.saturating_sub(1);
        }
    }

    /// Toggles detailed inspection pane for the selected worker
    pub fn toggle_inspect(&mut self) {
        self.inspect_mode = !self.inspect_mode;
    }

    /// Renders the activity drawer overlaid at the bottom of the screen
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &crate::ui::Theme) {
        if !self.is_open {
            return;
        }

        // Allocate bottom percentage of viewport
        let height_pct = crate::constants::SUBAGENT_DRAWER_HEIGHT_PERCENT;
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(100 - height_pct),
                Constraint::Percentage(height_pct),
            ])
            .split(area);

        let drawer_area = chunks[1];
        frame.render_widget(Clear, drawer_area);

        let workers = crate::agent::subagent::try_get_global_subagent_pool()
            .map(|p| p.snapshot_subagents())
            .unwrap_or_default();

        let running_count = workers
            .iter()
            .filter(|w| matches!(w.state, SubagentState::Running))
            .count();
        let total_count = workers.len();

        let title = format!(
            " 🤖 Subagent Swarm Activity Drawer [{} Running | {} Total] (Ctrl+S to hide) ",
            running_count, total_count
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.brand_accent))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(drawer_area);
        frame.render_widget(block, drawer_area);

        if workers.is_empty() {
            let empty_text = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "  ℹ ",
                        Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "No subagents currently active in this session.",
                        Style::default().fg(theme.text_primary),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  • Use tool ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "dispatch_subagent(role, prompt)",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " to spawn concurrent background workers.",
                        Style::default().fg(theme.muted),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  • Supported roles: ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "Researcher, CodeReviewer, TestEngineer, SecurityAuditor, Custom",
                        Style::default().fg(theme.info),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  • Press ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "Ctrl+S",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" or type ", Style::default().fg(theme.muted)),
                    Span::styled("/swarm", Style::default().fg(theme.brand_accent)),
                    Span::styled(
                        " anytime to toggle this drawer.",
                        Style::default().fg(theme.muted),
                    ),
                ]),
            ];
            frame.render_widget(Paragraph::new(empty_text), inner);
            return;
        }

        // Clamp selection to valid range
        let selected_idx = self.selected_index.min(workers.len().saturating_sub(1));

        // Split inner into main section and bottom navigation bar
        let inner_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(inner);

        // Split main section into left list (45%) and right inspector (55%)
        let split_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(inner_layout[0]);

        // Left Pane: Worker Cards List
        let mut list_lines = Vec::new();
        for (i, worker) in workers.iter().enumerate() {
            let is_selected = i == selected_idx;
            let (status_icon, status_color, status_text) = match &worker.state {
                SubagentState::Running => ("●", Color::Green, "RUNNING"),
                SubagentState::Completed => ("✔", Color::Cyan, "DONE"),
                SubagentState::Failed(_) => ("✗", Color::Red, "FAILED"),
                SubagentState::Canceled => ("⊘", Color::Yellow, "CANCELED"),
                SubagentState::Idle => ("○", Color::DarkGray, "IDLE"),
            };

            let prefix = if is_selected { "▶ " } else { "  " };
            let isolation = if worker.isolate_worktree {
                " [Worktree]"
            } else {
                " [Read-Only]"
            };

            let item_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            list_lines.push(Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} ", status_icon),
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&worker.id, item_style),
                Span::styled(
                    format!(" ({})", worker.role.badge()),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(isolation, Style::default().fg(theme.info)),
            ]));

            // Sub-line: active tool or status
            let detail = if let Some(ref t) = worker.current_tool {
                format!("    ⚡ Tool: `{}`", t)
            } else if let Some(ref s) = worker.status_message {
                format!("    ↳ {}", s)
            } else {
                "    ↳ Idle".to_string()
            };
            list_lines.push(Line::from(vec![Span::styled(
                detail,
                Style::default().fg(if worker.current_tool.is_some() {
                    theme.warning
                } else {
                    theme.muted
                }),
            )]));

            // Sub-line: turns and tokens
            let metrics = format!(
                "    Turns: {} | Tokens: {} | Status: {}",
                worker.turns_executed, worker.tokens_used, status_text
            );
            list_lines.push(Line::from(vec![Span::styled(
                metrics,
                Style::default().fg(theme.muted),
            )]));
            list_lines.push(Line::from(""));
        }

        let left_block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(theme.muted));
        frame.render_widget(
            Paragraph::new(list_lines)
                .block(left_block)
                .wrap(Wrap { trim: false }),
            split_layout[0],
        );

        // Right Pane: Detail Inspector for Selected Worker
        let selected_worker = &workers[selected_idx];
        let mut detail_lines = Vec::new();
        detail_lines.push(Line::from(vec![
            Span::styled("Worker: ", Style::default().fg(theme.muted)),
            Span::styled(
                &selected_worker.id,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | Role: ", Style::default().fg(theme.muted)),
            Span::styled(
                selected_worker.role.badge(),
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        detail_lines.push(Line::from(vec![
            Span::styled("State: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:?}", selected_worker.state),
                Style::default().fg(theme.info),
            ),
            Span::styled(" | Worktree: ", Style::default().fg(theme.muted)),
            Span::styled(
                if selected_worker.isolate_worktree {
                    format!("subagent/{}", selected_worker.id)
                } else {
                    "None (Main / Read-only)".to_string()
                },
                Style::default().fg(theme.text_primary),
            ),
        ]));

        if let Some(ref tool) = selected_worker.current_tool {
            detail_lines.push(Line::from(vec![
                Span::styled(
                    "⚡ Active Tool: ",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    tool,
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        if let Some(ref status) = selected_worker.status_message {
            detail_lines.push(Line::from(vec![
                Span::styled("Status Detail: ", Style::default().fg(theme.muted)),
                Span::styled(status, Style::default().fg(theme.text_primary)),
            ]));
        }

        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(
            "─── Task Prompt ───",
            Style::default().fg(theme.muted),
        )));
        for p_line in selected_worker.prompt.lines().take(5) {
            detail_lines.push(Line::from(Span::styled(
                format!("  {}", p_line),
                Style::default().fg(theme.text_primary),
            )));
        }
        if selected_worker.prompt.lines().count() > 5 {
            detail_lines.push(Line::from(Span::styled(
                "  ...",
                Style::default().fg(theme.muted),
            )));
        }

        // Findings / summary
        if let Some(ref summary) = selected_worker.final_summary {
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                "─── Final Findings / Report ───",
                Style::default().fg(theme.brand_accent),
            )));
            for s_line in summary.lines().take(8) {
                detail_lines.push(Line::from(Span::styled(
                    format!("  {}", s_line),
                    Style::default().fg(theme.text_primary),
                )));
            }
            if summary.lines().count() > 8 {
                detail_lines.push(Line::from(Span::styled(
                    "  ... (view full report via scratchpad_read)",
                    Style::default().fg(theme.muted),
                )));
            }
        }

        frame.render_widget(
            Paragraph::new(detail_lines).wrap(Wrap { trim: false }),
            split_layout[1],
        );

        // Bottom Navigation Bar
        let nav_spans = vec![
            Span::styled(
                " [↑/k / ↓/j] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Navigate  ", Style::default().fg(theme.muted)),
            Span::styled(
                "[Enter] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Toggle Inspect  ", Style::default().fg(theme.muted)),
            Span::styled(
                "[x] ",
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Kill Worker  ", Style::default().fg(theme.muted)),
            Span::styled(
                "[a] ",
                Style::default()
                    .fg(theme.destructive)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Kill All  ", Style::default().fg(theme.muted)),
            Span::styled(
                "[Esc / Ctrl+S] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Close", Style::default().fg(theme.muted)),
        ];
        frame.render_widget(Paragraph::new(Line::from(nav_spans)), inner_layout[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_drawer_state() {
        let mut drawer = SubagentDrawer::new();
        assert!(!drawer.is_open);
        drawer.toggle();
        assert!(drawer.is_open);

        drawer.next(3);
        assert_eq!(drawer.selected_index, 1);
        drawer.next(3);
        assert_eq!(drawer.selected_index, 2);
        drawer.next(3);
        assert_eq!(drawer.selected_index, 0);

        drawer.previous(3);
        assert_eq!(drawer.selected_index, 2);

        drawer.toggle_inspect();
        assert!(drawer.inspect_mode);

        drawer.close();
        assert!(!drawer.is_open);
        assert!(!drawer.inspect_mode);
    }
}
