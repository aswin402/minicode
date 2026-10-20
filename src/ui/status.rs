use crate::constants::GIT_BRANCH_CACHE_TTL_SECS;
use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static BRANCH_CACHE: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);
static IS_FETCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
}

pub struct StatusWidgets;

impl StatusWidgets {
    pub fn get_git_branch(workspace: &Path) -> Option<String> {
        let now = Instant::now();
        let mut cached_branch = None;
        let mut needs_refresh = true;

        if let Ok(guard) = BRANCH_CACHE.lock() {
            if let Some((last_check, ref branch)) = *guard {
                cached_branch = branch.clone();
                if now.duration_since(last_check) < Duration::from_secs(GIT_BRANCH_CACHE_TTL_SECS) {
                    needs_refresh = false;
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
                        if !b.is_empty() {
                            Some(b)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });

                if let Ok(mut guard) = BRANCH_CACHE.lock() {
                    *guard = Some((Instant::now(), branch));
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

    pub fn render_bottom_bar(frame: &mut Frame, area: Rect, ctx: &StatusContext) {
        let home_dir = dirs::home_dir();
        let display_path = if let Some(ref home) = home_dir {
            if let Ok(rel) = ctx.workspace.strip_prefix(home) {
                format!("~/{}", rel.display())
            } else {
                ctx.workspace.display().to_string()
            }
        } else {
            ctx.workspace.display().to_string()
        };

        // Format right context token metrics and dollar cost first so we know exact right-column width dynamically
        let used_str = Self::format_tokens(ctx.used_tokens);
        let max_str = Self::format_tokens(ctx.max_context);
        let ratio = if ctx.max_context > 0 {
            (ctx.used_tokens as f64) / (ctx.max_context as f64)
        } else {
            0.0
        };

        let used_color = if ratio > 0.85 {
            ctx.theme.destructive // Coral red warning
        } else if ratio > 0.60 {
            ctx.theme.warning // Warm orange
        } else {
            ctx.theme.success // Mint green
        };

        let mut right_spans = Vec::new();
        if ctx.show_cost {
            let cost_str = crate::agent::pricing::ModelPricing::format_cost(ctx.session_cost_usd);
            right_spans.push(Span::styled(
                format!("{} ", cost_str),
                Style::default()
                    .fg(ctx.theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ));
            right_spans.push(Span::styled("• ", Style::default().fg(ctx.theme.muted)));
        }

        if ctx.cached_tokens > 0 {
            let hit_pct = if ctx.used_tokens > 0 {
                ((ctx.cached_tokens as f64 / ctx.used_tokens as f64) * 100.0).clamp(0.0, 100.0)
                    as usize
            } else {
                100
            };
            right_spans.push(Span::styled(
                format!("kv ({}%) ", hit_pct),
                Style::default().fg(ctx.theme.muted),
            ));
            right_spans.push(Span::styled("• ", Style::default().fg(ctx.theme.muted)));
        }

        right_spans.extend(vec![
            Span::styled(
                used_str,
                Style::default().fg(used_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" / ", Style::default().fg(ctx.theme.muted)),
            Span::styled(max_str, Style::default().fg(ctx.theme.muted)),
            Span::styled(" ", Style::default()),
        ]);

        let right_width: u16 = right_spans
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()) as u16)
            .sum::<u16>()
            .saturating_add(1);
        let left_budget = (area.width.saturating_sub(right_width + 1)) as usize;

        let provider_prefix = format!("{}/", ctx.provider);
        let provider_model = if ctx.model.starts_with(&provider_prefix) {
            ctx.model.to_string()
        } else {
            format!("{}/{}", ctx.provider, ctx.model)
        };

        let level_label = if ctx.effort_level.trim().is_empty() {
            "medium"
        } else {
            ctx.effort_level.trim()
        };
        let level_color = match level_label {
            "low" => ctx.theme.info,
            "medium" => ctx.theme.success,
            "high" => ctx.theme.brand_accent,
            _ => ctx.theme.text_primary,
        };

        let git_branch = Self::get_git_branch(ctx.workspace);
        let caveman_badge = crate::agent::caveman::read_flag_safe(
            &crate::agent::caveman::default_flag_path(ctx.workspace),
        )
        .filter(|l| l.is_enabled())
        .map(|l| l.badge_label().to_string());
        let swarm_active = crate::agent::subagent::try_get_global_subagent_pool()
            .map(|p| p.active_worker_count())
            .unwrap_or(0);
        let (task_count, task_mem_mb, task_cpu_pct) = ctx.tasks_summary.unwrap_or_else(|| {
            crate::dev::registry::get_global_dev_registry().get_telemetry_snapshot()
        });

        // Estimate whether full badge labels + full workspace path fit within left_budget
        let base_model_w = 1
            + unicode_width::UnicodeWidthStr::width(provider_model.as_str())
            + 3
            + unicode_width::UnicodeWidthStr::width(level_label);
        let git_w = git_branch
            .as_ref()
            .map(|b| 3 + 4 + b.len())
            .unwrap_or(0);
        let mcp_full_w = if ctx.mcp_count > 0 {
            3 + format!("mcp:{} active", ctx.mcp_count).len()
        } else {
            0
        };
        let swarm_full_w = if swarm_active > 0 {
            3 + format!("swarm:{} active", swarm_active).len()
        } else {
            0
        };
        let mem_str = if task_mem_mb < 10.0 {
            format!("{:.1}MB", task_mem_mb)
        } else {
            format!("{:.0}MB", task_mem_mb)
        };
        let tasks_full_str = format!("tasks:{} ({})", task_count, mem_str);
        let tasks_full_w = if task_count > 0 {
            3 + tasks_full_str.len()
        } else {
            0
        };
        let badges_full_w = git_w + mcp_full_w + swarm_full_w + tasks_full_w;
        let path_full_w = 3 + unicode_width::UnicodeWidthStr::width(display_path.as_str());

        let compact_badges = base_model_w + path_full_w + badges_full_w > left_budget;
        let badges_actual_w = if compact_badges {
            git_w
                + if ctx.mcp_count > 0 {
                    3 + format!("mcp:{}", ctx.mcp_count).len()
                } else {
                    0
                }
                + if swarm_active > 0 {
                    3 + format!("swarm:{}", swarm_active).len()
                } else {
                    0
                }
                + tasks_full_w
        } else {
            badges_full_w
        };

        let path_budget = left_budget
            .saturating_sub(base_model_w + badges_actual_w + 3)
            .max(12);
        let fitted_path = Self::compact_path_to_budget(&display_path, path_budget);

        let mut left_spans = vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                provider_model,
                Style::default()
                    .fg(ctx.theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::default().fg(ctx.theme.muted)),
            Span::styled(
                level_label.to_string(),
                Style::default()
                    .fg(level_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | ", Style::default().fg(ctx.theme.muted)),
            Span::styled(fitted_path, Style::default().fg(ctx.theme.info)),
        ];

        let current_left_width = |spans: &[Span]| -> usize {
            spans
                .iter()
                .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                .sum()
        };

        if let Some(branch) = git_branch {
            let seg = format!("git:{}", branch);
            if current_left_width(&left_spans) + 3 + seg.len() <= left_budget {
                left_spans.push(Span::styled(" · ", Style::default().fg(ctx.theme.muted)));
                left_spans.push(Span::styled(seg, Style::default().fg(ctx.theme.success)));
            }
        }

        if let Some(cm) = caveman_badge {
            if current_left_width(&left_spans) + 3 + cm.len() <= left_budget {
                left_spans.push(Span::styled(" · ", Style::default().fg(ctx.theme.muted)));
                left_spans.push(Span::styled(
                    cm,
                    Style::default()
                        .fg(ctx.theme.warning)
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }

        if ctx.mcp_count > 0 {
            let seg = if compact_badges {
                format!("mcp:{}", ctx.mcp_count)
            } else {
                format!("mcp:{} active", ctx.mcp_count)
            };
            if current_left_width(&left_spans) + 3 + seg.len() <= left_budget {
                left_spans.push(Span::styled(" · ", Style::default().fg(ctx.theme.muted)));
                left_spans.push(Span::styled(
                    seg,
                    Style::default().fg(ctx.theme.brand_accent),
                ));
            }
        }

        if swarm_active > 0 {
            let seg = if compact_badges {
                format!("swarm:{}", swarm_active)
            } else {
                format!("swarm:{} active", swarm_active)
            };
            if current_left_width(&left_spans) + 3 + seg.len() <= left_budget {
                left_spans.push(Span::styled(" · ", Style::default().fg(ctx.theme.muted)));
                left_spans.push(Span::styled(
                    seg,
                    Style::default()
                        .fg(ctx.theme.info)
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }

        if task_count > 0 {
            let task_color = if task_mem_mb > 1500.0 || task_cpu_pct > 80.0 {
                ctx.theme.destructive
            } else if task_mem_mb > 800.0 || task_cpu_pct > 40.0 {
                ctx.theme.warning
            } else {
                ctx.theme.success
            };

            let badge_text = if current_left_width(&left_spans) + 3 + tasks_full_str.len()
                <= left_budget
            {
                tasks_full_str
            } else {
                format!("tasks:{}", task_count)
            };

            if current_left_width(&left_spans) + 3 + badge_text.len() <= left_budget {
                left_spans.push(Span::styled(" · ", Style::default().fg(ctx.theme.muted)));
                left_spans.push(Span::styled(
                    badge_text,
                    Style::default().fg(task_color).add_modifier(Modifier::BOLD),
                ));
            }
        }

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(ctx.theme.bg_primary));

        // Subdivide area horizontally using exact dynamic width of right metrics
        let chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([
                ratatui::layout::Constraint::Min(1), // Left: Provider/Model · Level | Path · Git · MCP · Swarm · Tasks
                ratatui::layout::Constraint::Length(right_width.max(1)), // Right: Exact dynamic Dollar spend + Token Context
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
}
