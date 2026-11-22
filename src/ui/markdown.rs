use crate::config::OutputStyle;
use crate::ui::icons::{nf, NerdIcon};
use crate::ui::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Semantic category for GitHub / Report-Skill blockquote callouts (`> [!NOTE]`, `> [!WARNING]`, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalloutKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl CalloutKind {
    fn parse_tag(text: &str) -> Option<(Self, &str)> {
        let trimmed = text.trim_start();
        for (prefix, kind) in [
            ("[!NOTE]", Self::Note),
            ("[!INFO]", Self::Note),
            ("[!TIP]", Self::Tip),
            ("[!SUCCESS]", Self::Tip),
            ("[!IMPORTANT]", Self::Important),
            ("[!WARNING]", Self::Warning),
            ("[!WARN]", Self::Warning),
            ("[!CAUTION]", Self::Caution),
            ("[!DANGER]", Self::Caution),
        ] {
            if let Some(rest) = trimmed
                .get(..prefix.len())
                .filter(|p| p.eq_ignore_ascii_case(prefix))
                .map(|_| trimmed[prefix.len()..].trim_start())
            {
                return Some((kind, rest));
            }
        }
        None
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Note => "NOTE",
            Self::Tip => "TIP",
            Self::Important => "IMPORTANT",
            Self::Warning => "WARNING",
            Self::Caution => "CAUTION",
        }
    }

    fn color(self, theme: &Theme) -> Color {
        match self {
            Self::Note => theme.info,
            Self::Tip => theme.success,
            Self::Important => theme.brand_accent,
            Self::Warning => theme.warning,
            Self::Caution => theme.destructive,
        }
    }

    fn badge_prefix(self, style: OutputStyle) -> String {
        match style {
            OutputStyle::Emoji => match self {
                Self::Note => "ℹ️ ".to_string(),
                Self::Tip => "💡 ".to_string(),
                Self::Important => "⚡ ".to_string(),
                Self::Warning => "⚠️ ".to_string(),
                Self::Caution => "🚨 ".to_string(),
            },
            OutputStyle::NerdFont => {
                let ic = match self {
                    Self::Note => NerdIcon::Info,
                    Self::Tip => NerdIcon::Think,
                    Self::Important => NerdIcon::Bolt,
                    Self::Warning => NerdIcon::Warning,
                    Self::Caution => NerdIcon::Error,
                };
                format!("{} ", nf(ic))
            }
            OutputStyle::Kaomoji => match self {
                Self::Note => "(•‿•) ".to_string(),
                Self::Tip => "(✧ω✧) ".to_string(),
                Self::Important => "(•̀ᴗ•́) ".to_string(),
                Self::Warning => "(⊙_⊙;) ".to_string(),
                Self::Caution => "(×_×) ".to_string(),
            },
            OutputStyle::Ascii => String::new(),
            OutputStyle::Minimal => String::new(),
            OutputStyle::Default
            | OutputStyle::Editorial
            | OutputStyle::Workspace
            | OutputStyle::Posh
            | OutputStyle::Symbols => match self {
                Self::Note => "◉ ".to_string(),
                Self::Tip => "✦ ".to_string(),
                Self::Important => "⚡ ".to_string(),
                Self::Warning => "△ ".to_string(),
                Self::Caution => "✗ ".to_string(),
            },
        }
    }
}

/// Rich terminal Markdown parser and syntax highlighter for minicode
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Renders markdown text into styled Ratatui Lines with a default width of 100 columns
    pub fn render<'a>(text: &'a str, theme: &'a Theme) -> Vec<Line<'a>> {
        Self::render_with_style(text, theme, 100, OutputStyle::Default)
    }

    /// Renders markdown text into styled Ratatui Lines constrained to `max_width` columns
    pub fn render_with_width<'a>(
        text: &'a str,
        theme: &'a Theme,
        max_width: usize,
    ) -> Vec<Line<'a>> {
        Self::render_with_style(text, theme, max_width, OutputStyle::Default)
    }

    /// Renders markdown text with a specific [`OutputStyle`] preset (Default, Minimal/Linear,
    /// Editorial/Report-Skill, Workspace/Notion, Oh My Posh, Nerd Font, Symbols, Emoji, Kaomoji, ASCII).
    pub fn render_with_style<'a>(
        text: &'a str,
        theme: &'a Theme,
        max_width: usize,
        style: OutputStyle,
    ) -> Vec<Line<'a>> {
        let max_width = max_width.max(20);
        let mut lines = Vec::new();
        let mut in_code_block = false;
        let raw_lines: Vec<&'a str> = text.lines().collect();
        let mut i = 0;

        while i < raw_lines.len() {
            let raw_line = raw_lines[i];
            let line = raw_line.trim_end();

            // Code block fence detection
            if line.trim_start().starts_with("```") {
                if in_code_block {
                    in_code_block = false;
                    if let Some(footer) = Self::render_code_fence_close(style, max_width, theme) {
                        lines.push(footer);
                    }
                } else {
                    in_code_block = true;
                    let lang = line.trim_start().strip_prefix("```").unwrap_or("").trim();
                    lines.push(Self::render_code_fence_open(lang, style, max_width, theme));
                }
                i += 1;
                continue;
            }

            // Inside code block: render with syntax styling and style-aware gutter
            if in_code_block {
                let clean_code = Self::normalize_status_glyphs(raw_line);
                let gutter = match style {
                    OutputStyle::Ascii => "  | ",
                    OutputStyle::Posh => "  ▎ ",
                    OutputStyle::Editorial => "│ ",
                    _ => "  │ ",
                };
                let gutter_style = match style {
                    OutputStyle::Posh => Style::default().fg(theme.brand_accent),
                    _ => Style::default().fg(theme.border),
                };
                let code_style = match style {
                    OutputStyle::Minimal | OutputStyle::Ascii => {
                        Style::default().fg(theme.text_primary)
                    }
                    _ => Style::default()
                        .fg(theme.text_primary)
                        .bg(theme.bg_elevated),
                };
                lines.push(Line::from(vec![
                    Span::styled(gutter, gutter_style),
                    Span::styled(clean_code, code_style),
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

            // Blockquote / GitHub & Report-Skill Callout block (`> ...` or `> [!TIP] ...`)
            if line.trim_start().starts_with('>') {
                let mut j = i;
                while j < raw_lines.len() && raw_lines[j].trim_start().starts_with('>') {
                    j += 1;
                }
                lines.extend(Self::render_blockquote_block(
                    &raw_lines[i..j],
                    style,
                    theme,
                ));
                i = j;
                continue;
            }

            // Markdown Table Block detection (e.g. contiguous "| Col 1 | Col 2 |" rows)
            if Self::is_table_row(line) {
                let mut j = i;
                while j < raw_lines.len()
                    && !raw_lines[j].trim_start().starts_with("```")
                    && Self::is_table_row(raw_lines[j].trim_end())
                {
                    j += 1;
                }
                let table_lines = &raw_lines[i..j];
                lines.extend(Self::render_table_block(
                    table_lines,
                    theme,
                    max_width,
                    style,
                ));
                i = j;
                continue;
            }

            // Headings (H1..H4 styled per OutputStyle)
            if let Some(rest) = line.strip_prefix("#### ") {
                lines.extend(Self::render_styled_heading(
                    4, rest, style, max_width, theme,
                ));
            } else if let Some(rest) = line.strip_prefix("### ") {
                lines.extend(Self::render_styled_heading(
                    3, rest, style, max_width, theme,
                ));
            } else if let Some(rest) = line.strip_prefix("## ") {
                lines.extend(Self::render_styled_heading(
                    2, rest, style, max_width, theme,
                ));
            } else if let Some(rest) = line.strip_prefix("# ") {
                lines.extend(Self::render_styled_heading(
                    1, rest, style, max_width, theme,
                ));
            } else if let Some(header_title) = Self::parse_section_header(line) {
                // Standalone bold section header (e.g., "**Recent Commits (latest 20):**" or "**Summary:**")
                lines.push(Self::render_section_header_line(header_title, style, theme));
            } else if let Some((indent, checked, rest)) = Self::parse_checklist_item(line) {
                lines.push(Self::render_checklist_line(
                    indent, checked, rest, style, theme,
                ));
            } else if let Some(rest) = line
                .strip_prefix("    - ")
                .or_else(|| line.strip_prefix("    * "))
                .or_else(|| line.strip_prefix("    • "))
                .or_else(|| line.strip_prefix("  - "))
                .or_else(|| line.strip_prefix("  * "))
                .or_else(|| line.strip_prefix("  • "))
            {
                // Sub-bullet list item
                let sub_marker = match style {
                    OutputStyle::Minimal | OutputStyle::Symbols => "  · ",
                    OutputStyle::Workspace | OutputStyle::Editorial => "  ◦ ",
                    OutputStyle::Ascii => "  - ",
                    _ => "  - ",
                };
                let mut spans = vec![Span::styled(sub_marker, Style::default().fg(theme.muted))];
                spans.extend(Self::parse_inline(rest, theme));
                lines.push(Line::from(spans));
            } else if let Some(rest) = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .or_else(|| line.strip_prefix("• "))
            {
                // Top-level bullet list item
                let bullet = match style {
                    OutputStyle::Minimal => "· ".to_string(),
                    OutputStyle::Editorial | OutputStyle::Workspace | OutputStyle::Symbols => {
                        "▸ ".to_string()
                    }
                    OutputStyle::Posh => "› ".to_string(),
                    OutputStyle::NerdFont => format!("{} ", nf(NerdIcon::Rocket)),
                    OutputStyle::Emoji => "🔹 ".to_string(),
                    OutputStyle::Ascii => "* ".to_string(),
                    OutputStyle::Default | OutputStyle::Kaomoji => "• ".to_string(),
                };
                let bullet_color = match style {
                    OutputStyle::Minimal => theme.muted,
                    _ => theme.brand_accent,
                };
                let mut spans = vec![Span::styled(bullet, Style::default().fg(bullet_color))];
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
                let div_w = max_width.min(56);
                let rule_str = match style {
                    OutputStyle::Ascii => "-".repeat(div_w),
                    OutputStyle::Editorial => "┈".repeat(div_w),
                    OutputStyle::Minimal => "─".repeat(div_w.min(36)),
                    _ => "─".repeat(div_w.min(52)),
                };
                lines.push(Line::from(vec![Span::styled(
                    rule_str,
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

    /// Renders a style-aware, width-responsive opening fence for fenced code blocks.
    fn render_code_fence_open(
        lang: &str,
        style: OutputStyle,
        max_width: usize,
        theme: &Theme,
    ) -> Line<'static> {
        let label = if lang.is_empty() { "code" } else { lang };
        let rule_cap = max_width.saturating_sub(6).clamp(16, 60);
        let border_st = Style::default().fg(theme.border);
        let lang_st = Style::default().fg(theme.info).add_modifier(Modifier::BOLD);

        match style {
            OutputStyle::Minimal => Line::from(vec![
                Span::styled("  │ ", border_st),
                Span::styled(label.to_string(), Style::default().fg(theme.muted)),
            ]),
            OutputStyle::Editorial => {
                let prefix = format!("┌─ {label} ");
                let rem = max_width
                    .saturating_sub(UnicodeWidthStr::width(prefix.as_str()) + 1)
                    .max(4);
                Line::from(vec![
                    Span::styled("┌─ ", border_st),
                    Span::styled(label.to_string(), lang_st),
                    Span::styled(format!(" {}┐", "─".repeat(rem)), border_st),
                ])
            }
            OutputStyle::Workspace => {
                let rem = rule_cap.saturating_sub(label.chars().count() + 4).max(6);
                Line::from(vec![
                    Span::styled("  ╭─ ", border_st),
                    Span::styled(label.to_string(), lang_st),
                    Span::styled(format!(" {}╮", "─".repeat(rem)), border_st),
                ])
            }
            OutputStyle::Posh => {
                let pill_bg = theme.bg_elevated;
                let rem = rule_cap.saturating_sub(label.chars().count() + 5).max(6);
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
                    Span::styled(
                        label.to_string(),
                        Style::default()
                            .fg(theme.info)
                            .bg(pill_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("\u{e0b4} ", Style::default().fg(pill_bg)),
                    Span::styled("─".repeat(rem), border_st),
                ])
            }
            OutputStyle::NerdFont => {
                let ic = nf(NerdIcon::Terminal);
                let rem = rule_cap.saturating_sub(label.chars().count() + 6).max(6);
                Line::from(vec![
                    Span::styled("  ╭─ ", border_st),
                    Span::styled(format!("{ic} {label}"), lang_st),
                    Span::styled(format!(" {}", "─".repeat(rem)), border_st),
                ])
            }
            OutputStyle::Symbols => {
                let rem = rule_cap.saturating_sub(label.chars().count() + 6).max(6);
                Line::from(vec![
                    Span::styled("  ┌─ ", border_st),
                    Span::styled(format!("⌘ {label}"), lang_st),
                    Span::styled(format!(" {}", "─".repeat(rem)), border_st),
                ])
            }
            OutputStyle::Emoji => {
                let rem = rule_cap.saturating_sub(label.chars().count() + 7).max(6);
                Line::from(vec![
                    Span::styled("  ╭─ ", border_st),
                    Span::styled(format!("💻 {label}"), lang_st),
                    Span::styled(format!(" {}", "─".repeat(rem)), border_st),
                ])
            }
            OutputStyle::Kaomoji => {
                let rem = rule_cap.saturating_sub(label.chars().count() + 10).max(6);
                Line::from(vec![
                    Span::styled("  ╭─ ", border_st),
                    Span::styled(format!("(>_<) {label}"), lang_st),
                    Span::styled(format!(" {}", "─".repeat(rem)), border_st),
                ])
            }
            OutputStyle::Ascii => {
                let rem = rule_cap.saturating_sub(label.chars().count() + 5).max(6);
                Line::from(vec![Span::styled(
                    format!("  +-- {label} {}", "-".repeat(rem)),
                    border_st,
                )])
            }
            OutputStyle::Default => {
                let rem = rule_cap.saturating_sub(label.chars().count() + 4).max(12);
                Line::from(vec![Span::styled(
                    format!("  ┌─ {label} {}", "─".repeat(rem)),
                    border_st,
                )])
            }
        }
    }

    /// Renders the closing fence for fenced code blocks (returns `None` for open-rail styles).
    fn render_code_fence_close(
        style: OutputStyle,
        max_width: usize,
        theme: &Theme,
    ) -> Option<Line<'static>> {
        let rule_cap = max_width.saturating_sub(6).clamp(16, 60);
        let border_st = Style::default().fg(theme.border);
        match style {
            OutputStyle::Minimal | OutputStyle::Posh => None,
            OutputStyle::Editorial => {
                let inner = max_width.saturating_sub(2).max(12);
                Some(Line::from(vec![Span::styled(
                    format!("└{}┘", "─".repeat(inner)),
                    border_st,
                )]))
            }
            OutputStyle::Workspace => Some(Line::from(vec![Span::styled(
                format!("  ╰{}╯", "─".repeat(rule_cap)),
                border_st,
            )])),
            OutputStyle::NerdFont | OutputStyle::Emoji | OutputStyle::Kaomoji => {
                Some(Line::from(vec![Span::styled(
                    format!("  ╰{}", "─".repeat(rule_cap)),
                    border_st,
                )]))
            }
            OutputStyle::Ascii => Some(Line::from(vec![Span::styled(
                format!("  +{}", "-".repeat(rule_cap)),
                border_st,
            )])),
            OutputStyle::Default | OutputStyle::Symbols => Some(Line::from(vec![Span::styled(
                format!("  └{}", "─".repeat(rule_cap)),
                border_st,
            )])),
        }
    }

    /// Renders a contiguous blockquote or GitHub/Report-Skill callout block (`> [!TIP] ...`).
    fn render_blockquote_block<'a>(
        quote_lines: &[&'a str],
        style: OutputStyle,
        theme: &'a Theme,
    ) -> Vec<Line<'a>> {
        if quote_lines.is_empty() {
            return Vec::new();
        }
        let strip_gt = |l: &'a str| -> &'a str {
            let t = l.trim_start();
            let rest = t.strip_prefix('>').unwrap_or(t);
            rest.strip_prefix(' ').unwrap_or(rest).trim_end()
        };

        let first_inner = strip_gt(quote_lines[0]);
        let callout = CalloutKind::parse_tag(first_inner);

        let rail = match style {
            OutputStyle::Ascii => "> ",
            OutputStyle::Minimal => "│ ",
            OutputStyle::Editorial | OutputStyle::Posh => "┃ ",
            _ => "▎ ",
        };

        let mut out = Vec::with_capacity(quote_lines.len() + 1);
        if let Some((kind, inline_rest)) = callout {
            let accent = kind.color(theme);
            let rail_st = Style::default().fg(accent).add_modifier(Modifier::BOLD);
            let prefix = kind.badge_prefix(style);
            let mut header_spans = vec![Span::styled(rail, rail_st)];
            match style {
                OutputStyle::Posh => {
                    let pill_bg = theme.bg_elevated;
                    header_spans.push(Span::styled("\u{e0b6}", Style::default().fg(pill_bg)));
                    header_spans.push(Span::styled(
                        format!("{}{}", prefix, kind.label()),
                        Style::default()
                            .fg(accent)
                            .bg(pill_bg)
                            .add_modifier(Modifier::BOLD),
                    ));
                    header_spans.push(Span::styled("\u{e0b4}", Style::default().fg(pill_bg)));
                }
                OutputStyle::Ascii => {
                    header_spans.push(Span::styled(
                        format!("[{}]", kind.label()),
                        Style::default().fg(accent).add_modifier(Modifier::BOLD),
                    ));
                }
                _ => {
                    header_spans.push(Span::styled(
                        format!("{}{}", prefix, kind.label()),
                        Style::default().fg(accent).add_modifier(Modifier::BOLD),
                    ));
                }
            }
            if !inline_rest.is_empty() {
                header_spans.push(Span::raw(" "));
                header_spans.extend(Self::parse_inline(inline_rest, theme));
            }
            out.push(Line::from(header_spans));

            for &raw_q in &quote_lines[1..] {
                let body = strip_gt(raw_q);
                let mut spans = vec![Span::styled(rail, Style::default().fg(accent))];
                if !body.is_empty() {
                    spans.extend(Self::parse_inline(body, theme));
                }
                out.push(Line::from(spans));
            }
        } else {
            let rail_st = Style::default().fg(theme.border);
            for &raw_q in quote_lines {
                let body = strip_gt(raw_q);
                let mut spans = vec![Span::styled(rail, rail_st)];
                if !body.is_empty() {
                    for mut sp in Self::parse_inline(body, theme) {
                        if sp.style.bg.is_none() {
                            sp.style = sp.style.fg(theme.muted).add_modifier(Modifier::ITALIC);
                        }
                        spans.push(sp);
                    }
                }
                out.push(Line::from(spans));
            }
        }
        out
    }

    /// Parses Markdown task list items (`- [x] done`, `- [ ] todo`, with optional 2/4-space indent).
    fn parse_checklist_item(line: &str) -> Option<(usize, bool, &str)> {
        let indent = if line.starts_with("    ") {
            4
        } else if line.starts_with("  ") {
            2
        } else {
            0
        };
        let trimmed = &line[indent..];
        let after_bullet = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))?;
        if let Some(rest) = after_bullet
            .strip_prefix("[x] ")
            .or_else(|| after_bullet.strip_prefix("[X] "))
        {
            Some((indent, true, rest))
        } else if let Some(rest) = after_bullet.strip_prefix("[ ] ") {
            Some((indent, false, rest))
        } else {
            None
        }
    }

    /// Renders a styled task checklist row.
    fn render_checklist_line<'a>(
        indent: usize,
        checked: bool,
        rest: &'a str,
        style: OutputStyle,
        theme: &'a Theme,
    ) -> Line<'a> {
        let pad = " ".repeat(indent);
        let (marker, marker_style) = match (style, checked) {
            (OutputStyle::Ascii, true) => (
                "[x] ".to_string(),
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            (OutputStyle::Ascii, false) => ("[ ] ".to_string(), Style::default().fg(theme.muted)),
            (OutputStyle::Emoji, true) => ("✅ ".to_string(), Style::default().fg(theme.success)),
            (OutputStyle::Emoji, false) => ("⬜ ".to_string(), Style::default().fg(theme.muted)),
            (OutputStyle::NerdFont, true) => (
                format!("{} ", nf(NerdIcon::Check)),
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            (OutputStyle::NerdFont, false) => ("○ ".to_string(), Style::default().fg(theme.muted)),
            (_, true) => (
                "✓ ".to_string(),
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            (_, false) => ("○ ".to_string(), Style::default().fg(theme.muted)),
        };

        let mut spans = Vec::new();
        if !pad.is_empty() {
            spans.push(Span::raw(pad));
        }
        spans.push(Span::styled(marker, marker_style));
        spans.extend(Self::parse_inline(rest, theme));
        Line::from(spans)
    }

    /// Renders a standalone bold section header (`**Summary:**`) according to `OutputStyle`.
    fn render_section_header_line(
        header_title: String,
        style: OutputStyle,
        theme: &Theme,
    ) -> Line<'static> {
        let base_st = Style::default()
            .fg(theme.highlight)
            .add_modifier(Modifier::BOLD);
        match style {
            OutputStyle::Default => Line::from(vec![Span::styled(header_title, base_st)]),
            OutputStyle::Symbols => Line::from(vec![
                Span::styled("▸ ", Style::default().fg(theme.brand_accent)),
                Span::styled(header_title, base_st),
            ]),
            OutputStyle::NerdFont => Line::from(vec![
                Span::styled(
                    format!("{} ", nf(NerdIcon::Symbol)),
                    Style::default().fg(theme.brand_accent),
                ),
                Span::styled(header_title, base_st),
            ]),
            OutputStyle::Emoji => Line::from(vec![
                Span::styled("🔹 ", Style::default().fg(theme.brand_accent)),
                Span::styled(header_title, base_st),
            ]),
            OutputStyle::Ascii => {
                Line::from(vec![Span::styled(format!("> {header_title}"), base_st)])
            }
            _ => Line::from(vec![Span::styled(header_title, base_st)]),
        }
    }

    /// Renders Markdown headings H1–H4 according to `OutputStyle`.
    fn render_styled_heading<'a>(
        level: u8,
        rest: &'a str,
        style: OutputStyle,
        max_width: usize,
        theme: &'a Theme,
    ) -> Vec<Line<'a>> {
        let heading_style = match level {
            1 => Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            2 => Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
            3 => Style::default()
                .fg(theme.highlight)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        };

        // Preserve exact Default rendering for backward compatibility with existing tests & users.
        if style == OutputStyle::Default {
            return vec![Self::render_heading_line(rest, heading_style, theme)];
        }

        let base_line = Self::render_heading_line(
            rest,
            match level {
                1 => Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
                2 => Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
                3 => Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
                _ => Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
            },
            theme,
        );
        let text_w: usize = base_line
            .spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();

        match style {
            OutputStyle::Minimal => {
                let mut out = vec![base_line];
                if level <= 2 {
                    let rule_w = text_w.max(12).min(max_width);
                    out.push(Line::from(vec![Span::styled(
                        "─".repeat(rule_w),
                        Style::default().fg(theme.border),
                    )]));
                }
                out
            }
            OutputStyle::Editorial => match level {
                1 => {
                    let rule_w = text_w.max(16).min(max_width);
                    vec![
                        base_line,
                        Line::from(vec![Span::styled(
                            "═".repeat(rule_w),
                            Style::default().fg(theme.brand_accent),
                        )]),
                    ]
                }
                2 => {
                    let rem = max_width.saturating_sub(text_w + 1).min(36);
                    let mut spans = base_line.spans;
                    if rem >= 3 {
                        spans.push(Span::styled(
                            format!(" {}", "─".repeat(rem)),
                            Style::default().fg(theme.border),
                        ));
                    }
                    vec![Line::from(spans)]
                }
                3 => {
                    let mut spans = vec![Span::styled("▸ ", Style::default().fg(theme.highlight))];
                    spans.extend(base_line.spans);
                    vec![Line::from(spans)]
                }
                _ => vec![base_line],
            },
            OutputStyle::Workspace => {
                let prefix = match level {
                    1 => "▸ ",
                    2 => "› ",
                    _ => "· ",
                };
                let mut spans = vec![Span::styled(
                    prefix,
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                )];
                spans.extend(base_line.spans);
                let mut out = vec![Line::from(spans)];
                if level == 1 {
                    let rule_w = (text_w + 2).min(max_width);
                    out.push(Line::from(vec![Span::styled(
                        "─".repeat(rule_w),
                        Style::default().fg(theme.border),
                    )]));
                }
                out
            }
            OutputStyle::Posh => {
                let pill_bg = theme.bg_elevated;
                let plain: String = base_line.spans.iter().map(|s| s.content.as_ref()).collect();
                match level {
                    1 => vec![Line::from(vec![
                        Span::styled("\u{e0b6}", Style::default().fg(pill_bg)),
                        Span::styled(
                            format!(" {plain} "),
                            Style::default()
                                .fg(theme.brand_accent)
                                .bg(pill_bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("\u{e0b4}", Style::default().fg(pill_bg)),
                    ])],
                    2 => vec![Line::from(vec![
                        Span::styled(
                            format!(" {plain} "),
                            Style::default()
                                .fg(theme.highlight)
                                .bg(pill_bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("\u{e0b0}", Style::default().fg(pill_bg)),
                    ])],
                    _ => {
                        let mut spans = vec![Span::styled("▎ ", Style::default().fg(theme.info))];
                        spans.extend(base_line.spans);
                        vec![Line::from(spans)]
                    }
                }
            }
            OutputStyle::NerdFont => {
                let ic = match level {
                    1 => nf(NerdIcon::Sparkle),
                    2 => nf(NerdIcon::Symbol),
                    3 => nf(NerdIcon::File),
                    _ => nf(NerdIcon::Info),
                };
                let mut spans = vec![Span::styled(
                    format!("{ic} "),
                    Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                )];
                spans.extend(base_line.spans);
                vec![Line::from(spans)]
            }
            OutputStyle::Symbols => {
                let sym = match level {
                    1 => "◈ ",
                    2 => "▸ ",
                    3 => "› ",
                    _ => "· ",
                };
                let mut spans = vec![Span::styled(
                    sym,
                    Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                )];
                spans.extend(base_line.spans);
                vec![Line::from(spans)]
            }
            OutputStyle::Emoji => {
                let em = match level {
                    1 => "📌 ",
                    2 => "🔹 ",
                    3 => "▸ ",
                    _ => "· ",
                };
                let mut spans = vec![Span::styled(em, Style::default().fg(theme.brand_accent))];
                spans.extend(base_line.spans);
                vec![Line::from(spans)]
            }
            OutputStyle::Kaomoji => {
                let prefix = match level {
                    1 => "(✦‿✦) ",
                    2 => "▸ ",
                    _ => "› ",
                };
                let mut spans = vec![Span::styled(
                    prefix,
                    Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                )];
                spans.extend(base_line.spans);
                vec![Line::from(spans)]
            }
            OutputStyle::Ascii => {
                let plain: String = base_line.spans.iter().map(|s| s.content.as_ref()).collect();
                match level {
                    1 => vec![Line::from(vec![Span::styled(
                        format!("== {plain} =="),
                        heading_style.remove_modifier(Modifier::UNDERLINED),
                    )])],
                    2 => vec![Line::from(vec![Span::styled(
                        format!("-- {plain} --"),
                        heading_style,
                    )])],
                    _ => vec![Line::from(vec![Span::styled(
                        format!("> {plain}"),
                        heading_style,
                    )])],
                }
            }
            OutputStyle::Default => unreachable!(),
        }
    }

    /// Normalizes wide multi-cell emojis, variation selectors, tabs, and ambiguous-width
    /// Unicode symbols into strictly 1-cell monospace terminal glyphs (`✓`, `✗`, `!`, `→`)
    /// so terminal cell widths never drift or corrupt differential redraws on scroll.
    pub fn normalize_status_glyphs(text: &str) -> String {
        if text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
            return text.to_string();
        }

        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        let skip_variation_selectors = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
            while matches!(chars.peek(), Some('\u{fe00}'..='\u{fe0f}')) {
                chars.next();
            }
        };

        while let Some(ch) = chars.next() {
            match ch {
                // Expand tabs to 4 spaces so terminal tab stops never desync buffer columns
                '\t' => out.push_str("    "),
                // Strip carriage returns, zero-width spaces/joiners, and variation selectors
                '\r'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{fe00}'..='\u{fe0f}'
                | '\u{feff}'
                | '\u{20e3}'
                | '\u{1f3fb}'..='\u{1f3ff}' => continue,
                // Pass / check icons -> strictly 1-cell Emoji=No `✓` (U+2713)
                '✅' | '☑' | '✔' | '✓' => {
                    skip_variation_selectors(&mut chars);
                    out.push('✓');
                }
                // Fail / error icons -> strictly 1-cell Emoji=No `✗` (U+2717)
                '❌' | '❎' | '✖' | '✕' | '✘' | '✗' => {
                    skip_variation_selectors(&mut chars);
                    out.push('✗');
                }
                // Warning / alert icons -> strictly 1-cell `!`
                '⚠' | '🚨' | '🛑' | '❗' | '❕' => {
                    skip_variation_selectors(&mut chars);
                    out.push('!');
                }
                // Info icon (`ℹ` U+2139 has Emoji=Yes, EAW=N -> 2 cells in emoji fonts) -> strictly 1-cell `•`
                'ℹ' => {
                    skip_variation_selectors(&mut chars);
                    out.push('•');
                }
                // Action / launch / pointer icons -> strictly 1-cell `→` (U+2192, Emoji=No)
                '🚀' | '⚡' | '▶' | '►' | '➜' | '➔' | '❯' | '👉' => {
                    skip_variation_selectors(&mut chars);
                    out.push('→');
                }
                '▼' => {
                    skip_variation_selectors(&mut chars);
                    out.push('▾');
                }
                // Strip decorative multi-cell / ambiguous-width emojis across SMP & Misc blocks
                c if Self::is_decorative_emoji_or_symbol(c) => {
                    while matches!(
                        chars.peek(),
                        Some('\u{fe00}'..='\u{fe0f}' | '\u{200d}' | '\u{1f3fb}'..='\u{1f3ff}')
                    ) {
                        let next_c = chars.next();
                        if next_c == Some('\u{200d}') {
                            // Consume ZWJ linked emoji
                            let _ = chars.next();
                        }
                    }
                    if chars.peek() == Some(&' ')
                        && (out.is_empty()
                            || out.ends_with(' ')
                            || out.ends_with("**")
                            || out.ends_with('['))
                    {
                        chars.next();
                    }
                }
                other if !other.is_control() => out.push(other),
                _ => {}
            }
        }

        out
    }

    /// Returns true if `c` is a decorative emoji or ambiguous-width symbol that can render
    /// as 2 cells in terminal emulators while reporting 1 or 2 cells inconsistently.
    #[inline]
    pub fn is_decorative_emoji_or_symbol(c: char) -> bool {
        matches!(
            c,
            // Miscellaneous Technical (watch, hourglass, media symbols, etc.)
            '\u{2300}'..='\u{23ff}'
            // Small/Medium white/black squares with Emoji_Presentation=Yes
            | '\u{25fd}'..='\u{25fe}'
            // Miscellaneous Symbols & Dingbats (except ✓ U+2713 and ✗ U+2717 which are handled above)
            | '\u{2600}'..='\u{27bf}'
            // Supplemental Arrows, Misc Symbols and Arrows
            | '\u{2b00}'..='\u{2bff}'
            // Enclosed Alphanumerics / CJK symbols / Mahjong / Domino / Playing Cards / Pictographs
            | '\u{1f000}'..='\u{1ffff}'
        )
    }

    /// Renders a Markdown heading line without any `#` characters, applying the
    /// heading level's color and font weight/modifiers across all spans.
    fn render_heading_line<'a>(rest: &'a str, heading_style: Style, theme: &'a Theme) -> Line<'a> {
        let trimmed = rest.trim();
        let unwrapped = if let Some(inner) = trimmed
            .strip_prefix("**")
            .and_then(|s| s.strip_suffix("**"))
        {
            if !inner.contains("**") && !inner.trim().is_empty() {
                inner.trim()
            } else {
                trimmed
            }
        } else {
            trimmed
        };

        let spans: Vec<Span<'a>> = Self::parse_inline(unwrapped, theme)
            .into_iter()
            .map(|mut span| {
                if span.style.bg.is_some() {
                    span.style = span.style.add_modifier(Modifier::BOLD);
                } else {
                    span.style = heading_style;
                }
                span
            })
            .collect();

        Line::from(spans)
    }

    /// Safely detects and parses standalone bold section headers (e.g. "**Recent Commits:**" or "**Summary**")
    fn parse_section_header(line: &str) -> Option<String> {
        if line.len() < 5 {
            return None;
        }
        if let Some(without_prefix) = line.strip_prefix("**") {
            if let Some(inner) = without_prefix.strip_suffix("**:") {
                if !inner.contains("**") && !inner.trim().is_empty() {
                    let clean = Self::normalize_status_glyphs(inner.trim());
                    return Some(format!("{}:", clean.trim()));
                }
            } else if let Some(inner) = without_prefix.strip_suffix("**") {
                if !inner.contains("**") && !inner.trim().is_empty() {
                    let clean = Self::normalize_status_glyphs(inner.trim());
                    return Some(clean.trim().to_string());
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

    /// Splits a Markdown table row on unescaped `|` characters outside of inline `` `code` `` spans.
    fn split_table_cells(line: &str) -> Vec<String> {
        let trimmed = line.trim();
        let inner = trimmed
            .strip_prefix('|')
            .unwrap_or(trimmed)
            .strip_suffix('|')
            .unwrap_or(trimmed);

        let mut cells = Vec::new();
        let mut current = String::new();
        let mut in_code = false;
        let mut chars = inner.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(&next_ch) = chars.peek() {
                    if next_ch == '|' || next_ch == '\\' || next_ch == '`' {
                        current.push(next_ch);
                        chars.next();
                        continue;
                    }
                }
                current.push(ch);
            } else if ch == '`' {
                in_code = !in_code;
                current.push(ch);
            } else if ch == '|' && !in_code {
                cells.push(current.trim().to_string());
                current.clear();
            } else {
                current.push(ch);
            }
        }
        cells.push(current.trim().to_string());
        cells
    }

    /// Wraps styled `Span`s within a table cell to fit within `col_width` visual columns.
    fn wrap_cell_spans<'a>(spans: &[Span<'a>], col_width: usize) -> Vec<Vec<Span<'a>>> {
        use unicode_width::UnicodeWidthChar;

        let col_width = col_width.max(1);
        let total_w: usize = spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        if total_w <= col_width {
            return vec![spans.to_vec()];
        }

        // Flatten spans into (char, Style) pairs so multi-span words wrap cleanly
        let mut styled_chars: Vec<(char, Style)> = Vec::new();
        for span in spans {
            for ch in span.content.chars() {
                styled_chars.push((ch, span.style));
            }
        }

        // Group into contiguous whitespace vs non-whitespace runs
        struct Run {
            chars: Vec<(char, Style)>,
            is_ws: bool,
            width: usize,
        }

        let mut runs: Vec<Run> = Vec::new();
        for (ch, style) in styled_chars {
            let is_ws = ch.is_whitespace();
            let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if let Some(last) = runs.last_mut() {
                if last.is_ws == is_ws {
                    last.chars.push((ch, style));
                    last.width += ch_w;
                    continue;
                }
            }
            runs.push(Run {
                chars: vec![(ch, style)],
                is_ws,
                width: ch_w,
            });
        }

        let push_char = |line_spans: &mut Vec<Span<'a>>, ch: char, style: Style| {
            if let Some(last) = line_spans.last_mut() {
                if last.style == style {
                    last.content.to_mut().push(ch);
                    return;
                }
            }
            let mut s = String::with_capacity(4);
            s.push(ch);
            line_spans.push(Span::styled(s, style));
        };

        let mut lines: Vec<Vec<Span<'a>>> = Vec::new();
        let mut cur_spans: Vec<Span<'a>> = Vec::new();
        let mut cur_w: usize = 0;

        for run in runs {
            if run.is_ws {
                if cur_w == 0 {
                    continue;
                } else if cur_w + run.width <= col_width {
                    for (ch, st) in run.chars {
                        push_char(&mut cur_spans, ch, st);
                    }
                    cur_w += run.width;
                } else {
                    lines.push(std::mem::take(&mut cur_spans));
                    cur_w = 0;
                }
            } else if cur_w + run.width <= col_width {
                for (ch, st) in run.chars {
                    push_char(&mut cur_spans, ch, st);
                }
                cur_w += run.width;
            } else if run.width > col_width {
                // Single word wider than col_width: break by character
                for (ch, st) in run.chars {
                    let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if cur_w + ch_w > col_width && cur_w > 0 {
                        lines.push(std::mem::take(&mut cur_spans));
                        cur_w = 0;
                    }
                    push_char(&mut cur_spans, ch, st);
                    cur_w += ch_w;
                }
            } else {
                if !cur_spans.is_empty() {
                    lines.push(std::mem::take(&mut cur_spans));
                }
                for (ch, st) in run.chars {
                    push_char(&mut cur_spans, ch, st);
                }
                cur_w = run.width;
            }
        }

        if !cur_spans.is_empty() || lines.is_empty() {
            lines.push(cur_spans);
        }

        lines
    }

    /// Returns true if a table cell looks like a numeric / latency / ratio / percentage metric
    /// (used for Report-Skill style automatic right-alignment on editorial/minimal/workspace tables).
    fn cell_looks_numeric(raw: &str) -> bool {
        let t = raw.trim().trim_matches('*').trim_matches('`').trim();
        if t.is_empty() || t == "-" || t == "—" {
            return true;
        }
        let cleaned = t
            .trim_start_matches(['+', '-', '$', '~', '<', '>'])
            .trim_end_matches(['%', 's', 'k', 'M', 'K', 'B', 'm']);
        !cleaned.is_empty()
            && cleaned
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '/' | ':'))
            && cleaned.chars().any(|c| c.is_ascii_digit())
    }

    /// Renders a full contiguous block of markdown table rows with character-aligned columns,
    /// style-aware borders (full box, booktabs hairline, rounded, or ASCII), proportional viewport
    /// fitting, auto right-aligned numeric columns, and multi-line cell wrapping.
    fn render_table_block<'a>(
        table_lines: &[&'a str],
        theme: &'a Theme,
        max_width: usize,
        style: OutputStyle,
    ) -> Vec<Line<'a>> {
        if table_lines.is_empty() {
            return Vec::new();
        }

        struct ParsedRow {
            raw_cells: Vec<String>,
            is_separator: bool,
        }

        let mut parsed_rows = Vec::new();
        for &line in table_lines {
            let raw_cells = Self::split_table_cells(line);
            if raw_cells.is_empty() {
                continue;
            }

            let is_sep = !raw_cells.is_empty()
                && raw_cells.iter().all(|cell| {
                    let t = cell.trim();
                    !t.is_empty() && t.chars().all(|c| c == '-' || c == ':')
                });

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
        let mut explicit_align = vec![false; num_cols];
        if let Some(sep_row) = parsed_rows.iter().find(|r| r.is_separator) {
            for (c, cell) in sep_row.raw_cells.iter().enumerate().take(num_cols) {
                let trimmed = cell.trim();
                let starts_colon = trimmed.starts_with(':');
                let ends_colon = trimmed.ends_with(':');
                if starts_colon && ends_colon {
                    alignments[c] = ColAlign::Center;
                    explicit_align[c] = true;
                } else if ends_colon {
                    alignments[c] = ColAlign::Right;
                    explicit_align[c] = true;
                } else if starts_colon {
                    alignments[c] = ColAlign::Left;
                    explicit_align[c] = true;
                }
            }
        }

        let first_is_header =
            parsed_rows.len() > 1 && parsed_rows.get(1).is_some_and(|r| r.is_separator);

        // Auto right-align numeric/metric columns on editorial, minimal, and workspace styles
        if matches!(
            style,
            OutputStyle::Minimal | OutputStyle::Editorial | OutputStyle::Workspace
        ) {
            for c in 0..num_cols {
                if explicit_align[c] {
                    continue;
                }
                let mut saw_value = false;
                let mut all_numeric = true;
                for (row_idx, row) in parsed_rows.iter().enumerate() {
                    if row.is_separator || (row_idx == 0 && first_is_header) {
                        continue;
                    }
                    let val = row.raw_cells.get(c).map(|s| s.as_str()).unwrap_or("");
                    if !val.trim().is_empty() {
                        saw_value = true;
                        if !Self::cell_looks_numeric(val) {
                            all_numeric = false;
                            break;
                        }
                    }
                }
                if saw_value && all_numeric {
                    alignments[c] = ColAlign::Right;
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
                    let cell_text = row.raw_cells.get(c).map(|s| s.as_str()).unwrap_or("");
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

        let is_booktabs = matches!(style, OutputStyle::Minimal | OutputStyle::Editorial);

        // Constrain total table width to `max_width` so borders never wrap across terminal lines.
        let border_overhead = if is_booktabs {
            num_cols.saturating_sub(1) * 2
        } else {
            3 * num_cols + 1
        };
        let avail_content_w = max_width.saturating_sub(border_overhead).max(num_cols * 3);
        let natural_total: usize = col_widths.iter().sum();

        if natural_total > avail_content_w {
            let min_col_w = 4usize.min(avail_content_w / num_cols.max(1)).max(3);
            let fair_share = (avail_content_w / num_cols.max(1)).max(min_col_w);

            // Lock columns that already fit comfortably within fair_share (or <= 24 cols for label columns)
            let mut locked = vec![false; num_cols];
            let mut used_by_locked = 0usize;
            for c in 0..num_cols {
                let cap = if c == 0 && num_cols > 1 {
                    fair_share.max(22).min(avail_content_w / 2)
                } else {
                    fair_share
                };
                if col_widths[c] <= cap {
                    locked[c] = true;
                    used_by_locked += col_widths[c];
                }
            }

            let remaining_budget = avail_content_w.saturating_sub(used_by_locked);
            let unlocked_natural_sum: usize = (0..num_cols)
                .filter(|&c| !locked[c])
                .map(|c| col_widths[c])
                .sum();

            if unlocked_natural_sum > 0 && remaining_budget > 0 {
                for c in 0..num_cols {
                    if !locked[c] {
                        let scaled = (col_widths[c] * remaining_budget) / unlocked_natural_sum;
                        col_widths[c] = scaled.max(min_col_w);
                    }
                }
            } else {
                col_widths.fill(fair_share);
            }

            // Final pass: clamp any excess due to rounding
            while col_widths.iter().sum::<usize>() > avail_content_w {
                if let Some((max_idx, _)) = col_widths
                    .iter()
                    .enumerate()
                    .filter(|(_, &w)| w > min_col_w)
                    .max_by_key(|(_, &w)| w)
                {
                    col_widths[max_idx] -= 1;
                } else {
                    break;
                }
            }
        }

        let mut out = Vec::new();
        let border_style = Style::default().fg(theme.border);

        if is_booktabs {
            let total_w: usize = col_widths.iter().sum::<usize>() + num_cols.saturating_sub(1) * 2;
            let rule_ch = if style == OutputStyle::Editorial {
                "━"
            } else {
                "─"
            };
            out.push(Line::from(vec![Span::styled(
                rule_ch.repeat(total_w),
                border_style,
            )]));

            for (row_idx, row) in parsed_rows.iter().enumerate() {
                if row.is_separator {
                    out.push(Line::from(vec![Span::styled(
                        "─".repeat(total_w),
                        border_style,
                    )]));
                } else if let Some(ref cells_spans) = row_spans[row_idx] {
                    let is_header = row_idx == 0 && first_is_header;
                    let wrapped_cells: Vec<Vec<Vec<Span<'a>>>> = (0..num_cols)
                        .map(|c| Self::wrap_cell_spans(&cells_spans[c], col_widths[c]))
                        .collect();
                    let max_sublines = wrapped_cells
                        .iter()
                        .map(|lines| lines.len())
                        .max()
                        .unwrap_or(1)
                        .max(1);

                    for sub_idx in 0..max_sublines {
                        let mut row_line = Vec::new();
                        for c in 0..num_cols {
                            if c > 0 {
                                row_line.push(Span::raw("  "));
                            }
                            let empty_spans: Vec<Span<'a>> = Vec::new();
                            let spans = wrapped_cells[c].get(sub_idx).unwrap_or(&empty_spans);
                            let visible_w: usize = spans
                                .iter()
                                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                                .sum();
                            let pad_total = col_widths[c].saturating_sub(visible_w);
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
                                    if st.fg.is_none() || st.fg == Some(theme.text_primary) {
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
                        out.push(Line::from(row_line));
                    }
                }
            }

            out.push(Line::from(vec![Span::styled(
                "─".repeat(total_w),
                border_style,
            )]));
            return out;
        }

        let (h_fill, tl, tm, tr, ml, mm, mr, bl, bm, br, v_left, v_mid, v_right) = match style {
            OutputStyle::Workspace | OutputStyle::Posh => (
                "─",
                "╭─",
                "─┬─",
                "─╮",
                "├─",
                "─┼─",
                "─┤",
                "╰─",
                "─┴─",
                "─╯",
                "│ ",
                " │ ",
                " │",
            ),
            OutputStyle::Ascii => (
                "-", "+-", "-+-", "-+", "+-", "-+-", "-+", "+-", "-+-", "-+", "| ", " | ", " |",
            ),
            _ => (
                "─",
                "┌─",
                "─┬─",
                "─┐",
                "├─",
                "─┼─",
                "─┤",
                "└─",
                "─┴─",
                "─┘",
                "│ ",
                " │ ",
                " │",
            ),
        };

        let build_table_border =
            |left: &'static str, mid: &'static str, right: &'static str| -> Line<'a> {
                let mut line = Vec::with_capacity(col_widths.len() * 2 + 2);
                line.push(Span::styled(left, border_style));
                for (c, &w) in col_widths.iter().enumerate() {
                    if c > 0 {
                        line.push(Span::styled(mid, border_style));
                    }
                    line.push(Span::styled(h_fill.repeat(w), border_style));
                }
                line.push(Span::styled(right, border_style));
                Line::from(line)
            };

        // 1. Top border
        out.push(build_table_border(tl, tm, tr));

        // 2. Render rows (with multi-line cell wrapping when a cell exceeds `col_widths[c]`)
        for (row_idx, row) in parsed_rows.iter().enumerate() {
            if row.is_separator {
                out.push(build_table_border(ml, mm, mr));
            } else if let Some(ref cells_spans) = row_spans[row_idx] {
                let is_header = row_idx == 0 && first_is_header;
                let wrapped_cells: Vec<Vec<Vec<Span<'a>>>> = (0..num_cols)
                    .map(|c| Self::wrap_cell_spans(&cells_spans[c], col_widths[c]))
                    .collect();
                let max_sublines = wrapped_cells
                    .iter()
                    .map(|lines| lines.len())
                    .max()
                    .unwrap_or(1)
                    .max(1);

                for sub_idx in 0..max_sublines {
                    let mut row_line = Vec::new();
                    row_line.push(Span::styled(v_left, border_style));

                    for c in 0..num_cols {
                        if c > 0 {
                            row_line.push(Span::styled(v_mid, border_style));
                        }

                        let empty_spans: Vec<Span<'a>> = Vec::new();
                        let spans = wrapped_cells[c].get(sub_idx).unwrap_or(&empty_spans);
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

                    row_line.push(Span::styled(v_right, border_style));
                    out.push(Line::from(row_line));
                }
            }
        }

        // 3. Bottom border
        out.push(build_table_border(bl, bm, br));

        out
    }

    /// Parses inline Markdown styling (bold, code, paths, URLs, metrics) safely
    pub fn parse_inline<'a>(text: &str, theme: &'a Theme) -> Vec<Span<'a>> {
        let normalized = Self::normalize_status_glyphs(text);
        let mut spans = Vec::new();
        let mut current_buf = String::new();

        let flush_buf = |buf: &mut String, spans: &mut Vec<Span<'a>>| {
            if !buf.is_empty() {
                let token_spans = Self::highlight_tokens(buf, theme);
                spans.extend(token_spans);
                buf.clear();
            }
        };

        let mut rem = normalized.as_str();
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

            // 2b. Strikethrough: ~~text~~
            if rem.starts_with("~~") {
                if let Some(end_idx) = rem[2..].find("~~") {
                    if end_idx > 0 {
                        let strike_content = &rem[2..2 + end_idx];
                        flush_buf(&mut current_buf, &mut spans);
                        spans.push(Span::styled(
                            strike_content.to_string(),
                            Style::default()
                                .fg(theme.muted)
                                .add_modifier(Modifier::CROSSED_OUT),
                        ));
                        rem = &rem[2 + end_idx + 2..];
                        continue;
                    }
                }
            }

            // 2c. Italic: *text* (non-empty, not starting with whitespace or '*')
            if rem.starts_with('*') && !rem.starts_with("**") && !rem.starts_with("* ") {
                if let Some(end_idx) = rem[1..].find('*') {
                    let italic_content = &rem[1..1 + end_idx];
                    if !italic_content.is_empty() && !italic_content.ends_with(' ') {
                        flush_buf(&mut current_buf, &mut spans);
                        spans.push(Span::styled(
                            italic_content.to_string(),
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(Modifier::ITALIC),
                        ));
                        rem = &rem[1 + end_idx + 1..];
                        continue;
                    }
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

    /// Highlights individual word tokens (file paths, URLs, metrics, versions, keywords, status icons)
    fn highlight_tokens<'a>(text: &str, theme: &'a Theme) -> Vec<Span<'a>> {
        let mut spans = Vec::new();
        let words =
            text.split_inclusive(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')');

        for word in words {
            let trimmed = word.trim_matches(|c: char| {
                c.is_whitespace()
                    || c == ','
                    || c == '('
                    || c == ')'
                    || c == ';'
                    || c == '"'
                    || c == '\''
            });

            if trimmed == "✔" || trimmed == "✓" {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if trimmed == "✖" || trimmed == "✗" {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default()
                        .fg(theme.destructive)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if trimmed == "!" {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if trimmed == "→" {
                spans.push(Span::styled(
                    word.to_string(),
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if Self::is_file_path(trimmed) {
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

    #[test]
    fn test_markdown_normalize_status_glyphs_and_rendering() {
        let theme = Theme::default();
        let input = "\
**✨ Verification results:**
- ✅ `bun run build` — clean, 0 TypeScript errors
- ⚠️ 8 non-blocking lint warnings
- ❌ Build failed on step 2
- 🚀 Dev server running";

        let lines = MarkdownRenderer::render(input, &theme);
        let rendered_text: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        assert_eq!(rendered_text[0], "Verification results:");
        assert_eq!(
            rendered_text[1],
            "• ✓ bun run build — clean, 0 TypeScript errors"
        );
        assert_eq!(rendered_text[2], "• ! 8 non-blocking lint warnings");
        assert_eq!(rendered_text[3], "• ✗ Build failed on step 2");
        assert_eq!(rendered_text[4], "• → Dev server running");
    }

    #[test]
    fn test_markdown_wide_table_wraps_inside_columns_without_breaking_borders() {
        let theme = Theme::default();
        let markdown = "\
| Section | Highlights |
|---|---|
| **Navbar** | Sticky glass header, backdrop-blur, mobile menu, GitHub link |
| **Security** | 6-cell grid (kernel sandbox, encrypted creds, secret redaction, workspace isolation, audit logs, controlled execution) + posture table |
";
        let max_width = 80;
        let lines = MarkdownRenderer::render_with_width(markdown, &theme, max_width);
        assert!(
            lines.len() > 6,
            "Wide cell should wrap into multiple table sub-rows"
        );

        let expected_w: usize = lines[0]
            .spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        assert!(
            expected_w <= max_width,
            "Table width {} exceeds max_width {}",
            expected_w,
            max_width
        );

        for (idx, line) in lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let w = UnicodeWidthStr::width(text.as_str());
            assert_eq!(
                w, expected_w,
                "Table row {} width {} != top border width {} (line: {:?})",
                idx, w, expected_w, text
            );
            assert!(
                text.starts_with('┌')
                    || text.starts_with('├')
                    || text.starts_with('└')
                    || text.starts_with('│'),
                "Table line {} must start with box-drawing border: {:?}",
                idx,
                text
            );
            assert!(
                text.ends_with('┐')
                    || text.ends_with('┤')
                    || text.ends_with('┘')
                    || text.ends_with('│'),
                "Table line {} must end with box-drawing border: {:?}",
                idx,
                text
            );
        }
    }

    #[test]
    fn test_markdown_headings_without_hashes() {
        let theme = Theme::default();
        let input = "# Main Title\n## Landing Page Summary\n### Section Details\n#### Sub-heading";
        let lines = MarkdownRenderer::render(input, &theme);
        assert_eq!(lines.len(), 4);

        let texts: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        assert_eq!(texts[0], "Main Title");
        assert_eq!(texts[1], "Landing Page Summary");
        assert_eq!(texts[2], "Section Details");
        assert_eq!(texts[3], "Sub-heading");

        assert!(lines[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD | Modifier::UNDERLINED));
        assert_eq!(lines[0].spans[0].style.fg, Some(theme.brand_accent));
        assert_eq!(lines[1].spans[0].style.fg, Some(theme.brand_accent));
        assert_eq!(lines[2].spans[0].style.fg, Some(theme.highlight));
        assert_eq!(lines[3].spans[0].style.fg, Some(theme.info));
    }

    #[test]
    fn test_markdown_callouts_checklists_and_all_output_styles() {
        let theme = Theme::default();
        let doc = "\
# Report Title
## Performance Metrics
> [!TIP] Zero regressions — all 231 tests passed.
> Regular quote line.
- [x] Fix Nerd Font fallback
- [ ] Publish release ~~draft~~ *v0.5.75*
```rust
let ok = true;
```
| Metric | Value |
| --- | --- |
| Latency | 142ms |";

        let styles = [
            OutputStyle::Default,
            OutputStyle::Minimal,
            OutputStyle::Editorial,
            OutputStyle::Workspace,
            OutputStyle::Posh,
            OutputStyle::NerdFont,
            OutputStyle::Symbols,
            OutputStyle::Emoji,
            OutputStyle::Kaomoji,
            OutputStyle::Ascii,
        ];

        for st in styles {
            let lines = MarkdownRenderer::render_with_style(doc, &theme, 80, st);
            assert!(!lines.is_empty(), "Empty output for style {st:?}");
            let full: String = lines
                .iter()
                .map(|l| {
                    l.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                full.contains("TIP") && full.contains("Zero regressions"),
                "Expected callout rendering for {st:?}, got:\n{full}"
            );
            assert!(
                full.contains("Fix Nerd Font fallback") && full.contains("Publish release"),
                "Expected checklist rendering for {st:?}, got:\n{full}"
            );
            assert!(
                full.contains("Latency") && full.contains("142ms"),
                "Expected table rendering for {st:?}, got:\n{full}"
            );
            if st == OutputStyle::Ascii {
                assert!(full.contains("[x]") && full.contains("[ ]") && full.contains("+--"));
            }
            if st == OutputStyle::Minimal || st == OutputStyle::Editorial {
                // Booktabs tables omit vertical box-table walls and column junctions
                assert!(
                    !full.contains("─┬─") && full.contains("Latency  142ms"),
                    "Booktabs style {st:?} should omit box table walls and auto-align numeric cells:\n{full}"
                );
            }
            if st == OutputStyle::Workspace {
                assert!(full.contains("╭─") && full.contains("╰─"));
            }
        }
    }
}
