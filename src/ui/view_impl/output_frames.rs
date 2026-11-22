//! Assistant output frame, top divider, and live preview renderers.
//!
//! Supports open-sided rounded/square/double bracket frames (like `╭─ ✦ minicode ───╮` / `╰───╯`
//! with clean indented copy-pasteable body lines), full cards, left accent rails, hairline/short/
//! medium/labeled/gradient/dashed/dotted rules, Oh My Posh pills/powerline ribbons, Nerd Font icons
//! (zero-box auto-fallback via `crate::ui::icons::nf`), Emoji, and pure 7-bit ASCII frames.

use crate::config::{OutputFrameStyle, OutputStyle};
use crate::ui::icons::{nf, NerdIcon};
use crate::ui::markdown::MarkdownRenderer;
use crate::ui::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::TimelineView;

/// Blends two RGB colors linearly (used for subtle gradient dividers); falls back cleanly on indexed colors.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let t = t.clamp(0.0, 1.0);
            let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t).round() as u8;
            let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t).round() as u8;
            let bl = (b1 as f32 + (b2 as f32 - b1 as f32) * t).round() as u8;
            Color::Rgb(r, g, bl)
        }
        _ => {
            if t < 0.5 {
                a
            } else {
                b
            }
        }
    }
}

/// Converts a borrowed `Line<'_>` into an owned `Line<'static>`.
fn owned_line(line: Line<'_>) -> Line<'static> {
    let spans: Vec<Span<'static>> = line
        .spans
        .into_iter()
        .map(|sp| Span::styled(sp.content.into_owned(), sp.style))
        .collect();
    Line::from(spans)
}

/// Visual column width of a ratatui `Line`.
fn line_visual_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

/// Builds a labeled open-bracket or box top rule: `<left><fill> <badge><title> <fill...><right>`.
#[allow(clippy::too_many_arguments)]
fn build_framed_top_rule(
    left: &str,
    fill: &str,
    right: &str,
    badge: Option<(&str, Style)>,
    title: Option<(&str, Style)>,
    frame_w: usize,
    border_style: Style,
) -> Line<'static> {
    let frame_w = frame_w.max(12);
    let left_w = UnicodeWidthStr::width(left);
    let right_w = UnicodeWidthStr::width(right);
    let fill_w = UnicodeWidthStr::width(fill).max(1);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(7);
    spans.push(Span::styled(left.to_string(), border_style));
    let mut used = left_w;

    if badge.is_some() || title.is_some() {
        spans.push(Span::styled(format!("{fill} "), border_style));
        used += fill_w + 1;

        if let Some((b_txt, b_st)) = badge {
            spans.push(Span::styled(b_txt.to_string(), b_st));
            used += UnicodeWidthStr::width(b_txt);
        }
        if let Some((t_txt, t_st)) = title {
            spans.push(Span::styled(t_txt.to_string(), t_st));
            used += UnicodeWidthStr::width(t_txt);
        }
        spans.push(Span::styled(" ", border_style));
        used += 1;
    }

    let rem_cols = frame_w.saturating_sub(used + right_w);
    let repeat_cnt = (rem_cols / fill_w).max(1);
    spans.push(Span::styled(fill.repeat(repeat_cnt), border_style));
    if !right.is_empty() {
        spans.push(Span::styled(right.to_string(), border_style));
    }
    Line::from(spans)
}

/// Builds a matching bottom rule: `<left><fill...><right>` of exact visual width `frame_w`.
fn build_framed_bottom_rule(
    left: &str,
    fill: &str,
    right: &str,
    frame_w: usize,
    border_style: Style,
) -> Line<'static> {
    let frame_w = frame_w.max(12);
    let left_w = UnicodeWidthStr::width(left);
    let right_w = UnicodeWidthStr::width(right);
    let fill_w = UnicodeWidthStr::width(fill).max(1);
    let rem_cols = frame_w.saturating_sub(left_w + right_w);
    let repeat_cnt = (rem_cols / fill_w).max(1);
    Line::from(vec![
        Span::styled(left.to_string(), border_style),
        Span::styled(fill.repeat(repeat_cnt), border_style),
        Span::styled(right.to_string(), border_style),
    ])
}

/// Builds a smooth theme-accent gradient hairline rule.
fn build_gradient_rule(width: usize, theme: &Theme) -> Line<'static> {
    let total = width.max(12);
    let segs = total.min(24);
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(segs);
    let chunk_base = total / segs;
    let extra = total % segs;

    for i in 0..segs {
        let len = chunk_base + usize::from(i < extra);
        if len == 0 {
            continue;
        }
        let t = if segs <= 1 {
            0.0
        } else {
            i as f32 / (segs - 1) as f32
        };
        let color = if t < 0.45 {
            lerp_color(theme.brand_accent, theme.info, t / 0.45)
        } else {
            lerp_color(theme.info, theme.border, (t - 0.45) / 0.55)
        };
        let ch = if t < 0.30 {
            "━"
        } else if t < 0.72 {
            "─"
        } else {
            "┈"
        };
        spans.push(Span::styled(ch.repeat(len), Style::default().fg(color)));
    }
    Line::from(spans)
}

/// Trims empty leading and trailing lines produced by markdown rendering so open frames never
/// exhibit awkward blank rows directly beneath `╭─` or directly above `╰─`.
fn trim_outer_blank_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let is_blank = |l: &Line<'static>| {
        l.spans
            .iter()
            .all(|s| s.content.chars().all(char::is_whitespace))
    };
    let start = lines.iter().position(|l| !is_blank(l)).unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|l| !is_blank(l))
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end {
        Vec::new()
    } else {
        lines.into_iter().skip(start).take(end - start).collect()
    }
}

/// Renders an assistant markdown entry with both the chosen [`OutputFrameStyle`] (top divider or
/// open/enclosed frame) and [`OutputStyle`] (Markdown typography, callouts, checklists, code & tables).
pub fn render_assistant_output_lines(
    text: &str,
    usable_width: usize,
    frame_style: OutputFrameStyle,
    output_style: OutputStyle,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let usable_width = usable_width.max(20);
    if text.trim().is_empty() {
        return Vec::new();
    }

    let border_style = Style::default().fg(theme.border);
    let accent_bold = Style::default()
        .fg(theme.brand_accent)
        .add_modifier(Modifier::BOLD);
    let info_bold = Style::default().fg(theme.info).add_modifier(Modifier::BOLD);
    let title_style = Style::default()
        .fg(theme.brand_accent)
        .add_modifier(Modifier::BOLD);

    let frame_w = usable_width.max(20);

    match frame_style {
        OutputFrameStyle::None => {
            MarkdownRenderer::render_with_style(text, theme, usable_width, output_style)
                .into_iter()
                .map(owned_line)
                .collect()
        }

        // ── Open-Sided Bracket Frames (Clean copy-pasteable indented body, NO side `│` walls) ──
        OutputFrameStyle::OpenRounded
        | OutputFrameStyle::OpenRoundedMinimal
        | OutputFrameStyle::OpenRoundedNerd
        | OutputFrameStyle::OpenRoundedEmoji
        | OutputFrameStyle::OpenSquare
        | OutputFrameStyle::OpenDouble
        | OutputFrameStyle::OpenTopOnly
        | OutputFrameStyle::AsciiFrame => {
            let inner_w = frame_w.saturating_sub(4).max(16);
            let raw_md: Vec<Line<'static>> =
                MarkdownRenderer::render_with_style(text, theme, inner_w, output_style)
                    .into_iter()
                    .map(owned_line)
                    .collect();
            let wrapped = TimelineView::wrap_lines_to_width(raw_md, inner_w)
                .into_iter()
                .map(owned_line)
                .collect();
            let body = trim_outer_blank_lines(wrapped);
            if body.is_empty() {
                return Vec::new();
            }

            let mut out = Vec::with_capacity(body.len() + 3);
            let nf_bot = format!("{} ", nf(NerdIcon::Bot));
            let top = match frame_style {
                OutputFrameStyle::OpenRounded | OutputFrameStyle::OpenTopOnly => {
                    build_framed_top_rule(
                        "╭",
                        "─",
                        "╮",
                        Some(("✦ ", info_bold)),
                        Some(("minicode", title_style)),
                        frame_w,
                        border_style,
                    )
                }
                OutputFrameStyle::OpenRoundedMinimal => {
                    build_framed_top_rule("╭", "─", "╮", None, None, frame_w, border_style)
                }
                OutputFrameStyle::OpenRoundedNerd => build_framed_top_rule(
                    "╭",
                    "─",
                    "╮",
                    Some((nf_bot.as_str(), info_bold)),
                    Some(("minicode", title_style)),
                    frame_w,
                    border_style,
                ),
                OutputFrameStyle::OpenRoundedEmoji => build_framed_top_rule(
                    "╭",
                    "─",
                    "╮",
                    Some(("✨ ", info_bold)),
                    Some(("minicode", title_style)),
                    frame_w,
                    border_style,
                ),
                OutputFrameStyle::OpenSquare => build_framed_top_rule(
                    "┌",
                    "─",
                    "┐",
                    None,
                    Some(("minicode", title_style)),
                    frame_w,
                    border_style,
                ),
                OutputFrameStyle::OpenDouble => build_framed_top_rule(
                    "╔",
                    "═",
                    "╗",
                    None,
                    Some(("minicode", title_style)),
                    frame_w,
                    border_style,
                ),
                OutputFrameStyle::AsciiFrame => build_framed_top_rule(
                    "+",
                    "-",
                    "+",
                    None,
                    Some(("minicode", title_style)),
                    frame_w,
                    border_style,
                ),
                _ => unreachable!(),
            };
            out.push(top);

            for line in body {
                if line.spans.is_empty() {
                    out.push(Line::from(String::new()));
                } else {
                    let mut spans = Vec::with_capacity(line.spans.len() + 1);
                    spans.push(Span::raw("  "));
                    spans.extend(line.spans);
                    out.push(Line::from(spans));
                }
            }

            match frame_style {
                OutputFrameStyle::OpenRounded
                | OutputFrameStyle::OpenRoundedMinimal
                | OutputFrameStyle::OpenRoundedNerd
                | OutputFrameStyle::OpenRoundedEmoji => {
                    out.push(build_framed_bottom_rule(
                        "╰",
                        "─",
                        "╯",
                        frame_w,
                        border_style,
                    ));
                }
                OutputFrameStyle::OpenSquare => {
                    out.push(build_framed_bottom_rule(
                        "└",
                        "─",
                        "┘",
                        frame_w,
                        border_style,
                    ));
                }
                OutputFrameStyle::OpenDouble => {
                    out.push(build_framed_bottom_rule(
                        "╚",
                        "═",
                        "╝",
                        frame_w,
                        border_style,
                    ));
                }
                OutputFrameStyle::AsciiFrame => {
                    out.push(build_framed_bottom_rule(
                        "+",
                        "-",
                        "+",
                        frame_w,
                        border_style,
                    ));
                }
                OutputFrameStyle::OpenTopOnly => {}
                _ => {}
            }
            out
        }

        // ── Full Enclosed Rounded Card (`╭─ ✦ minicode ───╮` / `│ body │` / `╰───╯`) ──
        OutputFrameStyle::FullCard => {
            let inner_w = frame_w.saturating_sub(4).max(14);
            let raw_md: Vec<Line<'static>> =
                MarkdownRenderer::render_with_style(text, theme, inner_w, output_style)
                    .into_iter()
                    .map(owned_line)
                    .collect();
            let wrapped = TimelineView::wrap_lines_to_width(raw_md, inner_w)
                .into_iter()
                .map(owned_line)
                .collect();
            let body = trim_outer_blank_lines(wrapped);
            if body.is_empty() {
                return Vec::new();
            }

            let mut out = Vec::with_capacity(body.len() + 2);
            out.push(build_framed_top_rule(
                "╭",
                "─",
                "╮",
                Some(("✦ ", info_bold)),
                Some(("minicode", title_style)),
                frame_w,
                border_style,
            ));

            for line in body {
                let vis_w = line_visual_width(&line);
                let pad = inner_w.saturating_sub(vis_w);
                let mut spans = Vec::with_capacity(line.spans.len() + 3);
                spans.push(Span::styled("│ ", border_style));
                spans.extend(line.spans);
                if pad > 0 {
                    spans.push(Span::raw(" ".repeat(pad)));
                }
                spans.push(Span::styled(" │", border_style));
                out.push(Line::from(spans));
            }

            out.push(build_framed_bottom_rule(
                "╰",
                "─",
                "╯",
                frame_w,
                border_style,
            ));
            out
        }

        // ── Left Accent Spine (`┃ ✦ minicode` / `┃ body`) ──
        OutputFrameStyle::LeftRail => {
            let inner_w = frame_w.saturating_sub(3).max(16);
            let raw_md: Vec<Line<'static>> =
                MarkdownRenderer::render_with_style(text, theme, inner_w, output_style)
                    .into_iter()
                    .map(owned_line)
                    .collect();
            let wrapped = TimelineView::wrap_lines_to_width(raw_md, inner_w)
                .into_iter()
                .map(owned_line)
                .collect();
            let body = trim_outer_blank_lines(wrapped);
            if body.is_empty() {
                return Vec::new();
            }

            let rail_head = Style::default().fg(theme.brand_accent);
            let rail_body = Style::default().fg(theme.border);
            let mut out = Vec::with_capacity(body.len() + 2);
            out.push(Line::from(vec![
                Span::styled("┃ ", rail_head),
                Span::styled("✦ ", info_bold),
                Span::styled("minicode", accent_bold),
            ]));
            for line in body {
                let mut spans = Vec::with_capacity(line.spans.len() + 1);
                spans.push(Span::styled("┃ ", rail_body));
                spans.extend(line.spans);
                out.push(Line::from(spans));
            }
            out
        }

        // ── Top Divider Rules (full, medium, short, labeled, gradient, posh, dashed, dotted) ──
        OutputFrameStyle::FullRule
        | OutputFrameStyle::MediumRule
        | OutputFrameStyle::ShortRule
        | OutputFrameStyle::LabeledRule
        | OutputFrameStyle::GradientRule
        | OutputFrameStyle::PoshPill
        | OutputFrameStyle::PoshPowerline
        | OutputFrameStyle::DashedRule
        | OutputFrameStyle::DottedRule => {
            let raw_md: Vec<Line<'static>> =
                MarkdownRenderer::render_with_style(text, theme, frame_w, output_style)
                    .into_iter()
                    .map(owned_line)
                    .collect();
            let body = trim_outer_blank_lines(raw_md);
            if body.is_empty() {
                return Vec::new();
            }

            let divider = match frame_style {
                OutputFrameStyle::FullRule => {
                    Line::from(vec![Span::styled("─".repeat(frame_w), border_style)])
                }
                OutputFrameStyle::MediumRule => {
                    let w = (frame_w / 2).clamp(16, 48).min(frame_w);
                    Line::from(vec![Span::styled("─".repeat(w), border_style)])
                }
                OutputFrameStyle::ShortRule => {
                    let w = 14usize.min(frame_w);
                    Line::from(vec![Span::styled("─".repeat(w), border_style)])
                }
                OutputFrameStyle::LabeledRule => {
                    let label = "── minicode ";
                    let rem = frame_w.saturating_sub(UnicodeWidthStr::width(label));
                    Line::from(vec![
                        Span::styled("── ", border_style),
                        Span::styled("minicode ", accent_bold),
                        Span::styled("─".repeat(rem), border_style),
                    ])
                }
                OutputFrameStyle::GradientRule => build_gradient_rule(frame_w, theme),
                OutputFrameStyle::PoshPill => {
                    let pill_bg = theme.bg_elevated;
                    let pill_txt = "✦ minicode";
                    let pill_w = UnicodeWidthStr::width(pill_txt) + 3; //  + txt +  + space
                    let rem = frame_w.saturating_sub(pill_w);
                    Line::from(vec![
                        Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
                        Span::styled(
                            "✦ ",
                            Style::default()
                                .fg(theme.info)
                                .bg(pill_bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "minicode",
                            Style::default()
                                .fg(theme.brand_accent)
                                .bg(pill_bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("\u{e0b4} ", Style::default().fg(pill_bg)),
                        Span::styled("─".repeat(rem), border_style),
                    ])
                }
                OutputFrameStyle::PoshPowerline => {
                    let pill_bg = theme.bg_elevated;
                    let pill_txt = " ✦ minicode ";
                    let pill_w = UnicodeWidthStr::width(pill_txt) + 2; //  + space
                    let rem = frame_w.saturating_sub(pill_w);
                    Line::from(vec![
                        Span::styled(
                            " ✦ ",
                            Style::default()
                                .fg(theme.info)
                                .bg(pill_bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "minicode ",
                            Style::default()
                                .fg(theme.brand_accent)
                                .bg(pill_bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("\u{e0b0} ", Style::default().fg(pill_bg)),
                        Span::styled("─".repeat(rem), border_style),
                    ])
                }
                OutputFrameStyle::DashedRule => {
                    Line::from(vec![Span::styled("╌".repeat(frame_w), border_style)])
                }
                OutputFrameStyle::DottedRule => {
                    Line::from(vec![Span::styled("┈".repeat(frame_w), border_style)])
                }
                _ => unreachable!(),
            };

            let mut out = Vec::with_capacity(body.len() + 1);
            out.push(divider);
            out.extend(body);
            out
        }
    }
}

/// Renders a live `/theme` preview showcasing the selected [`OutputFrameStyle`] with the active
/// [`OutputStyle`].
pub fn render_output_frame_preview(
    frame_style: OutputFrameStyle,
    output_style: OutputStyle,
    preview_w: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let sample = "Page is live at `http://127.0.0.1:4173` — **12/12** checks passed in 48ms.\n\n- [x] Verify `src/ui/view.rs` layout alignment\n- [ ] Ready for your next command";
    let w = preview_w.clamp(36, 76);
    render_assistant_output_lines(sample, w, frame_style, output_style, theme)
        .into_iter()
        .map(|l| {
            let mut spans = Vec::with_capacity(l.spans.len() + 1);
            spans.push(Span::raw("  "));
            spans.extend(l.spans);
            Line::from(spans)
        })
        .collect()
}

/// Renders a live `/theme` preview showcasing the selected [`OutputStyle`] across headings,
/// GitHub/Report-Skill callouts, checklists, code blocks, and tables with numeric columns.
pub fn render_output_style_preview(
    output_style: OutputStyle,
    frame_style: OutputFrameStyle,
    preview_w: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let sample = "\
## Release Summary
> [!TIP] All 231 tests passed in 1.74s.
- [x] Verify `src/ui/icons.rs` fallback (**v0.5.75**)
```rust
let ok = render_output(style);
```
| Check | Latency | Status |
| --- | --- | --- |
| Clippy | 142ms | Passed |";
    let w = preview_w.clamp(40, 76);
    render_assistant_output_lines(sample, w, frame_style, output_style, theme)
        .into_iter()
        .map(|l| {
            let mut spans = Vec::with_capacity(l.spans.len() + 1);
            spans.push(Span::raw("  "));
            spans.extend(l.spans);
            Line::from(spans)
        })
        .collect()
}
