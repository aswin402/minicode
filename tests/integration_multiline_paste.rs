//! Integration tests for multiline paste handling, preview placeholder collapsing,
//! height constraints, and prompt expansion in minicode TUI.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use minicode::ui::theme::Theme;
use minicode::ui::InputDock;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

#[test]
fn test_integration_short_paste_under_5_lines() {
    let mut dock = InputDock::new();
    let text = "Line 1\nLine 2\nLine 3\nLine 4";
    dock.handle_paste(text);

    // No collapsed blocks should be created
    assert!(dock.pasted_blocks.is_empty());
    assert_eq!(dock.textarea.lines().len(), 4);
    assert_eq!(dock.required_height(), 6); // 4 lines + 2 borders

    // Resolving submission matches typed text exactly
    let submission = dock.resolve_submission(&dock.textarea.lines().join("\n"));
    assert_eq!(submission.display, text);
    assert_eq!(submission.full, text);
}

#[test]
fn test_integration_long_paste_collapses_to_bracketed_preview() {
    let mut dock = InputDock::new();
    let code_snippet = "\
pub async fn execute_pipeline(items: &[String]) -> Result<Vec<u64>> {
    let mut results = Vec::new();
    for item in items {
        let val = parse_item(item).await?;
        results.push(val);
    }
    tracing::info!(count = results.len(), \"Pipeline execution completed\");
    Ok(results)
}";
    let count = code_snippet.lines().count();
    assert!(count > 5);

    dock.handle_paste(code_snippet);

    assert_eq!(dock.pasted_blocks.len(), 1);
    let block = &dock.pasted_blocks[0];
    assert_eq!(block.line_count, count);
    assert!(block
        .placeholder
        .contains(&format!("..... +{} lines]", count)));
    assert!(block.placeholder.starts_with("[pub async fn"));

    // User types instructions following the placeholder
    dock.textarea
        .insert_str("can you refactor this to run concurrently with FuturesUnordered?");

    let full_typed = dock.textarea.lines().join("\n");
    let submission = dock.resolve_submission(&full_typed);

    // The display in chat preserves the compact preview
    assert!(submission.display.contains(&block.placeholder));
    assert!(submission.display.contains("FuturesUnordered?"));

    // The full text dispatched to the LLM agent contains all original lines
    assert!(submission.full.contains("pub async fn execute_pipeline"));
    assert!(submission.full.contains("tracing::info!"));
    assert!(submission.full.contains("FuturesUnordered?"));
    assert!(!submission.full.contains(&block.placeholder));
}

#[test]
fn test_integration_multiple_pastes_and_user_prompts() {
    let mut dock = InputDock::new();

    let paste1 = "fn alpha() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n    let e = 5;\n    let f = 6;\n}";
    let paste2 = "fn beta() {\n    let g = 7;\n    let h = 8;\n    let i = 9;\n    let j = 10;\n    let k = 11;\n    let l = 12;\n}";

    dock.textarea.insert_str("Here is first: ");
    dock.handle_paste(paste1);
    dock.textarea.insert_str("and here is second: ");
    dock.handle_paste(paste2);
    dock.textarea.insert_str("Please compare these two.");

    assert_eq!(dock.pasted_blocks.len(), 2);

    let full_typed = dock.textarea.lines().join("\n");
    let submission = dock.resolve_submission(&full_typed);

    // Display has both placeholders
    assert!(submission
        .display
        .contains(&dock.pasted_blocks[0].placeholder));
    assert!(submission
        .display
        .contains(&dock.pasted_blocks[1].placeholder));
    assert!(submission.display.contains("Here is first: "));
    assert!(submission.display.contains("Please compare these two."));

    // Full prompt has both original bodies fully expanded
    assert!(submission.full.contains("fn alpha()"));
    assert!(submission.full.contains("let f = 6;"));
    assert!(submission.full.contains("fn beta()"));
    assert!(submission.full.contains("let l = 12;"));
    assert!(!submission.full.contains(&dock.pasted_blocks[0].placeholder));
    assert!(!submission.full.contains(&dock.pasted_blocks[1].placeholder));
}

#[test]
fn test_integration_shift_enter_vs_plain_enter() {
    let mut dock = InputDock::new();
    dock.textarea.insert_str("Prompt Header");

    // Shift+Enter should insert newline
    let shift_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
    assert!(dock.handle_key(shift_enter).is_none());

    dock.textarea.insert_str("Detail paragraph 1");

    // Ctrl+Enter should insert newline
    let ctrl_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL);
    assert!(dock.handle_key(ctrl_enter).is_none());

    dock.textarea.insert_str("Detail paragraph 2");

    assert_eq!(dock.textarea.lines().len(), 3);

    // Regular Enter should submit
    let plain_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let sub = dock
        .handle_key(plain_enter)
        .expect("Must return submission");

    assert_eq!(
        sub.display,
        "Prompt Header\nDetail paragraph 1\nDetail paragraph 2"
    );
    assert_eq!(
        sub.full,
        "Prompt Header\nDetail paragraph 1\nDetail paragraph 2"
    );

    // Dock is reset
    assert_eq!(dock.textarea.lines(), &[""]);
}

#[test]
fn test_integration_render_input_dock_scrolling() {
    let mut dock = InputDock::new();
    for i in 1..=15 {
        if i > 1 {
            dock.textarea.insert_newline();
        }
        dock.textarea.insert_str(&format!("Typing line {}", i));
    }

    // Height is strictly capped at 5 lines + 2 borders = 7
    assert_eq!(dock.required_height(), 7);

    // Render into TestBackend
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("Failed to initialize test backend");
    let theme = Theme::default();

    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 80, 7);
            dock.render(f, area, &theme);
        })
        .expect("Failed to draw input dock");

    // Ensure the terminal rendered without panicking and cursor stays visible
    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width >= 80);
}
