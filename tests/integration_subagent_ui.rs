/// Integration tests for Phase 35: Adaptive Inline Subagent UI & Swarm Live Stream Engine
///
/// Tests Crush/OpenCode style subagent tree blocks, adaptive swarm matrix cards,
/// role color styling, and timeline view state transitions.
use minicode::ui::theme::Theme;
use minicode::ui::view::{
    SubagentItemStatus, SubagentTreeBlock, SubagentTreeItem, SwarmMatrixBlock, TimelineEntry,
    TimelineView,
};

#[test]
fn test_theme_role_accent_color_mapping() {
    let theme = Theme::aura_dark();

    // Researcher -> Cyan / Info
    assert_eq!(theme.role_accent_color("Researcher"), theme.info);
    assert_eq!(theme.role_accent_color("researcher"), theme.info);

    // CodeReviewer -> Magenta / Highlight
    assert_eq!(theme.role_accent_color("CodeReviewer"), theme.highlight);
    assert_eq!(theme.role_accent_color("critic"), theme.highlight);

    // TestEngineer -> Green / Success
    assert_eq!(theme.role_accent_color("TestEngineer"), theme.success);
    assert_eq!(theme.role_accent_color("qa_worker"), theme.success);

    // SecurityAuditor -> Warm Orange / Warning
    assert_eq!(theme.role_accent_color("SecurityAuditor"), theme.warning);
    assert_eq!(theme.role_accent_color("audit"), theme.warning);

    // Custom / Default -> Brand Accent
    assert_eq!(theme.role_accent_color("custom_agent"), theme.brand_accent);
}

#[test]
fn test_subagent_tree_lifecycle_and_updates() {
    let mut timeline = TimelineView::new();

    let tree_block = SubagentTreeBlock {
        id: "researcher-1".to_string(),
        role_name: "Researcher".to_string(),
        task_prompt: "Research the architecture of herdr".to_string(),
        items: vec![SubagentTreeItem {
            name: "ripgrep_search".to_string(),
            detail: "\"portable-pty\"".to_string(),
            status: SubagentItemStatus::Success,
        }],
        is_running: true,
        is_success: false,
        outcome: None,
        error_message: None,
        tokens_used: 1200,
        duration_ms: None,
    };

    timeline.add_subagent_tree(tree_block);
    assert_eq!(timeline.entries.len(), 1);

    // Update item
    timeline.update_subagent_tree_item(
        "researcher-1",
        SubagentTreeItem {
            name: "read_file".to_string(),
            detail: "src/agent/subagent/mod.rs".to_string(),
            status: SubagentItemStatus::Running,
        },
    );

    if let TimelineEntry::SubagentTree(ref b) = timeline.entries[0] {
        assert_eq!(b.items.len(), 2);
        assert_eq!(b.items[1].name, "read_file");
        assert!(b.is_running);
    } else {
        panic!("Expected SubagentTree entry");
    }

    // Complete tree
    timeline.complete_subagent_tree(
        "researcher-1",
        true,
        Some("Herdr uses client-server daemon over Unix sockets".to_string()),
        None,
        4200,
        Some(1400),
    );

    if let TimelineEntry::SubagentTree(ref b) = timeline.entries[0] {
        assert!(!b.is_running);
        assert!(b.is_success);
        assert_eq!(b.tokens_used, 4200);
        assert_eq!(b.duration_ms, Some(1400));
        assert!(b.outcome.is_some());
    } else {
        panic!("Expected SubagentTree entry");
    }
}

#[test]
fn test_subagent_swarm_matrix_toggle() {
    let mut timeline = TimelineView::new();

    let worker1 = SubagentTreeBlock {
        id: "researcher-1".to_string(),
        role_name: "Researcher".to_string(),
        task_prompt: "Explore codebase".to_string(),
        items: Vec::new(),
        is_running: false,
        is_success: true,
        outcome: Some("Done".to_string()),
        error_message: None,
        tokens_used: 2000,
        duration_ms: Some(800),
    };

    let worker2 = SubagentTreeBlock {
        id: "tester-2".to_string(),
        role_name: "TestEngineer".to_string(),
        task_prompt: "Run tests".to_string(),
        items: Vec::new(),
        is_running: true,
        is_success: false,
        outcome: None,
        error_message: None,
        tokens_used: 1500,
        duration_ms: None,
    };

    let swarm = SwarmMatrixBlock {
        title: "Subagent Swarm".to_string(),
        workers: vec![worker1, worker2],
        is_expanded: false,
        total_tokens: 3500,
        is_running: true,
        duration_ms: Some(1200),
    };

    timeline.add_subagent_swarm(swarm);
    assert_eq!(timeline.entries.len(), 1);

    if let TimelineEntry::SubagentSwarm(ref s) = timeline.entries[0] {
        assert!(!s.is_expanded);
        assert_eq!(s.workers.len(), 2);
    }

    // Toggle expansion
    timeline.toggle_subagent_swarm();

    if let TimelineEntry::SubagentSwarm(ref s) = timeline.entries[0] {
        assert!(s.is_expanded);
    }
}
