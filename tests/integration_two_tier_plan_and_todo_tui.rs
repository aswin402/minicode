//! Integration tests for Two-Tier Plan Hierarchy, Scoped Active Phase Tracking,
//! Inline Todo Widget Rendering, and Interactive Milestone DAG Modal.

use minicode::config::{Config, TodoWidgetStyle};
use minicode::context::memory::working_memory::{TaskItemStatus, WorkingMemory};
use minicode::ui::modals::todo::{TodoModalPane, TodoModalState};
use minicode::ui::theme::Theme;
use minicode::ui::todo_widget::{render_todo_widget, todo_widget_required_height};
use minicode::ui::view::{LivePlanBlock, LivePlanTaskItem, LivePlanTaskStatus};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_integration_two_tier_scoped_active_phase_parsing() {
    let dir = tempdir().expect("Failed to create tempdir");
    let docs_dir = dir.path().join("onpkg_docs").join("core");
    fs::create_dir_all(&docs_dir).expect("Failed to create docs dir");

    let todo_content = r#"# Project Task Tracker

## Phase 1
- [x] T1: Scaffolding and project structure setup
- [x] T2: Database schema migrations

## Phase 2
- [x] T3: Authentication endpoints
- [>] T4: Session management and refresh tokens
- [ ] T5: Role-based authorization

## Phase 3
- [ ] T6: Billing and stripe integration
- [ ] T7: Performance benchmarking
"#;

    fs::write(docs_dir.join("todo.md"), todo_content).expect("Failed to write todo.md");

    let wm = WorkingMemory::new(dir.path());

    // 1. Milestone DAG parsing
    let milestones = wm.read_roadmap_milestones();
    assert_eq!(milestones.len(), 3);
    assert_eq!(milestones[0].id, "Phase 1");
    assert_eq!(milestones[0].completed_tasks, 2);
    assert_eq!(milestones[0].total_tasks, 2);
    assert!(!milestones[0].is_active);

    assert_eq!(milestones[1].id, "Phase 2");
    assert_eq!(milestones[1].completed_tasks, 1);
    assert_eq!(milestones[1].in_progress_tasks, 1);
    assert_eq!(milestones[1].pending_tasks, 1);
    assert_eq!(milestones[1].total_tasks, 3);
    assert!(milestones[1].is_active);

    assert_eq!(milestones[2].id, "Phase 3");
    assert_eq!(milestones[2].completed_tasks, 0);
    assert_eq!(milestones[2].total_tasks, 2);
    assert!(!milestones[2].is_active);

    // 2. Scoped Active Phase Tracking: Must only return Phase 2 (not all 7 tasks across all phases)
    let (active_title, tasks) = wm.read_active_phase_tasks();
    assert_eq!(active_title.as_deref(), Some("Phase 2"));
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].title, "T3: Authentication endpoints");
    assert_eq!(tasks[0].status, TaskItemStatus::Completed);
    assert_eq!(tasks[1].title, "T4: Session management and refresh tokens");
    assert_eq!(tasks[1].status, TaskItemStatus::InProgress);
    assert_eq!(tasks[2].title, "T5: Role-based authorization");
    assert_eq!(tasks[2].status, TaskItemStatus::Pending);
}

#[test]
fn test_integration_auto_hide_when_all_phases_completed() {
    let dir = tempdir().expect("Failed to create tempdir");
    let docs_dir = dir.path().join(".minicode");
    fs::create_dir_all(&docs_dir).expect("Failed to create dir");

    let todo_content = r#"# All Tasks Finished
## Phase 1
- [x] T1: Setup
- [x] T2: Launch
"#;
    fs::write(docs_dir.join("todo.md"), todo_content).expect("Failed to write todo.md");

    let wm = WorkingMemory::new(dir.path());
    let (active_title, tasks) = wm.read_active_phase_tasks();

    // When everything is completed, active phase is None and tasks are empty -> widget auto-hides!
    assert!(active_title.is_none());
    assert!(tasks.is_empty());
}

#[test]
fn test_integration_todo_widget_all_styles_and_heights() {
    let plan = LivePlanBlock {
        title: "Phase 142".to_string(),
        completed_tasks: 1,
        total_tasks: 3,
        active_task: Some("T2: Form validation logic and interactive UI".to_string()),
        tasks: vec![
            LivePlanTaskItem {
                title: "T1: Scaffolding and project structure setup".to_string(),
                status: LivePlanTaskStatus::Completed,
            },
            LivePlanTaskItem {
                title: "T2: Form validation logic and interactive UI".to_string(),
                status: LivePlanTaskStatus::InProgress,
            },
            LivePlanTaskItem {
                title: "T3: End-to-end verification and documentation".to_string(),
                status: LivePlanTaskStatus::Pending,
            },
        ],
    };

    let theme = Theme::aura_dark();
    let plan_opt = Some(plan.clone());

    // 1. Tree Style (Oh My Pi default)
    let height_tree = todo_widget_required_height(&plan_opt, TodoWidgetStyle::Tree);
    assert_eq!(height_tree, 7); // 3 tasks + 4 = 7 (including top space)
    let lines_tree = render_todo_widget(&plan, TodoWidgetStyle::Tree, 80, &theme);
    assert_eq!(lines_tree.len(), 7);
    assert!(lines_tree[0].spans.is_empty() || lines_tree[0].spans[0].content.is_empty());

    // 2. Card Style (Rounded container box)
    let height_card = todo_widget_required_height(&plan_opt, TodoWidgetStyle::Card);
    assert_eq!(height_card, 7); // 3 tasks + 4 = 7 (including top space)
    let lines_card = render_todo_widget(&plan, TodoWidgetStyle::Card, 80, &theme);
    assert_eq!(lines_card.len(), 7);
    assert!(lines_card[0].spans.is_empty() || lines_card[0].spans[0].content.is_empty());

    // 3. Rail Style (Left accent bar)
    let height_rail = todo_widget_required_height(&plan_opt, TodoWidgetStyle::Rail);
    assert_eq!(height_rail, 6); // 3 tasks + 3 = 6 (including top space)
    let lines_rail = render_todo_widget(&plan, TodoWidgetStyle::Rail, 80, &theme);
    assert_eq!(lines_rail.len(), 6);
    assert!(lines_rail[0].spans.is_empty() || lines_rail[0].spans[0].content.is_empty());

    // 4. Minimal Style (Open rule header)
    let height_minimal = todo_widget_required_height(&plan_opt, TodoWidgetStyle::Minimal);
    assert_eq!(height_minimal, 6); // 3 tasks + 3 = 6 (including top space)
    let lines_minimal = render_todo_widget(&plan, TodoWidgetStyle::Minimal, 80, &theme);
    assert_eq!(lines_minimal.len(), 6);
    assert!(lines_minimal[0].spans.is_empty() || lines_minimal[0].spans[0].content.is_empty());

    // 5. None / Auto-hide when plan is None
    let height_none = todo_widget_required_height(&None, TodoWidgetStyle::Tree);
    assert_eq!(height_none, 0);
}

#[test]
fn test_integration_todo_modal_dag_interaction() {
    let dir = tempdir().expect("Failed to create tempdir");
    let docs_dir = dir.path().join("onpkg_docs").join("core");
    fs::create_dir_all(&docs_dir).expect("Failed to create docs dir");

    let todo_content = r#"# Project Task Tracker

## Active Milestones (Phase 1)
- [x] T1: Scaffolding

## Active Milestones (Phase 2)
- [x] T2: Auth
- [>] T3: Sessions
- [ ] T4: Roles
"#;
    fs::write(docs_dir.join("todo.md"), todo_content).expect("Failed to write todo.md");

    let wm = WorkingMemory::new(dir.path());
    let milestones = wm.read_roadmap_milestones();
    let mut state = TodoModalState::new(milestones);

    // Initial state: auto-selected active milestone (Phase 2 at index 1)
    assert_eq!(state.selected_milestone, 1);
    assert_eq!(state.active_pane, TodoModalPane::Milestones);
    assert_eq!(state.selected_task, 0);

    // Switch pane to Tasks with toggle_pane
    state.toggle_pane();
    assert_eq!(state.active_pane, TodoModalPane::Tasks);

    // Navigate down tasks
    state.next();
    assert_eq!(state.selected_task, 1);
    state.next();
    assert_eq!(state.selected_task, 2);

    // Switch back to Milestones
    state.toggle_pane();
    assert_eq!(state.active_pane, TodoModalPane::Milestones);
    state.prev();
    assert_eq!(state.selected_milestone, 0);
}

#[test]
fn test_integration_config_todo_style_persistence() {
    let mut config = Config::default();
    assert_eq!(config.ui.todo_style, "tree");

    config.ui.todo_style = "card".to_string();
    assert_eq!(config.ui.todo_style_enum(), TodoWidgetStyle::Card);

    config.ui.todo_style = "rail".to_string();
    assert_eq!(config.ui.todo_style_enum(), TodoWidgetStyle::Rail);

    config.ui.todo_style = "minimal".to_string();
    assert_eq!(config.ui.todo_style_enum(), TodoWidgetStyle::Minimal);
}

#[test]
fn test_integration_todo_modal_smart_collapse_and_rendering() {
    use minicode::ui::modals::todo::render_todo_modal;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let dir = tempdir().expect("Failed to create tempdir");
    let docs_dir = dir.path().join("onpkg_docs").join("core");
    fs::create_dir_all(&docs_dir).expect("Failed to create docs dir");

    let mut todo_content = String::from("# Large Project Roadmap\n\n");
    for i in 1..=10 {
        todo_content.push_str(&format!("## Phase {}\n", i));
        if i < 8 {
            todo_content.push_str("- [x] T1: Completed setup\n- [x] T2: Completed polish\n\n");
        } else if i == 8 {
            todo_content.push_str("- [x] T1: Auth done\n- [>] T2: Distributed consensus in flight\n- [ ] T3: Council tool\n\n");
        } else {
            todo_content.push_str("- [ ] T1: Upcoming work\n\n");
        }
    }
    fs::write(docs_dir.join("todo.md"), todo_content).expect("Failed to write todo.md");

    let wm = WorkingMemory::new(dir.path());
    let milestones = wm.read_roadmap_milestones();
    assert_eq!(milestones.len(), 10);

    let mut state = TodoModalState::new(milestones);
    // Active milestone is Phase 8 (index 7)
    assert_eq!(state.selected_milestone, 7);
    assert!(!state.show_all_completed);

    // Toggle expand
    state.toggle_expand();
    assert!(state.show_all_completed);
    state.toggle_expand();
    assert!(!state.show_all_completed);

    // Render to Ratatui test buffer
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("Failed to create terminal");
    let theme = Theme::aura_dark();

    terminal
        .draw(|f| {
            render_todo_modal(f, f.area(), &state, &theme);
        })
        .expect("Failed to draw todo modal");

    let buffer = terminal.backend().buffer();
    let text = format!("{:?}", buffer);

    // Assert top border embeds clean title without clutter
    assert!(text.contains("Roadmap"));
    assert!(text.contains("Phase 8"));

    // Assert bottom dock embeds progress meter and keyhints
    assert!(text.contains("Progress"));
    assert!(text.contains("33%"));
    assert!(text.contains("[Tab]"));
    assert!(text.contains("[Space]"));
}
