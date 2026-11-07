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
