//! Interactive Split-Pane Git-Graph Milestone DAG and Task Tracker Modal.
//!
//! Left pane: Vertical Git-commit style DAG showing roadmap milestones (◉──◉──▶──○).
//! Right pane: Selected milestone details, progress bar, and atomic step tasks list.

use crate::context::memory::working_memory::{MilestonePhase, TaskItemStatus};
use crate::ui::layout_utils::centered_rect;
use crate::ui::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

/// Currently focused pane in the Todo modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoModalPane {
    Milestones,
    Tasks,
}

/// State for the interactive Todo & Milestone DAG modal.
#[derive(Debug, Clone)]
pub struct TodoModalState {
    pub milestones: Vec<MilestonePhase>,
    pub selected_milestone: usize,
    pub selected_task: usize,
    pub active_pane: TodoModalPane,
    #[allow(dead_code)]
    pub milestone_scroll: usize,
    #[allow(dead_code)]
    pub task_scroll: usize,
}

impl TodoModalState {
    /// Creates a new `TodoModalState`, auto-selecting the active milestone.
    pub fn new(milestones: Vec<MilestonePhase>) -> Self {
        // Auto-select the active milestone, or the first one with pending tasks, or the last milestone
        let selected_milestone = milestones
            .iter()
            .position(|m| m.is_active)
            .or_else(|| milestones.iter().position(|m| m.pending_tasks > 0))
            .unwrap_or_else(|| milestones.len().saturating_sub(1));

        Self {
            milestones,
            selected_milestone,
            selected_task: 0,
            active_pane: TodoModalPane::Milestones,
            milestone_scroll: 0,
            task_scroll: 0,
        }
    }

    /// Moves selection down in the currently focused pane.
    pub fn next(&mut self) {
        match self.active_pane {
            TodoModalPane::Milestones => {
                if !self.milestones.is_empty()
                    && self.selected_milestone + 1 < self.milestones.len()
                {
                    self.selected_milestone += 1;
                    self.selected_task = 0;
                    self.task_scroll = 0;
                }
            }
            TodoModalPane::Tasks => {
                if let Some(milestone) = self.milestones.get(self.selected_milestone) {
                    if !milestone.tasks.is_empty() && self.selected_task + 1 < milestone.tasks.len()
                    {
                        self.selected_task += 1;
                    }
                }
            }
        }
    }

    /// Moves selection up in the currently focused pane.
    pub fn prev(&mut self) {
        match self.active_pane {
            TodoModalPane::Milestones => {
                if self.selected_milestone > 0 {
                    self.selected_milestone -= 1;
                    self.selected_task = 0;
                    self.task_scroll = 0;
                }
            }
            TodoModalPane::Tasks => {
                if self.selected_task > 0 {
                    self.selected_task -= 1;
                }
            }
        }
    }

    /// Toggles active pane focus between Milestones and Tasks.
    pub fn toggle_pane(&mut self) {
        self.active_pane = match self.active_pane {
            TodoModalPane::Milestones => TodoModalPane::Tasks,
            TodoModalPane::Tasks => TodoModalPane::Milestones,
        };
    }

    /// Jumps to the first item in the current pane.
    pub fn first(&mut self) {
        match self.active_pane {
            TodoModalPane::Milestones => {
                self.selected_milestone = 0;
                self.selected_task = 0;
                self.task_scroll = 0;
            }
            TodoModalPane::Tasks => {
                self.selected_task = 0;
            }
        }
    }

    /// Jumps to the last item in the current pane.
    pub fn last(&mut self) {
        match self.active_pane {
            TodoModalPane::Milestones => {
                if !self.milestones.is_empty() {
                    self.selected_milestone = self.milestones.len() - 1;
                    self.selected_task = 0;
                    self.task_scroll = 0;
                }
            }
            TodoModalPane::Tasks => {
                if let Some(milestone) = self.milestones.get(self.selected_milestone) {
                    if !milestone.tasks.is_empty() {
                        self.selected_task = milestone.tasks.len() - 1;
                    }
                }
            }
        }
    }
}

/// Renders the interactive Git-Graph Milestone DAG modal.
pub fn render_todo_modal(frame: &mut Frame, area: Rect, state: &TodoModalState, theme: &Theme) {
    let popup_area = centered_rect(88, 85, area);
    frame.render_widget(Clear, popup_area);

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(Span::styled(
            " 📋 Implementation Roadmap & Milestone DAG (F8) ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner_area = main_block.inner(popup_area);
    frame.render_widget(main_block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Top Summary Bar
            Constraint::Min(8),    // Split Body (Milestones DAG + Tasks List)
            Constraint::Length(1), // Bottom Navigation Keyhints
        ])
        .split(inner_area);

    // 1. Top Summary Bar
    let total_milestones = state.milestones.len();
    let completed_milestones = state
        .milestones
        .iter()
        .filter(|m| m.total_tasks > 0 && m.completed_tasks == m.total_tasks)
        .count();
    let total_tasks: usize = state.milestones.iter().map(|m| m.total_tasks).sum();
    let total_done: usize = state.milestones.iter().map(|m| m.completed_tasks).sum();

    let active_milestone_name = state
        .milestones
        .iter()
        .find(|m| m.is_active)
        .map(|m| format!("{}: {}", m.id, m.title))
        .unwrap_or_else(|| "All Milestones Completed".to_string());

    let summary_spans = vec![
        Span::styled("Milestones: ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}/{} Completed", completed_milestones, total_milestones),
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(theme.border)),
        Span::styled("Tasks: ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}/{} Total", total_done, total_tasks),
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(theme.border)),
        Span::styled("Active: ", Style::default().fg(theme.muted)),
        Span::styled(
            active_milestone_name,
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ),
    ];
    frame.render_widget(
        Paragraph::new(vec![Line::from(summary_spans), Line::from(String::new())]),
        chunks[0],
    );

    // 2. Split Body
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(42), // Left: Milestones DAG
            Constraint::Percentage(58), // Right: Tasks List & Details
        ])
        .split(chunks[1]);

    // Render Left Pane: Milestones DAG
    let left_border_color = if state.active_pane == TodoModalPane::Milestones {
        theme.brand_accent
    } else {
        theme.border
    };
    let left_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(left_border_color))
        .title(Span::styled(
            " Milestones (Roadmap DAG) ",
            Style::default()
                .fg(left_border_color)
                .add_modifier(Modifier::BOLD),
        ));
    let left_inner = left_block.inner(body_chunks[0]);
    frame.render_widget(left_block, body_chunks[0]);

    let mut milestone_lines = Vec::new();
    let visible_rows = (left_inner.height as usize).saturating_sub(1);
    let start_idx = if state.selected_milestone >= visible_rows {
        state.selected_milestone.saturating_sub(visible_rows / 2)
    } else {
        0
    };

    for (idx, milestone) in state.milestones.iter().enumerate().skip(start_idx) {
        let is_selected = idx == state.selected_milestone;
        let pct = if milestone.total_tasks > 0 {
            (milestone.completed_tasks * 100) / milestone.total_tasks
        } else {
            100
        };

        let (node_marker, node_color) = if milestone.is_active {
            ("▶ ", theme.info)
        } else if milestone.total_tasks > 0 && milestone.completed_tasks == milestone.total_tasks {
            ("◉ ", Color::Green)
        } else {
            ("○ ", Color::DarkGray)
        };

        let bg_color = if is_selected {
            theme.bg_elevated
        } else {
            theme.bg_primary
        };

        let title_disp = if milestone.title.len() > 22 {
            format!("{}...", &milestone.title[..19])
        } else {
            milestone.title.clone()
        };

        let pointer = if is_selected && state.active_pane == TodoModalPane::Milestones {
            "❯ "
        } else {
            "  "
        };

        milestone_lines.push(Line::from(vec![
            Span::styled(
                pointer,
                Style::default().fg(theme.brand_accent).bg(bg_color),
            ),
            Span::styled(
                node_marker,
                Style::default()
                    .fg(node_color)
                    .bg(bg_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}: {} ", milestone.id, title_disp),
                Style::default()
                    .fg(if is_selected {
                        theme.text_primary
                    } else {
                        theme.muted
                    })
                    .bg(bg_color)
                    .add_modifier(if is_selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(
                format!("({}%)", pct),
                Style::default()
                    .fg(if pct == 100 {
                        Color::Green
                    } else {
                        theme.muted
                    })
                    .bg(bg_color),
            ),
        ]));

        if idx + 1 < state.milestones.len() {
            milestone_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("│", Style::default().fg(theme.border)),
            ]));
        }
    }

    if state.milestones.is_empty() {
        milestone_lines.push(Line::from(Span::styled(
            "  (No milestones found in todo.md)",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    frame.render_widget(Paragraph::new(milestone_lines), left_inner);

    // Render Right Pane: Milestone Details & Tasks
    let right_border_color = if state.active_pane == TodoModalPane::Tasks {
        theme.brand_accent
    } else {
        theme.border
    };

    let selected_m = state.milestones.get(state.selected_milestone);
    let right_title = if let Some(m) = selected_m {
        format!(" {} Tasks ({}/{}) ", m.id, m.completed_tasks, m.total_tasks)
    } else {
        " Milestone Details ".to_string()
    };

    let right_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(right_border_color))
        .title(Span::styled(
            right_title,
            Style::default()
                .fg(right_border_color)
                .add_modifier(Modifier::BOLD),
        ));
    let right_inner = right_block.inner(body_chunks[1]);
    frame.render_widget(right_block, body_chunks[1]);

    let mut task_lines = Vec::new();
    if let Some(m) = selected_m {
        // Milestone title & gauge
        let pct = if m.total_tasks > 0 {
            (m.completed_tasks * 100) / m.total_tasks
        } else {
            100
        };

        let filled = (pct * 20) / 100;
        let empty = 20usize.saturating_sub(filled);
        let progress_bar = format!("[{}{}] {}%", "█".repeat(filled), "░".repeat(empty), pct);

        task_lines.push(Line::from(vec![
            Span::styled("Objective: ", Style::default().fg(theme.muted)),
            Span::styled(
                m.title.clone(),
                Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
            ),
        ]));
        task_lines.push(Line::from(vec![
            Span::styled("Progress:  ", Style::default().fg(theme.muted)),
            Span::styled(
                progress_bar,
                Style::default().fg(if pct == 100 {
                    Color::Green
                } else {
                    theme.brand_accent
                }),
            ),
            Span::styled(
                if m.is_active {
                    "  [ACTIVE MILESTONE]"
                } else {
                    ""
                },
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        task_lines.push(Line::from(Span::styled(
            "─".repeat((right_inner.width as usize).saturating_sub(2)),
            Style::default().fg(theme.border),
        )));

        // Step Tasks List
        if m.tasks.is_empty() {
            task_lines.push(Line::from(Span::styled(
                "  (No atomic tasks specified for this milestone)",
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::ITALIC),
            )));
        } else {
            for (idx, task) in m.tasks.iter().enumerate() {
                let is_selected_task = idx == state.selected_task;
                let bg_color = if is_selected_task && state.active_pane == TodoModalPane::Tasks {
                    theme.bg_elevated
                } else {
                    theme.bg_primary
                };

                let pointer = if is_selected_task && state.active_pane == TodoModalPane::Tasks {
                    "❯ "
                } else {
                    "  "
                };

                let (bullet, bullet_color, text_color, modifier) = match task.status {
                    TaskItemStatus::Completed => {
                        ("✔ ", Color::Green, theme.muted, Modifier::empty())
                    }
                    TaskItemStatus::InProgress => {
                        ("▶ ", theme.info, theme.text_primary, Modifier::BOLD)
                    }
                    TaskItemStatus::Pending => {
                        ("○ ", Color::DarkGray, theme.muted, Modifier::empty())
                    }
                };

                task_lines.push(Line::from(vec![
                    Span::styled(
                        pointer,
                        Style::default().fg(theme.brand_accent).bg(bg_color),
                    ),
                    Span::styled(
                        bullet,
                        Style::default()
                            .fg(bullet_color)
                            .bg(bg_color)
                            .add_modifier(modifier),
                    ),
                    Span::styled(
                        task.title.clone(),
                        Style::default()
                            .fg(text_color)
                            .bg(bg_color)
                            .add_modifier(modifier),
                    ),
                ]));
            }
        }
    } else {
        task_lines.push(Line::from(Span::styled(
            "  Select a milestone from the left pane to view its step tasks",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    frame.render_widget(Paragraph::new(task_lines), right_inner);

    // 3. Bottom Navigation Keyhints
    let keyhints = vec![
        Span::styled(
            "[Tab] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Switch Pane  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[↑/↓] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[Home/End] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("First/Last  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[Esc] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", Style::default().fg(theme.muted)),
    ];
    frame.render_widget(Paragraph::new(Line::from(keyhints)), chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_milestones() -> Vec<MilestonePhase> {
        vec![
            MilestonePhase {
                id: "Phase 140".to_string(),
                title: "MiniDev Runtime".to_string(),
                total_tasks: 2,
                completed_tasks: 2,
                in_progress_tasks: 0,
                pending_tasks: 0,
                tasks: Vec::new(),
                is_active: false,
            },
            MilestonePhase {
                id: "Phase 141".to_string(),
                title: "In-TUI Process Monitor".to_string(),
                total_tasks: 4,
                completed_tasks: 1,
                in_progress_tasks: 1,
                pending_tasks: 2,
                tasks: Vec::new(),
                is_active: true,
            },
            MilestonePhase {
                id: "Phase 142".to_string(),
                title: "Two-Tier Plan".to_string(),
                total_tasks: 3,
                completed_tasks: 0,
                in_progress_tasks: 0,
                pending_tasks: 3,
                tasks: Vec::new(),
                is_active: false,
            },
        ]
    }

    #[test]
    fn test_todo_modal_state_navigation() {
        let mut state = TodoModalState::new(sample_milestones());
        // Auto-selects active milestone (index 1)
        assert_eq!(state.selected_milestone, 1);
        assert_eq!(state.active_pane, TodoModalPane::Milestones);

        // Next milestone
        state.next();
        assert_eq!(state.selected_milestone, 2);

        // Previous milestone
        state.prev();
        assert_eq!(state.selected_milestone, 1);

        // Toggle pane
        state.toggle_pane();
        assert_eq!(state.active_pane, TodoModalPane::Tasks);
        state.toggle_pane();
        assert_eq!(state.active_pane, TodoModalPane::Milestones);

        // First & last
        state.first();
        assert_eq!(state.selected_milestone, 0);
        state.last();
        assert_eq!(state.selected_milestone, 2);
    }
}
