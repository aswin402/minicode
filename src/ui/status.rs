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
            std::thread::spawn(move || {
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
            });
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

    pub fn render_bottom_bar(
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        workspace: &Path,
        model: &str,
        mcp_count: usize,
    ) {
        let home_dir = dirs::home_dir();
        let display_path = if let Some(ref home) = home_dir {
            if let Ok(rel) = workspace.strip_prefix(home) {
                format!("~/{}", rel.display())
            } else {
                workspace.display().to_string()
            }
        } else {
            workspace.display().to_string()
        };

        let mut footer_spans = vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                model,
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::default().fg(theme.muted)),
            Span::styled(display_path, Style::default().fg(theme.info)),
        ];

        if let Some(branch) = Self::get_git_branch(workspace) {
            footer_spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
            footer_spans.push(Span::styled(
                format!("git:{}", branch),
                Style::default().fg(theme.success),
            ));
        }

        if mcp_count > 0 {
            footer_spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
            footer_spans.push(Span::styled(
                format!("mcp:{} active", mcp_count),
                Style::default().fg(theme.brand_accent),
            ));
        }

        let footer_line = Line::from(footer_spans);

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(theme.bg_primary));

        let paragraph = Paragraph::new(footer_line).block(block);
        frame.render_widget(paragraph, area);
    }
}
