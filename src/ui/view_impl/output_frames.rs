//! Assistant output frame, top divider, badge icon, end-of-turn summary footer,
//! and `/theme` live preview renderers.
//!
//! Supports:
//! - 20 [`OutputFrameStyle`] options (open-sided rounded/square/double brackets with spotless
//!   copy-pasteable indented body, full cards, left accent rails, full/medium/short/labeled/
//!   gradient/dashed/dotted rules, Oh My Posh pills & powerline ribbons, and ASCII frames).
//! - 17 [`OutputBadgeStyle`] options for the leading badge icon (`✦`, `◈`, `❖`, `⚡`, Nerd Font
//!   auto-fallback, Emoji, ASCII, static & **animated** Kaomoji, and animated sparkle pulse).
//! - End-of-turn task telemetry footer (`TurnSummary`) showing total turn tokens, total elapsed
//!   turn time, and optional tool count / cost / model / KV cache with customizable placement
//!   (`BottomRight` default, `BottomLeft`, `BottomCenter`, `BottomRuleRight`, `BottomRuleLeft`,
//!   `Hidden`) and 12 visual styles.

use crate::config::{
    OutputBadgeStyle, OutputFrameStyle, OutputStyle, TurnSummaryConfig, TurnSummaryPosition,
    TurnSummaryStyle,
};
use crate::ui::icons::{nf, NerdIcon};
use crate::ui::markdown::MarkdownRenderer;
use crate::ui::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::TimelineView;

/// Animated blinking/smiling Kaomoji sequence for [`OutputBadgeStyle::KaomojiAnimated`].
const KAOMOJI_ANIM_FRAMES: &[&str] = &["(•‿•)", "(•‿•)", "(•_•)", "(^‿^)", "(✧‿✧)", "(•‿•)"];

/// Animated cat Kaomoji sequence for [`OutputBadgeStyle::KaomojiCat`].
const KAOMOJI_CAT_FRAMES: &[&str] = &["(=^･ω･^=)", "(=^･ω･^=)", "(=^･ｪ･^=)", "(=^-ω-^=)"];

/// Animated 1-cell Unicode sparkle pulse for [`OutputBadgeStyle::SparkleAnimated`].
const SPARKLE_ANIM_FRAMES: &[&str] = &["✦", "✧", "✶", "✷", "✶", "✧"];

/// Telemetry metrics recorded when an agent task turn completes.
#[derive(Debug, Clone, Copy)]
pub struct TurnSummaryMetrics<'a> {
    pub tokens_used: usize,
    pub cached_tokens: usize,
    pub duration_secs: f64,
    pub tool_calls: usize,
    pub cost_usd: f64,
    pub model: &'a str,
}

/// Resolves the rendered prefix string (including trailing space) for an [`OutputBadgeStyle`].
/// `anim_frame == 0` produces the calm resting frame when idle.
#[must_use]
pub fn resolve_output_badge(style: OutputBadgeStyle, anim_frame: usize) -> Option<String> {
    let raw: &str = match style {
        OutputBadgeStyle::Sparkle => "✦",
        OutputBadgeStyle::Diamond => "◈",
        OutputBadgeStyle::BotSymbol => "❖",
        OutputBadgeStyle::Bolt => "⚡",
        OutputBadgeStyle::NerdFont => nf(NerdIcon::Bot),
        OutputBadgeStyle::NerdFontSparkle => nf(NerdIcon::Sparkle),
        OutputBadgeStyle::EmojiSparkle => "✨",
        OutputBadgeStyle::EmojiRobot => "🤖",
        OutputBadgeStyle::EmojiPaw => "🐾",
        OutputBadgeStyle::EmojiRocket => "🚀",
        OutputBadgeStyle::Ascii => ">",
        OutputBadgeStyle::AsciiStar => "*",
        OutputBadgeStyle::Kaomoji => "(•‿•)",
        OutputBadgeStyle::KaomojiAnimated => {
            KAOMOJI_ANIM_FRAMES[anim_frame % KAOMOJI_ANIM_FRAMES.len()]
        }
        OutputBadgeStyle::KaomojiCat => KAOMOJI_CAT_FRAMES[anim_frame % KAOMOJI_CAT_FRAMES.len()],
        OutputBadgeStyle::SparkleAnimated => {
            SPARKLE_ANIM_FRAMES[anim_frame % SPARKLE_ANIM_FRAMES.len()]
        }
        OutputBadgeStyle::None => return None,
    };
    Some(format!("{raw} "))
}

/// Formats total turn tokens compactly (`842`, `12.4k`, `1.25M`).
#[must_use]
pub fn format_turn_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.2}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 10_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else if tokens >= 1_000 {
        format!("{:.2}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

/// Formats total turn elapsed seconds compactly (`420ms`, `18.4s`, `2m 14s`, `1h 04m`).
#[must_use]
pub fn format_turn_duration(secs: f64) -> String {
    let secs = secs.max(0.0);
    if secs < 1.0 {
        format!("{}ms", (secs * 1000.0).round() as u64)
    } else if secs < 60.0 {
        format!("{secs:.1}s")
    } else if secs < 3600.0 {
        let total = secs.round() as u64;
        format!("{}m {:02}s", total / 60, total % 60)
    } else {
        let total = secs.round() as u64;
        format!("{}h {:02}m", total / 3600, (total % 3600) / 60)
    }
}

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

/// Visual column width of a slice of `Span`s.
fn spans_visual_width(spans: &[Span<'_>]) -> usize {
    spans
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

/// Renders an assistant markdown entry with the chosen [`OutputFrameStyle`], [`OutputStyle`],
/// and [`OutputBadgeStyle`] (with optional `anim_frame` for live animated kaomoji/sparkle).
pub fn render_assistant_output_lines(
    text: &str,
    usable_width: usize,
    frame_style: OutputFrameStyle,
    output_style: OutputStyle,
    badge_style: OutputBadgeStyle,
    anim_frame: usize,
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
    let user_badge = resolve_output_badge(badge_style, anim_frame);

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
            let badge_pair = user_badge.as_deref().map(|b| (b, info_bold));
            let ascii_badge = if badge_style == OutputBadgeStyle::Sparkle {
                None
            } else {
                badge_pair
            };

            let top = match frame_style {
                OutputFrameStyle::OpenRounded | OutputFrameStyle::OpenTopOnly => {
                    build_framed_top_rule(
                        "╭",
                        "─",
                        "╮",
                        badge_pair,
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
                    if badge_style == OutputBadgeStyle::Sparkle {
                        None
                    } else {
                        badge_pair
                    },
                    Some(("minicode", title_style)),
                    frame_w,
                    border_style,
                ),
                OutputFrameStyle::OpenDouble => build_framed_top_rule(
                    "╔",
                    "═",
                    "╗",
                    if badge_style == OutputBadgeStyle::Sparkle {
                        None
                    } else {
                        badge_pair
                    },
                    Some(("minicode", title_style)),
                    frame_w,
                    border_style,
                ),
                OutputFrameStyle::AsciiFrame => build_framed_top_rule(
                    "+",
                    "-",
                    "+",
                    ascii_badge,
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

        // ── Full Enclosed Rounded Card (`╭─ <badge> minicode ───╮` / `│ body │` / `╰───╯`) ──
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
                user_badge.as_deref().map(|b| (b, info_bold)),
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

        // ── Left Accent Spine (`┃ <badge> minicode` / `┃ body`) ──
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
            let mut head_spans = vec![Span::styled("┃ ", rail_head)];
            if let Some(ref b) = user_badge {
                head_spans.push(Span::styled(b.clone(), info_bold));
            }
            head_spans.push(Span::styled("minicode", accent_bold));
            out.push(Line::from(head_spans));
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
                    let mut spans = vec![Span::styled("── ", border_style)];
                    let mut used = 3usize;
                    if let Some(ref b) = user_badge {
                        if badge_style != OutputBadgeStyle::Sparkle {
                            used += UnicodeWidthStr::width(b.as_str());
                            spans.push(Span::styled(b.clone(), info_bold));
                        }
                    }
                    used += UnicodeWidthStr::width("minicode ");
                    spans.push(Span::styled("minicode ", accent_bold));
                    let rem = frame_w.saturating_sub(used);
                    spans.push(Span::styled("─".repeat(rem), border_style));
                    Line::from(spans)
                }
                OutputFrameStyle::GradientRule => build_gradient_rule(frame_w, theme),
                OutputFrameStyle::PoshPill => {
                    let pill_bg = theme.bg_elevated;
                    let b_str = user_badge.as_deref().unwrap_or("");
                    let pill_w =
                        UnicodeWidthStr::width(b_str) + UnicodeWidthStr::width("minicode") + 3;
                    let rem = frame_w.saturating_sub(pill_w);
                    let mut spans = vec![Span::styled("\u{e0b6}", Style::default().fg(pill_bg))];
                    if !b_str.is_empty() {
                        spans.push(Span::styled(
                            b_str.to_string(),
                            Style::default()
                                .fg(theme.info)
                                .bg(pill_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }
                    spans.push(Span::styled(
                        "minicode",
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(pill_bg)
                            .add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled("\u{e0b4} ", Style::default().fg(pill_bg)));
                    spans.push(Span::styled("─".repeat(rem), border_style));
                    Line::from(spans)
                }
                OutputFrameStyle::PoshPowerline => {
                    let pill_bg = theme.bg_elevated;
                    let b_str = user_badge.as_deref().unwrap_or("");
                    let pill_w =
                        1 + UnicodeWidthStr::width(b_str) + UnicodeWidthStr::width("minicode ") + 2;
                    let rem = frame_w.saturating_sub(pill_w);
                    let mut spans = vec![Span::styled(" ", Style::default().bg(pill_bg))];
                    if !b_str.is_empty() {
                        spans.push(Span::styled(
                            b_str.to_string(),
                            Style::default()
                                .fg(theme.info)
                                .bg(pill_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }
                    spans.push(Span::styled(
                        "minicode ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(pill_bg)
                            .add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled("\u{e0b0} ", Style::default().fg(pill_bg)));
                    spans.push(Span::styled("─".repeat(rem), border_style));
                    Line::from(spans)
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

/// Builds the styled inline spans for an end-of-turn summary footer according to
/// [`TurnSummaryConfig`] (honoring all granular show/hide toggles and all 12 styles).
fn build_turn_summary_spans(
    metrics: &TurnSummaryMetrics<'_>,
    cfg: TurnSummaryConfig,
    theme: &Theme,
) -> Vec<Span<'static>> {
    struct Segment {
        icon_sym: &'static str,
        icon_nf: NerdIcon,
        icon_emoji: &'static str,
        label_verbose: &'static str,
        value: String,
        value_compact: String,
        color: Color,
    }

    let mut segs: Vec<Segment> = Vec::with_capacity(6);

    if cfg.show_tokens && metrics.tokens_used > 0 {
        let tok_num = format_turn_tokens(metrics.tokens_used);
        segs.push(Segment {
            icon_sym: "◈",
            icon_nf: NerdIcon::Chip,
            icon_emoji: "🪙",
            label_verbose: "tokens",
            value: match cfg.style {
                TurnSummaryStyle::Minimal | TurnSummaryStyle::Dotted => format!("{tok_num} tokens"),
                TurnSummaryStyle::Detailed => format!("tokens {tok_num}"),
                _ => format!("{tok_num} tok"),
            },
            value_compact: tok_num,
            color: theme.info,
        });
    }

    if cfg.show_duration && metrics.duration_secs >= 0.001 {
        let dur_str = format_turn_duration(metrics.duration_secs);
        segs.push(Segment {
            icon_sym: "◷",
            icon_nf: NerdIcon::Clock,
            icon_emoji: "⏱",
            label_verbose: "time",
            value: match cfg.style {
                TurnSummaryStyle::Detailed => format!("time {dur_str}"),
                _ => dur_str.clone(),
            },
            value_compact: dur_str,
            color: theme.brand_accent,
        });
    }

    if cfg.show_tools && metrics.tool_calls > 0 {
        let noun = if metrics.tool_calls == 1 {
            "tool"
        } else {
            "tools"
        };
        segs.push(Segment {
            icon_sym: "⚙",
            icon_nf: NerdIcon::Gear,
            icon_emoji: "🔧",
            label_verbose: "tools",
            value: format!("{} {noun}", metrics.tool_calls),
            value_compact: format!("{}t", metrics.tool_calls),
            color: theme.success,
        });
    }

    if cfg.show_kv_cache && metrics.cached_tokens > 0 && metrics.tokens_used > 0 {
        let pct = ((metrics.cached_tokens.min(metrics.tokens_used) as f64
            / metrics.tokens_used as f64)
            * 100.0)
            .round() as usize;
        segs.push(Segment {
            icon_sym: "▣",
            icon_nf: NerdIcon::Database,
            icon_emoji: "⚡",
            label_verbose: "kv",
            value: format!("kv {pct}%"),
            value_compact: format!("kv{pct}%"),
            color: theme.success,
        });
    }

    if cfg.show_cost && metrics.cost_usd > 0.00005 {
        let cost_str = if metrics.cost_usd < 0.01 {
            format!("${:.4}", metrics.cost_usd)
        } else {
            format!("${:.3}", metrics.cost_usd)
        };
        segs.push(Segment {
            icon_sym: "$",
            icon_nf: NerdIcon::Bolt,
            icon_emoji: "💵",
            label_verbose: "cost",
            value: cost_str.clone(),
            value_compact: cost_str,
            color: theme.warning,
        });
    }

    if cfg.show_model && !metrics.model.trim().is_empty() {
        let short_model = metrics
            .model
            .rsplit('/')
            .next()
            .unwrap_or(metrics.model)
            .to_string();
        segs.push(Segment {
            icon_sym: "✦",
            icon_nf: NerdIcon::Bot,
            icon_emoji: "🤖",
            label_verbose: "model",
            value: short_model.clone(),
            value_compact: short_model,
            color: theme.muted,
        });
    }

    if segs.is_empty() {
        return Vec::new();
    }

    let muted_st = Style::default().fg(theme.muted);
    let border_st = Style::default().fg(theme.border);
    let pill_bg = theme.bg_elevated;
    let mut out: Vec<Span<'static>> = Vec::with_capacity(segs.len() * 4 + 4);

    match cfg.style {
        TurnSummaryStyle::Minimal => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" · ", border_st));
                }
                out.push(Span::styled(seg.value.clone(), muted_st));
            }
        }
        TurnSummaryStyle::Compact => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" · ", border_st));
                }
                out.push(Span::styled(seg.value_compact.clone(), muted_st));
            }
        }
        TurnSummaryStyle::Detailed => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled("  ·  ", border_st));
                }
                if seg.value.starts_with(seg.label_verbose) {
                    out.push(Span::styled(
                        format!("{} ", seg.label_verbose),
                        Style::default().fg(theme.muted),
                    ));
                    let rest = seg
                        .value
                        .strip_prefix(seg.label_verbose)
                        .unwrap_or(&seg.value)
                        .trim_start();
                    out.push(Span::styled(
                        rest.to_string(),
                        Style::default().fg(seg.color).add_modifier(Modifier::BOLD),
                    ));
                } else {
                    out.push(Span::styled(
                        seg.value.clone(),
                        Style::default().fg(seg.color).add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
        TurnSummaryStyle::Symbols => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" · ", border_st));
                }
                out.push(Span::styled(
                    format!("{} ", seg.icon_sym),
                    Style::default().fg(seg.color),
                ));
                out.push(Span::styled(seg.value.clone(), muted_st));
            }
        }
        TurnSummaryStyle::NerdFont => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" · ", border_st));
                }
                out.push(Span::styled(
                    format!("{} ", nf(seg.icon_nf)),
                    Style::default().fg(seg.color),
                ));
                out.push(Span::styled(seg.value.clone(), muted_st));
            }
        }
        TurnSummaryStyle::PoshPill => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::raw(" "));
                }
                out.push(Span::styled("\u{e0b6}", Style::default().fg(pill_bg)));
                out.push(Span::styled(
                    format!("{} ", seg.icon_sym),
                    Style::default()
                        .fg(seg.color)
                        .bg(pill_bg)
                        .add_modifier(Modifier::BOLD),
                ));
                out.push(Span::styled(
                    seg.value.clone(),
                    Style::default().fg(theme.text_primary).bg(pill_bg),
                ));
                out.push(Span::styled("\u{e0b4}", Style::default().fg(pill_bg)));
            }
        }
        TurnSummaryStyle::PoshPowerline => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::raw(" "));
                }
                out.push(Span::styled(
                    format!(" {} ", seg.icon_sym),
                    Style::default()
                        .fg(seg.color)
                        .bg(pill_bg)
                        .add_modifier(Modifier::BOLD),
                ));
                out.push(Span::styled(
                    format!("{} ", seg.value),
                    Style::default().fg(theme.text_primary).bg(pill_bg),
                ));
                out.push(Span::styled("\u{e0b0}", Style::default().fg(pill_bg)));
            }
        }
        TurnSummaryStyle::Brackets => {
            out.push(Span::styled("[", border_st));
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" | ", border_st));
                }
                out.push(Span::styled(
                    seg.value.clone(),
                    Style::default().fg(seg.color),
                ));
            }
            out.push(Span::styled("]", border_st));
        }
        TurnSummaryStyle::Emoji => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" · ", border_st));
                }
                out.push(Span::styled(
                    format!("{} ", seg.icon_emoji),
                    Style::default().fg(seg.color),
                ));
                out.push(Span::styled(seg.value.clone(), muted_st));
            }
        }
        TurnSummaryStyle::Kaomoji => {
            out.push(Span::styled(
                "(•‿•) ",
                Style::default().fg(theme.brand_accent),
            ));
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" · ", border_st));
                }
                out.push(Span::styled(seg.value.clone(), muted_st));
            }
        }
        TurnSummaryStyle::Dotted => {
            out.push(Span::styled("· ", border_st));
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" · ", border_st));
                }
                out.push(Span::styled(seg.value.clone(), muted_st));
            }
            out.push(Span::styled(" ·", border_st));
        }
        TurnSummaryStyle::Ascii => {
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    out.push(Span::styled(" | ", border_st));
                }
                out.push(Span::styled(seg.value.clone(), muted_st));
            }
        }
    }

    out
}

/// Renders the end-of-turn summary footer with a clean breathing spacer row above and alignment
/// controlled by [`TurnSummaryConfig::position`] (`BottomRight` default).
pub fn render_turn_summary_lines(
    metrics: &TurnSummaryMetrics<'_>,
    usable_width: usize,
    cfg: TurnSummaryConfig,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if !cfg.enabled || cfg.position == TurnSummaryPosition::Hidden {
        return Vec::new();
    }
    let content_spans = build_turn_summary_spans(metrics, cfg, theme);
    if content_spans.is_empty() {
        return Vec::new();
    }

    let w = usable_width.max(20);
    let content_w = spans_visual_width(&content_spans);
    let border_st = Style::default().fg(theme.border);

    let line = match cfg.position {
        TurnSummaryPosition::Hidden => return Vec::new(),
        TurnSummaryPosition::BottomRight => {
            let pad = w.saturating_sub(content_w);
            let mut spans = Vec::with_capacity(content_spans.len() + 1);
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
            spans.extend(content_spans);
            Line::from(spans)
        }
        TurnSummaryPosition::BottomLeft => Line::from(content_spans),
        TurnSummaryPosition::BottomCenter => {
            let pad = w.saturating_sub(content_w) / 2;
            let mut spans = Vec::with_capacity(content_spans.len() + 1);
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
            spans.extend(content_spans);
            Line::from(spans)
        }
        TurnSummaryPosition::BottomRuleRight => {
            let trail = " ──";
            let trail_w = UnicodeWidthStr::width(trail);
            let lead_w = w.saturating_sub(content_w + trail_w + 1);
            let mut spans = Vec::with_capacity(content_spans.len() + 3);
            if lead_w > 0 {
                spans.push(Span::styled("─".repeat(lead_w), border_st));
                spans.push(Span::raw(" "));
            }
            spans.extend(content_spans);
            spans.push(Span::styled(trail, border_st));
            Line::from(spans)
        }
        TurnSummaryPosition::BottomRuleLeft => {
            let lead = "── ";
            let lead_w = UnicodeWidthStr::width(lead);
            let trail_w = w.saturating_sub(lead_w + content_w + 1);
            let mut spans = Vec::with_capacity(content_spans.len() + 3);
            spans.push(Span::styled(lead, border_st));
            spans.extend(content_spans);
            if trail_w > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled("─".repeat(trail_w), border_st));
            }
            Line::from(spans)
        }
    };

    // Slight vertical breathing space below the assistant's final line, then the summary footer
    vec![Line::from(String::new()), line]
}

/// Renders a live `/theme` preview showcasing the selected [`OutputFrameStyle`] with the active
/// [`OutputStyle`] and [`OutputBadgeStyle`].
pub fn render_output_frame_preview(
    frame_style: OutputFrameStyle,
    output_style: OutputStyle,
    badge_style: OutputBadgeStyle,
    anim_frame: usize,
    preview_w: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let sample = "Page is live at `http://127.0.0.1:4173` — **12/12** checks passed in 48ms.\n\n- [x] Verify `src/ui/view.rs` layout alignment\n- [ ] Ready for your next command";
    let w = preview_w.clamp(36, 76);
    render_assistant_output_lines(
        sample,
        w,
        frame_style,
        output_style,
        badge_style,
        anim_frame,
        theme,
    )
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
    badge_style: OutputBadgeStyle,
    anim_frame: usize,
    preview_w: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let sample = "\
## Release Summary
> [!TIP] All 233 tests passed in 1.64s.
- [x] Verify `src/ui/icons.rs` fallback (**v0.5.76**)
```rust
let ok = render_output(style);
```
| Check | Latency | Status |
| --- | --- | --- |
| Clippy | 142ms | Passed |";
    let w = preview_w.clamp(40, 76);
    render_assistant_output_lines(
        sample,
        w,
        frame_style,
        output_style,
        badge_style,
        anim_frame,
        theme,
    )
    .into_iter()
    .map(|l| {
        let mut spans = Vec::with_capacity(l.spans.len() + 1);
        spans.push(Span::raw("  "));
        spans.extend(l.spans);
        Line::from(spans)
    })
    .collect()
}

/// Renders a live `/theme` preview showcasing the selected [`OutputBadgeStyle`] inside both the
/// open rounded bracket (`╭─ <badge> minicode ──╮`) and the Oh My Posh pill (`<badge> minicode ──`).
pub fn render_output_badge_preview(
    badge_style: OutputBadgeStyle,
    active_frame: OutputFrameStyle,
    anim_frame: usize,
    preview_w: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let w = preview_w.clamp(38, 72);
    let sample = "Ready — 12/12 checks passed cleanly in 48ms.";
    let primary_frame = match active_frame {
        OutputFrameStyle::None
        | OutputFrameStyle::FullRule
        | OutputFrameStyle::MediumRule
        | OutputFrameStyle::ShortRule
        | OutputFrameStyle::GradientRule
        | OutputFrameStyle::DashedRule
        | OutputFrameStyle::DottedRule
        | OutputFrameStyle::OpenRoundedMinimal => OutputFrameStyle::OpenRounded,
        other => other,
    };
    let mut out = render_assistant_output_lines(
        sample,
        w,
        primary_frame,
        OutputStyle::Default,
        badge_style,
        anim_frame,
        theme,
    );
    out.push(Line::from(String::new()));
    let alt_frame = if primary_frame == OutputFrameStyle::PoshPill {
        OutputFrameStyle::OpenRounded
    } else {
        OutputFrameStyle::PoshPill
    };
    let alt_lines = render_assistant_output_lines(
        "Also updates Oh My Posh pills, Left Rail, Full Card & Powerline headers.",
        w,
        alt_frame,
        OutputStyle::Default,
        badge_style,
        anim_frame,
        theme,
    );
    out.extend(alt_lines);

    out.into_iter()
        .map(|l| {
            let mut spans = Vec::with_capacity(l.spans.len() + 1);
            spans.push(Span::raw("  "));
            spans.extend(l.spans);
            Line::from(spans)
        })
        .collect()
}

/// Renders a live `/theme` preview showcasing the selected [`TurnSummaryStyle`] &
/// [`TurnSummaryPosition`] below a realistic miniature response with breathing room.
pub fn render_turn_summary_preview(
    summary_cfg: TurnSummaryConfig,
    active_frame: OutputFrameStyle,
    active_badge: OutputBadgeStyle,
    preview_w: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let w = preview_w.clamp(40, 74);
    let sample = "Fixed `src/ui/view.rs` and verified all **233** tests pass.";
    let mut out = render_assistant_output_lines(
        sample,
        w,
        active_frame,
        OutputStyle::Default,
        active_badge,
        0,
        theme,
    );

    let metrics = TurnSummaryMetrics {
        tokens_used: 12_480,
        cached_tokens: 11_480,
        duration_secs: 18.4,
        tool_calls: 4,
        cost_usd: 0.014,
        model: "minimax/MiniMax-M3.1-Flash",
    };

    let footer = render_turn_summary_lines(&metrics, w, summary_cfg, theme);
    if footer.is_empty() {
        out.push(Line::from(String::new()));
        out.push(Line::from(vec![Span::styled(
            "(Turn Summary hidden — press 'p' to change placement or enable in 20. Show / Hide)",
            Style::default().fg(theme.muted),
        )]));
    } else {
        out.extend(footer);
    }

    out.into_iter()
        .map(|l| {
            let mut spans = Vec::with_capacity(l.spans.len() + 1);
            spans.push(Span::raw("  "));
            spans.extend(l.spans);
            Line::from(spans)
        })
        .collect()
}
