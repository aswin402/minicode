//! Inline Todo & Task Plan TUI Widget rendered above the input box.
//!
//! Provides 4 configurable visual styles:
//! - `Tree` (Default): Oh My Pi tree style (`TODO \n └── Plan · 1/3 \n ├── ✔ T1`)
//! - `Card`: Rounded container card (`╭── 📋 TODO ──╮`)
//! - `Rail`: Modern left accent rail (`▎ 📋 TODO`)
//! - `Minimal`: Clean open top rule (`📋 TODO ────── [Plan]`)

use crate::config::TodoWidgetStyle;
use crate::ui::theme::Theme;
use crate::ui::view::{LivePlanBlock, LivePlanTaskStatus};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Calculates the exact required row height for the inline todo widget.
pub fn todo_widget_required_height(plan: &Option<LivePlanBlock>, style: TodoWidgetStyle) -> u16 {
    match plan {
        Some(p) if !p.tasks.is_empty() => {
            let task_count = p.tasks.len() as u16;
            match style {
                TodoWidgetStyle::Tree => task_count + 4,
                TodoWidgetStyle::Card => task_count + 4,
                TodoWidgetStyle::Rail => task_count + 3,
                TodoWidgetStyle::Minimal => task_count + 3,
            }
        }
        _ => 0,
    }
}

/// Renders the inline todo widget into a list of Ratatui `Line`s according to the selected style.
pub fn render_todo_widget<'a>(
    plan: &'a LivePlanBlock,
    style: TodoWidgetStyle,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    if plan.tasks.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::with_capacity(plan.tasks.len() + 5);
    // Top spacing line to visually separate the todo widget from the streaming timeline
    lines.push(Line::from(""));

    let content_lines = match style {
        TodoWidgetStyle::Tree => render_todo_tree(plan, width, theme),
        TodoWidgetStyle::Card => render_todo_card(plan, width, theme),
        TodoWidgetStyle::Rail => render_todo_rail(plan, width, theme),
        TodoWidgetStyle::Minimal => render_todo_minimal(plan, width, theme),
    };
    lines.extend(content_lines);
    lines
}

/// Style 1: Oh My Pi tree layout (Default)
fn render_todo_tree<'a>(plan: &'a LivePlanBlock, width: u16, theme: &'a Theme) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(plan.tasks.len() + 3);

    // Line 1: Header "TODO" in bold cyan/info
    lines.push(Line::from(vec![Span::styled(
        "TODO",
        Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
    )]));

    // Line 2: "└── Plan · 1/3"
    let subheader_title = if plan.title.trim().is_empty() {
        "Plan"
    } else {
        plan.title.as_str()
    };
    lines.push(Line::from(vec![
        Span::styled("└── ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} · ", subheader_title),
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}/{}", plan.completed_tasks, plan.total_tasks),
            Style::default().fg(theme.muted),
        ),
    ]));

    // Lines 3..N: Tree branch tasks
    let task_count = plan.tasks.len();
    for (idx, task) in plan.tasks.iter().enumerate() {
        let is_last = idx == task_count - 1;
        let branch = if is_last {
            "    └── "
        } else {
            "    ├── "
        };

        let (bullet, bullet_color, text_color, modifier) = match task.status {
            LivePlanTaskStatus::Completed => ("✔ ", theme.success, theme.muted, Modifier::empty()),
            LivePlanTaskStatus::InProgress => {
                ("▶ ", theme.info, theme.text_primary, Modifier::BOLD)
            }
            LivePlanTaskStatus::Pending => ("○ ", theme.muted, theme.muted, Modifier::empty()),
        };

        lines.push(Line::from(vec![
            Span::styled(branch, Style::default().fg(theme.muted)),
            Span::styled(
                bullet,
                Style::default().fg(bullet_color).add_modifier(modifier),
            ),
            Span::styled(
                task.title.clone(),
                Style::default().fg(text_color).add_modifier(modifier),
            ),
        ]));
    }

    // Divider line below the tree
    let divider_width = (width as usize).max(10);
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(divider_width),
        Style::default().fg(theme.border),
    )]));

    lines
}

/// Style 2: Rounded container card with interior tree
fn render_todo_card<'a>(plan: &'a LivePlanBlock, width: u16, theme: &'a Theme) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(plan.tasks.len() + 3);

    // Top border
    let left_header = format!(
        "╭── 📋 TODO · {}/{} Tasks Completed ──",
        plan.completed_tasks, plan.total_tasks
    );
    let right_tag = format!("── [{}] ──╮", plan.title);
    let left_len = UnicodeWidthStr::width(left_header.as_str());
    let right_len = UnicodeWidthStr::width(right_tag.as_str());
    let middle_dashes = (width as usize).saturating_sub(left_len + right_len);
    let top_border = format!("{}{}{}", left_header, "─".repeat(middle_dashes), right_tag);

    lines.push(Line::from(vec![Span::styled(
        top_border,
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    )]));

    // Sub-header line
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.brand_accent)),
        Span::styled("└── Tasks", Style::default().fg(theme.muted)),
    ]));

    // Tasks
    let task_count = plan.tasks.len();
    for (idx, task) in plan.tasks.iter().enumerate() {
        let is_last = idx == task_count - 1;
        let branch = if is_last {
            "    └── "
        } else {
            "    ├── "
        };

        let (bullet, bullet_color, text_color, modifier) = match task.status {
            LivePlanTaskStatus::Completed => ("✔ ", theme.success, theme.muted, Modifier::empty()),
            LivePlanTaskStatus::InProgress => {
                ("▶ ", theme.info, theme.text_primary, Modifier::BOLD)
            }
            LivePlanTaskStatus::Pending => ("○ ", theme.muted, theme.muted, Modifier::empty()),
        };

        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(theme.brand_accent)),
            Span::styled(branch, Style::default().fg(theme.muted)),
            Span::styled(
                bullet,
                Style::default().fg(bullet_color).add_modifier(modifier),
            ),
            Span::styled(
                task.title.clone(),
                Style::default().fg(text_color).add_modifier(modifier),
            ),
        ]));
    }

    // Bottom border
    let bottom_dashes = (width as usize).saturating_sub(2);
    lines.push(Line::from(vec![Span::styled(
        format!("╰{}╯", "─".repeat(bottom_dashes)),
        Style::default().fg(theme.brand_accent),
    )]));

    lines
}

/// Style 3: Modern left accent rail
fn render_todo_rail<'a>(plan: &'a LivePlanBlock, width: u16, theme: &'a Theme) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(plan.tasks.len() + 2);

    // Header line
    lines.push(Line::from(vec![
        Span::styled(
            "▎ ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "📋 TODO · ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{}/{} Completed · [{}]",
                plan.completed_tasks, plan.total_tasks, plan.title
            ),
            Style::default().fg(theme.muted),
        ),
    ]));

    // Tasks
    for task in &plan.tasks {
        let (bullet, bullet_color, text_color, modifier) = match task.status {
            LivePlanTaskStatus::Completed => ("✔ ", theme.success, theme.muted, Modifier::empty()),
            LivePlanTaskStatus::InProgress => {
                ("▶ ", theme.info, theme.text_primary, Modifier::BOLD)
            }
            LivePlanTaskStatus::Pending => ("○ ", theme.muted, theme.muted, Modifier::empty()),
        };

        lines.push(Line::from(vec![
            Span::styled("▎   ", Style::default().fg(theme.brand_accent)),
            Span::styled(
                bullet,
                Style::default().fg(bullet_color).add_modifier(modifier),
            ),
            Span::styled(
                task.title.clone(),
                Style::default().fg(text_color).add_modifier(modifier),
            ),
        ]));
    }

    // Divider line
    lines.push(Line::from(vec![Span::styled(
        "─".repeat((width as usize).max(10)),
        Style::default().fg(theme.border),
    )]));

    lines
}

/// Style 4: Clean minimalist top rule and open layout
fn render_todo_minimal<'a>(plan: &'a LivePlanBlock, width: u16, theme: &'a Theme) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(plan.tasks.len() + 2);

    // Header line with dashes
    let left_header = format!(
        "📋 TODO · {}/{} Completed ",
        plan.completed_tasks, plan.total_tasks
    );
    let right_tag = format!(" [{}]", plan.title);
    let left_len = UnicodeWidthStr::width(left_header.as_str());
    let right_len = UnicodeWidthStr::width(right_tag.as_str());
    let middle_dashes = (width as usize).saturating_sub(left_len + right_len);
    let full_rule = format!("{}{}{}", left_header, "─".repeat(middle_dashes), right_tag);

    lines.push(Line::from(vec![Span::styled(
        full_rule,
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    )]));

    // Tasks with clean indentation
    for task in &plan.tasks {
        let (bullet, bullet_color, text_color, modifier) = match task.status {
            LivePlanTaskStatus::Completed => ("✔ ", theme.success, theme.muted, Modifier::empty()),
            LivePlanTaskStatus::InProgress => {
                ("▶ ", theme.info, theme.text_primary, Modifier::BOLD)
            }
            LivePlanTaskStatus::Pending => ("○ ", theme.muted, theme.muted, Modifier::empty()),
        };

        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                bullet,
                Style::default().fg(bullet_color).add_modifier(modifier),
            ),
            Span::styled(
                task.title.clone(),
                Style::default().fg(text_color).add_modifier(modifier),
            ),
        ]));
    }

    // Divider line
    lines.push(Line::from(vec![Span::styled(
        "─".repeat((width as usize).max(10)),
        Style::default().fg(theme.border),
    )]));

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::view::LivePlanTaskItem;

    fn sample_plan() -> LivePlanBlock {
        LivePlanBlock {
            title: "Phase 142".to_string(),
            total_tasks: 3,
            completed_tasks: 1,
            active_task: Some("T2: Form validation".to_string()),
            tasks: vec![
                LivePlanTaskItem {
                    title: "T1: Scaffolding".to_string(),
                    status: LivePlanTaskStatus::Completed,
                },
                LivePlanTaskItem {
                    title: "T2: Form validation".to_string(),
                    status: LivePlanTaskStatus::InProgress,
                },
                LivePlanTaskItem {
                    title: "T3: Verification".to_string(),
                    status: LivePlanTaskStatus::Pending,
                },
            ],
        }
    }

    #[test]
    fn test_todo_widget_required_height() {
        let plan = Some(sample_plan());
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Tree), 7);
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Card), 7);
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Rail), 6);
        assert_eq!(
            todo_widget_required_height(&plan, TodoWidgetStyle::Minimal),
            6
        );

        // Empty plan should return 0
        let empty_plan = Some(LivePlanBlock {
            title: "Empty".to_string(),
            total_tasks: 0,
            completed_tasks: 0,
            active_task: None,
            tasks: Vec::new(),
        });
        assert_eq!(
            todo_widget_required_height(&empty_plan, TodoWidgetStyle::Tree),
            0
        );
        assert_eq!(todo_widget_required_height(&None, TodoWidgetStyle::Tree), 0);
    }

    #[test]
    fn test_render_all_styles() {
        let plan = sample_plan();
        let theme = Theme::default();

        let tree_lines = render_todo_widget(&plan, TodoWidgetStyle::Tree, 80, &theme);
        assert_eq!(tree_lines.len(), 7);
        assert!(tree_lines[0].spans.is_empty() || tree_lines[0].spans[0].content.is_empty());

        let card_lines = render_todo_widget(&plan, TodoWidgetStyle::Card, 80, &theme);
        assert_eq!(card_lines.len(), 7);
        assert!(card_lines[0].spans.is_empty() || card_lines[0].spans[0].content.is_empty());

        let rail_lines = render_todo_widget(&plan, TodoWidgetStyle::Rail, 80, &theme);
        assert_eq!(rail_lines.len(), 6);
        assert!(rail_lines[0].spans.is_empty() || rail_lines[0].spans[0].content.is_empty());

        let minimal_lines = render_todo_widget(&plan, TodoWidgetStyle::Minimal, 80, &theme);
        assert_eq!(minimal_lines.len(), 6);
        assert!(minimal_lines[0].spans.is_empty() || minimal_lines[0].spans[0].content.is_empty());
    }
}
