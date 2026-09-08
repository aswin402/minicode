//! UI layout geometry helpers for centering popups and modals.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Computes a centered sub-rectangle using percentage constraints.
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let margin_y = 100_u16.saturating_sub(percent_y) / 2;
    let margin_x = 100_u16.saturating_sub(percent_x) / 2;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(margin_y),
            Constraint::Percentage(percent_y.min(100)),
            Constraint::Percentage(margin_y),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(margin_x),
            Constraint::Percentage(percent_x.min(100)),
            Constraint::Percentage(margin_x),
        ])
        .split(popup_layout[1])[1]
}

/// Computes a centered sub-rectangle using exact width and height in terminal cells.
pub fn centered_rect_exact(width: u16, height: u16, r: Rect) -> Rect {
    let w = width.min(r.width.saturating_sub(2));
    let h = height.min(r.height.saturating_sub(2));
    let x = r.x + (r.width.saturating_sub(w)) / 2;
    let y = r.y + (r.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centered_rect_dimensions() {
        let screen = Rect::new(0, 0, 100, 50);
        let popup = centered_rect(80, 60, screen);
        assert_eq!(popup.width, 80);
        assert_eq!(popup.height, 30);
        assert_eq!(popup.x, 10);
        assert_eq!(popup.y, 10);
    }

    #[test]
    fn test_centered_rect_exact_dimensions() {
        let screen = Rect::new(0, 0, 100, 50);
        let popup = centered_rect_exact(40, 20, screen);
        assert_eq!(popup.width, 40);
        assert_eq!(popup.height, 20);
        assert_eq!(popup.x, 30);
        assert_eq!(popup.y, 15);
    }

    #[test]
    fn test_centered_rect_exact_clamped_to_screen() {
        let screen = Rect::new(0, 0, 20, 10);
        let popup = centered_rect_exact(50, 50, screen);
        assert!(popup.width <= screen.width);
        assert!(popup.height <= screen.height);
        assert!(popup.x + popup.width <= screen.width);
        assert!(popup.y + popup.height <= screen.height);
    }

    #[test]
    fn test_compute_scroll_offset() {
        assert_eq!(compute_scroll_offset(0, 5), 0);
        assert_eq!(compute_scroll_offset(4, 5), 0);
        assert_eq!(compute_scroll_offset(5, 5), 1);
        assert_eq!(compute_scroll_offset(10, 5), 6);
        assert_eq!(compute_scroll_offset(3, 0), 0);
    }
}

/// Computes the scroll offset for a list widget so that the selected item remains visible.
#[must_use]
pub fn compute_scroll_offset(selected_index: usize, max_visible: usize) -> usize {
    if max_visible == 0 {
        return 0;
    }
    if selected_index < max_visible {
        0
    } else {
        selected_index.saturating_sub(max_visible - 1)
    }
}
