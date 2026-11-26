//! Customizable inline Tool Execution Card & Icon Pack renderer for `TimelineView` and `/theme` previews.

use super::{classify_diagnostic_line, TimelineView, FOLDED_DIFF_SENTINEL, FOLDED_OUT_SENTINEL};
use crate::config::{ToolCardStyle, ToolIconStyle};
use crate::ui::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCategory {
    Shell,
    Browser,
    Write,
    Edit,
    Read,
    Search,
    Plan,
    Git,
    Subagent,
    Skill,
    Doctor,
    Generic,
}

/// Fallback substring rules used only for unregistered external/MCP tool names.
const FALLBACK_TOOL_CATEGORY_RULES: &[(&[&str], ToolCategory)] = &[
    (&["bash", "exec", "shell", "terminal"], ToolCategory::Shell),
    (&["browser", "web", "fetch", "http"], ToolCategory::Browser),
    (&["doctor"], ToolCategory::Doctor),
    (
        &["subagent", "swarm", "delegate", "worker"],
        ToolCategory::Subagent,
    ),
    (&["vault", "skill"], ToolCategory::Skill),
    (&["git"], ToolCategory::Git),
    (&["plan", "todo", "progress"], ToolCategory::Plan),
    (
        &["patch", "edit", "replace", "refactor"],
        ToolCategory::Edit,
    ),
    (&["write", "create"], ToolCategory::Write),
    (&["read", "view"], ToolCategory::Read),
    (&["search", "find", "grep", "graph"], ToolCategory::Search),
];

fn classify_tool_category(raw_name: &str) -> ToolCategory {
    let lower = raw_name.trim().to_ascii_lowercase();
    let canonical = crate::tools::meta::canonical_name(lower.as_str());

    if let Some(reg_cat) = crate::tools::meta::category_of(canonical) {
        use crate::tools::category::ToolCategory as RegCat;
        return match reg_cat {
            RegCat::Exec | RegCat::Dev => ToolCategory::Shell,
            RegCat::Web => ToolCategory::Browser,
            RegCat::Git => ToolCategory::Git,
            RegCat::Agent => ToolCategory::Subagent,
            RegCat::Vault | RegCat::MiniKit => {
                if canonical.contains("doctor") {
                    ToolCategory::Doctor
                } else {
                    ToolCategory::Skill
                }
            }
            RegCat::MiniPower => {
                if canonical.contains("doctor") {
                    ToolCategory::Doctor
                } else {
                    ToolCategory::Plan
                }
            }
            RegCat::Blocks => ToolCategory::Write,
            RegCat::Memory => {
                if canonical.contains("refactor") {
                    ToolCategory::Edit
                } else if canonical.contains("skill") {
                    ToolCategory::Skill
                } else if canonical.contains("graph")
                    || canonical.contains("map")
                    || canonical.contains("audit")
                    || canonical.contains("smell")
                {
                    ToolCategory::Search
                } else {
                    ToolCategory::Plan
                }
            }
            RegCat::Search | RegCat::Codegraph => {
                if canonical.contains("rewrite") || canonical.contains("replace") {
                    ToolCategory::Edit
                } else {
                    ToolCategory::Search
                }
            }
            RegCat::Files => {
                if matches!(canonical, "patch_file" | "replace_in_files" | "multi_edit")
                    || canonical.contains("patch")
                    || canonical.contains("edit")
                    || canonical.contains("replace")
                {
                    ToolCategory::Edit
                } else if crate::tools::meta::is_file_mutation(canonical)
                    || matches!(
                        canonical,
                        "write_file" | "make_directory" | "remove_file" | "move_file" | "copy_file"
                    )
                {
                    ToolCategory::Write
                } else {
                    ToolCategory::Read
                }
            }
        };
    }

    FALLBACK_TOOL_CATEGORY_RULES
        .iter()
        .find(|(tokens, _)| tokens.iter().any(|t| canonical.contains(t)))
        .map_or(ToolCategory::Generic, |(_, cat)| *cat)
}

/// Resolves the semantic leading icon for a tool call under the active `ToolIconStyle`.
#[must_use]
pub fn tool_lead_icon(raw_name: &str, icon_style: ToolIconStyle, success: bool) -> &'static str {
    let cat = classify_tool_category(raw_name);
    match icon_style {
        ToolIconStyle::Minimal => {
            if success {
                "•"
            } else {
                "✗"
            }
        }
        ToolIconStyle::None => "",
        ToolIconStyle::Symbols => match cat {
            ToolCategory::Shell => "❯",
            ToolCategory::Browser => "◎",
            ToolCategory::Write => "✎",
            ToolCategory::Edit => "⚙",
            ToolCategory::Read => "§",
            ToolCategory::Search => "⌖",
            ToolCategory::Plan | ToolCategory::Git => "⎇",
            ToolCategory::Subagent => "❖",
            ToolCategory::Skill => "✦",
            ToolCategory::Doctor => "⚕",
            ToolCategory::Generic => "◈",
        },
        ToolIconStyle::Ascii => match cat {
            ToolCategory::Shell => "$",
            ToolCategory::Browser => "@",
            ToolCategory::Write => "+",
            ToolCategory::Edit => "~",
            ToolCategory::Read | ToolCategory::Search => "?",
            ToolCategory::Plan | ToolCategory::Git => "#",
            ToolCategory::Subagent => "&",
            ToolCategory::Doctor => "+",
            ToolCategory::Skill | ToolCategory::Generic => "*",
        },
        ToolIconStyle::Emoji => match cat {
            ToolCategory::Shell => "⚡",
            ToolCategory::Browser => "🌐",
            ToolCategory::Write => "📝",
            ToolCategory::Edit => "🔧",
            ToolCategory::Read => "📖",
            ToolCategory::Search => "🔍",
            ToolCategory::Plan => "🧭",
            ToolCategory::Git => "🌿",
            ToolCategory::Subagent => "🤖",
            ToolCategory::Skill => "🧠",
            ToolCategory::Doctor => "🩺",
            ToolCategory::Generic => "✨",
        },
        ToolIconStyle::Kaomoji => match cat {
            ToolCategory::Shell => "(ง°_°)ง",
            ToolCategory::Browser => "( °o°)",
            ToolCategory::Write => "( ▸_▸)◈",
            ToolCategory::Edit => "(⌐▪_▪)",
            ToolCategory::Read | ToolCategory::Search => "( ◉_◉)",
            ToolCategory::Plan | ToolCategory::Git => "( °_°)◈",
            ToolCategory::Subagent => "( ◉_◉)⊃",
            ToolCategory::Skill => "( ✧_✧)",
            ToolCategory::Doctor => "(◉_◉)🩺",
            ToolCategory::Generic => "( •_•)",
        },
        ToolIconStyle::NerdFont => {
            use crate::ui::icons::{nf, NerdIcon};
            match cat {
                ToolCategory::Shell => nf(NerdIcon::Terminal),
                ToolCategory::Browser => nf(NerdIcon::Globe),
                ToolCategory::Write => nf(NerdIcon::Write),
                ToolCategory::Edit => nf(NerdIcon::Edit),
                ToolCategory::Read => nf(NerdIcon::Read),
                ToolCategory::Search => nf(NerdIcon::Search),
                ToolCategory::Plan => nf(NerdIcon::Checklist),
                ToolCategory::Git => nf(NerdIcon::GitBranch),
                ToolCategory::Subagent => nf(NerdIcon::Bot),
                ToolCategory::Skill => nf(NerdIcon::Think),
                ToolCategory::Doctor => nf(NerdIcon::Sparkle),
                ToolCategory::Generic => nf(NerdIcon::Gear),
            }
        }
    }
}

/// Resolves the completion/failure status indicator glyph for the active `ToolIconStyle`.
#[must_use]
pub fn tool_outcome_glyph(icon_style: ToolIconStyle, success: bool) -> &'static str {
    match (icon_style, success) {
        (ToolIconStyle::Ascii, true) => "[OK]",
        (ToolIconStyle::Ascii, false) => "[ERR]",
        (ToolIconStyle::Emoji, true) => "✅",
        (ToolIconStyle::Emoji, false) => "❌",
        (ToolIconStyle::Kaomoji, true) => "(•‿•)",
        (ToolIconStyle::Kaomoji, false) => "(×_×)",
        (ToolIconStyle::NerdFont, true) => crate::ui::icons::nf(crate::ui::icons::NerdIcon::Check),
        (ToolIconStyle::NerdFont, false) => crate::ui::icons::nf(crate::ui::icons::NerdIcon::Error),
        (_, true) => "✓",
        (_, false) => "✗",
    }
}

/// Resolves the Thought header prefix for the active `ToolIconStyle`.
#[must_use]
pub fn thought_header_prefix(icon_style: ToolIconStyle) -> String {
    match icon_style {
        ToolIconStyle::Minimal => "• ".to_string(),
        ToolIconStyle::Symbols => "✦ ".to_string(),
        ToolIconStyle::Ascii => "* ".to_string(),
        ToolIconStyle::Emoji => "💭 ".to_string(),
        ToolIconStyle::Kaomoji => "( ._.) ".to_string(),
        ToolIconStyle::NerdFont => {
            format!(
                "{} ",
                crate::ui::icons::nf(crate::ui::icons::NerdIcon::Think)
            )
        }
        ToolIconStyle::None => String::new(),
    }
}

/// Softly blends `accent` into `base` by `ratio` for harmonious Oh My Posh / card surface tints.
fn tint_surface(accent: Color, base: Color, fallback: Color, ratio: f32) -> Color {
    match (accent, base) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let t = ratio.clamp(0.0, 1.0);
            let r = (br as f32 + (ar as f32 - br as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8;
            let g = (bg as f32 + (ag as f32 - bg as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8;
            let b = (bb as f32 + (ab as f32 - bb as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8;
            Color::Rgb(r, g, b)
        }
        _ => fallback,
    }
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

fn truncate_to_cols(text: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_cols {
        return text.to_string();
    }
    if max_cols <= 1 {
        return "…".to_string();
    }
    let limit = max_cols - 1;
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > limit {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// A pre-styled logical body line inside a tool execution block (diff or output).
struct BodyRow {
    spans: Vec<Span<'static>>,
}

/// Builds styled logical body rows (diff lines + regular output lines + folded summaries)
/// wrapped to `max_content_width` visual cells.
fn build_tool_body_rows(
    output: &str,
    success: bool,
    max_content_width: usize,
    max_lines_override: Option<usize>,
    theme: &Theme,
) -> Vec<BodyRow> {
    let mut rows: Vec<BodyRow> = Vec::new();
    let max_w = max_content_width.max(16);

    let diff_marker = crate::tools::middleware::DIFF_MARKER;
    let (diff_section, regular_output) = if let Some(rest) = output.strip_prefix(diff_marker) {
        if let Some(split_pos) = rest.find("\n\n") {
            (&rest[..split_pos + 1], rest[split_pos + 2..].trim())
        } else {
            (rest, "")
        }
    } else {
        ("", output.trim())
    };

    let diff_limit = max_lines_override.unwrap_or(crate::constants::UI_MAX_TOOL_OUTPUT_LINES);
    if !diff_section.is_empty() {
        let mut diff_line_count = 0;
        for diff_line in diff_section.lines() {
            if let Some(rem_str) = diff_line.strip_prefix(FOLDED_DIFF_SENTINEL) {
                rows.push(BodyRow {
                    spans: vec![Span::styled(
                        format!("... +{} diff lines (folded)", rem_str.trim()),
                        Style::default().fg(theme.border),
                    )],
                });
                break;
            }
            if diff_line.starts_with("---") || diff_line.starts_with("+++") {
                for (c_idx, chunk) in TimelineView::split_line_into_chunks(diff_line, max_w)
                    .into_iter()
                    .enumerate()
                {
                    let mut sp = Vec::new();
                    if c_idx > 0 {
                        sp.push(Span::raw("  "));
                    }
                    sp.push(Span::styled(
                        chunk,
                        Style::default()
                            .fg(theme.muted)
                            .add_modifier(Modifier::ITALIC),
                    ));
                    rows.push(BodyRow { spans: sp });
                }
            } else if let Some((marker, color, rest)) = diff_line
                .strip_prefix("+ ")
                .map(|r| ("+", theme.success, r))
                .or_else(|| {
                    diff_line
                        .strip_prefix("- ")
                        .map(|r| ("-", theme.destructive, r))
                })
            {
                for (c_idx, chunk) in
                    TimelineView::split_line_into_chunks(rest, max_w.saturating_sub(2).max(12))
                        .into_iter()
                        .enumerate()
                {
                    if c_idx == 0 {
                        rows.push(BodyRow {
                            spans: vec![
                                Span::styled(
                                    marker,
                                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(format!(" {}", chunk), Style::default().fg(color)),
                            ],
                        });
                    } else {
                        rows.push(BodyRow {
                            spans: vec![
                                Span::raw("  "),
                                Span::styled(chunk, Style::default().fg(color)),
                            ],
                        });
                    }
                }
                diff_line_count += 1;
            } else if let Some(rest) = diff_line.strip_prefix("  ") {
                for chunk in
                    TimelineView::split_line_into_chunks(rest, max_w.saturating_sub(2).max(12))
                {
                    rows.push(BodyRow {
                        spans: vec![
                            Span::raw("  "),
                            Span::styled(chunk, Style::default().fg(theme.muted)),
                        ],
                    });
                }
            }
            if diff_line_count > diff_limit {
                let remaining = diff_section
                    .lines()
                    .filter(|l| l.starts_with("+ ") || l.starts_with("- "))
                    .count()
                    .saturating_sub(diff_line_count);
                if remaining > 0 {
                    rows.push(BodyRow {
                        spans: vec![Span::styled(
                            format!("... +{} diff lines (folded)", remaining),
                            Style::default().fg(theme.border),
                        )],
                    });
                }
                break;
            }
        }
    }

    if !regular_output.is_empty() {
        let max_lines = max_lines_override.unwrap_or(if success {
            crate::constants::UI_MAX_TOOL_OUTPUT_LINES
        } else {
            12
        });
        let mut after_panic = false;
        let mut pre_folded_rem: Option<usize> = None;

        for (shown_count, raw_out_line) in regular_output.lines().enumerate() {
            if let Some(rem_str) = raw_out_line.strip_prefix(FOLDED_OUT_SENTINEL) {
                pre_folded_rem = rem_str.trim().parse::<usize>().ok();
                break;
            }
            if shown_count >= max_lines {
                break;
            }
            let out_line = if !success && raw_out_line.contains("Raw arguments") {
                "[Arguments payload truncated: model exceeded single-call capacity]"
            } else {
                raw_out_line
            };
            let trimmed_line = out_line.trim();
            let is_panic_header = trimmed_line.contains("panicked at");
            let line_style = classify_diagnostic_line(
                out_line,
                trimmed_line,
                is_panic_header || after_panic,
                success,
            )
            .style(theme);
            after_panic = is_panic_header;

            for (c_idx, chunk) in TimelineView::split_line_into_chunks(out_line, max_w)
                .into_iter()
                .enumerate()
            {
                let mut sp = Vec::new();
                if c_idx > 0 {
                    sp.push(Span::raw("  "));
                }
                sp.push(Span::styled(chunk, line_style));
                rows.push(BodyRow { spans: sp });
            }
        }

        let folded_remaining = pre_folded_rem.or_else(|| {
            let total = regular_output.lines().count();
            (total > max_lines).then(|| total.saturating_sub(max_lines))
        });
        if let Some(remaining) = folded_remaining.filter(|&r| r > 0) {
            rows.push(BodyRow {
                spans: vec![Span::styled(
                    format!("... +{} lines (output folded)", remaining),
                    Style::default().fg(theme.border),
                )],
            });
        }
    }

    rows
}

/// Renders an inline Tool Execution block (`ToolStart` or `ToolFinished`) according to
/// the active `ToolCardStyle` and `ToolIconStyle`.
#[allow(clippy::too_many_arguments)]
pub fn render_tool_block_lines(
    raw_name: &str,
    command_or_path: &str,
    output: Option<&str>,
    is_running: bool,
    success: bool,
    duration_ms: Option<u64>,
    area_width: u16,
    card_style: ToolCardStyle,
    icon_style: ToolIconStyle,
    surrounding_bg: Color,
    max_lines_override: Option<usize>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let title = TimelineView::format_tool_title(raw_name);
    let clean_cmd = if command_or_path.is_empty() {
        String::new()
    } else {
        crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(command_or_path)
    };
    let status_color = if is_running || success {
        theme.brand_accent
    } else {
        theme.destructive
    };
    let border_color = if is_running || success {
        theme.muted
    } else {
        theme.destructive
    };
    let lead_glyph = tool_lead_icon(raw_name, icon_style, is_running || success);
    let lead_with_space = if lead_glyph.is_empty() {
        String::new()
    } else {
        format!("{} ", lead_glyph)
    };
    let outcome_glyph = tool_outcome_glyph(icon_style, success);
    let outcome_color = if is_running {
        theme.info
    } else if success {
        theme.success
    } else {
        theme.destructive
    };
    let dur_parens = duration_ms
        .filter(|d| *d > 0)
        .map(|d| format!(" ({}ms)", d))
        .unwrap_or_default();
    let dur_short = duration_ms
        .filter(|d| *d > 0)
        .map(|d| format!("{}ms", d))
        .unwrap_or_default();
    let status_word = if is_running {
        "running..."
    } else if success {
        "completed"
    } else {
        "failed"
    };
    let short_badge = if is_running {
        "running…".to_string()
    } else if !dur_short.is_empty() {
        format!("{} {}", outcome_glyph, dur_short)
    } else {
        format!("{} {}", outcome_glyph, status_word)
    };

    let card_w = (area_width.saturating_sub(2) as usize).max(32);
    let body_max_w = match card_style {
        ToolCardStyle::RoundedCard | ToolCardStyle::SquareCard | ToolCardStyle::SplitBar => {
            card_w.saturating_sub(5).max(16)
        }
        ToolCardStyle::LeftRail => card_w.saturating_sub(4).max(16),
        _ => card_w.saturating_sub(6).max(16),
    };
    let body_rows = output
        .map(|out| build_tool_body_rows(out, success, body_max_w, max_lines_override, theme))
        .unwrap_or_default();

    let mut lines: Vec<Line<'static>> = Vec::new();

    match card_style {
        ToolCardStyle::Classic => {
            let mut h_spans = Vec::new();
            if !lead_with_space.is_empty() {
                h_spans.push(Span::styled(
                    lead_with_space,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            h_spans.push(Span::styled(
                format!("{} ", title),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ));
            if !clean_cmd.is_empty() {
                h_spans.push(Span::styled(
                    clean_cmd,
                    Style::default().fg(theme.text_primary),
                ));
            }
            lines.push(Line::from(h_spans));

            for row in body_rows {
                let mut r_spans = vec![Span::styled("  │ ", Style::default().fg(border_color))];
                r_spans.extend(row.spans);
                lines.push(Line::from(r_spans));
            }

            if is_running {
                lines.push(Line::from(vec![
                    Span::styled("  └ ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "running...",
                        Style::default()
                            .fg(theme.muted)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("  └ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!("{} ", outcome_glyph),
                        Style::default()
                            .fg(outcome_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{}{}", status_word, dur_parens),
                        Style::default().fg(if success {
                            theme.muted
                        } else {
                            theme.destructive
                        }),
                    ),
                ]));
            }
        }
        ToolCardStyle::RoundedCard | ToolCardStyle::SquareCard => {
            let (tl, tr, bl, br, h, v) = if card_style == ToolCardStyle::RoundedCard {
                ("╭", "╮", "╰", "╯", "─", "│")
            } else {
                ("┌", "┐", "└", "┘", "─", "│")
            };
            let frame_color = if is_running || success {
                theme.border
            } else {
                theme.destructive
            };
            let badge_len = UnicodeWidthStr::width(short_badge.as_str());
            let title_prefix_w = UnicodeWidthStr::width(lead_with_space.as_str())
                + UnicodeWidthStr::width(title.as_str())
                + 2;
            let cmd_budget = card_w
                .saturating_sub(title_prefix_w + badge_len + 10)
                .max(8);
            let cmd_fit = if clean_cmd.is_empty() {
                String::new()
            } else {
                format!(" {}", truncate_to_cols(&clean_cmd, cmd_budget))
            };
            let left_label_w = title_prefix_w + UnicodeWidthStr::width(cmd_fit.as_str());
            let fill_len = card_w.saturating_sub(left_label_w + badge_len + 7).max(1);

            let mut top_spans = vec![
                Span::styled(format!("{}{} ", tl, h), Style::default().fg(frame_color)),
                Span::styled(
                    lead_with_space,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    title,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if !cmd_fit.is_empty() {
                top_spans.push(Span::styled(
                    cmd_fit,
                    Style::default().fg(theme.text_primary),
                ));
            }
            top_spans.push(Span::styled(
                format!(" {} ", h.repeat(fill_len)),
                Style::default().fg(frame_color),
            ));
            top_spans.push(Span::styled(
                short_badge,
                Style::default()
                    .fg(outcome_color)
                    .add_modifier(Modifier::BOLD),
            ));
            top_spans.push(Span::styled(
                format!(" {}{}", h, tr),
                Style::default().fg(frame_color),
            ));
            lines.push(Line::from(top_spans));

            let inner_w = card_w.saturating_sub(4).max(16);
            if body_rows.is_empty() && is_running {
                let txt = "running...";
                let pad = inner_w.saturating_sub(UnicodeWidthStr::width(txt));
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", v), Style::default().fg(frame_color)),
                    Span::styled(
                        txt,
                        Style::default()
                            .fg(theme.muted)
                            .add_modifier(Modifier::ITALIC),
                    ),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(format!(" {}", v), Style::default().fg(frame_color)),
                ]));
            } else {
                for row in body_rows {
                    let rw = spans_width(&row.spans);
                    let pad = inner_w.saturating_sub(rw);
                    let mut r_spans = vec![Span::styled(
                        format!("{} ", v),
                        Style::default().fg(frame_color),
                    )];
                    r_spans.extend(row.spans);
                    r_spans.push(Span::raw(" ".repeat(pad)));
                    r_spans.push(Span::styled(
                        format!(" {}", v),
                        Style::default().fg(frame_color),
                    ));
                    lines.push(Line::from(r_spans));
                }
            }

            let bot_w = card_w.saturating_sub(2).max(4);
            lines.push(Line::from(vec![Span::styled(
                format!("{}{}{}", bl, h.repeat(bot_w), br),
                Style::default().fg(frame_color),
            )]));
        }
        ToolCardStyle::LeftRail | ToolCardStyle::SplitBar => {
            let is_split = card_style == ToolCardStyle::SplitBar;
            let card_bg = tint_surface(status_color, surrounding_bg, theme.bg_elevated, 0.08);
            let inner_w = card_w.saturating_sub(if is_split { 4 } else { 3 }).max(16);
            let rail_style = Style::default().fg(status_color).bg(card_bg);

            let badge_w = UnicodeWidthStr::width(short_badge.as_str());
            let left_prefix_w = UnicodeWidthStr::width(lead_with_space.as_str())
                + UnicodeWidthStr::width(title.as_str())
                + 1;
            let cmd_budget = inner_w.saturating_sub(left_prefix_w + badge_w + 3).max(6);
            let cmd_fit = if clean_cmd.is_empty() {
                String::new()
            } else {
                format!(" {}", truncate_to_cols(&clean_cmd, cmd_budget))
            };
            let used_w = UnicodeWidthStr::width(lead_with_space.as_str())
                + UnicodeWidthStr::width(title.as_str())
                + UnicodeWidthStr::width(cmd_fit.as_str())
                + badge_w;
            let gap = inner_w.saturating_sub(used_w).max(1);

            let mut hdr = vec![Span::styled("┃ ", rail_style)];
            if !lead_with_space.is_empty() {
                hdr.push(Span::styled(
                    lead_with_space,
                    Style::default()
                        .fg(status_color)
                        .bg(card_bg)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            hdr.push(Span::styled(
                title,
                Style::default()
                    .fg(status_color)
                    .bg(card_bg)
                    .add_modifier(Modifier::BOLD),
            ));
            if !cmd_fit.is_empty() {
                hdr.push(Span::styled(
                    cmd_fit,
                    Style::default().fg(theme.text_primary).bg(card_bg),
                ));
            }
            hdr.push(Span::styled(" ".repeat(gap), Style::default().bg(card_bg)));
            hdr.push(Span::styled(
                short_badge,
                Style::default()
                    .fg(outcome_color)
                    .bg(card_bg)
                    .add_modifier(Modifier::BOLD),
            ));
            if is_split {
                hdr.push(Span::styled(" ┃", rail_style));
            } else {
                hdr.push(Span::styled(" ", Style::default().bg(card_bg)));
            }
            lines.push(Line::from(hdr));

            if body_rows.is_empty() && is_running {
                let txt = "running...";
                let pad = inner_w.saturating_sub(UnicodeWidthStr::width(txt));
                let mut r_spans = vec![
                    Span::styled("┃ ", rail_style),
                    Span::styled(
                        txt,
                        Style::default()
                            .fg(theme.muted)
                            .bg(card_bg)
                            .add_modifier(Modifier::ITALIC),
                    ),
                    Span::styled(" ".repeat(pad), Style::default().bg(card_bg)),
                ];
                if is_split {
                    r_spans.push(Span::styled(" ┃", rail_style));
                } else {
                    r_spans.push(Span::styled(" ", Style::default().bg(card_bg)));
                }
                lines.push(Line::from(r_spans));
            } else {
                for row in body_rows {
                    let rw = spans_width(&row.spans);
                    let pad = inner_w.saturating_sub(rw);
                    let mut r_spans = vec![Span::styled("┃ ", rail_style)];
                    for sp in row.spans {
                        r_spans.push(Span::styled(sp.content, sp.style.bg(card_bg)));
                    }
                    r_spans.push(Span::styled(" ".repeat(pad), Style::default().bg(card_bg)));
                    if is_split {
                        r_spans.push(Span::styled(" ┃", rail_style));
                    } else {
                        r_spans.push(Span::styled(" ", Style::default().bg(card_bg)));
                    }
                    lines.push(Line::from(r_spans));
                }
            }
        }
        ToolCardStyle::PoshPill | ToolCardStyle::PoshBubbles => {
            let is_bubbles = card_style == ToolCardStyle::PoshBubbles;
            let pill_bg = tint_surface(status_color, surrounding_bg, theme.bg_elevated, 0.22);
            let foot_bg = tint_surface(outcome_color, surrounding_bg, theme.bg_elevated, 0.20);
            let pill_text = format!("{}{}", lead_with_space, title);

            let mut hdr: Vec<Span<'static>> = Vec::new();
            if is_bubbles {
                let cap_st = Style::default().fg(pill_bg).bg(surrounding_bg);
                hdr.push(Span::styled("\u{e0b6}", cap_st));
                hdr.push(Span::styled(
                    pill_text,
                    Style::default()
                        .fg(status_color)
                        .bg(pill_bg)
                        .add_modifier(Modifier::BOLD),
                ));
                hdr.push(Span::styled("\u{e0b4}", cap_st));
            } else {
                hdr.push(Span::styled(
                    "▎",
                    Style::default().fg(status_color).bg(pill_bg),
                ));
                hdr.push(Span::styled(
                    format!(" {} ", pill_text),
                    Style::default()
                        .fg(status_color)
                        .bg(pill_bg)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            if !clean_cmd.is_empty() {
                hdr.push(Span::styled(
                    format!(" {}", clean_cmd),
                    Style::default().fg(theme.text_primary),
                ));
            }
            lines.push(Line::from(hdr));

            let gutter = if is_bubbles { "  │ " } else { "  ▎ " };
            for row in body_rows {
                let mut r_spans = vec![Span::styled(gutter, Style::default().fg(border_color))];
                r_spans.extend(row.spans);
                lines.push(Line::from(r_spans));
            }

            let mut foot = vec![Span::styled("  ╰ ", Style::default().fg(border_color))];
            if is_bubbles {
                let cap_st = Style::default().fg(foot_bg).bg(surrounding_bg);
                foot.push(Span::styled("\u{e0b6}", cap_st));
                foot.push(Span::styled(
                    short_badge,
                    Style::default()
                        .fg(outcome_color)
                        .bg(foot_bg)
                        .add_modifier(Modifier::BOLD),
                ));
                foot.push(Span::styled("\u{e0b4}", cap_st));
            } else {
                foot.push(Span::styled(
                    format!(" {} ", short_badge),
                    Style::default()
                        .fg(outcome_color)
                        .bg(foot_bg)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            lines.push(Line::from(foot));
        }
        ToolCardStyle::Compact => {
            let mut hdr = Vec::new();
            if !lead_with_space.is_empty() {
                hdr.push(Span::styled(
                    lead_with_space,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            hdr.push(Span::styled(
                title,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ));
            if !clean_cmd.is_empty() {
                hdr.push(Span::styled(
                    format!(" {}", clean_cmd),
                    Style::default().fg(theme.text_primary),
                ));
            }
            hdr.push(Span::styled("  ", Style::default()));
            hdr.push(Span::styled(
                short_badge,
                Style::default()
                    .fg(outcome_color)
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::from(hdr));

            for row in body_rows {
                let mut r_spans = vec![Span::raw("  ")];
                r_spans.extend(row.spans);
                lines.push(Line::from(r_spans));
            }
        }
        ToolCardStyle::Minimal => {
            let badge_w = UnicodeWidthStr::width(short_badge.as_str());
            let title_w = UnicodeWidthStr::width(lead_with_space.as_str())
                + UnicodeWidthStr::width(title.as_str())
                + 4;
            let cmd_budget = card_w.saturating_sub(title_w + badge_w + 8).max(8);
            let cmd_fit = if clean_cmd.is_empty() {
                String::new()
            } else {
                format!(" {}", truncate_to_cols(&clean_cmd, cmd_budget))
            };
            let rule_rem = card_w
                .saturating_sub(title_w + UnicodeWidthStr::width(cmd_fit.as_str()) + badge_w + 3)
                .max(2);

            let mut hdr = vec![
                Span::styled("── ", Style::default().fg(theme.border)),
                Span::styled(
                    lead_with_space,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    title,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if !cmd_fit.is_empty() {
                hdr.push(Span::styled(
                    cmd_fit,
                    Style::default().fg(theme.text_primary),
                ));
            }
            hdr.push(Span::styled(
                format!(" {} ", "─".repeat(rule_rem)),
                Style::default().fg(theme.border),
            ));
            hdr.push(Span::styled(
                short_badge,
                Style::default()
                    .fg(outcome_color)
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::from(hdr));

            for row in body_rows {
                let mut r_spans = vec![Span::raw("   ")];
                r_spans.extend(row.spans);
                lines.push(Line::from(r_spans));
            }
        }
        ToolCardStyle::Brackets => {
            let tag = format!("[{}{}]", lead_with_space, title.to_ascii_uppercase());
            let mut hdr = vec![Span::styled(
                tag,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )];
            if !clean_cmd.is_empty() {
                hdr.push(Span::styled(
                    format!(" {}", clean_cmd),
                    Style::default().fg(theme.text_primary),
                ));
            }
            lines.push(Line::from(hdr));

            for row in body_rows {
                let mut r_spans = vec![Span::styled("  > ", Style::default().fg(border_color))];
                r_spans.extend(row.spans);
                lines.push(Line::from(r_spans));
            }

            let foot_tag = if is_running {
                "[RUNNING]".to_string()
            } else if !dur_short.is_empty() {
                format!("[{} {} · {}]", outcome_glyph, status_word, dur_short)
            } else {
                format!("[{} {}]", outcome_glyph, status_word)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    foot_tag,
                    Style::default()
                        .fg(outcome_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }

    lines
}

/// Renders a compact live preview of two representative agent tool cards (`Bash` + `Browser Navigate`)
/// for the `/theme` modal (`14. Tool Cards` and `15. Tool Icons`).
pub fn render_tool_card_preview(
    card_style: ToolCardStyle,
    icon_style: ToolIconStyle,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let preview_w = width.clamp(42, 74) as u16;
    let mut out = render_tool_block_lines(
        "exec_cmd",
        "cargo test --lib ui::status",
        Some("test result: ok. 12 passed; 0 failed"),
        false,
        true,
        Some(48),
        preview_w,
        card_style,
        icon_style,
        theme.bg_elevated,
        Some(1),
        theme,
    );
    out.push(Line::from(String::new()));
    out.extend(render_tool_block_lines(
        "browser_navigate",
        "http://127.0.0.1:4173/",
        Some("Viewport: 1440x900px | Interactive Tree ready"),
        false,
        true,
        Some(312),
        preview_w,
        card_style,
        icon_style,
        theme.bg_elevated,
        Some(1),
        theme,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_tool_category_uses_canonical_registry_metadata() {
        assert_eq!(classify_tool_category("exec_cmd"), ToolCategory::Shell);
        assert_eq!(classify_tool_category("bash"), ToolCategory::Shell);
        assert_eq!(classify_tool_category("patch_file"), ToolCategory::Edit);
        assert_eq!(classify_tool_category("edit_file"), ToolCategory::Edit);
        assert_eq!(
            classify_tool_category("replace_in_files"),
            ToolCategory::Edit
        );
        assert_eq!(classify_tool_category("write_file"), ToolCategory::Write);
        assert_eq!(classify_tool_category("read_file"), ToolCategory::Read);
        assert_eq!(classify_tool_category("view_file"), ToolCategory::Read);
        assert_eq!(
            classify_tool_category("read_worker_messages"),
            ToolCategory::Subagent
        );
        assert_eq!(
            classify_tool_category("project_doctor"),
            ToolCategory::Doctor
        );
        assert_eq!(classify_tool_category("vault_load"), ToolCategory::Skill);
        assert_eq!(classify_tool_category("git_status"), ToolCategory::Git);
        assert_eq!(
            classify_tool_category("mcp_external_http_fetch"),
            ToolCategory::Browser
        );
    }
}
