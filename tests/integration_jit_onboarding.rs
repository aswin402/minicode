//! End-to-end integration tests for deferred provider setup (Gate 1),
//! zero-modal Orchestrator-driven CodeGraph autonomy, and silent background AST shard healing.

use minicode::agent::orchestrator::{RepoState, WorkflowRouter};
use minicode::app::{AgentCommand, App, CommandAction};
use minicode::config::Config;
use minicode::context::graph::{CodeGraph, CodeGraphTelemetry};
use minicode::ui::modal::ModalState;
use std::fs;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_unindexed_repo_silent_startup() {
    let dir = tempdir().expect("tempdir");
    let config = Config::default();
    let app = App::new(dir.path(), config);

    assert!(
        matches!(app.modal, ModalState::None),
        "App must initialize with ModalState::None, got {:?}",
        app.modal
    );
    assert!(app.pending_submission.is_none());
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

    assert_eq!(
        app.pending_submission.as_ref().map(|s| s.prompt.as_str()),
        Some("add login to auth.rs")
    );
    assert!(control_rx.try_recv().is_err());
}

#[tokio::test]
async fn test_unindexed_existing_repo_dispatches_to_orchestrator_without_modal() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(src_dir.join("auth.rs"), "pub fn verify_session() {}\n").expect("write auth.rs");
    fs::write(src_dir.join("main.rs"), "fn main() {}\n").expect("write main.rs");

    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let graph_file = dir.path().join(".minicode").join("graph.json");
    assert!(!graph_file.exists());

    // Any engineering prompt on an unindexed multi-file project dispatches straight to the Orchestrator
    // without popping a blocking WorkspaceAnalysis modal.
    let prompt = "add login in auth.rs";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);
    assert!(
        matches!(app.modal, ModalState::None),
        "Expected zero blocking modal on unindexed repo, got {:?}",
        app.modal
    );
    assert!(app.pending_submission.is_none());

    match control_rx.try_recv() {
        Ok(AgentCommand::Prompt(dispatched, _)) => {
            assert_eq!(dispatched, prompt);
        }
        _ => panic!("Expected AgentCommand::Prompt dispatched directly to Orchestrator"),
    }

    // Orchestrator receives truthful unindexed telemetry so the LLM decides when to invoke CodeGraph tools
    let (repo_state, code_files, doc_files) = WorkflowRouter::scan_repository_state(dir.path());
    assert_eq!(repo_state, RepoState::ExistingCodebase);
    let telemetry = CodeGraph::inspect_workspace_telemetry(dir.path(), &code_files);
    assert_eq!(telemetry, CodeGraphTelemetry::Unindexed { source_files: 2 });

    let state_block =
        WorkflowRouter::enrich_repository_state(dir.path(), repo_state, &code_files, &doc_files);
    assert!(state_block.contains("AST CodeGraph: unindexed (0/2 source files indexed"));
}

#[tokio::test]
async fn test_natural_language_analyze_prompt_routes_to_orchestrator_not_modal() {
    let dir = tempdir().expect("tempdir");
    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "analyze the full project";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);
    assert!(
        matches!(app.modal, ModalState::None),
        "Natural language prompt must go to Orchestrator LLM, not hijack UI with a modal"
    );
    assert!(matches!(
        control_rx.try_recv(),
        Ok(AgentCommand::Prompt(ref p, _)) if p == prompt
    ));
}

#[tokio::test]
async fn test_major_workspace_drift_never_blocks_and_heals_silently_in_background() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");

    fs::write(src_dir.join("main.rs"), "fn main() {}\n").expect("write main.rs");
    fs::write(src_dir.join("lib.rs"), "pub fn add() {}\n").expect("write lib.rs");

    let mut graph = CodeGraph::new();
    graph.build_graph(dir.path()).expect("build graph");
    graph.save_to_disk(dir.path()).expect("save to disk");

    // Simulate 20 newly created files (e.g., website build or git branch switch)
    for i in 1..=20 {
        fs::write(
            src_dir.join(format!("page_{}.tsx", i)),
            format!("export function Page{}() {{ return <div/>; }}\n", i),
        )
        .expect("write page file");
    }

    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "update the navbar styling";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);
    assert!(
        matches!(app.modal, ModalState::None),
        "Must never block prompt with WorkspaceDrift modal, got {:?}",
        app.modal
    );
    assert!(app.pending_submission.is_none());
    assert!(matches!(
        control_rx.try_recv(),
        Ok(AgentCommand::Prompt(ref p, _)) if p == prompt
    ));

    // Wait briefly for the spawned background shard sync to finish and verify disk persistence
    for _ in 0..40 {
        let mut check = CodeGraph::new();
        if check.load_cached(dir.path()) && check.file_count() >= 22 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    let mut updated_graph = CodeGraph::new();
    assert!(updated_graph.load_cached(dir.path()));
    assert_eq!(updated_graph.file_count(), 22);
    assert_eq!(
        updated_graph.check_drift(dir.path()).unwrap().total_drift(),
        0
    );
}

#[tokio::test]
async fn test_chained_provider_setup_dispatches_prompt_immediately() {
    let dir = tempdir().expect("tempdir");
    let mut config = Config::default();
    config.provider.default = "anthropic".to_string();
    config.provider.api_keys.clear();
    std::env::remove_var("ANTHROPIC_API_KEY");

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "add login to auth.rs";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);
    assert!(matches!(
        app.modal,
        ModalState::ProviderSetupRequired { .. }
    ));

    app.handle_modal_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('1'),
            crossterm::event::KeyModifiers::NONE,
        ),
        &control_tx,
    )
    .await;

    assert!(matches!(app.modal, ModalState::ApiKeyInput { .. }));

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

    app.handle_modal_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
        &control_tx,
    )
    .await;

    assert!(matches!(app.modal, ModalState::ModelSelect { .. }));
    app.handle_modal_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
        &control_tx,
    )
    .await;

    // After completing API key & model setup, no secondary modal blocks the prompt — it dispatches immediately!
    assert!(
        matches!(app.modal, ModalState::None),
        "Expected ModalState::None after setup completion, got {:?}",
        app.modal
    );
    assert!(app.pending_submission.is_none());

    match control_rx.try_recv() {
        Ok(AgentCommand::UpdateConfig { provider, .. }) => {
            assert_eq!(provider.name(), "anthropic");
        }
        _ => panic!("Expected AgentCommand::UpdateConfig"),
    }

    match control_rx.try_recv() {
        Ok(AgentCommand::Prompt(dispatched, _)) => {
            assert_eq!(dispatched, prompt);
        }
        _ => panic!("Expected pending prompt to dispatch immediately after provider setup"),
    }

    std::env::remove_var("ANTHROPIC_API_KEY");
}

#[tokio::test]
async fn test_corrupted_graph_self_heals_without_modal() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(src_dir.join("router.rs"), "pub fn route() {}\n").expect("write router.rs");

    let minicode_dir = dir.path().join(".minicode");
    fs::create_dir_all(&minicode_dir).expect("create .minicode dir");
    fs::write(minicode_dir.join("graph.json"), "{ invalid json garbage }")
        .expect("write corrupt graph.json");

    let mut config = Config::default();
    config.provider.default = "ollama".to_string();

    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    let prompt = "modify the router logic in src/router.rs";
    let action = app
        .handle_command_or_prompt(prompt, None, &control_tx)
        .await
        .expect("handle_command_or_prompt");

    assert_eq!(action, CommandAction::Continue);
    assert!(
        matches!(app.modal, ModalState::None),
        "Corrupted graph.json must self-heal silently without blocking user prompt"
    );
    assert!(matches!(
        control_rx.try_recv(),
        Ok(AgentCommand::Prompt(ref p, _)) if p == prompt
    ));

    for _ in 0..40 {
        let mut check = CodeGraph::new();
        if check.load_cached(dir.path()) && check.file_count() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    let mut healed = CodeGraph::new();
    assert!(healed.load_cached(dir.path()));
    assert_eq!(healed.file_count(), 1);
}
