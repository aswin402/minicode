//! End-to-end integration tests for JIT repository onboarding, deferred provider setup,
//! and workspace drift arbitration gates (Phase 136).

use minicode::app::{AgentCommand, App, CommandAction};
use minicode::config::Config;
use minicode::context::graph::CodeGraph;
use minicode::ui::modal::ModalState;
use std::fs;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_unindexed_repo_silent_startup() {
    let dir = tempdir().expect("tempdir");
    let config = Config::default();
    let app = App::new(dir.path(), config);

    // Silent startup: unindexed repo must boot with ModalState::None
    assert!(
        matches!(app.modal, ModalState::None),
        "App must initialize with ModalState::None, got {:?}",
        app.modal
    );
    assert!(app.pending_submission.is_none());
    assert!(!app.session_skipped_indexing);
    assert!(!app.session_skipped_drift);
}

#[tokio::test]
async fn test_unconfigured_provider_triggers_setup_modal() {
    let dir = tempdir().expect("tempdir");
    let mut config = Config::default();
    config.provider.default = "mistral".to_string();
    config.provider.api_keys.clear();
    std::env::remove_var("MISTRAL_API_KEY");

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "add login to auth.rs";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // Gate 1 must intercept and present ProviderSetupRequired
    match &app.modal {
        ModalState::ProviderSetupRequired {
            provider_name,
            pending_prompt_preview,
            selected_index,
        } => {
            assert_eq!(provider_name, "mistral");
            assert!(pending_prompt_preview.contains("add login"));
            assert_eq!(*selected_index, 0);
        }
        other => panic!("Expected ProviderSetupRequired modal, got {:?}", other),
    }

    // Pending submission must be captured
    assert_eq!(
        app.pending_submission.as_ref().map(|s| s.prompt.as_str()),
        Some("add login to auth.rs")
    );

    // No prompt dispatched to LLM agent actor yet
    assert!(control_rx.try_recv().is_err());
}

#[tokio::test]
async fn test_general_query_bypasses_analysis() {
    let dir = tempdir().expect("tempdir");
    let mut config = Config::default();
    // Local provider requires no API key
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "what is a mutex in computer science?";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // General query must bypass indexing modal and execute immediately
    assert!(
        matches!(app.modal, ModalState::None),
        "Expected ModalState::None for general query, got {:?}",
        app.modal
    );
    assert!(app.pending_submission.is_none());

    // Prompt must be dispatched directly to agent control channel
    match control_rx.try_recv() {
        Ok(AgentCommand::Prompt(dispatched, _)) => {
            assert_eq!(dispatched, prompt);
        }
        _ => panic!("Expected AgentCommand::Prompt dispatched"),
    }
}

#[tokio::test]
async fn test_crud_prompt_intercepted_on_unindexed_repo() {
    let dir = tempdir().expect("tempdir");
    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    // Unindexed repo: .minicode/graph.json does not exist
    let graph_file = dir.path().join(".minicode").join("graph.json");
    assert!(!graph_file.exists());

    let prompt = "add login in auth.rs";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // Gate 2 must intercept and present WorkspaceAnalysis
    match &app.modal {
        ModalState::WorkspaceAnalysis { is_indexed, .. } => {
            assert!(!*is_indexed, "Repo should be marked unindexed");
        }
        other => panic!("Expected WorkspaceAnalysis modal, got {:?}", other),
    }

    // Pending prompt must be saved
    assert_eq!(
        app.pending_submission.as_ref().map(|s| s.prompt.as_str()),
        Some("add login in auth.rs")
    );

    // Prompt must not be sent to agent actor yet
    assert!(control_rx.try_recv().is_err());
}

#[tokio::test]
async fn test_existing_repo_drift_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");

    // Create 2 initial files
    fs::write(src_dir.join("main.rs"), "fn main() {}\n").expect("write main.rs");
    fs::write(src_dir.join("lib.rs"), "pub fn add() {}\n").expect("write lib.rs");

    // Index and save graph
    let mut graph = CodeGraph::new();
    graph.build_graph(dir.path()).expect("build graph");
    graph.save_to_disk(dir.path()).expect("save to disk");

    let graph_file = dir.path().join(".minicode").join("graph.json");
    assert!(graph_file.exists());

    // Introduce significant drift (>10 added files)
    for i in 1..=12 {
        fs::write(
            src_dir.join(format!("feature_{}.rs", i)),
            format!("pub fn f{}() {{}}\n", i),
        )
        .expect("write feature file");
    }

    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "refactor the sql queries in our codebase";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // Drift Gate must intercept and present WorkspaceDrift
    match &app.modal {
        ModalState::WorkspaceDrift {
            added_count,
            selected_index,
            ..
        } => {
            assert!(
                *added_count >= 12,
                "Expected >=12 added files, got {}",
                added_count
            );
            assert_eq!(*selected_index, 0);
        }
        other => panic!("Expected WorkspaceDrift modal, got {:?}", other),
    }

    // Pending submission must be saved
    assert_eq!(
        app.pending_submission.as_ref().map(|s| s.prompt.as_str()),
        Some("refactor the sql queries in our codebase")
    );

    // Prompt must not be sent to agent actor yet
    assert!(control_rx.try_recv().is_err());
}

#[tokio::test]
async fn test_explicit_analysis_keyword_triggers_analysis() {
    let dir = tempdir().expect("tempdir");
    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, _) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "analyze the full project";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // Explicit request must immediately open WorkspaceAnalysis modal
    assert!(
        matches!(app.modal, ModalState::WorkspaceAnalysis { .. }),
        "Expected WorkspaceAnalysis modal, got {:?}",
        app.modal
    );
}

#[tokio::test]
async fn test_chained_provider_setup_then_crud_intercepts_analysis() {
    let dir = tempdir().expect("tempdir");
    let mut config = Config::default();
    config.provider.default = "anthropic".to_string();
    config.provider.api_keys.clear();
    std::env::remove_var("ANTHROPIC_API_KEY");

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    // Step 1: User enters CRUD prompt on unconfigured provider and unindexed repo
    let prompt = "add login to auth.rs";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // Gate 1 must intercept first
    assert!(matches!(
        app.modal,
        ModalState::ProviderSetupRequired { .. }
    ));
    assert_eq!(
        app.pending_submission.as_ref().map(|s| s.prompt.as_str()),
        Some("add login to auth.rs")
    );

    // Step 2: User presses '1' (Configure provider key)
    app.handle_modal_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('1'),
            crossterm::event::KeyModifiers::NONE,
        ),
        &control_tx,
    )
    .await;

    // Modal must now be ApiKeyInput
    assert!(matches!(app.modal, ModalState::ApiKeyInput { .. }));

    // User types key "sk-ant-test-key"
    for c in "sk-ant-test-key".chars() {
        app.handle_modal_key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ),
            &control_tx,
        )
        .await;
    }

    // Step 3: User presses Enter to submit key
    app.handle_modal_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
        &control_tx,
    )
    .await;

    // Step 4: ModelSelect is presented with curated models; user presses Enter to confirm model
    assert!(matches!(app.modal, ModalState::ModelSelect { .. }));
    app.handle_modal_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
        &control_tx,
    )
    .await;

    // Gate 1 completed! Now Gate 2 MUST be evaluated automatically.
    // Because repo is unindexed, app.modal MUST now be WorkspaceAnalysis!
    match &app.modal {
        ModalState::WorkspaceAnalysis { is_indexed, .. } => {
            assert!(!*is_indexed, "Repo should be marked unindexed");
        }
        other => panic!("Expected chained WorkspaceAnalysis modal, got {:?}", other),
    }

    // Pending submission must still be preserved for Gate 2 arbitration
    assert_eq!(
        app.pending_submission.as_ref().map(|s| s.prompt.as_str()),
        Some("add login to auth.rs")
    );

    // UpdateConfig command was sent when provider switch finished, but NOT a Prompt command
    match control_rx.try_recv() {
        Ok(AgentCommand::UpdateConfig { provider, .. }) => {
            assert_eq!(provider.name(), "anthropic");
        }
        _ => panic!("Expected AgentCommand::UpdateConfig"),
    }

    // No Prompt command dispatched to LLM agent actor yet because Gate 2 intercepted with WorkspaceAnalysis!
    assert!(control_rx.try_recv().is_err());

    // Clean up environment variable
    std::env::remove_var("ANTHROPIC_API_KEY");
}

#[tokio::test]
async fn test_acronym_slashes_bypass_analysis() {
    let dir = tempdir().expect("tempdir");
    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "what is the difference between TCP/IP and UDP?";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // Slashes in acronyms (TCP/IP) must not trigger CRUD file path gate
    assert!(
        matches!(app.modal, ModalState::None),
        "Expected ModalState::None for TCP/IP question, got {:?}",
        app.modal
    );
    assert!(control_rx.try_recv().is_ok());
}

#[tokio::test]
async fn test_corrupted_graph_triggers_analysis() {
    let dir = tempdir().expect("tempdir");
    let minicode_dir = dir.path().join(".minicode");
    fs::create_dir_all(&minicode_dir).expect("create .minicode dir");
    fs::write(minicode_dir.join("graph.json"), "{ invalid json garbage }")
        .expect("write corrupt graph.json");

    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, _) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "modify the router logic in src/router.rs";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // Corrupted graph.json must safely fall back to WorkspaceAnalysis
    assert!(
        matches!(app.modal, ModalState::WorkspaceAnalysis { .. }),
        "Expected WorkspaceAnalysis modal on corrupted graph.json, got {:?}",
        app.modal
    );
}

#[tokio::test]
async fn test_minor_drift_seamless_sync() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");

    fs::write(src_dir.join("main.rs"), "fn main() {}\n").expect("write main.rs");
    fs::write(src_dir.join("lib.rs"), "pub fn add() {}\n").expect("write lib.rs");

    let mut graph = CodeGraph::new();
    graph.build_graph(dir.path()).expect("build graph");
    graph.save_to_disk(dir.path()).expect("save to disk");

    // Add only 1 file (minor drift: 1 file < 10)
    fs::write(src_dir.join("helper.rs"), "pub fn helper() {}\n").expect("write helper.rs");

    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "refactor src/main.rs";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);

    // Minor drift does NOT block user with modal
    assert!(
        matches!(app.modal, ModalState::None),
        "Expected ModalState::None for minor drift, got {:?}",
        app.modal
    );
    assert!(control_rx.try_recv().is_ok());

    // Verify background incremental update synced helper.rs into graph.json
    let mut updated_graph = CodeGraph::new();
    assert!(updated_graph.load_cached(dir.path()));
    assert!(
        updated_graph.file_count() >= 3,
        "Expected helper.rs to be indexed into cached graph"
    );
}
