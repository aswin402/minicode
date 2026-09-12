/// Integration tests for Phase 27:
///   - Workspace-local session storage (`.minicode/sessions/`)
///   - Session enrichment via `list_sessions_rich()`
///   - Global fallback when `.minicode/` does not exist
use minicode::agent::types::AgentEvent;
use minicode::session::store::SessionStore;

// ── Helper ────────────────────────────────────────────────────────────────────

fn tmp_dir(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("minicode_sb_{}_", tag))
        .tempdir()
        .unwrap()
}

// ── 1. Workspace-local path ───────────────────────────────────────────────────

#[test]
fn test_workspace_store_creates_sessions_inside_minicode() {
    let ws_guard = tmp_dir("ws_local");
    let ws = ws_guard.path();
    let minicode_dir = ws.join(".minicode");
    std::fs::create_dir_all(&minicode_dir).unwrap();

    let store = SessionStore::with_workspace(ws);
    let sid = store.create_session(ws).unwrap();
    assert!(!sid.is_empty());

    // Session file must be under .minicode/sessions/
    let expected = minicode_dir.join("sessions").join(format!("{}.jsonl", sid));
    assert!(
        expected.exists(),
        "session file not found at {}",
        expected.display()
    );
}

// ── 2. Global fallback when .minicode/ absent ─────────────────────────────────

#[test]
fn test_workspace_store_falls_back_to_global_when_no_minicode_dir() {
    let ws_guard = tmp_dir("ws_fallback");
    let ws = ws_guard.path();

    // Should not panic, just fall back silently
    let store = SessionStore::with_workspace(ws);
    let sid = store.create_session(ws).unwrap();
    assert!(!sid.is_empty());
}

// ── 3. list_sessions returns sorted newest-first ──────────────────────────────

#[test]
fn test_list_sessions_sorted_newest_first() {
    let dir_guard = tmp_dir("sorted");
    let dir = dir_guard.path().to_path_buf();
    let store = SessionStore::with_dir(dir.clone());

    let id1 = store.create_session(&dir).unwrap();
    // Small delay so timestamps differ
    std::thread::sleep(std::time::Duration::from_millis(20));
    let id2 = store.create_session(&dir).unwrap();

    let sessions = store.list_sessions().unwrap();
    assert_eq!(sessions.len(), 2);
    // Newest (id2) should be first
    assert_eq!(sessions[0].id, id2);
    assert_eq!(sessions[1].id, id1);
}

// ── 4. list_sessions_rich returns correct event_count ─────────────────────────

#[test]
fn test_list_sessions_rich_event_count() {
    let dir_guard = tmp_dir("rich");
    let dir = dir_guard.path().to_path_buf();
    let store = SessionStore::with_dir(dir.clone());
    let sid = store.create_session(&dir).unwrap();

    let event = AgentEvent::TurnStart {
        turn_id: 1,
        timestamp: chrono::Utc::now().to_rfc3339(),
        model: "gemini-2.5-pro".to_string(),
        context_tokens: 100,
    };
    store.append_event(&sid, &event).unwrap();
    store.append_event(&sid, &event).unwrap();
    store.append_event(&sid, &event).unwrap();

    let rich = store.list_sessions_rich().unwrap();
    assert_eq!(rich.len(), 1);
    assert_eq!(rich[0].event_count, 3);
}

// ── 5. load_session round-trips events ───────────────────────────────────────

#[test]
fn test_load_session_round_trip() {
    let dir_guard = tmp_dir("rt");
    let dir = dir_guard.path().to_path_buf();
    let store = SessionStore::with_dir(dir.clone());
    let sid = store.create_session(&dir).unwrap();

    let event = AgentEvent::TurnStart {
        turn_id: 7,
        timestamp: chrono::Utc::now().to_rfc3339(),
        model: "claude-sonnet-4-5".to_string(),
        context_tokens: 512,
    };
    store.append_event(&sid, &event).unwrap();

    let loaded = store.load_session(&sid).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0], event);
}

// ── 6. load_session returns error for non-existent id ─────────────────────────

#[test]
fn test_load_session_not_found_error() {
    let dir_guard = tmp_dir("nf");
    let dir = dir_guard.path().to_path_buf();
    let store = SessionStore::with_dir(dir);

    let result = store.load_session("this-id-does-not-exist");
    assert!(result.is_err());
}

// ── 7. get_last_session_id returns most recent ────────────────────────────────

#[test]
fn test_get_last_session_id_returns_newest() {
    let dir_guard = tmp_dir("last");
    let dir = dir_guard.path().to_path_buf();
    let store = SessionStore::with_dir(dir.clone());

    let _id1 = store.create_session(&dir).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let id2 = store.create_session(&dir).unwrap();

    let last = store.get_last_session_id();
    assert_eq!(last, Some(id2));
}
