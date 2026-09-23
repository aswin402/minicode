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
    PlanHierarchy,
}

impl MiniPowerTab {
    pub fn all() -> &'static [MiniPowerTab] {
        &[
            MiniPowerTab::Pillars,
            MiniPowerTab::RedFlags,
            MiniPowerTab::VerificationBarrier,
            MiniPowerTab::PlanHierarchy,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            MiniPowerTab::Pillars => "Pillars & Methodology",
            MiniPowerTab::RedFlags => "Anti-Rationalization",
            MiniPowerTab::VerificationBarrier => "Verification Barrier",
            MiniPowerTab::PlanHierarchy => "Two-Tier Plan",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            MiniPowerTab::Pillars => MiniPowerTab::RedFlags,
            MiniPowerTab::RedFlags => MiniPowerTab::VerificationBarrier,
            MiniPowerTab::VerificationBarrier => MiniPowerTab::PlanHierarchy,
            MiniPowerTab::PlanHierarchy => MiniPowerTab::Pillars,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            MiniPowerTab::Pillars => MiniPowerTab::PlanHierarchy,
            MiniPowerTab::RedFlags => MiniPowerTab::Pillars,
            MiniPowerTab::VerificationBarrier => MiniPowerTab::RedFlags,
            MiniPowerTab::PlanHierarchy => MiniPowerTab::VerificationBarrier,
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
        MiniPowerTab::PlanHierarchy => render_plan_hierarchy_content(state, theme),
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
        _ => " [Tab/←/→] Switch Tab | [1-4] Select Tab | [↑/↓] Scroll | [Esc/q] Close ",
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

fn render_plan_hierarchy_content<'a>(
    state: &'a MiniPowerModalState,
    theme: &'a Theme,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::styled(
            "📋 Two-Tier Autonomous Plan Hierarchy",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " — Strategic Roadmap & Tactical Execution",
            Style::default().fg(theme.muted),
        ),
    ]));
    lines.push(Line::from(""));

    // --- LEVEL 1: MACRO ROADMAP ---
    lines.push(Line::from(vec![Span::styled(
        "═══ LEVEL 1: STRATEGIC ROADMAP (Macro Tasks) ═══",
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    )]));

    let todo_path = crate::tools::minikit::resolve_doc_path(&state.workspace_root, "todo.md");
    let rel_todo = todo_path
        .strip_prefix(&state.workspace_root)
        .unwrap_or(&todo_path);

    lines.push(Line::from(vec![
        Span::styled("Source: ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}", rel_todo.display()),
            Style::default().fg(theme.info),
        ),
    ]));
    lines.push(Line::from(""));

    if todo_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&todo_path) {
            let mut shown_tasks = 0;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("- [x]") || trimmed.starts_with("* [x]") {
                    let text = trimmed
                        .trim_start_matches("- [x]")
                        .trim_start_matches("* [x]")
                        .trim();
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  ✔ ",
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(text.to_string(), Style::default().fg(theme.muted)),
                    ]));
                    shown_tasks += 1;
                } else if trimmed.starts_with("- [ ]") || trimmed.starts_with("* [ ]") {
                    let text = trimmed
                        .trim_start_matches("- [ ]")
                        .trim_start_matches("* [ ]")
                        .trim();
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  ○ ",
                            Style::default()
                                .fg(theme.warning)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(text.to_string(), Style::default().fg(theme.text_primary)),
                    ]));
                    shown_tasks += 1;
                } else if trimmed.starts_with("## ") {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  📁 {}", trimmed.trim_start_matches('#').trim()),
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )]));
                }
                if shown_tasks >= 15 {
                    break;
                }
            }
            if shown_tasks == 0 {
                lines.push(Line::from(vec![Span::styled(
                    "  (No active tasks parsed in roadmap file)",
                    Style::default().fg(theme.muted),
                )]));
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "  (Failed to read roadmap file)",
                Style::default().fg(theme.destructive),
            )]));
        }
    } else {
        lines.push(Line::from(vec![Span::styled(
            "  (Roadmap file does not exist yet. Run `/plan <goal>` or `power_plan` to generate it)",
            Style::default().fg(theme.muted),
        )]));
    }

    lines.push(Line::from(""));

    // --- LEVEL 2: MICRO EXECUTION INTENT LEDGER ---
    lines.push(Line::from(vec![Span::styled(
        "═══ LEVEL 2: TACTICAL EXECUTION (Micro Intent Ledger) ═══",
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    )]));

    let intent_file = state
        .workspace_root
        .join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
    let rel_intent = intent_file
        .strip_prefix(&state.workspace_root)
        .unwrap_or(&intent_file);

    lines.push(Line::from(vec![
        Span::styled("Storage: ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}", rel_intent.display()),
            Style::default().fg(theme.info),
        ),
    ]));
    lines.push(Line::from(""));

    if intent_file.exists() {
        if let Ok(ledger) =
            crate::context::memory::intent::IntentLedger::load_from_disk(&intent_file)
        {
            if !ledger.root_objective.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  🎯 Root Goal: ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        ledger.root_objective.clone(),
                        Style::default().fg(theme.text_primary),
                    ),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("  📊 Progress: ", Style::default().fg(theme.muted)),
                    Span::styled(
                        format!(
                            "{}/{} items completed",
                            ledger.completed_count(),
                            ledger.total_count()
                        ),
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                lines.push(Line::from(""));
            }

            if ledger.items.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "  (Intent ledger is currently empty)",
                    Style::default().fg(theme.muted),
                )]));
            } else {
                for item in &ledger.items {
                    let is_active = ledger.active_item_id.as_deref() == Some(&item.id);
                    let (marker, style) = match item.status {
                        crate::context::memory::intent::RequirementStatus::Completed => (
                            "✔ Completed  ",
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ),
                        crate::context::memory::intent::RequirementStatus::InProgress => (
                            "▶ In-Progress",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        crate::context::memory::intent::RequirementStatus::Blocked => (
                            "✖ Blocked    ",
                            Style::default()
                                .fg(theme.destructive)
                                .add_modifier(Modifier::BOLD),
                        ),
                        crate::context::memory::intent::RequirementStatus::Skipped => {
                            ("↷ Skipped    ", Style::default().fg(theme.muted))
                        }
                        crate::context::memory::intent::RequirementStatus::Pending => {
                            ("○ Pending    ", Style::default().fg(theme.warning))
                        }
                    };

                    let active_marker = if is_active { " ◀ (Active Focus)" } else { "" };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  [{}] ", marker), style),
                        Span::styled(item.title.clone(), Style::default().fg(theme.text_primary)),
                        Span::styled(
                            active_marker,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "  (Failed to parse intent ledger)",
                Style::default().fg(theme.destructive),
            )]));
        }
    } else {
        lines.push(Line::from(vec![Span::styled(
            "  (No active tactical ledger. Initialized on autonomous `/goal` or `/power task` execution)",
            Style::default().fg(theme.muted),
        )]));
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
        assert_eq!(tab.next().next().next(), MiniPowerTab::PlanHierarchy);
        assert_eq!(tab.next().next().next().next(), MiniPowerTab::Pillars);

        assert_eq!(tab.prev(), MiniPowerTab::PlanHierarchy);
        assert_eq!(tab.prev().prev(), MiniPowerTab::VerificationBarrier);
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
