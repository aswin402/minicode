//! In-TUI MiniPower Autonomous Methodology & Verification Modal.

use crate::agent::minipower::MiniPowerEngine;
use crate::agent::verification_barrier::{GateStatus, VerificationReport};
use crate::ui::layout_utils::centered_rect;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::path::{Path, PathBuf};

/// Active tab within the `/power` MiniPower modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiniPowerTab {
    Pillars,
    RedFlags,
    VerificationBarrier,
}

impl MiniPowerTab {
    pub fn all() -> &'static [MiniPowerTab] {
        &[
            MiniPowerTab::Pillars,
            MiniPowerTab::RedFlags,
            MiniPowerTab::VerificationBarrier,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            MiniPowerTab::Pillars => "Pillars & Methodology",
            MiniPowerTab::RedFlags => "Anti-Rationalization",
            MiniPowerTab::VerificationBarrier => "Verification Barrier",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            MiniPowerTab::Pillars => MiniPowerTab::RedFlags,
            MiniPowerTab::RedFlags => MiniPowerTab::VerificationBarrier,
            MiniPowerTab::VerificationBarrier => MiniPowerTab::Pillars,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            MiniPowerTab::Pillars => MiniPowerTab::VerificationBarrier,
            MiniPowerTab::RedFlags => MiniPowerTab::Pillars,
            MiniPowerTab::VerificationBarrier => MiniPowerTab::RedFlags,
        }
    }
}

/// State for the interactive `/power` modal dialog.
#[derive(Debug, Clone)]
pub struct MiniPowerModalState {
    #[allow(dead_code)]
    pub workspace_root: PathBuf,
    pub active_tab: MiniPowerTab,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub verification_report: Option<VerificationReport>,
    pub is_evaluating: bool,
}

impl MiniPowerModalState {
    pub fn new(workspace: &Path) -> Self {
        Self {
            workspace_root: workspace.to_path_buf(),
            active_tab: MiniPowerTab::Pillars,
            selected_index: 0,
            scroll_offset: 0,
            verification_report: None,
            is_evaluating: false,
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = self.active_tab.next();
        self.scroll_offset = 0;
        self.selected_index = 0;
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = self.active_tab.prev();
        self.scroll_offset = 0;
        self.selected_index = 0;
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn set_report(&mut self, report: VerificationReport) {
        self.verification_report = Some(report);
        self.is_evaluating = false;
    }
}

pub fn render_minipower(frame: &mut Frame, state: &MiniPowerModalState, area: Rect, theme: &Theme) {
    let popup_area = centered_rect(82, 80, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(" ⚡ MiniPower — Native Autonomous Engineering ")
        .title_alignment(Alignment::Center);
    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab navigation
            Constraint::Min(5),    // Content area
            Constraint::Length(1), // Footer hint
        ])
        .split(inner_area);

    // 1. Tab Bar
    let mut tab_spans = Vec::new();
    for (i, tab) in MiniPowerTab::all().iter().enumerate() {
        let is_active = *tab == state.active_tab;
        let style = if is_active {
            Style::default()
                .fg(Color::Black)
                .bg(theme.brand_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        tab_spans.push(Span::styled(
            format!(" [{}] {} ", i + 1, tab.title()),
            style,
        ));
        tab_spans.push(Span::raw(" "));
    }

    let tab_bar = Paragraph::new(Line::from(tab_spans))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border)),
        );
    frame.render_widget(tab_bar, chunks[0]);

    // 2. Tab Content
    let content_lines = match state.active_tab {
        MiniPowerTab::Pillars => render_pillars_content(theme),
        MiniPowerTab::RedFlags => render_red_flags_content(theme),
        MiniPowerTab::VerificationBarrier => render_barrier_content(state, theme),
    };

    let visible_lines: Vec<Line> = content_lines
        .into_iter()
        .skip(state.scroll_offset)
        .collect();

    let content_widget =
        Paragraph::new(visible_lines).style(Style::default().fg(theme.text_primary));
    frame.render_widget(content_widget, chunks[1]);

    // 3. Footer Hint
    let footer_text = match state.active_tab {
        MiniPowerTab::VerificationBarrier => {
            " [Tab/←/→] Switch Tab | [r/v] Run Live Verification | [↑/↓] Scroll | [Esc/q] Close "
        }
        _ => " [Tab/←/→] Switch Tab | [1-3] Select Tab | [↑/↓] Scroll | [Esc/q] Close ",
    };
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.muted));
    frame.render_widget(footer, chunks[2]);
}

fn render_pillars_content<'a>(theme: &'a Theme) -> Vec<Line<'a>> {
    vec![
        Line::from(vec![
            Span::styled("6 Core Engineering Pillars", Style::default().fg(theme.brand_accent).add_modifier(Modifier::BOLD)),
            Span::styled(" — Systematic Discipline over Improvisation", Style::default().fg(theme.muted)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  1. 🧠 Socratic Brainstorming   ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::raw("Clarify intent & trade-offs before touching files (`/power brainstorm <topic>`)"),
        ]),
        Line::from(vec![
            Span::styled("  2. 🌳 Git Worktree Isolation   ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::raw("Execute mutating tasks in ephemeral worktrees (`/power task <prompt>`)"),
        ]),
        Line::from(vec![
            Span::styled("  3. 📋 Bite-Sized Planning      ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::raw("2-5 min atomic tasks with concrete acceptance tests (`/power plan <topic>`)"),
        ]),
        Line::from(vec![
            Span::styled("  4. 🛡️ Two-Stage Subagent Review", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::raw("Automated Stage 1 (Spec Compliance) & Stage 2 (Code Quality) (`/power review`)"),
        ]),
        Line::from(vec![
            Span::styled("  5. 🧪 Strict Red/Green TDD     ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::raw("Write/update failing tests first, verify red, implement minimal code, verify green"),
        ]),
        Line::from(vec![
            Span::styled("  6. ⚡ Evidence Before Assertions", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::raw("Zero unverified claims; all completions must exit code 0 (`/power verify`)"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("💡 Quick Shortcuts: ", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
            Span::raw("Ctrl+P (this modal) | /power | /power brainstorm | /power plan | /power task | /power verify"),
        ]),
    ]
}

fn render_red_flags_content<'a>(theme: &'a Theme) -> Vec<Line<'a>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "Anti-Rationalization Guardrails",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " — Excuses Mapped to Engineering Reality",
                Style::default().fg(theme.muted),
            ),
        ]),
        Line::from(""),
    ];

    for (excuse, reality) in MiniPowerEngine::anti_rationalization_table() {
        lines.push(Line::from(vec![
            Span::styled("  • \"", Style::default().fg(theme.warning)),
            Span::styled(
                *excuse,
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled("\"", Style::default().fg(theme.warning)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    ╰─── ", Style::default().fg(theme.muted)),
            Span::styled(*reality, Style::default().fg(theme.text_primary)),
        ]));
        lines.push(Line::from(""));
    }

    lines
}

fn render_barrier_content<'a>(state: &'a MiniPowerModalState, theme: &'a Theme) -> Vec<Line<'a>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "4-Gate Pre-Completion Verification Barrier",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " — Programmatic Exit Code 0 Gatekeeper",
                Style::default().fg(theme.muted),
            ),
        ]),
        Line::from(""),
    ];

    if state.is_evaluating {
        lines.push(Line::from(vec![Span::styled(
            "  ⏳ Evaluating verification gates across workspace...",
            Style::default().fg(theme.warning),
        )]));
        return lines;
    }

    if let Some(ref report) = state.verification_report {
        let badge = if report.all_passed {
            Span::styled(
                "  ✔ ALL 4 GATES PASSED CLEANLY",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                "  ❌ VERIFICATION ISSUES DETECTED",
                Style::default()
                    .fg(theme.destructive)
                    .add_modifier(Modifier::BOLD),
            )
        };
        lines.push(Line::from(vec![badge]));
        lines.push(Line::from(""));

        let render_gate = |name: &str, status: &GateStatus| -> Vec<Line<'a>> {
            let mut glines = Vec::new();
            match status {
                GateStatus::Passed => {
                    glines.push(Line::from(vec![
                        Span::styled("  ✔ ", Style::default().fg(theme.success)),
                        Span::styled(
                            name.to_string(),
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(": Passed"),
                    ]));
                }
                GateStatus::Skipped { reason } => {
                    glines.push(Line::from(vec![
                        Span::styled("  ○ ", Style::default().fg(theme.muted)),
                        Span::styled(name.to_string(), Style::default().fg(theme.text_primary)),
                        Span::raw(format!(": Skipped ({})", reason)),
                    ]));
                }
                GateStatus::Failed {
                    reason,
                    actionable_remediation,
                    ..
                } => {
                    glines.push(Line::from(vec![
                        Span::styled("  ❌ ", Style::default().fg(theme.destructive)),
                        Span::styled(
                            name.to_string(),
                            Style::default()
                                .fg(theme.destructive)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!(": {}", reason)),
                    ]));
                    glines.push(Line::from(vec![
                        Span::styled("     ╰── Remediation: ", Style::default().fg(theme.warning)),
                        Span::raw(actionable_remediation.clone()),
                    ]));
                }
            }
            glines
        };

        lines.extend(render_gate(
            "Gate 1: AST Syntax & Compiler Integrity",
            &report.gate1_syntax_compiler,
        ));
        lines.extend(render_gate(
            "Gate 2: Reproducer & Test Integrity",
            &report.gate2_reproducer_test,
        ));
        lines.extend(render_gate(
            "Gate 3: Structural Integrity & Conflicts",
            &report.gate3_regression_conflicts,
        ));
        lines.extend(render_gate(
            "Gate 4: Diff Sanity & Secret Leaks",
            &report.gate4_diff_sanity,
        ));
    } else {
        lines.push(Line::from(vec![
            Span::styled("  Press ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[r]",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" or ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[v]",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to evaluate all 4 gates against modified workspace files.",
                Style::default().fg(theme.text_primary),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  • Gate 1: ", Style::default().fg(theme.muted)),
            Span::raw("AST syntax verification and scoped compiler integrity"),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  • Gate 2: ", Style::default().fg(theme.muted)),
            Span::raw("Reproducer & regression test suite execution (exit code 0)"),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  • Gate 3: ", Style::default().fg(theme.muted)),
            Span::raw("Structural file tree audit & git merge conflict markers"),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  • Gate 4: ", Style::default().fg(theme.muted)),
            Span::raw("Diff sanity: zero hardcoded secrets, zero forbidden stdout debug logs"),
        ]));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minipower_tab_transitions() {
        let tab = MiniPowerTab::Pillars;
        assert_eq!(tab.next(), MiniPowerTab::RedFlags);
        assert_eq!(tab.next().next(), MiniPowerTab::VerificationBarrier);
        assert_eq!(tab.next().next().next(), MiniPowerTab::Pillars);

        assert_eq!(tab.prev(), MiniPowerTab::VerificationBarrier);
    }

    #[test]
    fn test_modal_state_creation() {
        let mut state = MiniPowerModalState::new(Path::new("/tmp/test"));
        assert_eq!(state.active_tab, MiniPowerTab::Pillars);
        assert_eq!(state.scroll_offset, 0);

        state.next_tab();
        assert_eq!(state.active_tab, MiniPowerTab::RedFlags);

        state.scroll_down();
        assert_eq!(state.scroll_offset, 1);
        state.scroll_up();
        assert_eq!(state.scroll_offset, 0);
    }
}
