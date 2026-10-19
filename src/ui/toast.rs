//! Minimalist, high-signal floating TUI Toast & Notification overlay engine.
//!
//! Renders non-blocking, auto-expiring notification cards anchored at the top-right
//! of the terminal viewport. All geometry, word wrapping, TTL progress hairlines, and
//! color tokens are computed dynamically from the terminal `Rect`, active `Theme`,
//! and structured `AgentEvent` / runtime state.

use crate::agent::types::AgentEvent;
use crate::ui::theme::Theme;
use crate::ui::view::TimelineView;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Semantic category of a floating TUI notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
    Subagent,
    Task,
}

impl ToastKind {
    /// Resolves the semantic accent color dynamically from the active `Theme`.
    #[must_use]
    pub fn accent_color(self, theme: &Theme) -> Color {
        match self {
            Self::Success => theme.success,
            Self::Info => theme.info,
            Self::Warning => theme.warning,
            Self::Error => theme.destructive,
            Self::Subagent => theme.highlight,
            Self::Task => theme.brand_accent,
        }
    }

    /// Minimalist geometric status indicator glyph for the notification header.
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Success => "✔",
            Self::Info => "◈",
            Self::Warning => "⚠",
            Self::Error => "✕",
            Self::Subagent => "◉",
            Self::Task => "⚙",
        }
    }

    /// Default time-to-live derived from notification severity.
    #[must_use]
    pub const fn default_ttl(self) -> Duration {
        match self {
            Self::Error | Self::Warning => Duration::from_millis(5500),
            Self::Subagent | Self::Task => Duration::from_millis(4500),
            Self::Success | Self::Info => Duration::from_millis(3800),
        }
    }
}

/// A single live toast notification entry.
#[derive(Debug, Clone)]
pub struct ToastItem {
    #[allow(dead_code)]
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub message: String,
    pub badge: Option<String>,
    pub dedup_key: Option<String>,
    pub created_at: Instant,
    pub ttl: Duration,
}

impl ToastItem {
    /// Returns the remaining lifetime ratio in `[0.0, 1.0]`.
    #[allow(dead_code)]
    #[must_use]
    pub fn remaining_ratio(&self, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(self.created_at).as_secs_f64();
        let total = self.ttl.as_secs_f64().max(0.001);
        (1.0 - (elapsed / total)).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn is_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.created_at) >= self.ttl
    }
}

/// Manages active floating toast notifications with in-place deduplication and TTL pruning.
#[derive(Debug)]
pub struct ToastManager {
    items: VecDeque<ToastItem>,
    next_id: u64,
    max_visible: usize,
    long_tool_threshold_ms: u64,
    long_turn_threshold_secs: u64,
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            next_id: 1,
            max_visible: 3,
            long_tool_threshold_ms: 3000,
            long_turn_threshold_secs: 5,
        }
    }

    /// Removes expired toasts based on the current monotonic clock.
    pub fn prune_expired(&mut self, now: Instant) {
        self.items.retain(|item| !item.is_expired(now));
    }

    /// Returns true if there is at least one non-expired toast to render.
    #[allow(dead_code)]
    #[must_use]
    pub fn has_active(&mut self) -> bool {
        self.prune_expired(Instant::now());
        !self.items.is_empty()
    }

    /// Pushes a new toast or updates an existing toast in-place when `dedup_key` matches.
    pub fn push(
        &mut self,
        kind: ToastKind,
        title: impl Into<String>,
        message: impl Into<String>,
        badge: Option<String>,
        dedup_key: Option<String>,
    ) {
        let now = Instant::now();
        self.prune_expired(now);

        let title_str = Self::first_line_clean(&title.into());
        let msg_str = Self::first_line_clean(&message.into());
        if title_str.is_empty() && msg_str.is_empty() {
            return;
        }

        let ttl = kind.default_ttl();

        if let Some(ref key) = dedup_key {
            if let Some(existing) = self
                .items
                .iter_mut()
                .find(|item| item.dedup_key.as_deref() == Some(key.as_str()))
            {
                existing.kind = kind;
                existing.title = title_str;
                existing.message = msg_str;
                existing.badge = badge;
                existing.created_at = now;
                existing.ttl = ttl;
                return;
            }
        }

        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.items.push_back(ToastItem {
            id,
            kind,
            title: title_str,
            message: msg_str,
            badge,
            dedup_key,
            created_at: now,
            ttl,
        });

        while self.items.len() > self.max_visible {
            self.items.pop_front();
        }
    }

    /// Convenience helper for simple status notifications.
    #[allow(dead_code)]
    pub fn notify(
        &mut self,
        kind: ToastKind,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.push(kind, title, message, None, None);
    }

    /// Derives high-signal toast notifications directly from structured `AgentEvent` variants.
    pub fn ingest_agent_event(&mut self, event: &AgentEvent, turn_elapsed: Option<Duration>) {
        match event {
            AgentEvent::SubagentProgress {
                subagent_id,
                role,
                action,
                status,
                ..
            } => {
                if status == "started" || status == "spawned" {
                    let detail = Self::first_line_clean(action);
                    let msg = if detail.is_empty() {
                        format!("Worker \"{}\" launched.", role)
                    } else {
                        format!("\"{}\" — {}", role, detail)
                    };
                    self.push(
                        ToastKind::Subagent,
                        "Subagent Spawned",
                        msg,
                        Some(subagent_id.clone()),
                        Some(format!("subagent:{subagent_id}")),
                    );
                }
            }
            AgentEvent::SubagentCompleted {
                subagent_id,
                role,
                success,
                summary,
                ..
            } => {
                let kind = if *success {
                    ToastKind::Subagent
                } else {
                    ToastKind::Error
                };
                let title = if *success {
                    "Subagent Completed"
                } else {
                    "Subagent Failed"
                };
                let detail = Self::first_line_clean(summary);
                let msg = if detail.is_empty() {
                    format!("Task \"{}\" finished.", role)
                } else {
                    format!("\"{}\" — {}", role, detail)
                };
                self.push(
                    kind,
                    title,
                    msg,
                    Some(subagent_id.clone()),
                    Some(format!("subagent:{subagent_id}")),
                );
            }
            AgentEvent::PlanUpdated {
                phase_label,
                total_tasks,
                completed_tasks,
                active_task,
                ..
            } if *total_tasks > 0 => {
                let pct = (*completed_tasks * 100) / (*total_tasks);
                let badge = Some(format!("{completed_tasks}/{total_tasks} · {pct}%"));
                if *completed_tasks >= *total_tasks {
                    let title = phase_label
                        .as_deref()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or("Plan Completed");
                    self.push(
                        ToastKind::Success,
                        title,
                        format!("All {total_tasks} plan tasks verified complete."),
                        badge,
                        Some("plan_progress".to_string()),
                    );
                } else if *completed_tasks > 0 {
                    let title = phase_label
                        .as_deref()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or("Plan Step Completed");
                    let msg = active_task
                        .as_deref()
                        .map(|next| format!("Next: \"{next}\""))
                        .unwrap_or_else(|| {
                            format!("Completed {completed_tasks} of {total_tasks} steps.")
                        });
                    self.push(
                        ToastKind::Task,
                        title,
                        msg,
                        badge,
                        Some("plan_progress".to_string()),
                    );
                } else {
                    let title = phase_label
                        .as_deref()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or("Plan Initialized");
                    let msg = active_task
                        .as_deref()
                        .map(|first| format!("Starting: \"{first}\""))
                        .unwrap_or_else(|| format!("{total_tasks} steps scheduled."));
                    self.push(
                        ToastKind::Info,
                        title,
                        msg,
                        Some(format!("0/{total_tasks}")),
                        Some("plan_progress".to_string()),
                    );
                }
            }
            AgentEvent::ApprovalRequest { tool, reason, .. } => {
                let detail = Self::first_line_clean(reason);
                self.push(
                    ToastKind::Warning,
                    "Approval Required",
                    if detail.is_empty() {
                        format!("Tool `{tool}` is waiting for confirmation.")
                    } else {
                        detail
                    },
                    Some(tool.clone()),
                    Some("approval_request".to_string()),
                );
            }
            AgentEvent::UserInquiry { request, .. } => {
                let q_count = request.questions.len();
                self.push(
                    ToastKind::Info,
                    "Clarification Requested",
                    if request.title.trim().is_empty() {
                        format!("{q_count} question(s) waiting for your input.")
                    } else {
                        request.title.clone()
                    },
                    Some(format!("{q_count}Q")),
                    Some("user_inquiry".to_string()),
                );
            }
            AgentEvent::ContextCompacted {
                tier,
                turns_summarized,
                tokens_before,
                tokens_after,
                savings_percent,
                ..
            } => {
                self.push(
                    ToastKind::Info,
                    "Context Compacted",
                    format!(
                        "Summarized {} turns ({}k → {}k tokens).",
                        turns_summarized,
                        tokens_before / 1000,
                        tokens_after / 1000
                    ),
                    Some(format!("-{savings_percent}% · T{tier}")),
                    Some("context_compaction".to_string()),
                );
            }
            AgentEvent::GitCommit { hash, message, .. } => {
                self.push(
                    ToastKind::Success,
                    "Git Commit Created",
                    message.clone(),
                    Some(hash.clone()),
                    Some("git_commit".to_string()),
                );
            }
            AgentEvent::AntiThrashTripped { intervention, .. } => {
                let line = intervention
                    .lines()
                    .find(|l| {
                        let t = l.trim();
                        !t.is_empty() && !t.starts_with('[')
                    })
                    .unwrap_or("Adaptive loop intervention applied.");
                self.push(
                    ToastKind::Warning,
                    "Loop Guardrail Active",
                    line.trim_start_matches('•').trim(),
                    Some("Guard".to_string()),
                    Some("anti_thrash".to_string()),
                );
            }
            AgentEvent::Error {
                message, retrying, ..
            } => {
                let kind = if *retrying {
                    ToastKind::Warning
                } else {
                    ToastKind::Error
                };
                let title = if *retrying {
                    "Retrying Request"
                } else {
                    "Execution Error"
                };
                self.push(
                    kind,
                    title,
                    message.clone(),
                    retrying.then(|| "Retry".to_string()),
                    Some("agent_error".to_string()),
                );
            }
            AgentEvent::ToolResult {
                tool,
                success,
                output,
                duration_ms,
                ..
            } => {
                let dur_badge = Some(Self::format_duration(Duration::from_millis(*duration_ms)));
                if !*success {
                    let reason = Self::first_line_clean(output);
                    let title = format!("{} Rejected", TimelineView::format_tool_title(tool));
                    self.push(
                        ToastKind::Warning,
                        title,
                        if reason.is_empty() {
                            format!("Tool `{tool}` returned an error.")
                        } else {
                            reason
                        },
                        dur_badge,
                        Some(format!("tool_err:{tool}")),
                    );
                } else if *duration_ms >= self.long_tool_threshold_ms
                    || matches!(
                        tool.as_str(),
                        "minitask"
                            | "kit_stack_add"
                            | "power_verify"
                            | "archive_plan"
                            | "browser_action"
                            | "browser_screenshot"
                            | "dev_start"
                            | "process_start"
                    )
                {
                    let human_tool = TimelineView::format_tool_title(tool);
                    let summary = Self::first_line_clean(output);
                    let msg = if summary.is_empty() {
                        format!(
                            "Task \"{human_tool}\" finished in {}.",
                            Self::format_duration(Duration::from_millis(*duration_ms))
                        )
                    } else {
                        summary
                    };
                    self.push(
                        ToastKind::Task,
                        format!("{human_tool} Completed"),
                        msg,
                        dur_badge,
                        Some(format!("tool_done:{tool}")),
                    );
                }
            }
            AgentEvent::TurnEnd {
                status,
                total_tokens_used,
                ..
            } => {
                if let Some(elapsed) = turn_elapsed {
                    if elapsed.as_secs() >= self.long_turn_threshold_secs {
                        let is_ok = status == crate::constants::TURN_STATUS_COMPLETE;
                        let dur_str = Self::format_duration(elapsed);
                        self.push(
                            if is_ok {
                                ToastKind::Success
                            } else {
                                ToastKind::Warning
                            },
                            if is_ok {
                                "Background Turn Completed"
                            } else {
                                "Turn Paused"
                            },
                            format!(
                                "Finished in {} ({:.1}k tokens).",
                                dur_str,
                                *total_tokens_used as f64 / 1000.0
                            ),
                            Some(dur_str),
                            Some("turn_end".to_string()),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Formats a `Duration` into a compact human-readable string (`"420ms"`, `"27s"`, `"3m 41s"`).
    #[must_use]
    pub fn format_duration(dur: Duration) -> String {
        let total_ms = dur.as_millis();
        if total_ms < 1000 {
            format!("{total_ms}ms")
        } else {
            let total_secs = dur.as_secs();
            if total_secs < 60 {
                format!("{total_secs}s")
            } else {
                let mins = total_secs / 60;
                let secs = total_secs % 60;
                if secs == 0 {
                    format!("{mins}m")
                } else {
                    format!("{mins}m {secs}s")
                }
            }
        }
    }

    fn first_line_clean(input: &str) -> String {
        input
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string()
    }

    /// Word-wraps `text` into lines of at most `max_width` terminal cells.
    fn wrap_text(text: &str, max_width: usize, max_lines: usize) -> Vec<String> {
        if max_width == 0 || max_lines == 0 {
            return Vec::new();
        }
        let mut lines = Vec::new();
        let mut current = String::new();
        let mut current_w = 0;

        for word in text.split_whitespace() {
            let word_w = UnicodeWidthStr::width(word);
            let sep_w = usize::from(!current.is_empty());
            if current_w + sep_w + word_w <= max_width {
                if !current.is_empty() {
                    current.push(' ');
                    current_w += 1;
                }
                current.push_str(word);
                current_w += word_w;
            } else {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    if lines.len() >= max_lines {
                        break;
                    }
                }
                if word_w <= max_width {
                    current.push_str(word);
                    current_w = word_w;
                } else {
                    let mut truncated = String::new();
                    let mut w = 0;
                    for ch in word.chars() {
                        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                        if w + cw > max_width.saturating_sub(1) {
                            truncated.push('…');
                            w += 1;
                            break;
                        }
                        truncated.push(ch);
                        w += cw;
                    }
                    current = truncated;
                    current_w = w;
                }
            }
        }

        if !current.is_empty() && lines.len() < max_lines {
            lines.push(current);
        }

        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    /// Truncates a string to fit within `max_width` cells.
    fn fit_width(text: &str, max_width: usize) -> (String, usize) {
        let mut out = String::new();
        let mut w = 0;
        let total_w = UnicodeWidthStr::width(text);
        if total_w <= max_width {
            return (text.to_string(), total_w);
        }
        let limit = max_width.saturating_sub(1);
        for ch in text.chars() {
            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
            if w + cw > limit {
                break;
            }
            out.push(ch);
            w += cw;
        }
        out.push('…');
        w += 1;
        (out, w)
    }

    /// Renders all active toasts anchored at the top-right of `area` using the default `OpenCode` style.
    #[allow(dead_code)]
    pub fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.render_with_style(frame, area, theme, crate::config::ToastStyle::OpenCode);
    }

    /// Renders all active toasts anchored at the top-right of `area` using the configured `ToastStyle`.
    pub fn render_with_style(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        style: crate::config::ToastStyle,
    ) {
        let now = Instant::now();
        self.prune_expired(now);
        if self.items.is_empty() || area.width < 24 || area.height < 4 {
            return;
        }

        // Dynamic viewport-proportional width constraints (zero fixed magic dimensions)
        let right_margin: u16 = (area.width / 60).clamp(1, 3);
        let top_margin: u16 = 1;
        let min_card_w = (area.width / 3)
            .max(36)
            .min(area.width.saturating_sub(right_margin + 1)) as usize;
        let max_card_w = ((area.width as usize * 48) / 100)
            .max(min_card_w)
            .min(area.width.saturating_sub(right_margin + 1) as usize);

        let mut cursor_y = area.y.saturating_add(top_margin);

        // Render newest at the top of the stack
        for item in self.items.iter().rev() {
            let accent = item.kind.accent_color(theme);
            let glyph = item.kind.glyph();
            let badge_text = item.badge.as_deref().unwrap_or("");
            let badge_w = if badge_text.is_empty() {
                0
            } else {
                UnicodeWidthStr::width(badge_text) + 2
            };

            let (left_border, right_border) = match style {
                crate::config::ToastStyle::OpenCode => ("┃  ", "  ┃"),
                crate::config::ToastStyle::Rounded
                | crate::config::ToastStyle::RoundedPadded
                | crate::config::ToastStyle::Bracket => ("│  ", "  │"),
                crate::config::ToastStyle::LeftBar => ("┃  ", "   "),
                crate::config::ToastStyle::CradleRound
                | crate::config::ToastStyle::CradleSquare
                | crate::config::ToastStyle::LRound
                | crate::config::ToastStyle::LSquare => ("│  ", "   "),
            };
            let side_pad =
                UnicodeWidthStr::width(left_border) + UnicodeWidthStr::width(right_border);

            let header_natural_w =
                side_pad + UnicodeWidthStr::width(item.title.as_str()) + badge_w + 4;
            let msg_natural_w = side_pad + UnicodeWidthStr::width(item.message.as_str()) + 4;
            let card_w = header_natural_w
                .max(msg_natural_w)
                .clamp(min_card_w, max_card_w);

            let content_w = card_w.saturating_sub(side_pad);
            let msg_budget = content_w.saturating_sub(2);
            let wrapped_msg = Self::wrap_text(&item.message, msg_budget, 2);

            let top_lines_count: usize = match style {
                crate::config::ToastStyle::RoundedPadded => 2,
                crate::config::ToastStyle::LRound | crate::config::ToastStyle::LSquare => 0,
                _ => 1,
            };
            let bottom_lines_count: usize = match style {
                crate::config::ToastStyle::RoundedPadded => 2,
                _ => 1,
            };
            let card_h = (top_lines_count + 1 + wrapped_msg.len() + bottom_lines_count) as u16;

            if cursor_y.saturating_add(card_h) > area.y.saturating_add(area.height) {
                break;
            }

            let card_x = area
                .x
                .saturating_add(area.width.saturating_sub(card_w as u16 + right_margin));
            let card_rect = Rect {
                x: card_x,
                y: cursor_y,
                width: card_w as u16,
                height: card_h,
            };

            let rail_style = Style::default().fg(accent).bg(theme.bg_elevated);
            let bg_style = Style::default().bg(theme.bg_elevated);
            let title_style = Style::default()
                .fg(theme.text_primary)
                .bg(theme.bg_elevated)
                .add_modifier(Modifier::BOLD);
            let glyph_style = Style::default()
                .fg(accent)
                .bg(theme.bg_elevated)
                .add_modifier(Modifier::BOLD);
            let badge_style = Style::default().fg(accent).bg(theme.bg_elevated);
            let msg_style = Style::default().fg(theme.muted).bg(theme.bg_elevated);

            let mut lines = Vec::with_capacity(card_h as usize);
            let inner_border_w = card_w.saturating_sub(2);
            let cradle_top_w = ((card_w * 42) / 100).max(14).min(card_w.saturating_sub(1));

            // 1. Top Frame / Breathing Row(s)
            match style {
                crate::config::ToastStyle::OpenCode => {
                    lines.push(Line::from(vec![
                        Span::styled("┃", rail_style),
                        Span::styled(" ".repeat(inner_border_w), bg_style),
                        Span::styled("┃", rail_style),
                    ]));
                }
                crate::config::ToastStyle::Rounded => {
                    lines.push(Line::from(vec![
                        Span::styled("╭", rail_style),
                        Span::styled("─".repeat(inner_border_w), rail_style),
                        Span::styled("╮", rail_style),
                    ]));
                }
                crate::config::ToastStyle::RoundedPadded => {
                    lines.push(Line::from(vec![
                        Span::styled("╭", rail_style),
                        Span::styled("─".repeat(inner_border_w), rail_style),
                        Span::styled("╮", rail_style),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("│", rail_style),
                        Span::styled(" ".repeat(inner_border_w), bg_style),
                        Span::styled("│", rail_style),
                    ]));
                }
                crate::config::ToastStyle::Bracket => {
                    let mid_spaces = card_w.saturating_sub(4);
                    lines.push(Line::from(vec![
                        Span::styled("┌─", rail_style),
                        Span::styled(" ".repeat(mid_spaces), bg_style),
                        Span::styled("─┐", rail_style),
                    ]));
                }
                crate::config::ToastStyle::LeftBar => {
                    lines.push(Line::from(vec![
                        Span::styled("┃", rail_style),
                        Span::styled(" ".repeat(card_w.saturating_sub(1)), bg_style),
                    ]));
                }
                crate::config::ToastStyle::CradleRound => {
                    let open_right = card_w.saturating_sub(cradle_top_w + 1);
                    lines.push(Line::from(vec![
                        Span::styled("╭", rail_style),
                        Span::styled("─".repeat(cradle_top_w), rail_style),
                        Span::styled(" ".repeat(open_right), bg_style),
                    ]));
                }
                crate::config::ToastStyle::CradleSquare => {
                    let open_right = card_w.saturating_sub(cradle_top_w + 1);
                    lines.push(Line::from(vec![
                        Span::styled("┌", rail_style),
                        Span::styled("─".repeat(cradle_top_w), rail_style),
                        Span::styled(" ".repeat(open_right), bg_style),
                    ]));
                }
                crate::config::ToastStyle::LRound | crate::config::ToastStyle::LSquare => {
                    // Open top — starts directly with the left vertical spine on the header row
                }
            }

            // 2. Header Row (`✔ Title ... badge`)
            let glyph_prefix = format!("{glyph} ");
            let glyph_prefix_w = UnicodeWidthStr::width(glyph_prefix.as_str());
            let title_max_w = content_w.saturating_sub(glyph_prefix_w + badge_w);
            let (fitted_title, fitted_title_w) = Self::fit_width(&item.title, title_max_w);
            let used_header_w = glyph_prefix_w + fitted_title_w + badge_w;
            let spacer_w = content_w.saturating_sub(used_header_w);

            let mut header_spans = vec![
                Span::styled(left_border, rail_style),
                Span::styled(glyph_prefix, glyph_style),
                Span::styled(fitted_title, title_style),
                Span::styled(" ".repeat(spacer_w), bg_style),
            ];
            if !badge_text.is_empty() {
                header_spans.push(Span::styled(format!("  {badge_text}"), badge_style));
            }
            header_spans.push(Span::styled(right_border, rail_style));
            lines.push(Line::from(header_spans));

            // 3. Body Message Row(s)
            for msg_line in &wrapped_msg {
                let (fitted_msg, fitted_w) = Self::fit_width(msg_line, msg_budget);
                let pad_w = msg_budget.saturating_sub(fitted_w);
                lines.push(Line::from(vec![
                    Span::styled(left_border, rail_style),
                    Span::styled("  ", bg_style),
                    Span::styled(fitted_msg, msg_style),
                    Span::styled(" ".repeat(pad_w), bg_style),
                    Span::styled(right_border, rail_style),
                ]));
            }

            // 4. Bottom Frame / Breathing Row(s)
            match style {
                crate::config::ToastStyle::OpenCode => {
                    lines.push(Line::from(vec![
                        Span::styled("┃", rail_style),
                        Span::styled(" ".repeat(inner_border_w), bg_style),
                        Span::styled("┃", rail_style),
                    ]));
                }
                crate::config::ToastStyle::Rounded => {
                    lines.push(Line::from(vec![
                        Span::styled("╰", rail_style),
                        Span::styled("─".repeat(inner_border_w), rail_style),
                        Span::styled("╯", rail_style),
                    ]));
                }
                crate::config::ToastStyle::RoundedPadded => {
                    lines.push(Line::from(vec![
                        Span::styled("│", rail_style),
                        Span::styled(" ".repeat(inner_border_w), bg_style),
                        Span::styled("│", rail_style),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("╰", rail_style),
                        Span::styled("─".repeat(inner_border_w), rail_style),
                        Span::styled("╯", rail_style),
                    ]));
                }
                crate::config::ToastStyle::Bracket => {
                    let mid_spaces = card_w.saturating_sub(4);
                    lines.push(Line::from(vec![
                        Span::styled("└─", rail_style),
                        Span::styled(" ".repeat(mid_spaces), bg_style),
                        Span::styled("─┘", rail_style),
                    ]));
                }
                crate::config::ToastStyle::LeftBar => {
                    lines.push(Line::from(vec![
                        Span::styled("┃", rail_style),
                        Span::styled(" ".repeat(card_w.saturating_sub(1)), bg_style),
                    ]));
                }
                crate::config::ToastStyle::CradleRound | crate::config::ToastStyle::LRound => {
                    lines.push(Line::from(vec![
                        Span::styled("╰", rail_style),
                        Span::styled("─".repeat(card_w.saturating_sub(1)), rail_style),
                    ]));
                }
                crate::config::ToastStyle::CradleSquare | crate::config::ToastStyle::LSquare => {
                    lines.push(Line::from(vec![
                        Span::styled("└", rail_style),
                        Span::styled("─".repeat(card_w.saturating_sub(1)), rail_style),
                    ]));
                }
            }

            frame.render_widget(Clear, card_rect);
            let paragraph = Paragraph::new(lines).block(Block::default().style(bg_style));
            frame.render_widget(paragraph, card_rect);

            cursor_y = cursor_y.saturating_add(card_h + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ToastStyle;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_toast_deduplication_and_rendering_all_styles() {
        let mut mgr = ToastManager::new();
        mgr.push(
            ToastKind::Task,
            "Plan Step Completed",
            "Next: \"HeroSection.tsx\"",
            Some("1/16 · 6%".to_string()),
            Some("plan_progress".to_string()),
        );
        mgr.push(
            ToastKind::Task,
            "Plan Step Completed",
            "Next: \"Navbar.tsx\"",
            Some("2/16 · 12%".to_string()),
            Some("plan_progress".to_string()),
        );

        // In-place deduplication keeps exactly 1 item for the same dedup_key
        assert_eq!(mgr.items.len(), 1);
        assert_eq!(mgr.items[0].badge.as_deref(), Some("2/16 · 12%"));

        let theme = Theme::aura_dark();
        for style in [
            ToastStyle::OpenCode,
            ToastStyle::Rounded,
            ToastStyle::RoundedPadded,
            ToastStyle::Bracket,
            ToastStyle::LeftBar,
            ToastStyle::CradleRound,
            ToastStyle::CradleSquare,
            ToastStyle::LRound,
            ToastStyle::LSquare,
        ] {
            let backend = TestBackend::new(100, 24);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    mgr.render_with_style(frame, frame.area(), &theme, style);
                })
                .unwrap();

            let buf = terminal.backend().buffer();
            let mut rendered_text = String::new();
            for y in 0..24 {
                for x in 0..100 {
                    rendered_text.push_str(buf[(x, y)].symbol());
                }
                rendered_text.push('\n');
            }
            assert!(rendered_text.contains("Plan Step Completed"));
            assert!(rendered_text.contains("2/16 · 12%"));
            assert!(rendered_text.contains("Next: \"Navbar.tsx\""));
        }
    }
}
