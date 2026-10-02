use crate::ui::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Rich terminal Markdown parser and syntax highlighter for minicode
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Renders markdown text into styled Ratatui Lines
    pub fn render<'a>(text: &'a str, theme: &'a Theme) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        let mut in_code_block = false;
        let raw_lines: Vec<&'a str> = text.lines().collect();
        let mut i = 0;

        while i < raw_lines.len() {
            let raw_line = raw_lines[i];
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
                i += 1;
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
                i += 1;
                continue;
            }

            // Empty line
            if line.is_empty() {
                lines.push(Line::from(String::new()));
                i += 1;
                continue;
            }

            // Markdown Table Block detection (e.g. contiguous "| Col 1 | Col 2 |" rows)
            if Self::is_table_row(line) {
                let mut j = i;
                while j < raw_lines.len()
                    && !raw_lines[j].trim_end().starts_with("```")
                    && Self::is_table_row(raw_lines[j].trim_end())
                {
                    j += 1;
                }
                let table_lines = &raw_lines[i..j];
                lines.extend(Self::render_table_block(table_lines, theme));
                i = j;
                continue;
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
            } else if let Some(header_title) = Self::parse_section_header(line) {
                // Standalone bold section header (e.g., "**Recent Commits (latest 20):**" or "**Summary:**")
                lines.push(Line::from(vec![Span::styled(
                    header_title,
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
            i += 1;
        }

        lines
    }

    /// Safely detects and parses standalone bold section headers (e.g. "**Recent Commits:**" or "**Summary**")
    fn parse_section_header(line: &str) -> Option<String> {
        if line.len() < 5 {
            return None;
        }
        if let Some(without_prefix) = line.strip_prefix("**") {
            if let Some(inner) = without_prefix.strip_suffix("**:") {
                if !inner.contains("**") && !inner.trim().is_empty() {
                    return Some(format!("{}:", inner.trim()));
                }
            } else if let Some(inner) = without_prefix.strip_suffix("**") {
                if !inner.contains("**") && !inner.trim().is_empty() {
                    return Some(inner.trim().to_string());
                }
            }
        }
        None
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

    /// Checks if a trimmed line is a Markdown table row (e.g. "| Col 1 | Col 2 |")
    fn is_table_row(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 2
    }

    /// Renders a full contiguous block of markdown table rows with character-aligned columns,
    /// box-drawing borders (┌─┬─┐, ├─┼─┤, └─┴─┘), and header styling.
    fn render_table_block<'a>(table_lines: &[&'a str], theme: &'a Theme) -> Vec<Line<'a>> {
        if table_lines.is_empty() {
            return Vec::new();
        }

        struct ParsedRow<'b> {
            raw_cells: Vec<&'b str>,
            is_separator: bool,
        }

        let mut parsed_rows = Vec::new();
        for &line in table_lines {
            let raw_cells: Vec<&'a str> = line
                .trim()
                .trim_matches('|')
                .split('|')
                .map(|s| s.trim())
                .collect();

            if raw_cells.is_empty() {
                continue;
            }

            let is_sep = !raw_cells.is_empty()
                && raw_cells
                    .iter()
                    .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'));

            parsed_rows.push(ParsedRow {
                raw_cells,
                is_separator: is_sep,
            });
        }

        if parsed_rows.is_empty() {
            return Vec::new();
        }

        let num_cols = parsed_rows
            .iter()
            .map(|r| r.raw_cells.len())
            .max()
            .unwrap_or(0);
        if num_cols == 0 {
            return Vec::new();
        }

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum ColAlign {
            Left,
            Center,
            Right,
        }

        let mut alignments = vec![ColAlign::Left; num_cols];
        if let Some(sep_row) = parsed_rows.iter().find(|r| r.is_separator) {
            for (c, cell) in sep_row.raw_cells.iter().enumerate().take(num_cols) {
                let trimmed = cell.trim();
                let starts_colon = trimmed.starts_with(':');
                let ends_colon = trimmed.ends_with(':');
                if starts_colon && ends_colon {
                    alignments[c] = ColAlign::Center;
                } else if ends_colon {
                    alignments[c] = ColAlign::Right;
                } else {
                    alignments[c] = ColAlign::Left;
                }
            }
        }

        let mut col_widths = vec![3usize; num_cols];
        let mut row_spans: Vec<Option<Vec<Vec<Span<'a>>>>> = Vec::new();

        for row in &parsed_rows {
            if row.is_separator {
                row_spans.push(None);
            } else {
                let mut cells_spans = Vec::new();
                for (c, col_w) in col_widths.iter_mut().enumerate().take(num_cols) {
                    let cell_text = row.raw_cells.get(c).copied().unwrap_or("");
                    let spans = Self::parse_inline(cell_text, theme);
                    let visible_w: usize = spans
                        .iter()
                        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                        .sum();
                    *col_w = (*col_w).max(visible_w);
                    cells_spans.push(spans);
                }
                row_spans.push(Some(cells_spans));
            }
        }

        let mut out = Vec::new();
        let border_style = Style::default().fg(theme.border);

        // 1. Top border: ┌───┬───┐
        let mut top_line = Vec::new();
        top_line.push(Span::styled("┌─", border_style));
        for (c, &w) in col_widths.iter().enumerate() {
            if c > 0 {
                top_line.push(Span::styled("─┬─", border_style));
            }
            top_line.push(Span::styled("─".repeat(w), border_style));
        }
        top_line.push(Span::styled("─┐", border_style));
        out.push(Line::from(top_line));

        // 2. Render rows
        let first_is_header =
            parsed_rows.len() > 1 && parsed_rows.get(1).is_some_and(|r| r.is_separator);

        for (row_idx, row) in parsed_rows.iter().enumerate() {
            if row.is_separator {
                // Divider row: ├───┼───┤
                let mut div_line = Vec::new();
                div_line.push(Span::styled("├─", border_style));
                for (c, &w) in col_widths.iter().enumerate() {
                    if c > 0 {
                        div_line.push(Span::styled("─┼─", border_style));
                    }
                    div_line.push(Span::styled("─".repeat(w), border_style));
                }
                div_line.push(Span::styled("─┤", border_style));
                out.push(Line::from(div_line));
            } else if let Some(ref cells_spans) = row_spans[row_idx] {
                let is_header = row_idx == 0 && first_is_header;
                let mut row_line = Vec::new();
                row_line.push(Span::styled("│ ", border_style));

                for c in 0..num_cols {
                    if c > 0 {
                        row_line.push(Span::styled(" │ ", border_style));
                    }

                    let spans = &cells_spans[c];
                    let visible_w: usize = spans
                        .iter()
                        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                        .sum();
                    let target_w = col_widths[c];
                    let pad_total = target_w.saturating_sub(visible_w);

                    let (pad_left, pad_right) = match alignments[c] {
                        ColAlign::Right => (pad_total, 0),
                        ColAlign::Center => {
                            let pl = pad_total / 2;
                            (pl, pad_total - pl)
                        }
                        ColAlign::Left => (0, pad_total),
                    };

                    if pad_left > 0 {
                        row_line.push(Span::raw(" ".repeat(pad_left)));
                    }

                    for span in spans {
                        if is_header {
                            let mut st = span.style.add_modifier(Modifier::BOLD);
                            if st.fg.is_none() {
                                st = st.fg(theme.highlight);
                            }
                            row_line.push(Span::styled(span.content.clone(), st));
                        } else {
                            row_line.push(span.clone());
                        }
                    }

                    if pad_right > 0 {
                        row_line.push(Span::raw(" ".repeat(pad_right)));
                    }
                }

                row_line.push(Span::styled(" │", border_style));
                out.push(Line::from(row_line));
            }
        }

        // 3. Bottom border: └───┴───┘
        let mut bot_line = Vec::new();
        bot_line.push(Span::styled("└─", border_style));
        for (c, &w) in col_widths.iter().enumerate() {
            if c > 0 {
                bot_line.push(Span::styled("─┴─", border_style));
            }
            bot_line.push(Span::styled("─".repeat(w), border_style));
        }
        bot_line.push(Span::styled("─┘", border_style));
        out.push(Line::from(bot_line));

        out
    }

    /// Parses inline Markdown styling (bold, code, paths, URLs, metrics) safely
    pub fn parse_inline<'a>(text: &'a str, theme: &'a Theme) -> Vec<Span<'a>> {
        let mut spans = Vec::new();
        let mut current_buf = String::new();

        let flush_buf = |buf: &mut String, spans: &mut Vec<Span<'a>>| {
            if !buf.is_empty() {
                let token_spans = Self::highlight_tokens(buf, theme);
                spans.extend(token_spans);
                buf.clear();
            }
        };

        let mut rem = text;
        while !rem.is_empty() {
            // 1. Inline Code: `code`
            if rem.starts_with('`') {
                if let Some(end_idx) = rem[1..].find('`') {
                    let code_content = &rem[1..1 + end_idx];
                    flush_buf(&mut current_buf, &mut spans);
                    spans.push(Span::styled(
                        code_content.to_string(),
                        Style::default()
                            .fg(theme.info)
                            .bg(theme.bg_elevated)
                            .add_modifier(Modifier::BOLD),
                    ));
                    rem = &rem[1 + end_idx + 1..];
                    continue;
                }
            }

            // 2. Bold: **bold**
            if rem.starts_with("**") {
                if let Some(end_idx) = rem[2..].find("**") {
                    let bold_content = &rem[2..2 + end_idx];
                    flush_buf(&mut current_buf, &mut spans);

                    let style = if bold_content.starts_with('v')
                        && bold_content[1..]
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_digit())
                            .unwrap_or(false)
                    {
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD)
                    } else if bold_content.starts_with("feat")
                        || bold_content.starts_with("fix")
                        || bold_content.starts_with("refactor")
                    {
                        Style::default().fg(theme.info).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD)
                    };

                    spans.push(Span::styled(bold_content.to_string(), style));
                    rem = &rem[2 + end_idx + 2..];
                    continue;
                }
            }

            // 3. Links: [text](url)
            if let Some(after_open) = rem.strip_prefix('[') {
                if let Some(bracket_end) = after_open.find(']') {
                    let link_text = &after_open[..bracket_end];
                    let after_bracket = &after_open[bracket_end + 1..];
                    if let Some(after_paren) = after_bracket.strip_prefix('(') {
                        if let Some(paren_end) = after_paren.find(')') {
                            flush_buf(&mut current_buf, &mut spans);
                            spans.push(Span::styled(
                                link_text.to_string(),
                                Style::default()
                                    .fg(theme.info)
                                    .add_modifier(Modifier::UNDERLINED),
                            ));
                            rem = &after_paren[paren_end + 1..];
                            continue;
                        }
                    }
                }
            }

            // Safely consume one UTF-8 character
            let mut char_indices = rem.char_indices();
            if let Some((_, ch)) = char_indices.next() {
                current_buf.push(ch);
                if let Some((next_idx, _)) = char_indices.next() {
                    rem = &rem[next_idx..];
                } else {
                    rem = "";
                }
            } else {
                rem = "";
            }
        }

        flush_buf(&mut current_buf, &mut spans);
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
    fn test_markdown_edge_cases_no_panic() {
        let theme = Theme::default();
        let edge_cases = [
            "**",
            "***",
            "****",
            "**:",
            "*",
            "`",
            "``",
            "```",
            "[",
            "[]",
            "[](",
            "[](url)",
            "• **",
            "1. **",
            "🦀 emoji and UTF-8: › ✔ └ ─",
            "",
            "\n\n\n",
        ];

        for input in edge_cases {
            let _ = MarkdownRenderer::render(input, &theme);
            let _ = MarkdownRenderer::parse_inline(input, &theme);
        }
    }

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

    #[test]
    fn test_markdown_table_alignment() {
        let theme = Theme::default();
        let markdown = "\
| Step | Description | Status |
| :--- | :---: | ---: |
| 1 | Navigate to https://example.com | OK |
| 2 | Extract headings and test content | Done |
";
        let lines = MarkdownRenderer::render(markdown, &theme);
        // Expect: top border, header row, divider row, data row 1, data row 2, bottom border = 6 lines
        assert_eq!(lines.len(), 6);

        // Verify each line has identical total visible character width!
        let widths: Vec<usize> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum()
            })
            .collect();

        assert!(widths.len() == 6);
        let expected_w = widths[0];
        for (idx, &w) in widths.iter().enumerate() {
            assert_eq!(
                w, expected_w,
                "Table line {} width ({}) does not match line 0 width ({})",
                idx, w, expected_w
            );
        }
    }
}
