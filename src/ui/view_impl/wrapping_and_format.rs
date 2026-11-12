//! Tool title formatting, command/path cleaning, visual line wrapping, and pill overlay helpers for `TimelineView`.

use super::TimelineView;
use crate::ui::theme::Theme;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl TimelineView {
    /// Formats an internal tool identifier into a human-readable badge title
    pub fn format_tool_title(raw_name: &str) -> String {
        match raw_name {
            "exec_cmd" | "bash" | "run_command" | "shell" | "cmd" => "Bash".to_string(),
            "read_file" | "cat" | "view_file" => "Read File".to_string(),
            "batch_read" => "Batch Read".to_string(),
            "look_at" => "Look At".to_string(),
            "write_file" | "create_file" => "Write File".to_string(),
            "patch_file" | "edit_file" | "replace_file_content" => "Edit File".to_string(),
            "list_directory" | "list_dir" | "ls" => "List Dir".to_string(),
            "grep_search" | "search_code" | "grep" => "Grep Search".to_string(),
            "find_files" | "find_by_name" | "locate_files" => "Find Files".to_string(),
            "file_info" | "stat_file" => "File Info".to_string(),
            "make_directory" | "mkdir" => "Make Dir".to_string(),
            "remove_file" | "rm" => "Remove File".to_string(),
            "webfetch" => "Web Fetch".to_string(),
            "web_search" => "Web Search".to_string(),
            "git_status" => "Git Status".to_string(),
            "git_diff" => "Git Diff".to_string(),
            "git_commit" => "Git Commit".to_string(),
            "git_log" => "Git Log".to_string(),
            "git_branch" => "Git Branch".to_string(),
            "code_graph" | "symbol_graph" => "Code Graph".to_string(),
            "semantic_search" => "Semantic Search".to_string(),
            "explore_symbol" | "symbol_definition" => "Explore Symbol".to_string(),
            "blast_radius" => "Blast Radius".to_string(),
            "kit_doctor" | "onpkg_doctor" => "MiniKit Doctor".to_string(),
            "kit_stack_list" | "onpkg_stack_list" => "MiniKit Stack List".to_string(),
            "kit_stack_add" | "onpkg_stack_add" => "MiniKit Stack Add".to_string(),
            "kit_stack_new" | "onpkg_stack_new" => "MiniKit Stack New".to_string(),
            "kit_stack_remove" | "onpkg_stack_remove" => "MiniKit Stack Remove".to_string(),
            "kit_stack_diff" | "onpkg_stack_diff" => "MiniKit Stack Diff".to_string(),
            "kit_skill_remove" | "onpkg_skill_remove" => "MiniKit Skill Remove".to_string(),
            "kit_info" | "kit_pkg_info" | "onpkg_pkg_info" => "MiniKit Package Info".to_string(),
            "kit_add" | "kit_pkg_add" | "onpkg_pkg_add" => "MiniKit Package Add".to_string(),
            "kit_remove" | "kit_pkg_remove" | "onpkg_pkg_remove" => {
                "MiniKit Package Remove".to_string()
            }
            "spawn_subagent" => "Spawn Subagent".to_string(),
            "send_subagent_message" => "Message Subagent".to_string(),
            "begin_transaction" => "Begin Transaction".to_string(),
            "commit_transaction" => "Commit Transaction".to_string(),
            "rollback_transaction" => "Rollback Transaction".to_string(),
            other => other
                .split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// Cleans and truncates shell commands for minimal single-line badge display.
    /// Strips multiline heredoc payloads (`<< ...`), normalizes whitespace, and truncates to 70 chars.
    #[inline]
    pub(super) fn truncate_header_display(s: &str, max_chars: usize) -> String {
        if s.chars().count() > max_chars {
            let mut res: String = s.chars().take(max_chars.saturating_sub(3)).collect();
            res.push_str("...");
            res
        } else {
            s.to_string()
        }
    }

    pub fn clean_command_display(cmd: &str) -> String {
        let first_line = cmd.lines().next().unwrap_or("").trim();
        // If it's a heredoc like `cat > file << 'EOF'`, cut off at `<<`
        let stripped = if let Some(idx) = first_line.find("<<") {
            first_line[..idx].trim_end()
        } else {
            first_line
        };
        let parts: Vec<&str> = stripped.split_whitespace().collect();
        let compacted = parts.join(" ");
        Self::truncate_header_display(&compacted, 70)
    }

    /// Cleans and truncates file paths for compact header display.
    pub fn clean_path_display(path: &str) -> String {
        Self::truncate_header_display(path.trim(), 70)
    }

    /// Splits a long line of text into sub-chunks of max `max_width` visual cells.
    /// Normalizes emojis/tabs first and splits on whitespace boundaries when possible.
    pub fn split_line_into_chunks(text: &str, max_width: usize) -> Vec<String> {
        let sanitized = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(text);
        let text = sanitized.as_str();
        if max_width == 0 {
            return vec![text.to_string()];
        }
        let total_width = UnicodeWidthStr::width(text);
        if total_width <= max_width {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_width = 0;

        let push_word_fitting_or_char_split =
            |word: &str,
             word_width: usize,
             current: &mut String,
             current_width: &mut usize,
             chunks: &mut Vec<String>| {
                if word_width <= max_width {
                    current.push_str(word);
                    *current_width = word_width;
                } else {
                    for ch in word.chars() {
                        let ch_w = UnicodeWidthChar::width(ch).unwrap_or(1);
                        if *current_width + ch_w > max_width && !current.is_empty() {
                            chunks.push(std::mem::take(current));
                            *current_width = 0;
                        }
                        current.push(ch);
                        *current_width += ch_w;
                    }
                }
            };

        for word in text.split(' ') {
            let word_width = UnicodeWidthStr::width(word);
            if current.is_empty() {
                push_word_fitting_or_char_split(
                    word,
                    word_width,
                    &mut current,
                    &mut current_width,
                    &mut chunks,
                );
            } else if current_width + 1 + word_width <= max_width {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_width;
            } else {
                chunks.push(std::mem::take(&mut current));
                current_width = 0;
                push_word_fitting_or_char_split(
                    word,
                    word_width,
                    &mut current,
                    &mut current_width,
                    &mut chunks,
                );
            }
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        if chunks.is_empty() {
            vec![text.to_string()]
        } else {
            chunks
        }
    }

    pub fn extract_cmd_display(name: &str, args_json: &str) -> String {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(args_json) {
            if name == "update_progress" {
                let status = val
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("completed");
                if let Some(step) = val.get("step").and_then(|s| s.as_str()) {
                    return Self::clean_command_display(&format!("[{}] {}", status, step));
                }
                if let Some(steps) = val.get("steps").and_then(|s| s.as_array()) {
                    return format!("[{}] {} steps", status, steps.len());
                }
            }
            if matches!(name, "create_plan" | "power_plan") {
                let step_count = ["steps", "tasks", "plan"]
                    .iter()
                    .find_map(|k| val.get(*k).and_then(|a| a.as_array()))
                    .map(|a| a.len());
                let title = val
                    .get("title")
                    .or_else(|| val.get("goal"))
                    .and_then(|t| t.as_str());
                return match (title, step_count) {
                    (Some(t), Some(n)) => {
                        Self::clean_command_display(&format!("\"{}\" ({} steps)", t, n))
                    }
                    (Some(t), None) => Self::clean_command_display(&format!("\"{}\"", t)),
                    (None, Some(n)) => format!("{} steps", n),
                    (None, None) => String::new(),
                };
            }
            if let Some(cmd) = val
                .get("command")
                .or_else(|| val.get("cmd"))
                .and_then(|c| c.as_str())
            {
                return Self::clean_command_display(cmd);
            }
            if let Some(path) = val
                .get("path")
                .or_else(|| val.get("file_path"))
                .or_else(|| val.get("target_file"))
                .and_then(|p| p.as_str())
            {
                return Self::clean_path_display(path);
            }
            if let Some(paths) = val.get("paths").and_then(|p| p.as_array()) {
                let items: Vec<&str> = paths.iter().filter_map(|v| v.as_str()).collect();
                if !items.is_empty() {
                    let preview = items.iter().take(2).copied().collect::<Vec<_>>().join(", ");
                    let summary = if items.len() > 2 {
                        format!("{} (+{} files)", preview, items.len() - 2)
                    } else {
                        preview
                    };
                    return Self::clean_path_display(&summary);
                }
            }
            if let Some(query) = val
                .get("query")
                .or_else(|| val.get("pattern"))
                .and_then(|q| q.as_str())
            {
                return format!("\"{}\"", query);
            }
            if let Some(url) = val.get("url").and_then(|u| u.as_str()) {
                return url.to_string();
            }
            if let Some(symbol) = val
                .get("symbol")
                .or_else(|| val.get("symbol_name"))
                .and_then(|s| s.as_str())
            {
                return symbol.to_string();
            }
            if let Some(message) = val.get("message").and_then(|m| m.as_str()) {
                let first_line = message.lines().next().unwrap_or("").trim();
                return format!("\"{}\"", first_line);
            }
            if let Some(vp) = val.get("viewport").and_then(|v| v.as_str()) {
                return vp.to_string();
            }
            if let (Some(w), Some(h)) = (
                val.get("width").and_then(|v| v.as_u64()),
                val.get("height").and_then(|v| v.as_u64()),
            ) {
                return format!("{}x{}", w, h);
            }
            if let Some(stack) = val
                .get("stack_name")
                .or_else(|| val.get("stack"))
                .and_then(|s| s.as_str())
            {
                return stack.to_string();
            }
            if let Some(cat) = val.get("category").and_then(|c| c.as_str()) {
                return cat.to_string();
            }
            if let Some(cats) = val.get("categories").and_then(|c| c.as_array()) {
                let names: Vec<&str> = cats.iter().filter_map(|v| v.as_str()).collect();
                if !names.is_empty() {
                    return names.join(", ");
                }
            }
            if let Some(action) = val.get("action").and_then(|a| a.as_str()) {
                if let Some(target) = val
                    .get("id")
                    .or_else(|| val.get("name"))
                    .or_else(|| val.get("profile"))
                    .and_then(|t| t.as_str())
                {
                    return Self::clean_command_display(&format!("{} {}", action, target));
                }
                return action.to_string();
            }
            if name == "browser_batch" {
                if let Some(steps) = val
                    .get("steps")
                    .or_else(|| val.get("actions"))
                    .and_then(|s| s.as_array())
                {
                    return format!("{} actions", steps.len());
                }
            }
        }
        let trimmed = args_json.trim();
        if trimmed == "{}" || trimmed.is_empty() {
            String::new()
        } else {
            Self::clean_command_display(&format!("{}({})", name, trimmed))
        }
    }

    /// Overlays a centered pill onto an existing rendered line without erasing text on the left or right of the pill.
    pub(super) fn overlay_centered_pill(
        line: &Line<'_>,
        pill_text: &'static str,
        pill_style: Style,
        clear_style: Style,
        full_w: usize,
    ) -> Line<'static> {
        let pill_w = UnicodeWidthStr::width(pill_text);
        let pill_start = full_w.saturating_sub(pill_w) / 2;
        let pill_end = pill_start + pill_w;

        let mut left_spans: Vec<Span<'static>> = Vec::new();
        let mut right_spans: Vec<Span<'static>> = Vec::new();
        let mut cur_col = 0usize;
        let mut left_col_end = 0usize;
        let mut right_col_start: Option<usize> = None;

        for span in &line.spans {
            let mut left_buf = String::new();
            let mut right_buf = String::new();

            for ch in span.content.chars() {
                let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
                let ch_start = cur_col;
                let ch_end = cur_col + cw;
                cur_col = ch_end;

                if ch_end <= pill_start {
                    left_buf.push(ch);
                    left_col_end = ch_end;
                } else if ch_start >= pill_end {
                    if right_col_start.is_none() {
                        right_col_start = Some(ch_start);
                    }
                    right_buf.push(ch);
                }
            }

            if !left_buf.is_empty() {
                left_spans.push(Span::styled(left_buf, span.style));
            }
            if !right_buf.is_empty() {
                right_spans.push(Span::styled(right_buf, span.style));
            }
        }

        if left_col_end < pill_start {
            left_spans.push(Span::styled(
                " ".repeat(pill_start - left_col_end),
                clear_style,
            ));
        }

        let mut out_spans = left_spans;
        out_spans.push(Span::styled(pill_text, pill_style));

        if let Some(r_start) = right_col_start {
            if r_start > pill_end {
                out_spans.push(Span::styled(" ".repeat(r_start - pill_end), clear_style));
            }
            out_spans.extend(right_spans);
        } else if full_w > pill_end {
            out_spans.push(Span::styled(" ".repeat(full_w - pill_end), clear_style));
        }

        Line::from(out_spans)
    }

    /// Computes the hanging-indentation continuation prefix `(prefix_str, style, width)`
    /// for wrapped continuation lines so bullet lists, numbered lists, code blocks,
    /// and indented paragraphs maintain clean column alignment when wrapped.
    pub(super) fn continuation_prefix_for_line(line: &Line<'_>) -> (String, Style, usize) {
        let first_span = match line.spans.first() {
            Some(s) => s,
            None => return (String::new(), Style::default(), 0),
        };
        let first_text = first_span.content.as_ref();

        if first_text == " › " {
            return ("   ".to_string(), first_span.style, 3);
        }
        if first_text == "  │ " || first_text == "  │   " {
            let w = UnicodeWidthStr::width(first_text);
            return (first_text.to_string(), first_span.style, w);
        }
        if first_text == "│ " {
            if let Some(second) = line.spans.get(1) {
                let s2 = second.content.as_ref();
                if s2 == " ✓ " || s2 == " ▸ " || s2 == " ○ " || s2 == " ✔ " || s2 == " ▶ "
                {
                    return ("│    ".to_string(), first_span.style, 5);
                }
            }
            return ("│ ".to_string(), first_span.style, 2);
        }
        if first_text == "• "
            || first_text == "› "
            || first_text == "✓ "
            || first_text == "✗ "
            || first_text == "! "
            || first_text == "→ "
            || first_text == "▸ "
        {
            return ("  ".to_string(), Style::default(), 2);
        }
        if first_text == "  - " || first_text == "  • " || first_text == "    - " {
            let w = UnicodeWidthStr::width(first_text);
            return (" ".repeat(w), Style::default(), w);
        }
        if first_text.ends_with(". ")
            && first_text.len() <= 6
            && first_text[..first_text.len() - 2]
                .chars()
                .all(|c| c.is_ascii_digit())
        {
            let w = first_text.len();
            return (" ".repeat(w), Style::default(), w);
        }

        // Preserve leading space indentation (up to 8 spaces) without allocating a String
        let leading_spaces = line
            .spans
            .iter()
            .flat_map(|s| s.content.chars())
            .take_while(|&c| c == ' ')
            .take(9)
            .count();
        if leading_spaces > 0 && leading_spaces <= 8 {
            return (" ".repeat(leading_spaces), Style::default(), leading_spaces);
        }

        (String::new(), Style::default(), 0)
    }

    /// Pre-wraps lines to the given visual column width so that each `Line`
    /// maps 1:1 to a rendered terminal row. Groups contiguous non-whitespace characters
    /// across adjacent `Span`s into atomic words and preserves hanging indentation.
    pub fn wrap_lines_to_width<'a>(lines: Vec<Line<'a>>, max_width: usize) -> Vec<Line<'a>> {
        let max_width = max_width.max(10);
        let mut wrapped: Vec<Line<'a>> = Vec::with_capacity(lines.len());

        struct StyledRun {
            chars: Vec<(char, Style)>,
            is_ws: bool,
            width: usize,
        }

        for line in lines {
            let total_width: usize = line
                .spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            let has_newline = line.spans.iter().any(|s| s.content.contains('\n'));

            if total_width <= max_width && !has_newline {
                wrapped.push(line);
                continue;
            }

            let (cont_pfx, cont_style, cont_w) = if max_width >= 20 {
                Self::continuation_prefix_for_line(&line)
            } else {
                (String::new(), Style::default(), 0)
            };
            let (cont_pfx, cont_w) = if cont_w * 2 >= max_width {
                (String::new(), 0)
            } else {
                (cont_pfx, cont_w)
            };

            // Split logical sub-lines on '\n' while preserving per-character styles
            let mut logical_sublines: Vec<Vec<(char, Style)>> = vec![Vec::new()];
            for span in line.spans {
                let style = span.style;
                for ch in span.content.chars() {
                    if ch == '\n' {
                        logical_sublines.push(Vec::new());
                    } else if ch == '\t' {
                        if let Some(last_sub) = logical_sublines.last_mut() {
                            for _ in 0..4 {
                                last_sub.push((' ', style));
                            }
                        }
                    } else if !ch.is_control() {
                        if let Some(last_sub) = logical_sublines.last_mut() {
                            last_sub.push((ch, style));
                        }
                    }
                }
            }

            for sub_chars in logical_sublines {
                if sub_chars.is_empty() {
                    wrapped.push(Line::from(String::new()));
                    continue;
                }

                // Group contiguous whitespace vs non-whitespace characters across spans
                let mut runs: Vec<StyledRun> = Vec::new();
                for (ch, style) in sub_chars {
                    let is_ws = ch.is_whitespace();
                    let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if ch_w == 0 {
                        continue;
                    }
                    if let Some(last) = runs.last_mut() {
                        if last.is_ws == is_ws {
                            last.chars.push((ch, style));
                            last.width += ch_w;
                            continue;
                        }
                    }
                    runs.push(StyledRun {
                        chars: vec![(ch, style)],
                        is_ws,
                        width: ch_w,
                    });
                }

                let mut cur_line_spans: Vec<Span<'a>> = Vec::new();
                let mut cur_line_width: usize = 0;
                let mut is_first_visual_line = true;

                let start_continuation_line = |spans: &mut Vec<Span<'a>>, width: &mut usize| {
                    spans.clear();
                    if cont_w > 0 {
                        spans.push(Span::styled(cont_pfx.clone(), cont_style));
                        *width = cont_w;
                    } else {
                        *width = 0;
                    }
                };

                for run in runs {
                    if run.is_ws {
                        let min_prefix_w = if is_first_visual_line { 0 } else { cont_w };
                        if !is_first_visual_line && cur_line_width <= min_prefix_w {
                            // Ignore leading whitespace right after a wrap on continuation lines
                            continue;
                        } else if cur_line_width + run.width <= max_width {
                            for (ch, st) in run.chars {
                                let mut buf = [0u8; 4];
                                Self::push_span_token(
                                    &mut cur_line_spans,
                                    ch.encode_utf8(&mut buf),
                                    st,
                                );
                            }
                            cur_line_width += run.width;
                        } else {
                            wrapped.push(Line::from(std::mem::take(&mut cur_line_spans)));
                            is_first_visual_line = false;
                            start_continuation_line(&mut cur_line_spans, &mut cur_line_width);
                        }
                    } else {
                        // Atomic multi-span word unit
                        let avail_on_fresh_line = max_width.saturating_sub(cont_w);
                        if cur_line_width + run.width <= max_width {
                            for (ch, st) in run.chars {
                                let mut buf = [0u8; 4];
                                Self::push_span_token(
                                    &mut cur_line_spans,
                                    ch.encode_utf8(&mut buf),
                                    st,
                                );
                            }
                            cur_line_width += run.width;
                        } else if run.width > avail_on_fresh_line {
                            // Word itself exceeds available line width: break by character
                            for (ch, st) in run.chars {
                                let ch_w = UnicodeWidthChar::width(ch).unwrap_or(1);
                                if cur_line_width + ch_w > max_width && cur_line_width > 0 {
                                    wrapped.push(Line::from(std::mem::take(&mut cur_line_spans)));
                                    is_first_visual_line = false;
                                    start_continuation_line(
                                        &mut cur_line_spans,
                                        &mut cur_line_width,
                                    );
                                }
                                let mut buf = [0u8; 4];
                                Self::push_span_token(
                                    &mut cur_line_spans,
                                    ch.encode_utf8(&mut buf),
                                    st,
                                );
                                cur_line_width += ch_w;
                            }
                        } else {
                            // Wrap entire word unit onto next continuation line
                            if !cur_line_spans.is_empty() {
                                wrapped.push(Line::from(std::mem::take(&mut cur_line_spans)));
                            }
                            is_first_visual_line = false;
                            start_continuation_line(&mut cur_line_spans, &mut cur_line_width);
                            for (ch, st) in run.chars {
                                let mut buf = [0u8; 4];
                                Self::push_span_token(
                                    &mut cur_line_spans,
                                    ch.encode_utf8(&mut buf),
                                    st,
                                );
                            }
                            cur_line_width += run.width;
                        }
                    }
                }

                if !cur_line_spans.is_empty() {
                    wrapped.push(Line::from(cur_line_spans));
                }
            }
        }

        wrapped
    }

    pub(super) fn push_span_token<'a>(spans: &mut Vec<Span<'a>>, text: &str, style: Style) {
        if let Some(last) = spans.last_mut() {
            if last.style == style {
                last.content.to_mut().push_str(text);
                return;
            }
        }
        spans.push(Span::styled(text.to_string(), style));
    }

    /// Renders assistant Markdown text into highlighted Ratatui lines constrained to `max_width`
    pub(super) fn render_markdown_with_width<'a>(
        text: &'a str,
        theme: &'a Theme,
        max_width: usize,
    ) -> Vec<Line<'a>> {
        crate::ui::markdown::MarkdownRenderer::render_with_width(text, theme, max_width)
    }

    /// Renders assistant Markdown text into highlighted Ratatui lines
    #[allow(dead_code)]
    pub(super) fn render_markdown<'a>(text: &'a str, theme: &'a Theme) -> Vec<Line<'a>> {
        crate::ui::markdown::MarkdownRenderer::render(text, theme)
    }
}
