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

        let pool_opt = crate::agent::subagent::try_get_global_subagent_pool();
        let workers = pool_opt
            .as_ref()
            .map(|p| p.snapshot_subagents())
            .unwrap_or_default();
        let max_parallel = pool_opt
            .as_ref()
            .map(|p| p.max_parallel())
            .unwrap_or(crate::constants::DEFAULT_MAX_PARALLEL_MINIPUBS);

        let running_count = workers
            .iter()
            .filter(|w| matches!(w.state, SubagentState::Running))
            .count();
        let total_count = workers.len();

        let title = format!(
            " 🤖 MiniPub Subagent Activity Drawer [{} Running / {} Max Parallel | {} Total] (Ctrl+S to hide) ",
            running_count, max_parallel, total_count
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
                        "No MiniPub subagents currently active in this session.",
                        Style::default().fg(theme.text_primary),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  • Use tool ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "spawn_subagent(role, task, todos, file_boundaries)",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " to launch lightweight in-process MiniPub workers.",
                        Style::default().fg(theme.muted),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  • Supported roles: ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "Scout, Coder, Reviewer, Tester",
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
                    Span::styled("/minipub", Style::default().fg(theme.brand_accent)),
                    Span::styled(" / ", Style::default().fg(theme.muted)),
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

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

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
                SubagentState::Starting => ("○", Color::DarkGray, "STARTING"),
                SubagentState::Running => ("●", Color::Green, "RUNNING"),
                SubagentState::WaitingForInput => ("◷", Color::Yellow, "WAITING"),
                SubagentState::Completed => ("✔", Color::Cyan, "DONE"),
                SubagentState::Failed(_) => ("✗", Color::Red, "FAILED"),
                SubagentState::Terminated => ("⊘", Color::Yellow, "TERMINATED"),
            };

            let prefix = if is_selected { "▶ " } else { "  " };
            let isolation = if worker.isolate_worktree {
                " [Worktree]"
            } else if !worker.file_boundaries.is_empty() {
                " [File-Lease]"
            } else {
                " [Shared]"
            };

            let item_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            let mut header_spans = vec![
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
            ];

            if !worker.todos.is_empty() {
                let done_todos = worker.todos.iter().filter(|t| t.completed).count();
                header_spans.push(Span::styled(
                    format!(" [Todos {}/{}]", done_todos, worker.todos.len()),
                    Style::default().fg(theme.success),
                ));
            }

            list_lines.push(Line::from(header_spans));

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

            // Sub-line: turns, tokens, and heartbeat
            let hb_age = if worker.last_activity_secs > 0 {
                now_secs.saturating_sub(worker.last_activity_secs)
            } else {
                0
            };
            let metrics = if worker.state.is_active() {
                format!(
                    "    Turns: {} | Tokens: {} | Heartbeat: {}s ago | {}",
                    worker.turns_executed, worker.tokens_used, hb_age, status_text
                )
            } else {
                format!(
                    "    Turns: {} | Tokens: {} | Status: {}",
                    worker.turns_executed, worker.tokens_used, status_text
                )
            };
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
            Span::styled("MiniPub: ", Style::default().fg(theme.muted)),
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

        let isolation_desc = if selected_worker.isolate_worktree {
            format!(".minicode/worktrees/subagent-{}", selected_worker.id)
        } else if !selected_worker.file_boundaries.is_empty() {
            "Cooperative File-Lease (.minicode/minipub/leases.json)".to_string()
        } else {
            "Shared Workspace".to_string()
        };

        detail_lines.push(Line::from(vec![
            Span::styled("State: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:?}", selected_worker.state),
                Style::default().fg(theme.info),
            ),
            Span::styled(" | Isolation: ", Style::default().fg(theme.muted)),
            Span::styled(isolation_desc, Style::default().fg(theme.text_primary)),
        ]));

        let end_ts = selected_worker.finished_at_secs.unwrap_or(now_secs);
        let elapsed_s = end_ts.saturating_sub(selected_worker.started_at_secs);
        let hb_s = if selected_worker.last_activity_secs > 0 {
            now_secs.saturating_sub(selected_worker.last_activity_secs)
        } else {
            0
        };
        detail_lines.push(Line::from(vec![
            Span::styled("Elapsed: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{}s", elapsed_s),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(" | Last Activity: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{}s ago", hb_s),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(" | Tokens: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{}", selected_worker.tokens_used),
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

        if !selected_worker.file_boundaries.is_empty() {
            detail_lines.push(Line::from(vec![
                Span::styled("File Boundaries: ", Style::default().fg(theme.muted)),
                Span::styled(
                    selected_worker.file_boundaries.join(", "),
                    Style::default().fg(theme.info),
                ),
            ]));
        }

        if !selected_worker.todos.is_empty() {
            let done_cnt = selected_worker.todos.iter().filter(|t| t.completed).count();
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                format!(
                    "─── Assigned MiniPub Todos ({}/{}) ───",
                    done_cnt,
                    selected_worker.todos.len()
                ),
                Style::default().fg(theme.brand_accent),
            )));
            for item in selected_worker.todos.iter().take(6) {
                let (check_glyph, check_color) = if item.completed {
                    ("✔ [x]", theme.success)
                } else {
                    ("○ [ ]", theme.muted)
                };
                detail_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} ", check_glyph),
                        Style::default().fg(check_color),
                    ),
                    Span::styled(
                        format!("{}. {}", item.id, item.task),
                        Style::default().fg(if item.completed {
                            theme.muted
                        } else {
                            theme.text_primary
                        }),
                    ),
                ]));
            }
        }

        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(
            "─── Task Prompt ───",
            Style::default().fg(theme.muted),
        )));
        for p_line in selected_worker.prompt.lines().take(4) {
            detail_lines.push(Line::from(Span::styled(
                format!("  {}", p_line),
                Style::default().fg(theme.text_primary),
            )));
        }
        if selected_worker.prompt.lines().count() > 4 {
            detail_lines.push(Line::from(Span::styled(
                "  ...",
                Style::default().fg(theme.muted),
            )));
        }

        // Findings / summary
        if let Some(ref summary) = selected_worker.final_summary {
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                "─── Boomerang Summary / Report ───",
                Style::default().fg(theme.brand_accent),
            )));
            for s_line in summary.lines().take(6) {
                detail_lines.push(Line::from(Span::styled(
                    format!("  {}", s_line),
                    Style::default().fg(theme.text_primary),
                )));
            }
            if summary.lines().count() > 6 {
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
