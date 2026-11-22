use crate::constants::GIT_BRANCH_CACHE_TTL_SECS;
use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static BRANCH_CACHE: Mutex<Option<(PathBuf, Instant, Option<String>)>> = Mutex::new(None);
static IS_FETCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static HOME_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusDisplayOptions {
    pub status_bar_style: crate::config::StatusBarStyle,
    pub context_meter_style: crate::config::ContextMeterStyle,
    pub show_kv_cache: bool,
    pub show_context_meter: bool,
    pub show_status_model: bool,
    pub show_status_effort: bool,
    pub show_status_path: bool,
    pub show_status_git: bool,
    pub show_status_badges: bool,
}

impl Default for StatusDisplayOptions {
    fn default() -> Self {
        Self {
            status_bar_style: crate::config::StatusBarStyle::Classic,
            context_meter_style: crate::config::ContextMeterStyle::Compact,
            show_kv_cache: true,
            show_context_meter: true,
            show_status_model: true,
            show_status_effort: true,
            show_status_path: true,
            show_status_git: true,
            show_status_badges: true,
        }
    }
}

impl StatusDisplayOptions {
    #[must_use]
    pub fn from_ui_config(ui: &crate::config::UiConfig) -> Self {
        Self {
            status_bar_style: ui.status_bar_style_enum(),
            context_meter_style: ui.context_meter_style_enum(),
            show_kv_cache: ui.show_kv_cache,
            show_context_meter: ui.show_context_meter,
            show_status_model: ui.show_status_model,
            show_status_effort: ui.show_status_effort,
            show_status_path: ui.show_status_path,
            show_status_git: ui.show_status_git,
            show_status_badges: ui.show_status_badges,
        }
    }
}

pub struct StatusContext<'a> {
    pub theme: &'a Theme,
    pub workspace: &'a Path,
    pub provider: &'a str,
    pub model: &'a str,
    pub effort_level: &'a str,
    pub mcp_count: usize,
    pub used_tokens: usize,
    pub max_context: usize,
    pub show_cost: bool,
    pub session_cost_usd: f64,
    pub cached_tokens: usize,
    pub tasks_summary: Option<(usize, f32, f32)>,
    pub display: StatusDisplayOptions,
}

pub struct StatusWidgets;

impl StatusWidgets {
    pub fn get_git_branch(workspace: &Path) -> Option<String> {
        let now = Instant::now();
        let mut cached_branch = None;
        let mut needs_refresh = true;

        if let Ok(guard) = BRANCH_CACHE.lock() {
            if let Some((ref cached_ws, last_check, ref branch)) = *guard {
                if cached_ws.as_path() == workspace {
                    cached_branch = branch.clone();
                    if now.duration_since(last_check)
                        < Duration::from_secs(GIT_BRANCH_CACHE_TTL_SECS)
                    {
                        needs_refresh = false;
                    }
                }
            }
        }

        if needs_refresh && !IS_FETCHING.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let ws = workspace.to_path_buf();
            let fetcher = move || {
                let output = std::process::Command::new("git")
                    .arg("rev-parse")
                    .arg("--abbrev-ref")
                    .arg("HEAD")
                    .current_dir(&ws)
                    .output();

                let branch = output.ok().and_then(|out| {
                    if out.status.success() {
                        let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        (!b.is_empty()).then_some(b)
                    } else {
                        None
                    }
                });

                if let Ok(mut guard) = BRANCH_CACHE.lock() {
                    *guard = Some((ws, Instant::now(), branch));
                }
                IS_FETCHING.store(false, std::sync::atomic::Ordering::SeqCst);
            };

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn_blocking(fetcher);
            } else {
                std::thread::spawn(fetcher);
            }
        }

        cached_branch
    }

    /// Explicitly invalidates git branch cache to force an immediate refresh
    #[allow(dead_code)]
    pub fn invalidate_git_cache() {
        if let Ok(mut guard) = BRANCH_CACHE.lock() {
            *guard = None;
        }
    }

    /// Formats a token count into a human-readable string (e.g. 450, 4.2k, 128k, 1.0M)
    pub fn format_tokens(n: usize) -> String {
        if n >= 1_000_000 {
            let val = n as f64 / 1_000_000.0;
            if (val - val.floor()).abs() < 0.05 {
                format!("{:.0}M", val)
            } else {
                format!("{:.1}M", val)
            }
        } else if n >= 1_000 {
            let val = n as f64 / 1_000.0;
            if (val - val.floor()).abs() < 0.05 {
                format!("{:.0}k", val)
            } else {
                format!("{:.1}k", val)
            }
        } else {
            format!("{}", n)
        }
    }

    /// Dynamically shortens a path string to fit within `max_chars` while keeping the final directory segments readable.
    fn compact_path_to_budget(path: &str, max_chars: usize) -> String {
        let len = unicode_width::UnicodeWidthStr::width(path);
        if len <= max_chars {
            return path.to_string();
        }
        if max_chars <= 4 {
            return "…".to_string();
        }
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if let Some(&last) = parts.last() {
            if parts.len() >= 2 {
                let last_two = format!("…/{}/{}", parts[parts.len() - 2], last);
                if unicode_width::UnicodeWidthStr::width(last_two.as_str()) <= max_chars {
                    return last_two;
                }
            }
            let last_one = format!("…/{}", last);
            if unicode_width::UnicodeWidthStr::width(last_one.as_str()) <= max_chars {
                return last_one;
            }
        }
        let keep = max_chars.saturating_sub(1);
        let tail: String = path
            .chars()
            .rev()
            .take(keep)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("…{}", tail)
    }

    /// Renders the right-hand Context Occupancy, KV Cache, and Session Cost telemetry spans
    /// according to the active `ContextMeterStyle` and visibility flags.
    #[allow(clippy::too_many_arguments)]
    pub fn render_context_meter_spans(
        theme: &Theme,
        used_tokens: usize,
        max_context: usize,
        cached_tokens: usize,
        show_cost: bool,
        session_cost_usd: f64,
        show_kv_cache: bool,
        show_context_meter: bool,
        style: crate::config::ContextMeterStyle,
    ) -> Vec<Span<'static>> {
        use crate::config::ContextMeterStyle;

        let used_str = Self::format_tokens(used_tokens);
        let max_str = Self::format_tokens(max_context);
        let ratio = if max_context > 0 {
            ((used_tokens as f64) / (max_context as f64)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let pct = (ratio * 100.0).round() as usize;

        let used_color = if ratio > 0.85 {
            theme.destructive
        } else if ratio > 0.60 {
            theme.warning
        } else {
            theme.success
        };

        let kv_hit_pct = if show_kv_cache && cached_tokens > 0 {
            let hit = if used_tokens > 0 {
                ((cached_tokens as f64 / used_tokens as f64) * 100.0).clamp(0.0, 100.0) as usize
            } else {
                100
            };
            Some(hit)
        } else {
            None
        };

        let pie_glyph = |r: f64| -> &'static str {
            if r <= 0.02 {
                "○"
            } else if r < 0.30 {
                "◔"
            } else if r < 0.60 {
                "◑"
            } else if r < 0.85 {
                "◕"
            } else {
                "●"
            }
        };

        let kv_color = |hit: usize| {
            if hit >= 70 {
                theme.success
            } else if hit >= 30 {
                theme.info
            } else {
                theme.muted
            }
        };

        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut has_prev = false;

        let push_sep = |spans: &mut Vec<Span<'static>>, has_prev: &mut bool, sep: &'static str| {
            if *has_prev {
                spans.push(Span::styled(sep, Style::default().fg(theme.muted)));
            }
            *has_prev = true;
        };

        if show_cost {
            let cost_str = crate::agent::pricing::ModelPricing::format_cost(session_cost_usd);
            match style {
                ContextMeterStyle::Pills => {
                    spans.push(Span::styled(
                        format!(" {} ", cost_str),
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated)
                            .add_modifier(Modifier::BOLD),
                    ));
                    has_prev = true;
                }
                _ => {
                    spans.push(Span::styled(
                        cost_str,
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ));
                    has_prev = true;
                }
            }
        }

        if let Some(hit_pct) = kv_hit_pct {
            match style {
                ContextMeterStyle::Compact => {
                    push_sep(&mut spans, &mut has_prev, " • ");
                    spans.push(Span::styled(
                        format!("kv ({}%)", hit_pct),
                        Style::default().fg(theme.muted),
                    ));
                }
                ContextMeterStyle::Circle => {
                    push_sep(&mut spans, &mut has_prev, " · ");
                    spans.push(Span::styled(
                        format!("{} ", pie_glyph(hit_pct as f64 / 100.0)),
                        Style::default().fg(kv_color(hit_pct)),
                    ));
                    spans.push(Span::styled(
                        format!("kv {}%", hit_pct),
                        Style::default().fg(theme.muted),
                    ));
                }
                ContextMeterStyle::Bar
                | ContextMeterStyle::Blocks
                | ContextMeterStyle::Braille
                | ContextMeterStyle::Percent
                | ContextMeterStyle::Gauge => {
                    push_sep(&mut spans, &mut has_prev, " • ");
                    spans.push(Span::styled("kv ", Style::default().fg(theme.muted)));
                    spans.push(Span::styled(
                        format!("{}%", hit_pct),
                        Style::default().fg(kv_color(hit_pct)),
                    ));
                }
                ContextMeterStyle::Pills => {
                    if has_prev {
                        spans.push(Span::raw(" "));
                    }
                    has_prev = true;
                    spans.push(Span::styled(
                        format!(" kv {}% ", hit_pct),
                        Style::default().fg(kv_color(hit_pct)).bg(theme.bg_elevated),
                    ));
                }
                ContextMeterStyle::Minimal => {
                    push_sep(&mut spans, &mut has_prev, " · ");
                    spans.push(Span::styled(
                        format!("{}% kv", hit_pct),
                        Style::default().fg(theme.muted),
                    ));
                }
            }
        }

        if show_context_meter {
            match style {
                ContextMeterStyle::Compact => {
                    push_sep(&mut spans, &mut has_prev, " • ");
                    spans.push(Span::styled(
                        used_str,
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(" / ", Style::default().fg(theme.muted)));
                    spans.push(Span::styled(max_str, Style::default().fg(theme.muted)));
                }
                ContextMeterStyle::Circle => {
                    push_sep(&mut spans, &mut has_prev, " · ");
                    spans.push(Span::styled(
                        format!("{} ", pie_glyph(ratio)),
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(
                        format!("{}%", pct),
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                }
                ContextMeterStyle::Bar => {
                    push_sep(&mut spans, &mut has_prev, " • ");
                    let width = 10usize;
                    let filled = ((ratio * width as f64).round() as usize).min(width);
                    let empty = width.saturating_sub(filled);
                    spans.push(Span::styled("[", Style::default().fg(theme.border)));
                    if filled > 0 {
                        spans.push(Span::styled(
                            "█".repeat(filled),
                            Style::default().fg(used_color),
                        ));
                    }
                    if empty > 0 {
                        spans.push(Span::styled(
                            "░".repeat(empty),
                            Style::default().fg(theme.border),
                        ));
                    }
                    spans.push(Span::styled("] ", Style::default().fg(theme.border)));
                    spans.push(Span::styled(
                        format!("{}%", pct),
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                }
                ContextMeterStyle::Blocks => {
                    push_sep(&mut spans, &mut has_prev, " • ");
                    let width = 8usize;
                    let scaled = ratio * width as f64;
                    let full = (scaled.floor() as usize).min(width);
                    let frac = scaled - (full as f64);
                    let mut filled_str = "█".repeat(full);
                    let mut partial_added = false;
                    if full < width && frac >= 0.25 {
                        filled_str.push(if frac >= 0.70 { '▓' } else { '▒' });
                        partial_added = true;
                    }
                    let rem = width.saturating_sub(full + usize::from(partial_added));
                    if !filled_str.is_empty() {
                        spans.push(Span::styled(filled_str, Style::default().fg(used_color)));
                    }
                    if rem > 0 {
                        spans.push(Span::styled(
                            "░".repeat(rem),
                            Style::default().fg(theme.border),
                        ));
                    }
                    spans.push(Span::styled(
                        format!(" {}%", pct),
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                }
                ContextMeterStyle::Braille => {
                    push_sep(&mut spans, &mut has_prev, " • ");
                    const LEVELS: [char; 5] = ['⠀', '⣀', '⣤', '⣶', '⣿'];
                    let cells = 6usize;
                    let total_steps = cells * 4;
                    let steps = ((ratio * total_steps as f64).round() as usize).min(total_steps);
                    let mut active_part = String::new();
                    let mut empty_part = String::new();
                    for i in 0..cells {
                        let cell_steps = steps.saturating_sub(i * 4).min(4);
                        if cell_steps > 0 {
                            active_part.push(LEVELS[cell_steps]);
                        } else {
                            empty_part.push('⣀');
                        }
                    }
                    if !active_part.is_empty() {
                        spans.push(Span::styled(active_part, Style::default().fg(used_color)));
                    }
                    if !empty_part.is_empty() {
                        spans.push(Span::styled(empty_part, Style::default().fg(theme.border)));
                    }
                    spans.push(Span::styled(
                        format!(" {}%", pct),
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                }
                ContextMeterStyle::Percent => {
                    push_sep(&mut spans, &mut has_prev, " • ");
                    spans.push(Span::styled("ctx ", Style::default().fg(theme.muted)));
                    spans.push(Span::styled(
                        format!("{}%", pct),
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                }
                ContextMeterStyle::Pills => {
                    if has_prev {
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::styled(
                        format!(" {} {}% ", pie_glyph(ratio), pct),
                        Style::default()
                            .fg(used_color)
                            .bg(theme.bg_elevated)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                ContextMeterStyle::Minimal => {
                    push_sep(&mut spans, &mut has_prev, " · ");
                    spans.push(Span::styled(
                        used_str,
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(
                        format!("/{}", max_str),
                        Style::default().fg(theme.muted),
                    ));
                }
                ContextMeterStyle::Gauge => {
                    push_sep(&mut spans, &mut has_prev, " • ");
                    let width = 10usize;
                    let filled = ((ratio * width as f64).round() as usize).min(width);
                    let empty = width.saturating_sub(filled);
                    if filled > 0 {
                        spans.push(Span::styled(
                            "━".repeat(filled),
                            Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                        ));
                    }
                    if empty > 0 {
                        spans.push(Span::styled(
                            "─".repeat(empty),
                            Style::default().fg(theme.border),
                        ));
                    }
                    spans.push(Span::styled(
                        format!(" {}%", pct),
                        Style::default().fg(used_color).add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }

        if !spans.is_empty() {
            spans.push(Span::styled(" ", Style::default()));
        }
        spans
    }

    /// Softly blends `accent` into `base` by `ratio` (0.0..=1.0) for Oh My Posh velvety tinted
    /// segment backgrounds that harmonize automatically with both dark and light themes.
    fn tint_bg(
        accent: ratatui::style::Color,
        base: ratatui::style::Color,
        fallback: ratatui::style::Color,
        ratio: f32,
    ) -> ratatui::style::Color {
        use ratatui::style::Color;
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

    /// Renders the right-aligned Mode Badges strip (`· ponytail:full | caveman:full | streaming`)
    /// on `theme.bg_primary` according to the active `ModeBarStyle` and per-badge visibility toggles.
    #[allow(clippy::too_many_arguments)]
    pub fn render_mode_bar_spans(
        theme: &Theme,
        ponytail_active: bool,
        ponytail_level: &str,
        caveman_active: bool,
        caveman_level: &str,
        streaming_active: bool,
        style: crate::config::ModeBarStyle,
        show_ponytail: bool,
        show_caveman: bool,
        show_streaming: bool,
    ) -> Vec<Span<'static>> {
        Self::render_mode_bar_spans_with_bg(
            theme,
            theme.bg_primary,
            ponytail_active,
            ponytail_level,
            caveman_active,
            caveman_level,
            streaming_active,
            style,
            show_ponytail,
            show_caveman,
            show_streaming,
        )
    }

    /// Renders the right-aligned Mode Badges strip with an explicit surrounding `bar_bg` so
    /// Oh My Posh caps (``, ``, ``, ``) blend seamlessly in both the live bar and `/theme` previews.
    #[allow(clippy::too_many_arguments)]
    pub fn render_mode_bar_spans_with_bg(
        theme: &Theme,
        bar_bg: ratatui::style::Color,
        ponytail_active: bool,
        ponytail_level: &str,
        caveman_active: bool,
        caveman_level: &str,
        streaming_active: bool,
        style: crate::config::ModeBarStyle,
        show_ponytail: bool,
        show_caveman: bool,
        show_streaming: bool,
    ) -> Vec<Span<'static>> {
        use crate::config::ModeBarStyle;

        if style == ModeBarStyle::Hidden || (!show_ponytail && !show_caveman && !show_streaming) {
            return Vec::new();
        }

        let pt_lvl = crate::agent::ponytail::PonytailLevel::from_str_active(ponytail_level);
        let cm_lvl = crate::agent::caveman::CavemanLevel::from_str_active(caveman_level);

        struct BadgeItem {
            full: String,
            posh: String,
            short: String,
            icon: &'static str,
            active: bool,
            color: ratatui::style::Color,
        }

        let mut items: Vec<BadgeItem> = Vec::with_capacity(3);
        if show_ponytail {
            items.push(BadgeItem {
                full: if ponytail_active {
                    format!("ponytail:{}", pt_lvl.as_str())
                } else {
                    "ponytail:off".to_string()
                },
                posh: if ponytail_active {
                    format!("ponytail·{}", pt_lvl.as_str())
                } else {
                    "ponytail".to_string()
                },
                short: if ponytail_active {
                    format!("pt:{}", pt_lvl.as_str())
                } else {
                    "pt:off".to_string()
                },
                icon: "✦",
                active: ponytail_active,
                color: theme.brand_accent,
            });
        }
        if show_caveman {
            items.push(BadgeItem {
                full: if caveman_active {
                    format!("caveman:{}", cm_lvl.as_str())
                } else {
                    "caveman:off".to_string()
                },
                posh: if caveman_active {
                    format!("caveman·{}", cm_lvl.as_str())
                } else {
                    "caveman".to_string()
                },
                short: if caveman_active {
                    format!("cm:{}", cm_lvl.as_str())
                } else {
                    "cm:off".to_string()
                },
                icon: "◈",
                active: caveman_active,
                color: theme.warning,
            });
        }
        if show_streaming {
            items.push(BadgeItem {
                full: "streaming".to_string(),
                posh: "stream".to_string(),
                short: "strm".to_string(),
                icon: "⚡",
                active: streaming_active,
                color: theme.success,
            });
        }

        let item_bg = |item: &BadgeItem| -> ratatui::style::Color {
            if item.active {
                Self::tint_bg(item.color, bar_bg, theme.bg_elevated, 0.20)
            } else {
                Self::tint_bg(theme.muted, bar_bg, theme.bg_elevated, 0.10)
            }
        };

        let mut spans: Vec<Span<'static>> = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            let fg_style = if item.active {
                Style::default().fg(item.color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };

            match style {
                ModeBarStyle::Classic => {
                    if idx == 0 {
                        spans.push(Span::styled(format!("· {}", item.full), fg_style));
                    } else {
                        spans.push(Span::styled(" | ", Style::default().fg(theme.muted)));
                        spans.push(Span::styled(item.full.clone(), fg_style));
                    }
                }
                ModeBarStyle::PoshPure => {
                    if idx > 0 {
                        spans.push(Span::raw("  "));
                    }
                    if item.active {
                        spans.push(Span::styled("• ", Style::default().fg(item.color)));
                        spans.push(Span::styled(
                            item.posh.clone(),
                            Style::default().fg(theme.text_primary),
                        ));
                    } else {
                        spans.push(Span::styled("◦ ", Style::default().fg(theme.border)));
                        spans.push(Span::styled(
                            item.posh.clone(),
                            Style::default().fg(theme.muted),
                        ));
                    }
                }
                ModeBarStyle::PoshSlash => {
                    if idx > 0 {
                        spans.push(Span::styled(" / ", Style::default().fg(theme.border)));
                    }
                    let txt_st = if item.active {
                        Style::default().fg(item.color)
                    } else {
                        Style::default().fg(theme.muted)
                    };
                    spans.push(Span::styled(item.posh.clone(), txt_st));
                }
                ModeBarStyle::PoshVelvet => {
                    if idx > 0 {
                        spans.push(Span::styled(" ", Style::default().bg(bar_bg)));
                    }
                    let seg_bg = item_bg(item);
                    let rail_fg = if item.active {
                        item.color
                    } else {
                        theme.border
                    };
                    spans.push(Span::styled("▎", Style::default().fg(rail_fg).bg(seg_bg)));
                    spans.push(Span::styled(
                        format!(" {} ", item.posh),
                        fg_style.bg(seg_bg),
                    ));
                }
                ModeBarStyle::Pills => {
                    if idx > 0 {
                        spans.push(Span::styled(" ", Style::default().bg(bar_bg)));
                    }
                    let seg_bg = item_bg(item);
                    spans.push(Span::styled(
                        format!(" {} ", item.posh),
                        fg_style.bg(seg_bg),
                    ));
                }
                ModeBarStyle::PoshBubbles => {
                    if idx > 0 {
                        spans.push(Span::styled(" ", Style::default().bg(bar_bg)));
                    }
                    let seg_bg = item_bg(item);
                    let cap_style = Style::default().fg(seg_bg).bg(bar_bg);
                    spans.push(Span::styled("\u{e0b6}", cap_style));
                    spans.push(Span::styled(item.posh.clone(), fg_style.bg(seg_bg)));
                    spans.push(Span::styled("\u{e0b4}", cap_style));
                }
                ModeBarStyle::PoshPowerline | ModeBarStyle::PoshSlant => {
                    let sep_glyph = if style == ModeBarStyle::PoshSlant {
                        "\u{e0ba}" //  right-aligned diagonal slant
                    } else {
                        "\u{e0b2}" //  right-aligned powerline arrow
                    };
                    let seg_bg = item_bg(item);
                    let prev_bg = if idx == 0 {
                        bar_bg
                    } else {
                        item_bg(&items[idx - 1])
                    };
                    spans.push(Span::styled(
                        sep_glyph,
                        Style::default().fg(seg_bg).bg(prev_bg),
                    ));
                    spans.push(Span::styled(
                        format!(" {} ", item.posh),
                        fg_style.bg(seg_bg),
                    ));
                }
                ModeBarStyle::Dots => {
                    if idx > 0 {
                        spans.push(Span::raw("  "));
                    }
                    let dot = if item.active { "● " } else { "○ " };
                    spans.push(Span::styled(dot, fg_style));
                    spans.push(Span::styled(item.full.clone(), fg_style));
                }
                ModeBarStyle::Compact => {
                    if idx > 0 {
                        spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
                    }
                    spans.push(Span::styled(item.short.clone(), fg_style));
                }
                ModeBarStyle::Minimal => {
                    if idx > 0 {
                        spans.push(Span::raw("  "));
                    }
                    spans.push(Span::styled(item.full.clone(), fg_style));
                }
                ModeBarStyle::Brackets => {
                    if idx > 0 {
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::styled("[", Style::default().fg(theme.border)));
                    spans.push(Span::styled(item.full.clone(), fg_style));
                    spans.push(Span::styled("]", Style::default().fg(theme.border)));
                }
                ModeBarStyle::Icons => {
                    if idx > 0 {
                        spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
                    }
                    spans.push(Span::styled(format!("{} ", item.icon), fg_style));
                    spans.push(Span::styled(item.full.clone(), fg_style));
                }
                ModeBarStyle::Hidden => {}
            }
        }

        if !spans.is_empty() {
            spans.push(Span::styled(" ", Style::default().bg(bar_bg)));
        }
        spans
    }

    /// Renders the left-hand bottom Status Bar telemetry spans on `theme.bg_primary` according
    /// to the active `StatusBarStyle` and per-segment visibility flags.
    #[allow(clippy::too_many_arguments)]
    pub fn render_status_left_spans(
        theme: &Theme,
        provider: &str,
        model: &str,
        effort_level: &str,
        display_path: &str,
        git_branch: Option<&str>,
        mcp_count: usize,
        swarm_active: usize,
        tasks_summary: (usize, f32, f32),
        web_badge: Option<&str>,
        left_budget: usize,
        display: &StatusDisplayOptions,
    ) -> Vec<Span<'static>> {
        Self::render_status_left_spans_with_bg(
            theme,
            theme.bg_primary,
            provider,
            model,
            effort_level,
            display_path,
            git_branch,
            mcp_count,
            swarm_active,
            tasks_summary,
            web_badge,
            left_budget,
            display,
        )
    }

    /// Renders the left-hand bottom Status Bar telemetry spans with an explicit surrounding `bar_bg`
    /// so Oh My Posh caps (``, ``, ``, ``, ``) blend seamlessly in both the live bar and `/theme`.
    #[allow(clippy::too_many_arguments)]
    pub fn render_status_left_spans_with_bg(
        theme: &Theme,
        bar_bg: ratatui::style::Color,
        provider: &str,
        model: &str,
        effort_level: &str,
        display_path: &str,
        git_branch: Option<&str>,
        mcp_count: usize,
        swarm_active: usize,
        tasks_summary: (usize, f32, f32),
        web_badge: Option<&str>,
        left_budget: usize,
        display: &StatusDisplayOptions,
    ) -> Vec<Span<'static>> {
        use crate::config::StatusBarStyle;

        let style = display.status_bar_style;
        let is_posh_ribbon = matches!(style, StatusBarStyle::Powerline | StatusBarStyle::PoshSlant);
        let is_posh_clean = matches!(
            style,
            StatusBarStyle::PoshPure
                | StatusBarStyle::PoshSlash
                | StatusBarStyle::PoshVelvet
                | StatusBarStyle::Pills
                | StatusBarStyle::PoshBubbles
                | StatusBarStyle::Powerline
                | StatusBarStyle::PoshSlant
                | StatusBarStyle::PoshDiamond
        );

        let provider_prefix = format!("{}/", provider);
        let full_provider_model = if model.starts_with(&provider_prefix) {
            model.to_string()
        } else {
            format!("{}/{}", provider, model)
        };
        let model_display = if style == StatusBarStyle::Compact {
            model.rsplit('/').next().unwrap_or(model).to_string()
        } else {
            full_provider_model
        };

        let raw_level = if effort_level.trim().is_empty() {
            "medium"
        } else {
            effort_level.trim()
        };
        let level_label = if style == StatusBarStyle::Compact {
            match raw_level {
                "medium" => "med",
                "high" => "hi",
                "low" => "lo",
                other => other,
            }
        } else {
            raw_level
        };
        let level_color = match raw_level {
            "low" => theme.info,
            "medium" => theme.success,
            "high" => theme.brand_accent,
            _ => theme.text_primary,
        };

        let (task_count, task_mem_mb, task_cpu_pct) = tasks_summary;
        let git_branch_opt = display.show_status_git.then_some(git_branch).flatten();
        let mcp_short =
            (display.show_status_badges && mcp_count > 0).then(|| format!("mcp:{}", mcp_count));
        let swarm_short = (display.show_status_badges && swarm_active > 0)
            .then(|| format!("swarm:{}", swarm_active));
        let mem_str = if task_mem_mb < 10.0 {
            format!("{:.1}MB", task_mem_mb)
        } else {
            format!("{:.0}MB", task_mem_mb)
        };
        let tasks_full_str = format!("tasks:{} ({})", task_count, mem_str);
        let active_tasks = display.show_status_badges && task_count > 0;
        let active_web = display.show_status_badges.then_some(web_badge).flatten();

        let pad_per_seg = if is_posh_clean { 4 } else { 3 };
        let base_model_w = usize::from(display.show_status_model)
            * (pad_per_seg + unicode_width::UnicodeWidthStr::width(model_display.as_str()))
            + usize::from(display.show_status_effort)
                * (pad_per_seg + unicode_width::UnicodeWidthStr::width(level_label));
        let git_w = git_branch_opt
            .map(|b| pad_per_seg + 4 + b.len())
            .unwrap_or(0);
        let mcp_full_w = mcp_short
            .as_ref()
            .map(|s| pad_per_seg + s.len() + 7)
            .unwrap_or(0);
        let swarm_full_w = swarm_short
            .as_ref()
            .map(|s| pad_per_seg + s.len() + 7)
            .unwrap_or(0);
        let tasks_full_w = if active_tasks {
            pad_per_seg + tasks_full_str.len()
        } else {
            0
        };
        let badges_full_w = git_w + mcp_full_w + swarm_full_w + tasks_full_w;
        let path_source = if style == StatusBarStyle::Compact {
            display_path
                .rsplit('/')
                .find(|s| !s.is_empty())
                .unwrap_or(display_path)
                .to_string()
        } else {
            display_path.to_string()
        };
        let path_full_w = if display.show_status_path {
            pad_per_seg + unicode_width::UnicodeWidthStr::width(path_source.as_str())
        } else {
            0
        };

        let compact_badges = base_model_w + path_full_w + badges_full_w > left_budget;
        let badges_actual_w = if compact_badges {
            git_w
                + mcp_short
                    .as_ref()
                    .map(|s| pad_per_seg + s.len())
                    .unwrap_or(0)
                + swarm_short
                    .as_ref()
                    .map(|s| pad_per_seg + s.len())
                    .unwrap_or(0)
                + tasks_full_w
        } else {
            badges_full_w
        };

        let path_budget = left_budget
            .saturating_sub(base_model_w + badges_actual_w + 4)
            .max(12);
        let fitted_path = Self::compact_path_to_budget(&path_source, path_budget);

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum SegmentGroup {
            Primary,
            Secondary,
        }

        struct StatusSeg {
            group: SegmentGroup,
            icon: Option<&'static str>,
            pure_prefix: Option<&'static str>,
            text: String,
            accent: ratatui::style::Color,
            bold: bool,
            optional: bool,
        }

        let mut planned: Vec<StatusSeg> = Vec::with_capacity(8);

        if display.show_status_model {
            planned.push(StatusSeg {
                group: SegmentGroup::Primary,
                icon: Some("◈"),
                pure_prefix: None,
                text: model_display,
                accent: theme.warning,
                bold: true,
                optional: false,
            });
        }
        if display.show_status_effort {
            planned.push(StatusSeg {
                group: SegmentGroup::Primary,
                icon: Some("⚡"),
                pure_prefix: None,
                text: level_label.to_string(),
                accent: level_color,
                bold: true,
                optional: false,
            });
        }
        if display.show_status_path {
            planned.push(StatusSeg {
                group: SegmentGroup::Secondary,
                icon: Some("⌂"),
                pure_prefix: None,
                text: fitted_path,
                accent: theme.info,
                bold: false,
                optional: false,
            });
        }
        if let Some(branch) = git_branch_opt {
            let seg = match style {
                StatusBarStyle::Compact | StatusBarStyle::Minimal | StatusBarStyle::Icons => {
                    branch.to_string()
                }
                StatusBarStyle::PoshPure
                | StatusBarStyle::PoshSlash
                | StatusBarStyle::PoshVelvet
                | StatusBarStyle::Pills
                | StatusBarStyle::PoshBubbles
                | StatusBarStyle::Powerline
                | StatusBarStyle::PoshSlant
                | StatusBarStyle::PoshDiamond => format!("⎇ {}", branch),
                _ => format!("git:{}", branch),
            };
            planned.push(StatusSeg {
                group: SegmentGroup::Secondary,
                icon: Some("⎇"),
                pure_prefix: Some("on "),
                text: seg,
                accent: theme.success,
                bold: false,
                optional: true,
            });
        }
        if let Some(mut seg) = mcp_short {
            if !compact_badges && style == StatusBarStyle::Classic {
                seg.push_str(" active");
            }
            planned.push(StatusSeg {
                group: SegmentGroup::Secondary,
                icon: Some("⊕"),
                pure_prefix: None,
                text: seg,
                accent: theme.brand_accent,
                bold: false,
                optional: true,
            });
        }
        if let Some(mut seg) = swarm_short {
            if !compact_badges && style == StatusBarStyle::Classic {
                seg.push_str(" active");
            }
            planned.push(StatusSeg {
                group: SegmentGroup::Secondary,
                icon: Some("❖"),
                pure_prefix: None,
                text: seg,
                accent: theme.info,
                bold: true,
                optional: true,
            });
        }
        if active_tasks {
            let task_color = if task_mem_mb > 1500.0 || task_cpu_pct > 80.0 {
                theme.destructive
            } else if task_mem_mb > 800.0 || task_cpu_pct > 40.0 {
                theme.warning
            } else {
                theme.success
            };
            let badge_text = if compact_badges {
                format!("tasks:{}", task_count)
            } else {
                tasks_full_str
            };
            planned.push(StatusSeg {
                group: SegmentGroup::Secondary,
                icon: Some("⚙"),
                pure_prefix: None,
                text: badge_text,
                accent: task_color,
                bold: true,
                optional: true,
            });
        }
        if let Some(web) = active_web {
            planned.push(StatusSeg {
                group: SegmentGroup::Secondary,
                icon: Some("◎"),
                pure_prefix: None,
                text: web.to_string(),
                accent: theme.brand_accent,
                bold: false,
                optional: true,
            });
        }

        if planned.is_empty() {
            return Vec::new();
        }

        // Seamless Oh My Posh contiguous ribbon styles (Powerline `` & Slant ``)
        if is_posh_ribbon {
            let sep_glyph = if style == StatusBarStyle::PoshSlant {
                "\u{e0b8}" // 
            } else {
                "\u{e0b0}" // 
            };
            let mut accepted: Vec<&StatusSeg> = Vec::with_capacity(planned.len());
            let mut used_w = 1usize; // leading space
            for seg in &planned {
                let seg_w = unicode_width::UnicodeWidthStr::width(seg.text.as_str()) + 3; // " text " + cap
                if !seg.optional || used_w + seg_w <= left_budget {
                    used_w += seg_w;
                    accepted.push(seg);
                }
            }
            if accepted.is_empty() {
                return Vec::new();
            }

            let mut spans: Vec<Span<'static>> =
                vec![Span::styled(" ", Style::default().bg(bar_bg))];
            let mut prev_bg: Option<ratatui::style::Color> = None;
            for seg in &accepted {
                let seg_bg = Self::tint_bg(seg.accent, bar_bg, theme.bg_elevated, 0.22);
                if let Some(p_bg) = prev_bg {
                    spans.push(Span::styled(
                        sep_glyph,
                        Style::default().fg(p_bg).bg(seg_bg),
                    ));
                }
                let mut text_st = Style::default().fg(seg.accent).bg(seg_bg);
                if seg.bold {
                    text_st = text_st.add_modifier(Modifier::BOLD);
                }
                spans.push(Span::styled(format!(" {} ", seg.text), text_st));
                prev_bg = Some(seg_bg);
            }
            if let Some(last_bg) = prev_bg {
                spans.push(Span::styled(
                    sep_glyph,
                    Style::default().fg(last_bg).bg(bar_bg),
                ));
            }
            return spans;
        }

        let current_left_width = |spans: &[Span]| -> usize {
            spans
                .iter()
                .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                .sum()
        };

        let mut left_spans: Vec<Span<'static>> = Vec::new();
        let mut last_group: Option<SegmentGroup> = None;

        for seg in planned {
            let mut text_style = Style::default().fg(seg.accent);
            if seg.bold {
                text_style = text_style.add_modifier(Modifier::BOLD);
            }
            let seg_bg = Self::tint_bg(seg.accent, bar_bg, theme.bg_elevated, 0.20);
            let is_first = last_group.is_none();
            let switched_group =
                last_group == Some(SegmentGroup::Primary) && seg.group == SegmentGroup::Secondary;

            let mut candidate: Vec<Span<'static>> = Vec::new();
            if is_first {
                candidate.push(Span::styled(" ", Style::default().bg(bar_bg)));
            } else {
                match style {
                    StatusBarStyle::Classic => {
                        let sep = if switched_group { " | " } else { " · " };
                        candidate.push(Span::styled(sep, Style::default().fg(theme.muted)));
                    }
                    StatusBarStyle::PoshPure => {
                        candidate.push(Span::raw("  "));
                    }
                    StatusBarStyle::PoshSlash => {
                        candidate.push(Span::styled(" / ", Style::default().fg(theme.border)));
                    }
                    StatusBarStyle::PoshVelvet
                    | StatusBarStyle::Pills
                    | StatusBarStyle::PoshBubbles
                    | StatusBarStyle::PoshDiamond
                    | StatusBarStyle::Brackets => {
                        candidate.push(Span::styled(" ", Style::default().bg(bar_bg)));
                    }
                    StatusBarStyle::Compact => {
                        candidate.push(Span::styled(" · ", Style::default().fg(theme.muted)));
                    }
                    StatusBarStyle::Minimal => {
                        candidate.push(Span::raw("  "));
                    }
                    StatusBarStyle::Icons => {
                        let sep = if switched_group { " │ " } else { " · " };
                        candidate.push(Span::styled(sep, Style::default().fg(theme.border)));
                    }
                    StatusBarStyle::Powerline | StatusBarStyle::PoshSlant => {}
                }
            }

            match style {
                StatusBarStyle::PoshPure => {
                    if let Some(pfx) = seg.pure_prefix {
                        candidate.push(Span::styled(
                            pfx,
                            Style::default()
                                .fg(theme.muted)
                                .add_modifier(Modifier::ITALIC),
                        ));
                    }
                    candidate.push(Span::styled(seg.text, text_style));
                }
                StatusBarStyle::PoshVelvet => {
                    candidate.push(Span::styled(
                        "▎",
                        Style::default().fg(seg.accent).bg(seg_bg),
                    ));
                    candidate.push(Span::styled(
                        format!(" {} ", seg.text),
                        text_style.bg(seg_bg),
                    ));
                }
                StatusBarStyle::Pills => {
                    candidate.push(Span::styled(
                        format!(" {} ", seg.text),
                        text_style.bg(seg_bg),
                    ));
                }
                StatusBarStyle::PoshBubbles => {
                    let cap_style = Style::default().fg(seg_bg).bg(bar_bg);
                    candidate.push(Span::styled("\u{e0b6}", cap_style));
                    candidate.push(Span::styled(seg.text, text_style.bg(seg_bg)));
                    candidate.push(Span::styled("\u{e0b4}", cap_style));
                }
                StatusBarStyle::PoshDiamond => {
                    let cap_style = Style::default().fg(seg_bg).bg(bar_bg);
                    candidate.push(Span::styled("\u{e0b2}", cap_style));
                    candidate.push(Span::styled(
                        format!(" {} ", seg.text),
                        text_style.bg(seg_bg),
                    ));
                    candidate.push(Span::styled("\u{e0b0}", cap_style));
                }
                StatusBarStyle::Brackets => {
                    candidate.push(Span::styled("[", Style::default().fg(theme.border)));
                    candidate.push(Span::styled(seg.text, text_style));
                    candidate.push(Span::styled("]", Style::default().fg(theme.border)));
                }
                StatusBarStyle::Icons => {
                    if let Some(ic) = seg.icon {
                        candidate.push(Span::styled(format!("{} ", ic), text_style));
                    }
                    candidate.push(Span::styled(seg.text, text_style));
                }
                StatusBarStyle::Classic
                | StatusBarStyle::PoshSlash
                | StatusBarStyle::Compact
                | StatusBarStyle::Minimal
                | StatusBarStyle::Powerline
                | StatusBarStyle::PoshSlant => {
                    candidate.push(Span::styled(seg.text, text_style));
                }
            }

            let add_w = current_left_width(&candidate);
            if !seg.optional || current_left_width(&left_spans) + add_w <= left_budget {
                left_spans.extend(candidate);
                last_group = Some(seg.group);
            }
        }

        left_spans
    }

    pub fn render_bottom_bar(frame: &mut Frame, area: Rect, ctx: &StatusContext) {
        let home_dir = HOME_DIR.get_or_init(dirs::home_dir);
        let display_path = if let Some(ref home) = home_dir {
            if let Ok(rel) = ctx.workspace.strip_prefix(home) {
                format!("~/{}", rel.display())
            } else {
                ctx.workspace.display().to_string()
            }
        } else {
            ctx.workspace.display().to_string()
        };

        let right_spans = Self::render_context_meter_spans(
            ctx.theme,
            ctx.used_tokens,
            ctx.max_context,
            ctx.cached_tokens,
            ctx.show_cost,
            ctx.session_cost_usd,
            ctx.display.show_kv_cache,
            ctx.display.show_context_meter,
            ctx.display.context_meter_style,
        );

        let right_width: u16 = right_spans
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()) as u16)
            .sum::<u16>()
            .saturating_add(1);
        let left_budget = (area.width.saturating_sub(right_width + 1)) as usize;

        let git_branch = Self::get_git_branch(ctx.workspace);
        let swarm_active = crate::agent::subagent::try_get_global_subagent_pool()
            .map(|p| p.active_worker_count())
            .unwrap_or(0);
        let tasks_summary = ctx.tasks_summary.unwrap_or_else(|| {
            crate::dev::registry::get_global_dev_registry().get_telemetry_snapshot()
        });
        let web_badge = crate::tools::browser::BrowserManager::live_spaces_badge_sync();

        let left_spans = Self::render_status_left_spans(
            ctx.theme,
            ctx.provider,
            ctx.model,
            ctx.effort_level,
            &display_path,
            git_branch.as_deref(),
            ctx.mcp_count,
            swarm_active,
            tasks_summary,
            web_badge.as_deref(),
            left_budget,
            &ctx.display,
        );

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(ctx.theme.bg_primary));

        let chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([
                ratatui::layout::Constraint::Min(1),
                ratatui::layout::Constraint::Length(right_width.max(1)),
            ])
            .split(area);

        let left_paragraph = Paragraph::new(Line::from(left_spans)).block(block.clone());
        frame.render_widget(left_paragraph, chunks[0]);

        let right_paragraph = Paragraph::new(Line::from(right_spans))
            .block(block)
            .alignment(ratatui::layout::Alignment::Right);
        frame.render_widget(right_paragraph, chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(StatusWidgets::format_tokens(0), "0");
        assert_eq!(StatusWidgets::format_tokens(450), "450");
        assert_eq!(StatusWidgets::format_tokens(1_000), "1k");
        assert_eq!(StatusWidgets::format_tokens(4_200), "4.2k");
        assert_eq!(StatusWidgets::format_tokens(128_000), "128k");
        assert_eq!(StatusWidgets::format_tokens(200_000), "200k");
        assert_eq!(StatusWidgets::format_tokens(1_000_000), "1M");
        assert_eq!(StatusWidgets::format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_status_bar_show_cost_toggle() {
        let theme = Theme::aura_dark();
        let ws = Path::new(".");
        let ctx_without_cost = StatusContext {
            theme: &theme,
            workspace: ws,
            provider: "anthropic",
            model: "claude-3-5-sonnet",
            effort_level: "medium",
            mcp_count: 0,
            used_tokens: 4200,
            max_context: 128000,
            show_cost: false,
            session_cost_usd: 0.042,
            cached_tokens: 1200,
            tasks_summary: None,
            display: StatusDisplayOptions::default(),
        };
        assert!(!ctx_without_cost.show_cost);

        let ctx_with_cost = StatusContext {
            show_cost: true,
            ..ctx_without_cost
        };
        assert!(ctx_with_cost.show_cost);
    }

    #[test]
    fn test_status_bar_tasks_badge_rendering() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let theme = Theme::aura_dark();
        let ws = Path::new("/tmp/test_workspace");
        let ctx = StatusContext {
            theme: &theme,
            workspace: ws,
            provider: "minimax",
            model: "MiniMax-M2.7",
            effort_level: "medium",
            mcp_count: 2,
            used_tokens: 1024,
            max_context: 128000,
            show_cost: true,
            session_cost_usd: 0.005,
            cached_tokens: 512,
            tasks_summary: Some((3, 142.0, 12.8)),
            display: StatusDisplayOptions::default(),
        };

        let backend = TestBackend::new(160, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                StatusWidgets::render_bottom_bar(f, f.area(), &ctx);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut line_str = String::new();
        for x in 0..buffer.area.width {
            line_str.push_str(buffer[(x, 0)].symbol());
        }

        assert!(line_str.contains("minimax/MiniMax-M2.7 · medium"));
        assert!(line_str.contains("tasks:3 (142MB)"));
        assert!(!line_str.contains("[F7]"));
        assert!(line_str.contains("mcp:2 active"));
        assert!(line_str.contains("kv (50%) •"));
        assert!(!line_str.contains("⚡"));
    }

    #[test]
    fn test_context_meter_styles_and_hide_toggles() {
        use crate::config::ContextMeterStyle;

        let theme = Theme::aura_dark();
        let join_spans = |spans: Vec<Span<'static>>| -> String {
            spans.into_iter().map(|s| s.content.into_owned()).collect()
        };

        // 1. Hide KV cache leaves clean context meter without dangling bullet
        let no_kv = join_spans(StatusWidgets::render_context_meter_spans(
            &theme,
            212_700,
            1_000_000,
            210_000,
            false,
            0.0,
            false,
            true,
            ContextMeterStyle::Compact,
        ));
        assert!(!no_kv.contains("kv"));
        assert!(!no_kv.contains("•"));
        assert!(no_kv.contains("212.7k / 1M"));

        // 2. Hide both KV and Context Meter produces empty spans
        let all_hidden = join_spans(StatusWidgets::render_context_meter_spans(
            &theme,
            212_700,
            1_000_000,
            210_000,
            false,
            0.0,
            false,
            false,
            ContextMeterStyle::Compact,
        ));
        assert!(all_hidden.is_empty());

        // 3. Visual & percentage styles omit redundant raw token fractions (212.7k/1M)
        for style in [
            ContextMeterStyle::Circle,
            ContextMeterStyle::Bar,
            ContextMeterStyle::Blocks,
            ContextMeterStyle::Braille,
            ContextMeterStyle::Percent,
            ContextMeterStyle::Pills,
            ContextMeterStyle::Gauge,
        ] {
            let rendered = join_spans(StatusWidgets::render_context_meter_spans(
                &theme, 212_700, 1_000_000, 210_000, false, 0.0, true, true, style,
            ));
            assert!(
                rendered.contains("21%"),
                "Style {:?} should display percentage, got: {rendered}",
                style
            );
            assert!(
                !rendered.contains("212.7k") && !rendered.contains("/1M"),
                "Visual/percentage style {:?} must not repeat raw token counts, got: {rendered}",
                style
            );
        }

        let circle = join_spans(StatusWidgets::render_context_meter_spans(
            &theme,
            212_700,
            1_000_000,
            210_000,
            false,
            0.0,
            true,
            true,
            ContextMeterStyle::Circle,
        ));
        assert!(circle.contains("◔ 21%"));

        let bar = join_spans(StatusWidgets::render_context_meter_spans(
            &theme,
            212_700,
            1_000_000,
            210_000,
            false,
            0.0,
            true,
            true,
            ContextMeterStyle::Bar,
        ));
        assert!(bar.contains("[██░░░░░░░░] 21%"));

        let braille = join_spans(StatusWidgets::render_context_meter_spans(
            &theme,
            500_000,
            1_000_000,
            0,
            false,
            0.0,
            true,
            true,
            ContextMeterStyle::Braille,
        ));
        assert!(braille.contains("⣿") && braille.contains("50%"));

        let gauge = join_spans(StatusWidgets::render_context_meter_spans(
            &theme,
            500_000,
            1_000_000,
            0,
            false,
            0.0,
            true,
            true,
            ContextMeterStyle::Gauge,
        ));
        assert!(gauge.contains("━━━━━─────") && gauge.contains("50%"));
    }

    #[test]
    fn test_mode_bar_and_status_left_styles_and_visibility() {
        use crate::config::{ModeBarStyle, StatusBarStyle};

        let theme = Theme::aura_dark();
        let join_spans = |spans: Vec<Span<'static>>| -> String {
            spans.into_iter().map(|s| s.content.into_owned()).collect()
        };

        // Mode bar hidden via style or per-item toggles
        assert!(StatusWidgets::render_mode_bar_spans(
            &theme,
            true,
            "full",
            true,
            "full",
            true,
            ModeBarStyle::Hidden,
            true,
            true,
            true
        )
        .is_empty());
        assert!(StatusWidgets::render_mode_bar_spans(
            &theme,
            true,
            "full",
            true,
            "full",
            true,
            ModeBarStyle::Classic,
            false,
            false,
            false
        )
        .is_empty());

        let only_stream = join_spans(StatusWidgets::render_mode_bar_spans(
            &theme,
            true,
            "full",
            true,
            "full",
            true,
            ModeBarStyle::Brackets,
            false,
            false,
            true,
        ));
        assert_eq!(only_stream.trim(), "[streaming]");

        let dots = join_spans(StatusWidgets::render_mode_bar_spans(
            &theme,
            true,
            "full",
            false,
            "full",
            true,
            ModeBarStyle::Dots,
            true,
            true,
            true,
        ));
        assert!(dots.contains("● ponytail:full") && dots.contains("○ caveman:off"));

        // Status bar left hiding model + path while showing git branch cleanly
        let opts = StatusDisplayOptions {
            status_bar_style: StatusBarStyle::Icons,
            show_status_model: false,
            show_status_effort: true,
            show_status_path: false,
            show_status_git: true,
            show_status_badges: false,
            ..Default::default()
        };
        let left = join_spans(StatusWidgets::render_status_left_spans(
            &theme,
            "minimax",
            "MiniMax-M3.1",
            "medium",
            "~/code/app",
            Some("main"),
            2,
            1,
            (2, 50.0, 2.0),
            None,
            120,
            &opts,
        ));
        assert!(!left.contains("MiniMax"));
        assert!(!left.contains("~/code/app"));
        assert!(left.contains("⚡ medium") && left.contains("⎇ main"));
    }

    #[test]
    fn test_oh_my_posh_status_bar_and_mode_bar_styles() {
        use crate::config::{ModeBarStyle, StatusBarStyle};

        let theme = Theme::aura_dark();
        let join_spans = |spans: Vec<Span<'static>>| -> String {
            spans.into_iter().map(|s| s.content.into_owned()).collect()
        };

        // 1. Every Oh My Posh status bar style renders cleanly with matching cap bg
        for (style, expected_marker) in [
            (StatusBarStyle::PoshPure, "on ⎇ main"),
            (StatusBarStyle::PoshSlash, " / "),
            (StatusBarStyle::PoshVelvet, "▎"),
            (StatusBarStyle::Pills, "⎇ main"),
            (StatusBarStyle::PoshBubbles, "\u{e0b6}"),
            (StatusBarStyle::Powerline, "\u{e0b0}"),
            (StatusBarStyle::PoshSlant, "\u{e0b8}"),
            (StatusBarStyle::PoshDiamond, "\u{e0b2}"),
        ] {
            let opts = StatusDisplayOptions {
                status_bar_style: style,
                ..Default::default()
            };
            let spans = StatusWidgets::render_status_left_spans_with_bg(
                &theme,
                theme.bg_elevated,
                "minimax",
                "MiniMax-M3.1-Flash",
                "medium",
                "~/programming/minitest82",
                Some("main"),
                0,
                0,
                (0, 0.0, 0.0),
                None,
                140,
                &opts,
            );
            let txt = join_spans(spans.clone());
            assert!(
                txt.contains(expected_marker) && txt.contains("MiniMax-M3.1-Flash"),
                "Status style {:?} missing expected marker {:?}, got: {txt}",
                style,
                expected_marker
            );
            if matches!(
                style,
                StatusBarStyle::PoshBubbles
                    | StatusBarStyle::Powerline
                    | StatusBarStyle::PoshSlant
                    | StatusBarStyle::PoshDiamond
            ) {
                let last_span = spans.last().expect("trailing cap");
                assert_eq!(
                    last_span.style.bg,
                    Some(theme.bg_elevated),
                    "Trailing cap for {:?} must match surrounding bar_bg",
                    style
                );
            }
        }

        // 2. Every Oh My Posh mode bar style renders cleanly & omits noisy `:off` clutter
        for (style, expected_marker) in [
            (ModeBarStyle::PoshPure, "• "),
            (ModeBarStyle::PoshSlash, " / "),
            (ModeBarStyle::PoshVelvet, "▎"),
            (ModeBarStyle::Pills, "ponytail·full"),
            (ModeBarStyle::PoshBubbles, "\u{e0b6}"),
            (ModeBarStyle::PoshPowerline, "\u{e0b2}"),
            (ModeBarStyle::PoshSlant, "\u{e0ba}"),
        ] {
            let txt = join_spans(StatusWidgets::render_mode_bar_spans_with_bg(
                &theme,
                theme.bg_elevated,
                true,
                "full",
                false,
                "full",
                true,
                style,
                true,
                true,
                true,
            ));
            assert!(
                txt.contains(expected_marker)
                    && txt.contains("ponytail·full")
                    && txt.contains("stream")
                    && !txt.contains("caveman:off"),
                "Mode style {:?} unexpected rendering: {txt}",
                style
            );
        }
    }
}
