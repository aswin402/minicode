//! Interactive software architecture governance and dependency linting modal rendering.

use crate::context::governance::ArchitectureReport;
use crate::ui::layout_utils::centered_rect;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// Renders the interactive 3-tab architecture audit modal.
pub fn render_architecture_audit(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    report: &ArchitectureReport,
    active_tab: usize,
    selected_index: usize,
    scroll_offset: usize,
) {
    let popup_area = centered_rect(82, 80, area);
    frame.render_widget(Clear, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header & Tab Bar
            Constraint::Min(5),    // Main Content
            Constraint::Length(1), // Keyhint footer
        ])
        .split(popup_area);

    // 1. Header & Tab Bar
    let score_color = if report.health_score >= 90 {
        theme.success
    } else if report.health_score >= 70 {
        theme.warning
    } else {
        theme.destructive
    };

    let title_line = Line::from(vec![
        Span::styled(
            " 🏛️ Architecture Governance ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" [Score: {}/100] ", report.health_score),
            Style::default()
                .fg(score_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({} files, {} LOC) ", report.total_files, report.total_loc),
            Style::default().fg(theme.muted),
        ),
    ]);

    let tabs = [
        (" [1] Overview & Violations ", active_tab == 0),
        (" [2] Coupling Matrix ", active_tab == 1),
        (" [3] Circular Cycles ", active_tab == 2),
    ];

    let mut tab_spans = Vec::new();
    for (name, is_active) in tabs {
        if is_active {
            tab_spans.push(Span::styled(
                name,
                Style::default()
                    .fg(Color::Black)
                    .bg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(name, Style::default().fg(theme.muted)));
        }
        tab_spans.push(Span::raw(" "));
    }

    let header_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(title_line);

    let tab_paragraph = Paragraph::new(Line::from(tab_spans)).block(header_block);
    frame.render_widget(tab_paragraph, chunks[0]);

    // 2. Main Tab Content
    let mut content_lines = Vec::new();

    match active_tab {
        // Tab 0: Overview & Violations
        0 => {
            content_lines.push(Line::from(""));
            if report.layer_violations.is_empty() && report.circular_cycles.is_empty() {
                content_lines.push(Line::from(vec![
                    Span::styled("  ✔ ", Style::default().fg(theme.success)),
                    Span::styled(
                        "All architectural boundaries are intact and 100% DAG compliant.",
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![Span::styled(
                    "    No layer inversions or cross-module boundary violations detected.",
                    Style::default().fg(theme.muted),
                )]));
            } else {
                content_lines.push(Line::from(vec![Span::styled(
                    format!(
                        "  ⚠️  Detected {} Layer Violations and {} Circular Cycles:",
                        report.layer_violations.len(),
                        report.circular_cycles.len()
                    ),
                    Style::default().fg(theme.warning),
                )]));
                content_lines.push(Line::from(""));

                for (idx, v) in report.layer_violations.iter().enumerate() {
                    let is_sel = idx == selected_index;
                    let cursor = if is_sel { "  👉 " } else { "     " };
                    let style = if is_sel {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    };

                    content_lines.push(Line::from(vec![
                        Span::raw(cursor),
                        Span::styled(
                            format!("[{}] ", v.rule_id),
                            Style::default().fg(theme.destructive),
                        ),
                        Span::styled(format!("{}:{} ➔ ", v.source_file, v.line_number), style),
                        Span::styled(&v.target_module, Style::default().fg(theme.brand_accent)),
                    ]));

                    content_lines.push(Line::from(vec![
                        Span::raw("        "),
                        Span::styled(&v.message, Style::default().fg(theme.muted)),
                    ]));
                    content_lines.push(Line::from(""));
                }
            }

            // God files warning if any
            if !report.god_files.is_empty() {
                content_lines.push(Line::from(vec![Span::styled(
                    "  ⚠️  God Files (>1,000 LOC):",
                    Style::default().fg(theme.warning),
                )]));
                for (file, loc) in &report.god_files {
                    content_lines.push(Line::from(vec![
                        Span::raw("    • "),
                        Span::styled(file, Style::default().fg(theme.text_primary)),
                        Span::styled(
                            format!(" ({} lines)", loc),
                            Style::default().fg(theme.muted),
                        ),
                    ]));
                }
                content_lines.push(Line::from(""));
            }
        }

        // Tab 1: Module Coupling & Instability Matrix
        1 => {
            content_lines.push(Line::from(""));
            content_lines.push(Line::from(vec![Span::styled(
                "  Module Coupling & Robert C. Martin Instability (I = Ce / [Ca + Ce]):",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            )]));
            content_lines.push(Line::from(""));

            let header = format!(
                "  {:<18} {:>6} {:>8} {:>6} {:>6} {:>8}   {}",
                "Module", "Files", "LOC", "Ca", "Ce", "Instab(I)", "Role"
            );
            content_lines.push(Line::from(vec![Span::styled(
                header,
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::UNDERLINED),
            )]));

            for (idx, m) in report.coupling_metrics.iter().enumerate() {
                let is_sel = idx == selected_index;
                let prefix = if is_sel { "▶ " } else { "  " };
                let style = if is_sel {
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };

                let role = if m.instability < 0.3 {
                    "🛡️ Core Base"
                } else if m.instability > 0.7 {
                    "🍃 Leaf / UI"
                } else {
                    "⚖️ Mediator"
                };

                let row = format!(
                    "{}{:<18} {:>6} {:>8} {:>6} {:>6} {:>8.2}   {}",
                    prefix,
                    m.module_name,
                    m.file_count,
                    m.total_loc,
                    m.afferent_coupling,
                    m.efferent_coupling,
                    m.instability,
                    role
                );
                content_lines.push(Line::from(vec![Span::styled(row, style)]));
            }
        }

        // Tab 2: Circular Cycles
        _ => {
            content_lines.push(Line::from(""));
            if report.circular_cycles.is_empty() {
                content_lines.push(Line::from(vec![
                    Span::styled("  ✔ ", Style::default().fg(theme.success)),
                    Span::styled(
                        "Codebase is 100% DAG compliant — zero circular dependency cycles.",
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            } else {
                content_lines.push(Line::from(vec![Span::styled(
                    format!(
                        "  ⚠️  Detected {} Strongly Connected Component Cycles:",
                        report.circular_cycles.len()
                    ),
                    Style::default().fg(theme.destructive),
                )]));
                content_lines.push(Line::from(""));

                for (idx, cycle) in report.circular_cycles.iter().enumerate() {
                    let is_sel = idx == selected_index;
                    let cursor = if is_sel { "  👉 " } else { "     " };
                    content_lines.push(Line::from(vec![
                        Span::raw(cursor),
                        Span::styled(
                            format!("Cycle #{}: ", idx + 1),
                            Style::default().fg(theme.warning),
                        ),
                        Span::styled(cycle.join(" ➔ "), Style::default().fg(theme.text_primary)),
                    ]));
                }
            }
        }
    }

    let content_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(theme.border));

    let visible_lines: Vec<Line> = content_lines.into_iter().skip(scroll_offset).collect();
    let content_para = Paragraph::new(visible_lines).block(content_block);
    frame.render_widget(content_para, chunks[1]);

    // 3. Footer with keyhints
    let keyhints = Line::from(vec![
        Span::styled(" [1/2/3/Tab] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Switch View  ", Style::default().fg(theme.muted)),
        Span::styled(" [↑/↓] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Navigate/Scroll  ", Style::default().fg(theme.muted)),
        Span::styled(" [Esc] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Dismiss", Style::default().fg(theme.muted)),
    ]);
    let footer_para = Paragraph::new(keyhints).alignment(Alignment::Center);
    frame.render_widget(footer_para, chunks[2]);
}
