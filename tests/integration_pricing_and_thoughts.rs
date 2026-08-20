use minicode::agent::pricing::ModelPricing;
use minicode::ui::theme::Theme;
use minicode::ui::view::{TimelineContext, TimelineView};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use std::path::Path;

#[test]
fn test_pricing_calculator_all_major_providers() {
    let sonnet_cost = ModelPricing::calculate_cost("anthropic", "claude-3-5-sonnet", 10_000, 2_000);
    // (10k / 1M) * $3 + (2k / 1M) * $15 = 0.03 + 0.03 = 0.06
    assert!((sonnet_cost - 0.06).abs() < 0.001);
    assert_eq!(ModelPricing::format_cost(sonnet_cost), "$0.06");

    let deepseek_cost = ModelPricing::calculate_cost("deepseek", "deepseek-chat", 100_000, 10_000);
    assert!(deepseek_cost > 0.0);

    let local_cost = ModelPricing::calculate_cost("ollama", "llama3.3", 50_000, 10_000);
    assert_eq!(local_cost, 0.0);
    assert_eq!(ModelPricing::format_cost(local_cost), "$0.00");
}

#[test]
fn test_timeline_thought_block_and_thinking_spinner_rendering() {
    let mut timeline = TimelineView::new();
    timeline.add_user_message("Refactor database".to_string());
    timeline.add_thought_block(
        "Analyzing schema dependencies in models.rs...".to_string(),
        Some(1.5),
    );
    timeline.append_assistant_delta("Here is the refactored database connection.");

    let theme = Theme::aura_dark();
    let workspace = Path::new(".");
    let ctx = TimelineContext {
        theme: &theme,
        is_working: true,
        working_millis: 1500,
        workspace,
        provider: "anthropic",
        model: "claude-3-5-sonnet",
    };

    let backend = TestBackend::new(80, 25);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 80, 25);
            timeline.render(f, area, &ctx);
        })
        .unwrap();

    let text_lines = timeline.selection.cached_plain_lines.borrow().clone();
    let combined = text_lines.join("\n");

    assert!(combined.contains("Refactor database"));
    assert!(combined.contains("• Thought for 1.5s"));
    assert!(combined.contains("Analyzing schema dependencies"));
    assert!(combined.contains("Thinking"));
}

#[test]
fn test_cross_chunk_streaming_thoughts() {
    let mut timeline = TimelineView::new();
    // Simulate streaming chunks split across tokens
    timeline.append_assistant_delta("<thought>Step 1: ");
    timeline.append_assistant_delta("Inspecting Cargo.toml for dependencies.\n");
    timeline.append_assistant_delta("Step 2: Checking build targets.</thought>");
    timeline.append_assistant_delta("Everything looks good to proceed!");

    assert_eq!(timeline.entries.len(), 2);
    if let minicode::ui::view::TimelineEntry::ThoughtBlock { text, .. } = &timeline.entries[0] {
        assert!(text.contains("Step 1: Inspecting Cargo.toml"));
        assert!(text.contains("Step 2: Checking build targets."));
    } else {
        panic!("First entry should be ThoughtBlock");
    }

    if let minicode::ui::view::TimelineEntry::AssistantMarkdown(text) = &timeline.entries[1] {
        assert_eq!(text, "Everything looks good to proceed!");
    } else {
        panic!("Second entry should be AssistantMarkdown");
    }
}
