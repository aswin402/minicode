use minicode::agent::minipower::MiniPowerEngine;
use minicode::agent::prompt::PromptBuilder;
use minicode::agent::verification_barrier::VerificationBarrier;
use minicode::context::memory::intent::{IntentLedger, RequirementStatus};
use minicode::tools::minikit::sync::MiniKitSyncEngine;
use minicode::tools::minikit::{
    resolve_core_docs_dir, resolve_doc_path, resolve_packages_docs_dir, resolve_skills_docs_dir,
    CORE_DOC_FILES,
};
use minicode::ui::modals::minipower::MiniPowerModalState;
use minicode::ui::theme::Theme;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_e2e_category_wise_doc_scaffolding_and_architecture() {
    let dir = tempdir().expect("Failed to create tempdir");
    let ws = dir.path();

    // 1. Scaffold a realistic Rust project
    fs::create_dir_all(ws.join("src").join("service")).unwrap();
    fs::write(
        ws.join("Cargo.toml"),
        "[package]\nname = \"e2e_app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        ws.join("src").join("main.rs"),
        "fn main() { println!(\"Hello\"); }\n",
    )
    .unwrap();
    fs::write(
        ws.join("src").join("service").join("mod.rs"),
        "pub fn do_work() {}\n",
    )
    .unwrap();

    // 2. Run MiniKitSyncEngine::sync
    let sync_res = MiniKitSyncEngine::sync(ws).expect("Sync should succeed");
    assert!(sync_res.contains("Core Specs:"));
    assert!(sync_res.contains("Domain Skills:"));
    assert!(sync_res.contains("Memory Isolation: .minicode/"));

    // 3. Verify directory layout
    let core_dir = resolve_core_docs_dir(ws);
    let skills_dir = resolve_skills_docs_dir(ws);
    let packages_dir = resolve_packages_docs_dir(ws);

    assert!(core_dir.is_dir(), "core/ directory must exist");
    assert!(skills_dir.is_dir(), "skills/ directory must exist");
    assert!(packages_dir.is_dir(), "packages/ directory must exist");

    // 4. Verify all 8 canonical core files exist in core/
    for doc in CORE_DOC_FILES {
        let p = core_dir.join(doc);
        assert!(
            p.is_file(),
            "Canonical core file {} must exist under core/",
            doc
        );
    }

    // 5. Verify architecture.md was synthesized with CodeGraph details
    let arch_content = fs::read_to_string(core_dir.join("architecture.md")).unwrap();
    assert!(arch_content.contains("Architecture Documentation:"));
    assert!(arch_content.contains("Clean Architecture Layer Breakdown"));
    assert!(arch_content.contains("Architectural Component & Data Flow"));

    // 6. Verify .gitignore has .minicode/
    let gitignore = fs::read_to_string(ws.join(".gitignore")).unwrap();
    assert!(
        gitignore.contains(".minicode/"),
        ".gitignore must contain .minicode/"
    );
}

#[tokio::test]
async fn test_e2e_legacy_migration_zero_data_loss() {
    let dir = tempdir().expect("Failed to create tempdir");
    let ws = dir.path();
    let docs = ws.join("minikit_docs");
    fs::create_dir_all(&docs).unwrap();

    // Create legacy flat files with custom content
    let legacy_prd = "# Custom PRD\n- Critical requirement that must never be lost";
    let legacy_todo = "# Task Tracker\n- [x] Phase 1\n- [ ] Phase 2 in progress";
    let legacy_skill = "---\ndescription: Rust guidelines\nglobs: [\"*.rs\"]\n---\n# Rust Skill";
    let catalog_index = "# OKF Index";
    let catalog_log = "# OKF Log";

    fs::write(docs.join("prd.md"), legacy_prd).unwrap();
    fs::write(docs.join("todo.md"), legacy_todo).unwrap();
    fs::write(docs.join("rust.md"), legacy_skill).unwrap();
    fs::write(docs.join("index.md"), catalog_index).unwrap();
    fs::write(docs.join("log.md"), catalog_log).unwrap();

    // Run ensure_workflow_docs (migration)
    MiniKitSyncEngine::ensure_workflow_docs(&docs, "legacy_demo", "rust");

    // Assert files moved without data loss
    assert_eq!(
        fs::read_to_string(docs.join("core").join("prd.md")).unwrap(),
        legacy_prd
    );
    assert_eq!(
        fs::read_to_string(docs.join("core").join("todo.md")).unwrap(),
        legacy_todo
    );
    assert_eq!(
        fs::read_to_string(docs.join("skills").join("rust.md")).unwrap(),
        legacy_skill
    );

    // Root index and log preserved
    assert_eq!(
        fs::read_to_string(docs.join("index.md")).unwrap(),
        catalog_index
    );
    assert_eq!(
        fs::read_to_string(docs.join("log.md")).unwrap(),
        catalog_log
    );

    // Flat files moved away from root
    assert!(!docs.join("prd.md").exists());
    assert!(!docs.join("todo.md").exists());
    assert!(!docs.join("rust.md").exists());
}

#[tokio::test]
async fn test_e2e_two_tier_plan_hierarchy_and_minipower() {
    let dir = tempdir().expect("Failed to create tempdir");
    let ws = dir.path();
    let docs = ws.join("minikit_docs");
    MiniKitSyncEngine::ensure_workflow_docs(&docs, "two_tier_demo", "rust");

    // 1. Test power_plan tool dispatching writes to core/todo.md
    let plan_args = json!({
        "topic": "Real-time Telemetry Service",
        "save_to_docs": true
    });
    let dispatch_res =
        minicode::tools::registry::minipower_tools::dispatch("power_plan", &plan_args, ws)
            .await
            .expect("Tool should be found")
            .expect("Dispatch should succeed");

    assert!(dispatch_res.contains("MiniPower: Structured Implementation Plan Builder"));
    let todo_content = fs::read_to_string(resolve_doc_path(ws, "todo.md")).unwrap();
    assert!(todo_content.contains("MiniPower Plan: Real-time Telemetry Service"));

    // 2. Test Level 2 IntentLedger persistence and lifecycle
    let persistence_path = ws.join(minicode::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
    let mut ledger = IntentLedger::new("Ship Telemetry Feature");
    let req1 = ledger.add_item(
        "Implement Prometheus Exporter",
        None,
        vec!["src/metrics.rs".to_string()],
    );
    let _req2 = ledger.add_item(
        "Add Health Endpoint",
        None,
        vec!["src/health.rs".to_string()],
    );
    ledger.set_status(&req1, RequirementStatus::Completed);
    ledger
        .save_to_disk(&persistence_path)
        .expect("Should save ledger to disk");

    let loaded_ledger =
        IntentLedger::load_from_disk(&persistence_path).expect("Should load ledger");
    assert_eq!(loaded_ledger.root_objective, "Ship Telemetry Feature");
    assert_eq!(loaded_ledger.completed_count(), 1);
    assert_eq!(loaded_ledger.total_count(), 2);

    // 3. Test PromptBuilder recency context with <project_blueprint> and <minipower_active_plan>
    let recency = PromptBuilder::build_recency_context(
        ws,
        Some("Focus on metrics"),
        &["src/metrics.rs".to_string()],
        None,
        None,
        Some(&loaded_ledger),
    );
    assert!(recency.contains("<project_blueprint>"));
    assert!(recency.contains("Location: minikit_docs/core/"));
    assert!(recency.contains("architecture.md"));
    assert!(recency.contains("todo.md"));
    assert!(recency.contains("<minipower_active_plan>"));
    assert!(recency.contains("<goal_anchor>"));
    assert!(recency.contains("<execution_ledger"));

    // 4. Test MiniPower modal state rendering
    let state = MiniPowerModalState::new(ws);
    let _theme = Theme::aura_dark();
    assert_eq!(
        state.active_tab,
        minicode::ui::modals::minipower::MiniPowerTab::Pillars
    );
    let next_tab = state.active_tab.next().next().next();
    assert_eq!(
        next_tab,
        minicode::ui::modals::minipower::MiniPowerTab::PlanHierarchy
    );
    assert_eq!(next_tab.title(), "Two-Tier Plan");
}

#[tokio::test]
async fn test_e2e_minipower_tools_suite() {
    let dir = tempdir().expect("Failed to create tempdir");
    let ws = dir.path();

    // 1. power_status
    let status = MiniPowerEngine::format_status_summary();
    assert!(status.contains("MiniPower: Native Autonomous Engineering Methodology"));
    assert!(status.contains("Core Pillars"));
    assert!(status.contains("Anti-Rationalization"));

    // 2. power_brainstorm
    let bs = MiniPowerEngine::format_brainstorm_prompt(ws, "Distributed Tracing");
    assert!(bs.contains("Socratic Brainstorming"));
    assert!(bs.contains("core/spec.md"));

    // 3. VerificationBarrier in clean repo
    let report = VerificationBarrier::verify(ws, &[]).await;
    assert!(report.all_passed);
}
