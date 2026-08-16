use crate::ui::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use similar::{ChangeTag, TextDiff};

/// Formatter for producing syntax-highlighted, colored unified diff lines in Ratatui.
pub struct DiffViewer;

impl DiffViewer {
    /// Generates colored `Line` elements for a unified diff between old and new text.
    pub fn render_diff(
        old_text: &str,
        new_text: &str,
        theme: &Theme,
        max_lines: usize,
    ) -> Vec<Line<'static>> {
        let diff = TextDiff::from_lines(old_text, new_text);
        let mut lines = Vec::new();

        for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
            if idx > 0 {
                lines.push(Line::from(vec![Span::styled(
                    "┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈",
                    Style::default().fg(theme.muted),
                )]));
            }

            for op in group {
                for change in diff.iter_changes(op) {
                    if lines.len() >= max_lines {
                        lines.push(Line::from(vec![Span::styled(
                            format!(
                                "  ... (diff truncated, {} more lines)",
                                diff.iter_changes(op).count()
                            ),
                            Style::default().fg(theme.muted),
                        )]));
                        return lines;
                    }

                    let sign = match change.tag() {
                        ChangeTag::Delete => "-",
                        ChangeTag::Insert => "+",
                        ChangeTag::Equal => " ",
                    };

                    let line_style = match change.tag() {
                        ChangeTag::Delete => {
                            Style::default().fg(theme.destructive).bg(theme.bg_elevated)
                        }
                        ChangeTag::Insert => {
                            Style::default().fg(theme.success).bg(theme.bg_elevated)
                        }
                        ChangeTag::Equal => Style::default().fg(theme.muted),
                    };

                    let text_str = change.value().trim_end_matches(['\r', '\n']).to_string();
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} ", sign),
                            Style::default()
                                .fg(match change.tag() {
                                    ChangeTag::Delete => theme.destructive,
                                    ChangeTag::Insert => theme.success,
                                    ChangeTag::Equal => theme.muted,
                                })
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(text_str, line_style),
                    ]));
                }
            }
        }

        if lines.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "  (No textual differences)",
                Style::default().fg(theme.muted),
            )]));
        }

        lines
    }

    /// Renders a patch preview directly from a patch text or structured arguments.
    pub fn render_patch_args(
        file_path: &str,
        search_block: &str,
        replace_block: &str,
        theme: &Theme,
        max_lines: usize,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(
                "Target: ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                file_path.to_string(),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        lines.push(Line::from(""));

        let diff_lines = Self::render_diff(
            search_block,
            replace_block,
            theme,
            max_lines.saturating_sub(2),
        );
        lines.extend(diff_lines);
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_viewer_generates_colored_lines() {
        let old = "fn main() {\n    println!(\"old\");\n}\n";
        let new = "fn main() {\n    println!(\"new\");\n}\n";
        let theme = Theme::default();

        let lines = DiffViewer::render_diff(old, new, &theme, 20);
        assert!(!lines.is_empty());
        let text: String = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("- ") && text.contains("println!(\"old\");"));
        assert!(text.contains("+ ") && text.contains("println!(\"new\");"));
    }
}
