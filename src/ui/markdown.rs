use crate::ui::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Rich terminal Markdown parser and syntax highlighter for minicode
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Renders markdown text into styled Ratatui Lines
    pub fn render<'a>(text: &'a str, theme: &'a Theme) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        let mut in_code_block = false;

        for raw_line in text.lines() {
            let line = raw_line.trim_end();

            // Code block fence detection
            if line.starts_with("```") {
                if in_code_block {
                    in_code_block = false;
                    lines.push(Line::from(vec![Span::styled(
                        "  └────────────────────────────────",
                        Style::default().fg(theme.border),
                    )]));
                } else {
                    in_code_block = true;
                    let lang = line.strip_prefix("```").unwrap_or("").trim();
                    let header = if lang.is_empty() {
                        "  ┌─ code ────────────────────────".to_string()
                    } else {
                        format!("  ┌─ {} ────────────────────────", lang)
                    };
                    lines.push(Line::from(vec![Span::styled(
                        header,
                        Style::default().fg(theme.border),
                    )]));
                }
                continue;
            }

            // Inside code block: render with syntax styling and border prefix
            if in_code_block {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(theme.border)),
                    Span::styled(
                        raw_line,
                        Style::default()
                            .fg(theme.text_primary)
                            .bg(theme.bg_elevated),
                    ),
                ]));
                continue;
            }

            // Empty line
            if line.is_empty() {
                lines.push(Line::from(String::new()));
                continue;
            }

            // Markdown Table Row detection (e.g. "| Col 1 | Col 2 |")
            if line.starts_with('|') && line.ends_with('|') {
                if let Some(table_line) = Self::render_table_row(line, theme) {
                    lines.push(table_line);
                    continue;
                }
            }

            // Headings
            if let Some(rest) = line.strip_prefix("#### ") {
                let mut spans = vec![Span::styled(
                    "#### ",
                    Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                )];
                spans.extend(Self::parse_inline(rest, theme));
                lines.push(Line::from(spans));
            } else if let Some(rest) = line.strip_prefix("### ") {
                let mut spans = vec![Span::styled(
                    "### ",
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD),
                )];
                spans.extend(Self::parse_inline(rest, theme));
                lines.push(Line::from(spans));
            } else if let Some(rest) = line.strip_prefix("## ") {
                let mut spans = vec![Span::styled(
                    "## ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                )];
                spans.extend(Self::parse_inline(rest, theme));
                lines.push(Line::from(spans));
            } else if let Some(rest) = line.strip_prefix("# ") {
                let mut spans = vec![Span::styled(
                    "# ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                )];
                spans.extend(Self::parse_inline(rest, theme));
                lines.push(Line::from(spans));
            } else if (line.starts_with("**") && (line.ends_with("**:") || line.ends_with("**")))
                && !line[2..line.len() - 2].contains("**")
            {
                // Standalone bold section header (e.g., "**Recent Commits (latest 20):**" or "**Summary:**")
                let content = if line.ends_with("**:") {
                    format!("{}:", &line[2..line.len() - 3])
                } else {
                    line[2..line.len() - 2].to_string()
                };
                lines.push(Line::from(vec![Span::styled(
                    content,
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD),
                )]));
            } else if let Some(rest) = line
                .strip_prefix("  - ")
                .or_else(|| line.strip_prefix("  * "))
                .or_else(|| line.strip_prefix("  • "))
            {
                // Sub-bullet list item
                let mut spans = vec![Span::styled("    - ", Style::default().fg(theme.muted))];
                spans.extend(Self::parse_inline(rest, theme));
                lines.push(Line::from(spans));
            } else if let Some(rest) = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .or_else(|| line.strip_prefix("• "))
            {
                // Bullet list item
                let mut spans = vec![Span::styled("• ", Style::default().fg(theme.brand_accent))];
                spans.extend(Self::parse_inline(rest, theme));
                lines.push(Line::from(spans));
            } else if let Some((num_str, rest)) = Self::parse_numbered_list(line) {
                // Numbered list item (e.g. "1. Model limitation")
                let mut spans = vec![Span::styled(
                    format!("{}. ", num_str),
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                )];
                spans.extend(Self::parse_inline(rest, theme));
                lines.push(Line::from(spans));
            } else if line == "---" || line == "***" || line == "___" {
                // Horizontal divider
                lines.push(Line::from(vec![Span::styled(
                    "────────────────────────────────────────────────────",
                    Style::default().fg(theme.border),
                )]));
            } else {
                // Standard text line with rich inline highlights
                let spans = Self::parse_inline(line, theme);
                lines.push(Line::from(spans));
            }
        }

        lines
    }

    /// Helper to detect and parse numbered list items (e.g. "1. Item", "12. Item")
    fn parse_numbered_list(line: &str) -> Option<(&str, &str)> {
        let trimmed = line.trim_start();
        if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
                let rest = &trimmed[dot_pos + 2..];
                return Some((num_part, rest));
            }
        }
        None
    }

    /// Renders markdown table rows (headers, separators, data rows)
    fn render_table_row<'a>(line: &'a str, theme: &'a Theme) -> Option<Line<'a>> {
        let raw_cells: Vec<&str> = line
            .trim_matches('|')
            .split('|')
            .map(|s| s.trim())
            .collect();

        if raw_cells.is_empty() {
            return None;
        }

        // Check if separator row (e.g. "| --- | :---: | ---: |")
        let is_separator = raw_cells
            .iter()
            .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'));

        if is_separator {
            let mut spans = Vec::new();
            spans.push(Span::styled("├", Style::default().fg(theme.border)));
            for (idx, _) in raw_cells.iter().enumerate() {
                if idx > 0 {
                    spans.push(Span::styled("┼", Style::default().fg(theme.border)));
                }
                spans.push(Span::styled(
                    "────────────────────",
                    Style::default().fg(theme.border),
                ));
            }
            spans.push(Span::styled("┤", Style::default().fg(theme.border)));
            return Some(Line::from(spans));
        }

        let mut spans = Vec::new();
        spans.push(Span::styled("│ ", Style::default().fg(theme.border)));

        for (idx, cell) in raw_cells.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled(" │ ", Style::default().fg(theme.border)));
            }
            let cell_spans = Self::parse_inline(cell, theme);
            spans.extend(cell_spans);
        }

        spans.push(Span::styled(" │", Style::default().fg(theme.border)));
        Some(Line::from(spans))
    }

    /// Parses inline Markdown styling (bold, code, paths, URLs, metrics)
    pub fn parse_inline<'a>(text: &'a str, theme: &'a Theme) -> Vec<Span<'a>> {
        let mut spans = Vec::new();
        let mut idx = 0;
        let bytes = text.as_bytes();
        let len = bytes.len();

        let mut current_text = String::new();

        let flush_current = |current: &mut String, spans: &mut Vec<Span<'a>>| {
            if !current.is_empty() {
                let token_spans = Self::highlight_tokens(current, theme);
                spans.extend(token_spans);
                current.clear();
            }
        };

        while idx < len {
            // 1. Inline Code: `code`
            if bytes[idx] == b'`' {
                if let Some(end_rel) = text[idx + 1..].find('`') {
                    let end_pos = idx + 1 + end_rel;
                    flush_current(&mut current_text, &mut spans);
                    let code_str = &text[idx + 1..end_pos];
                    spans.push(Span::styled(
                        code_str.to_string(),
                        Style::default()
                            .fg(theme.info)
                            .bg(theme.bg_elevated)
                            .add_modifier(Modifier::BOLD),
                    ));
                    idx = end_pos + 1;
                    continue;
                }
            }

            // 2. Bold: **bold**
            if idx + 1 < len && bytes[idx] == b'*' && bytes[idx + 1] == b'*' {
                if let Some(end_rel) = text[idx + 2..].find("**") {
                    let end_pos = idx + 2 + end_rel;
                    flush_current(&mut current_text, &mut spans);
                    let bold_str = &text[idx + 2..end_pos];

                    // Check if bold string is a version, keyword, or path
                    let style = if bold_str.starts_with('v')
                        && bold_str[1..]
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_digit())
                            .unwrap_or(false)
                    {
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD)
                    } else if bold_str.starts_with("feat")
                        || bold_str.starts_with("fix")
                        || bold_str.starts_with("refactor")
                    {
                        Style::default().fg(theme.info).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD)
                    };

                    spans.push(Span::styled(bold_str.to_string(), style));
                    idx = end_pos + 2;
                    continue;
                }
            }

            // 3. Links: [text](url)
            if bytes[idx] == b'[' {
                if let Some(close_bracket) = text[idx + 1..].find(']') {
                    let text_end = idx + 1 + close_bracket;
                    if text_end + 1 < len && bytes[text_end + 1] == b'(' {
                        if let Some(close_paren) = text[text_end + 2..].find(')') {
                            let url_end = text_end + 2 + close_paren;
                            flush_current(&mut current_text, &mut spans);
                            let link_text = &text[idx + 1..text_end];
                            spans.push(Span::styled(
                                link_text.to_string(),
                                Style::default()
                                    .fg(theme.info)
                                    .add_modifier(Modifier::UNDERLINED),
                            ));
                            idx = url_end + 1;
                            continue;
                        }
                    }
                }
            }

            current_text.push(text[idx..].chars().next().unwrap_or(' '));
            idx += text[idx..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        }

        flush_current(&mut current_text, &mut spans);
        spans
    }

    /// Highlights individual word tokens (file paths, URLs, metrics, versions, keywords)
    fn highlight_tokens<'a>(text: &str, theme: &'a Theme) -> Vec<Span<'a>> {
        let mut spans = Vec::new();
        let words =
            text.split_inclusive(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')');

        for word in words {
            let trimmed = word.trim_matches(|c: char| {
                c == ',' || c == '(' || c == ')' || c == ';' || c == '"' || c == '\''
            });

            if Self::is_file_path(trimmed) {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default().fg(theme.success),
                ));
            } else if Self::is_version(trimmed) || Self::is_metric_or_latency(trimmed) {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if Self::is_url(trimmed) {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default()
                        .fg(theme.info)
                        .add_modifier(Modifier::UNDERLINED),
                ));
            } else if trimmed == "Hello!"
                || trimmed == "Passed"
                || trimmed == "Success"
                || trimmed == "Correct"
            {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default().fg(theme.success),
                ));
            } else if trimmed == "Failed" || trimmed == "Error" || trimmed == "Wrong" {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default().fg(theme.destructive),
                ));
            } else {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default().fg(theme.text_primary),
                ));
            }
        }

        spans
    }

    /// Detects file paths (e.g. "Cargo.toml:31", "src/brain.rs", "models/qwen2.5-0.5b-instruct-q4_k_m.gguf")
    fn is_file_path(s: &str) -> bool {
        if s.is_empty() || s.len() < 3 {
            return false;
        }

        // File extensions
        let has_code_ext = s.ends_with(".rs")
            || s.ends_with(".toml")
            || s.ends_with(".json")
            || s.ends_with(".ts")
            || s.ends_with(".js")
            || s.ends_with(".md")
            || s.ends_with(".py")
            || s.ends_with(".go")
            || s.ends_with(".yaml")
            || s.ends_with(".yml")
            || s.ends_with(".gguf")
            || s.ends_with(".c")
            || s.ends_with(".cpp")
            || s.ends_with(".h");

        // Line number suffix (e.g. "Cargo.toml:31", "src/brain.rs:344")
        let has_line_ref = if let Some(colon_pos) = s.rfind(':') {
            let after = &s[colon_pos + 1..];
            !after.is_empty()
                && after.chars().all(|c| c.is_ascii_digit())
                && Self::is_file_path(&s[..colon_pos])
        } else {
            false
        };

        let has_path_prefix = s.starts_with("src/")
            || s.starts_with("tests/")
            || s.starts_with("models/")
            || s.starts_with("configs/")
            || s.starts_with("./")
            || s.starts_with("~/");

        has_code_ext || has_line_ref || (has_path_prefix && s.contains('.'))
    }

    /// Detects version strings (e.g. "v0.0.27", "v1.2.0")
    fn is_version(s: &str) -> bool {
        if let Some(rest) = s.strip_prefix('v') {
            if rest.contains('.') && rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return true;
            }
        }
        false
    }

    /// Detects metric, latency, or memory strings (e.g. "4.3s", "7.6s", "1.27GB", "491MB", "4.2k", "128k")
    fn is_metric_or_latency(s: &str) -> bool {
        let trimmed = s
            .trim_start_matches('~')
            .trim_start_matches('<')
            .trim_start_matches('>');
        if trimmed.ends_with('s') || trimmed.ends_with("ms") {
            let num = trimmed.trim_end_matches("ms").trim_end_matches('s');
            !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.')
        } else if trimmed.ends_with("MB") || trimmed.ends_with("GB") || trimmed.ends_with("KB") {
            let num = trimmed
                .trim_end_matches("MB")
                .trim_end_matches("GB")
                .trim_end_matches("KB")
                .trim();
            !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.')
        } else if trimmed.ends_with('k') || trimmed.ends_with('M') {
            let num = trimmed.trim_end_matches('k').trim_end_matches('M');
            !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.')
        } else {
            false
        }
    }

    /// Detects URLs
    fn is_url(s: &str) -> bool {
        s.starts_with("http://") || s.starts_with("https://")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_bold_and_code_spans() {
        let theme = Theme::default();
        let spans = MarkdownRenderer::parse_inline("Check `Cargo.toml:31` and **v0.0.27**", &theme);
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_markdown_section_header() {
        let theme = Theme::default();
        let lines = MarkdownRenderer::render(
            "**Recent Commits (latest 20):**\n• **v0.0.27** - Smooth",
            &theme,
        );
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_markdown_numbered_list() {
        let theme = Theme::default();
        let lines = MarkdownRenderer::render(
            "1. Model limitation\n2. Native RAM usage is ~1.27 GB",
            &theme,
        );
        assert_eq!(lines.len(), 2);
    }
}
