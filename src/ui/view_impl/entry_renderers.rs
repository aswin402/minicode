//! Specialized timeline block renderers (`SubagentTree`, `SubagentSwarm`, `ContextCompaction`, `LivePlan`, `SystemStatus`).

use super::{
    LivePlanBlock, LivePlanTaskStatus, SubagentItemStatus, SubagentTreeBlock, SwarmMatrixBlock,
    TimelineContext, TimelineView,
};
use crate::constants::SPINNER_FRAMES;
use crate::ui::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

impl TimelineView {
    pub(super) fn render_subagent_tree_entry<'a>(
        block: &SubagentTreeBlock,
        ctx: &TimelineContext,
        theme: &'a Theme,
        lines: &mut Vec<Line<'a>>,
    ) {
        let role_color = theme.role_accent_color(&block.role_name);
        let (status_bullet, status_color, header_suffix) = if block.is_running {
            ("◉", role_color, "".to_string())
        } else if block.is_success {
            let dur = if let Some(ms) = block.duration_ms {
                format!(
                    " ({:.1}s • {} tokens)",
                    (ms as f64) / 1000.0,
                    block.tokens_used
                )
            } else {
                format!(" ({} tokens)", block.tokens_used)
            };
            ("✓", theme.success, dur)
        } else {
            let dur = if let Some(ms) = block.duration_ms {
                format!(
                    " (Failed in {:.1}s • {} tokens)",
                    (ms as f64) / 1000.0,
                    block.tokens_used
                )
            } else {
                format!(" (Failed • {} tokens)", block.tokens_used)
            };
            ("✗", theme.destructive, dur)
        };

        // Line 1: ◉ [Researcher: researcher-1]
        let clean_role_name =
            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&block.role_name);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", status_bullet),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{}: {}]", clean_role_name, block.id),
                Style::default().fg(role_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(header_suffix, Style::default().fg(theme.muted)),
        ]));

        // Empty line
        lines.push(Line::from(String::new()));

        // Task Badge Line
        let prompt_trimmed = block.task_prompt.trim();
        let prompt_lines: Vec<&str> = prompt_trimmed.lines().collect();

        if let Some(first_line) = prompt_lines.first() {
            let clean_first =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(first_line);
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(
                    " Task ",
                    Style::default()
                        .fg(theme.bg_primary)
                        .bg(role_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default()),
                Span::styled(clean_first, Style::default().fg(theme.text_primary)),
            ]));

            for rest_line in prompt_lines.iter().skip(1) {
                let clean_rest =
                    crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(rest_line);
                lines.push(Line::from(vec![
                    Span::styled("         ", Style::default()),
                    Span::styled(clean_rest, Style::default().fg(theme.text_primary)),
                ]));
            }
        }

        // Tree Branches
        let item_count = block.items.len();
        for (idx, item) in block.items.iter().enumerate() {
            let is_last = idx == item_count - 1;
            let branch = if is_last {
                "  ╰─── "
            } else {
                "  ├─── "
            };

            let frame_idx = ((ctx.working_millis / crate::constants::SPINNER_FRAME_MS) as usize)
                % SPINNER_FRAMES.len();
            let spinner = SPINNER_FRAMES[frame_idx];

            let (item_bullet, item_color) = match &item.status {
                SubagentItemStatus::Running if block.is_running => (spinner, theme.warning),
                SubagentItemStatus::Running if block.is_success => ("✓", theme.success),
                SubagentItemStatus::Running => ("✗", theme.destructive),
                SubagentItemStatus::Success => ("✓", theme.success),
                SubagentItemStatus::Failed(_) => ("✗", theme.destructive),
            };

            let clean_name =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&item.name);
            let clean_detail =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&item.detail);
            lines.push(Line::from(vec![
                Span::styled(branch, Style::default().fg(theme.border)),
                Span::styled(
                    format!("{} ", item_bullet),
                    Style::default().fg(item_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} ", clean_name),
                    Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(clean_detail, Style::default().fg(theme.muted)),
            ]));
        }

        // Outcome / Error
        if let Some(ref outcome) = block.outcome {
            let clean_outcome =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(outcome.trim());
            lines.push(Line::from(String::new()));
            lines.push(Line::from(vec![
                Span::styled(
                    "  Outcome: ",
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(clean_outcome, Style::default().fg(theme.text_primary)),
            ]));
        } else if let Some(ref err) = block.error_message {
            let clean_err =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(err.trim());
            lines.push(Line::from(String::new()));
            lines.push(Line::from(vec![
                Span::styled(
                    "  Error: ",
                    Style::default()
                        .fg(theme.destructive)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(clean_err, Style::default().fg(theme.destructive)),
            ]));
        }

        lines.push(Line::from(String::new()));
    }

    pub(super) fn render_subagent_swarm_entry<'a>(
        swarm: &SwarmMatrixBlock,
        ctx: &TimelineContext,
        theme: &'a Theme,
        area_width: u16,
        lines: &mut Vec<Line<'a>>,
    ) {
        let frame_idx = ((ctx.working_millis / crate::constants::SPINNER_FRAME_MS) as usize)
            % SPINNER_FRAMES.len();
        let spinner = SPINNER_FRAMES[frame_idx];

        let top_bullet = if swarm.is_running { spinner } else { "✓" };
        let bullet_color = if swarm.is_running {
            theme.warning
        } else {
            theme.success
        };

        let elapsed_str = if let Some(ms) = swarm.duration_ms {
            format!(" • {:.1}s", (ms as f64) / 1000.0)
        } else {
            "".to_string()
        };

        let header_title = format!(
            "┌─ {} {} ({} Workers • {} tokens{}) ──",
            top_bullet,
            swarm.title,
            swarm.workers.len(),
            swarm.total_tokens,
            elapsed_str
        );

        lines.push(Line::from(vec![Span::styled(
            header_title,
            Style::default()
                .fg(bullet_color)
                .add_modifier(Modifier::BOLD),
        )]));

        if !swarm.is_expanded {
            // Collapsed Matrix Summary
            for worker in &swarm.workers {
                let role_color = theme.role_accent_color(&worker.role_name);
                let clean_worker_role =
                    crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                        &worker.role_name,
                    );
                let (w_bullet, w_color) = if worker.is_running {
                    ("◉", role_color)
                } else if worker.is_success {
                    ("✓", theme.success)
                } else {
                    ("✗", theme.destructive)
                };

                let last_action = worker
                    .items
                    .last()
                    .map(|it| {
                        crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&format!(
                            "{} {}",
                            it.name, it.detail
                        ))
                    })
                    .unwrap_or_else(|| "Initializing...".to_string());

                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.border)),
                    Span::styled(
                        format!("{} ", w_bullet),
                        Style::default().fg(w_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{}] ", clean_worker_role),
                        Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{}: ", worker.id),
                        Style::default().fg(theme.text_primary),
                    ),
                    Span::styled(
                        format!("{} ", last_action),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(
                        format!("({} tok)", worker.tokens_used),
                        Style::default().fg(theme.border),
                    ),
                ]));
            }

            lines.push(Line::from(vec![Span::styled(
                "│ ",
                Style::default().fg(theme.border),
            )]));
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.border)),
                Span::styled(
                    "[Space] Expand Full Tree  •  [k] Cancel Worker",
                    Style::default().fg(theme.muted),
                ),
            ]));
        } else {
            // Expanded Full Tree View inside Swarm Card
            for worker in &swarm.workers {
                let role_color = theme.role_accent_color(&worker.role_name);
                let clean_worker_role =
                    crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                        &worker.role_name,
                    );
                let (w_bullet, w_color) = if worker.is_running {
                    ("◉", role_color)
                } else if worker.is_success {
                    ("✓", theme.success)
                } else {
                    ("✗", theme.destructive)
                };

                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.border)),
                    Span::styled(
                        format!("▾ {} ", w_bullet),
                        Style::default().fg(w_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{}: {}]", clean_worker_role, worker.id),
                        Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                    ),
                ]));

                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.border)),
                    Span::styled("   Task: ", Style::default().fg(role_color)),
                    Span::styled(
                        crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                            worker.task_prompt.lines().next().unwrap_or(""),
                        ),
                        Style::default().fg(theme.text_primary),
                    ),
                ]));

                let item_count = worker.items.len();
                for (idx, item) in worker.items.iter().enumerate() {
                    let is_last = idx == item_count - 1;
                    let branch = if is_last {
                        "   ╰─── "
                    } else {
                        "   ├─── "
                    };
                    let (i_bullet, i_color) = match &item.status {
                        SubagentItemStatus::Running if worker.is_running => {
                            (spinner, theme.warning)
                        }
                        SubagentItemStatus::Running if worker.is_success => ("✓", theme.success),
                        SubagentItemStatus::Running => ("✗", theme.destructive),
                        SubagentItemStatus::Success => ("✓", theme.success),
                        SubagentItemStatus::Failed(_) => ("✗", theme.destructive),
                    };

                    let clean_item_name =
                        crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&item.name);
                    lines.push(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(theme.border)),
                        Span::styled(branch, Style::default().fg(theme.border)),
                        Span::styled(format!("{} ", i_bullet), Style::default().fg(i_color)),
                        Span::styled(
                            format!("{} ", clean_item_name),
                            Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                &item.detail,
                            ),
                            Style::default().fg(theme.muted),
                        ),
                    ]));
                }

                lines.push(Line::from(vec![Span::styled(
                    "│ ",
                    Style::default().fg(theme.border),
                )]));
            }

            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.border)),
                Span::styled(
                    "[Space] Collapse to Matrix  •  [k] Cancel Worker",
                    Style::default().fg(theme.muted),
                ),
            ]));
        }

        let bottom_border = format!("└{}", "─".repeat((area_width as usize).saturating_sub(2)));
        lines.push(Line::from(vec![Span::styled(
            bottom_border,
            Style::default().fg(theme.border),
        )]));
        lines.push(Line::from(String::new()));
    }

    pub(super) fn render_context_compaction_entry<'a>(
        tier: usize,
        turns_summarized: usize,
        tokens_before: usize,
        tokens_after: usize,
        savings_percent: usize,
        theme: &'a Theme,
        lines: &mut Vec<Line<'a>>,
    ) {
        let tier_label = match tier {
            1 => "Tier 1: Masking",
            2 => "Tier 2: Turn Summary",
            3 => "Tier 3: Memory Anchor",
            _ => "Compacted",
        };
        let before_k = if tokens_before >= 1000 {
            format!("{:.1}k", tokens_before as f64 / 1000.0)
        } else {
            tokens_before.to_string()
        };
        let after_k = if tokens_after >= 1000 {
            format!("{:.1}k", tokens_after as f64 / 1000.0)
        } else {
            tokens_after.to_string()
        };
        lines.push(Line::from(vec![
            Span::styled("• ", Style::default().fg(theme.brand_accent)),
            Span::styled(
                format!(
                    "Context Auto-Compacted [{}]: {} turns summarized, {} → {} tokens ({}% saved)",
                    tier_label, turns_summarized, before_k, after_k, savings_percent
                ),
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        lines.push(Line::from(String::new()));
    }

    pub(super) fn render_live_plan_entry<'a>(
        plan: &LivePlanBlock,
        theme: &'a Theme,
        area_width: u16,
        lines: &mut Vec<Line<'a>>,
    ) {
        let header_text = format!(
            "┌── Plan Progress: {}/{} Tasks Completed ──",
            plan.completed_tasks, plan.total_tasks
        );
        let header_len = UnicodeWidthStr::width(header_text.as_str());
        let right_padding = (area_width as usize)
            .saturating_sub(header_len)
            .saturating_sub(1);
        let full_header = format!("{}{}", header_text, "─".repeat(right_padding));

        lines.push(Line::from(vec![Span::styled(
            full_header,
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )]));

        for task in &plan.tasks {
            let (bullet, bullet_color, text_color, modifier) = match task.status {
                LivePlanTaskStatus::Completed => {
                    ("✓", theme.success, theme.muted, Modifier::CROSSED_OUT)
                }
                LivePlanTaskStatus::InProgress => {
                    (">", theme.info, theme.brand_accent, Modifier::BOLD)
                }
                LivePlanTaskStatus::Pending => ("○", theme.muted, theme.muted, Modifier::empty()),
            };

            let clean_title =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&task.title);
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.border)),
                Span::styled(
                    format!(" {} ", bullet),
                    Style::default()
                        .fg(bullet_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    clean_title,
                    Style::default().fg(text_color).add_modifier(modifier),
                ),
            ]));

            for (s_idx, sub) in task.sub_tasks.iter().enumerate() {
                let is_sub_last = s_idx == task.sub_tasks.len() - 1;
                let sub_branch = if is_sub_last {
                    "   └── "
                } else {
                    "   ├── "
                };
                let (s_bullet, s_bullet_color, s_text_color, s_modifier) = match sub.status {
                    LivePlanTaskStatus::Completed => {
                        ("✓", theme.success, theme.muted, Modifier::CROSSED_OUT)
                    }
                    LivePlanTaskStatus::InProgress => {
                        (">", theme.info, theme.brand_accent, Modifier::BOLD)
                    }
                    LivePlanTaskStatus::Pending => {
                        ("○", theme.muted, theme.muted, Modifier::empty())
                    }
                };
                let clean_sub =
                    crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(&sub.title);
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.border)),
                    Span::styled(sub_branch, Style::default().fg(theme.muted)),
                    Span::styled(
                        format!("{} ", s_bullet),
                        Style::default()
                            .fg(s_bullet_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        clean_sub,
                        Style::default().fg(s_text_color).add_modifier(s_modifier),
                    ),
                ]));
            }
        }

        let bottom_border = format!("└{}", "─".repeat((area_width as usize).saturating_sub(2)));
        lines.push(Line::from(vec![Span::styled(
            bottom_border,
            Style::default().fg(theme.border),
        )]));
        lines.push(Line::from(String::new()));
    }

    pub(super) fn render_system_status_entry<'a>(
        status: &str,
        theme: &'a Theme,
        lines: &mut Vec<Line<'a>>,
    ) {
        for raw_line in status.lines() {
            let normalized =
                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(raw_line);
            let trimmed = normalized.trim();
            if trimmed.is_empty() {
                continue;
            }
            let prefixes = [
                ("✓ ", theme.success),
                ("✗ ", theme.destructive),
                ("! ", theme.warning),
                ("▲ ", theme.warning),
                ("→ ", theme.brand_accent),
                ("▸ ", theme.brand_accent),
                ("• ", theme.brand_accent),
                ("◆ ", theme.brand_accent),
                ("● ", theme.brand_accent),
            ];

            let mut matched = false;
            for (prefix, color) in prefixes {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    lines.push(Line::from(vec![
                        Span::styled(
                            prefix,
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(rest.to_string(), Style::default().fg(theme.text_primary)),
                    ]));
                    matched = true;
                    break;
                }
            }

            if !matched {
                let lower = trimmed.to_lowercase();
                if lower.starts_with("error") || lower.contains("error:") {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "✗ ",
                            Style::default()
                                .fg(theme.destructive)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(trimmed.to_string(), Style::default().fg(theme.text_primary)),
                    ]));
                } else if lower.contains("rate limit") || lower.contains("retrying") {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "! ",
                            Style::default()
                                .fg(theme.warning)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(trimmed.to_string(), Style::default().fg(theme.text_primary)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("• ", Style::default().fg(theme.brand_accent)),
                        Span::styled(trimmed.to_string(), Style::default().fg(theme.text_primary)),
                    ]));
                }
            }
        }
    }
}
