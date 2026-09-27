//! Interactive Single-Pane Minimalist Roadmap DAG and Task Tracker Modal.
//!
//! Features:
//! - Border-embedded title and status with zero top banner clutter.
//! - Continuous 6-column aligned Git-graph subway spine (`│`, `◉`, `✔`, `○`).
//! - Smart history collapse (hides 200+ completed phases, auto-expanding on navigation or Space).
//! - Active/selected milestone auto-expands its atomic task tree.
//! - Responsive integrated bottom footer dock with dynamic unicode progress meter and navigation keyhints.

use crate::context::memory::working_memory::{MilestonePhase, TaskItemStatus};
use crate::ui::layout_utils::centered_rect;
use crate::ui::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
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
    pub show_all_completed: bool,
    #[allow(dead_code)]
    pub milestone_scroll: usize,
    #[allow(dead_code)]
    pub task_scroll: usize,
}

impl TodoModalState {
    /// Creates a new `TodoModalState`, auto-selecting the active milestone.
    pub fn new(milestones: Vec<MilestonePhase>) -> Self {
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
            show_all_completed: false,
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
    /// If moving into collapsed historical phases, auto-expands so the cursor never vanishes.
    pub fn prev(&mut self) {
        match self.active_pane {
            TodoModalPane::Milestones => {
                if self.selected_milestone > 0 {
                    self.selected_milestone -= 1;
                    self.selected_task = 0;
                    self.task_scroll = 0;

                    let active_idx = self
                        .milestones
                        .iter()
                        .position(|m| m.is_active)
                        .or_else(|| self.milestones.iter().position(|m| m.pending_tasks > 0))
                        .unwrap_or(0);
                    if active_idx > 2 && self.selected_milestone < active_idx.saturating_sub(1) {
                        self.show_all_completed = true;
                    }
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

    /// Toggles expanding/collapsing all historical completed milestones.
    pub fn toggle_expand(&mut self) {
        self.show_all_completed = !self.show_all_completed;
    }

    /// Jumps to the first item in the current pane.
    pub fn first(&mut self) {
        match self.active_pane {
            TodoModalPane::Milestones => {
                self.selected_milestone = 0;
                self.selected_task = 0;
                self.task_scroll = 0;
                self.show_all_completed = true;
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

/// Renders the single-pane minimal roadmap DAG modal.
pub fn render_todo_modal(frame: &mut Frame, area: Rect, state: &TodoModalState, theme: &Theme) {
    let popup_area = centered_rect(84, 82, area);
    frame.render_widget(Clear, popup_area);

    // Active milestone tag embedded directly in top border
    let active_milestone = state.milestones.iter().find(|m| m.is_active);
    let active_label = if let Some(m) = active_milestone {
        format!("{} · Active", m.id)
    } else if state.milestones.is_empty() {
        "No Milestones".to_string()
    } else {
        "All Completed".to_string()
    };

    let title_span = Span::styled(
        format!(" 📋 Roadmap · {} ", active_label),
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    );

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(title_span)
        .title(
            Line::from(vec![Span::styled(
                " [Esc] Close ",
                Style::default().fg(theme.muted),
            )])
            .alignment(ratatui::layout::Alignment::Right),
        );

    let inner_area = main_block.inner(popup_area);
    frame.render_widget(main_block, popup_area);

    // Minimal 3-row layout: Tree Body (Min 4) + Divider (1) + Footer Dock (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),    // 0: Full-width unified Roadmap DAG Tree & Tasks
            Constraint::Length(1), // 1: Horizontal divider
            Constraint::Length(1), // 2: Integrated Footer Dock (Progress Meter & Controls)
        ])
        .split(inner_area);

    // Determine active milestone index for smart historical collapsing
    let active_idx = state
        .milestones
        .iter()
        .position(|m| m.is_active)
        .or_else(|| state.milestones.iter().position(|m| m.pending_tasks > 0))
        .unwrap_or(0);

    let mut lines = Vec::new();
    let mut cursor_line = 0usize;

    if state.milestones.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "   (No milestones found in todo.md)",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        // Smart Historical Collapsing:
        // Only collapse if user hasn't explicitly expanded AND selected milestone is within the visible window.
        let should_collapse = !state.show_all_completed
            && active_idx > 2
            && state.selected_milestone >= active_idx.saturating_sub(1);

        let start_idx = if should_collapse {
            active_idx.saturating_sub(1)
        } else {
            0
        };

        if should_collapse {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled("··· ", Style::default().fg(theme.muted)),
                Span::styled(
                    format!(
                        "{} completed phases (press Space to expand)",
                        active_idx.saturating_sub(1)
                    ),
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            lines.push(Line::from(vec![Span::raw("   │")]));
        } else if state.show_all_completed && active_idx > 2 {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled("··· ", Style::default().fg(theme.muted)),
                Span::styled(
                    format!(
                        "Showing all {} phases (press Space to collapse)",
                        state.milestones.len()
                    ),
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            lines.push(Line::from(vec![Span::raw("   │")]));
        }

        let visible_milestones: Vec<(usize, &MilestonePhase)> = state
            .milestones
            .iter()
            .enumerate()
            .skip(start_idx)
            .collect();

        let visible_count = visible_milestones.len();

        for (v_i, (m_idx, milestone)) in visible_milestones.iter().enumerate() {
            let is_selected_m = *m_idx == state.selected_milestone;
            let is_last_m = v_i == visible_count - 1;

            // Character grid:
            // Index 0..1: pointer "▶ " if selected milestone, else "  "
            // Index 2: " "
            // Index 3: node glyph ("◉", "✔", "○") aligned with rail "│"
            // Index 4..5: "  "
            // Index 6+: milestone title
            let (node_glyph, node_color) = if milestone.is_active {
                ("◉", theme.info)
            } else if milestone.total_tasks > 0
                && milestone.completed_tasks == milestone.total_tasks
            {
                ("✔", theme.success)
            } else {
                ("○", theme.muted)
            };

            let pointer = if is_selected_m { "▶ " } else { "  " };

            let m_bg = if is_selected_m && state.active_pane == TodoModalPane::Milestones {
                theme.bg_elevated
            } else {
                theme.bg_primary
            };

            let pct = if milestone.total_tasks > 0 {
                (milestone.completed_tasks * 100) / milestone.total_tasks
            } else {
                100
            };

            let status_badge = if milestone.is_active {
                format!(
                    " [Active · {}/{} Tasks]",
                    milestone.completed_tasks, milestone.total_tasks
                )
            } else if pct == 100 {
                " [Done]".to_string()
            } else {
                format!(" [{}%]", pct)
            };

            let status_style = if milestone.is_active {
                Style::default()
                    .fg(theme.info)
                    .bg(m_bg)
                    .add_modifier(Modifier::BOLD)
            } else if pct == 100 {
                Style::default().fg(theme.success).bg(m_bg)
            } else {
                Style::default().fg(theme.muted).bg(m_bg)
            };

            let current_line_idx = lines.len();
            if is_selected_m && state.active_pane == TodoModalPane::Milestones {
                cursor_line = current_line_idx;
            }

            lines.push(Line::from(vec![
                Span::styled(
                    pointer,
                    Style::default()
                        .fg(theme.brand_accent)
                        .bg(m_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default().bg(m_bg)),
                Span::styled(
                    node_glyph,
                    Style::default()
                        .fg(node_color)
                        .bg(m_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", Style::default().bg(m_bg)),
                Span::styled(
                    format!("{}: {}", milestone.id, milestone.title),
                    Style::default()
                        .fg(if is_selected_m {
                            theme.text_primary
                        } else {
                            theme.muted
                        })
                        .bg(m_bg)
                        .add_modifier(if is_selected_m {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(status_badge, status_style),
            ]));

            // Atomic tasks branch under active or selected milestone
            let show_tasks = milestone.is_active || is_selected_m;
            if show_tasks {
                if milestone.tasks.is_empty() {
                    if is_selected_m {
                        lines.push(Line::from(vec![
                            Span::raw("   │  "),
                            Span::styled(
                                "└── (No atomic tasks in this milestone)",
                                Style::default()
                                    .fg(theme.muted)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                        ]));
                    }
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("   │  "),
                        Span::styled(
                            "└── Tasks",
                            Style::default()
                                .fg(theme.muted)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" ({}/{})", milestone.completed_tasks, milestone.total_tasks),
                            Style::default().fg(theme.muted),
                        ),
                    ]));

                    let task_count = milestone.tasks.len();
                    for (t_idx, task) in milestone.tasks.iter().enumerate() {
                        let is_selected_task = is_selected_m && t_idx == state.selected_task;
                        let is_last_task = t_idx == task_count - 1;
                        let branch = if is_last_task {
                            "      └── "
                        } else {
                            "      ├── "
                        };

                        let task_bg =
                            if is_selected_task && state.active_pane == TodoModalPane::Tasks {
                                theme.bg_elevated
                            } else {
                                theme.bg_primary
                            };

                        let (t_bullet, t_color, t_mod) = match task.status {
                            TaskItemStatus::Completed => ("✔ ", theme.success, Modifier::empty()),
                            TaskItemStatus::InProgress => ("▶ ", theme.info, Modifier::BOLD),
                            TaskItemStatus::Pending => ("○ ", theme.muted, Modifier::empty()),
                        };

                        let task_pointer =
                            if is_selected_task && state.active_pane == TodoModalPane::Tasks {
                                "▶ "
                            } else {
                                "  "
                            };

                        let t_line_idx = lines.len();
                        if is_selected_task && state.active_pane == TodoModalPane::Tasks {
                            cursor_line = t_line_idx;
                        }

                        lines.push(Line::from(vec![
                            Span::styled(
                                task_pointer,
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .bg(task_bg)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" │", Style::default().fg(theme.border).bg(task_bg)),
                            Span::styled(branch, Style::default().fg(theme.muted).bg(task_bg)),
                            Span::styled(
                                t_bullet,
                                Style::default().fg(t_color).bg(task_bg).add_modifier(t_mod),
                            ),
                            Span::styled(
                                task.title.clone(),
                                Style::default()
                                    .fg(if is_selected_task {
                                        theme.text_primary
                                    } else {
                                        theme.muted
                                    })
                                    .bg(task_bg)
                                    .add_modifier(t_mod),
                            ),
                        ]));
                    }
                }
            }

            // Connecting rail between milestones
            if !is_last_m {
                lines.push(Line::from(vec![Span::raw("   │")]));
            }
        }
    }

    // Auto-scroll viewport centering cursor line
    let visible_height = chunks[0].height as usize;
    let scroll_y = if cursor_line >= visible_height {
        cursor_line.saturating_sub(visible_height / 2)
    } else {
        0
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((scroll_y as u16, 0)),
        chunks[0],
    );

    // Horizontal Divider Rule
    let divider = "─".repeat(chunks[1].width as usize);
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            divider,
            Style::default().fg(theme.border),
        )])),
        chunks[1],
    );

    // Responsive Integrated Footer Dock: Dynamic Progress Meter + Navigation Keyhints
    let selected_m = state.milestones.get(state.selected_milestone);
    let (m_label, pct, done, total) = if let Some(m) = selected_m {
        let p = if m.total_tasks > 0 {
            (m.completed_tasks * 100) / m.total_tasks
        } else {
            100
        };
        (m.id.clone(), p, m.completed_tasks, m.total_tasks)
    } else {
        ("Roadmap".to_string(), 100, 0, 0)
    };

    let available_w = chunks[2].width as usize;

    let (bar_slots, right_spans) = if available_w >= 100 {
        let bar_slots = 16usize;
        let right_spans = vec![
            Span::styled(
                "[Tab] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if state.active_pane == TodoModalPane::Milestones {
                    "Tasks  "
                } else {
                    "Roadmap  "
                },
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                "[Space] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if state.show_all_completed {
                    "Collapse  "
                } else {
                    "All  "
                },
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                "[↑/↓] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Move  ", Style::default().fg(theme.muted)),
            Span::styled(
                "[Esc] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Close  ", Style::default().fg(theme.muted)),
        ];
        (bar_slots, right_spans)
    } else if available_w >= 70 {
        let bar_slots = 8usize;
        let right_spans = vec![
            Span::styled(
                "[Tab] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[Space] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[↑/↓] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[Esc] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Close  ", Style::default().fg(theme.muted)),
        ];
        (bar_slots, right_spans)
    } else {
        let bar_slots = 6usize;
        let right_spans = vec![
            Span::styled(
                "[Esc] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Close ", Style::default().fg(theme.muted)),
        ];
        (bar_slots, right_spans)
    };

    let filled = (pct * bar_slots) / 100;
    let empty = bar_slots.saturating_sub(filled);
    let bar_str = format!("{}{}", "▰".repeat(filled), "▱".repeat(empty));
    let bar_color = if pct == 100 {
        theme.success
    } else {
        theme.brand_accent
    };

    let left_spans = if available_w >= 70 {
        vec![
            Span::raw("  "),
            Span::styled(
                format!("{} Progress  ", m_label),
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(bar_str, Style::default().fg(bar_color)),
            Span::styled(
                format!("  {}% ", pct),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({}/{} Tasks)", done, total),
                Style::default().fg(theme.muted),
            ),
        ]
    } else {
        vec![
            Span::raw(" "),
            Span::styled(
                format!("{} ", m_label),
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(bar_str, Style::default().fg(bar_color)),
            Span::styled(
                format!(" {}%", pct),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    };

    let left_w: usize = left_spans
        .iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    let right_w: usize = right_spans
        .iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    let space_count = available_w.saturating_sub(left_w + right_w);

    let mut footer_spans = left_spans;
    footer_spans.push(Span::raw(" ".repeat(space_count)));
    footer_spans.extend(right_spans);

    frame.render_widget(Paragraph::new(Line::from(footer_spans)), chunks[2]);
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
        assert!(!state.show_all_completed);

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

        // Toggle expand
        state.toggle_expand();
        assert!(state.show_all_completed);
        state.toggle_expand();
        assert!(!state.show_all_completed);

        // First & last
        state.first();
        assert_eq!(state.selected_milestone, 0);
        state.last();
        assert_eq!(state.selected_milestone, 2);
    }
}
