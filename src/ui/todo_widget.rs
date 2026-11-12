//! Inline Todo & Task Plan TUI Widget rendered above the input box.
//!
//! Provides 4 configurable visual styles:
//! - `Tree` (Default): Oh My Pi tree style (`TODO \n └── Plan · 1/3 \n ├── ✔ T1`)
//! - `Card`: Rounded container card (`╭── 📋 TODO ──╮`)
//! - `Rail`: Modern left accent rail (`▎ 📋 TODO`)
//! - `Minimal`: Clean open top rule (`📋 TODO ────── [Plan]`)

use crate::config::TodoWidgetStyle;
use crate::ui::theme::Theme;
use crate::ui::view::{LivePlanBlock, LivePlanTaskItem, LivePlanTaskStatus};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Truncates a single-line todo title so that its display width never exceeds `max_cols`,
/// appending `"..."` at the end when truncated so the user always sees a clean ellipsis
/// instead of hard-clipped words at the right terminal edge.
pub fn truncate_todo_line(text: &str, max_cols: usize) -> String {
    let cleaned = text.replace("**", "");
    if max_cols == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(cleaned.as_str()) <= max_cols {
        return cleaned;
    }
    if max_cols <= 3 {
        return ".".repeat(max_cols);
    }
    let target = max_cols - 3;
    let mut out = String::with_capacity(target + 3);
    let mut used = 0usize;
    for ch in cleaned.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > target {
            break;
        }
        out.push(ch);
        used += w;
    }
    let trimmed = out.trim_end();
    format!("{trimmed}...")
}

/// Dynamically filters tasks for the inline TUI widget.
/// Displays up to 5 Tier-1 milestone tasks around the active `[>]` task, with any
/// active Tier-2 tactical sub-tasks (`sub_tasks`) expanded directly underneath their parent.
pub fn filter_inline_tasks(tasks: &[LivePlanTaskItem]) -> Vec<&LivePlanTaskItem> {
    const MAX_INLINE_TASKS: usize = 5;
    if tasks.is_empty() {
        return Vec::new();
    }
    if tasks.len() <= MAX_INLINE_TASKS {
        return tasks.iter().collect();
    }
    // Sliding window of MAX_INLINE_TASKS around the active or next pending task
    let focus_idx = tasks
        .iter()
        .position(|t| t.status == LivePlanTaskStatus::InProgress)
        .or_else(|| {
            tasks
                .iter()
                .position(|t| t.status == LivePlanTaskStatus::Pending)
        })
        .unwrap_or(tasks.len().saturating_sub(1));
    let start = focus_idx
        .saturating_sub(2)
        .min(tasks.len().saturating_sub(MAX_INLINE_TASKS));
    let end = (start + MAX_INLINE_TASKS).min(tasks.len());
    tasks[start..end].iter().collect()
}

/// Calculates the exact required row height for the inline todo widget.
/// Returns 0 when no active plan tasks are present (e.g. after `archive_plan`).
pub fn todo_widget_required_height(plan: &Option<LivePlanBlock>, style: TodoWidgetStyle) -> u16 {
    match plan {
        Some(p) if !p.tasks.is_empty() => {
            let tasks = filter_inline_tasks(&p.tasks);
            let total_count: u16 = tasks.iter().map(|t| 1 + t.sub_tasks.len() as u16).sum();
            match style {
                TodoWidgetStyle::Tree => total_count + 4,
                TodoWidgetStyle::Card => total_count + 4,
                TodoWidgetStyle::Rail => total_count + 3,
                TodoWidgetStyle::Minimal => total_count + 3,
                TodoWidgetStyle::Checklist => total_count + 3,
                TodoWidgetStyle::Pipeline => total_count + 3,
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
        TodoWidgetStyle::Checklist => render_todo_checklist(plan, width, theme),
        TodoWidgetStyle::Pipeline => render_todo_pipeline(plan, width, theme),
    };
    lines.extend(content_lines);
    lines
}

fn task_status_visuals(
    status: LivePlanTaskStatus,
    theme: &Theme,
) -> (
    &'static str,
    ratatui::style::Color,
    ratatui::style::Color,
    Modifier,
    Modifier,
) {
    match status {
        LivePlanTaskStatus::Completed => (
            "✓ ",
            theme.success,
            theme.muted,
            Modifier::empty(),
            Modifier::CROSSED_OUT,
        ),
        LivePlanTaskStatus::InProgress => (
            "> ",
            theme.info,
            theme.text_primary,
            Modifier::BOLD,
            Modifier::BOLD,
        ),
        LivePlanTaskStatus::Pending => (
            "○ ",
            theme.muted,
            theme.muted,
            Modifier::empty(),
            Modifier::empty(),
        ),
    }
}

fn push_tree_task_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    tasks: &[&'a LivePlanTaskItem],
    width: u16,
    theme: &'a Theme,
    card_prefix: bool,
) {
    let card_cols: usize = if card_prefix { 2 } else { 0 };
    // Prefix: card_cols + 8 ("    ├── ") + 2 ("✓ ") = 10 + card_cols; plus 2 cols right margin
    let max_task_cols = (width as usize).saturating_sub(card_cols + 10 + 2).max(8);
    // Sub-task prefix: card_cols + 8 ("    │   ") + 4 ("├── ") + 2 ("✓ ") = 14 + card_cols; plus 2 cols right margin
    let max_sub_cols = (width as usize).saturating_sub(card_cols + 14 + 2).max(8);

    let task_count = tasks.len();
    for (idx, task) in tasks.iter().enumerate() {
        let is_last = idx == task_count - 1;
        let branch = if is_last {
            "    └── "
        } else {
            "    ├── "
        };
        let (bullet, bullet_color, text_color, bullet_mod, text_mod) =
            task_status_visuals(task.status, theme);
        let raw_title = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&task.title);
        let clean_title = truncate_todo_line(&raw_title, max_task_cols);

        let mut spans = Vec::with_capacity(4);
        if card_prefix {
            spans.push(Span::styled("│ ", Style::default().fg(theme.brand_accent)));
        }
        spans.push(Span::styled(branch, Style::default().fg(theme.muted)));
        spans.push(Span::styled(
            bullet,
            Style::default().fg(bullet_color).add_modifier(bullet_mod),
        ));
        spans.push(Span::styled(
            clean_title,
            Style::default().fg(text_color).add_modifier(text_mod),
        ));
        lines.push(Line::from(spans));

        let sub_count = task.sub_tasks.len();
        for (s_idx, sub) in task.sub_tasks.iter().enumerate() {
            let is_sub_last = s_idx == sub_count - 1;
            let parent_rail = if idx == task_count - 1 {
                "        "
            } else {
                "    │   "
            };
            let sub_branch = if is_sub_last {
                "└── "
            } else {
                "├── "
            };
            let (s_bullet, s_bullet_color, s_text_color, s_bullet_mod, s_text_mod) =
                task_status_visuals(sub.status, theme);
            let raw_sub =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&sub.title);
            let clean_sub = truncate_todo_line(&raw_sub, max_sub_cols);

            let mut sub_spans = Vec::with_capacity(5);
            if card_prefix {
                sub_spans.push(Span::styled("│ ", Style::default().fg(theme.brand_accent)));
            }
            sub_spans.push(Span::styled(parent_rail, Style::default().fg(theme.muted)));
            sub_spans.push(Span::styled(sub_branch, Style::default().fg(theme.muted)));
            sub_spans.push(Span::styled(
                s_bullet,
                Style::default()
                    .fg(s_bullet_color)
                    .add_modifier(s_bullet_mod),
            ));
            sub_spans.push(Span::styled(
                clean_sub,
                Style::default().fg(s_text_color).add_modifier(s_text_mod),
            ));
            lines.push(Line::from(sub_spans));
        }
    }
}

fn push_flat_task_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    tasks: &[&'a LivePlanTaskItem],
    width: u16,
    theme: &'a Theme,
    rail_style: bool,
) {
    let prefix_style = if rail_style {
        Style::default().fg(theme.brand_accent)
    } else {
        Style::default()
    };
    let task_prefix = if rail_style { "▎   " } else { "   " };
    let sub_prefix = if rail_style { "▎       " } else { "       " };
    let task_prefix_cols: usize = if rail_style { 6 } else { 5 };
    let sub_prefix_cols: usize = if rail_style { 14 } else { 13 };
    let max_task_cols = (width as usize).saturating_sub(task_prefix_cols + 2).max(8);
    let max_sub_cols = (width as usize).saturating_sub(sub_prefix_cols + 2).max(8);

    for task in tasks {
        let (bullet, bullet_color, text_color, bullet_mod, text_mod) =
            task_status_visuals(task.status, theme);
        let raw_title = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&task.title);
        let clean_title = truncate_todo_line(&raw_title, max_task_cols);
        lines.push(Line::from(vec![
            Span::styled(task_prefix, prefix_style),
            Span::styled(
                bullet,
                Style::default().fg(bullet_color).add_modifier(bullet_mod),
            ),
            Span::styled(
                clean_title,
                Style::default().fg(text_color).add_modifier(text_mod),
            ),
        ]));

        for (s_idx, sub) in task.sub_tasks.iter().enumerate() {
            let is_sub_last = s_idx == task.sub_tasks.len() - 1;
            let sub_branch = if is_sub_last {
                "└── "
            } else {
                "├── "
            };
            let (s_bullet, s_bullet_color, s_text_color, s_bullet_mod, s_text_mod) =
                task_status_visuals(sub.status, theme);
            let raw_sub =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&sub.title);
            let clean_sub = truncate_todo_line(&raw_sub, max_sub_cols);
            lines.push(Line::from(vec![
                Span::styled(sub_prefix, prefix_style),
                Span::styled(sub_branch, Style::default().fg(theme.muted)),
                Span::styled(
                    s_bullet,
                    Style::default()
                        .fg(s_bullet_color)
                        .add_modifier(s_bullet_mod),
                ),
                Span::styled(
                    clean_sub,
                    Style::default().fg(s_text_color).add_modifier(s_text_mod),
                ),
            ]));
        }
    }
}

/// Formats the header progress counter so activating a Tier-2 sub-plan on a multi-step roadmap
/// never resets the counter backwards (e.g., `6/8` -> `7/8 (sub 0/3)` -> `7/8 (sub 1/3)` -> `8/8`).
pub fn format_plan_progress_counts(plan: &LivePlanBlock) -> String {
    if plan.tasks.len() > 1 {
        if let Some(active_macro) = plan
            .tasks
            .iter()
            .find(|t| t.status == LivePlanTaskStatus::InProgress && !t.sub_tasks.is_empty())
        {
            let macro_done = plan
                .tasks
                .iter()
                .filter(|t| t.status == LivePlanTaskStatus::Completed)
                .count();
            let macro_total = plan.tasks.len();
            let sub_done = active_macro
                .sub_tasks
                .iter()
                .filter(|s| s.status == LivePlanTaskStatus::Completed)
                .count();
            let sub_total = active_macro.sub_tasks.len();
            return format!(
                "{}/{} (sub {}/{})",
                macro_done, macro_total, sub_done, sub_total
            );
        }
    }
    format!("{}/{}", plan.completed_tasks, plan.total_tasks)
}

/// Style 1: Oh My Pi tree layout (Default)
fn render_todo_tree<'a>(plan: &'a LivePlanBlock, width: u16, theme: &'a Theme) -> Vec<Line<'a>> {
    let tasks = filter_inline_tasks(&plan.tasks);
    let mut lines = Vec::with_capacity(tasks.len() + 3);

    // Line 1: Header "TODO" in bold cyan/info
    lines.push(Line::from(vec![Span::styled(
        "TODO",
        Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
    )]));

    // Line 2: "└── Plan · 1/3" (or "7/8 (sub 1/3)" when a sub-plan is active)
    let raw_subheader = if plan.title.trim().is_empty() {
        "Plan"
    } else {
        plan.title.as_str()
    };
    let count_label = format_plan_progress_counts(plan);
    let max_subheader_cols = (width as usize)
        .saturating_sub(4 + 3 + count_label.len() + 2)
        .max(8);
    let subheader_title = truncate_todo_line(raw_subheader, max_subheader_cols);
    lines.push(Line::from(vec![
        Span::styled("└── ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} · ", subheader_title),
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ),
        Span::styled(count_label, Style::default().fg(theme.muted)),
    ]));

    push_tree_task_lines(&mut lines, &tasks, width, theme, false);

    // Divider line below the tree
    let divider_width = (width.saturating_sub(1) as usize).max(10);
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(divider_width),
        Style::default().fg(theme.border),
    )]));

    lines
}

/// Style 2: Rounded container card with interior tree
fn render_todo_card<'a>(plan: &'a LivePlanBlock, width: u16, theme: &'a Theme) -> Vec<Line<'a>> {
    let tasks = filter_inline_tasks(&plan.tasks);
    let mut lines = Vec::with_capacity(tasks.len() + 3);
    let counts = format_plan_progress_counts(plan);

    // Top border
    let left_header = format!("╭── TODO · {} Tasks Completed ──", counts);
    let right_tag = format!("── [{}] ──╮", plan.title);
    let left_len = UnicodeWidthStr::width(left_header.as_str());
    let right_len = UnicodeWidthStr::width(right_tag.as_str());
    let middle_dashes = (width.saturating_sub(1) as usize).saturating_sub(left_len + right_len);
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

    push_tree_task_lines(&mut lines, &tasks, width, theme, true);

    // Bottom border
    let bottom_dashes = (width.saturating_sub(1) as usize).saturating_sub(2);
    lines.push(Line::from(vec![Span::styled(
        format!("╰{}╯", "─".repeat(bottom_dashes)),
        Style::default().fg(theme.brand_accent),
    )]));

    lines
}

/// Style 3: Modern left accent rail
fn render_todo_rail<'a>(plan: &'a LivePlanBlock, width: u16, theme: &'a Theme) -> Vec<Line<'a>> {
    let tasks = filter_inline_tasks(&plan.tasks);
    let mut lines = Vec::with_capacity(tasks.len() + 2);
    let counts = format_plan_progress_counts(plan);

    // Header line
    lines.push(Line::from(vec![
        Span::styled(
            "▎ ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "TODO · ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} Completed · [{}]", counts, plan.title),
            Style::default().fg(theme.muted),
        ),
    ]));

    push_flat_task_lines(&mut lines, &tasks, width, theme, true);

    // Divider line
    lines.push(Line::from(vec![Span::styled(
        "─".repeat((width.saturating_sub(1) as usize).max(10)),
        Style::default().fg(theme.border),
    )]));

    lines
}

/// Style 4: Clean minimalist top rule and open layout
fn render_todo_minimal<'a>(plan: &'a LivePlanBlock, width: u16, theme: &'a Theme) -> Vec<Line<'a>> {
    let tasks = filter_inline_tasks(&plan.tasks);
    let mut lines = Vec::with_capacity(tasks.len() + 2);
    let counts = format_plan_progress_counts(plan);

    // Header line with dashes
    let left_header = format!("TODO · {} Completed ", counts);
    let right_tag = format!(" [{}]", plan.title);
    let left_len = UnicodeWidthStr::width(left_header.as_str());
    let right_len = UnicodeWidthStr::width(right_tag.as_str());
    let middle_dashes = (width.saturating_sub(1) as usize).saturating_sub(left_len + right_len);
    let full_rule = format!("{}{}{}", left_header, "─".repeat(middle_dashes), right_tag);

    lines.push(Line::from(vec![Span::styled(
        full_rule,
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    )]));

    push_flat_task_lines(&mut lines, &tasks, width, theme, false);

    // Divider line
    lines.push(Line::from(vec![Span::styled(
        "─".repeat((width.saturating_sub(1) as usize).max(10)),
        Style::default().fg(theme.border),
    )]));

    lines
}

/// Style 5: Markdown Checklist (`[x]`, `[>]`, `[ ]`)
fn render_todo_checklist<'a>(
    plan: &'a LivePlanBlock,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let tasks = filter_inline_tasks(&plan.tasks);
    let mut lines = Vec::with_capacity(tasks.len() + 2);
    let max_task_cols = (width as usize).saturating_sub(8).max(8);
    let max_sub_cols = (width as usize).saturating_sub(12).max(8);
    let counts = format_plan_progress_counts(plan);

    lines.push(Line::from(vec![
        Span::styled(
            "◆ CHECKLIST · ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} Done · [{}]", counts, plan.title),
            Style::default().fg(theme.muted),
        ),
    ]));

    for task in &tasks {
        let (box_glyph, box_color, text_color, box_mod, text_mod) = match task.status {
            LivePlanTaskStatus::Completed => (
                "[x] ",
                theme.success,
                theme.muted,
                Modifier::empty(),
                Modifier::CROSSED_OUT,
            ),
            LivePlanTaskStatus::InProgress => (
                "[>] ",
                theme.info,
                theme.text_primary,
                Modifier::BOLD,
                Modifier::BOLD,
            ),
            LivePlanTaskStatus::Pending => (
                "[ ] ",
                theme.muted,
                theme.muted,
                Modifier::empty(),
                Modifier::empty(),
            ),
        };
        let raw_title = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&task.title);
        let clean_title = truncate_todo_line(&raw_title, max_task_cols);
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                box_glyph,
                Style::default().fg(box_color).add_modifier(box_mod),
            ),
            Span::styled(
                clean_title,
                Style::default().fg(text_color).add_modifier(text_mod),
            ),
        ]));

        for sub in &task.sub_tasks {
            let (s_box, s_color, s_text, s_box_mod, s_text_mod) = match sub.status {
                LivePlanTaskStatus::Completed => (
                    "[x] ",
                    theme.success,
                    theme.muted,
                    Modifier::empty(),
                    Modifier::CROSSED_OUT,
                ),
                LivePlanTaskStatus::InProgress => (
                    "[>] ",
                    theme.info,
                    theme.text_primary,
                    Modifier::BOLD,
                    Modifier::BOLD,
                ),
                LivePlanTaskStatus::Pending => (
                    "[ ] ",
                    theme.muted,
                    theme.muted,
                    Modifier::empty(),
                    Modifier::empty(),
                ),
            };
            let raw_sub =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&sub.title);
            let clean_sub = truncate_todo_line(&raw_sub, max_sub_cols);
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(s_box, Style::default().fg(s_color).add_modifier(s_box_mod)),
                Span::styled(
                    clean_sub,
                    Style::default().fg(s_text).add_modifier(s_text_mod),
                ),
            ]));
        }
    }

    lines.push(Line::from(vec![Span::styled(
        "─".repeat((width.saturating_sub(1) as usize).max(10)),
        Style::default().fg(theme.border),
    )]));

    lines
}

/// Style 6: Stepper Pipeline (`●`, `◈`, `○` connected by vertical spine)
fn render_todo_pipeline<'a>(
    plan: &'a LivePlanBlock,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let tasks = filter_inline_tasks(&plan.tasks);
    let mut lines = Vec::with_capacity(tasks.len() + 2);
    let max_task_cols = (width as usize).saturating_sub(8).max(8);
    let max_sub_cols = (width as usize).saturating_sub(10).max(8);
    let counts = format_plan_progress_counts(plan);

    lines.push(Line::from(vec![
        Span::styled(
            "◈ PIPELINE · ",
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("Step {} · {}", counts, plan.title),
            Style::default().fg(theme.muted),
        ),
    ]));

    for task in &tasks {
        let (node_glyph, node_color, text_color, node_mod, text_mod) = match task.status {
            LivePlanTaskStatus::Completed => (
                "●── ",
                theme.success,
                theme.muted,
                Modifier::empty(),
                Modifier::CROSSED_OUT,
            ),
            LivePlanTaskStatus::InProgress => (
                "◈── ",
                theme.brand_accent,
                theme.text_primary,
                Modifier::BOLD,
                Modifier::BOLD,
            ),
            LivePlanTaskStatus::Pending => (
                "○── ",
                theme.muted,
                theme.muted,
                Modifier::empty(),
                Modifier::empty(),
            ),
        };
        let raw_title = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&task.title);
        let clean_title = truncate_todo_line(&raw_title, max_task_cols);
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                node_glyph,
                Style::default().fg(node_color).add_modifier(node_mod),
            ),
            Span::styled(
                clean_title,
                Style::default().fg(text_color).add_modifier(text_mod),
            ),
        ]));

        for sub in &task.sub_tasks {
            let (s_bullet, s_color, s_text, s_bullet_mod, s_text_mod) =
                task_status_visuals(sub.status, theme);
            let raw_sub =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&sub.title);
            let clean_sub = truncate_todo_line(&raw_sub, max_sub_cols);
            lines.push(Line::from(vec![
                Span::styled("  │   ", Style::default().fg(theme.muted)),
                Span::styled(
                    s_bullet,
                    Style::default().fg(s_color).add_modifier(s_bullet_mod),
                ),
                Span::styled(
                    clean_sub,
                    Style::default().fg(s_text).add_modifier(s_text_mod),
                ),
            ]));
        }
    }

    lines.push(Line::from(vec![Span::styled(
        "─".repeat((width.saturating_sub(1) as usize).max(10)),
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
                    sub_tasks: Vec::new(),
                },
                LivePlanTaskItem {
                    title: "T2: Form validation".to_string(),
                    status: LivePlanTaskStatus::InProgress,
                    sub_tasks: vec![
                        LivePlanTaskItem {
                            title: "T2.1: Schema definition".to_string(),
                            status: LivePlanTaskStatus::Completed,
                            sub_tasks: Vec::new(),
                        },
                        LivePlanTaskItem {
                            title: "T2.2: Field validators".to_string(),
                            status: LivePlanTaskStatus::InProgress,
                            sub_tasks: Vec::new(),
                        },
                    ],
                },
                LivePlanTaskItem {
                    title: "T3: Verification".to_string(),
                    status: LivePlanTaskStatus::Pending,
                    sub_tasks: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn test_todo_widget_required_height() {
        let plan = Some(sample_plan());
        // In sample_plan, all 3 Tier-1 tasks (T1, T2, T3) + T2's 2 Tier-2 subtasks are shown (5 total items).
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Tree), 9);
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Card), 9);
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Rail), 8);
        assert_eq!(
            todo_widget_required_height(&plan, TodoWidgetStyle::Minimal),
            8
        );

        // 100% completed plan remains renderable while unarchived (before `archive_plan` clears tasks)
        let completed_unarchived = Some(LivePlanBlock {
            title: "Plan".to_string(),
            total_tasks: 2,
            completed_tasks: 2,
            active_task: None,
            tasks: vec![
                LivePlanTaskItem {
                    title: "Step 1".to_string(),
                    status: LivePlanTaskStatus::Completed,
                    sub_tasks: Vec::new(),
                },
                LivePlanTaskItem {
                    title: "Step 2".to_string(),
                    status: LivePlanTaskStatus::Completed,
                    sub_tasks: Vec::new(),
                },
            ],
        });
        assert_eq!(
            todo_widget_required_height(&completed_unarchived, TodoWidgetStyle::Tree),
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
        assert_eq!(tree_lines.len(), 9);
        assert!(tree_lines[0].spans.is_empty() || tree_lines[0].spans[0].content.is_empty());

        let card_lines = render_todo_widget(&plan, TodoWidgetStyle::Card, 80, &theme);
        assert_eq!(card_lines.len(), 9);
        assert!(card_lines[0].spans.is_empty() || card_lines[0].spans[0].content.is_empty());

        let rail_lines = render_todo_widget(&plan, TodoWidgetStyle::Rail, 80, &theme);
        assert_eq!(rail_lines.len(), 8);
        assert!(rail_lines[0].spans.is_empty() || rail_lines[0].spans[0].content.is_empty());

        let minimal_lines = render_todo_widget(&plan, TodoWidgetStyle::Minimal, 80, &theme);
        assert_eq!(minimal_lines.len(), 8);
        assert!(minimal_lines[0].spans.is_empty() || minimal_lines[0].spans[0].content.is_empty());
    }

    #[test]
    fn test_tier_two_nested_subplan_tree_rendering() {
        let plan = LivePlanBlock {
            title: "Active Milestone: Landing Page".to_string(),
            total_tasks: 3,
            completed_tasks: 1,
            active_task: Some("Build interactive TerminalPreview sub-component".to_string()),
            tasks: vec![
                LivePlanTaskItem {
                    title: "1. Design tokens & shared primitives".to_string(),
                    status: LivePlanTaskStatus::Completed,
                    sub_tasks: Vec::new(),
                },
                LivePlanTaskItem {
                    title: "2. Create Hero section".to_string(),
                    status: LivePlanTaskStatus::InProgress,
                    sub_tasks: vec![
                        LivePlanTaskItem {
                            title: "Define Hero layout grid and typography hierarchy".to_string(),
                            status: LivePlanTaskStatus::Completed,
                            sub_tasks: Vec::new(),
                        },
                        LivePlanTaskItem {
                            title: "Build interactive TerminalPreview sub-component".to_string(),
                            status: LivePlanTaskStatus::InProgress,
                            sub_tasks: Vec::new(),
                        },
                        LivePlanTaskItem {
                            title: "Assemble HeroSection.tsx with CTA buttons and badge"
                                .to_string(),
                            status: LivePlanTaskStatus::Pending,
                            sub_tasks: Vec::new(),
                        },
                    ],
                },
                LivePlanTaskItem {
                    title: "3. Build Code Intelligence section".to_string(),
                    status: LivePlanTaskStatus::Pending,
                    sub_tasks: Vec::new(),
                },
            ],
        };
        let theme = Theme::default();
        let tree_lines = render_todo_widget(&plan, TodoWidgetStyle::Tree, 100, &theme);
        let rendered: Vec<String> = tree_lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        assert_eq!(
            rendered[3],
            "    ├── ✓ 1. Design tokens & shared primitives"
        );
        assert_eq!(rendered[4], "    ├── > 2. Create Hero section");
        assert_eq!(
            rendered[5],
            "    │   ├── ✓ Define Hero layout grid and typography hierarchy"
        );
        assert_eq!(
            rendered[6],
            "    │   ├── > Build interactive TerminalPreview sub-component"
        );
        assert_eq!(
            rendered[7],
            "    │   └── ○ Assemble HeroSection.tsx with CTA buttons and badge"
        );
        assert_eq!(rendered[8], "    └── ○ 3. Build Code Intelligence section");
    }

    #[test]
    fn test_render_without_subtasks() {
        let plan = LivePlanBlock {
            title: "Milestone".to_string(),
            total_tasks: 2,
            completed_tasks: 1,
            active_task: Some("T2".to_string()),
            tasks: vec![
                LivePlanTaskItem {
                    title: "T1".to_string(),
                    status: LivePlanTaskStatus::Completed,
                    sub_tasks: Vec::new(),
                },
                LivePlanTaskItem {
                    title: "T2".to_string(),
                    status: LivePlanTaskStatus::InProgress,
                    sub_tasks: Vec::new(),
                },
            ],
        };
        let theme = Theme::default();
        let tree_lines = render_todo_widget(&plan, TodoWidgetStyle::Tree, 80, &theme);
        assert_eq!(tree_lines.len(), 6); // 1 spacer + 1 header + 1 subheader + 2 tasks + 1 divider

        // Line 3 is T1 (Completed): "✓ " without strikethrough, "T1" with Modifier::CROSSED_OUT
        let t1_line = &tree_lines[3];
        assert_eq!(t1_line.spans[1].content.as_ref(), "✓ ");
        assert!(!t1_line.spans[1]
            .style
            .add_modifier
            .contains(Modifier::CROSSED_OUT));
        assert_eq!(t1_line.spans[2].content.as_ref(), "T1");
        assert!(t1_line.spans[2]
            .style
            .add_modifier
            .contains(Modifier::CROSSED_OUT));

        // Line 4 is T2 (InProgress): "> " minimal arrow with Modifier::BOLD
        let t2_line = &tree_lines[4];
        assert_eq!(t2_line.spans[1].content.as_ref(), "> ");
        assert!(t2_line.spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(t2_line.spans[2].content.as_ref(), "T2");
        assert!(t2_line.spans[2].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_long_todo_lines_truncated_with_ellipsis() {
        let long_task = "Verify: typecheck, lint, build, browser QA 3 viewports: bun run build (tsc -b + vite build), bun run lint, dev server via minitask";
        let plan = LivePlanBlock {
            title: "Active Milestone: minicode landing page".to_string(),
            total_tasks: 1,
            completed_tasks: 0,
            active_task: Some(long_task.to_string()),
            tasks: vec![LivePlanTaskItem {
                title: long_task.to_string(),
                status: LivePlanTaskStatus::InProgress,
                sub_tasks: Vec::new(),
            }],
        };
        let theme = Theme::default();
        let width: u16 = 80;
        let tree_lines = render_todo_widget(&plan, TodoWidgetStyle::Tree, width, &theme);
        let task_line = &tree_lines[3];
        let rendered_title = task_line.spans[2].content.as_ref();
        assert!(
            rendered_title.ends_with("..."),
            "Expected truncated task line to end with '...', got: {rendered_title}"
        );
        let total_line_width: usize = task_line
            .spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        assert!(
            total_line_width <= width as usize,
            "Expected line width {total_line_width} <= {width}"
        );
    }

    #[test]
    fn test_filter_inline_tasks_preserves_five_rows_across_9_step_plan() {
        let mut tasks: Vec<LivePlanTaskItem> = (1..=9)
            .map(|i| LivePlanTaskItem {
                title: format!("Step {i}"),
                status: if i < 4 {
                    LivePlanTaskStatus::Completed
                } else if i == 4 {
                    LivePlanTaskStatus::InProgress
                } else {
                    LivePlanTaskStatus::Pending
                },
                sub_tasks: Vec::new(),
            })
            .collect();

        // Active Step 4 (`idx = 3`): shows 2 completed checkmarks above (`Step 2`, `Step 3`), `Step 4`, and `Step 5`, `Step 6`
        let mid_window = filter_inline_tasks(&tasks);
        let mid_titles: Vec<&str> = mid_window.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            mid_titles,
            vec!["Step 2", "Step 3", "Step 4", "Step 5", "Step 6"]
        );

        // Active Step 9 (`idx = 8`): still shows 5 full rows (`Step 5`..=`Step 9`) instead of shrinking to 3 rows
        for (idx, t) in tasks.iter_mut().enumerate() {
            t.status = if idx < 8 {
                LivePlanTaskStatus::Completed
            } else {
                LivePlanTaskStatus::InProgress
            };
        }
        let end_window = filter_inline_tasks(&tasks);
        let end_titles: Vec<&str> = end_window.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            end_titles,
            vec!["Step 5", "Step 6", "Step 7", "Step 8", "Step 9"]
        );
    }

    #[test]
    fn test_subplan_header_preserves_monotonic_macro_and_sub_counts() {
        let mut tasks: Vec<LivePlanTaskItem> = (1..=8)
            .map(|i| LivePlanTaskItem {
                title: format!("Step {i}"),
                status: if i < 8 {
                    LivePlanTaskStatus::Completed
                } else {
                    LivePlanTaskStatus::InProgress
                },
                sub_tasks: Vec::new(),
            })
            .collect();
        tasks[7].sub_tasks = vec![
            LivePlanTaskItem {
                title: "Run production build, lint, and typecheck".to_string(),
                status: LivePlanTaskStatus::Completed,
                sub_tasks: Vec::new(),
            },
            LivePlanTaskItem {
                title: "Start dev server and verify desktop browser layout & QA".to_string(),
                status: LivePlanTaskStatus::InProgress,
                sub_tasks: Vec::new(),
            },
            LivePlanTaskItem {
                title: "Verify responsive viewports (tablet & mobile) and interactive flows"
                    .to_string(),
                status: LivePlanTaskStatus::Pending,
                sub_tasks: Vec::new(),
            },
        ];

        let plan = LivePlanBlock {
            title: "Active Milestone: Landing Page".to_string(),
            total_tasks: 3,
            completed_tasks: 1,
            active_task: Some(
                "Start dev server and verify desktop browser layout & QA".to_string(),
            ),
            tasks,
        };

        assert_eq!(format_plan_progress_counts(&plan), "7/8 (sub 1/3)");

        let theme = Theme::default();
        let tree_lines = render_todo_widget(&plan, TodoWidgetStyle::Tree, 100, &theme);
        let subheader: String = tree_lines[2]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            subheader.contains("7/8 (sub 1/3)"),
            "Expected subheader to preserve macro + sub counts, got: {subheader}"
        );
    }
}
