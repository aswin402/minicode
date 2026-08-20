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
    pub mcp_count: usize,
    pub used_tokens: usize,
    pub max_context: usize,
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

        let provider_model = format!("{}:{}", ctx.provider, ctx.model);

        let mut left_spans = vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                provider_model,
                Style::default()
                    .fg(ctx.theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | ", Style::default().fg(ctx.theme.muted)),
            Span::styled(display_path, Style::default().fg(ctx.theme.info)),
        ];

        if let Some(branch) = Self::get_git_branch(ctx.workspace) {
            left_spans.push(Span::styled(" · ", Style::default().fg(ctx.theme.muted)));
            left_spans.push(Span::styled(
                format!("git:{}", branch),
                Style::default().fg(ctx.theme.success),
            ));
        }

        if ctx.mcp_count > 0 {
            left_spans.push(Span::styled(" · ", Style::default().fg(ctx.theme.muted)));
            left_spans.push(Span::styled(
                format!("mcp:{} active", ctx.mcp_count),
                Style::default().fg(ctx.theme.brand_accent),
            ));
        }

        // Format right context token metrics (e.g., "4.2k / 128k")
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

        let right_spans = vec![
            Span::styled(
                used_str,
                Style::default().fg(used_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" / ", Style::default().fg(ctx.theme.muted)),
            Span::styled(max_str, Style::default().fg(ctx.theme.muted)),
            Span::styled(" ", Style::default()),
        ];

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(ctx.theme.bg_primary));

        // Subdivide area horizontally into left metadata and right-aligned context counter
        let chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([
                ratatui::layout::Constraint::Min(1), // Left: Provider, Path, Git, MCP
                ratatui::layout::Constraint::Length(18), // Right: Token Context
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
}
