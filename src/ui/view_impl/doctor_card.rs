//! Inline Streaming Timeline MiniDoctor Card & Live Working Animation renderer.
//!
//! When the agent invokes `project_doctor` (or `minidoctor`/`doctor`), this renderer replaces the
//! generic tool output block in the streaming timeline with a themed, animated MiniDoctor card:
//! - **Running state (`is_running == true`):** animated working wave / ECG / radar / orbit /
//!   kaomoji / spectrum / ASCII scan bar + 5-phase live pipeline strip.
//! - **Completed state (`is_running == false`):** styled 0–100 Health Score gauge, grade & delta,
//!   severity counters (`crit`/`err`/`warn`/`dead`), optional dimension sub-scores, detected stacks,
//!   `.minicode/minidoctor/NEEDEDFIX.md` chip, and optional top priority findings preview.

use crate::config::{DoctorAnimStyle, DoctorInlineConfig, DoctorStyle, UiConfig};
use crate::ui::icons::{nf, NerdIcon};
use crate::ui::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Returns `true` when `raw_name` refers to the MiniDoctor codebase diagnostic tool.
#[must_use]
pub fn is_doctor_tool(raw_name: &str) -> bool {
    matches!(
        raw_name.trim().to_ascii_lowercase().as_str(),
        "project_doctor" | "minidoctor" | "doctor"
    )
}

/// Parsed structured finding from `project_doctor` compact output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineDoctorFinding {
    pub badge: String,
    pub rule: String,
    pub location: String,
    pub message: String,
}

/// Parsed structured payload extracted from `minidoctor::format_agent_compact_summary`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDoctorOutput {
    pub score: u8,
    pub grade: String,
    pub label: String,
    pub delta: Option<i16>,
    pub scope: String,
    pub stacks: String,
    pub files: usize,
    pub duration_ms: u64,
    pub criticals: usize,
    pub errors: usize,
    pub warnings: usize,
    pub dead_code: usize,
    pub subscores: Option<(u8, u8, u8, u8, u8)>,
    pub neededfix_path: Option<String>,
    pub findings: Vec<InlineDoctorFinding>,
    pub more_count: usize,
}

impl ParsedDoctorOutput {
    /// Parses `minidoctor::format_agent_compact_summary` output.
    /// Returns `None` for non-scan outputs (e.g., `explain_rule` responses or errors).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let mut lines = raw.lines();
        let header = lines.next()?.trim();
        let after_prefix = header.strip_prefix("MiniDoctor Health Score:")?.trim();

        // Parse `<score>/100 (<grade>, <label>)[ (+<delta>)] · Scope: ... · Stacks: ... · Files: N (Mms)`
        let (score_part, rest_segments) = match after_prefix.split_once(" · ") {
            Some((a, b)) => (a.trim(), b),
            None => (after_prefix, ""),
        };
        let (score_str, after_hundred) = score_part.split_once("/100")?;
        let score: u8 = score_str.trim().parse().ok()?;

        let mut grade = String::from("?");
        let mut label = String::new();
        let mut delta: Option<i16> = None;

        if let Some(open) = after_hundred.find('(') {
            if let Some(close_rel) = after_hundred[open + 1..].find(')') {
                let inside = &after_hundred[open + 1..open + 1 + close_rel];
                if let Some((g, l)) = inside.split_once(',') {
                    grade = g.trim().to_string();
                    label = l.trim().to_string();
                } else {
                    grade = inside.trim().to_string();
                }
                let tail = &after_hundred[open + 1 + close_rel + 1..];
                if let Some(d_open) = tail.find('(') {
                    if let Some(d_close_rel) = tail[d_open + 1..].find(')') {
                        let d_str = tail[d_open + 1..d_open + 1 + d_close_rel].trim();
                        delta = d_str.parse::<i16>().ok();
                    }
                }
            }
        }

        let mut scope = String::from("full workspace");
        let mut stacks = String::new();
        let mut files: usize = 0;
        let mut duration_ms: u64 = 0;

        // Note: `Stacks:` itself joins multiple stacks with ` · ` (e.g. `Stacks: Rust · React · Files: 14 (22ms)`),
        // so extract sections by their named markers instead of splitting blindly on ` · `.
        if let Some(after_scope) = rest_segments
            .strip_prefix("Scope:")
            .or_else(|| rest_segments.split("Scope:").nth(1))
        {
            if let Some((sc, after_stacks_marker)) = after_scope.split_once(" · Stacks:") {
                scope = sc.trim().to_string();
                if let Some((st, after_files_marker)) = after_stacks_marker.split_once(" · Files:")
                {
                    stacks = st.trim().to_string();
                    let v = after_files_marker.trim();
                    let first_num = v.split_whitespace().next().unwrap_or("0");
                    files = first_num.parse().unwrap_or(0);
                    if let Some(ms_open) = v.find('(') {
                        if let Some(ms_end) = v[ms_open + 1..].find("ms)") {
                            duration_ms = v[ms_open + 1..ms_open + 1 + ms_end]
                                .trim()
                                .parse()
                                .unwrap_or(0);
                        }
                    }
                } else {
                    stacks = after_stacks_marker.trim().to_string();
                }
            } else {
                scope = after_scope.trim().to_string();
            }
        }

        let mut criticals = 0usize;
        let mut errors = 0usize;
        let mut warnings = 0usize;
        let mut dead_code = 0usize;
        let mut subscores: Option<(u8, u8, u8, u8, u8)> = None;
        let mut neededfix_path: Option<String> = None;
        let mut findings = Vec::new();
        let mut more_count = 0usize;

        for line in lines {
            let trimmed = line.trim();
            if let Some(after_counts) = trimmed.strip_prefix("Counts:") {
                let (counts_seg, sub_seg) = match after_counts.split_once('|') {
                    Some((c, s)) => (c.trim(), Some(s.trim())),
                    None => (after_counts.trim(), None),
                };
                for part in counts_seg.split(',') {
                    let p = part.trim();
                    let num = p
                        .split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<usize>().ok())
                        .unwrap_or(0);
                    if p.contains("critical") {
                        criticals = num;
                    } else if p.contains("error") {
                        errors = num;
                    } else if p.contains("warning") {
                        warnings = num;
                    } else if p.contains("dead") {
                        dead_code = num;
                    }
                }
                if let Some(sub) = sub_seg {
                    let s_body = sub.strip_prefix("Sub-scores:").unwrap_or(sub).trim();
                    let mut vals = [100u8; 5];
                    let mut matched = 0usize;
                    for item in s_body.split(',') {
                        let it = item.trim();
                        let val = it
                            .split_whitespace()
                            .last()
                            .and_then(|n| n.parse::<u8>().ok());
                        if let Some(v) = val {
                            if it.starts_with("Correct") {
                                vals[0] = v;
                                matched += 1;
                            } else if it.starts_with("Sec") {
                                vals[1] = v;
                                matched += 1;
                            } else if it.starts_with("Perf") {
                                vals[2] = v;
                                matched += 1;
                            } else if it.starts_with("Arch") {
                                vals[3] = v;
                                matched += 1;
                            } else if it.starts_with("Maint") {
                                vals[4] = v;
                                matched += 1;
                            }
                        }
                    }
                    if matched == 5 {
                        subscores = Some((vals[0], vals[1], vals[2], vals[3], vals[4]));
                    }
                }
            } else if let Some(path_raw) = trimmed.strip_prefix("Workspace Checklist:") {
                let cleaned = path_raw.trim().trim_matches('`').to_string();
                if !cleaned.is_empty() {
                    neededfix_path = Some(cleaned);
                }
            } else if let Some(rest) = trimmed.strip_prefix("- [") {
                if let Some((badge, after_badge)) = rest.split_once(']') {
                    let badge = badge.trim().to_string();
                    let mut cursor = after_badge.trim();
                    // Optional secondary `[DEAD CODE]` tag
                    if cursor.starts_with('[') {
                        if let Some((_, after_sub)) = cursor.split_once(']') {
                            cursor = after_sub.trim();
                        }
                    }
                    let (rule, after_rule) = if let Some(r_start) = cursor.strip_prefix('`') {
                        if let Some((r, rem)) = r_start.split_once('`') {
                            (r.trim().to_string(), rem.trim())
                        } else {
                            (String::new(), cursor)
                        }
                    } else {
                        (String::new(), cursor)
                    };
                    let (loc_and_msg, _) = match after_rule.split_once(" | Fix:") {
                        Some((a, b)) => (a.trim(), Some(b.trim())),
                        None => (after_rule, None),
                    };
                    let (location, message) = match loc_and_msg.split_once(" — ") {
                        Some((l, m)) => (l.trim().to_string(), m.trim().to_string()),
                        None => (String::new(), loc_and_msg.to_string()),
                    };
                    findings.push(InlineDoctorFinding {
                        badge,
                        rule,
                        location,
                        message,
                    });
                }
            } else if let Some(more_rest) = trimmed.strip_prefix("... and ") {
                more_count = more_rest
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0);
            }
        }

        Some(Self {
            score,
            grade,
            label,
            delta,
            scope,
            stacks,
            files,
            duration_ms,
            criticals,
            errors,
            warnings,
            dead_code,
            subscores,
            neededfix_path,
            findings,
            more_count,
        })
    }
}

fn score_color(score: u8, theme: &Theme) -> Color {
    if score >= 85 {
        theme.success
    } else if score >= 65 {
        theme.warning
    } else {
        theme.destructive
    }
}

fn truncate_cols(s: &str, max_cols: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_cols {
        return s.to_string();
    }
    if max_cols <= 1 {
        return String::new();
    }
    let mut out = String::new();
    let mut cols = 0usize;
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(1);
        if cols + w + 1 > max_cols {
            break;
        }
        out.push(ch);
        cols += w;
    }
    out.push('…');
    out
}

fn truncate_cols_ascii(s: &str, max_cols: usize, is_ascii: bool) -> String {
    if !is_ascii {
        return truncate_cols(s, max_cols);
    }
    if s.len() <= max_cols {
        return s.to_string();
    }
    if max_cols <= 3 {
        return s.chars().take(max_cols).collect();
    }
    let mut out: String = s.chars().take(max_cols.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

/// Builds the live animated indicator glyph, animated scan bar spans, and active pipeline stage
/// label for a given [`DoctorAnimStyle`] and wall-clock `working_millis`.
pub fn render_doctor_working_anim(
    anim: DoctorAnimStyle,
    force_ascii: bool,
    working_millis: u64,
    bar_w: usize,
    theme: &Theme,
) -> (Span<'static>, Vec<Span<'static>>, &'static str) {
    let eff_anim = if force_ascii {
        DoctorAnimStyle::Ascii
    } else {
        anim
    };
    let bar_w = bar_w.clamp(12, 34);
    let tick = (working_millis / 90) as usize;
    let phase_idx = ((working_millis / 900) as usize) % 5;
    let stage_label = match phase_idx {
        0 => "Detecting stacks",
        1 => "AST rules & hooks",
        2 => "Dead code & duplicates",
        3 => "Security & secrets",
        _ => "Scoring & NEEDEDFIX.md",
    };

    let mut bar_spans = Vec::with_capacity(bar_w);

    match eff_anim {
        DoctorAnimStyle::Pulse => {
            // Live scrolling ECG heartbeat trace
            const ECG_PATTERN: &[&str] = &[
                "─", "─", "─", "╱", "╲", "─", "─", "╱", "╲", "─", "─", "─", "─", "─",
            ];
            let head = tick % bar_w;
            let even = tick.is_multiple_of(2);
            let lead = Span::styled(
                if even { "♥" } else { "♡" },
                Style::default()
                    .fg(if even { theme.brand_accent } else { theme.info })
                    .add_modifier(Modifier::BOLD),
            );
            for i in 0..bar_w {
                let pat_idx =
                    (i + ECG_PATTERN.len() - (tick % ECG_PATTERN.len())) % ECG_PATTERN.len();
                let ch = ECG_PATTERN[pat_idx];
                let dist = (bar_w + head - i) % bar_w;
                let col = if dist == 0 {
                    theme.highlight
                } else if dist <= 3 {
                    theme.brand_accent
                } else if ch != "─" {
                    theme.info
                } else {
                    theme.border
                };
                bar_spans.push(Span::styled(ch, Style::default().fg(col)));
            }
            (lead, bar_spans, stage_label)
        }
        DoctorAnimStyle::Radar => {
            // Back-and-forth Kitt scanner beam
            let span = (bar_w.saturating_sub(1)).max(1);
            let cycle = span * 2;
            let pos = tick % cycle;
            let head = if pos <= span { pos } else { cycle - pos };
            let lead = Span::styled(
                "◈",
                Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
            );
            for i in 0..bar_w {
                let d = head.abs_diff(i);
                let (ch, col) = match d {
                    0 => ("█", theme.highlight),
                    1 => ("▓", theme.brand_accent),
                    2 => ("▒", theme.info),
                    _ => ("░", theme.border),
                };
                bar_spans.push(Span::styled(ch, Style::default().fg(col)));
            }
            (lead, bar_spans, stage_label)
        }
        DoctorAnimStyle::Orbit => {
            const ORBIT: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let lead = Span::styled(
                ORBIT[tick % ORBIT.len()],
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            );
            let filled = (tick % (bar_w + 1)).max(2).min(bar_w);
            for i in 0..bar_w {
                let (ch, col) = if i + 1 == filled {
                    ("╸", theme.highlight)
                } else if i < filled {
                    ("━", theme.brand_accent)
                } else {
                    ("─", theme.border)
                };
                bar_spans.push(Span::styled(ch, Style::default().fg(col)));
            }
            (lead, bar_spans, stage_label)
        }
        DoctorAnimStyle::Kaomoji => {
            const FACES: &[&str] = &["(◉_◉)", "(•̀ᴗ•́)", "(¬‿¬)", "(✧_✧)"];
            let face = FACES[(tick / 3) % FACES.len()];
            let lead = Span::styled(
                face,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            );
            let dot = tick % bar_w;
            for i in 0..bar_w {
                let (ch, col) = if i == dot {
                    ("✦", theme.highlight)
                } else if i.abs_diff(dot) == 1 {
                    ("•", theme.info)
                } else {
                    ("·", theme.border)
                };
                bar_spans.push(Span::styled(ch, Style::default().fg(col)));
            }
            (lead, bar_spans, stage_label)
        }
        DoctorAnimStyle::Blocks => {
            const BARS: &[&str] = &[
                "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
            ];
            let lead = Span::styled(
                "▮",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            );
            for i in 0..bar_w {
                let idx = (i + tick) % BARS.len();
                let col = if (6..=8).contains(&idx) {
                    theme.brand_accent
                } else if (4..=10).contains(&idx) {
                    theme.info
                } else {
                    theme.border
                };
                bar_spans.push(Span::styled(BARS[idx], Style::default().fg(col)));
            }
            (lead, bar_spans, stage_label)
        }
        DoctorAnimStyle::Ascii => {
            const ASCII_SPIN: &[&str] = &["-", "\\", "|", "/"];
            let lead = Span::styled(
                format!("[{}]", ASCII_SPIN[tick % ASCII_SPIN.len()]),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            );
            let head = tick % bar_w;
            bar_spans.push(Span::styled("[", Style::default().fg(theme.border)));
            for i in 0..bar_w.saturating_sub(2) {
                let (ch, col) = if i == head % bar_w.saturating_sub(2).max(1) {
                    (">", theme.highlight)
                } else if i < head % bar_w.saturating_sub(2).max(1) {
                    ("=", theme.brand_accent)
                } else {
                    ("-", theme.border)
                };
                bar_spans.push(Span::styled(ch, Style::default().fg(col)));
            }
            bar_spans.push(Span::styled("]", Style::default().fg(theme.border)));
            (lead, bar_spans, stage_label)
        }
    }
}

fn doctor_style_badge(style: DoctorStyle) -> String {
    match style {
        DoctorStyle::Aura => "◈ MINIDOCTOR".to_string(),
        DoctorStyle::NerdFont => format!("{} MINIDOCTOR", nf(NerdIcon::Sparkle)),
        DoctorStyle::Kaomoji => "(◉_◉) MINIDOCTOR".to_string(),
        DoctorStyle::Emoji => "🩺 MINIDOCTOR".to_string(),
        DoctorStyle::Ascii => "[MINIDOCTOR]".to_string(),
        DoctorStyle::Posh => "❖ MINIDOCTOR".to_string(),
        DoctorStyle::Flutter => "◆ MINIDOCTOR".to_string(),
        DoctorStyle::Minimal => "MINIDOCTOR".to_string(),
        DoctorStyle::Cyber => "◈ HUD // MINIDOCTOR".to_string(),
        DoctorStyle::Compact => "▪ MINIDOCTOR".to_string(),
    }
}

fn border_chars(
    style: DoctorStyle,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match style {
        DoctorStyle::Ascii => ("+", "+", "+", "+", "-", "|"),
        DoctorStyle::Cyber => ("╔", "╗", "╚", "╝", "═", "║"),
        DoctorStyle::Minimal => ("─", "─", "─", "─", "─", " "),
        DoctorStyle::Posh => ("▎", "", "▎", "", "─", "▎"),
        DoctorStyle::Compact => ("▸", "", "", "", "─", " "),
        _ => ("╭", "╮", "╰", "╯", "─", "│"),
    }
}

fn gauge_chars(style: DoctorStyle) -> (&'static str, &'static str) {
    match style {
        DoctorStyle::Ascii => ("=", "-"),
        DoctorStyle::Cyber => ("▰", "▱"),
        DoctorStyle::Minimal => ("─", "·"),
        DoctorStyle::Flutter => ("█", "░"),
        _ => ("━", "─"),
    }
}

/// Renders an inline MiniDoctor card inside the streaming timeline (`ToolStart` / `ToolFinished`).
#[allow(clippy::too_many_arguments)]
pub fn render_inline_doctor_card(
    _raw_name: &str,
    command_or_path: &str,
    output: Option<&str>,
    is_running: bool,
    success: bool,
    duration_ms: Option<u64>,
    area_width: u16,
    cfg: DoctorInlineConfig,
    working_millis: u64,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let style = cfg.style;
    let is_ascii = style == DoctorStyle::Ascii;
    let is_cyber = style == DoctorStyle::Cyber;
    let is_minimal = style == DoctorStyle::Minimal;
    let is_posh = style == DoctorStyle::Posh;
    let is_compact = style == DoctorStyle::Compact;
    let is_flutter = style == DoctorStyle::Flutter;

    let card_w = (area_width.saturating_sub(2) as usize).clamp(40, 108);
    let inner_w = card_w.saturating_sub(4).max(28);
    let (tl, tr, bl, br, h_ch, v_ch) = border_chars(style);
    let badge = doctor_style_badge(style);
    let sep = if is_ascii { " | " } else { " · " };

    let border_col = if !is_running && !success {
        theme.destructive
    } else if is_cyber {
        theme.info
    } else if is_running {
        theme.brand_accent
    } else {
        theme.border
    };
    let border_st = Style::default().fg(border_col);
    let accent_b = Style::default()
        .fg(theme.brand_accent)
        .add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme.muted);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let scope_hint = if command_or_path.trim().is_empty() {
        "full workspace".to_string()
    } else {
        truncate_cols_ascii(command_or_path.trim(), 28, is_ascii)
    };

    // ── RUNNING STATE: Live Animated Working Card ────────────────────────────
    if is_running {
        let bar_w = inner_w.saturating_sub(28).clamp(12, 30);
        let (anim_lead, anim_bar, stage_lbl) =
            render_doctor_working_anim(cfg.anim, is_ascii, working_millis, bar_w, theme);
        let phase_idx = ((working_millis / 900) as usize) % 5;

        if is_compact {
            let mut spans = vec![
                Span::styled(format!(" {badge} "), accent_b),
                anim_lead,
                Span::raw(" "),
            ];
            spans.extend(anim_bar);
            spans.push(Span::styled(
                format!("  {stage_lbl}{sep}{scope_hint}"),
                muted,
            ));
            lines.push(Line::from(spans));
            return lines;
        }

        if is_posh {
            let pill_bg = theme.bg_elevated;
            lines.push(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
                Span::styled(format!(" {badge} "), accent_b.bg(pill_bg)),
                Span::styled("\u{e0b4} ", Style::default().fg(pill_bg)),
                Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
                Span::styled(
                    format!(" Scanning · {scope_hint} "),
                    Style::default()
                        .fg(theme.info)
                        .bg(pill_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("\u{e0b4}", Style::default().fg(pill_bg)),
            ]));
        } else if is_minimal {
            let fill = card_w
                .saturating_sub(UnicodeWidthStr::width(badge.as_str()) + scope_hint.len() + 18)
                .max(3);
            lines.push(Line::from(vec![
                Span::styled(format!(" {badge} "), accent_b),
                Span::styled(
                    format!("scanning ({scope_hint}) "),
                    Style::default().fg(theme.info),
                ),
                Span::styled(h_ch.repeat(fill), border_st),
            ]));
        } else {
            let right_tag = if is_ascii {
                format!(" [scanning: {scope_hint}] ")
            } else {
                format!(" scanning · {scope_hint} ")
            };
            let fixed_w = 7
                + UnicodeWidthStr::width(badge.as_str())
                + UnicodeWidthStr::width(right_tag.as_str());
            let fill = card_w.saturating_sub(fixed_w).max(1);
            lines.push(Line::from(vec![
                Span::styled(format!(" {tl}{h_ch} "), border_st),
                Span::styled(badge, accent_b),
                Span::styled(format!(" {}", h_ch.repeat(fill)), border_st),
                Span::styled(right_tag, Style::default().fg(theme.info)),
                Span::styled(format!("{h_ch}{tr}"), border_st),
            ]));
        }

        // Row 2: Live Animated Scan Wave + Active Stage
        let prefix = if is_minimal {
            Span::raw("   ")
        } else if is_posh {
            Span::styled(" ▎ ", accent_b)
        } else {
            Span::styled(format!(" {v_ch} "), border_st)
        };
        let mut wave_row = vec![prefix.clone(), anim_lead, Span::raw(" ")];
        wave_row.extend(anim_bar);
        wave_row.push(Span::styled(
            format!("  {stage_lbl}..."),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::from(wave_row));

        // Row 3: 5-Stage Pipeline Pills
        let stages = ["Stacks", "AST", "DeadCode", "Security", "Score"];
        let mut stage_row = vec![prefix];
        for (i, st_name) in stages.iter().enumerate() {
            let (ic, col, mods) = if i < phase_idx {
                (
                    if is_ascii { "[ok]" } else { "✓" },
                    theme.success,
                    Modifier::empty(),
                )
            } else if i == phase_idx {
                (
                    if is_ascii { "[>]" } else { "◉" },
                    theme.brand_accent,
                    Modifier::BOLD,
                )
            } else {
                (
                    if is_ascii { "[ ]" } else { "○" },
                    theme.muted,
                    Modifier::empty(),
                )
            };
            if i > 0 {
                stage_row.push(Span::styled(sep, Style::default().fg(theme.border)));
            }
            stage_row.push(Span::styled(
                format!("{ic} {st_name}"),
                Style::default().fg(col).add_modifier(mods),
            ));
        }
        lines.push(Line::from(stage_row));

        if !is_minimal && !is_posh {
            lines.push(Line::from(Span::styled(
                format!(" {bl}{}{br}", h_ch.repeat(card_w.saturating_sub(3).max(10))),
                border_st,
            )));
        }
        return lines;
    }

    // ── COMPLETED STATE ──────────────────────────────────────────────────────
    let raw_out = output.unwrap_or("");
    let Some(parsed) = ParsedDoctorOutput::parse(raw_out) else {
        // Fallback for `explain_rule` or error outputs: clean themed MiniDoctor info box
        let status_txt = if success {
            if is_ascii {
                "[OK]"
            } else {
                "✓ done"
            }
        } else if is_ascii {
            "[ERR]"
        } else {
            "✗ failed"
        };
        let dur_txt = duration_ms
            .filter(|d| *d > 0)
            .map(|d| format!(" {d}ms"))
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(format!(" {tl}{h_ch} "), border_st),
            Span::styled(badge, accent_b),
            Span::styled(
                format!(" {status_txt}{dur_txt} "),
                Style::default().fg(if success {
                    theme.success
                } else {
                    theme.destructive
                }),
            ),
        ]));
        for l in raw_out.lines().take(6) {
            lines.push(Line::from(vec![
                Span::styled(format!(" {v_ch} "), border_st),
                Span::styled(
                    truncate_cols_ascii(l.trim(), inner_w, is_ascii),
                    Style::default().fg(theme.text_primary),
                ),
            ]));
        }
        if !is_minimal && !is_posh && !is_compact {
            lines.push(Line::from(Span::styled(
                format!(" {bl}{}{br}", h_ch.repeat(card_w.saturating_sub(3).max(10))),
                border_st,
            )));
        }
        return lines;
    };

    let s_col = score_color(parsed.score, theme);
    let score_b = Style::default().fg(s_col).add_modifier(Modifier::BOLD);
    let eff_ms = if parsed.duration_ms > 0 {
        parsed.duration_ms
    } else {
        duration_ms.unwrap_or(0)
    };
    let delta_span = if cfg.show_sparkline {
        match parsed.delta {
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
            _ => Span::raw(""),
        }
    } else {
        Span::raw("")
    };

    let mood_prefix = match style {
        DoctorStyle::Kaomoji => {
            if parsed.score >= 80 {
                "(•‿•) "
            } else if parsed.score >= 55 {
                "(・_・) "
            } else {
                "(×_×) "
            }
        }
        DoctorStyle::Emoji => {
            if parsed.score >= 80 {
                "💚 "
            } else if parsed.score >= 55 {
                "🟡 "
            } else {
                "🔴 "
            }
        }
        DoctorStyle::NerdFont => {
            if parsed.score >= 80 {
                nf(NerdIcon::Check)
            } else if parsed.score >= 55 {
                nf(NerdIcon::Warning)
            } else {
                nf(NerdIcon::Error)
            }
        }
        _ => "",
    };
    let mood_with_space = if mood_prefix.is_empty() {
        String::new()
    } else if mood_prefix.ends_with(' ') {
        mood_prefix.to_string()
    } else {
        format!("{mood_prefix} ")
    };

    let (bar_fill, bar_empty) = gauge_chars(style);
    let gauge_w = inner_w.saturating_sub(36).clamp(14, 34);
    let filled = ((parsed.score as usize) * gauge_w) / 100;
    let empty = gauge_w.saturating_sub(filled);

    // Compact 2-row high-density inline layout
    if is_compact {
        lines.push(Line::from(vec![
            Span::styled(format!(" {badge} "), accent_b),
            Span::styled(
                format!("{}{}/100 ({})", mood_with_space, parsed.score, parsed.grade),
                score_b,
            ),
            delta_span,
            Span::raw(" "),
            Span::styled(bar_fill.repeat(filled.min(16)), score_b),
            Span::styled(
                bar_empty.repeat(16usize.saturating_sub(filled.min(16))),
                Style::default().fg(theme.border),
            ),
            Span::styled(
                format!(
                    " {}c {}e {}w {}d{sep}{}f · {}ms",
                    parsed.criticals,
                    parsed.errors,
                    parsed.warnings,
                    parsed.dead_code,
                    parsed.files,
                    eff_ms
                ),
                muted,
            ),
        ]));
        if cfg.show_findings && !parsed.findings.is_empty() {
            for f in parsed.findings.iter().take(2) {
                lines.push(Line::from(vec![
                    Span::raw("   "),
                    finding_badge_span(&f.badge, is_ascii, theme),
                    Span::raw(" "),
                    Span::styled(f.rule.clone(), Style::default().fg(theme.info)),
                    Span::styled(format!(" {}", f.location), muted),
                ]));
            }
        }
        return lines;
    }

    let frame_border = if is_flutter {
        Style::default().fg(s_col)
    } else {
        border_st
    };

    // Row 1: Header Frame
    if is_posh {
        let pill_bg = theme.bg_elevated;
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
            Span::styled(format!(" {badge} "), accent_b.bg(pill_bg)),
            Span::styled("\u{e0b4} ", Style::default().fg(pill_bg)),
            Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
            Span::styled(
                format!(" {}/100 ({}) ", parsed.score, parsed.grade),
                score_b.bg(pill_bg),
            ),
            Span::styled("\u{e0b4}", Style::default().fg(pill_bg)),
            delta_span,
            Span::raw(" "),
            Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
            Span::styled(format!(" {} ", parsed.label), score_b.bg(pill_bg)),
            Span::styled("\u{e0b4}", Style::default().fg(pill_bg)),
            Span::styled(format!("  {}ms", eff_ms), muted),
        ]));
    } else if is_minimal {
        lines.push(Line::from(vec![
            Span::styled(format!(" {badge}  "), accent_b),
            Span::styled(
                format!(
                    "{}{}/100 ({}) {}",
                    mood_with_space, parsed.score, parsed.grade, parsed.label
                ),
                score_b,
            ),
            delta_span,
            Span::styled(
                format!("  {sep}{} files{sep}{}ms", parsed.files, eff_ms),
                muted,
            ),
        ]));
    } else {
        let score_tag = format!(
            " {}{}/100 ({}) {}",
            mood_with_space, parsed.score, parsed.grade, parsed.label
        );
        let delta_w = UnicodeWidthStr::width(delta_span.content.as_ref());
        // Total top row width = 1(" ") + 3(tl+h+" ") + badge + 1(" ") + fill + score_tag + delta_w + 3(" "+h+tr)
        // Bottom row width    = 1(" ") + 1(bl) + (card_w - 3) + 1(br) = card_w
        let fixed_w = 1
            + 3
            + UnicodeWidthStr::width(badge.as_str())
            + 1
            + UnicodeWidthStr::width(score_tag.as_str())
            + delta_w
            + 3;
        let fill = card_w.saturating_sub(fixed_w).max(1);
        lines.push(Line::from(vec![
            Span::styled(format!(" {tl}{h_ch} "), frame_border),
            Span::styled(badge, accent_b),
            Span::styled(format!(" {}", h_ch.repeat(fill)), frame_border),
            Span::styled(score_tag, score_b),
            delta_span,
            Span::styled(format!(" {h_ch}{tr}"), frame_border),
        ]));
    }

    let left_rail = if is_minimal {
        Span::raw("   ")
    } else if is_posh {
        Span::styled(
            " ▎ ",
            Style::default().fg(s_col).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!(" {v_ch} "), frame_border)
    };

    // Row 2: Health Score Gauge Bar + Files + Duration + Scope
    lines.push(Line::from(vec![
        left_rail.clone(),
        Span::styled(bar_fill.repeat(filled), score_b),
        Span::styled(bar_empty.repeat(empty), Style::default().fg(theme.border)),
        Span::styled(
            format!(
                "  {} files{sep}{}ms{sep}{}",
                parsed.files, eff_ms, parsed.scope
            ),
            muted,
        ),
    ]));

    // Row 3: Severity Counts + Optional Dimension Sub-scores
    let mut counts_spans = vec![
        left_rail.clone(),
        Span::styled(
            format!("{} crit", parsed.criticals),
            Style::default()
                .fg(if parsed.criticals > 0 {
                    theme.destructive
                } else {
                    theme.muted
                })
                .add_modifier(if parsed.criticals > 0 {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled(sep, Style::default().fg(theme.border)),
        Span::styled(
            format!("{} err", parsed.errors),
            Style::default().fg(if parsed.errors > 0 {
                theme.destructive
            } else {
                theme.muted
            }),
        ),
        Span::styled(sep, Style::default().fg(theme.border)),
        Span::styled(
            format!("{} warn", parsed.warnings),
            Style::default().fg(if parsed.warnings > 0 {
                theme.warning
            } else {
                theme.muted
            }),
        ),
        Span::styled(sep, Style::default().fg(theme.border)),
        Span::styled(
            format!("{} dead", parsed.dead_code),
            Style::default().fg(if parsed.dead_code > 0 {
                theme.info
            } else {
                theme.muted
            }),
        ),
    ];
    if cfg.show_subscores {
        if let Some((co, se, pe, ar, ma)) = parsed.subscores {
            counts_spans.push(Span::styled("  |  ", Style::default().fg(theme.border)));
            counts_spans.push(Span::styled(
                format!("C:{co} S:{se} P:{pe} A:{ar} M:{ma}"),
                muted,
            ));
        }
    }
    lines.push(Line::from(counts_spans));

    // Row 4: Stacks & NEEDEDFIX.md artifact path
    let stacks_raw = if is_ascii {
        parsed.stacks.replace(" · ", ", ")
    } else {
        parsed.stacks.clone()
    };
    let stacks_text = if stacks_raw.is_empty() {
        "Generic".to_string()
    } else {
        truncate_cols_ascii(&stacks_raw, inner_w / 2, is_ascii)
    };
    let mut meta_spans = vec![
        left_rail.clone(),
        Span::styled(
            match style {
                DoctorStyle::Emoji => "📦 ".to_string(),
                DoctorStyle::NerdFont => format!("{} ", nf(NerdIcon::Chip)),
                _ => "Stacks: ".to_string(),
            },
            muted,
        ),
        Span::styled(
            stacks_text,
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(ref md) = parsed.neededfix_path {
        meta_spans.push(Span::styled(sep, Style::default().fg(theme.border)));
        meta_spans.push(Span::styled(
            format!("{} {}", if is_ascii { "[md]" } else { "✓" }, md),
            Style::default().fg(theme.success),
        ));
    }
    lines.push(Line::from(meta_spans));

    // Row 5+: Top Priority Findings Preview (when enabled & present)
    if cfg.show_findings && !parsed.findings.is_empty() {
        let max_show = 4usize;
        for f in parsed.findings.iter().take(max_show) {
            let msg_budget = inner_w
                .saturating_sub(f.rule.len() + f.location.len() + 14)
                .max(12);
            let msg_fit = truncate_cols_ascii(&f.message, msg_budget, is_ascii);
            lines.push(Line::from(vec![
                left_rail.clone(),
                finding_badge_span(&f.badge, is_ascii, theme),
                Span::raw(" "),
                Span::styled(
                    f.rule.clone(),
                    Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {} ", f.location), muted),
                Span::styled(msg_fit, Style::default().fg(theme.text_primary)),
            ]));
        }
        let extra = parsed
            .findings
            .len()
            .saturating_sub(max_show)
            .saturating_add(parsed.more_count);
        if extra > 0 {
            lines.push(Line::from(vec![
                left_rail.clone(),
                Span::styled(
                    format!(
                        "  +{} more finding(s) — open /doctor or .minicode/minidoctor/NEEDEDFIX.md",
                        extra
                    ),
                    muted,
                ),
            ]));
        }
    } else if parsed.findings.is_empty() {
        lines.push(Line::from(vec![
            left_rail,
            Span::styled(
                if is_ascii {
                    "[OK] 0 issues detected across all active checks."
                } else {
                    "✓ 0 issues detected across all active checks."
                },
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if !is_minimal && !is_posh {
        lines.push(Line::from(Span::styled(
            format!(" {bl}{}{br}", h_ch.repeat(card_w.saturating_sub(3).max(10))),
            frame_border,
        )));
    }

    lines
}

fn finding_badge_span(badge: &str, is_ascii: bool, theme: &Theme) -> Span<'static> {
    let upper = badge.to_ascii_uppercase();
    if is_ascii {
        let col = match upper.as_str() {
            "CRIT" | "ERR" => theme.destructive,
            "WARN" => theme.warning,
            _ => theme.info,
        };
        return Span::styled(
            format!("[{upper:<4}]"),
            Style::default().fg(col).add_modifier(Modifier::BOLD),
        );
    }
    match upper.as_str() {
        "CRIT" => Span::styled(
            " CRIT ",
            Style::default()
                .fg(Color::White)
                .bg(theme.destructive)
                .add_modifier(Modifier::BOLD),
        ),
        "ERR" => Span::styled(
            " ERR  ",
            Style::default().fg(Color::White).bg(theme.destructive),
        ),
        "WARN" => Span::styled(
            " WARN ",
            Style::default()
                .fg(Color::Black)
                .bg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        _ => Span::styled(
            format!(" {:<4} ", upper),
            Style::default().fg(Color::Black).bg(theme.info),
        ),
    }
}

/// Renders a live animated `/theme` → `20. Doctor Styles` preview showcasing BOTH the animated
/// working state (ticking with wall-clock time!) and the completed inline MiniDoctor timeline card.
pub fn render_doctor_inline_theme_preview(
    style: DoctorStyle,
    anim: DoctorAnimStyle,
    ui: &UiConfig,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let working_millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let cfg = DoctorInlineConfig {
        style,
        anim,
        enabled: ui.show_doctor_inline_card,
        show_sparkline: ui.show_doctor_sparkline,
        show_subscores: ui.show_doctor_subscores,
        show_findings: ui.show_doctor_inline_findings,
    };

    let mut out = Vec::new();
    let sample_running = render_inline_doctor_card(
        "project_doctor",
        "scope=full",
        None,
        true,
        true,
        None,
        66,
        cfg,
        working_millis,
        theme,
    );
    out.extend(sample_running.into_iter().take(4));

    let sample_done_output = "\
MiniDoctor Health Score: 92/100 (A, Excellent) (+6) · Scope: full workspace · Stacks: Rust · React · FastAPI · Files: 24 (18ms)
Counts: 0 critical, 1 errors, 1 warnings, 0 dead code | Sub-scores: Correctness 94, Security 96, Perf 90, Arch 92, Maint 88
Workspace Checklist: `.minicode/minidoctor/NEEDEDFIX.md`

Top Prioritized Findings:
- [ERR] `react/dangerously-set-inner-html` src/Card.tsx:14 — Unsanitized innerHTML | Fix: Sanitize with DOMPurify
- [WARN] `react/index-as-key` src/List.tsx:22 — Array index used as key | Fix: Use item.id";

    let sample_done = render_inline_doctor_card(
        "project_doctor",
        "scope=full",
        Some(sample_done_output),
        false,
        true,
        Some(18),
        66,
        cfg,
        working_millis,
        theme,
    );
    out.extend(sample_done.into_iter().take(7));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_doctor_output_full_and_clean() {
        let raw = "\
MiniDoctor Health Score: 49/100 (D, Critical) (-12) · Scope: full workspace · Stacks: Python · FastAPI · React · Files: 2 (32ms)
Counts: 3 critical, 1 errors, 4 warnings, 0 dead code | Sub-scores: Correctness 70, Security 40, Perf 60, Arch 85, Maint 90
Workspace Checklist: `.minicode/minidoctor/NEEDEDFIX.md`

Top Prioritized Findings:
- [CRIT] `sec/hardcoded-secret` app/main.py:6 — Hardcoded secret assigned to SECRET_KEY | Fix: Load from env
- [WARN] `react/index-as-key` web/src/App.tsx:20 — Array index used as key | Fix: Use item.id
... and 6 more findings (full list in `.minicode/minidoctor/NEEDEDFIX.md`).";

        let parsed = ParsedDoctorOutput::parse(raw).expect("valid summary parses");
        assert_eq!(parsed.score, 49);
        assert_eq!(parsed.grade, "D");
        assert_eq!(parsed.label, "Critical");
        assert_eq!(parsed.delta, Some(-12));
        assert_eq!(parsed.files, 2);
        assert_eq!(parsed.duration_ms, 32);
        assert_eq!(parsed.criticals, 3);
        assert_eq!(parsed.errors, 1);
        assert_eq!(parsed.warnings, 4);
        assert_eq!(parsed.subscores, Some((70, 40, 60, 85, 90)));
        assert_eq!(
            parsed.neededfix_path.as_deref(),
            Some(".minicode/minidoctor/NEEDEDFIX.md")
        );
        assert_eq!(parsed.findings.len(), 2);
        assert_eq!(parsed.findings[0].badge, "CRIT");
        assert_eq!(parsed.findings[0].rule, "sec/hardcoded-secret");
        assert_eq!(parsed.more_count, 6);

        assert!(ParsedDoctorOutput::parse("Rule `py/bare-except` ...").is_none());
    }

    #[test]
    fn test_render_inline_doctor_card_all_styles_and_anims() {
        let theme = Theme::default();
        let sample = "\
MiniDoctor Health Score: 100/100 (A, Excellent) (+50) · Scope: full workspace · Stacks: Rust · React · Files: 11 (15ms)
Counts: 0 critical, 0 errors, 0 warnings, 0 dead code | Sub-scores: Correctness 100, Security 100, Perf 100, Arch 100, Maint 100
Workspace Checklist: `.minicode/minidoctor/NEEDEDFIX.md`
✓ 0 issues found.";

        for style in [
            DoctorStyle::Aura,
            DoctorStyle::NerdFont,
            DoctorStyle::Kaomoji,
            DoctorStyle::Emoji,
            DoctorStyle::Ascii,
            DoctorStyle::Posh,
            DoctorStyle::Flutter,
            DoctorStyle::Minimal,
            DoctorStyle::Cyber,
            DoctorStyle::Compact,
        ] {
            for &anim in DoctorAnimStyle::all() {
                let cfg = DoctorInlineConfig {
                    style,
                    anim,
                    enabled: true,
                    show_sparkline: true,
                    show_subscores: true,
                    show_findings: true,
                };
                let running = render_inline_doctor_card(
                    "project_doctor",
                    "scope=full",
                    None,
                    true,
                    true,
                    None,
                    90,
                    cfg,
                    1250,
                    &theme,
                );
                assert!(!running.is_empty());

                let done = render_inline_doctor_card(
                    "project_doctor",
                    "scope=full",
                    Some(sample),
                    false,
                    true,
                    Some(15),
                    90,
                    cfg,
                    1250,
                    &theme,
                );
                assert!(!done.is_empty());

                if style == DoctorStyle::Ascii {
                    for line in running.iter().chain(done.iter()) {
                        let txt: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                        assert!(
                            txt.is_ascii(),
                            "ASCII inline doctor card must be 7-bit ASCII: {txt}"
                        );
                    }
                }
            }
        }
    }
}
