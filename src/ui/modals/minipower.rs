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

/// Active tab within the MiniPower modal (`Ctrl+P`).
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

/// Cached snapshot of the Two-Tier Plan Hierarchy to avoid 60fps disk I/O during rendering.
#[derive(Debug, Clone)]
pub struct CachedPlanHierarchy {
    pub rel_todo_display: String,
    pub todo_status: CachedTodoStatus,
    pub rel_intent_display: String,
    pub intent_status: CachedIntentStatus,
}

#[derive(Debug, Clone)]
pub enum CachedTodoStatus {
    Loaded(Vec<(PlanRowKind, String)>),
    ReadError,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanRowKind {
    Completed,
    Pending,
    Heading,
}

#[derive(Debug, Clone)]
pub enum CachedIntentStatus {
    Loaded(crate::context::memory::intent::IntentLedger),
    ParseError,
    Missing,
}

impl CachedPlanHierarchy {
    pub fn load(workspace_root: &Path) -> Self {
        let todo_path = crate::tools::minikit::resolve_doc_path(workspace_root, "todo.md");
        let rel_todo_display = todo_path
            .strip_prefix(workspace_root)
            .unwrap_or(&todo_path)
            .display()
            .to_string();

        let todo_status = if todo_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&todo_path) {
                let mut rows = Vec::new();
                let mut shown_tasks = 0;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed
                        .strip_prefix("- [x]")
                        .or_else(|| trimmed.strip_prefix("* [x]"))
                    {
                        rows.push((PlanRowKind::Completed, rest.trim().to_string()));
                        shown_tasks += 1;
                    } else if let Some(rest) = trimmed
                        .strip_prefix("- [ ]")
                        .or_else(|| trimmed.strip_prefix("* [ ]"))
                    {
                        rows.push((PlanRowKind::Pending, rest.trim().to_string()));
                        shown_tasks += 1;
                    } else if trimmed.starts_with("## ") {
                        rows.push((
                            PlanRowKind::Heading,
                            format!("  ◈ {}", trimmed.trim_start_matches('#').trim()),
                        ));
                    }
                    if shown_tasks >= 15 {
                        break;
                    }
                }
                CachedTodoStatus::Loaded(rows)
            } else {
                CachedTodoStatus::ReadError
            }
        } else {
            CachedTodoStatus::Missing
        };

        let intent_file = workspace_root.join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
        let rel_intent_display = intent_file
            .strip_prefix(workspace_root)
            .unwrap_or(&intent_file)
            .display()
            .to_string();

        let intent_status = if intent_file.exists() {
            match crate::context::memory::intent::IntentLedger::load_from_disk(&intent_file) {
                Ok(ledger) => CachedIntentStatus::Loaded(ledger),
                Err(_) => CachedIntentStatus::ParseError,
            }
        } else {
            CachedIntentStatus::Missing
        };

        Self {
            rel_todo_display,
            todo_status,
            rel_intent_display,
            intent_status,
        }
    }
}

/// State for the interactive MiniPower modal dialog (`Ctrl+P`).
#[derive(Debug, Clone)]
pub struct MiniPowerModalState {
    #[allow(dead_code)]
    pub workspace_root: PathBuf,
    pub active_tab: MiniPowerTab,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub verification_report: Option<VerificationReport>,
    pub is_evaluating: bool,
    pub cached_plan: CachedPlanHierarchy,
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
            cached_plan: CachedPlanHierarchy::load(workspace),
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = self.active_tab.next();
        self.scroll_offset = 0;
        self.selected_index = 0;
        if self.active_tab == MiniPowerTab::PlanHierarchy {
            self.cached_plan = CachedPlanHierarchy::load(&self.workspace_root);
        }
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = self.active_tab.prev();
        self.scroll_offset = 0;
        self.selected_index = 0;
        if self.active_tab == MiniPowerTab::PlanHierarchy {
            self.cached_plan = CachedPlanHierarchy::load(&self.workspace_root);
        }
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
        .title(" ◆ MiniPower — Native Autonomous Engineering ")
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

    let visible_height = chunks[1].height as usize;
    let max_scroll = content_lines.len().saturating_sub(visible_height);
    let scroll = state.scroll_offset.min(max_scroll);
    let visible_lines: Vec<Line> = content_lines.into_iter().skip(scroll).collect();

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
    const PILLARS: &[(&str, &str)] = &[
        (
            "  1. ◉ Socratic Brainstorming   ",
            "Clarify intent, ceremony size & Builder Check before coding (`power_brainstorm`)",
        ),
        (
            "  2. ◈ Git Worktree Isolation   ",
            "Execute mutating tasks in ephemeral worktrees (`power_worktree_task`)",
        ),
        (
            "  3. ◆ Bite-Sized Planning      ",
            "Decision-focused tasks with Consumes/Produces & Review Focus (`power_plan`)",
        ),
        (
            "  4. ◆ Two-Stage Subagent Review",
            "Automated Stage 1 (Spec Compliance) & Stage 2 (Code Quality) (`power_review`)",
        ),
        (
            "  5. ◈ Strict Red/Green TDD     ",
            "Falsifiability gate, zero mirror/mock assertions, RED -> GREEN -> full suite",
        ),
        (
            "  6. ◆ Evidence & Forensics     ",
            "4-Gate barrier (`power_verify`) & 7-dimension transcript audit (`power_diagnose`)",
        ),
    ];

    let mut lines = Vec::with_capacity(PILLARS.len() + 4);
    lines.push(Line::from(vec![
        Span::styled(
            "6 Core Engineering Pillars",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " — Autonomous Orchestrator Methodology",
            Style::default().fg(theme.muted),
        ),
    ]));
    lines.push(Line::from(""));

    let label_style = Style::default()
        .fg(theme.success)
        .add_modifier(Modifier::BOLD);
    for &(title, desc) in PILLARS {
        lines.push(Line::from(vec![
            Span::styled(title, label_style),
            Span::raw(desc),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "◈ Orchestrator Tools: ",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(
            "power_brainstorm | power_plan | power_worktree_task | power_review | power_verify | power_diagnose",
        ),
    ]));
    lines
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
            "  ◷ Evaluating verification gates across workspace...",
            Style::default().fg(theme.warning),
        )]));
        return lines;
    }

    if let Some(ref report) = state.verification_report {
        let badge = if report.all_passed {
            Span::styled(
                "  ✓ ALL 4 GATES PASSED CLEANLY",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                "  ✗ VERIFICATION ISSUES DETECTED",
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
                        Span::styled("  ✓ ", Style::default().fg(theme.success)),
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
                        Span::styled("  ✗ ", Style::default().fg(theme.destructive)),
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
    let cached = &state.cached_plan;

    lines.push(Line::from(vec![
        Span::styled(
            "◆ Two-Tier Autonomous Plan Hierarchy",
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

    lines.push(Line::from(vec![
        Span::styled("Source: ", Style::default().fg(theme.muted)),
        Span::styled(
            cached.rel_todo_display.as_str(),
            Style::default().fg(theme.info),
        ),
    ]));
    lines.push(Line::from(""));

    match &cached.todo_status {
        CachedTodoStatus::Loaded(rows) => {
            let mut shown_tasks = 0;
            for (kind, text) in rows {
                match kind {
                    PlanRowKind::Completed => {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "  ✓ ",
                                Style::default()
                                    .fg(theme.success)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(text.as_str(), Style::default().fg(theme.muted)),
                        ]));
                        shown_tasks += 1;
                    }
                    PlanRowKind::Pending => {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "  ○ ",
                                Style::default()
                                    .fg(theme.warning)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(text.as_str(), Style::default().fg(theme.text_primary)),
                        ]));
                        shown_tasks += 1;
                    }
                    PlanRowKind::Heading => {
                        lines.push(Line::from(vec![Span::styled(
                            text.as_str(),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        )]));
                    }
                }
            }
            if shown_tasks == 0 {
                lines.push(Line::from(vec![Span::styled(
                    "  (No active tasks parsed in roadmap file)",
                    Style::default().fg(theme.muted),
                )]));
            }
        }
        CachedTodoStatus::ReadError => {
            lines.push(Line::from(vec![Span::styled(
                "  (Failed to read roadmap file)",
                Style::default().fg(theme.destructive),
            )]));
        }
        CachedTodoStatus::Missing => {
            lines.push(Line::from(vec![Span::styled(
                "  (Roadmap file does not exist yet. Run `/plan <goal>` or `power_plan` to generate it)",
                Style::default().fg(theme.muted),
            )]));
        }
    }

    lines.push(Line::from(""));

    // --- LEVEL 2: MICRO EXECUTION INTENT LEDGER ---
    lines.push(Line::from(vec![Span::styled(
        "═══ LEVEL 2: TACTICAL EXECUTION (Micro Intent Ledger) ═══",
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    )]));

    lines.push(Line::from(vec![
        Span::styled("Storage: ", Style::default().fg(theme.muted)),
        Span::styled(
            cached.rel_intent_display.as_str(),
            Style::default().fg(theme.info),
        ),
    ]));
    lines.push(Line::from(""));

    match &cached.intent_status {
        CachedIntentStatus::Loaded(ledger) => {
            if !ledger.root_objective.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  ◉ Root Goal: ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        ledger.root_objective.as_str(),
                        Style::default().fg(theme.text_primary),
                    ),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("  ◈ Progress: ", Style::default().fg(theme.muted)),
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
                            "✓ Completed  ",
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ),
                        crate::context::memory::intent::RequirementStatus::InProgress => (
                            "▸ In-Progress",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        crate::context::memory::intent::RequirementStatus::Blocked => (
                            "✗ Blocked    ",
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
                        Span::styled(item.title.as_str(), Style::default().fg(theme.text_primary)),
                        Span::styled(
                            active_marker,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
            }
        }
        CachedIntentStatus::ParseError => {
            lines.push(Line::from(vec![Span::styled(
                "  (Failed to parse intent ledger)",
                Style::default().fg(theme.destructive),
            )]));
        }
        CachedIntentStatus::Missing => {
            lines.push(Line::from(vec![Span::styled(
                "  (No active tactical ledger. Initialized automatically by the orchestrator during plan execution)",
                Style::default().fg(theme.muted),
            )]));
        }
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
