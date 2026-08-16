use minicode::ui::approval::{ApprovalModalState, ApprovalOption, ApprovalResponse};
use minicode::ui::diff_viewer::DiffViewer;
use minicode::ui::theme::Theme;
use serde_json::json;

#[test]
fn test_diff_viewer_unified_rendering() {
    let theme = Theme::default();
    let old_code = "fn greet() {\n    println!(\"hello\");\n}\n";
    let new_code = "fn greet() {\n    println!(\"hello world\");\n}\n";

    let lines = DiffViewer::render_diff(old_code, new_code, &theme, 10);
    assert!(!lines.is_empty());

    let rendered: Vec<String> = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();

    assert!(rendered
        .iter()
        .any(|l| l.contains("- ") && l.contains("println!(\"hello\");")));
    assert!(rendered
        .iter()
        .any(|l| l.contains("+ ") && l.contains("println!(\"hello world\");")));
}

#[test]
fn test_approval_modal_options_and_navigation() {
    let theme = Theme::default();
    let args = json!({
        "path": "src/main.rs",
        "search_block": "let x = 1;",
        "replace_block": "let x = 2;"
    });

    let mut modal =
        ApprovalModalState::from_tool_call(1, "call_patch_1", "patch_file", &args, &theme);

    assert_eq!(modal.target_description, "Patch file: src/main.rs");
    assert_eq!(modal.selected_index, 0);

    // Navigate through all options
    modal.next_option();
    assert_eq!(modal.selected_index, 1);
    modal.next_option();
    assert_eq!(modal.selected_index, 2);
    modal.next_option();
    assert_eq!(modal.selected_index, 3);

    // Wrap around
    modal.next_option();
    assert_eq!(modal.selected_index, 0);

    // Verify option labels
    let options = ApprovalOption::all();
    assert_eq!(options.len(), 4);
    assert!(options[0].label().contains("Accept"));
    assert!(options[1].label().contains("Reject"));
    assert!(options[2].label().contains("Allow for this Session"));
    assert!(options[3].label().contains("Type Feedback"));
}

#[test]
fn test_approval_modal_custom_feedback_flow() {
    let theme = Theme::default();
    let args = json!({
        "cmd": "cargo clean"
    });

    let mut modal = ApprovalModalState::from_tool_call(2, "call_exec_1", "exec_cmd", &args, &theme);

    assert!(modal.target_description.contains("cargo clean"));

    // Select Option 4 (Type Feedback)
    modal.select_by_number(4);
    assert!(modal.is_typing_feedback);

    // Type custom instructions
    for ch in "Use cargo test instead".chars() {
        modal.handle_char(ch);
    }
    assert_eq!(modal.feedback_input, "Use cargo test instead");

    // Test backspace
    modal.handle_backspace();
    assert_eq!(modal.feedback_input, "Use cargo test instea");
    modal.handle_char('d');

    // Confirm
    let response = modal.confirm_selection();
    assert_eq!(
        response,
        Some(ApprovalResponse::CustomFeedback(
            "Use cargo test instead".to_string()
        ))
    );
}
