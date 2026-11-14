//! Unit tests for `TimelineView` scrolling, mouse selection, line wrapping, and entry rendering.

use super::*;

#[test]
fn test_timeline_scrolling_and_auto_scroll_resumption() {
    let view = TimelineView::new();
    view.max_scroll.set(50);
    view.auto_scroll.set(true);

    // 1. Scrolling up disables auto_scroll and steps backward from max_scroll
    view.scroll_up(10);
    assert!(!view.auto_scroll.get());
    assert_eq!(view.scroll_offset.get(), 40);

    // 2. Further scroll up
    view.scroll_up(20);
    assert_eq!(view.scroll_offset.get(), 20);

    // 3. Scroll to top
    view.scroll_to_top();
    assert_eq!(view.scroll_offset.get(), 0);
    assert!(!view.auto_scroll.get());

    // 4. Scroll down
    view.scroll_down(30);
    assert_eq!(view.scroll_offset.get(), 30);
    assert!(!view.auto_scroll.get());

    // 5. Scroll down to or beyond max_scroll re-enables auto_scroll
    view.scroll_down(30);
    assert_eq!(view.scroll_offset.get(), 50);
    assert!(view.auto_scroll.get());

    // 6. Scroll to bottom
    view.scroll_up(15);
    assert!(!view.auto_scroll.get());
    view.scroll_to_bottom();
    assert_eq!(view.scroll_offset.get(), 50);
    assert!(view.auto_scroll.get());
}

#[test]
fn test_timeline_mouse_selection_and_copy() {
    let view = TimelineView::new();
    view.selection.timeline_area.set(Rect::new(0, 0, 80, 24));
    *view.selection.cached_plain_lines.borrow_mut() = vec![
        "Line zero hello world".to_string(),
        "Line one minicode assistant".to_string(),
        "Line two testing auto copy".to_string(),
    ];

    // 1. Single line mouse drag selection
    view.handle_mouse_down(5, 0); // "zero" starts around index 5
    view.handle_mouse_drag(9, 0);
    let extracted = view.handle_mouse_up(9, 0);
    assert_eq!(extracted, Some("zero".to_string()));

    // 2. Multi-line mouse drag selection
    view.handle_mouse_down(5, 0);
    view.handle_mouse_drag(8, 1);
    let multi_extracted = view.handle_mouse_up(8, 1);
    assert!(multi_extracted.is_some());
    let text = multi_extracted.unwrap();
    assert!(text.contains("zero hello world\nLine one"));
}

#[test]
fn test_apply_selection_to_line() {
    let theme = Theme::aura_dark();
    let original_line = Line::from(vec![
        Span::raw("Hello "),
        Span::raw("minicode "),
        Span::raw("world"),
    ]);

    // Select "minicode" (columns 6..14, excluding the space at col 14)
    let highlighted = crate::ui::selection::TimelineSelection::apply_selection_to_line(
        original_line,
        6,
        14,
        &theme,
    );
    assert_eq!(highlighted.spans.len(), 4);
    assert_eq!(highlighted.spans[0].content, "Hello ");
    assert_eq!(highlighted.spans[1].content, "minicode");
    assert_eq!(highlighted.spans[2].content, " ");
    assert_eq!(highlighted.spans[3].content, "world");
    assert!(highlighted.spans[1]
        .style
        .add_modifier
        .contains(Modifier::REVERSED));
}

#[test]
fn test_wrap_lines_to_width() {
    // 1. Short line remains unchanged
    let lines = vec![Line::from("Hello world")];
    let wrapped = TimelineView::wrap_lines_to_width(lines, 40);
    assert_eq!(wrapped.len(), 1);
    assert_eq!(wrapped[0].spans[0].content, "Hello world");

    // 2. Long line wraps at word boundary
    let long_line = vec![Line::from("The quick brown fox jumps over the lazy dog")];
    let wrapped = TimelineView::wrap_lines_to_width(long_line, 15);
    assert!(wrapped.len() >= 3);
    for l in &wrapped {
        let line_text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(UnicodeWidthStr::width(line_text.as_str()) <= 15);
    }

    // 3. Very long continuous token wraps by character
    let unbroken = vec![Line::from("abcdefghijklmnopqrstuvwxyz0123456789")];
    let wrapped = TimelineView::wrap_lines_to_width(unbroken, 10);
    assert_eq!(wrapped.len(), 4);
    for l in &wrapped {
        let line_text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(UnicodeWidthStr::width(line_text.as_str()) <= 10);
    }
}

#[test]
fn test_paragraph_rendering_alignment() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut view = TimelineView::new();
    view.entries
        .push(TimelineEntry::UserPrompt("read src/main.rs".to_string()));
    view.entries.push(TimelineEntry::ToolFinished {
        name: "read_file".to_string(),
        command_or_path: "src/main.rs".to_string(),
        output: "line 1\nline 2\nline 3\nline 4\nline 5".to_string(),
        success: true,
        duration_ms: Some(15),
    });
    view.entries.push(TimelineEntry::AssistantMarkdown(
        "This is a longer assistant message that explains what the file contains and will definitely wrap across several lines on a standard width terminal window so we can see how wrapping affects selection.".to_string(),
    ));

    for i in 0..25 {
        view.entries.push(TimelineEntry::SystemStatus(format!(
            "Status entry number {}",
            i
        )));
    }

    let theme = Theme::aura_dark();
    let ctx = TimelineContext {
        theme: &theme,
        is_working: false,
        working_millis: 0,
        current_activity: None,
        spinner_style: crate::ui::animation::SpinnerStyle::default(),
        workspace: std::path::Path::new("."),
        provider: "test",
        model: "test",
    };

    // Render with auto_scroll=true (which will scroll down)
    terminal
        .draw(|f| {
            view.render(f, Rect::new(0, 0, 80, 20), &ctx);
        })
        .unwrap();

    let initial_scroll = view.scroll_offset.get();
    assert!(initial_scroll > 0);

    // Get the plain text displayed at screen row 10
    let buf = terminal.backend().buffer();
    let mut row_10_text = String::new();
    for x in 0..20 {
        row_10_text.push(buf[(x, 10)].symbol().chars().next().unwrap_or(' '));
    }

    // Simulate clicking and dragging on screen row 10
    view.handle_mouse_down(0, 10);
    view.handle_mouse_drag(20, 10);
    let extracted = view.handle_mouse_up(20, 10);
    assert_eq!(extracted, Some(row_10_text.clone()));

    // Re-render to verify that row 10 has the selection style applied
    terminal
        .draw(|f| {
            view.render(f, Rect::new(0, 0, 80, 20), &ctx);
        })
        .unwrap();

    let buf2 = terminal.backend().buffer();
    // Row 10 must be highlighted with REVERSED modifier
    assert!(buf2[(0, 10)].modifier.contains(Modifier::REVERSED));
    // Adjacent rows must NOT be highlighted
    assert!(!buf2[(0, 9)].modifier.contains(Modifier::REVERSED));
    assert!(!buf2[(0, 11)].modifier.contains(Modifier::REVERSED));

    // Test manual scroll alignment: scroll up 5 lines
    view.scroll_up(5);
    terminal
        .draw(|f| {
            view.render(f, Rect::new(0, 0, 80, 20), &ctx);
        })
        .unwrap();

    let buf3 = terminal.backend().buffer();
    let mut scrolled_row_5_text = String::new();
    for x in 0..15 {
        scrolled_row_5_text.push(buf3[(x, 5)].symbol().chars().next().unwrap_or(' '));
    }

    view.handle_mouse_down(0, 5);
    view.handle_mouse_drag(15, 5);
    let scrolled_extracted = view.handle_mouse_up(15, 5);
    assert_eq!(scrolled_extracted, Some(scrolled_row_5_text));
}

#[test]
fn test_minimax_think_tag_parsing_single_chunk() {
    let mut view = TimelineView::new();
    view.add_user_message("hii".to_string());
    let raw = "<think>\nThinking about greeting.\n</think>\n\nHey! How can I help you?";
    view.append_assistant_delta(raw);
    view.finalize_pending_thoughts(Some(1.5));

    let mut has_thought = false;
    let mut has_assistant = false;
    for entry in &view.entries {
        match entry {
            TimelineEntry::ThoughtBlock { text, .. } => {
                assert!(text.contains("Thinking about greeting."));
                assert!(!text.contains("<think>"));
                assert!(!text.contains("</think>"));
                has_thought = true;
            }
            TimelineEntry::AssistantMarkdown(text) => {
                assert!(text.contains("Hey! How can I help you?"));
                assert!(!text.contains("<think>"));
                assert!(!text.contains("</think>"));
                has_assistant = true;
            }
            _ => {}
        }
    }
    assert!(has_thought, "Expected a ThoughtBlock for <think>");
    assert!(has_assistant, "Expected AssistantMarkdown without <think>");
}

#[test]
fn test_minimax_think_tag_parsing_split_chunks() {
    let mut view = TimelineView::new();
    view.add_user_message("hii".to_string());
    view.append_assistant_delta("<th");
    view.append_assistant_delta("ink>\nStep 1 reasoning.\n</th");
    view.append_assistant_delta("ink>\nHello world!");
    view.finalize_pending_thoughts(Some(2.0));

    let mut has_thought = false;
    let mut has_assistant = false;
    for entry in &view.entries {
        match entry {
            TimelineEntry::ThoughtBlock { text, .. } => {
                assert!(text.contains("Step 1 reasoning."));
                assert!(!text.contains("<think>"));
                assert!(!text.contains("</think>"));
                has_thought = true;
            }
            TimelineEntry::AssistantMarkdown(text) => {
                assert!(text.contains("Hello world!"));
                assert!(!text.contains("<think>"));
                assert!(!text.contains("</think>"));
                has_assistant = true;
            }
            _ => {}
        }
    }
    assert!(has_thought, "Expected ThoughtBlock from split chunks");
    assert!(
        has_assistant,
        "Expected clean AssistantMarkdown from split chunks"
    );
}

#[test]
fn test_format_tool_title() {
    assert_eq!(TimelineView::format_tool_title("exec_cmd"), "Bash");
    assert_eq!(TimelineView::format_tool_title("read_file"), "Read File");
    assert_eq!(TimelineView::format_tool_title("patch_file"), "Edit File");
    assert_eq!(
        TimelineView::format_tool_title("list_directory"),
        "List Dir"
    );
    assert_eq!(
        TimelineView::format_tool_title("grep_search"),
        "Grep Search"
    );
    assert_eq!(
        TimelineView::format_tool_title("custom_inspect_node"),
        "Custom Inspect Node"
    );
}

#[test]
fn test_extract_cmd_display() {
    assert_eq!(
        TimelineView::extract_cmd_display("exec_cmd", r#"{"command": "cargo test"}"#),
        "cargo test"
    );
    assert_eq!(
        TimelineView::extract_cmd_display("read_file", r#"{"path": "src/main.rs"}"#),
        "src/main.rs"
    );
    assert_eq!(
        TimelineView::extract_cmd_display("grep_search", r#"{"query": "required_height"}"#),
        "\"required_height\""
    );
    assert_eq!(TimelineView::extract_cmd_display("git_status", "{}"), "");
    assert_eq!(
        TimelineView::extract_cmd_display(
            "update_progress",
            r#"{"status": "completed", "step": "active"}"#
        ),
        "[completed] active"
    );
    assert_eq!(
        TimelineView::extract_cmd_display(
            "create_plan",
            r#"{"title": "Landing Page", "steps": ["s1", "s2", "s3"], "architecture": "long..."}"#
        ),
        "\"Landing Page\" (3 steps)"
    );
    assert_eq!(
        TimelineView::extract_cmd_display("kit_stack_add", r#"{"stack_name": "react-vite-ts"}"#),
        "react-vite-ts"
    );
    assert_eq!(
        TimelineView::extract_cmd_display("activate_tools", r#"{"category": "web"}"#),
        "web"
    );
    assert_eq!(
        TimelineView::extract_cmd_display("minitask", r#"{"action": "stop", "id": "dev-5180"}"#),
        "stop dev-5180"
    );
    assert_eq!(
        TimelineView::extract_cmd_display("browser_emulate", r#"{"viewport": "tablet"}"#),
        "tablet"
    );
    assert_eq!(
        TimelineView::extract_cmd_display(
            "browser_batch",
            r#"{"steps": [{"action": "emulate", "viewport": "desktop"}, {"action": "screenshot"}]}"#
        ),
        "2 actions"
    );
}

#[test]
fn test_clean_command_display_heredoc() {
    let cmd = "cat > /path/to/Login.tsx << 'ENDOFFILE'\n'use client';\nexport default function() {}\nENDOFFILE";
    assert_eq!(
        TimelineView::clean_command_display(cmd),
        "cat > /path/to/Login.tsx"
    );
}

#[test]
fn test_clean_command_display_truncation() {
    let cmd = "npm run very-long-command-name-that-keeps-going-and-going-and-exceeds-seventy-characters-easily";
    let cleaned = TimelineView::clean_command_display(cmd);
    assert!(cleaned.len() <= 70);
    assert!(cleaned.ends_with("..."));
}

#[test]
fn test_split_line_into_chunks() {
    let line = "export interface Customer { id: string; name: string; email: string; }";
    let chunks = TimelineView::split_line_into_chunks(line, 30);
    assert!(chunks.len() > 1);
    for c in chunks {
        assert!(unicode_width::UnicodeWidthStr::width(c.as_str()) <= 30);
    }
}

#[test]
fn test_finish_tool_call_updates_in_place() {
    let mut view = TimelineView::new();
    view.add_tool_call(
        "exec_cmd".to_string(),
        r#"{"command": "cargo check"}"#.to_string(),
    );

    assert_eq!(view.entries.len(), 1);
    match &view.entries[0] {
        TimelineEntry::ToolStart {
            name,
            command_or_path,
        } => {
            assert_eq!(name, "exec_cmd");
            assert_eq!(command_or_path, "cargo check");
        }
        _ => panic!("Expected ToolStart"),
    }

    view.finish_tool_call("exec_cmd", true, "Finished dev profile".to_string(), 120);

    // Crucial test: entries.len() MUST remain 1 (no duplicate Running + Ran entries!)
    assert_eq!(view.entries.len(), 1);
    match &view.entries[0] {
        TimelineEntry::ToolFinished {
            name,
            command_or_path,
            success,
            output,
            duration_ms,
        } => {
            assert_eq!(name, "exec_cmd");
            assert_eq!(command_or_path, "cargo check");
            assert!(*success);
            assert!(output.contains("Finished dev profile"));
            assert_eq!(*duration_ms, Some(120));
        }
        _ => panic!("Expected ToolFinished"),
    }
}

#[test]
fn test_live_plan_update_and_transcript() {
    let mut view = TimelineView::new();
    let plan1 = LivePlanBlock {
        title: "Plan Progress".to_string(),
        total_tasks: 3,
        completed_tasks: 1,
        active_task: Some("T2: Implementation".to_string()),
        tasks: vec![
            LivePlanTaskItem {
                title: "T1: Design".to_string(),
                status: LivePlanTaskStatus::Completed,
                sub_tasks: Vec::new(),
            },
            LivePlanTaskItem {
                title: "T2: Implementation".to_string(),
                status: LivePlanTaskStatus::InProgress,
                sub_tasks: Vec::new(),
            },
            LivePlanTaskItem {
                title: "T3: Verification".to_string(),
                status: LivePlanTaskStatus::Pending,
                sub_tasks: Vec::new(),
            },
        ],
    };

    view.update_live_plan(plan1);
    assert_eq!(view.entries.len(), 1);

    // Update in-place
    let plan2 = LivePlanBlock {
        title: "Plan Progress".to_string(),
        total_tasks: 3,
        completed_tasks: 2,
        active_task: Some("T3: Verification".to_string()),
        tasks: vec![
            LivePlanTaskItem {
                title: "T1: Design".to_string(),
                status: LivePlanTaskStatus::Completed,
                sub_tasks: Vec::new(),
            },
            LivePlanTaskItem {
                title: "T2: Implementation".to_string(),
                status: LivePlanTaskStatus::Completed,
                sub_tasks: Vec::new(),
            },
            LivePlanTaskItem {
                title: "T3: Verification".to_string(),
                status: LivePlanTaskStatus::InProgress,
                sub_tasks: Vec::new(),
            },
        ],
    };

    view.update_live_plan(plan2);
    // Crucial test: in-place update must not duplicate entries
    assert_eq!(view.entries.len(), 1);

    let transcript = view.get_all_transcript_text();
    assert!(transcript.contains("### Plan Progress: 2/3 Tasks Completed"));
    assert!(transcript.contains("- [x] T1: Design"));
    assert!(transcript.contains("- [x] T2: Implementation"));
    assert!(transcript.contains("- [/] T3: Verification"));
}

#[test]
fn test_wrap_lines_preserves_multi_span_atomic_words_and_hanging_indent() {
    let theme = crate::ui::theme::Theme::tokyo_night();
    let md =
        "1. Configured Obsidian-Slate (`#090B12`) with `badge`, `button`, and `table` components.";
    let rendered = crate::ui::markdown::MarkdownRenderer::render_with_width(md, &theme, 44);
    let wrapped = TimelineView::wrap_lines_to_width(rendered, 44);

    let plain_lines: Vec<String> = wrapped
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .filter(|s| !s.trim().is_empty())
        .collect();

    assert!(plain_lines.len() >= 2);
    // Continuation lines of a numbered list ("1. ") must have 3-space hanging indent
    for cont in &plain_lines[1..] {
        assert!(
            cont.starts_with("   "),
            "Expected 3-space hanging indent on continuation line, got: {:?}",
            cont
        );
        // Punctuation attached to inline code must never wrap onto a line by itself
        let trimmed = cont.trim_start();
        assert!(
            !trimmed.starts_with(")") && !trimmed.starts_with(","),
            "Punctuation should stay attached to inline code word, got: {:?}",
            cont
        );
    }
    // Check that `(#090B12)` stayed together on a single line
    assert!(
        plain_lines.iter().any(|l| l.contains("(#090B12)")),
        "Expected (#090B12) to stay intact as a single word unit across spans: {:?}",
        plain_lines
    );
}

#[test]
fn test_wide_table_and_scroll_alignment_in_timeline_render() {
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    let theme = crate::ui::theme::Theme::tokyo_night();
    let mut view = TimelineView::new();
    let md = "| Area | Implementation |\n|---|---|\n| **Design System** | Configured Obsidian-Slate (`#090B12`) + Indigo (`#6366F1`) + Emerald (`#10B981`) |\n| **Verification** | `npm run build` succeeded with 0 TypeScript or Vite bundle errors |\n";
    view.append_assistant_text(md);

    let backend = TestBackend::new(72, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 72, 12);

    for scroll_pos in [0u16, 1, 2, 3] {
        view.scroll_offset.set(scroll_pos);
        view.auto_scroll.set(false);
        terminal
            .draw(|frame| {
                let ctx = TimelineContext {
                    theme: &theme,
                    is_working: false,
                    working_millis: 0,
                    current_activity: None,
                    spinner_style: crate::ui::animation::SpinnerStyle::from_id("braille"),
                    workspace: std::path::Path::new("/tmp"),
                    provider: "openrouter",
                    model: "qwen",
                };
                view.render(frame, area, &ctx);
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        for y in 0..area.height {
            let mut row_str = String::new();
            for x in 0..area.width {
                row_str.push_str(buf[(x, y)].symbol());
            }
            if row_str.contains('│') {
                let trimmed = row_str.trim();
                assert!(
                    trimmed.starts_with('│') && trimmed.ends_with('│'),
                    "Table row at scroll={} y={} has misaligned borders: {:?}",
                    scroll_pos,
                    y,
                    row_str
                );
            }
        }
    }
}

#[test]
fn test_clicking_bottom_line_does_not_show_back_to_bottom_pill() {
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    let theme = crate::ui::theme::Theme::tokyo_night();
    let mut view = TimelineView::new();
    for i in 0..30 {
        view.append_assistant_text(&format!("Line {}\n", i));
    }
    view.append_assistant_text("Dev server still running at http://127.0.0.1:4177.\n");

    let backend = TestBackend::new(80, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 80, 16);

    let draw_once = |v: &TimelineView, term: &mut Terminal<TestBackend>| {
        term.draw(|frame| {
            let ctx = TimelineContext {
                theme: &theme,
                is_working: false,
                working_millis: 0,
                current_activity: None,
                spinner_style: crate::ui::animation::SpinnerStyle::from_id("braille"),
                workspace: std::path::Path::new("/tmp"),
                provider: "minimax",
                model: "MiniMax-M3.1-Flash",
            };
            v.render(frame, area, &ctx);
        })
        .unwrap();
    };

    // 1. Initial render at bottom
    draw_once(&view, &mut terminal);
    assert!(view.auto_scroll.get());

    // 2. Click on the last line (where the URL is) while at the bottom
    view.handle_mouse_down(32, 15);
    let _ = view.handle_mouse_up(32, 15);
    draw_once(&view, &mut terminal);

    let buf = terminal.backend().buffer();
    let mut all_text = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            all_text.push_str(buf[(x, y)].symbol());
        }
        all_text.push('\n');
    }
    assert!(
        !all_text.contains("Back to bottom"),
        "Clicking at the bottom of the timeline must not trigger 'Back to bottom' pill"
    );
    assert!(
        all_text.contains("http://127.0.0.1:4177"),
        "The URL on the last line must remain visible when clicked"
    );
}

#[test]
fn test_long_pasted_user_prompt_shows_full_lines_and_sticky_header_wraps_full_words() {
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    let theme = crate::ui::theme::Theme::tokyo_night();
    let mut view = TimelineView::new();
    let long_prompt = "Create a premium, modern landing page for **minicode**, an autonomous AI coding agent built for developers who want fast, reliable, secure, and highly autonomous software development.\n\nLine 3 content\nLine 4 content\nLine 5 content\nLine 6 content\nLine 7 content final-marker-line";
    view.add_user_message(long_prompt.to_string());
    for i in 0..30 {
        view.append_assistant_text(&format!("Assistant output line {}\n", i));
    }

    let backend = TestBackend::new(112, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 112, 24);

    // 1. Scroll to top: all lines of the pasted user prompt must be shown in full
    view.scroll_to_top();
    terminal
        .draw(|frame| {
            let ctx = TimelineContext {
                theme: &theme,
                is_working: false,
                working_millis: 0,
                current_activity: None,
                spinner_style: crate::ui::animation::SpinnerStyle::from_id("braille"),
                workspace: std::path::Path::new("/tmp"),
                provider: "openrouter",
                model: "qwen",
            };
            view.render(frame, area, &ctx);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let mut top_text = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            top_text.push_str(buf[(x, y)].symbol());
        }
        top_text.push('\n');
    }
    assert!(
        top_text.contains("final-marker-line"),
        "Expected full pasted prompt lines to be rendered in TUI output, got:\n{}",
        top_text
    );

    // 2. Scroll down: sticky top prompt header must wrap on full word boundaries
    view.scroll_to_bottom();
    terminal
        .draw(|frame| {
            let ctx = TimelineContext {
                theme: &theme,
                is_working: false,
                working_millis: 0,
                current_activity: None,
                spinner_style: crate::ui::animation::SpinnerStyle::from_id("braille"),
                workspace: std::path::Path::new("/tmp"),
                provider: "openrouter",
                model: "qwen",
            };
            view.render(frame, area, &ctx);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let mut row0 = String::new();
    let mut row1 = String::new();
    for x in 0..area.width {
        row0.push_str(buf[(x, 0)].symbol());
        row1.push_str(buf[(x, 1)].symbol());
    }
    assert!(
        !row1.trim_start().starts_with("re,"),
        "Sticky prompt header must wrap on full word boundaries instead of splitting 'secure,': row0={:?} row1={:?}",
        row0.trim_end(),
        row1.trim_end()
    );
}

#[test]
fn test_resolve_plan_tool_display_upgrades_opaque_active_step_labels() {
    let mut view = TimelineView::new();
    view.add_tool_call(
        "update_progress".to_string(),
        r#"{"step":"active","status":"completed"}"#.to_string(),
    );
    view.finish_tool_call(
        "update_progress",
        true,
        "✔ Updated step 'Nav + Hero + live terminal preview' to 'completed'. Current plan: 2/8 tasks completed.\n► Next Active Step: [>] \"Swarm DAG\".".to_string(),
        12,
    );
    match view.entries.last() {
        Some(TimelineEntry::ToolFinished {
            command_or_path, ..
        }) => {
            assert_eq!(
                command_or_path,
                "[2/8 completed] Nav + Hero + live terminal preview"
            );
        }
        other => panic!("Unexpected entry: {other:?}"),
    }

    let sub_resolved = TimelineView::resolve_plan_tool_display(
        "update_progress",
        "✔ Step 'Run static build, typecheck, and lint verification' is already 'completed' (auto-reconciled — active step [>] preserved). Current plan: 7/8 macro tasks completed (active step sub-plan: 1/4 sub-steps completed).",
    );
    assert_eq!(
        sub_resolved.as_deref(),
        Some("[7/8 · sub 1/4 completed] Run static build, typecheck, and lint verification")
    );

    let final_resolved = TimelineView::resolve_plan_tool_display(
        "update_progress",
        "✔ Updated step 'Run final regression build, lint, and pre-completion verification' to 'completed'. 🎉 All 8 tasks in active plan completed (100%)!",
    );
    assert_eq!(
        final_resolved.as_deref(),
        Some("[8/8 completed] Run final regression build, lint, and pre-completion verification")
    );

    let archived_resolved = TimelineView::resolve_plan_tool_display(
        "archive_plan",
        "✔ Archived completed task plan (8/8 steps completed) -> .minicode/plan/archive/20261113_archived_plan.md\n  ✓ 1. Scaffold stack",
    );
    assert_eq!(archived_resolved.as_deref(), Some("[8/8 completed]"));
}

#[test]
fn test_record_plan_progress_update_emits_catchup_milestones_and_deduplicates_explicit_calls() {
    let mut view = TimelineView::new();
    view.entries.push(TimelineEntry::UserPrompt(
        "Build minicode landing page".to_string(),
    ));

    let plan_v0 = LivePlanBlock {
        title: "minicode".to_string(),
        total_tasks: 2,
        completed_tasks: 0,
        active_task: Some("Hero section".to_string()),
        tasks: vec![
            LivePlanTaskItem {
                title: "Above-the-fold".to_string(),
                status: LivePlanTaskStatus::InProgress,
                sub_tasks: vec![
                    LivePlanTaskItem {
                        title: "Hero section".to_string(),
                        status: LivePlanTaskStatus::InProgress,
                        sub_tasks: vec![],
                    },
                    LivePlanTaskItem {
                        title: "TerminalPreview".to_string(),
                        status: LivePlanTaskStatus::Pending,
                        sub_tasks: vec![],
                    },
                ],
            },
            LivePlanTaskItem {
                title: "Verification".to_string(),
                status: LivePlanTaskStatus::Pending,
                sub_tasks: vec![],
            },
        ],
    };

    // Initial plan baseline (`prev_plan = None`) must not emit synthetic completion entries
    view.record_plan_progress_update(None, &plan_v0);
    assert_eq!(view.entries.len(), 1);

    // Catch-up after `write_file` (not explicit `update_progress`) emits substep milestone
    view.add_tool_call(
        "write_file".to_string(),
        "src/components/TerminalPreview.tsx".to_string(),
    );
    view.finish_tool_call("write_file", true, "Wrote file".to_string(), 8);

    let mut plan_v1 = plan_v0.clone();
    plan_v1.tasks[0].sub_tasks[0].status = LivePlanTaskStatus::Completed;
    plan_v1.tasks[0].sub_tasks[1].status = LivePlanTaskStatus::InProgress;
    plan_v1.active_task = Some("TerminalPreview".to_string());
    view.record_plan_progress_update(Some(&plan_v0), &plan_v1);

    match view.entries.last() {
        Some(TimelineEntry::ToolFinished {
            name,
            command_or_path,
            duration_ms,
            ..
        }) => {
            assert_eq!(name, "update_progress");
            assert_eq!(command_or_path, "[0/2 · sub 1/2 completed] Hero section");
            assert!(duration_ms.is_none());
        }
        other => panic!("Expected catch-up update_progress entry, got: {other:?}"),
    }

    // Explicit `update_progress` call immediately preceding `PlanUpdated` must NOT emit a duplicate!
    view.add_tool_call(
        "update_progress".to_string(),
        r#"{"step":"active","status":"completed"}"#.to_string(),
    );
    view.finish_tool_call(
        "update_progress",
        true,
        "✔ Updated step 'TerminalPreview' to 'completed'. Current plan: 1/2 tasks completed."
            .to_string(),
        5,
    );
    let entries_before = view.entries.len();

    let mut plan_v2 = plan_v1.clone();
    plan_v2.completed_tasks = 1;
    plan_v2.tasks[0].status = LivePlanTaskStatus::Completed;
    plan_v2.tasks[0].sub_tasks[1].status = LivePlanTaskStatus::Completed;
    plan_v2.tasks[1].status = LivePlanTaskStatus::InProgress;
    plan_v2.active_task = Some("Verification".to_string());
    view.record_plan_progress_update(Some(&plan_v1), &plan_v2);

    assert_eq!(
        view.entries.len(),
        entries_before,
        "Explicit update_progress call must not be duplicated by record_plan_progress_update"
    );
}
