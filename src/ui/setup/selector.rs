use std::io::{self, Write};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use super::guard::TerminalGuard;

/// Represents an item in an interactive selection list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorItem {
    pub id: String,
    pub label: String,
    pub badge: Option<String>,
    pub hint: Option<String>,
}

impl SelectorItem {
    /// Creates a new selector item with an ID and display label.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            badge: None,
            hint: None,
        }
    }

    /// Adds a status badge to the item (e.g. `Active`, `Configured`).
    pub fn with_badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// Adds a descriptive hint to the item (e.g. `Configure LLM providers`).
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Helper to navigate to the previous item index with wrap-around.
pub fn prev_index(current: usize, total: usize) -> usize {
    InteractiveSelector::prev_index(current, total)
}

/// Helper to navigate to the next item index with wrap-around.
pub fn next_index(current: usize, total: usize) -> usize {
    InteractiveSelector::next_index(current, total)
}

/// Interactive terminal selector supporting arrow keys, vim keys (j/k), Enter, and Esc.
#[derive(Debug, Default, Clone)]
pub struct InteractiveSelector;

impl InteractiveSelector {
    /// Creates a new interactive selector.
    pub fn new() -> Self {
        Self
    }

    /// Calculates previous index with bounds check and wrap-around.
    pub fn prev_index(current: usize, total: usize) -> usize {
        if total == 0 {
            0
        } else if current == 0 || current >= total {
            total - 1
        } else {
            current - 1
        }
    }

    /// Calculates next index with bounds check and wrap-around.
    pub fn next_index(current: usize, total: usize) -> usize {
        if total == 0 || current + 1 >= total {
            0
        } else {
            current + 1
        }
    }

    /// Formats an individual selector item into an ANSI styled string line.
    pub fn format_item(item: &SelectorItem, is_selected: bool) -> String {
        let mut s = String::new();
        if is_selected {
            s.push_str("\x1b[1;36m  ❯ \x1b[0m\x1b[1m");
            s.push_str(&item.label);
            s.push_str("\x1b[0m");
        } else {
            s.push_str("    ");
            s.push_str(&item.label);
        }

        if let Some(badge) = &item.badge {
            if !badge.is_empty() {
                s.push(' ');
                s.push_str(badge);
            }
        }

        if let Some(hint) = &item.hint {
            if !hint.is_empty() {
                s.push_str(" \x1b[90m");
                s.push_str(hint);
                s.push_str("\x1b[0m");
            }
        }

        s
    }

    /// Generates all rendered lines for the current prompt, items, and navigation footer.
    pub fn render_lines(prompt: &str, items: &[SelectorItem], current_index: usize) -> Vec<String> {
        let mut lines = Vec::new();
        if !prompt.is_empty() {
            for line in prompt.lines() {
                lines.push(format!("\x1b[1m{}\x1b[0m", line));
            }
        }
        for (i, item) in items.iter().enumerate() {
            lines.push(Self::format_item(item, i == current_index));
        }
        lines.push(
            "\x1b[90m  ──────────────────────────────────────────────────────────\x1b[0m"
                .to_string(),
        );
        lines.push("  \x1b[90m↑/↓ Navigate • ↵ Select • Esc Back\x1b[0m".to_string());
        lines
    }

    fn redraw<W: Write>(
        out: &mut W,
        prompt: &str,
        items: &[SelectorItem],
        current_index: usize,
        total_lines: usize,
    ) -> io::Result<()> {
        if total_lines > 0 {
            write!(out, "\x1b[{}A", total_lines)?;
        }
        let lines = Self::render_lines(prompt, items, current_index);
        for line in &lines {
            write!(out, "\x1b[2K\r{}\r\n", line)?;
        }
        out.flush()
    }

    fn clear_lines<W: Write>(out: &mut W, total_lines: usize) -> io::Result<()> {
        if total_lines > 0 {
            write!(out, "\x1b[{}A", total_lines)?;
            for _ in 0..total_lines {
                write!(out, "\x1b[2K\r\n")?;
            }
            write!(out, "\x1b[{}A\r", total_lines)?;
            out.flush()?;
        }
        Ok(())
    }

    /// Prompts the user with an interactive terminal selector.
    /// Returns `Ok(Some(index))` if confirmed with Enter, or `Ok(None)` if cancelled with Esc / Ctrl+C.
    pub fn select(
        &self,
        prompt: &str,
        items: &[SelectorItem],
        initial_index: usize,
    ) -> io::Result<Option<usize>> {
        if items.is_empty() {
            return Ok(None);
        }

        let _guard = TerminalGuard::new()?;
        let mut out = io::stdout();

        let mut current_index = initial_index.min(items.len() - 1);
        let lines = Self::render_lines(prompt, items, current_index);
        let total_lines = lines.len();

        for line in &lines {
            write!(out, "{}\r\n", line)?;
        }
        out.flush()?;

        loop {
            let ev = crossterm::event::read()?;
            if let Event::Key(key_event) = ev {
                if key_event.kind == KeyEventKind::Release {
                    continue;
                }

                if key_event.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key_event.code, KeyCode::Char('c' | 'C'))
                {
                    Self::clear_lines(&mut out, total_lines)?;
                    return Ok(None);
                }

                match key_event.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        let new_index = Self::prev_index(current_index, items.len());
                        if new_index != current_index {
                            current_index = new_index;
                            Self::redraw(&mut out, prompt, items, current_index, total_lines)?;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let new_index = Self::next_index(current_index, items.len());
                        if new_index != current_index {
                            current_index = new_index;
                            Self::redraw(&mut out, prompt, items, current_index, total_lines)?;
                        }
                    }
                    KeyCode::Enter => {
                        Self::clear_lines(&mut out, total_lines)?;
                        return Ok(Some(current_index));
                    }
                    KeyCode::Esc => {
                        Self::clear_lines(&mut out, total_lines)?;
                        return Ok(None);
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_item_builder() {
        let item = SelectorItem::new("provider", "⚡ Provider")
            .with_badge("Active")
            .with_hint("Configure LLM providers");
        assert_eq!(item.id, "provider");
        assert_eq!(item.label, "⚡ Provider");
        assert_eq!(item.badge.as_deref(), Some("Active"));
        assert_eq!(item.hint.as_deref(), Some("Configure LLM providers"));
    }

    #[test]
    fn test_navigation_wrap_and_bounds() {
        assert_eq!(InteractiveSelector::prev_index(0, 3), 2);
        assert_eq!(InteractiveSelector::prev_index(1, 3), 0);
        assert_eq!(InteractiveSelector::next_index(2, 3), 0);
        assert_eq!(InteractiveSelector::next_index(1, 3), 2);
    }

    #[test]
    fn test_navigation_edge_cases() {
        // Zero items
        assert_eq!(InteractiveSelector::prev_index(0, 0), 0);
        assert_eq!(InteractiveSelector::next_index(0, 0), 0);

        // Single item
        assert_eq!(InteractiveSelector::prev_index(0, 1), 0);
        assert_eq!(InteractiveSelector::next_index(0, 1), 0);

        // Standalone functions
        assert_eq!(prev_index(0, 3), 2);
        assert_eq!(next_index(2, 3), 0);

        // Out of bounds current
        assert_eq!(InteractiveSelector::prev_index(5, 3), 2);
        assert_eq!(InteractiveSelector::next_index(5, 3), 0);
    }

    #[test]
    fn test_format_item_highlighted_and_normal() {
        let item = SelectorItem::new("test", "Test Item")
            .with_badge("[Active]")
            .with_hint("Hint text");

        let highlighted = InteractiveSelector::format_item(&item, true);
        assert!(highlighted.contains("  ❯ "));
        assert!(highlighted.contains("Test Item"));
        assert!(highlighted.contains("[Active]"));
        assert!(highlighted.contains("Hint text"));

        let normal = InteractiveSelector::format_item(&item, false);
        assert!(normal.starts_with("    "));
        assert!(!normal.contains("❯"));
        assert!(normal.contains("Test Item"));
        assert!(normal.contains("[Active]"));
        assert!(normal.contains("Hint text"));
    }

    #[test]
    fn test_render_lines_structure() {
        let items = vec![
            SelectorItem::new("1", "First"),
            SelectorItem::new("2", "Second"),
        ];

        let lines = InteractiveSelector::render_lines("Choose:", &items, 0);
        assert_eq!(lines.len(), 5); // 1 header + 2 items + 1 separator + 1 footer
        assert!(lines[0].contains("Choose:"));
        assert!(lines[1].contains("First"));
        assert!(lines[1].contains("❯")); // Selected
        assert!(lines[2].contains("Second"));
        assert!(!lines[2].contains("❯")); // Not selected
        assert!(lines[3].contains("───"));
        assert!(lines[4].contains("↑/↓ Navigate"));
    }

    #[test]
    fn test_empty_items_select() {
        let selector = InteractiveSelector::new();
        let result = selector.select("Empty", &[], 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
