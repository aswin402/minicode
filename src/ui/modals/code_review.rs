//! Minimalist interactive Code Review & Security Audit dialog modal (`/review`).

use crate::git::{ReviewFinding, ReviewReport};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// State for the interactive `/review` Code Review dialog modal.
#[derive(Debug, Clone)]
pub struct CodeReviewModalState {
    pub report: ReviewReport,
    pub selected_index: usize,
    pub staged_only: bool,
}

impl CodeReviewModalState {
    pub fn new(report: ReviewReport, staged_only: bool) -> Self {
        Self {
            report,
            selected_index: 0,
            staged_only,
        }
    }

    pub fn next(&mut self) {
        if !self.report.findings.is_empty() && self.selected_index + 1 < self.report.findings.len()
        {
            self.selected_index += 1;
        }
    }

    pub fn prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn selected_finding(&self) -> Option<&ReviewFinding> {
        self.report.findings.get(self.selected_index)
    }
}

/// Renders the minimal modern Code Review dialog modal.
pub fn render_code_review(
    frame: &mut Frame,
    area: Rect,
    state: &CodeReviewModalState,
    theme: &Theme,
) {
    let popup_area = centered_rect(82, 78, area);
    frame.render_widget(Clear, popup_area);

    let score = state.report.total_score;
    let score_color = if score >= 90 {
        theme.success
    } else if score >= 70 {
        theme.warning
    } else {
        theme.destructive
    };

    let scope_label = if state.staged_only {
        "Staged"
    } else {
        "Working Tree"
    };

    let title_line = Line::from(vec![
        Span::styled(
            " ◆ Code Review · ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("Score {}/100", score),
            Style::default()
                .fg(score_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " · {} file(s) · {} ",
                state.report.files_audited, scope_label
            ),
            Style::default().fg(theme.muted),
        ),
    ]);

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(title_line)
        .title(
            Line::from(vec![Span::styled(
                " [Esc] Close ",
                Style::default().fg(theme.muted),
            )])
            .alignment(Alignment::Right),
        )
        .style(Style::default().bg(theme.bg_elevated));

    let inner_area = main_block.inner(popup_area);
    frame.render_widget(main_block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // 0: Summary header row
            Constraint::Min(6),    // 1: Findings list
            Constraint::Length(6), // 2: Selected finding detail card
            Constraint::Length(1), // 3: Divider
            Constraint::Length(1), // 4: Footer keyhints
        ])
        .split(inner_area);

    // 0. Summary bar
    let high_count = state
        .report
        .findings
        .iter()
        .filter(|f| f.severity.eq_ignore_ascii_case("HIGH"))
        .count();
    let med_count = state
        .report
        .findings
        .iter()
        .filter(|f| f.severity.eq_ignore_ascii_case("MEDIUM"))
        .count();
    let low_count = state
        .report
        .findings
        .iter()
        .filter(|f| f.severity.eq_ignore_ascii_case("LOW"))
        .count();

    let summary_para = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            &state.report.summary,
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(summary_para, chunks[0]);

    // 1. Findings list
    let mut list_lines = Vec::new();
    if state.report.findings.is_empty() {
        list_lines.push(Line::from(""));
        list_lines.push(Line::from(vec![
            Span::styled("   ✓ ", Style::default().fg(theme.success)),
            Span::styled(
                "No issues or risks detected across audited files.",
                Style::default().fg(theme.text_primary),
            ),
        ]));
    } else {
        let max_vis = (chunks[1].height as usize).max(1);
        let scroll = compute_scroll_offset(state.selected_index, max_vis);
        for (rel_idx, finding) in state
            .report
            .findings
            .iter()
            .enumerate()
            .skip(scroll)
            .take(max_vis)
        {
            let is_sel = rel_idx == state.selected_index;
            let marker = if is_sel { "  ◉ " } else { "  ○ " };
            let marker_style = if is_sel {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };

            let (sev_badge, sev_color) = match finding.severity.to_uppercase().as_str() {
                "HIGH" => (" HIGH ", theme.destructive),
                "MEDIUM" => (" MED  ", theme.warning),
                "LOW" => (" LOW  ", theme.brand_accent),
                _ => (" PASS ", theme.success),
            };

            let title_style = if is_sel {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            list_lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::styled(
                    sev_badge,
                    Style::default()
                        .fg(theme.bg_primary)
                        .bg(sev_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("[{}] ", finding.category),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(&finding.title, title_style),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(list_lines), chunks[1]);

    // 2. Selected finding details box
    let detail_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " Finding Details ",
            Style::default().fg(theme.muted),
        ));
    let detail_inner = detail_block.inner(chunks[2]);
    frame.render_widget(detail_block, chunks[2]);

    if let Some(finding) = state.selected_finding() {
        let mut detail_lines = vec![Line::from(vec![
            Span::raw(" "),
            Span::styled(&finding.details, Style::default().fg(theme.text_primary)),
        ])];
        if let Some(ref hint) = finding.line_hint {
            if !hint.trim().is_empty() {
                detail_lines.push(Line::from(vec![
                    Span::styled(" Location: ", Style::default().fg(theme.muted)),
                    Span::styled(hint, Style::default().fg(theme.brand_accent)),
                ]));
            }
        }
        let detail_p = Paragraph::new(detail_lines).wrap(Wrap { trim: true });
        frame.render_widget(detail_p, detail_inner);
    }

    // 3. Divider
    let div_width = chunks[3].width as usize;
    let div_line = Line::from(Span::styled(
        "─".repeat(div_width),
        Style::default().fg(theme.border),
    ));
    frame.render_widget(Paragraph::new(div_line), chunks[3]);

    // 4. Footer
    let footer_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(chunks[4]);

    let counts_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{} High", high_count),
            Style::default().fg(if high_count > 0 {
                theme.destructive
            } else {
                theme.muted
            }),
        ),
        Span::styled(" · ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} Med", med_count),
            Style::default().fg(if med_count > 0 {
                theme.warning
            } else {
                theme.muted
            }),
        ),
        Span::styled(" · ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} Low", low_count),
            Style::default().fg(theme.muted),
        ),
    ]);
    frame.render_widget(Paragraph::new(counts_line), footer_cols[0]);

    let hints_line = Line::from(vec![
        Span::styled("[↑/↓]", Style::default().fg(theme.brand_accent)),
        Span::styled(" Select  ", Style::default().fg(theme.muted)),
        Span::styled("[Tab]", Style::default().fg(theme.brand_accent)),
        Span::styled(" Scope  ", Style::default().fg(theme.muted)),
        Span::styled("[d]", Style::default().fg(theme.brand_accent)),
        Span::styled(" Git Diff  ", Style::default().fg(theme.muted)),
        Span::styled("[Esc]", Style::default().fg(theme.brand_accent)),
        Span::styled(" Close ", Style::default().fg(theme.muted)),
    ])
    .alignment(Alignment::Right);
    frame.render_widget(Paragraph::new(hints_line), footer_cols[1]);
}
