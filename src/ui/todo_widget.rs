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

/// Compact window sizes for the two-mode inline TUI Todo widget:
/// - **Macro Todo Mode**: up to 3 collapsed macro todos (`1 prev ✓, active >, 1 next ○`), 0 sub-todos.
/// - **Sub-Todo Focus Mode**: ONLY the active parent Todo title + up to 4 windowed sub-todos.
const MAX_INLINE_MACRO_TASKS: usize = 3;
const MAX_INLINE_SUB_TASKS: usize = 4;

/// Which display mode the inline Todo widget is currently projecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineTodoMode {
    /// Shows compact collapsed macro todos around the active `>` todo (no sub-todos expanded).
    MacroWindow,
    /// Shows ONLY the active parent Todo title + its sub-todos alone.
    SubTodoFocus {
        step_number: usize,
        macro_total: usize,
        sub_done: usize,
        sub_total: usize,
    },
}

/// A projected row in the inline Todo widget with an explicit slice of visible sub-todos.
#[derive(Debug, Clone)]
pub struct ProjectedTodoTask<'a> {
    pub task: &'a LivePlanTaskItem,
    pub sub_tasks: Vec<&'a LivePlanTaskItem>,
}

/// Complete inline widget projection shared by height calculation and all 6 widget styles.
#[derive(Debug, Clone)]
pub struct InlineTodoProjection<'a> {
    pub mode: InlineTodoMode,
    pub rows: Vec<ProjectedTodoTask<'a>>,
}

impl<'a> InlineTodoProjection<'a> {
    pub fn total_row_count(&self) -> usize {
        self.rows.iter().map(|r| 1 + r.sub_tasks.len()).sum()
    }
}

/// Returns `true` when `task` is the active `InProgress` macro todo AND its sub-todos are
/// actively being worked on (either at least one sub-todo is `InProgress`, or some—but not all—
/// sub-todos are already `Completed`).
pub fn is_task_executing_subtodos(task: &LivePlanTaskItem) -> bool {
    if task.status != LivePlanTaskStatus::InProgress || task.sub_tasks.is_empty() {
        return false;
    }
    let sub_total = task.sub_tasks.len();
    let sub_done = task
        .sub_tasks
        .iter()
        .filter(|s| s.status == LivePlanTaskStatus::Completed)
        .count();
    let has_in_progress_sub = task
        .sub_tasks
        .iter()
        .any(|s| s.status == LivePlanTaskStatus::InProgress);
    has_in_progress_sub || (sub_done > 0 && sub_done < sub_total)
}

fn window_task_slice(tasks: &[LivePlanTaskItem], max_items: usize) -> Vec<&LivePlanTaskItem> {
    if tasks.is_empty() || max_items == 0 {
        return Vec::new();
    }
    if tasks.len() <= max_items {
        return tasks.iter().collect();
    }
    let focus_idx = tasks
        .iter()
        .position(|t| t.status == LivePlanTaskStatus::InProgress)
        .or_else(|| {
            tasks
                .iter()
                .position(|t| t.status == LivePlanTaskStatus::Pending)
        })
        .unwrap_or(tasks.len().saturating_sub(1));
    // Show 1 completed item above the active item when >= 3 rows are available;
    // otherwise anchor directly on the active item so it is never pushed off-screen.
    let lead_context = usize::from(max_items >= 3);
    let start = focus_idx
        .saturating_sub(lead_context)
        .min(tasks.len().saturating_sub(max_items));
    let end = (start + max_items).min(tasks.len());
    tasks[start..end].iter().collect()
}

/// Dynamically filters macro tasks for Macro Todo Mode (`1 prev ✓, active >, 1 next ○`).
pub fn filter_inline_tasks(tasks: &[LivePlanTaskItem]) -> Vec<&LivePlanTaskItem> {
    window_task_slice(tasks, MAX_INLINE_MACRO_TASKS)
}

/// Projects the active plan into either **Sub-Todo Focus Mode** (active Todo title + its sub-todos
/// alone) or **Macro Todo Mode** (compact collapsed macro todos with 0 sub-rows), respecting an
/// optional `max_content_rows` height budget for short split terminals.
pub fn project_inline_todo_view<'a>(
    tasks: &'a [LivePlanTaskItem],
    max_content_rows: Option<usize>,
) -> InlineTodoProjection<'a> {
    if tasks.is_empty() {
        return InlineTodoProjection {
            mode: InlineTodoMode::MacroWindow,
            rows: Vec::new(),
        };
    }

    if let Some(0) = max_content_rows {
        return InlineTodoProjection {
            mode: InlineTodoMode::MacroWindow,
            rows: Vec::new(),
        };
    }

    if let Some((active_idx, active_macro)) = tasks
        .iter()
        .enumerate()
        .find(|(_, t)| is_task_executing_subtodos(t))
    {
        let sub_total = active_macro.sub_tasks.len();
        let sub_done = active_macro
            .sub_tasks
            .iter()
            .filter(|s| s.status == LivePlanTaskStatus::Completed)
            .count();
        let mode = InlineTodoMode::SubTodoFocus {
            step_number: active_idx + 1,
            macro_total: tasks.len(),
            sub_done,
            sub_total,
        };

        let budget = max_content_rows.unwrap_or(1 + MAX_INLINE_SUB_TASKS);
        if budget == 1 {
            // Ultra-tight 1-row terminal fallback: surface the active sub-todo directly
            let focused_sub = window_task_slice(&active_macro.sub_tasks, 1);
            let rows = focused_sub
                .into_iter()
                .map(|sub| ProjectedTodoTask {
                    task: sub,
                    sub_tasks: Vec::new(),
                })
                .collect();
            return InlineTodoProjection { mode, rows };
        }

        let sub_limit = budget.saturating_sub(1).min(MAX_INLINE_SUB_TASKS);
        let sub_tasks = window_task_slice(&active_macro.sub_tasks, sub_limit);
        return InlineTodoProjection {
            mode,
            rows: vec![ProjectedTodoTask {
                task: active_macro,
                sub_tasks,
            }],
        };
    }

    let macro_limit = max_content_rows
        .map(|b| b.min(MAX_INLINE_MACRO_TASKS))
        .unwrap_or(MAX_INLINE_MACRO_TASKS);
    let rows = window_task_slice(tasks, macro_limit)
        .into_iter()
        .map(|task| ProjectedTodoTask {
            task,
            sub_tasks: Vec::new(),
        })
        .collect();

    InlineTodoProjection {
        mode: InlineTodoMode::MacroWindow,
        rows,
    }
}

const fn style_chrome_rows(style: TodoWidgetStyle) -> u16 {
    match style {
        TodoWidgetStyle::Tree | TodoWidgetStyle::Card => 4,
        TodoWidgetStyle::Rail
        | TodoWidgetStyle::Minimal
        | TodoWidgetStyle::Checklist
        | TodoWidgetStyle::Pipeline => 3,
    }
}

/// Calculates the exact required row height for the inline todo widget.
/// Returns 0 when no active plan tasks are present (e.g. after `archive_plan`).
pub fn todo_widget_required_height(plan: &Option<LivePlanBlock>, style: TodoWidgetStyle) -> u16 {
    todo_widget_required_height_bounded(plan, style, None)
}

/// Calculates the exact required row height bounded by `max_height` (if provided).
/// Returns 0 if the plan is empty or if `max_height` cannot fit even 1 task row + chrome.
pub fn todo_widget_required_height_bounded(
    plan: &Option<LivePlanBlock>,
    style: TodoWidgetStyle,
    max_height: Option<u16>,
) -> u16 {
    match plan {
        Some(p) if !p.tasks.is_empty() => {
            let chrome = style_chrome_rows(style);
            let max_content_rows = match max_height {
                Some(h) if h <= chrome => return 0,
                Some(h) => Some((h - chrome) as usize),
                None => None,
            };
            let projected = project_inline_todo_view(&p.tasks, max_content_rows);
            let total_count = projected.total_row_count() as u16;
            if total_count == 0 {
                0
            } else {
                total_count + chrome
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
    render_todo_widget_bounded(plan, style, width, theme, None)
}

/// Renders the inline todo widget bounded to at most `max_height` rows so short split terminals
/// never clip the active `>` todo or the `Thinking...` activity bar underneath.
pub fn render_todo_widget_bounded<'a>(
    plan: &'a LivePlanBlock,
    style: TodoWidgetStyle,
    width: u16,
    theme: &'a Theme,
    max_height: Option<u16>,
) -> Vec<Line<'a>> {
    if plan.tasks.is_empty() {
        return Vec::new();
    }

    let chrome = style_chrome_rows(style);
    let max_content_rows = match max_height {
        Some(h) if h <= chrome => return Vec::new(),
        Some(h) => Some((h - chrome) as usize),
        None => None,
    };
    let projected = project_inline_todo_view(&plan.tasks, max_content_rows);
    if projected.rows.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::with_capacity(projected.total_row_count() + chrome as usize);
    // Top spacing line to visually separate the todo widget from the streaming timeline
    lines.push(Line::from(""));

    let content_lines = match style {
        TodoWidgetStyle::Tree => render_todo_tree(plan, &projected, width, theme),
        TodoWidgetStyle::Card => render_todo_card(plan, &projected, width, theme),
        TodoWidgetStyle::Rail => render_todo_rail(plan, &projected, width, theme),
        TodoWidgetStyle::Minimal => render_todo_minimal(plan, &projected, width, theme),
        TodoWidgetStyle::Checklist => render_todo_checklist(plan, &projected, width, theme),
        TodoWidgetStyle::Pipeline => render_todo_pipeline(plan, &projected, width, theme),
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
    rows: &[ProjectedTodoTask<'a>],
    width: u16,
    theme: &'a Theme,
    card_prefix: bool,
) {
    let card_cols: usize = if card_prefix { 2 } else { 0 };
    // Prefix: card_cols + 8 ("    ├── ") + 2 ("✓ ") = 10 + card_cols; plus 2 cols right margin
    let max_task_cols = (width as usize).saturating_sub(card_cols + 10 + 2).max(8);
    // Sub-task prefix: card_cols + 8 ("    │   ") + 4 ("├── ") + 2 ("✓ ") = 14 + card_cols; plus 2 cols right margin
    let max_sub_cols = (width as usize).saturating_sub(card_cols + 14 + 2).max(8);

    let task_count = rows.len();
    for (idx, row) in rows.iter().enumerate() {
        let task = row.task;
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

        let sub_count = row.sub_tasks.len();
        for (s_idx, sub) in row.sub_tasks.iter().enumerate() {
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
    rows: &[ProjectedTodoTask<'a>],
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

    for row in rows {
        let task = row.task;
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

        let sub_count = row.sub_tasks.len();
        for (s_idx, sub) in row.sub_tasks.iter().enumerate() {
            let is_sub_last = s_idx == sub_count - 1;
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
/// never resets the counter backwards (e.g., `6/8` -> `7/8 (sub 0/3)` -> `7/8 (sub 1/3)` -> `8/8`),
/// and shows clean macro progress (`2/7`) when sub-todos have not started yet.
pub fn format_plan_progress_counts(plan: &LivePlanBlock) -> String {
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
        if is_task_executing_subtodos(active_macro) && macro_total > 1 {
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
        return format!("{}/{}", macro_done, macro_total);
    }
    format!("{}/{}", plan.completed_tasks, plan.total_tasks)
}

fn format_tree_subheader_counts(plan: &LivePlanBlock, mode: InlineTodoMode) -> String {
    match mode {
        InlineTodoMode::SubTodoFocus {
            step_number,
            macro_total,
            sub_done,
            sub_total,
        } => {
            if macro_total > 1 {
                format!(
                    "Step {}/{} · sub {}/{}",
                    step_number, macro_total, sub_done, sub_total
                )
            } else {
                format!("sub {}/{}", sub_done, sub_total)
            }
        }
        InlineTodoMode::MacroWindow => format_plan_progress_counts(plan),
    }
}

/// Style 1: Oh My Pi tree layout (Default)
fn render_todo_tree<'a>(
    plan: &'a LivePlanBlock,
    projected: &InlineTodoProjection<'a>,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(projected.total_row_count() + 3);

    // Line 1: Header "TODO" in bold cyan/info
    lines.push(Line::from(vec![Span::styled(
        "TODO",
        Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
    )]));

    // Line 2: "└── Plan · 2/7" (Macro Mode) or "└── Plan · Step 2/7 · sub 1/2" (Sub-Todo Focus Mode)
    let raw_subheader = if plan.title.trim().is_empty() {
        "Plan"
    } else {
        plan.title.as_str()
    };
    let count_label = format_tree_subheader_counts(plan, projected.mode);
    let count_cols = UnicodeWidthStr::width(count_label.as_str());
    let max_subheader_cols = (width as usize)
        .saturating_sub(4 + 3 + count_cols + 2)
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

    push_tree_task_lines(&mut lines, &projected.rows, width, theme, false);

    // Divider line below the tree
    let divider_width = (width.saturating_sub(1) as usize).max(10);
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(divider_width),
        Style::default().fg(theme.border),
    )]));

    lines
}

/// Style 2: Rounded container card with interior tree
fn render_todo_card<'a>(
    plan: &'a LivePlanBlock,
    projected: &InlineTodoProjection<'a>,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(projected.total_row_count() + 3);
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

    push_tree_task_lines(&mut lines, &projected.rows, width, theme, true);

    // Bottom border
    let bottom_dashes = (width.saturating_sub(1) as usize).saturating_sub(2);
    lines.push(Line::from(vec![Span::styled(
        format!("╰{}╯", "─".repeat(bottom_dashes)),
        Style::default().fg(theme.brand_accent),
    )]));

    lines
}

/// Style 3: Modern left accent rail
fn render_todo_rail<'a>(
    plan: &'a LivePlanBlock,
    projected: &InlineTodoProjection<'a>,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(projected.total_row_count() + 2);
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

    push_flat_task_lines(&mut lines, &projected.rows, width, theme, true);

    // Divider line
    lines.push(Line::from(vec![Span::styled(
        "─".repeat((width.saturating_sub(1) as usize).max(10)),
        Style::default().fg(theme.border),
    )]));

    lines
}

/// Style 4: Clean minimalist top rule and open layout
fn render_todo_minimal<'a>(
    plan: &'a LivePlanBlock,
    projected: &InlineTodoProjection<'a>,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(projected.total_row_count() + 2);
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

    push_flat_task_lines(&mut lines, &projected.rows, width, theme, false);

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
    projected: &InlineTodoProjection<'a>,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(projected.total_row_count() + 2);
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

    for row in &projected.rows {
        let task = row.task;
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

        for sub in &row.sub_tasks {
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
    projected: &InlineTodoProjection<'a>,
    width: u16,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(projected.total_row_count() + 2);
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

    for row in &projected.rows {
        let task = row.task;
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

        for sub in &row.sub_tasks {
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
        // In sample_plan, T2 has an InProgress sub-task -> Sub-Todo Focus Mode shows ONLY T2 + its 2 sub-tasks (3 items).
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Tree), 7);
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Card), 7);
        assert_eq!(todo_widget_required_height(&plan, TodoWidgetStyle::Rail), 6);
        assert_eq!(
            todo_widget_required_height(&plan, TodoWidgetStyle::Minimal),
            6
        );
        assert_eq!(
            todo_widget_required_height(&plan, TodoWidgetStyle::Checklist),
            6
        );
        assert_eq!(
            todo_widget_required_height(&plan, TodoWidgetStyle::Pipeline),
            6
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

        let checklist_lines = render_todo_widget(&plan, TodoWidgetStyle::Checklist, 80, &theme);
        assert_eq!(checklist_lines.len(), 6);

        let pipeline_lines = render_todo_widget(&plan, TodoWidgetStyle::Pipeline, 80, &theme);
        assert_eq!(pipeline_lines.len(), 6);
    }

    #[test]
    fn test_tier_two_nested_subplan_tree_rendering_focuses_active_todo_and_subtodos_alone() {
        let mut plan = LivePlanBlock {
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

        // Sub-Todo Focus Mode: sibling macro tasks 1 & 3 are hidden; ONLY active Todo 2 + its sub-todos render
        assert_eq!(
            rendered[2],
            "└── Active Milestone: Landing Page · Step 2/3 · sub 1/3"
        );
        assert_eq!(rendered[3], "    └── > 2. Create Hero section");
        assert_eq!(
            rendered[4],
            "        ├── ✓ Define Hero layout grid and typography hierarchy"
        );
        assert_eq!(
            rendered[5],
            "        ├── > Build interactive TerminalPreview sub-component"
        );
        assert_eq!(
            rendered[6],
            "        └── ○ Assemble HeroSection.tsx with CTA buttons and badge"
        );
        assert_eq!(tree_lines.len(), 8);

        // Once Step 2's sub-todos all complete and Step 3 becomes active -> switches back to Macro Todo Mode!
        plan.tasks[1].status = LivePlanTaskStatus::Completed;
        for sub in &mut plan.tasks[1].sub_tasks {
            sub.status = LivePlanTaskStatus::Completed;
        }
        plan.tasks[2].status = LivePlanTaskStatus::InProgress;
        plan.completed_tasks = 2;

        let macro_lines = render_todo_widget(&plan, TodoWidgetStyle::Tree, 100, &theme);
        let macro_rendered: Vec<String> = macro_lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        assert_eq!(
            macro_rendered[2],
            "└── Active Milestone: Landing Page · 2/3"
        );
        assert_eq!(
            macro_rendered[3],
            "    ├── ✓ 1. Design tokens & shared primitives"
        );
        assert_eq!(macro_rendered[4], "    ├── ✓ 2. Create Hero section");
        assert_eq!(
            macro_rendered[5],
            "    └── ○ 3. Build Code Intelligence section".replacen("○", ">", 1)
        );
        // Zero sub-rows expanded in Macro Todo Mode!
        assert_eq!(macro_lines.len(), 7);
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
    fn test_filter_inline_tasks_compact_three_row_window_across_9_step_plan() {
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

        // Active Step 4 (`idx = 3`): shows 1 completed checkmark (`Step 3`), active `Step 4`, and next `Step 5`
        let mid_window = filter_inline_tasks(&tasks);
        let mid_titles: Vec<&str> = mid_window.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(mid_titles, vec!["Step 3", "Step 4", "Step 5"]);

        // Active Step 9 (`idx = 8`): preserves 3 full rows (`Step 7`, `Step 8`, `Step 9`)
        for (idx, t) in tasks.iter_mut().enumerate() {
            t.status = if idx < 8 {
                LivePlanTaskStatus::Completed
            } else {
                LivePlanTaskStatus::InProgress
            };
        }
        let end_window = filter_inline_tasks(&tasks);
        let end_titles: Vec<&str> = end_window.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(end_titles, vec!["Step 7", "Step 8", "Step 9"]);
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
            subheader.contains("Step 8/8 · sub 1/3"),
            "Expected subheader to preserve macro step + sub counts, got: {subheader}"
        );
    }

    #[test]
    fn test_minitest58_all_macro_tasks_with_subtasks_never_bloats_or_clips() {
        // Reproduces minitest58 where all 7 macro tasks have 2-4 persisted sub-todos in todo.md
        let make_sub = |title: &str, status: LivePlanTaskStatus| LivePlanTaskItem {
            title: title.to_string(),
            status,
            sub_tasks: Vec::new(),
        };
        let mut plan = LivePlanBlock {
            title: "Active Milestone: minicode landing page".to_string(),
            total_tasks: 2,
            completed_tasks: 0,
            active_task: Some("Build TerminalPreview with sequenced agent events".to_string()),
            tasks: vec![
                LivePlanTaskItem {
                    title: "Foundation: fonts, theme tokens, shell".to_string(),
                    status: LivePlanTaskStatus::Completed,
                    sub_tasks: vec![
                        make_sub("bun add fonts", LivePlanTaskStatus::Completed),
                        make_sub("Rewrite src/index.css", LivePlanTaskStatus::Completed),
                        make_sub("Rewrite Navbar and Footer", LivePlanTaskStatus::Completed),
                        make_sub("Point App.tsx router", LivePlanTaskStatus::Completed),
                    ],
                },
                LivePlanTaskItem {
                    title: "Hero + live terminal preview".to_string(),
                    status: LivePlanTaskStatus::InProgress,
                    sub_tasks: vec![
                        make_sub(
                            "Build TerminalPreview with sequenced agent events",
                            LivePlanTaskStatus::InProgress,
                        ),
                        make_sub(
                            "Build Hero with CTAs Get Started / View on GitHub",
                            LivePlanTaskStatus::Pending,
                        ),
                    ],
                },
                LivePlanTaskItem {
                    title: "Philosophy + live performance configurator".to_string(),
                    status: LivePlanTaskStatus::Pending,
                    sub_tasks: vec![
                        make_sub("Philosophy section", LivePlanTaskStatus::Pending),
                        make_sub("Performance configurator", LivePlanTaskStatus::Pending),
                    ],
                },
                LivePlanTaskItem {
                    title: "Swarm DAG + Multi-Agent Council".to_string(),
                    status: LivePlanTaskStatus::Pending,
                    sub_tasks: vec![
                        make_sub("Swarm DAG", LivePlanTaskStatus::Pending),
                        make_sub("Council", LivePlanTaskStatus::Pending),
                    ],
                },
                LivePlanTaskItem {
                    title: "Code Intelligence + Security + Browser HUD".to_string(),
                    status: LivePlanTaskStatus::Pending,
                    sub_tasks: vec![
                        make_sub("Code Intelligence", LivePlanTaskStatus::Pending),
                        make_sub("Security", LivePlanTaskStatus::Pending),
                        make_sub("Browser HUD", LivePlanTaskStatus::Pending),
                    ],
                },
                LivePlanTaskItem {
                    title: "Tools, Interfaces, Comparison, Principles, Proof, CTA".to_string(),
                    status: LivePlanTaskStatus::Pending,
                    sub_tasks: vec![
                        make_sub("Tools matrix", LivePlanTaskStatus::Pending),
                        make_sub("Interfaces", LivePlanTaskStatus::Pending),
                    ],
                },
                LivePlanTaskItem {
                    title: "Verification: lint/build + 3-viewport browser QA".to_string(),
                    status: LivePlanTaskStatus::Pending,
                    sub_tasks: vec![
                        make_sub("bun run lint && bun run build", LivePlanTaskStatus::Pending),
                        make_sub(
                            "browser_screenshot 3 viewports",
                            LivePlanTaskStatus::Pending,
                        ),
                    ],
                },
            ],
        };

        let theme = Theme::default();

        // 1. Screenshot 1 state: Step 2 executing sub-todos -> ONLY Hero + its 2 sub-todos (7 lines, NOT 22!)
        let focus_lines = render_todo_widget(&plan, TodoWidgetStyle::Tree, 100, &theme);
        assert_eq!(focus_lines.len(), 7);
        assert_eq!(
            todo_widget_required_height(&Some(plan.clone()), TodoWidgetStyle::Tree),
            7
        );
        let focus_text: Vec<String> = focus_lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(focus_text[2].contains("Step 2/7 · sub 0/2"));
        assert!(focus_text[3].contains("> Hero + live terminal preview"));
        assert!(focus_text[4].contains("> Build TerminalPreview"));
        assert!(focus_text[5].contains("○ Build Hero with CTAs"));
        assert!(
            !focus_text.iter().any(|l| l.contains("Foundation")),
            "Completed Step 1 and its 4 sub-todos must not clutter Sub-Todo Focus Mode"
        );

        // 2. Screenshot 2 state: Step 2 finished, Step 3 (`Philosophy`) promoted with 0 started sub-todos
        plan.tasks[1].status = LivePlanTaskStatus::Completed;
        for s in &mut plan.tasks[1].sub_tasks {
            s.status = LivePlanTaskStatus::Completed;
        }
        plan.tasks[2].status = LivePlanTaskStatus::InProgress;

        let macro_lines = render_todo_widget(&plan, TodoWidgetStyle::Tree, 100, &theme);
        assert_eq!(macro_lines.len(), 7);
        assert_eq!(
            todo_widget_required_height(&Some(plan.clone()), TodoWidgetStyle::Tree),
            7
        );
        let macro_text: Vec<String> = macro_lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(
            macro_text[2],
            "└── Active Milestone: minicode landing page · 2/7"
        );
        assert!(macro_text[3].contains("✓ Hero + live terminal preview"));
        assert!(macro_text[4].contains("> Philosophy + live performance configurator"));
        assert!(macro_text[5].contains("○ Swarm DAG + Multi-Agent Council"));
        assert!(
            !macro_text.iter().any(|l| l.contains("bun add fonts")),
            "Completed/pending sub-todos must stay collapsed in Macro Todo Mode"
        );
    }

    #[test]
    fn test_bounded_height_guard_matches_rendered_lines_across_all_styles() {
        let plan = sample_plan();
        let plan_opt = Some(plan.clone());
        let theme = Theme::default();
        let styles = [
            TodoWidgetStyle::Tree,
            TodoWidgetStyle::Card,
            TodoWidgetStyle::Rail,
            TodoWidgetStyle::Minimal,
            TodoWidgetStyle::Checklist,
            TodoWidgetStyle::Pipeline,
        ];

        for style in styles {
            for max_h in 0..=12u16 {
                let expected_h = todo_widget_required_height_bounded(&plan_opt, style, Some(max_h));
                let rendered = render_todo_widget_bounded(&plan, style, 80, &theme, Some(max_h));
                assert_eq!(
                    expected_h as usize,
                    rendered.len(),
                    "Height mismatch for style {:?} at max_h={max_h}",
                    style
                );
                assert!(
                    expected_h <= max_h,
                    "Exceeded max_h={max_h} for style {:?}: got {expected_h}",
                    style
                );
                if expected_h > 0 {
                    let joined: String = rendered
                        .iter()
                        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    assert!(
                        joined.contains("T2.2: Field validators"),
                        "Active sub-todo must remain visible at max_h={max_h} for style {:?}, got:\n{joined}",
                        style
                    );
                }
            }
        }
    }
}
