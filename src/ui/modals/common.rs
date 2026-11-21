//! Shared helper types, navigation chrome, and utilities for TUI modals.

pub use crate::ui::layout_utils::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

/// Navigation metadata passed to modal renderers for Back / Close chrome and active badges.
#[derive(Debug, Clone, Default)]
pub struct ModalNavInfo {
    /// Short label of the immediate parent menu (e.g., `"Commands"`, `"Providers"`, `"Settings"`),
    /// or `None` when the current modal sits directly above the main TUI chat view.
    pub parent_label: Option<String>,
    /// True when stepping back stays inside a parent modal or outer tab rather than closing to chat.
    pub can_go_back: bool,
    /// Currently active provider id for rendering `[active]` badges in provider/model pickers.
    pub active_provider: Option<String>,
    /// Currently active model id for rendering `[active]` badges in model pickers.
    pub active_model: Option<String>,
}

/// Interactive cell bounding boxes for rendered modal navigation buttons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModalNavRegions {
    pub back_button: Option<Rect>,
    pub close_button: Option<Rect>,
}

impl ModalNavRegions {
    pub fn hit_back(&self, col: u16, row: u16) -> bool {
        self.back_button.is_some_and(|r| rect_contains(r, col, row))
    }

    pub fn hit_close(&self, col: u16, row: u16) -> bool {
        self.close_button
            .is_some_and(|r| rect_contains(r, col, row))
    }
}

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    r.width > 0
        && r.height > 0
        && col >= r.x
        && col < r.x.saturating_add(r.width)
        && row >= r.y
        && row < r.y.saturating_add(r.height)
}

/// Renders clickable `[← Back]` (and `[× Close]` when nested) pill buttons embedded
/// along the top-right border of `modal_rect`, returning exact terminal cell hitboxes.
pub fn render_modal_nav_buttons(
    frame: &mut Frame,
    modal_rect: Rect,
    theme: &Theme,
    nav: &ModalNavInfo,
) -> ModalNavRegions {
    if modal_rect.width < 16 || modal_rect.height < 2 {
        return ModalNavRegions::default();
    }

    let back_label = if nav.can_go_back {
        if let Some(ref parent) = nav.parent_label {
            let candidate = format!(" [← Back: {}] ", parent);
            let cand_w = UnicodeWidthStr::width(candidate.as_str()) as u16;
            if modal_rect.width >= cand_w.saturating_add(44) {
                candidate
            } else {
                " [← Back] ".to_string()
            }
        } else {
            " [← Back] ".to_string()
        }
    } else {
        " [← Back] ".to_string()
    };

    let close_label: Option<&str> = if nav.can_go_back {
        if modal_rect.width >= 52 {
            Some(" [× Close] ")
        } else {
            Some(" [×] ")
        }
    } else {
        None
    };

    let back_w = UnicodeWidthStr::width(back_label.as_str()) as u16;
    let close_w = close_label
        .map(|s| UnicodeWidthStr::width(s) as u16)
        .unwrap_or(0);
    let total_w = back_w.saturating_add(close_w);
    // Preserve left title visibility and the right border corner glyph.
    if total_w.saturating_add(4) > modal_rect.width {
        return ModalNavRegions::default();
    }

    let y = modal_rect.y;
    let right_edge = modal_rect
        .x
        .saturating_add(modal_rect.width)
        .saturating_sub(1);

    let (back_rect, close_rect) = if let Some(cl) = close_label {
        let close_x = right_edge.saturating_sub(close_w);
        let back_x = close_x.saturating_sub(back_w);
        let c_rect = Rect::new(close_x, y, close_w, 1);
        let b_rect = Rect::new(back_x, y, back_w, 1);

        let back_p = Paragraph::new(Line::from(Span::styled(
            back_label,
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_elevated)
                .add_modifier(Modifier::BOLD),
        )));
        frame.render_widget(back_p, b_rect);

        let close_p = Paragraph::new(Line::from(Span::styled(
            cl,
            Style::default().fg(theme.muted).bg(theme.bg_elevated),
        )));
        frame.render_widget(close_p, c_rect);

        (Some(b_rect), Some(c_rect))
    } else {
        let back_x = right_edge.saturating_sub(back_w);
        let b_rect = Rect::new(back_x, y, back_w, 1);

        let back_p = Paragraph::new(Line::from(Span::styled(
            back_label,
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_elevated)
                .add_modifier(Modifier::BOLD),
        )));
        frame.render_widget(back_p, b_rect);

        (Some(b_rect), None)
    };

    ModalNavRegions {
        back_button: back_rect,
        close_button: close_rect,
    }
}

#[derive(Debug, Clone)]
pub struct TurnCheckpointInfo {
    pub turn_id: usize,
    pub prompt: String,
    #[allow(dead_code)]
    pub timestamp: String,
    pub time_ago: String,
    pub files: Vec<String>,
    pub is_latest: bool,
}

pub fn format_time_ago(rfc3339_ts: &str) -> String {
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(rfc3339_ts) {
        let now = chrono::Utc::now();
        let diff = now.signed_duration_since(ts.with_timezone(&chrono::Utc));
        let secs = diff.num_seconds();
        if secs < 60 {
            format!("{}s ago", secs.max(1))
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else if secs < 86400 {
            format!("{}h ago", secs / 3600)
        } else {
            format!("{}d ago", secs / 86400)
        }
    } else {
        "recently".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_render_modal_nav_buttons_nested_and_hitboxes() {
        let theme = Theme::default();
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let nav = ModalNavInfo {
            parent_label: Some("Commands".to_string()),
            can_go_back: true,
            ..Default::default()
        };
        let mut regions = ModalNavRegions::default();
        terminal
            .draw(|f| {
                regions = render_modal_nav_buttons(f, f.area(), &theme, &nav);
            })
            .unwrap();

        let back = regions.back_button.expect("back button rect");
        let close = regions.close_button.expect("close button rect");
        assert!(regions.hit_back(back.x + 1, back.y));
        assert!(!regions.hit_back(close.x + 1, close.y));
        assert!(regions.hit_close(close.x + 1, close.y));
        assert!(!regions.hit_close(back.x + 1, back.y));

        let buf = terminal.backend().buffer();
        let top_row: String = (0..100).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(top_row.contains("[← Back: Commands]"), "got: {top_row}");
        assert!(top_row.contains("[× Close]"), "got: {top_row}");
    }

    #[test]
    fn test_render_modal_nav_buttons_top_level_and_tiny_area() {
        let theme = Theme::default();
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let nav = ModalNavInfo::default();
        let mut regions = ModalNavRegions::default();
        terminal
            .draw(|f| {
                regions = render_modal_nav_buttons(f, f.area(), &theme, &nav);
            })
            .unwrap();

        assert!(regions.back_button.is_some());
        assert!(regions.close_button.is_none());

        let tiny_backend = TestBackend::new(10, 2);
        let mut tiny_term = Terminal::new(tiny_backend).unwrap();
        tiny_term
            .draw(|f| {
                let r = render_modal_nav_buttons(f, f.area(), &theme, &nav);
                assert!(r.back_button.is_none());
                assert!(r.close_button.is_none());
            })
            .unwrap();
    }
}
