use super::*;

#[test]
fn test_working_memory_lifecycle() {
    let temp_dir = std::env::temp_dir().join(format!("minicode_test_wm_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    assert!(!wm.has_active_plan());

    // Initialize plan
    let steps = vec![
        "Scaffold API endpoints".to_string(),
        "Write integration tests".to_string(),
        "Run cargo clippy".to_string(),
    ];
    wm.init_plan("Build v0.1.0 API", &steps).unwrap();
    assert!(wm.has_active_plan());

    // Read plan
    let plan = wm.read_plan().unwrap().unwrap();
    assert!(plan.contains("Build v0.1.0 API"));
    assert!(plan.contains("1. [>] Scaffold API endpoints"));

    // Append finding
    wm.append_finding("Discovered existing axum router in src/routes.rs")
        .unwrap();
    let findings = fs::read_to_string(wm.findings_path()).unwrap();
    assert!(findings.contains("axum router in src/routes.rs"));

    // Update progress & check box
    wm.update_progress("Scaffold API endpoints", "Completed")
        .unwrap();
    let updated_plan = wm.read_plan().unwrap().unwrap();
    assert!(updated_plan.contains("[x] Scaffold API endpoints"));

    // Prompt block formatting
    let block = wm.to_prompt_block();
    assert!(block.contains("<working_memory>"));

    // Archive plan
    let archived = wm.archive_plan().unwrap().unwrap();
    assert!(archived.exists());
    assert!(!wm.has_active_plan());

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_senior_engineer_scratchpad_and_task_parsing() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_notes_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);

    // Scratchpad tests
    assert!(wm.read_notes().is_none());
    wm.append_note(
        NoteSection::Invariants,
        "Must use rustls-tls-webpki-roots for pure-Rust portability",
    )
    .unwrap();
    wm.append_note(
        NoteSection::NegativeKnowledge,
        "Do not use walkdir crate due to repository policy",
    )
    .unwrap();

    let notes = wm.read_notes().unwrap();
    assert!(notes.contains("Must use rustls-tls-webpki-roots"));
    assert!(notes.contains("Do not use walkdir crate"));

    // Test todo.md parsing
    let todo_path = temp_dir.join("todo.md");
    fs::write(
        &todo_path,
        "- [x] Task 1: Initialize AST Graph\n- [>] Task 2: Implement 4D Scorer\n- [ ] Task 3: Run Benchmarks\n",
    )
    .unwrap();

    let tasks = wm.read_parsed_tasks();
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].status, TaskItemStatus::Completed);
    assert_eq!(tasks[1].status, TaskItemStatus::InProgress);
    assert_eq!(tasks[2].status, TaskItemStatus::Pending);

    let block = wm.to_prompt_block();
    assert!(block.contains("<live_execution_plan>"));
    assert!(block.contains("✔ Task 1: Initialize AST Graph"));
    assert!(block.contains("▶ Task 2: Implement 4D Scorer"));
    assert!(block.contains("○ Task 3: Run Benchmarks"));
    assert!(block.contains("<senior_engineer_scratchpad>"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_roadmap_milestones_and_active_scoping() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_roadmap_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let todo_path = temp_dir.join("todo.md");

    // 1. Multi-phase file with Phase 1 done, Phase 2 in progress, Phase 3 pending
    let multi_phase_content = r#"
### Phase 140: MiniDev Runtime
- [x] 140.1: Process isolation
- [x] 140.2: Port scanner

### Phase 141: In-TUI Process Monitor
- [x] 141.1: Interactive modal
- [>] 141.2: Status indicators
- [ ] 141.3: Ring-buffer logs
- [ ] 141.4: Telemetry

### Phase 142: Two-Tier Plan
- [ ] 142.1: Scoped WorkingMemory
"#;
    fs::write(&todo_path, multi_phase_content).unwrap();

    let milestones = wm.read_roadmap_milestones();
    assert_eq!(milestones.len(), 3);

    // Phase 140
    assert_eq!(milestones[0].id, "Phase 140");
    assert_eq!(milestones[0].title, "MiniDev Runtime");
    assert_eq!(milestones[0].total_tasks, 2);
    assert_eq!(milestones[0].completed_tasks, 2);
    assert!(!milestones[0].is_active);

    // Phase 141 should be marked active because it has an InProgress task
    assert_eq!(milestones[1].id, "Phase 141");
    assert_eq!(milestones[1].total_tasks, 4);
    assert_eq!(milestones[1].completed_tasks, 1);
    assert_eq!(milestones[1].in_progress_tasks, 1);
    assert_eq!(milestones[1].pending_tasks, 2);
    assert!(milestones[1].is_active);

    // Phase 142
    assert!(!milestones[2].is_active);

    // Scoped active tasks should return Phase 141 only
    let (phase_label, active_tasks) = wm.read_active_phase_tasks();
    assert_eq!(
        phase_label,
        Some("Phase 141: In-TUI Process Monitor".to_string())
    );
    assert_eq!(active_tasks.len(), 4);
    assert_eq!(active_tasks[0].status, TaskItemStatus::Completed);
    assert_eq!(active_tasks[1].status, TaskItemStatus::InProgress);

    // 2. Mark all tasks completed: active tasks should return None
    let all_done_content = r#"
### Phase 140: MiniDev Runtime
- [x] 140.1: Process isolation
- [x] 140.2: Port scanner
"#;
    fs::write(&todo_path, all_done_content).unwrap();
    let (phase_label_done, active_tasks_done) = wm.read_active_phase_tasks();
    assert_eq!(phase_label_done, None);
    assert!(active_tasks_done.is_empty());

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_update_progress_index_matching_and_auto_advancement() {
    let temp_dir = std::env::temp_dir().join(format!("minicode_test_adv_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "1. Setup environment".to_string(),
        "2. Implement backend service".to_string(),
        "3. Verify endpoints".to_string(),
    ];
    wm.init_plan("Test Service Pipeline", &steps).unwrap();

    let tasks = wm.read_parsed_tasks();
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].title, "Setup environment");
    assert_eq!(tasks[0].status, TaskItemStatus::InProgress);
    assert_eq!(tasks[1].status, TaskItemStatus::Pending);

    // 1. Update by 1-based index "1" to completed -> should auto-advance step 2 to InProgress!
    wm.update_progress("1", "completed").unwrap();
    let tasks_after_1 = wm.read_parsed_tasks();
    assert_eq!(tasks_after_1[0].status, TaskItemStatus::Completed);
    assert_eq!(tasks_after_1[1].status, TaskItemStatus::InProgress);
    assert_eq!(tasks_after_1[2].status, TaskItemStatus::Pending);

    // 2. Update by keyword "active" to completed -> should auto-advance step 3 to InProgress!
    wm.update_progress("active", "completed").unwrap();
    let tasks_after_2 = wm.read_parsed_tasks();
    assert_eq!(tasks_after_2[1].status, TaskItemStatus::Completed);
    assert_eq!(tasks_after_2[2].status, TaskItemStatus::InProgress);

    // 3. Update by title substring to completed
    wm.update_progress("Verify endpoints", "completed").unwrap();
    let tasks_after_3 = wm.read_parsed_tasks();
    assert_eq!(tasks_after_3[2].status, TaskItemStatus::Completed);

    // 4. Update non-existent step -> must return helpful error
    let err = wm.update_progress("non_existent_step_xyz", "completed");
    assert!(err.is_err());
    let err_msg = err.unwrap_err().to_string();
    assert!(err_msg.contains("No matching task step found"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_update_progress_space_normalization_and_leading_index() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_norm_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Research components".to_string(),
        "Set up structure".to_string(),
        "Build navbar".to_string(),
        "Build hero".to_string(),
        "Build features".to_string(),
        "Build pricing".to_string(),
        "Build testimonials".to_string(),
        "Build contact".to_string(),
        "Add footer".to_string(),
        "Test and verify all components render correctly".to_string(),
    ];
    wm.init_plan("NovaCode Landing", &steps).unwrap();

    // Verify canonical todo.md was created in minikit_docs/core/todo.md
    let canonical_todo = temp_dir.join("minikit_docs").join("core").join("todo.md");
    assert!(canonical_todo.exists());

    // 1. Update with status "In Progress" (with space) on "10. Test and verify all components render correctly"
    wm.update_progress(
        "10. Test and verify all components render correctly",
        "In Progress",
    )
    .unwrap();

    let tasks = wm.read_parsed_tasks();
    assert_eq!(tasks.len(), 10);
    assert_eq!(tasks[9].status, TaskItemStatus::InProgress);

    // 2. Update with status "Completed"
    wm.update_progress(
        "10. Test and verify all components render correctly",
        "Completed",
    )
    .unwrap();
    let tasks2 = wm.read_parsed_tasks();
    assert_eq!(tasks2[9].status, TaskItemStatus::Completed);

    // 3. Test invalid status produces clear error
    let err = wm.update_progress("1", "unknown_bogus_status");
    assert!(err.is_err());
    let err_msg = err.unwrap_err().to_string();
    assert!(err_msg.contains("Invalid task status"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_read_active_plan_summary_all_completed() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_summary_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Initial setup".to_string(),
        "Core implementation".to_string(),
        "Unit tests".to_string(),
        "Verification and docs".to_string(),
    ];
    wm.init_plan("Test Plan", &steps).unwrap();

    // Mark all 4 steps completed
    for step in &steps {
        wm.update_progress(step, "completed").unwrap();
    }

    // Active plan summary must report 4/4 completed, active_task: None, and all tasks completed
    let summary = wm
        .read_active_plan_summary()
        .expect("Summary must exist when milestones are present");
    assert_eq!(summary.total_tasks, 4);
    assert_eq!(summary.completed_tasks, 4);
    assert_eq!(summary.in_progress_tasks, 0);
    assert_eq!(summary.pending_tasks, 0);
    assert_eq!(summary.active_task, None);
    assert_eq!(summary.tasks.len(), 4);
    for t in &summary.tasks {
        assert_eq!(t.status, TaskItemStatus::Completed);
    }

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_reconcile_workspace_tasks_dynamic_deliverables() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_reconcile_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Create index.html — semantic HTML structure".to_string(),
        "Create styles.css — dark cyber theme and layout".to_string(),
        "Create main.js — terminal preview simulation and animations".to_string(),
        "Verify responsive layout in browser".to_string(),
    ];
    wm.init_plan("Landing Page Plan", &steps).unwrap();

    // Initially no files exist -> 0 reconciled
    let count0 = wm.reconcile_workspace_tasks(&[]);
    assert_eq!(count0, 0);

    // Write index.html to disk
    fs::write(temp_dir.join("index.html"), "<!DOCTYPE html><html></html>").unwrap();
    let count1 = wm.reconcile_workspace_tasks(&["index.html".to_string()]);
    assert_eq!(count1, 1);

    let tasks1 = wm.read_parsed_tasks();
    assert_eq!(tasks1[0].status, TaskItemStatus::Completed);
    assert_eq!(tasks1[1].status, TaskItemStatus::InProgress);

    // Now write styles.css and main.js to disk
    fs::write(temp_dir.join("styles.css"), "body { margin: 0; }").unwrap();
    fs::write(temp_dir.join("main.js"), "console.log('ready');").unwrap();

    // Turn ends with main.js modified -> reconciles tasks 2 and 3!
    let count2 = wm.reconcile_workspace_tasks(&["styles.css".to_string(), "main.js".to_string()]);
    assert_eq!(count2, 2);

    let tasks2 = wm.read_parsed_tasks();
    assert_eq!(tasks2[0].status, TaskItemStatus::Completed);
    assert_eq!(tasks2[1].status, TaskItemStatus::Completed);
    assert_eq!(tasks2[2].status, TaskItemStatus::Completed);
    assert_eq!(tasks2[3].status, TaskItemStatus::InProgress);

    // Explicit verification step requires update_progress (no silent background auto-completion)
    wm.update_progress("Verify responsive layout in browser", "completed")
        .unwrap();

    let final_tasks = wm.read_parsed_tasks();
    assert_eq!(final_tasks.len(), 4);
    assert_eq!(final_tasks[0].status, TaskItemStatus::Completed);
    assert_eq!(final_tasks[1].status, TaskItemStatus::Completed);
    assert_eq!(final_tasks[2].status, TaskItemStatus::Completed);
    assert_eq!(final_tasks[3].status, TaskItemStatus::Completed);

    let summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary.total_tasks, 4);
    assert_eq!(summary.completed_tasks, 4);
    assert_eq!(summary.in_progress_tasks, 0);
    assert_eq!(summary.pending_tasks, 0);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_reconcile_workspace_tasks_semantic_and_single_step() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_reconcile_semantic_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Scaffold project structure".to_string(),
        "Implement modern hero component in src/Hero.tsx".to_string(),
        "Verify all features".to_string(),
    ];
    wm.init_plan("Semantic Plan", &steps).unwrap();

    // 1. Scaffolding task matched by package.json creation
    fs::write(temp_dir.join("package.json"), "{}").unwrap();
    let c1 = wm.reconcile_workspace_tasks(&["package.json".to_string()]);
    assert_eq!(c1, 1);
    let tasks = wm.read_parsed_tasks();
    assert_eq!(tasks[0].status, TaskItemStatus::Completed);
    assert_eq!(tasks[1].status, TaskItemStatus::InProgress);

    // 2. Candidate file src/Hero.tsx created -> reconciles hero component
    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("Hero.tsx"), "export const Hero = () => null;").unwrap();
    let c2 = wm.reconcile_workspace_tasks(&["src/Hero.tsx".to_string()]);
    assert_eq!(c2, 1);
    let tasks_after_hero = wm.read_parsed_tasks();
    assert_eq!(tasks_after_hero[0].status, TaskItemStatus::Completed);
    assert_eq!(tasks_after_hero[1].status, TaskItemStatus::Completed);
    assert_eq!(tasks_after_hero[2].status, TaskItemStatus::InProgress);

    // 3. Verification task completed explicitly
    wm.update_progress("Verify all features", "completed")
        .unwrap();

    let final_tasks = wm.read_parsed_tasks();
    assert_eq!(final_tasks[0].status, TaskItemStatus::Completed);
    assert_eq!(final_tasks[1].status, TaskItemStatus::Completed);
    assert_eq!(final_tasks[2].status, TaskItemStatus::Completed);

    let summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary.total_tasks, 3);
    assert_eq!(summary.completed_tasks, 3);
    assert_eq!(summary.in_progress_tasks, 0);
    assert_eq!(summary.pending_tasks, 0);

    let progress_content = fs::read_to_string(wm.progress_path()).unwrap();
    assert!(progress_content.contains("- Status: Completed (100%)"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_two_tier_planning_architecture_and_switching() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_two_tier_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Setup Tailwind and icons in src/index.css".to_string(),
        "Build Navbar in src/components/Navbar.tsx".to_string(),
    ];
    let milestones = vec![
        "Core UI Components".to_string(),
        "Interactive States & Polish".to_string(),
    ];
    let architecture = "# Blueprint\nTop-to-bottom architecture.";

    wm.init_two_tier_plan(
        "Two-Tier Web App",
        &steps,
        Some(architecture),
        Some(&milestones),
    )
    .unwrap();

    // 1. Verify implementation.md was written
    let impl_file = wm.canonical_implementation_path();
    assert!(impl_file.exists());
    let impl_content = fs::read_to_string(&impl_file).unwrap();
    assert!(impl_content.contains("# Blueprint"));

    // 2. Verify todo.md was written with roadmap milestones
    let todo_file = wm.canonical_todo_path();
    assert!(todo_file.exists());
    let todo_content = fs::read_to_string(&todo_file).unwrap();
    assert!(todo_content.contains("Core UI Components"));
    assert!(todo_content.contains("Interactive States & Polish"));

    // 3. While tactical tasks are pending in task_plan.md, read_active_plan_summary returns the tactical steps
    let summary1 = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary1.total_tasks, 2);
    assert_eq!(summary1.completed_tasks, 0);

    // 4. Complete first tactical step
    wm.update_progress("Setup Tailwind and icons in src/index.css", "completed")
        .unwrap();
    let summary2 = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary2.completed_tasks, 1);

    // 5. Complete second tactical step -> all tactical steps in task_plan.md are now done!
    wm.update_progress("Build Navbar in src/components/Navbar.tsx", "completed")
        .unwrap();

    // 6. After tactical tasks complete, milestone 1 in todo.md was marked completed [x],
    // and summary switches to displaying the roadmap milestones (showing Phase 2 next)!
    let summary3 = wm.read_active_plan_summary().unwrap();
    assert!(
        summary3.phase_label.contains("Phase 2")
            || summary3.phase_label.contains("Interactive States")
            || summary3.phase_label.contains("Two-Tier")
    );

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_update_progress_auto_bootstrap_when_no_plan() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_auto_bootstrap_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    // Initially no plan exists
    assert!(wm.read_parsed_tasks().is_empty());

    // Calling update_progress directly should auto-bootstrap the plan and mark step completed
    let res = wm.update_progress("1. Initial scaffold of project", "completed");
    assert!(res.is_ok());

    let tasks = wm.read_parsed_tasks();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, TaskItemStatus::Completed);

    let progress_content = fs::read_to_string(wm.progress_path()).unwrap();
    assert!(progress_content.contains("- Status: Completed (100%)"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_reconcile_in_file_html_sections_and_bulk_remaining() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_html_sections_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Scaffold project and core files".to_string(),
        "Build Hero section with terminal preview".to_string(),
        "Build Philosophy section".to_string(),
        "Build Autonomous Swarm section".to_string(),
        "Final testing and verification".to_string(),
    ];
    wm.init_plan("Landing Page Plan", &steps).unwrap();

    // Write index.html containing semantic section IDs/comments
    let html_content = r#"<!DOCTYPE html>
<html>
<head><title>minicode</title></head>
<body>
  <!-- HERO -->
  <section id="hero" class="hero"><h1>Hero</h1></section>
  <!-- PHILOSOPHY -->
  <section id="philosophy"><h2>Philosophy</h2></section>
  <!-- SWARM -->
  <section id="swarm"><h2>Swarm</h2></section>
</body>
</html>"#;
    fs::write(temp_dir.join("index.html"), html_content).unwrap();

    // Reconcile workspace tasks against index.html (verifies active deliverable without silent skips)
    let reconciled = wm.reconcile_workspace_tasks(&["index.html".to_string()]);
    assert!(
        reconciled >= 1,
        "Expected at least 1 task reconciled for active deliverable, got {}",
        reconciled
    );

    let tasks = wm.read_parsed_tasks();
    assert_eq!(tasks[0].status, TaskItemStatus::Completed);

    // Verify bulk completion via "remaining" is strictly rejected
    let res = wm.update_progress("remaining", "completed");
    assert!(res.is_err(), "Bulk completion must be rejected");

    // Complete remaining tasks individually
    for idx in 2..=5 {
        let res = wm.update_progress(&idx.to_string(), "completed");
        assert!(res.is_ok());
    }

    let final_tasks = wm.read_parsed_tasks();
    assert_eq!(final_tasks[4].status, TaskItemStatus::Completed);
    let summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary.completed_tasks, 5);
    assert_eq!(summary.total_tasks, 5);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_init_tactical_subplan_and_archive_completion() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_tactical_subplan_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let macro_tasks = vec![
        "Create landing page hero section".to_string(),
        "Build interactive bento features".to_string(),
    ];
    wm.init_plan("Landing Page", &macro_tasks).unwrap();

    // Decompose active task 1 into tactical sub-steps via init_tactical_subplan
    let sub_steps = vec![
        "Check theme tokens in index.css".to_string(),
        "Implement HeroSection.tsx with terminal preview".to_string(),
    ];
    wm.init_tactical_subplan("Create landing page hero section", &sub_steps)
        .unwrap();

    let summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary.total_tasks, 2);
    assert_eq!(summary.completed_tasks, 0);
    assert_eq!(
        summary.active_task.as_deref(),
        Some("Check theme tokens in index.css")
    );
    // Verify Tier-1 macro tasks are preserved in summary.tasks with Tier-2 sub_tasks nested under task 0
    assert_eq!(summary.tasks.len(), 2);
    assert_eq!(summary.tasks[0].title, "Create landing page hero section");
    assert_eq!(summary.tasks[0].sub_tasks.len(), 2);
    assert_eq!(
        summary.tasks[0].sub_tasks[0].title,
        "Check theme tokens in index.css"
    );
    assert_eq!(
        summary.tasks[0].sub_tasks[1].title,
        "Implement HeroSection.tsx with terminal preview"
    );
    assert_eq!(summary.tasks[1].title, "Build interactive bento features");
    assert!(summary.tasks[1].sub_tasks.is_empty());

    // Complete sub-step 1 -> macro task 1 in todo.md must remain [>]
    wm.update_progress("1", "completed").unwrap();
    let todo_mid = fs::read_to_string(wm.canonical_todo_path()).unwrap();
    assert!(todo_mid.contains("- [>] Create landing page hero section"));

    // Complete sub-step 2 -> macro task 1 in todo.md becomes [x] and macro task 2 becomes [>]
    wm.update_progress("2", "completed").unwrap();
    let todo_after = fs::read_to_string(wm.canonical_todo_path()).unwrap();
    assert!(todo_after.contains("- [x] Create landing page hero section"));
    assert!(todo_after.contains("- [>] Build interactive bento features"));

    // Archive plan -> all remaining tasks in todo.md become [x]
    wm.archive_plan().unwrap();
    let todo_final = fs::read_to_string(wm.canonical_todo_path()).unwrap();
    assert!(todo_final.contains("- [x] Create landing page hero section"));
    assert!(todo_final.contains("- [x] Build interactive bento features"));
    assert!(!todo_final.contains("[>]"));
    assert!(!todo_final.contains("[ ]"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest54_direct_plan_sync_and_stem_reconciliation() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_minitest54_sync_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Scaffold static-website foundation".to_string(),
        "Build navigation bar with links and CTA".to_string(),
        "Build hero section with terminal preview animation".to_string(),
        "Build 6-feature bento grid".to_string(),
        "Verify and build production bundle".to_string(),
    ];
    let milestones = vec![
        "Foundation & Structure".to_string(),
        "Hero Section".to_string(),
        "Bento Features".to_string(),
        "Verification".to_string(),
    ];
    wm.init_two_tier_plan("WarpFlow Landing", &steps, None, Some(&milestones))
        .unwrap();

    // 1. Reconcile stack choice when user selects react-vite
    wm.reconcile_stack_choice("react-vite");
    let plan_text = wm.read_task_plan().unwrap().unwrap();
    assert!(plan_text.contains("Scaffold react-vite foundation"));
    assert!(!plan_text.contains("static-website"));

    // 2. Reconcile step 1 via package.json
    fs::write(temp_dir.join("package.json"), "{}").unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["package.json".to_string()]),
        1
    );

    // 3. Reconcile steps 2, 3, 4 via CamelCase component stems (no literal filenames in step titles)
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Navbar.tsx".to_string()]),
        1
    );
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Hero.tsx".to_string()]),
        1
    );
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/FeaturesBento.tsx".to_string()]),
        1
    );

    // 4. Complete final step 5 -> both task_plan.md and todo.md must be 5/5 (100%), never jumping backward to 1/4!
    wm.update_progress("5", "completed").unwrap();
    let summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary.total_tasks, 5);
    assert_eq!(summary.completed_tasks, 5);
    assert_eq!(summary.in_progress_tasks, 0);
    assert_eq!(summary.pending_tasks, 0);

    let todo_final = fs::read_to_string(wm.canonical_todo_path()).unwrap();
    assert!(!todo_final.contains("[>]"));
    assert!(!todo_final.contains("[ ]"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest58_html_write_does_not_complete_css_js_or_verify_steps() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_minitest58_sync_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Create index.html with all sections (navbar, hero, features bento, latency comparison, pricing, footer)".to_string(),
        "Create styles.css with design tokens and component styles".to_string(),
        "Create app.js with interactive latency comparison animation".to_string(),
        "Verify with browser_navigate and browser_screenshot".to_string(),
    ];
    wm.init_plan("PulseDB Landing", &steps).unwrap();

    // Write index.html containing <nav>, <section id="comparison">, etc. (>= 25 lines)
    let mut html_lines = vec![
        "<!DOCTYPE html>".to_string(),
        "<html><body>".to_string(),
        "<nav>PulseDB</nav>".to_string(),
        "<section id=\"comparison\">Comparison</section>".to_string(),
        "<footer>Footer</footer>".to_string(),
    ];
    for i in 0..25 {
        html_lines.push(format!("<!-- line {} -->", i));
    }
    html_lines.push("</body></html>".to_string());
    fs::write(temp_dir.join("index.html"), html_lines.join("\n")).unwrap();

    // Reconciling index.html MUST only complete Step 1, never Steps 2, 3, or 4!
    let completed_count = wm.reconcile_workspace_tasks(&["index.html".to_string()]);
    assert_eq!(completed_count, 1);

    let summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary.completed_tasks, 1);
    assert_eq!(summary.in_progress_tasks, 1);
    assert_eq!(summary.pending_tasks, 2);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest69_stem_morphology_acronym_and_archive_suppression() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_minitest69_sync_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Architecture pipeline: Build multi-layer execution diagram".to_string(),
        "Security & Sandbox section: Build secure isolation cards".to_string(),
        "Final CTA & Footer: Build call-to-action banner and archive plan".to_string(),
    ];
    wm.init_plan("Minicode Landing", &steps).unwrap();

    // 1. ArchitectureSection.tsx must NOT collide with "archive" in task 3
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/ArchitectureSection.tsx".to_string()]),
        1
    );
    // 2. SecuritySection.tsx must match "Security" / "secure" in task 2 via morphological root ("secur")
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/SecuritySection.tsx".to_string()]),
        1
    );
    // 3. CTASection.tsx must split uppercase acronym boundary ("CTASection" -> ["cta"]) and match task 3
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/CTASection.tsx".to_string()]),
        1
    );

    // 4. Archive plan -> read_active_plan_summary() must return None and to_prompt_block() must omit <live_execution_plan>
    wm.archive_plan().unwrap();
    assert!(wm.read_active_plan_summary().is_none());
    let prompt_block = wm.to_prompt_block();
    assert!(!prompt_block.contains("<live_execution_plan>"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_multi_candidate_reconciliation_and_active_double_step_guard() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_multi_candidate_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(temp_dir.join("src/components")).unwrap();

    // 1. Verify glued conjunction/action prefixes are normalized
    let extracted = WorkingMemory::extract_candidate_files(
        "Build InteractivePlayground.tsx andWorkflowSteps.tsx andCTABanner.tsx",
    );
    assert_eq!(
        extracted,
        vec![
            "InteractivePlayground.tsx".to_string(),
            "WorkflowSteps.tsx".to_string(),
            "CTABanner.tsx".to_string()
        ]
    );

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Build Navbar.tsx and Hero.tsx".to_string(),
        "Build LogoBar.tsx and FeaturesBento.tsx".to_string(),
        "Assemble all sections in src/App.tsx".to_string(),
    ];
    wm.init_plan("Landing Page Plan", &steps).unwrap();

    // Writing only Navbar.tsx must NOT complete Step 1 yet
    fs::write(
        temp_dir.join("src/components/Navbar.tsx"),
        "export const Navbar = () => <nav />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Navbar.tsx".to_string()]),
        0
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 0);

    // Writing Hero.tsx completes Step 1 (since both Navbar.tsx and Hero.tsx now exist) and promotes Step 2 to [>]
    fs::write(
        temp_dir.join("src/components/Hero.tsx"),
        "export const Hero = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Hero.tsx".to_string()]),
        1
    );
    let summary_after_step1 = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary_after_step1.completed_tasks, 1);
    assert_eq!(
        summary_after_step1.active_task.as_deref(),
        Some("Build LogoBar.tsx and FeaturesBento.tsx")
    );

    // Calling update_progress("active", "completed") right after auto-advancement when Step 2's
    // deliverables (LogoBar.tsx, FeaturesBento.tsx) do NOT exist yet must be an idempotent no-op!
    wm.update_progress("active", "completed").unwrap();
    let summary_guarded = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary_guarded.completed_tasks, 1);
    assert_eq!(
        summary_guarded.active_task.as_deref(),
        Some("Build LogoBar.tsx and FeaturesBento.tsx")
    );

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_bipartite_best_owner_and_clause_coverage() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_bipartite_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(temp_dir.join("src/components")).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Build Browser Automation HUD section: simulated headless Chrome viewport".to_string(),
        "Build Comparison table + Architectural Principles + Final CTA: structured vs-table and cards".to_string(),
        "Typecheck, lint, production build, browser screenshots at 1440px and 375px, and visual QA verification".to_string(),
    ];
    wm.init_plan("Minicode Landing", &steps).unwrap();

    // 1. Write BrowserHud.tsx -> completes Step 1
    fs::write(
        temp_dir.join("src/components/BrowserHud.tsx"),
        "export const BrowserHud = () => <div />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/BrowserHud.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_parsed_tasks()[0].status, TaskItemStatus::Completed);
    assert_eq!(wm.read_parsed_tasks()[1].status, TaskItemStatus::InProgress);
    assert_eq!(wm.read_parsed_tasks()[2].status, TaskItemStatus::Pending);

    // 2. Re-editing BrowserHud.tsx must NOT spill over to complete Step 3 ("...browser screenshots...")
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/BrowserHud.tsx".to_string()]),
        0
    );
    assert_eq!(wm.read_parsed_tasks()[2].status, TaskItemStatus::Pending);

    // 3. Writing only Comparison.tsx must NOT complete multi-clause Step 2 yet
    fs::write(
        temp_dir.join("src/components/Comparison.tsx"),
        "export const Comparison = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Comparison.tsx".to_string()]),
        0
    );
    assert_eq!(wm.read_parsed_tasks()[1].status, TaskItemStatus::InProgress);

    // 4. Writing Principles.tsx covers clause 2 of 3; Step 2 still waits for Final CTA
    fs::write(
        temp_dir.join("src/components/Principles.tsx"),
        "export const Principles = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Principles.tsx".to_string()]),
        0
    );
    assert_eq!(wm.read_parsed_tasks()[1].status, TaskItemStatus::InProgress);

    // 5. Writing FinalCta.tsx covers clause 3 of 3 -> all clauses covered, Step 2 completes!
    fs::write(
        temp_dir.join("src/components/FinalCta.tsx"),
        "export const FinalCta = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/FinalCta.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_parsed_tasks()[1].status, TaskItemStatus::Completed);
    assert_eq!(wm.read_parsed_tasks()[2].status, TaskItemStatus::InProgress);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_trailing_dot_and_shorthand_subpath_reconciliation() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_subpath_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(temp_dir.join("src/components/sections")).unwrap();
    fs::create_dir_all(temp_dir.join("src/components/dag")).unwrap();

    let extracted = WorkingMemory::extract_candidate_files(
        "Implement Architecture, Swarm, and Council in src/components/Architecture.tsx, sections/Swarm.tsx, and src/components/Council.tsx.",
    );
    assert_eq!(
        extracted,
        vec![
            "src/components/Architecture.tsx".to_string(),
            "sections/Swarm.tsx".to_string(),
            "src/components/Council.tsx".to_string(),
        ]
    );

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Swarm DAG + council + code intelligence: src/components/dag/SwarmDag.tsx + sections/Swarm.tsx, sections/Council.tsx, sections/CodeIntelligence.tsx.".to_string(),
        "Verify build".to_string(),
    ];
    wm.init_plan("Swiss Spec Landing", &steps).unwrap();

    for rel in [
        "src/components/dag/SwarmDag.tsx",
        "src/components/sections/Swarm.tsx",
        "src/components/sections/Council.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const X = 1;").unwrap();
        assert_eq!(wm.reconcile_workspace_tasks(&[rel.to_string()]), 0);
    }

    // Writing the 4th deliverable completes Step 1 via component-suffix matching (`sections/*.tsx`)
    fs::write(
        temp_dir.join("src/components/sections/CodeIntelligence.tsx"),
        "export const CodeIntelligence = 1;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/sections/CodeIntelligence.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_parsed_tasks()[0].status, TaskItemStatus::Completed);
    assert!(wm
        .format_remaining_tasks_hint()
        .contains("1/2 tasks `[x]` Completed"));
    assert!(wm
        .format_remaining_tasks_hint()
        .contains("#2 \"Verify build\""));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_multi_word_initialism_cta_reconciliation() {
    let tokens = WorkingMemory::split_identifier_stem_tokens(
        "src/components/landing/sections/CallToAction.tsx",
    );
    assert_eq!(
        tokens,
        vec!["call".to_string(), "action".to_string(), "cta".to_string()]
    );

    let temp_dir = std::env::temp_dir().join(format!("minicode_test_cta_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(temp_dir.join("src/components")).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Build Principles ('Built for serious engineering') + Social proof placeholders + CTA + Footer".to_string(),
        "Verify: bun run build, lint, then browser screenshots at 1440/768/390 and fix layout issues".to_string(),
    ];
    wm.init_plan("Landing Page Plan", &steps).unwrap();

    for rel in [
        "src/components/Principles.tsx",
        "src/components/SocialProof.tsx",
        "src/components/CallToAction.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const C = () => <section />;").unwrap();
        assert_eq!(wm.reconcile_workspace_tasks(&[rel.to_string()]), 0);
    }

    // Writing Footer.tsx completes all 4 clauses (Principles + SocialProof + CallToAction ("CTA") + Footer)
    fs::write(
        temp_dir.join("src/components/Footer.tsx"),
        "export const Footer = () => <footer />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Footer.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_parsed_tasks()[0].status, TaskItemStatus::Completed);
    assert!(wm
        .format_remaining_tasks_hint()
        .contains("(already `[x]`: #1 — do not re-update)"));
    assert!(wm
        .format_remaining_tasks_hint()
        .contains("Remaining uncompleted step(s): #2"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_parenthetical_conjunction_in_heading_clauses_reconciliation() {
    let clauses = WorkingMemory::split_heading_clauses(
        "modessection (tui + headless ndjson) + comparisonsection (feature matrix)",
    );
    assert_eq!(
        clauses,
        vec!["modessection".to_string(), "comparisonsection".to_string()]
    );

    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_paren_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(temp_dir.join("src/components")).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "ModesSection (TUI + headless NDJSON) + ComparisonSection (feature matrix)".to_string(),
        "Verify build and responsive screenshots".to_string(),
    ];
    wm.init_plan("Landing Page Plan", &steps).unwrap();

    fs::write(
        temp_dir.join("src/components/ModesSection.tsx"),
        "export const ModesSection = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/ModesSection.tsx".to_string()]),
        0
    );

    fs::write(
        temp_dir.join("src/components/ComparisonSection.tsx"),
        "export const ComparisonSection = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/ComparisonSection.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_parsed_tasks()[0].status, TaskItemStatus::Completed);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_reconcile_section_wrapper_does_not_prematurely_complete_multi_clause_step() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_minitest48_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(temp_dir.join("src/components/sections")).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Build Hero section + interactive terminal".to_string(),
        "Build Philosophy, Architecture, and Swarm sections".to_string(),
    ];
    wm.init_plan("Minicode Landing", &steps).unwrap();

    // Generic layout wrapper `Section.tsx` + `TerminalSession.tsx` must NOT complete Step 1 before `Hero.tsx` exists!
    fs::write(
        temp_dir.join("src/components/Section.tsx"),
        "export const Section = () => <section />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/sections/TerminalSession.tsx"),
        "export const TerminalSession = () => <div />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&[
            "src/components/Section.tsx".to_string(),
            "src/components/sections/TerminalSession.tsx".to_string(),
        ]),
        0
    );
    assert_ne!(wm.read_parsed_tasks()[0].status, TaskItemStatus::Completed);

    // Once Hero.tsx is created, Step 1 completes!
    fs::write(
        temp_dir.join("src/components/sections/Hero.tsx"),
        "export const Hero = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/sections/Hero.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_parsed_tasks()[0].status, TaskItemStatus::Completed);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest49_codeblock_does_not_prematurely_complete_code_intelligence() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_minitest49_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(temp_dir.join("src/components/landing")).unwrap();
    fs::create_dir_all(temp_dir.join("src/sections")).unwrap();
    fs::create_dir_all(temp_dir.join("minikit_docs/core")).unwrap();
    fs::write(
        temp_dir.join("minikit_docs/core/prd.md"),
        "- [ ] **Objective 1**: Core system functionality.\n",
    )
    .unwrap();
    fs::write(
        temp_dir.join("minikit_docs/core/implementation.md"),
        "---\nstatus: active\n---\n",
    )
    .unwrap();
    fs::write(
        temp_dir.join("minikit_docs/core/design.md"),
        "---\ncolors:\n  primary: \"#5e6ad2\"\n---\n| `{colors.primary}` | `#5e6ad2` |\n",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/index.css"),
        "@theme {\n  --color-signal: #1fd1c0;\n  --color-signal-dim: #148a7e;\n}\n",
    )
    .unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Shared primitives: Section wrapper, Eyebrow, GlowCard, Reveal motion wrapper, Button"
            .to_string(),
        "Code Intelligence section: AST/CodeGraph/PageRank visualization".to_string(),
    ];
    wm.init_plan("minicode landing page", &steps).unwrap();

    // Creating `src/components/landing/CodeBlock.tsx` during Step 1 must NOT complete Step 2 ("Code Intelligence section")!
    fs::write(
        temp_dir.join("src/components/landing/CodeBlock.tsx"),
        "export const CodeBlock = () => <pre />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/landing/CodeBlock.tsx".to_string()]),
        0
    );
    assert_ne!(wm.read_parsed_tasks()[1].status, TaskItemStatus::Completed);

    // Creating `src/sections/CodeIntelligence.tsx` completes Step 2!
    fs::write(
        temp_dir.join("src/sections/CodeIntelligence.tsx"),
        "export const CodeIntelligence = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/sections/CodeIntelligence.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_parsed_tasks()[1].status, TaskItemStatus::Completed);

    // Completing Step 1 brings plan to 100% and automatically syncs prd.md and implementation.md!
    wm.update_progress("1", "Completed").unwrap();
    let prd = fs::read_to_string(temp_dir.join("minikit_docs/core/prd.md")).unwrap();
    assert!(prd.contains("- [x] **Objective 1**"));
    let impl_doc =
        fs::read_to_string(temp_dir.join("minikit_docs/core/implementation.md")).unwrap();
    assert!(impl_doc.contains("status: completed"));
    let design_doc = fs::read_to_string(temp_dir.join("minikit_docs/core/design.md")).unwrap();
    assert!(design_doc.contains("primary: \"#1fd1c0\""));
    assert!(design_doc.contains("| `{colors.primary}` | `#1fd1c0` |"));

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_multi_step_hierarchical_substeps_persistence_and_auto_promotion() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_hier_substeps_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "1. Design tokens & shared primitives".to_string(),
        "2. Create Hero section".to_string(),
        "3. Build Code Intelligence section".to_string(),
    ];
    let step_substeps = vec![
        Vec::new(),
        vec![
            "Define Hero layout grid and typography hierarchy".to_string(),
            "Build interactive TerminalPreview sub-component".to_string(),
            "Assemble HeroSection.tsx with CTA buttons and badge".to_string(),
        ],
        vec![
            "Build CodeGraph visualization".to_string(),
            "Wire AST zoom controls".to_string(),
        ],
    ];

    wm.init_two_tier_plan_with_substeps("Landing Page", &steps, &step_substeps, None, None)
        .unwrap();

    // Verify all nested substeps for Steps 2 and 3 are persisted indented in canonical todo.md
    let todo_initial = fs::read_to_string(wm.canonical_todo_path()).unwrap();
    assert!(todo_initial.contains("  - [ ] Define Hero layout grid and typography hierarchy"));
    assert!(todo_initial.contains("  - [ ] Wire AST zoom controls"));
    assert!(!wm.is_tactical_subplan());

    // Completing Step 1 promotes Step 2 to `[>]` AND auto-activates Step 2's Tier-2 tactical sub-plan!
    wm.update_progress("1", "completed").unwrap();
    assert!(wm.is_tactical_subplan());
    let summary_step2 = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary_step2.tasks.len(), 3);
    assert_eq!(summary_step2.tasks[0].status, TaskItemStatus::Completed);
    assert_eq!(summary_step2.tasks[1].status, TaskItemStatus::InProgress);
    assert_eq!(summary_step2.tasks[1].sub_tasks.len(), 3);
    assert_eq!(
        summary_step2.active_task.as_deref(),
        Some("Define Hero layout grid and typography hierarchy")
    );

    // Completing all 3 substeps of Step 2 marks Step 2 `[x]`, promotes Step 3 to `[>]`,
    // and auto-activates Step 3's 2 substeps!
    wm.update_progress("1", "completed").unwrap();
    wm.update_progress("2", "completed").unwrap();
    wm.update_progress("3", "completed").unwrap();

    let summary_step3 = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary_step3.tasks[0].status, TaskItemStatus::Completed);
    assert_eq!(summary_step3.tasks[1].status, TaskItemStatus::Completed);
    assert_eq!(summary_step3.tasks[2].status, TaskItemStatus::InProgress);
    assert_eq!(summary_step3.tasks[2].sub_tasks.len(), 2);
    assert_eq!(
        summary_step3.active_task.as_deref(),
        Some("Build CodeGraph visualization")
    );

    // Completing Step 3's 2 substeps finishes the entire two-tier roadmap (100%)
    wm.update_progress("1", "completed").unwrap();
    wm.update_progress("2", "completed").unwrap();
    assert!(wm.is_all_project_tasks_completed());

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest50_colon_enumerated_sections_reconcile_and_sdd_archive() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_minitest50_reconcile_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(temp_dir.join("src/components/sections")).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Core engineering sections: Philosophy (zero bloat, pure Rust), Performance (benchmarks & resource gauges), Swarm (DAG & Boomerang visualization)".to_string(),
        "Context & safety sections: ContextArchitecture (CodeGraph + PageRank + Memory tiers), SecurityGovernance (sandbox, verification barrier, SkillSpector)".to_string(),
    ];
    wm.init_plan("minicode Premium Landing Page", &steps)
        .unwrap();

    // Creating 2 of 3 sections for Step 1 must NOT prematurely mark Step 1 completed
    fs::write(
        temp_dir.join("src/components/sections/Philosophy.tsx"),
        "export const Philosophy = () => <section />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/sections/Performance.tsx"),
        "export const Performance = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&[
            "src/components/sections/Philosophy.tsx".to_string(),
            "src/components/sections/Performance.tsx".to_string(),
        ]),
        0
    );
    assert_eq!(wm.read_parsed_tasks()[0].status, TaskItemStatus::InProgress);

    // Creating the 3rd section (`Swarm.tsx`) completes Step 1 and promotes Step 2 to `[>]`!
    fs::write(
        temp_dir.join("src/components/sections/Swarm.tsx"),
        "export const Swarm = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/sections/Swarm.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_parsed_tasks()[0].status, TaskItemStatus::Completed);
    assert_eq!(wm.read_parsed_tasks()[1].status, TaskItemStatus::InProgress);

    // Creating CamelCase/PascalCase `ContextArchitecture.tsx` + `SecurityGovernance.tsx` completes Step 2!
    fs::write(
        temp_dir.join("src/components/sections/ContextArchitecture.tsx"),
        "export const ContextArchitecture = () => <section />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/sections/SecurityGovernance.tsx"),
        "export const SecurityGovernance = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&[
            "src/components/sections/ContextArchitecture.tsx".to_string(),
            "src/components/sections/SecurityGovernance.tsx".to_string(),
        ]),
        1
    );
    assert!(wm.is_all_project_tasks_completed());

    // Simulate SDD spec directory `.minicode/sdd/task-plan-minicode-premium-landing-page/progress.md`
    let sdd_slug_dir = temp_dir
        .join(".minicode")
        .join("sdd")
        .join("task-plan-minicode-premium-landing-page");
    fs::create_dir_all(&sdd_slug_dir).unwrap();
    fs::write(
        sdd_slug_dir.join("progress.md"),
        "## Verification Evidence\nAll 4 gates passed.",
    )
    .unwrap();

    let started = std::time::SystemTime::now();
    let archive_path = wm.archive_plan().unwrap().expect("archived plan path");
    let archived_md = fs::read_to_string(&archive_path).unwrap();
    assert!(archived_md.contains("All 4 gates passed."));
    assert!(
        !temp_dir.join(".minicode").join("sdd").exists(),
        ".minicode/sdd must be folded into archive and cleaned up on archive_plan"
    );
    assert_eq!(
        wm.recent_or_active_plan_goal(Some(started)).as_deref(),
        Some("minicode Premium Landing Page")
    );
    // Future turn starting after archive must NOT reuse stale archived plan goal
    let future_turn = started + std::time::Duration::from_secs(10);
    assert_eq!(wm.recent_or_active_plan_goal(Some(future_turn)), None);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest50_verbatim_7_step_reconciliation_and_cross_task_ownership() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_minitest50_full_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(temp_dir.join("src/data")).unwrap();
    fs::create_dir_all(temp_dir.join("src/components/ui")).unwrap();
    fs::create_dir_all(temp_dir.join("src/components/hero")).unwrap();
    fs::create_dir_all(temp_dir.join("src/components/sections")).unwrap();
    fs::create_dir_all(temp_dir.join("minikit_docs/core")).unwrap();
    fs::write(
        temp_dir.join("minikit_docs/core/architecture.md"),
        "# Architecture",
    )
    .unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Design tokens + base stylesheet: Rewrite src/index.css with @theme tokens (canvas/surface/ink/signal), hairline grid, radial glows, keyframes for scan/cursor/dash-flow, reduced-motion guards.".to_string(),
        "Data modules + shared UI primitives: src/data/tools.ts, terminalScript.ts, comparison.ts, council.ts; src/components/ui/Section.tsx, CodeBlock.tsx, GithubIcon.tsx, StatTile.tsx.".to_string(),
        "Nav + hero + interactive terminal: src/components/Navbar.tsx (fixed glass, inline SVG brand mark), src/components/hero/Hero.tsx + hero/TerminalPreview.tsx with timer-driven agent loop.".to_string(),
        "Core engineering sections: Philosophy (evidence vs assertion), Performance (interactive footprint calculator), Swarm (animated SVG DAG).".to_string(),
        "Intelligence + safety sections: Council (multi-model synthesis), CodeIntelligence (AST/CodeGraph/PageRank), Security (layered), BrowserHud (animated cursor + ARIA).".to_string(),
        "Ecosystem + product sections: ToolsGrid (10 categories), DualMode (TUI vs NDJSON tabs), Comparison table, Principles grid, SocialProof (live GitHub stars), FinalCta, Footer.".to_string(),
        "Verify: build, lint, browser QA: bun run build, bun run lint, dev server + headless screenshots at 1440/768/390 widths; fix overflow/lint/type errors.".to_string(),
    ];
    wm.init_plan("minicode premium landing page", &steps)
        .unwrap();

    // Step 1
    fs::write(temp_dir.join("src/index.css"), "@theme { --bg: #06080B; }").unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/index.css".to_string()]),
        1
    );

    // Step 2
    for rel in [
        "src/data/tools.ts",
        "src/data/terminalScript.ts",
        "src/data/comparison.ts",
        "src/data/council.ts",
        "src/components/ui/Section.tsx",
        "src/components/ui/CodeBlock.tsx",
        "src/components/ui/GithubIcon.tsx",
        "src/components/ui/StatTile.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const x = 1;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 2);

    // Step 3
    for rel in [
        "src/components/Navbar.tsx",
        "src/components/hero/TerminalPreview.tsx",
        "src/components/hero/Hero.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const x = 1;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 3);

    // Step 4 (extensionless colon-enumerated deliverables)
    for rel in [
        "src/components/sections/Philosophy.tsx",
        "src/components/sections/Performance.tsx",
        "src/components/sections/Swarm.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const x = 1;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 4);

    // Double-step guard on extensionless Step 5 (`Council, CodeIntelligence, Security, BrowserHud`):
    // calling update_progress("active", "completed") before Step 5's files exist must be an idempotent no-op!
    wm.update_progress("active", "completed").unwrap();
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 4);
    assert_eq!(wm.read_parsed_tasks()[4].status, TaskItemStatus::InProgress);

    // Step 5
    for rel in [
        "src/components/sections/Council.tsx",
        "src/components/sections/CodeIntelligence.tsx",
        "src/components/sections/Security.tsx",
        "src/components/sections/BrowserHud.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const x = 1;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 5);

    // Step 6 cross-task ownership guard: writing 5 of 7 sections (`DualMode`, `Principles`, `SocialProof`,
    // `FinalCta`, `Footer`) BEFORE `ToolsGrid.tsx` and `Comparison.tsx` must NOT complete Step 6
    // even though `src/data/tools.ts` and `src/data/comparison.ts` from Step 2 exist!
    for rel in [
        "src/components/sections/DualMode.tsx",
        "src/components/sections/Principles.tsx",
        "src/components/sections/SocialProof.tsx",
        "src/components/sections/FinalCta.tsx",
        "src/components/Footer.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const x = 1;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 5);

    // Writing `ToolsGrid.tsx` and `Comparison.tsx` now completes Step 6 (6/7)!
    for rel in [
        "src/components/sections/ToolsGrid.tsx",
        "src/components/sections/Comparison.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const x = 1;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 6);
    assert_eq!(wm.read_parsed_tasks()[6].status, TaskItemStatus::InProgress);

    // Step 7 (`Verify: ...`) must NOT auto-complete via reconcile_verification_tasks()
    assert_eq!(wm.reconcile_verification_tasks(), 0);
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 6);

    // Explicit update_progress(step="active", status="completed") completes Step 7 (7/7 = 100%)
    wm.update_progress("active", "completed").unwrap();
    assert!(wm.is_all_project_tasks_completed());

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest50_out_of_order_update_progress_reconciles_verified_earlier_steps() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_minitest50_ooo_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(temp_dir.join("src/data")).unwrap();
    fs::create_dir_all(temp_dir.join("src/components/ui")).unwrap();
    fs::create_dir_all(temp_dir.join("src/components/hero")).unwrap();
    fs::create_dir_all(temp_dir.join("src/components/sections")).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Design tokens + base stylesheet: Rewrite src/index.css with @theme tokens.".to_string(),
        "Data modules + shared UI primitives: src/data/tools.ts, terminalScript.ts, comparison.ts, council.ts; src/components/ui/Section.tsx, CodeBlock.tsx, GithubIcon.tsx, StatTile.tsx.".to_string(),
        "Nav + hero + interactive terminal: src/components/Navbar.tsx, src/components/hero/Hero.tsx + hero/TerminalPreview.tsx.".to_string(),
        "Core engineering sections: Philosophy (evidence vs assertion), Performance (interactive footprint calculator), Swarm (animated SVG DAG).".to_string(),
        "Intelligence + safety sections: Council (multi-model synthesis), CodeIntelligence (AST/CodeGraph/PageRank), Security (layered), BrowserHud (animated cursor + ARIA).".to_string(),
        "Ecosystem + product sections: ToolsGrid (10 categories), DualMode (TUI vs NDJSON tabs), Comparison table, Principles grid, SocialProof (live GitHub stars), FinalCta, Footer.".to_string(),
        "Verify: build, lint, browser QA: bun run build, bun run lint, dev server + headless screenshots.".to_string(),
    ];
    wm.init_plan("minicode premium landing page", &steps)
        .unwrap();

    // Mark Steps 1..=3 completed and write all files EXCEPT Step 5's `BrowserHud.tsx` directly on disk
    for rel in [
        "src/index.css",
        "src/data/tools.ts",
        "src/data/terminalScript.ts",
        "src/data/comparison.ts",
        "src/data/council.ts",
        "src/components/ui/Section.tsx",
        "src/components/ui/CodeBlock.tsx",
        "src/components/ui/GithubIcon.tsx",
        "src/components/ui/StatTile.tsx",
        "src/components/Navbar.tsx",
        "src/components/hero/TerminalPreview.tsx",
        "src/components/hero/Hero.tsx",
        "src/components/sections/Philosophy.tsx",
        "src/components/sections/Performance.tsx",
        "src/components/sections/Swarm.tsx",
        "src/components/sections/Council.tsx",
        "src/components/sections/CodeIntelligence.tsx",
        "src/components/sections/Security.tsx",
        "src/components/sections/ToolsGrid.tsx",
        "src/components/sections/DualMode.tsx",
        "src/components/sections/Comparison.tsx",
        "src/components/sections/Principles.tsx",
        "src/components/sections/SocialProof.tsx",
        "src/components/sections/FinalCta.tsx",
        "src/components/Footer.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const x = 1;").unwrap();
    }
    wm.update_progress("1", "completed").unwrap();
    wm.update_progress("2", "completed").unwrap();
    wm.update_progress("3", "completed").unwrap();
    assert_eq!(wm.read_parsed_tasks()[3].status, TaskItemStatus::InProgress);

    // Calling out-of-order `update_progress` on Step 7 when Step 4 & Step 6 are complete on disk
    // and Step 5 is still missing `BrowserHud.tsx`:
    // -> Step 4 & Step 6 auto-complete, Step 7 completes, and Step 5 wraps around to `[>] InProgress`!
    wm.update_progress("Verify: build, lint, browser QA", "Completed")
        .unwrap();
    let tasks_partial = wm.read_parsed_tasks();
    assert_eq!(tasks_partial[3].status, TaskItemStatus::Completed);
    assert_eq!(tasks_partial[4].status, TaskItemStatus::InProgress);
    assert_eq!(tasks_partial[5].status, TaskItemStatus::Completed);
    assert_eq!(tasks_partial[6].status, TaskItemStatus::Completed);

    // Now write `BrowserHud.tsx` and run `reconcile_verification_tasks()` -> 7/7 (100%) completed!
    fs::write(
        temp_dir.join("src/components/sections/BrowserHud.tsx"),
        "export const BrowserHud = () => <section />;",
    )
    .unwrap();
    assert_eq!(wm.reconcile_verification_tasks(), 1);
    assert!(wm.is_all_project_tasks_completed());

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest52_subplan_live_incremental_reconciliation_and_root_goal_archive() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_minitest52_full_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(temp_dir.join("src/components/sections")).unwrap();
    fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();

    // Materialize genuine `react-vite` starter template files (including `Navbar.tsx` and `HomePage.tsx`)
    let react_vite = crate::tools::minikit::stacks::builtin::builtin_stacks()
        .into_iter()
        .find(|s| s.name == "react-vite")
        .expect("react-vite builtin stack");
    for sf in &react_vite.files {
        let dst = temp_dir.join(&sf.path);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&dst, &sf.content).unwrap();
    }
    fs::write(
        temp_dir.join("minikit.json"),
        r#"{"name":"minitest52","stack":"react-vite"}"#,
    )
    .unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Design tokens & global CSS: Rewrite src/index.css with dark obsidian tokens, fonts, glow/grid utilities, reduced-motion handling".to_string(),
        "Navbar + Footer shell: src/components/Navbar.tsx rewrite + src/components/Footer.tsx new; compact top nav with brand, section links, GitHub CTA".to_string(),
        "Hero + interactive terminal preview: Hero.tsx (H1 <=2 lines, CTA bar) + TerminalPreview.tsx (interactive tabs: plan/inspect/verify, streaming log lines)".to_string(),
        "Philosophy + Architecture bento: Philosophy.tsx, Architecture.tsx (asymmetric 8+4 bento with perf telemetry), Swarm.tsx (DAG), Council.tsx".to_string(),
        "Intelligence, security, browser, tools bento: CodeIntelligence.tsx, Security.tsx, BrowserAutomation.tsx (HUD mock), ToolsCatalogue.tsx (live search/filter micro-tool, data-ui=true)".to_string(),
        "Interfaces, comparison, principles, community, CTA: Interfaces.tsx (TUI vs headless stream), Comparison.tsx, Principles.tsx, Community.tsx (placeholders only), CTA.tsx".to_string(),
        "Compose HomePage: Wire all sections into src/pages/HomePage.tsx in order; ensure <h1> once, headings hierarchy, aria labels".to_string(),
        "Verify build + 3-viewport browser check: Run bun run build (tsc+vite) and dev server; screenshot 1440/768/390 and check hero fold, equal margins, no overflow".to_string(),
    ];
    let step_substeps = vec![
        vec![
            "Reset theme variables to obsidian palette".to_string(),
            "Add JetBrains Mono/Space Grotesk/Inter imports".to_string(),
            "Add grid-pattern + glow utility classes (radial-gradient, no blur filters)"
                .to_string(),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    let milestones = vec![
        "Foundation: design tokens, typography, layout shell".to_string(),
        "Core narrative sections: hero, philosophy, architecture, swarm, council".to_string(),
        "Intelligence + security + browser + tools bento".to_string(),
        "Interfaces, comparison, principles, community, CTA, footer".to_string(),
        "Responsive + browser verification across 3 viewports".to_string(),
    ];

    wm.init_two_tier_plan_with_substeps(
        "minicode landing page",
        &steps,
        &step_substeps,
        None,
        Some(&milestones),
    )
    .unwrap();
    assert!(wm.is_tactical_subplan());

    // Step 1: Writing `src/index.css` reconciles all 3 intra-file substeps, completes Macro Step 1 (1/8),
    // and transitions `task_plan.md` from `Mode: subplan` back to `Mode: direct`!
    fs::write(
        temp_dir.join("src/index.css"),
        "@import url('https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@600&family=Inter:wght@400&family=JetBrains+Mono:wght@400&display=swap');\n\
         @import \"tailwindcss\";\n\
         @theme { --color-background: var(--background); }\n\
         :root { --background: #08090d; }\n\
         /* no blur filters */\n\
         .glow-field { background: radial-gradient(circle, rgba(34,211,238,0.13), transparent 70%); }\n\
         .grid-field { background-size: 56px 56px; }\n",
    )
    .unwrap();
    assert!(wm.reconcile_workspace_tasks(&["src/index.css".to_string()]) >= 1);
    assert!(
        !wm.is_tactical_subplan(),
        "task_plan.md must transition back to Mode: direct after Step 1's subplan finishes"
    );
    let summary_after_1 = wm.read_active_plan_summary().unwrap();
    assert_eq!(summary_after_1.total_tasks, 8);
    assert_eq!(summary_after_1.completed_tasks, 1);

    // Step 2: Writing `Footer.tsx` BEFORE rewriting starter `Navbar.tsx` must NOT complete Step 2!
    fs::write(
        temp_dir.join("src/components/Footer.tsx"),
        "export function Footer() { return <footer>minicode</footer>; }",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Footer.tsx".to_string()]),
        0,
        "Unmodified scaffold Navbar.tsx must not satisfy Step 2"
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 1);

    // Rewriting `src/components/Navbar.tsx` completes Step 2 (2/8) in real time!
    fs::write(
        temp_dir.join("src/components/Navbar.tsx"),
        "export function Navbar() { return <header>minicode</header>; }",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/components/Navbar.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 2);

    // Step 3: `Hero.tsx` + `TerminalPreview.tsx` -> 3/8
    for rel in [
        "src/components/sections/Hero.tsx",
        "src/components/sections/TerminalPreview.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const X = () => <section />;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 3);

    // Step 4: `Philosophy.tsx`, `Architecture.tsx`, `Swarm.tsx`, `Council.tsx` -> 4/8
    for rel in [
        "src/components/sections/Philosophy.tsx",
        "src/components/sections/Architecture.tsx",
        "src/components/sections/Swarm.tsx",
        "src/components/sections/Council.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const X = () => <section />;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 4);

    // Step 5: `CodeIntelligence.tsx`, `Security.tsx`, `BrowserAutomation.tsx`, `ToolsCatalogue.tsx` -> 5/8
    for rel in [
        "src/components/sections/CodeIntelligence.tsx",
        "src/components/sections/Security.tsx",
        "src/components/sections/BrowserAutomation.tsx",
        "src/components/sections/ToolsCatalogue.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const X = () => <section />;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 5);

    // Step 6: Mixed-case `src/components/sections/Cta.tsx` must match `CTA.tsx` case-insensitively on Linux -> 6/8!
    // Meanwhile `src/pages/HomePage.tsx` is still the unmodified starter scaffold, so Step 7 stays `[>]`!
    for rel in [
        "src/components/sections/Interfaces.tsx",
        "src/components/sections/Comparison.tsx",
        "src/components/sections/Principles.tsx",
        "src/components/sections/Community.tsx",
        "src/components/sections/Cta.tsx",
    ] {
        fs::write(temp_dir.join(rel), "export const X = () => <section />;").unwrap();
        wm.reconcile_workspace_tasks(&[rel.to_string()]);
    }
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 6);

    // Step 7: Rewriting `src/pages/HomePage.tsx` completes Step 7 -> 7/8!
    fs::write(
        temp_dir.join("src/pages/HomePage.tsx"),
        "export default function HomePage() { return <main><h1>minicode</h1></main>; }",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/pages/HomePage.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 7);

    // Step 8 (`Verify build + 3-viewport browser check...`) is a non-auto-verifiable verification gate:
    assert!(!WorkingMemory::task_declares_verifiable_deliverables(
        &steps[7]
    ));
    fs::write(
        temp_dir.join("src/hooks/useReveal.ts"),
        "export function useReveal() {}",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/hooks/useReveal.ts".to_string()]),
        0,
        "Editing a helper hook during browser verification must not prematurely complete Step 8"
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 7);

    // Completing Step 8 via `update_progress` reaches 8/8 (never reverting to 3/3!),
    // preserves root goal for git commit, and archives all 8 roadmap tasks!
    wm.update_progress("active", "completed").unwrap();
    let final_summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(final_summary.total_tasks, 8);
    assert_eq!(final_summary.completed_tasks, 8);
    assert!(wm.is_all_project_tasks_completed());

    let started = std::time::SystemTime::now() - std::time::Duration::from_secs(5);
    let archive_path = wm.archive_plan().unwrap().expect("archived plan path");
    let archived_md = fs::read_to_string(&archive_path).unwrap();
    assert!(archived_md.contains("# Task Plan: minicode landing page"));
    assert!(archived_md.contains("8. [x] Verify build + 3-viewport browser check"));
    let todo_final = fs::read_to_string(wm.canonical_todo_path()).unwrap();
    assert!(todo_final.contains("- [x] Verify build + 3-viewport browser check"));
    assert_eq!(
        wm.recent_or_active_plan_goal(Some(started)).as_deref(),
        Some("minicode landing page"),
        "Auto-git-commit must use the root roadmap title rather than Step 1's substep title"
    );

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_subplan_cross_macro_progression_auto_closes_abstract_prose_substeps() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_subplan_cross_macro_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(temp_dir.join("src/components")).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Global CSS: Rewrite src/index.css".to_string(),
        "Build Navbar: Create src/components/Navbar.tsx".to_string(),
    ];
    // Abstract prose substep with zero literal identifier overlap in `src/index.css`
    let step_substeps = vec![
        vec!["Calibrate visual contrast ratios".to_string()],
        Vec::new(),
    ];
    wm.init_two_tier_plan_with_substeps("Demo App", &steps, &step_substeps, None, None)
        .unwrap();
    assert!(wm.is_tactical_subplan());

    // Step 1's target file is written on disk, though the abstract prose substep has no literal token match
    fs::write(temp_dir.join("src/index.css"), ":root { --bg: #000; }").unwrap();
    wm.reconcile_workspace_tasks(&["src/index.css".to_string()]);
    assert!(wm.is_tactical_subplan());

    // Moving on to Step 2 (`src/components/Navbar.tsx`) auto-closes Step 1's remaining intra-file prose substep,
    // transitions `task_plan.md` to `Mode: direct`, AND reconciles Step 2 in the same pass!
    fs::write(
        temp_dir.join("src/components/Navbar.tsx"),
        "export const Navbar = () => <nav />;",
    )
    .unwrap();
    assert!(wm.reconcile_workspace_tasks(&["src/components/Navbar.tsx".to_string()]) >= 2);
    assert!(!wm.is_tactical_subplan());
    assert!(wm.is_all_project_tasks_completed());

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest53_step_by_step_update_progress_and_compound_verification_guard() {
    use crate::context::memory::working_memory::ActiveStepReadiness;

    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_minitest53_progress_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(temp_dir.join("src/components/sections")).unwrap();
    fs::create_dir_all(temp_dir.join("src/pages")).unwrap();
    fs::create_dir_all(temp_dir.join("src/data")).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Establish theme tokens and base styles: Rewrite src/index.css: dark graphite theme tokens, amber accent.".to_string(),
        "Build shared chrome: Navbar and Footer: Replace src/components/Navbar.tsx with brand bar. Add src/components/Footer.tsx.".to_string(),
        "Build swarm DAG and council sections: SwarmSection.tsx (DAG with goal -> parallel tasks -> verify -> merge), CouncilSection.tsx.".to_string(),
        "Compose page, verify build and responsive geometry: Compose all sections in src/pages/HomePage.tsx; run bun run build and bun run lint; start dev server and probe layout at 1440/768/390 with browser_screenshot; fix overflow/overlap issues.".to_string(),
    ];

    assert!(!WorkingMemory::step_requires_runtime_verification(
        &steps[0]
    ));
    assert!(!WorkingMemory::step_requires_runtime_verification(
        &steps[1]
    ));
    // Parenthetical `verify` inside `(DAG with goal -> parallel tasks -> verify -> merge)` must NOT false-trigger!
    assert!(!WorkingMemory::step_requires_runtime_verification(
        &steps[2]
    ));
    // Compound Step 4 (deliverable + build/lint/browser verification) MUST require runtime verification!
    assert!(WorkingMemory::step_requires_runtime_verification(&steps[3]));

    wm.init_two_tier_plan("minicode landing page", &steps, None, None)
        .unwrap();

    // 1. Writing Step 1's file (`src/index.css`) keeps Step 1 `[>]` during mid-turn transition check
    //    and marks `ActiveStepReadiness::DeliverablesReady` so the agent calls `update_progress` explicitly!
    fs::write(temp_dir.join("src/index.css"), ":root { --bg: #0b0c0e; }").unwrap();
    assert_eq!(
        wm.reconcile_preceding_tasks_on_step_transition(&["src/index.css".to_string()]),
        0
    );
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 0);

    // 2. Suppose the agent skips `update_progress` on Step 1 and moves directly to Step 2 (`Navbar.tsx`):
    //    `reconcile_preceding_tasks_on_step_transition` catches up Step 1 -> `[x]` while keeping Step 2 `[>]`!
    fs::write(
        temp_dir.join("src/components/Navbar.tsx"),
        "export const Navbar = () => <header />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_preceding_tasks_on_step_transition(&["src/components/Navbar.tsx".to_string()]),
        1
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 1);
    // Step 2 still needs Footer.tsx, so readiness is InProgress
    assert_eq!(wm.active_step_readiness(), ActiveStepReadiness::InProgress);

    // Writing Footer.tsx makes Step 2 DeliverablesReady, then agent calls explicit `update_progress`
    fs::write(
        temp_dir.join("src/components/Footer.tsx"),
        "export const Footer = () => <footer />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_preceding_tasks_on_step_transition(&["src/components/Footer.tsx".to_string()]),
        0
    );
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady
    );
    wm.update_progress("active", "completed").unwrap();
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 2);

    // 3. Complete Step 3 explicitly
    fs::write(
        temp_dir.join("src/components/sections/SwarmSection.tsx"),
        "export const SwarmSection = () => <section />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/sections/CouncilSection.tsx"),
        "export const CouncilSection = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady
    );
    wm.update_progress("active", "completed").unwrap();
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 3);

    // 4. Compound Step 4: writing `src/pages/HomePage.tsx` MUST NOT prematurely complete Step 4!
    fs::write(
        temp_dir.join("src/pages/HomePage.tsx"),
        "export const HomePage = () => <main />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_preceding_tasks_on_step_transition(&["src/pages/HomePage.tsx".to_string()]),
        0
    );
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/pages/HomePage.tsx".to_string()]),
        0
    );
    assert_eq!(wm.reconcile_verification_tasks(), 0);
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::AwaitingVerification
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 3);

    // Once build/lint/browser QA finishes, explicit `update_progress` completes Step 4 (`4/4`)!
    wm.update_progress("active", "completed").unwrap();
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 4);
    assert!(wm.is_all_project_tasks_completed());

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest54_compound_verification_subplan_decomposition_and_stage_transitions() {
    let temp_dir = std::env::temp_dir().join("minicode_wm_minitest54_compound_verify");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src/sections")).unwrap();

    // 1. Compound pure verification step decomposes into 3 Tier-2 substeps, whereas file-deliverable
    //    or static-only steps return empty substeps.
    let minitest54_step9 = "Verify: build, lint, 3-viewport browser check: bun run build + lint, start dev server, browser screenshot at 1440/768/390, fix overflow and lopsided sections";
    let auto_subs = WorkingMemory::synthesize_compound_verification_substeps(minitest54_step9);
    assert_eq!(auto_subs.len(), 3);
    assert!(WorkingMemory::synthesize_compound_verification_substeps(
        "Compose all sections in src/pages/HomePage.tsx; run bun run build and browser QA"
    )
    .is_empty());
    assert!(
        WorkingMemory::synthesize_compound_verification_substeps("Run cargo test and clippy")
            .is_empty()
    );

    let wm = WorkingMemory::new(&temp_dir);
    let steps: Vec<String> = vec![
        "1. Design tokens in src/index.css".to_string(),
        "2. Build Navbar in src/Navbar.tsx".to_string(),
        "3. Build Hero in src/sections/Hero.tsx".to_string(),
        "4. Build Swarm in src/sections/Swarm.tsx".to_string(),
        "5. Build Council in src/sections/Council.tsx".to_string(),
        "6. Build Memory in src/sections/Memory.tsx".to_string(),
        "7. Build Benchmarks in src/sections/Benchmarks.tsx".to_string(),
        "8. Compose HomePage in src/HomePage.tsx".to_string(),
        format!("9. {}", minitest54_step9),
    ];
    let mut step_substeps: Vec<Vec<String>> = vec![Vec::new(); 9];
    step_substeps[8] = auto_subs;

    wm.init_two_tier_plan_with_substeps("Landing Page", &steps, &step_substeps, None, None)
        .unwrap();

    // 2. Full 9-step list is preserved in `read_active_plan_summary` (no double-windowing truncation)
    let initial_summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(initial_summary.tasks.len(), 9);
    assert_eq!(initial_summary.total_tasks, 9);

    // Advance Steps 1..=8 to `[x]`
    for _ in 0..8 {
        wm.update_progress("active", "completed").unwrap();
    }

    // 3. Step 9 automatically activates `Mode: subplan` with 3 nested sub_tasks under `tasks[8]`
    assert!(wm.is_tactical_subplan());
    let s9_summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(s9_summary.tasks.len(), 9);
    assert_eq!(s9_summary.tasks[8].sub_tasks.len(), 3);
    assert_eq!(s9_summary.total_tasks, 3);
    assert_eq!(s9_summary.completed_tasks, 0);

    // 4. Starting dev server via `minitask` transitions Stage 1 (static build/lint) -> Stage 2 (desktop QA)
    assert_eq!(
        wm.reconcile_verification_substep_on_tool_transition(
            "minitask",
            &serde_json::json!({"action": "start", "command": "bun run dev -- --port 5180"}),
            true,
        ),
        1
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 1);

    // Patching a component during responsive QA must NOT prematurely auto-close verification substeps
    fs::write(
        temp_dir.join("src/sections/Hero.tsx"),
        "export const Hero = () => <section className=\"min-w-0\" />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_workspace_tasks(&["src/sections/Hero.tsx".to_string()]),
        0
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 1);

    // 5. Switching to tablet/mobile viewport transitions Stage 2 (desktop QA) -> Stage 3 (responsive QA)
    assert_eq!(
        wm.reconcile_verification_substep_on_tool_transition(
            "browser_emulate",
            &serde_json::json!({"viewport": "tablet"}),
            true,
        ),
        1
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 2);

    // 6. Calling `update_progress` with parent macro step #9 title cleanly completes remaining substep,
    //    advances `todo.md` to 9/9 `[x]`, syncs `task_plan.md` back to `Mode: direct`, and enables archival!
    wm.update_progress(
        "9. Verify: build, lint, 3-viewport browser check",
        "completed",
    )
    .unwrap();
    assert!(!wm.is_tactical_subplan());
    assert!(wm.is_all_project_tasks_completed());
    let final_summary = wm.read_active_plan_summary().unwrap();
    assert_eq!(final_summary.total_tasks, 9);
    assert_eq!(final_summary.completed_tasks, 9);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest54_uncompleted_first_matching_in_update_progress() {
    let temp_dir = std::env::temp_dir().join("minicode_wm_minitest54_uncompleted_first");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let steps = vec![
        "Run static build and lint verification".to_string(),
        "Run live browser layout verification".to_string(),
    ];
    wm.init_two_tier_plan("Verify Suite", &steps, None, None)
        .unwrap();

    wm.update_progress("1", "completed").unwrap();
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 1);

    // Shared keyword "verification" must match uncompleted Step 2 rather than returning AlreadyInStatus on Step 1
    wm.update_progress("verification", "completed").unwrap();
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 2);
    assert!(wm.is_all_project_tasks_completed());

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest55_verification_double_stepping_guard_and_progress_log_alignment() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_wm_minitest55_guard_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    let wm = WorkingMemory::new(&temp_dir);
    let step8 = "Verify: bun run build, bun run lint, start dev server, browser QA at 1440/768/390";
    let auto_subs = WorkingMemory::synthesize_compound_verification_substeps(step8);
    assert_eq!(auto_subs.len(), 3);

    let steps = vec![
        "Design tokens in src/index.css".to_string(),
        "Build Hero in src/Hero.tsx".to_string(),
        "Build Swarm in src/Swarm.tsx".to_string(),
        "Build Council in src/Council.tsx".to_string(),
        "Build Memory in src/Memory.tsx".to_string(),
        "Build Benchmarks in src/Benchmarks.tsx".to_string(),
        "Assemble App in src/App.tsx".to_string(),
        step8.to_string(),
    ];
    let mut step_substeps = vec![Vec::new(); 8];
    step_substeps[7] = auto_subs;

    wm.init_two_tier_plan_with_substeps("Landing Page", &steps, &step_substeps, None, None)
        .unwrap();

    // Complete Step 1 -> progress.md MUST log Step 1 ("Design tokens in src/index.css"), NOT off-by-one Step 2!
    let (mod1, title1) = wm.update_progress_detailed("active", "completed").unwrap();
    assert!(mod1);
    assert_eq!(title1, "Design tokens in src/index.css");
    let progress_after_1 = fs::read_to_string(wm.progress_path()).unwrap();
    assert!(progress_after_1.contains("**completed**: Design tokens in src/index.css"));
    assert!(!progress_after_1.contains("**completed**: Build Hero in src/Hero.tsx"));

    // Advance Steps 2..=7 to `[x]` -> Step 8's 3-substep subplan auto-activates
    for _ in 1..7 {
        wm.update_progress("active", "completed").unwrap();
    }
    assert!(wm.is_tactical_subplan());
    assert!(wm
        .format_remaining_tasks_hint()
        .contains("Macro Roadmap: 7/8 completed"));

    // Starting dev server via `minitask` auto-completes Substep 1 (`1/3`) and promotes Substep 2 (`desktop QA`)
    assert_eq!(
        wm.reconcile_verification_substep_on_tool_transition(
            "minitask",
            &serde_json::json!({"action": "start", "command": "bun run dev -- --port 4173"}),
            true,
        ),
        1
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 1);

    // Immediately calling `update_progress("active", "completed")` BEFORE any browser check ran
    // MUST idempotently acknowledge Substep 1 rather than prematurely closing Substep 2 (`desktop QA`)!
    let (mod_premature, title_premature) =
        wm.update_progress_detailed("active", "completed").unwrap();
    assert!(!mod_premature);
    assert!(
        title_premature.contains("Run static build, typecheck, and lint verification"),
        "got: {title_premature}"
    );
    assert_eq!(
        wm.read_active_plan_summary().unwrap().completed_tasks,
        1,
        "Desktop browser QA substep must remain [>] until browser QA actually runs"
    );

    // Once desktop `browser_qa_audit` runs, `update_progress("active", "completed")` cleanly completes Substep 2 (`2/3`)
    wm.record_verification_tool_activity("browser_qa_audit", &serde_json::json!({}), true);
    let (mod_desktop, title_desktop) = wm.update_progress_detailed("active", "completed").unwrap();
    assert!(mod_desktop);
    assert!(title_desktop.contains("desktop browser layout & QA"));
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 2);

    fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_minitest56_lhs_deliverable_clauses_and_step_transition_reconciliation() {
    use crate::context::memory::working_memory::ActiveStepReadiness;

    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_wm_minitest56_reconcile_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(temp_dir.join("src/components/landing")).unwrap();
    fs::create_dir_all(temp_dir.join("src/data")).unwrap();
    fs::create_dir_all(temp_dir.join("src/pages")).unwrap();

    let steps = vec![
        "Scaffold stack + read project docs: Run kit_stack_add(\"react-vite-gsap\") into the workspace, inspect generated files, and read minikit_docs/ conventions.".to_string(),
        "Create design system & global CSS: Update index.html (fonts + title + dark theme), src/index.css (@theme tokens, obsidian surfaces).".to_string(),
        "Nav + Hero + live terminal: Sticky glass nav, two-column hero + animated TUI terminal preview component.".to_string(),
        "Philosophy + lightweight sections: Centered philosophy section + 3 comparison stat cards.".to_string(),
        "Bento grid + swarm DAG: 12-col bento layout: Multi-Agent Swarm card with SVG DAG visualization.".to_string(),
        "Council + Code Intelligence sections: Council visual + Code Intelligence split.".to_string(),
        "Security + Browser Automation sections: Security layers diagram + headless Chrome CDP visual with bounding-box HUD & console/network stream.".to_string(),
        "Tools + interfaces + comparison + CTA: Tool taxonomy grid, TUI vs --json-stream split showcase, honest comparison table, final CTA banner, and minimal footer.".to_string(),
        "Verify build + responsive browser QA: Run bun run build & lint checks, start dev server via minitask, inspect desktop/tablet/mobile viewports with browser_qa_audit & screenshots, fix any layout/contrast/overflow issues, and run power_verify.".to_string(),
    ];

    // Step 1 (scaffold + read) and Step 9 (verification gate) are NOT file-deliverable groups,
    // whereas Steps 2..=8 ALL declare verifiable file/component deliverables!
    assert!(!WorkingMemory::task_declares_verifiable_deliverables(
        &steps[0]
    ));
    for s in &steps[1..8] {
        assert!(
            WorkingMemory::task_declares_verifiable_deliverables(s),
            "Expected deliverable step to be verifiable: {s}"
        );
    }
    assert!(!WorkingMemory::task_declares_verifiable_deliverables(
        &steps[8]
    ));

    let mut step_substeps = vec![Vec::new(); 9];
    step_substeps[8] = WorkingMemory::synthesize_compound_verification_substeps(&steps[8]);
    assert_eq!(step_substeps[8].len(), 3);

    let wm = WorkingMemory::new(&temp_dir);
    wm.init_two_tier_plan_with_substeps(
        "minicode landing page",
        &steps,
        &step_substeps,
        None,
        None,
    )
    .unwrap();

    // Advance Steps 1 & 2 (`package.json` + `index.html` + `src/index.css`)
    fs::write(temp_dir.join("package.json"), "{\"name\":\"demo\"}").unwrap();
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady
    );
    wm.update_progress("active", "completed").unwrap();

    fs::write(
        temp_dir.join("index.html"),
        "<!doctype html><html><body></body></html>",
    )
    .unwrap();
    fs::write(temp_dir.join("src/index.css"), ":root { --bg: #08090d; }").unwrap();
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady
    );
    wm.update_progress("active", "completed").unwrap();

    // Step 3 (`Nav + Hero + live terminal`): `Navbar.tsx` (`navbar` <-> `nav`), `HeroSection.tsx`, `AgentTerminal.tsx`
    fs::write(
        temp_dir.join("src/components/Navbar.tsx"),
        "export const Navbar = () => <nav />;",
    )
    .unwrap();
    assert_eq!(wm.active_step_readiness(), ActiveStepReadiness::InProgress);
    fs::write(
        temp_dir.join("src/components/landing/HeroSection.tsx"),
        "export const HeroSection = () => <section />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/landing/AgentTerminal.tsx"),
        "export const AgentTerminal = () => <div />;",
    )
    .unwrap();
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady,
        "Navbar.tsx + HeroSection.tsx + AgentTerminal.tsx must satisfy 'Nav + Hero + live terminal'"
    );
    wm.update_progress("active", "completed").unwrap();

    // Step 4: explicit `update_progress("active", "completed")` must always advance when not auto-promoted
    wm.update_progress("active", "completed").unwrap();
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 4);

    // Step 5 (`Bento grid + swarm DAG`): `SwarmSection.tsx` (with 12-col grid layout) + `TaskGraph.tsx`
    // must satisfy `DeliverablesReady` without requiring a standalone `Bento*.tsx` file!
    fs::write(
        temp_dir.join("src/components/landing/SwarmSection.tsx"),
        "export const SwarmSection = () => <div className=\"grid lg:grid-cols-12\"><div className=\"lg:col-span-8\" /></div>;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/landing/TaskGraph.tsx"),
        "export const TaskGraph = () => <svg />;",
    )
    .unwrap();
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady,
        "SwarmSection.tsx (with 12-col grid markup) + TaskGraph.tsx must satisfy 'Bento grid + swarm DAG'"
    );
    wm.update_progress("active", "completed").unwrap();

    // Advance Step 6 explicitly
    wm.update_progress("active", "completed").unwrap();
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 6);

    // Step 7 (`Security + Browser Automation sections`): must own `BrowserSection.tsx` without Step 9 (`Verify build + responsive browser QA`) stealing or tying ownership!
    fs::write(
        temp_dir.join("src/components/landing/SecuritySection.tsx"),
        "export const SecuritySection = () => <section />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/landing/BrowserHUD.tsx"),
        "export const BrowserHUD = () => <div />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/landing/BrowserSection.tsx"),
        "export const BrowserSection = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady,
        "SecuritySection.tsx + BrowserSection.tsx must satisfy Step 7 even when Step 9 mentions browser QA"
    );

    // Suppose the model skips `update_progress` on Step 7 and immediately writes Step 8's first file (`ToolsSection.tsx`):
    // `reconcile_preceding_tasks_on_step_transition` MUST immediately catch up Step 7 -> `[x]` (7/9)!
    fs::write(
        temp_dir.join("src/components/landing/ToolsSection.tsx"),
        "export const ToolsSection = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.reconcile_preceding_tasks_on_step_transition(&[
            "src/components/landing/ToolsSection.tsx".to_string()
        ]),
        1
    );
    assert_eq!(wm.read_active_plan_summary().unwrap().completed_tasks, 7);
    assert_eq!(wm.active_step_readiness(), ActiveStepReadiness::InProgress);

    // Complete remaining Step 8 deliverables (`InterfacesSection.tsx`, `CompareSection.tsx` <-> `comparison`, `CTASection.tsx`)
    fs::write(
        temp_dir.join("src/components/landing/InterfacesSection.tsx"),
        "export const InterfacesSection = () => <section />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/landing/CompareSection.tsx"),
        "export const CompareSection = () => <section />;",
    )
    .unwrap();
    fs::write(
        temp_dir.join("src/components/landing/CTASection.tsx"),
        "export const CTASection = () => <section />;",
    )
    .unwrap();
    assert_eq!(
        wm.active_step_readiness(),
        ActiveStepReadiness::DeliverablesReady,
        "CompareSection.tsx must match 'comparison' root and mark Step 8 DeliverablesReady"
    );

    // If the model starts Step 9's static build (`exec_cmd: bun run build`) without calling `update_progress` on Step 8,
    // `reconcile_verification_substep_on_tool_transition` auto-completes Step 8 (8/9) and activates Step 9's sub-plan!
    assert_eq!(
        wm.reconcile_verification_substep_on_tool_transition(
            "exec_cmd",
            &serde_json::json!({"command": "bun run build && bun run lint"}),
            true,
        ),
        1
    );
    assert!(wm.is_tactical_subplan());
    assert!(wm
        .format_remaining_tasks_hint()
        .contains("Macro Roadmap: 8/9 completed"));

    fs::remove_dir_all(temp_dir).ok();
}
