use minicode::session::backup::{BackupManager, BackupManifest};
use minicode::session::undo::rollback_to_checkpoint;
use minicode::ui::modal::{format_time_ago, ModalState};
use minicode::ui::Theme;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::fs;
use std::path::PathBuf;

fn create_temp_workspace() -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_integration_undo_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();
    temp_dir
}

#[tokio::test]
async fn test_multi_turn_checkpoint_creation_and_rollback() {
    let ws = create_temp_workspace();
    let mgr = BackupManager::new(&ws);

    let file_a = ws.join("service.rs");
    let file_b = ws.join("handler.rs");
    let file_c = ws.join("temp.rs");

    // Turn 1: Initial files
    fs::write(&file_a, "// Service v1").unwrap();
    mgr.record_turn_start(1, "create service v1", 0).unwrap();
    let bk_a1 = mgr.create_checkpoint(&ws, &file_a, 1).unwrap();
    let mut m1 = BackupManifest::new(1);
    m1.files = vec![bk_a1];
    m1.user_prompt = Some("create service v1".to_string());
    mgr.save_turn_manifest(&m1).unwrap();

    // Turn 2: Modify service.rs, add handler.rs
    mgr.record_turn_start(2, "add handler and mutate service", 2)
        .unwrap();
    let bk_a2 = mgr.create_checkpoint(&ws, &file_a, 2).unwrap();
    let bk_b2 = mgr.create_checkpoint(&ws, &file_b, 2).unwrap();
    let mut m2 = BackupManifest::new(2);
    m2.files = vec![bk_a2, bk_b2];
    m2.user_prompt = Some("add handler and mutate service".to_string());
    mgr.save_turn_manifest(&m2).unwrap();
    fs::write(&file_a, "// Service v2").unwrap();
    fs::write(&file_b, "// Handler v1").unwrap();

    // Turn 3: Add temp.rs, modify handler.rs
    mgr.record_turn_start(3, "add temp file", 4).unwrap();
    let bk_b3 = mgr.create_checkpoint(&ws, &file_b, 3).unwrap();
    let bk_c3 = mgr.create_checkpoint(&ws, &file_c, 3).unwrap();
    let mut m3 = BackupManifest::new(3);
    m3.files = vec![bk_b3, bk_c3];
    m3.user_prompt = Some("add temp file".to_string());
    mgr.save_turn_manifest(&m3).unwrap();
    fs::write(&file_b, "// Handler v2").unwrap();
    fs::write(&file_c, "// Temp file").unwrap();

    // Verify list_checkpoints has all 3 in descending order
    let checkpoints = mgr.list_checkpoints();
    assert_eq!(checkpoints.len(), 3);
    assert_eq!(checkpoints[0].turn_id, 3);
    assert_eq!(checkpoints[0].user_prompt.as_deref(), Some("add temp file"));
    assert_eq!(checkpoints[1].turn_id, 2);
    assert_eq!(
        checkpoints[1].user_prompt.as_deref(),
        Some("add handler and mutate service")
    );
    assert_eq!(checkpoints[2].turn_id, 1);
    assert_eq!(
        checkpoints[2].user_prompt.as_deref(),
        Some("create service v1")
    );

    // Rollback to checkpoint 2 (which reverts Turn 3 and Turn 2)
    let res = rollback_to_checkpoint(&ws, 2).unwrap();
    assert_eq!(res.turn_id, 2);
    assert!(res.restored_count >= 1);

    // Verify temp.rs and handler.rs were cleaned up / restored
    assert!(
        !file_c.exists(),
        "temp.rs created in turn 3 should be deleted"
    );
    assert_eq!(fs::read_to_string(&file_a).unwrap(), "// Service v1");

    // Only checkpoint 1 should remain
    let remaining_checkpoints = mgr.list_checkpoints();
    assert_eq!(remaining_checkpoints.len(), 1);
    assert_eq!(remaining_checkpoints[0].turn_id, 1);

    let _ = fs::remove_dir_all(&ws);
}

#[test]
fn test_undo_modal_timeline_graph_rendering() {
    let theme = Theme::aura_dark();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    let manifests = vec![
        BackupManifest {
            turn_id: 5,
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_prompt: Some("fix the thought duration timer".to_string()),
            message_index: 8,
            working_memory_plan: None,
            files: vec![minicode::session::backup::BackedUpFile {
                original_path: "/ws/src/ui/view.rs".to_string(),
                backup_path: "/ws/.minicode/backups/5/src/ui/view.rs".to_string(),
                existed_before: true,
            }],
        },
        BackupManifest {
            turn_id: 4,
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_prompt: Some("implement cost spend in status bar".to_string()),
            message_index: 6,
            working_memory_plan: None,
            files: vec![],
        },
    ];

    let modal = ModalState::new_undo_checkpoint(manifests);

    terminal
        .draw(|f| {
            let area = f.area();
            modal.render(f, area, &theme);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();

    assert!(content.contains("Undo to Checkpoint"));
    assert!(content.contains("[Turn 5] (Latest)"));
    assert!(content.contains("fix the thought duration timer"));
    assert!(content.contains("[Turn 4]"));
    assert!(content.contains("implement cost spend"));
    assert!(content.contains("Revert"));
}

#[test]
fn test_format_time_ago() {
    let now = chrono::Utc::now().to_rfc3339();
    assert!(format_time_ago(&now).contains("ago"));

    let past_5m = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
    assert_eq!(format_time_ago(&past_5m), "5m ago");

    let past_2h = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
    assert_eq!(format_time_ago(&past_2h), "2h ago");
}
