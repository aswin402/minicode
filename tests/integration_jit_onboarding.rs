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

    // Gate 1 must intercept and present ProviderSetupRequired
    match &app.modal {
        ModalState::ProviderSetupRequired {
            provider_name,
            pending_prompt_preview,
            selected_index,
        } => {
            assert_eq!(provider_name, "anthropic");
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
