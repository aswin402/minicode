//! Animated Interactive MiniDoctor Health Diagnostics Modal (`/doctor`).
//!
//! Features:
//! - Live non-blocking background scan with 5-stage animated choreography & active theme spinner
//! - Smooth ease-out Health Score count-up & color-transition gauge bar (`flutter_doctor`-inspired)
//! - Historical sparkline trend (`▁▂▃▄▅▆▇█`) + score delta from `.minicode/minidoctor/history.jsonl`
//! - Filterable diagnostic table (`CRIT` / `ERR` / `WARN` / `DEAD`), live detail drawer & Top Priority Fixes
//! - Direct 1-keystroke agent auto-fix (`[f]` Fix All Crit/Err, `[Enter]` Fix Selected),
//!   `.minicode/minidoctor/NEEDEDFIX.md` export (`[e]`), Diff/Full toggle (`[d]`), and visual style cycling (`[s]`)

use crate::ui::animation::{render_preview_spinner, SpinnerStyle};
use crate::ui::icons::{nf, NerdIcon};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use minidoctor::{
    explain_rule, render_sparkline, Diagnostic, DiagnosticCategory, DoctorOptions, DoctorReport,
    ScanPhase, ScanProgress, ScanScope, Severity,
};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unicode_width::UnicodeWidthStr;

/// Filter tabs inside the `/doctor` diagnostic explorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoctorFilterTab {
    #[default]
    All,
    CriticalAndErrors,
    Warnings,
    DeadCode,
    Security,
}

impl DoctorFilterTab {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::CriticalAndErrors,
        Self::Warnings,
        Self::DeadCode,
        Self::Security,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::CriticalAndErrors => "Crit/Err",
            Self::Warnings => "Warnings",
            Self::DeadCode => "Dead Code",
            Self::Security => "Security",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::All => Self::CriticalAndErrors,
            Self::CriticalAndErrors => Self::Warnings,
            Self::Warnings => Self::DeadCode,
            Self::DeadCode => Self::Security,
            Self::Security => Self::All,
        }
    }

    #[must_use]
    pub fn matches(self, d: &Diagnostic) -> bool {
        match self {
            Self::All => true,
            Self::CriticalAndErrors => matches!(d.severity, Severity::Critical | Severity::Error),
            Self::Warnings => {
                d.severity == Severity::Warning && d.category != DiagnosticCategory::DeadCode
            }
            Self::DeadCode => d.category == DiagnosticCategory::DeadCode,
            Self::Security => d.category == DiagnosticCategory::Security,
        }
    }
}

/// Visual chrome themes persisted via `ui.doctor_style` (`/theme` → `20. Doctor Styles`)
/// and cycleable live via `[s]` inside `/doctor`.
pub use crate::config::DoctorStyle as DoctorVisualStyle;

#[derive(Debug, Default)]
struct DoctorLiveScan {
    progress: Option<ScanProgress>,
    finished_report: Option<Result<DoctorReport, String>>,
}

/// Interactive state for `ModalState::Doctor`.
#[derive(Debug, Clone)]
pub struct DoctorModalState {
    pub workspace_root: PathBuf,
    pub scope: ScanScope,
    pub report: Option<DoctorReport>,
    pub scanning: bool,
    pub scan_started_at: Instant,
    pub reveal_started_at: Option<Instant>,
    pub live_progress: Option<ScanProgress>,
    pub active_tab: DoctorFilterTab,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub visual_style: DoctorVisualStyle,
    pub spinner_style: SpinnerStyle,
    pub show_sparkline: bool,
    pub show_subscores: bool,
    pub show_detail_drawer: bool,
    pub animate_reveal: bool,
    pub status_banner: Option<String>,
    pub error_message: Option<String>,
    live_channel: Option<Arc<Mutex<DoctorLiveScan>>>,
}

impl DoctorModalState {
    /// Creates a new modal state, loading any cached report for instant preview, and
    /// immediately kicking off a live background scan if `start_immediately` is true.
    pub fn new(workspace_root: &Path, scope: ScanScope, start_immediately: bool) -> Self {
        let cached = minidoctor::load_latest_report(workspace_root);
        let mut state = Self {
            workspace_root: workspace_root.to_path_buf(),
            scope: scope.clone(),
            report: cached,
            scanning: false,
            scan_started_at: Instant::now(),
            reveal_started_at: None,
            live_progress: None,
            active_tab: DoctorFilterTab::All,
            selected_index: 0,
            scroll_offset: 0,
            visual_style: DoctorVisualStyle::Aura,
            spinner_style: SpinnerStyle::default(),
            show_sparkline: true,
            show_subscores: true,
            show_detail_drawer: true,
            animate_reveal: true,
            status_banner: None,
            error_message: None,
            live_channel: None,
        };
        if start_immediately {
            state.start_scan(scope);
        } else if state.report.is_some() {
            state.reveal_started_at = Some(Instant::now());
        }
        state
    }

    /// Applies persisted `/theme` preferences (`doctor_style`, animation spinner, and Doctor visibility toggles).
    pub fn apply_ui_config(&mut self, ui: &crate::config::UiConfig) {
        self.visual_style = ui.doctor_style_enum();
        self.spinner_style = SpinnerStyle::from_id(&ui.animation);
        self.show_sparkline = ui.show_doctor_sparkline;
        self.show_subscores = ui.show_doctor_subscores;
        self.show_detail_drawer = ui.show_doctor_detail_drawer;
        self.animate_reveal = ui.doctor_animate_reveal;
    }

    /// Creates a pre-populated static state (useful for deterministic unit tests and cached snapshots).
    #[cfg(test)]
    pub fn from_report_for_test(workspace_root: &Path, report: DoctorReport) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            scope: ScanScope::Full,
            report: Some(report),
            scanning: false,
            scan_started_at: Instant::now(),
            reveal_started_at: None,
            live_progress: None,
            active_tab: DoctorFilterTab::All,
            selected_index: 0,
            scroll_offset: 0,
            visual_style: DoctorVisualStyle::Aura,
            spinner_style: SpinnerStyle::default(),
            show_sparkline: true,
            show_subscores: true,
            show_detail_drawer: true,
            animate_reveal: true,
            status_banner: None,
            error_message: None,
            live_channel: None,
        }
    }

    /// Spawns a non-blocking background `minidoctor` scan streaming live phase progress.
    pub fn start_scan(&mut self, scope: ScanScope) {
        self.scope = scope.clone();
        self.scanning = true;
        self.scan_started_at = Instant::now();
        self.reveal_started_at = None;
        self.error_message = None;
        self.status_banner = None;
        self.live_progress = Some(ScanProgress {
            phase: ScanPhase::DetectingStacks,
            percent: 4,
            detail: "Initializing universal native Rust analyzer...".to_string(),
            detected_stacks: Vec::new(),
            files_scanned: 0,
            findings_so_far: 0,
        });

        let shared = Arc::new(Mutex::new(DoctorLiveScan::default()));
        self.live_channel = Some(Arc::clone(&shared));

        let ws = self.workspace_root.clone();
        let opts = DoctorOptions {
            scope,
            run_dead_code: true,
            run_security: true,
            write_workspace_artifacts: true,
            export_md_path: None,
            threshold_override: None,
            max_files: 2500,
        };

        std::thread::spawn(move || {
            let res = minidoctor::run_doctor_with_progress(&ws, &opts, |prog| {
                if let Ok(mut guard) = shared.lock() {
                    guard.progress = Some(prog);
                }
            });
            if let Ok(mut guard) = shared.lock() {
                guard.finished_report = Some(res.map_err(|e| e.to_string()));
            }
        });
    }

    /// Polls the background scan thread without blocking. Returns `true` when the scan transitions to completed.
    pub fn poll_progress(&mut self) -> bool {
        let Some(ref shared) = self.live_channel else {
            return false;
        };
        let (prog_opt, finished_opt) = {
            let Ok(mut guard) = shared.lock() else {
                return false;
            };
            (guard.progress.clone(), guard.finished_report.take())
        };

        if let Some(p) = prog_opt {
            self.live_progress = Some(p);
        }

        if let Some(res) = finished_opt {
            self.scanning = false;
            self.live_channel = None;
            match res {
                Ok(report) => {
                    let md_note = report
                        .neededfix_path
                        .as_deref()
                        .unwrap_or(".minicode/minidoctor/NEEDEDFIX.md");
                    self.status_banner = Some(format!(
                        "Saved human + AI checklist to {md_note} ({}ms)",
                        report.duration_ms
                    ));
                    self.report = Some(report);
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                    self.reveal_started_at = Some(Instant::now());
                }
                Err(err) => {
                    self.error_message = Some(err);
                }
            }
            return true;
        }
        false
    }

    /// Returns `true` while scanning OR during the 850ms post-scan score count-up reveal animation.
    /// Automatically drops back to `false` once settled for 0% idle CPU.
    #[must_use]
    pub fn has_live_animation(&self) -> bool {
        self.scanning
            || (self.animate_reveal
                && self
                    .reveal_started_at
                    .is_some_and(|t| t.elapsed() < Duration::from_millis(850)))
    }

    /// Returns the filtered diagnostics slice for the active tab.
    #[must_use]
    pub fn filtered_diagnostics(&self) -> Vec<&Diagnostic> {
        let Some(ref rep) = self.report else {
            return Vec::new();
        };
        rep.diagnostics
            .iter()
            .filter(|d| self.active_tab.matches(d))
            .collect()
    }

    pub fn select_next(&mut self) {
        let len = self.filtered_diagnostics().len();
        if len > 0 {
            self.selected_index = (self.selected_index + 1) % len;
        }
    }

    pub fn select_prev(&mut self) {
        let len = self.filtered_diagnostics().len();
        if len > 0 {
            self.selected_index = if self.selected_index == 0 {
                len - 1
            } else {
                self.selected_index - 1
            };
        }
    }

    pub fn set_tab(&mut self, tab: DoctorFilterTab) {
        self.active_tab = tab;
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn toggle_diff_scope(&mut self) {
        let next_scope = match self.scope {
            ScanScope::Full => ScanScope::Changed(None),
            _ => ScanScope::Full,
        };
        self.start_scan(next_scope);
    }

    /// Forces a fresh write of `.minicode/minidoctor/NEEDEDFIX.md` from the current report.
    pub fn refresh_neededfix_md(&mut self) -> Result<String, String> {
        let Some(ref mut rep) = self.report else {
            return Err("No report available yet — wait for scan to finish.".to_string());
        };
        let path = minidoctor::persist_workspace_artifacts(&self.workspace_root, rep, None)
            .map_err(|e| e.to_string())?;
        let display = path
            .strip_prefix(&self.workspace_root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".minicode/minidoctor/NEEDEDFIX.md".to_string());
        self.status_banner = Some(format!("Refreshed {display} & latest.json"));
        Ok(display)
    }

    /// Builds a prioritized AI remediation prompt targeting `.minicode/minidoctor/NEEDEDFIX.md`.
    #[must_use]
    pub fn build_fix_all_prompt(&self) -> Option<String> {
        let rep = self.report.as_ref()?;
        if rep.diagnostics.is_empty() {
            return None;
        }
        let md_path = rep
            .neededfix_path
            .as_deref()
            .unwrap_or(".minicode/minidoctor/NEEDEDFIX.md");
        let top = rep.top_priority_diagnostics(6);
        let mut prompt = format!(
            "Read `{md_path}` and fix the prioritized MiniDoctor issues (current Health Score: {}/100 — {}). Focus on Critical and Error findings first, then re-run `project_doctor` and tests to verify the score improves.\n",
            rep.score.overall, rep.score.label
        );
        if !top.is_empty() {
            prompt.push_str("\nTop blockers:\n");
            for d in top {
                prompt.push_str(&format!(
                    "- [{}] `{}` at `{}`: {} (Fix: {})\n",
                    d.severity.badge(),
                    d.rule,
                    d.location(),
                    d.message,
                    d.suggestion
                ));
            }
        }
        Some(prompt)
    }

    /// Builds a surgical AI remediation prompt targeting only the currently highlighted finding.
    #[must_use]
    pub fn build_fix_selected_prompt(&self) -> Option<String> {
        let items = self.filtered_diagnostics();
        let d = items.get(self.selected_index.min(items.len().saturating_sub(1)))?;
        Some(format!(
            "Fix the MiniDoctor [{}] finding `{}` at `{}`:\n- Issue: {}\n- Suggested fix: {}\nAfter fixing, verify compilation/tests and run `project_doctor(scope=\"changed\")` to confirm the issue is resolved.",
            d.severity.badge(),
            d.rule,
            d.location(),
            d.message,
            d.suggestion
        ))
    }

    /// Computes eased animation progress `0.0..=1.0` for the post-scan score count-up reveal.
    #[must_use]
    fn reveal_progress(&self) -> f64 {
        if !self.animate_reveal {
            return 1.0;
        }
        let Some(started) = self.reveal_started_at else {
            return 1.0;
        };
        let t = (started.elapsed().as_secs_f64() / 0.72).clamp(0.0, 1.0);
        // Cubic ease-out: fast lift, smooth landing on final score
        1.0 - (1.0 - t).powi(3)
    }
}

fn current_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn score_color(score: u8, theme: &Theme) -> Color {
    if score >= 75 {
        theme.success
    } else if score >= 50 {
        theme.warning
    } else {
        theme.destructive
    }
}

/// Renders the animated `/doctor` health diagnostic modal inside `minicode`'s TUI.
pub fn render_doctor_modal(
    frame: &mut Frame,
    area: Rect,
    state: &DoctorModalState,
    theme: &Theme,
    spinner_style: SpinnerStyle,
) {
    let popup = centered_rect(92, 88, area);
    frame.render_widget(Clear, popup);

    let is_ascii = state.visual_style == DoctorVisualStyle::Ascii;
    let is_cyber = state.visual_style == DoctorVisualStyle::Cyber;
    let is_minimal = state.visual_style == DoctorVisualStyle::Minimal;
    let header_badge = match state.visual_style {
        DoctorVisualStyle::Aura => "◈ MINIDOCTOR".to_string(),
        DoctorVisualStyle::NerdFont => format!("{} MINIDOCTOR", nf(NerdIcon::Sparkle)),
        DoctorVisualStyle::Kaomoji => "(◉_◉) MINIDOCTOR".to_string(),
        DoctorVisualStyle::Emoji => "🩺 MINIDOCTOR".to_string(),
        DoctorVisualStyle::Ascii => "[MINIDOCTOR]".to_string(),
        DoctorVisualStyle::Posh => "❖ MINIDOCTOR".to_string(),
        DoctorVisualStyle::Flutter => "◆ FLUTTER-STYLE MINIDOCTOR".to_string(),
        DoctorVisualStyle::Minimal => "MINIDOCTOR".to_string(),
        DoctorVisualStyle::Cyber => "◈ HUD // MINIDOCTOR".to_string(),
        DoctorVisualStyle::Compact => "▪ MINIDOCTOR".to_string(),
    };

    let mut outer = Block::default()
        .title(Line::from(vec![
            Span::styled(
                format!(" {header_badge} "),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "{} {} {} [{}] ",
                    if is_ascii { "-" } else { "·" },
                    state.scope.label(),
                    if is_ascii { "-" } else { "·" },
                    state.visual_style.label()
                ),
                Style::default().fg(theme.muted),
            ),
        ]))
        .title_alignment(Alignment::Left)
        .borders(if is_minimal {
            Borders::TOP | Borders::BOTTOM
        } else {
            Borders::ALL
        })
        .border_style(Style::default().fg(if is_cyber {
            theme.info
        } else {
            theme.brand_accent
        }))
        .style(Style::default().bg(theme.bg_primary));

    if is_ascii {
        outer = outer.border_set(border::Set {
            top_left: "+",
            top_right: "+",
            bottom_left: "+",
            bottom_right: "+",
            vertical_left: "|",
            vertical_right: "|",
            horizontal_top: "-",
            horizontal_bottom: "-",
        });
    } else if is_cyber {
        outer = outer.border_type(BorderType::Double);
    } else if is_minimal {
        outer = outer.border_type(BorderType::Plain);
    } else {
        outer = outer.border_type(BorderType::Rounded);
    }

    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    if inner.height < 10 || inner.width < 36 {
        return;
    }

    if state.scanning {
        render_scanning_choreography(frame, inner, state, theme, spinner_style);
        return;
    }

    if let Some(ref err) = state.error_message {
        let p = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  ✗ Scan failed: {err}"),
                Style::default()
                    .fg(theme.destructive)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press [r] to retry or [Esc] to close.",
                Style::default().fg(theme.muted),
            )),
        ]);
        frame.render_widget(p, inner);
        return;
    }

    let Some(ref report) = state.report else {
        let p = Paragraph::new(Line::from(Span::styled(
            "  Press [r] to run MiniDoctor health analysis...",
            Style::default().fg(theme.muted),
        )));
        frame.render_widget(p, inner);
        return;
    };

    render_completed_dashboard(frame, inner, state, report, theme);
}

fn render_scanning_choreography(
    frame: &mut Frame,
    area: Rect,
    state: &DoctorModalState,
    theme: &Theme,
    spinner_style: SpinnerStyle,
) {
    let millis = current_epoch_millis();
    let elapsed = state.scan_started_at.elapsed().as_secs_f64();
    let prog = state.live_progress.as_ref();
    let active_phase = prog.map(|p| p.phase).unwrap_or(ScanPhase::DetectingStacks);
    let pct = prog.map(|p| p.percent).unwrap_or(5);
    let detail = prog
        .map(|p| p.detail.as_str())
        .unwrap_or("Scanning workspace...");
    let is_ascii = state.visual_style == DoctorVisualStyle::Ascii;

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(""));

    // Animated spinner + title row
    let mut head_spans = vec![Span::raw("  ")];
    head_spans.extend(render_preview_spinner(spinner_style, millis, theme));
    head_spans.push(Span::styled(
        " Running Universal Native Rust Health Scan  ",
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    ));
    head_spans.push(Span::styled(
        format!("{elapsed:.1}s  ({pct}%)"),
        Style::default().fg(theme.info),
    ));
    lines.push(Line::from(head_spans));
    lines.push(Line::from(""));

    // Smooth animated pulsing progress bar
    let bar_w = (area.width as usize).saturating_sub(14).clamp(20, 56);
    let filled = ((pct as usize) * bar_w) / 100;
    let pulse_pos = ((millis / 70) as usize) % bar_w.max(1);
    let mut bar_spans = vec![Span::raw("  ")];
    for i in 0..bar_w {
        let (ch, col) = if i < filled {
            let glow = i == filled.saturating_sub(1) || i == pulse_pos;
            (
                if is_ascii { "=" } else { "━" },
                if glow {
                    theme.highlight
                } else {
                    theme.brand_accent
                },
            )
        } else if i == pulse_pos {
            (if is_ascii { ">" } else { "╸" }, theme.info)
        } else {
            (if is_ascii { "-" } else { "─" }, theme.border)
        };
        bar_spans.push(Span::styled(ch, Style::default().fg(col)));
    }
    bar_spans.push(Span::styled(
        format!("  {pct:>3}%"),
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::from(bar_spans));
    lines.push(Line::from(""));

    // 5-Stage Live Pipeline Checklist
    let phases = [
        ScanPhase::DetectingStacks,
        ScanPhase::NativeAstRules,
        ScanPhase::DeadCodeAndDuplicates,
        ScanPhase::SecurityAudit,
        ScanPhase::ScoringAndArtifacts,
    ];
    for ph in phases {
        let cur_idx = active_phase.index();
        let ph_idx = ph.index();
        let (icon, col, label_style) = if ph_idx < cur_idx {
            (
                if is_ascii { "[ok]" } else { "✓" },
                theme.success,
                Style::default().fg(theme.muted),
            )
        } else if ph_idx == cur_idx {
            let spin_frames = if is_ascii {
                ["-", "\\", "|", "/"]
            } else {
                ["⠋", "⠙", "⠸", "⠴"]
            };
            let f = spin_frames[((millis / 90) as usize) % spin_frames.len()];
            (
                f,
                theme.brand_accent,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (
                if is_ascii { "[ ]" } else { "○" },
                theme.border,
                Style::default().fg(theme.muted),
            )
        };
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("{icon:<4} "),
                Style::default().fg(col).add_modifier(Modifier::BOLD),
            ),
            Span::styled(ph.label(), label_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Current: ", Style::default().fg(theme.muted)),
        Span::styled(detail.to_string(), Style::default().fg(theme.info)),
    ]));

    if let Some(p) = prog {
        if !p.detected_stacks.is_empty() {
            let stack_list = p
                .detected_stacks
                .iter()
                .map(|s| s.label())
                .collect::<Vec<_>>()
                .join(" · ");
            lines.push(Line::from(vec![
                Span::styled("  Stacks:  ", Style::default().fg(theme.muted)),
                Span::styled(
                    stack_list,
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("  Metrics: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!(
                    "{} file(s) analyzed · {} finding(s) so far",
                    p.files_scanned, p.findings_so_far
                ),
                Style::default().fg(theme.text_primary),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Artifacts will be saved to .minicode/minidoctor/NEEDEDFIX.md",
        Style::default().fg(theme.muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_completed_dashboard(
    frame: &mut Frame,
    area: Rect,
    state: &DoctorModalState,
    report: &DoctorReport,
    theme: &Theme,
) {
    let is_ascii = state.visual_style == DoctorVisualStyle::Ascii;
    let is_cyber = state.visual_style == DoctorVisualStyle::Cyber;
    let is_minimal = state.visual_style == DoctorVisualStyle::Minimal;
    let is_compact = state.visual_style == DoctorVisualStyle::Compact;
    let is_flutter = state.visual_style == DoctorVisualStyle::Flutter;

    let hero_h: u16 = if is_compact {
        4
    } else if is_minimal {
        5
    } else {
        7
    };
    let detail_h: u16 = if state.show_detail_drawer { 6 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(hero_h),   // 0: Hero Health Score Card
            Constraint::Length(2),        // 1: Filter tabs strip
            Constraint::Min(6),           // 2: Diagnostics table
            Constraint::Length(detail_h), // 3: Live detail drawer (toggleable)
            Constraint::Length(1),        // 4: Action hotkeys footer
        ])
        .split(area);

    let s = &report.score;

    // ── 0. Animated Hero Health Score Card ───────────────────────────────────
    let ease = state.reveal_progress();
    let display_score = ((s.overall as f64) * ease).round().clamp(0.0, 100.0) as u8;
    let s_col = score_color(display_score, theme);

    let mut hero_block = Block::default()
        .borders(if is_minimal {
            Borders::BOTTOM
        } else {
            Borders::ALL
        })
        .border_style(Style::default().fg(if is_cyber {
            theme.info
        } else if is_flutter {
            s_col
        } else {
            theme.border
        }))
        .style(Style::default().bg(if is_minimal {
            theme.bg_primary
        } else {
            theme.bg_elevated
        }));
    if is_ascii {
        hero_block = hero_block.border_set(border::Set {
            top_left: "+",
            top_right: "+",
            bottom_left: "+",
            bottom_right: "+",
            vertical_left: "|",
            vertical_right: "|",
            horizontal_top: "-",
            horizontal_bottom: "-",
        });
    } else if is_cyber {
        hero_block = hero_block.border_type(BorderType::Double);
    } else if is_minimal {
        hero_block = hero_block.border_type(BorderType::Plain);
    } else {
        hero_block = hero_block.border_type(BorderType::Rounded);
    }
    let hero_inner = hero_block.inner(chunks[0]);
    frame.render_widget(hero_block, chunks[0]);

    let bar_w = (hero_inner.width as usize).saturating_sub(26).clamp(16, 48);
    let filled = ((display_score as usize) * bar_w) / 100;
    let empty = bar_w.saturating_sub(filled);
    let (bar_fill, bar_empty) = match state.visual_style {
        DoctorVisualStyle::Ascii => ("=", "-"),
        DoctorVisualStyle::Cyber => ("▰", "▱"),
        DoctorVisualStyle::Minimal => ("─", "·"),
        DoctorVisualStyle::Flutter => ("█", "░"),
        _ => ("━", "─"),
    };

    let delta_span = if state.show_sparkline {
        match s.delta_from_previous {
            Some(d) if d > 0 => Span::styled(
                format!(" +{d}"),
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Some(d) if d < 0 => Span::styled(
                format!(" {d}"),
                Style::default()
                    .fg(theme.destructive)
                    .add_modifier(Modifier::BOLD),
            ),
            Some(_) => Span::styled(
                if is_ascii { " =0" } else { " ±0" },
                Style::default().fg(theme.muted),
            ),
            None => Span::raw(""),
        }
    } else {
        Span::raw("")
    };

    let spark_span = if state.show_sparkline && !is_ascii && s.trend_history.len() > 1 {
        Span::styled(
            format!("   Trend {}", render_sparkline(&s.trend_history)),
            Style::default().fg(theme.info),
        )
    } else {
        Span::raw("")
    };

    let face_tag = match state.visual_style {
        DoctorVisualStyle::Kaomoji => {
            if display_score >= 75 {
                " (•‿•) ".to_string()
            } else if display_score >= 50 {
                " (・_・) ".to_string()
            } else {
                " (×_×) ".to_string()
            }
        }
        DoctorVisualStyle::Emoji => {
            if display_score >= 75 {
                " 💚 ".to_string()
            } else if display_score >= 50 {
                " 🟡 ".to_string()
            } else {
                " 🔴 ".to_string()
            }
        }
        DoctorVisualStyle::NerdFont => {
            let ic = if display_score >= 75 {
                nf(NerdIcon::Check)
            } else if display_score >= 50 {
                nf(NerdIcon::Warning)
            } else {
                nf(NerdIcon::Error)
            };
            format!(" {ic} ")
        }
        DoctorVisualStyle::Cyber => " ◈ ".to_string(),
        _ => " ".to_string(),
    };

    let pill_bg = theme.bg_primary;
    let score_spans = if state.visual_style == DoctorVisualStyle::Posh {
        vec![
            Span::raw(" "),
            Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
            Span::styled(
                format!(" {display_score}/100 ({}) ", s.grade),
                Style::default()
                    .fg(s_col)
                    .bg(pill_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("\u{e0b4}", Style::default().fg(pill_bg)),
            delta_span,
            Span::raw(" "),
            Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
            Span::styled(
                format!(" {} ", s.label),
                Style::default()
                    .fg(s_col)
                    .bg(pill_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("\u{e0b4}", Style::default().fg(pill_bg)),
            spark_span,
        ]
    } else {
        vec![
            Span::styled(
                format!("{face_tag}{display_score:>3}"),
                Style::default()
                    .fg(s_col)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
            Span::styled(
                format!("/100 ({})", s.grade),
                Style::default().fg(s_col).add_modifier(Modifier::BOLD),
            ),
            delta_span,
            Span::styled("   HEALTH SCORE  ", Style::default().fg(theme.muted)),
            Span::styled(
                s.label.clone(),
                Style::default().fg(s_col).add_modifier(Modifier::BOLD),
            ),
            spark_span,
        ]
    };

    let sep_dot = if is_ascii { " | " } else { " · " };
    let mut metrics_spans = vec![
        Span::raw("  "),
        Span::styled(
            format!("{} crit", s.criticals),
            Style::default()
                .fg(if s.criticals > 0 {
                    theme.destructive
                } else {
                    theme.muted
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(sep_dot, Style::default().fg(theme.border)),
        Span::styled(
            format!("{} err", s.errors),
            Style::default()
                .fg(if s.errors > 0 {
                    theme.destructive
                } else {
                    theme.muted
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(sep_dot, Style::default().fg(theme.border)),
        Span::styled(
            format!("{} warn", s.warnings),
            Style::default().fg(if s.warnings > 0 {
                theme.warning
            } else {
                theme.muted
            }),
        ),
        Span::styled(sep_dot, Style::default().fg(theme.border)),
        Span::styled(
            format!("{} dead", s.dead_code),
            Style::default().fg(if s.dead_code > 0 {
                theme.info
            } else {
                theme.muted
            }),
        ),
    ];
    if state.show_subscores {
        metrics_spans.push(Span::styled("   |  ", Style::default().fg(theme.border)));
        metrics_spans.push(Span::styled(
            format!(
                "Correct {}{sep_dot}Sec {}{sep_dot}Perf {}{sep_dot}Arch {}{sep_dot}Maint {}",
                s.correctness, s.security, s.performance, s.architecture, s.maintainability
            ),
            Style::default().fg(theme.muted),
        ));
    }

    let stacks_lbl = match state.visual_style {
        DoctorVisualStyle::NerdFont => format!("  {} Stacks: ", nf(NerdIcon::Chip)),
        DoctorVisualStyle::Emoji => "  📦 Stacks: ".to_string(),
        _ => "  Stacks: ".to_string(),
    };

    let mut hero_lines = vec![
        Line::from(score_spans),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(bar_fill.repeat(filled), Style::default().fg(s_col)),
            Span::styled(bar_empty.repeat(empty), Style::default().fg(theme.border)),
            Span::styled(
                format!(
                    "  {} files{sep_dot}{}ms",
                    report.profile.file_count, report.duration_ms
                ),
                Style::default().fg(theme.muted),
            ),
        ]),
    ];
    if !is_compact {
        hero_lines.push(Line::from(metrics_spans));
        hero_lines.push(Line::from(vec![
            Span::styled(stacks_lbl, Style::default().fg(theme.muted)),
            Span::styled(
                report.profile.stack_summary(),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ", Style::default()),
            Span::styled(
                state
                    .status_banner
                    .as_deref()
                    .or(report.neededfix_path.as_deref())
                    .unwrap_or(".minicode/minidoctor/NEEDEDFIX.md"),
                Style::default().fg(theme.success),
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(hero_lines), hero_inner);

    // ── 1. Filter Tabs Bar ───────────────────────────────────────────────────
    let mut tab_spans = vec![Span::raw(" ")];
    for (idx, tab) in DoctorFilterTab::ALL.iter().enumerate() {
        let count = report.diagnostics.iter().filter(|d| tab.matches(d)).count();
        let is_active = *tab == state.active_tab;
        let label = format!(" [{}] {} ({}) ", idx + 1, tab.label(), count);
        tab_spans.push(Span::styled(
            label,
            if is_active {
                Style::default()
                    .fg(theme.bg_primary)
                    .bg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ));
        tab_spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), chunks[1]);

    // ── 2. Diagnostics List ──────────────────────────────────────────────────
    let filtered = state.filtered_diagnostics();
    let mut list_block = Block::default()
        .title(format!(
            " Diagnostics ({}) {} {} ",
            filtered.len(),
            if is_ascii { "-" } else { "—" },
            state.active_tab.label()
        ))
        .borders(if is_minimal {
            Borders::TOP
        } else {
            Borders::ALL
        })
        .border_style(Style::default().fg(if is_cyber { theme.info } else { theme.border }))
        .style(Style::default().bg(theme.bg_primary));
    if is_ascii {
        list_block = list_block.border_set(border::Set {
            top_left: "+",
            top_right: "+",
            bottom_left: "+",
            bottom_right: "+",
            vertical_left: "|",
            vertical_right: "|",
            horizontal_top: "-",
            horizontal_bottom: "-",
        });
    } else if is_cyber {
        list_block = list_block.border_type(BorderType::Double);
    } else if is_minimal {
        list_block = list_block.border_type(BorderType::Plain);
    } else {
        list_block = list_block.border_type(BorderType::Rounded);
    }
    let list_inner = list_block.inner(chunks[2]);
    frame.render_widget(list_block, chunks[2]);

    let visible_rows = (list_inner.height as usize).max(1);
    let sel_clamped = state.selected_index.min(filtered.len().saturating_sub(1));
    let scroll = compute_scroll_offset(sel_clamped, visible_rows);

    let mut list_lines: Vec<Line<'static>> = Vec::with_capacity(visible_rows);
    if filtered.is_empty() {
        list_lines.push(Line::from(""));
        list_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                if is_ascii {
                    "[OK] Zero findings in this category!"
                } else {
                    "✓ Zero findings in this category — codebase looks clean!"
                },
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        let total_w = list_inner.width as usize;
        for (rel_idx, diag) in filtered.iter().skip(scroll).take(visible_rows).enumerate() {
            let abs_idx = scroll + rel_idx;
            let is_sel = abs_idx == sel_clamped;
            let cursor = if is_sel {
                if is_ascii {
                    "> "
                } else {
                    "❯ "
                }
            } else {
                "  "
            };

            let (badge_txt, badge_style) = if diag.category == DiagnosticCategory::DeadCode {
                (
                    " DEAD ",
                    Style::default()
                        .fg(Color::White)
                        .bg(theme.info)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                match diag.severity {
                    Severity::Critical => (
                        " CRIT ",
                        Style::default()
                            .fg(Color::White)
                            .bg(theme.destructive)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Severity::Error => (
                        " ERR  ",
                        Style::default().fg(Color::White).bg(theme.destructive),
                    ),
                    Severity::Warning => (
                        " WARN ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Severity::Info => (" INFO ", Style::default().fg(Color::Black).bg(theme.info)),
                }
            };

            let rule_label = if let Some(dk) = diag.dead_code_kind {
                format!("{}{sep_dot}{}", dk.label(), diag.rule)
            } else {
                diag.rule.clone()
            };
            let loc = diag.location();
            let left_fixed = 2 + 6 + 2; // cursor + badge + spacing
            let rule_col_w = 34usize.min(total_w / 3);
            let loc_w = UnicodeWidthStr::width(loc.as_str()).min(36);
            let msg_budget = total_w.saturating_sub(left_fixed + rule_col_w + loc_w + 5);

            let trunc = |s: &str, max: usize| -> String {
                if UnicodeWidthStr::width(s) <= max {
                    s.to_string()
                } else if max > 1 {
                    let mut out = String::new();
                    let mut w = 0;
                    for ch in s.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                        if w + cw + 1 > max {
                            break;
                        }
                        out.push(ch);
                        w += cw;
                    }
                    out.push(if is_ascii { '~' } else { '…' });
                    out
                } else {
                    String::new()
                }
            };

            let row_bg = if is_sel {
                theme.bg_elevated
            } else {
                theme.bg_primary
            };
            let rule_disp = trunc(&rule_label, rule_col_w);
            let msg_disp = trunc(&diag.message, msg_budget);
            let loc_disp = trunc(&loc, loc_w);
            let pad_rule = rule_col_w.saturating_sub(UnicodeWidthStr::width(rule_disp.as_str()));
            let pad_msg = msg_budget.saturating_sub(UnicodeWidthStr::width(msg_disp.as_str()));

            list_lines.push(Line::from(vec![
                Span::styled(
                    cursor,
                    Style::default()
                        .fg(theme.brand_accent)
                        .bg(row_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(badge_txt, badge_style),
                Span::styled("  ", Style::default().bg(row_bg)),
                Span::styled(
                    format!("{rule_disp}{}", " ".repeat(pad_rule)),
                    Style::default()
                        .fg(theme.info)
                        .bg(row_bg)
                        .add_modifier(if is_sel {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled("  ", Style::default().bg(row_bg)),
                Span::styled(
                    format!("{msg_disp}{}", " ".repeat(pad_msg)),
                    Style::default()
                        .fg(if is_sel {
                            theme.text_primary
                        } else {
                            theme.muted
                        })
                        .bg(row_bg),
                ),
                Span::styled("  ", Style::default().bg(row_bg)),
                Span::styled(loc_disp, Style::default().fg(theme.muted).bg(row_bg)),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(list_lines), list_inner);

    // ── 3. Live Detail Drawer & Remediation Recipe (toggleable via 21. Show / Hide) ─
    if state.show_detail_drawer && chunks[3].height >= 3 {
        let mut detail_block = Block::default()
            .title(" Diagnostic Detail & Auto-Fix Recipe ")
            .borders(if is_minimal {
                Borders::TOP
            } else {
                Borders::ALL
            })
            .border_style(Style::default().fg(if is_cyber { theme.info } else { theme.border }))
            .style(Style::default().bg(if is_minimal {
                theme.bg_primary
            } else {
                theme.bg_elevated
            }));
        if is_ascii {
            detail_block = detail_block.border_set(border::Set {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                vertical_left: "|",
                vertical_right: "|",
                horizontal_top: "-",
                horizontal_bottom: "-",
            });
        } else if is_cyber {
            detail_block = detail_block.border_type(BorderType::Double);
        } else if is_minimal {
            detail_block = detail_block.border_type(BorderType::Plain);
        } else {
            detail_block = detail_block.border_type(BorderType::Rounded);
        }
        let detail_inner = detail_block.inner(chunks[3]);
        frame.render_widget(detail_block, chunks[3]);

        let detail_lines = if let Some(sel) = filtered.get(sel_clamped) {
            let why = explain_rule(&sel.rule)
                .map(|m| m.why)
                .unwrap_or("Detected by MiniDoctor static analysis.");
            let mut dl = vec![
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", sel.rule),
                        Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("at {}  ", sel.location()),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(
                        format!("[{}]", sel.category.label()),
                        Style::default().fg(theme.brand_accent),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        " Why: ",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(why, Style::default().fg(theme.text_primary)),
                ]),
                Line::from(vec![
                    Span::styled(
                        " Fix: ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        sel.suggestion.clone(),
                        Style::default().fg(theme.text_primary),
                    ),
                ]),
            ];
            if let Some(ref snip) = sel.snippet {
                dl.push(Line::from(vec![
                    Span::styled(" Code: ", Style::default().fg(theme.muted)),
                    Span::styled(snip.clone(), Style::default().fg(theme.highlight)),
                ]));
            }
            dl
        } else {
            vec![Line::from(Span::styled(
                if is_ascii {
                    " [OK] Workspace passed all checks in this view. Press [d] to toggle Diff/Full or [e] to refresh .minicode/minidoctor/NEEDEDFIX.md."
                } else {
                    " ✓ Workspace passed all checks in this view. Press [d] to toggle Diff/Full or [e] to refresh .minicode/minidoctor/NEEDEDFIX.md."
                },
                Style::default().fg(theme.success),
            ))]
        };
        frame.render_widget(
            Paragraph::new(detail_lines).wrap(Wrap { trim: true }),
            detail_inner,
        );
    }

    // ── 4. Action Footer ─────────────────────────────────────────────────────
    let footer = Line::from(vec![
        Span::styled(
            if is_ascii { " [j/k] " } else { " [↑/↓] " },
            Style::default().fg(theme.brand_accent),
        ),
        Span::styled("Select  ", Style::default().fg(theme.text_primary)),
        Span::styled("[1-5/Tab] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Filter  ", Style::default().fg(theme.text_primary)),
        Span::styled(
            "[Enter] ",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Fix Selected  ", Style::default().fg(theme.text_primary)),
        Span::styled(
            "[f] ",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Fix All Crit/Err  ",
            Style::default().fg(theme.text_primary),
        ),
        Span::styled("[d] ", Style::default().fg(theme.info)),
        Span::styled("Diff/Full  ", Style::default().fg(theme.text_primary)),
        Span::styled("[r] ", Style::default().fg(theme.info)),
        Span::styled("Rescan  ", Style::default().fg(theme.text_primary)),
        Span::styled("[e] ", Style::default().fg(theme.warning)),
        Span::styled("Write .md  ", Style::default().fg(theme.text_primary)),
        Span::styled("[s] ", Style::default().fg(theme.info)),
        Span::styled("Style  ", Style::default().fg(theme.text_primary)),
        Span::styled("[Esc] ", Style::default().fg(theme.muted)),
        Span::styled("Close", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer), chunks[4]);
}

/// Renders a live, theme-colored miniature preview of any [`DoctorVisualStyle`] inside `/theme` → `20. Doctor Styles`.
pub fn render_doctor_style_preview(
    style: DoctorVisualStyle,
    ui: &crate::config::UiConfig,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let border_st = Style::default().fg(theme.border);
    let accent_b = Style::default()
        .fg(theme.brand_accent)
        .add_modifier(Modifier::BOLD);
    let ok_b = Style::default()
        .fg(theme.success)
        .add_modifier(Modifier::BOLD);
    let err_pill = Style::default()
        .fg(Color::White)
        .bg(theme.destructive)
        .add_modifier(Modifier::BOLD);
    let warn_pill = Style::default()
        .fg(Color::Black)
        .bg(theme.warning)
        .add_modifier(Modifier::BOLD);
    let info_b = Style::default().fg(theme.info).add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme.muted);
    let text = Style::default().fg(theme.text_primary);
    let pill_bg = theme.bg_primary;

    let spark_str = if ui.show_doctor_sparkline && style != DoctorVisualStyle::Ascii {
        "  Trend ▃▄▅▆▇█"
    } else if ui.show_doctor_sparkline {
        "  +6"
    } else {
        ""
    };
    let sub_line_str = if ui.show_doctor_subscores {
        if style == DoctorVisualStyle::Ascii {
            " | Correct 94 | Sec 90 | Perf 88"
        } else {
            " · Correct 94 · Sec 90 · Perf 88"
        }
    } else {
        ""
    };

    match style {
        DoctorVisualStyle::Aura => vec![
            Line::from(vec![
                Span::styled("╭─ ", border_st),
                Span::styled("◈ MINIDOCTOR", accent_b),
                Span::styled(" ─────────── ", border_st),
                Span::styled("92/100 (A) Excellent", ok_b),
                Span::styled(spark_str, info_b),
                Span::styled(" ─╮", border_st),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", ok_b),
                Span::styled("────", border_st),
                Span::styled(format!(" 24 files{sub_line_str}"), muted),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled(" CRIT ", err_pill),
                Span::raw(" "),
                Span::styled("fastapi/sync-in-async-route  ", info_b),
                Span::styled("app/routes.py:42", muted),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled(" WARN ", warn_pill),
                Span::raw(" "),
                Span::styled("react/index-as-key           ", info_b),
                Span::styled("src/List.tsx:18", muted),
            ]),
            Line::from(Span::styled(
                "╰─────────────────────────────────────────────────────────────╯",
                border_st,
            )),
        ],
        DoctorVisualStyle::NerdFont => {
            let sp = nf(NerdIcon::Sparkle);
            let chk = nf(NerdIcon::Check);
            let chip = nf(NerdIcon::Chip);
            let err_i = nf(NerdIcon::Error);
            let wrn_i = nf(NerdIcon::Warning);
            vec![
                Line::from(vec![
                    Span::styled("╭─ ", border_st),
                    Span::styled(format!("{sp} MINIDOCTOR"), accent_b),
                    Span::styled(" ───────── ", border_st),
                    Span::styled(format!("{chk} 92/100 (A)"), ok_b),
                    Span::styled(spark_str, info_b),
                    Span::styled(" ─╮", border_st),
                ]),
                Line::from(vec![
                    Span::styled("│ ", border_st),
                    Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", ok_b),
                    Span::styled("────", border_st),
                    Span::styled(format!(" {chip} Rust · React{sub_line_str}"), muted),
                ]),
                Line::from(vec![
                    Span::styled("│ ", border_st),
                    Span::styled(
                        format!("{err_i} CRIT "),
                        Style::default()
                            .fg(theme.destructive)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("fastapi/sync-in-async-route  ", info_b),
                    Span::styled("app/routes.py:42", muted),
                ]),
                Line::from(vec![
                    Span::styled("│ ", border_st),
                    Span::styled(
                        format!("{wrn_i} WARN "),
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("react/index-as-key           ", info_b),
                    Span::styled("src/List.tsx:18", muted),
                ]),
                Line::from(Span::styled(
                    "╰─────────────────────────────────────────────────────────────╯",
                    border_st,
                )),
            ]
        }
        DoctorVisualStyle::Kaomoji => vec![
            Line::from(vec![
                Span::styled("╭─ ", border_st),
                Span::styled("(◉_◉) MINIDOCTOR", accent_b),
                Span::styled(" ───── ", border_st),
                Span::styled("(•‿•) 92/100 Great!", ok_b),
                Span::styled(spark_str, info_b),
                Span::styled(" ─╮", border_st),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", ok_b),
                Span::styled("────", border_st),
                Span::styled(format!(" 24 files{sub_line_str}"), muted),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled(
                    "(×_×) ",
                    Style::default()
                        .fg(theme.destructive)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("fastapi/sync-in-async-route  ", info_b),
                Span::styled("app/routes.py:42", muted),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled(
                    "(°△°) ",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("react/index-as-key           ", info_b),
                Span::styled("src/List.tsx:18", muted),
            ]),
            Line::from(Span::styled(
                "╰─────────────────────────────────────────────────────────────╯",
                border_st,
            )),
        ],
        DoctorVisualStyle::Emoji => vec![
            Line::from(vec![
                Span::styled("╭─ ", border_st),
                Span::styled("🩺 MINIDOCTOR", accent_b),
                Span::styled(" ───────── ", border_st),
                Span::styled("💚 92/100 Excellent", ok_b),
                Span::styled(spark_str, info_b),
                Span::styled(" ─╮", border_st),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", ok_b),
                Span::styled("────", border_st),
                Span::styled(format!(" 📦 Rust · FastAPI{sub_line_str}"), muted),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled("🚨 ", Style::default().fg(theme.destructive)),
                Span::styled("fastapi/sync-in-async-route  ", info_b),
                Span::styled("app/routes.py:42", muted),
            ]),
            Line::from(vec![
                Span::styled("│ ", border_st),
                Span::styled("⚠️  ", Style::default().fg(theme.warning)),
                Span::styled("react/index-as-key           ", info_b),
                Span::styled("src/List.tsx:18", muted),
            ]),
            Line::from(Span::styled(
                "╰─────────────────────────────────────────────────────────────╯",
                border_st,
            )),
        ],
        DoctorVisualStyle::Ascii => vec![
            Line::from(vec![
                Span::styled("+-- ", border_st),
                Span::styled("[MINIDOCTOR]", accent_b),
                Span::styled(" ----------- ", border_st),
                Span::styled("92/100 (A) Great", ok_b),
                Span::styled(spark_str, info_b),
                Span::styled(" --+", border_st),
            ]),
            Line::from(vec![
                Span::styled("| ", border_st),
                Span::styled("==============================", ok_b),
                Span::styled("----", border_st),
                Span::styled(format!(" 24 files{sub_line_str}"), muted),
            ]),
            Line::from(vec![
                Span::styled("| ", border_st),
                Span::styled(
                    "[CRIT] ",
                    Style::default()
                        .fg(theme.destructive)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("fastapi/sync-in-async-route  ", info_b),
                Span::styled("app/routes.py:42", muted),
            ]),
            Line::from(vec![
                Span::styled("| ", border_st),
                Span::styled(
                    "[WARN] ",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("react/index-as-key           ", info_b),
                Span::styled("src/List.tsx:18", muted),
            ]),
            Line::from(Span::styled(
                "+-------------------------------------------------------------+",
                border_st,
            )),
        ],
        DoctorVisualStyle::Posh => vec![
            Line::from(vec![
                Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
                Span::styled(" ❖ MINIDOCTOR ", accent_b.bg(pill_bg)),
                Span::styled("\u{e0b4} ", Style::default().fg(pill_bg)),
                Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
                Span::styled(" 92/100 (A) ", ok_b.bg(pill_bg)),
                Span::styled("\u{e0b4} ", Style::default().fg(pill_bg)),
                Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
                Span::styled(" Excellent ", ok_b.bg(pill_bg)),
                Span::styled("\u{e0b4}", Style::default().fg(pill_bg)),
                Span::styled(spark_str, info_b),
            ]),
            Line::from(vec![
                Span::styled("▎ ", accent_b),
                Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", ok_b),
                Span::styled("────", border_st),
                Span::styled(format!(" 24 files{sub_line_str}"), muted),
            ]),
            Line::from(vec![
                Span::styled("▎ ", accent_b),
                Span::styled(" CRIT ", err_pill),
                Span::raw(" "),
                Span::styled("fastapi/sync-in-async-route  ", info_b),
                Span::styled("app/routes.py:42", muted),
            ]),
            Line::from(vec![
                Span::styled("▎ ", border_st),
                Span::styled(" WARN ", warn_pill),
                Span::raw(" "),
                Span::styled("react/index-as-key           ", info_b),
                Span::styled("src/List.tsx:18", muted),
            ]),
        ],
        DoctorVisualStyle::Flutter => vec![
            Line::from(Span::styled(
                "╭─────────────────────────────────────────────────────────────╮",
                ok_b,
            )),
            Line::from(vec![
                Span::styled("│ ", ok_b),
                Span::styled("92 (A)", ok_b.add_modifier(Modifier::UNDERLINED)),
                Span::styled("  HEALTH SCORE ", muted),
                Span::styled("█████████████████████░░░", ok_b),
                Span::styled(spark_str, info_b),
            ]),
            Line::from(vec![
                Span::styled("│ ", ok_b),
                Span::styled("Great ", ok_b),
                Span::styled(
                    format!(" 1 crit · 1 err · 2 warn · 24 files{sub_line_str}"),
                    text,
                ),
            ]),
            Line::from(Span::styled(
                "╰─────────────────────────────────────────────────────────────╯",
                ok_b,
            )),
            Line::from(vec![
                Span::styled("  → Fix ", Style::default().fg(theme.destructive)),
                Span::styled("fastapi/sync-in-async-route", info_b),
                Span::styled(" (app/routes.py:42)", muted),
            ]),
        ],
        DoctorVisualStyle::Minimal => vec![
            Line::from(vec![
                Span::styled("MINIDOCTOR  ", accent_b),
                Span::styled("92/100 (A) Excellent", ok_b),
                Span::styled(spark_str, info_b),
            ]),
            Line::from(vec![
                Span::styled("──────────────────────────────", ok_b),
                Span::styled("····", border_st),
                Span::styled(format!(" 24 files{sub_line_str}"), muted),
            ]),
            Line::from(vec![
                Span::styled(
                    "crit  ",
                    Style::default()
                        .fg(theme.destructive)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("fastapi/sync-in-async-route   ", text),
                Span::styled("app/routes.py:42", muted),
            ]),
            Line::from(vec![
                Span::styled("warn  ", Style::default().fg(theme.warning)),
                Span::styled("react/index-as-key            ", text),
                Span::styled("src/List.tsx:18", muted),
            ]),
        ],
        DoctorVisualStyle::Cyber => vec![
            Line::from(vec![
                Span::styled("╔═ ", info_b),
                Span::styled("◈ HUD // MINIDOCTOR", accent_b),
                Span::styled(" ═════ ", info_b),
                Span::styled("92/100 [A]", ok_b),
                Span::styled(spark_str, info_b),
                Span::styled(" ═╗", info_b),
            ]),
            Line::from(vec![
                Span::styled("║ ", info_b),
                Span::styled("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰", ok_b),
                Span::styled("▱▱▱▱", border_st),
                Span::styled(format!(" 24 files{sub_line_str}"), muted),
            ]),
            Line::from(vec![
                Span::styled("║ ", info_b),
                Span::styled(" CRIT ", err_pill),
                Span::raw(" "),
                Span::styled("fastapi/sync-in-async-route  ", info_b),
                Span::styled("app/routes.py:42", muted),
            ]),
            Line::from(Span::styled(
                "╚═════════════════════════════════════════════════════════════╝",
                info_b,
            )),
        ],
        DoctorVisualStyle::Compact => vec![
            Line::from(vec![
                Span::styled("▪ MINIDOCTOR ", accent_b),
                Span::styled("92/100 (A)", ok_b),
                Span::styled(" ━━━━━━━━━━── ", ok_b),
                Span::styled("1c 1e 2w · 24f", muted),
                Span::styled(spark_str, info_b),
            ]),
            Line::from(vec![
                Span::styled("❯ ", accent_b),
                Span::styled(" CRIT ", err_pill),
                Span::raw(" "),
                Span::styled("fastapi/sync-in-async-route ", info_b),
                Span::styled("app/routes.py:42", muted),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(" WARN ", warn_pill),
                Span::raw(" "),
                Span::styled("react/index-as-key          ", info_b),
                Span::styled("src/List.tsx:18", muted),
            ]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minidoctor::{DetectedStack, ProjectProfile, ScoreBreakdown};
    use ratatui::{backend::TestBackend, Terminal};

    fn sample_report() -> DoctorReport {
        DoctorReport {
            version: "0.1.0".to_string(),
            generated_at: "2026-11-23 11:30:00 UTC".to_string(),
            workspace: "/tmp/demo".to_string(),
            scope_label: "full workspace".to_string(),
            git_head: Some("abc1234".to_string()),
            duration_ms: 42,
            profile: ProjectProfile {
                stacks: vec![DetectedStack::Rust, DetectedStack::FastApi],
                file_count: 14,
                total_loc: 920,
                language_counts: Default::default(),
                manifest_notes: vec!["Cargo.toml".to_string()],
            },
            score: ScoreBreakdown {
                overall: 68,
                grade: "C".to_string(),
                label: "Needs work".to_string(),
                correctness: 75,
                security: 60,
                performance: 80,
                architecture: 95,
                maintainability: 92,
                criticals: 1,
                errors: 1,
                warnings: 1,
                infos: 0,
                dead_code: 0,
                security_issues: 1,
                threshold: 75,
                delta_from_previous: Some(6),
                trend_history: vec![55, 62, 68],
            },
            diagnostics: vec![
                Diagnostic {
                    rule: "sec/hardcoded-secret".to_string(),
                    severity: Severity::Critical,
                    category: DiagnosticCategory::Security,
                    dead_code_kind: None,
                    file: "app/config.py".to_string(),
                    line: 12,
                    column: 1,
                    message: "Hardcoded secret assigned to `SECRET_KEY`".to_string(),
                    suggestion: "Load SECRET_KEY from environment.".to_string(),
                    snippet: Some("SECRET_KEY = \"super_secret_12345\"".to_string()),
                },
                Diagnostic {
                    rule: "rust/unwrap-in-non-test".to_string(),
                    severity: Severity::Error,
                    category: DiagnosticCategory::Correctness,
                    dead_code_kind: None,
                    file: "src/lib.rs".to_string(),
                    line: 33,
                    column: 10,
                    message: "`.unwrap()` in non-test code can panic".to_string(),
                    suggestion: "Propagate with `?`.".to_string(),
                    snippet: Some("let x = opt.unwrap();".to_string()),
                },
            ],
            neededfix_path: Some(".minicode/minidoctor/NEEDEDFIX.md".to_string()),
        }
    }

    #[test]
    fn test_render_doctor_modal_all_styles_and_scanning_state() {
        let theme = Theme::default();
        let backend = TestBackend::new(120, 38);
        let mut terminal = Terminal::new(backend).unwrap();

        // 1. Scanning animation view
        let mut scanning_state = DoctorModalState::new(Path::new("."), ScanScope::Full, false);
        scanning_state.scanning = true;
        assert!(scanning_state.has_live_animation());
        terminal
            .draw(|f| {
                render_doctor_modal(
                    f,
                    f.area(),
                    &scanning_state,
                    &theme,
                    SpinnerStyle::NerdOrbit,
                );
            })
            .unwrap();

        // 2. Completed dashboard across all 6 visual styles
        let mut state = DoctorModalState::from_report_for_test(Path::new("."), sample_report());
        assert!(!state.has_live_animation());
        assert!(state
            .build_fix_all_prompt()
            .unwrap()
            .contains("NEEDEDFIX.md"));
        assert!(state
            .build_fix_selected_prompt()
            .unwrap()
            .contains("sec/hardcoded-secret"));

        for style in [
            DoctorVisualStyle::Aura,
            DoctorVisualStyle::NerdFont,
            DoctorVisualStyle::Kaomoji,
            DoctorVisualStyle::Emoji,
            DoctorVisualStyle::Ascii,
            DoctorVisualStyle::Posh,
            DoctorVisualStyle::Flutter,
            DoctorVisualStyle::Minimal,
            DoctorVisualStyle::Cyber,
            DoctorVisualStyle::Compact,
        ] {
            state.visual_style = style;
            terminal
                .draw(|f| {
                    render_doctor_modal(f, f.area(), &state, &theme, SpinnerStyle::default());
                })
                .unwrap();

            let preview =
                render_doctor_style_preview(style, &crate::config::UiConfig::default(), &theme);
            assert!(
                preview.len() >= 3,
                "style {:?} preview should have at least 3 rows",
                style
            );
            let joined: String = preview
                .iter()
                .map(|l| {
                    l.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            if style == DoctorVisualStyle::Ascii {
                assert!(
                    joined.is_ascii(),
                    "ASCII doctor preview must be pure 7-bit ASCII: {joined}"
                );
            }
            if style == DoctorVisualStyle::NerdFont {
                assert!(
                    !joined
                        .chars()
                        .any(|c| ('\u{EA60}'..='\u{F8FF}').contains(&c)),
                    "NerdFont doctor preview in auto/fallback test mode must emit zero PUA glyphs: {joined}"
                );
            }
        }
    }
}
