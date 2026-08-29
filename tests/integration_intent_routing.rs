use minicode::agent::intent::{match_intent, AgentIntent};
use minicode::agent::types::{AgentEvent, CommandDescription, StdinCommand};
use minicode::ui::modal::{ModalState, COMMAND_CATALOG_ITEMS};

#[test]
fn test_intent_matcher_direct_slash_commands() {
    let cases = [
        ("/stack", AgentIntent::StackScaffold),
        ("/plan add OAuth2", AgentIntent::MilestonePlan),
        ("/goal complete all tasks", AgentIntent::AutonomousGoal),
        ("/review --staged", AgentIntent::CodeReview),
        ("/diff", AgentIntent::GitDiff),
        ("/history", AgentIntent::SessionHistory),
        ("/sessions", AgentIntent::SessionHistory),
        ("/undo", AgentIntent::UndoRollback),
        ("/map", AgentIntent::RepoMap),
        ("/compact", AgentIntent::ContextCompact),
        ("/commands", AgentIntent::CommandCatalog),
        ("/help", AgentIntent::CommandCatalog),
    ];

    for (input, expected_intent) in cases {
        let m = match_intent(input).expect(&format!("Should match intent for: {}", input));
        assert_eq!(m.intent, expected_intent, "Failed on input: {}", input);
        assert_eq!(m.confidence, 1.0, "Confidence should be 1.0 for: {}", input);
        assert!(m.suggested_command.is_some());
    }
}

#[test]
fn test_intent_matcher_natural_language_scaffolding() {
    let queries = [
        "scaffold a nextjs stack with onpkg",
        "scaffold a react vite app",
        "create a fastapi stack",
        "bootstrap hono project",
        "onpkg stack add flutter",
    ];

    for q in queries {
        let m = match_intent(q).expect(&format!("Should match stack scaffolding for: {}", q));
        assert_eq!(
            m.intent,
            AgentIntent::StackScaffold,
            "Failed on query: {}",
            q
        );
        assert!(m.confidence >= 0.85);
        assert_eq!(m.suggested_command.as_deref(), Some("/stack"));
    }
}

#[test]
fn test_intent_matcher_natural_language_planning() {
    let cases = [
        ("plan the database migration", "database migration"),
        (
            "create a plan for OAuth2 authentication",
            "OAuth2 authentication",
        ),
        (
            "make an implementation plan for websocket events",
            "websocket events",
        ),
        (
            "break down this feature into verifiable milestones",
            "this feature into verifiable milestones",
        ),
    ];

    for (input, expected_extracted_query) in cases {
        let m = match_intent(input).expect(&format!("Should match planning intent for: {}", input));
        assert_eq!(m.intent, AgentIntent::MilestonePlan, "Failed on: {}", input);
        assert_eq!(
            m.query, expected_extracted_query,
            "Extracted query mismatch for: {}",
            input
        );
        assert!(m.confidence >= 0.85);
    }
}

#[test]
fn test_intent_matcher_natural_language_goals() {
    let queries = [
        "execute goal to fix all lint warnings",
        "run in autonomous mode to complete all tasks",
        "complete all remaining todo items",
        "run goal autonomously",
    ];

    for q in queries {
        let m = match_intent(q).expect(&format!("Should match goal intent for: {}", q));
        assert_eq!(m.intent, AgentIntent::AutonomousGoal, "Failed on: {}", q);
        assert!(m.confidence >= 0.85);
        assert_eq!(m.suggested_command.as_deref(), Some("/goal"));
    }
}

#[test]
fn test_intent_matcher_natural_language_review_and_diff() {
    let review_queries = [
        "review my changes on git diff",
        "run adversarial code review on staged files",
        "code review current workspace diff",
    ];

    for q in review_queries {
        let m = match_intent(q).expect(&format!("Should match code review for: {}", q));
        assert_eq!(m.intent, AgentIntent::CodeReview);
        assert!(m.confidence >= 0.85);
    }

    let diff_queries = [
        "show diff",
        "view diff",
        "show git diff",
        "what did i change",
        "show uncommitted changes",
    ];

    for q in diff_queries {
        let m = match_intent(q).expect(&format!("Should match git diff for: {}", q));
        assert_eq!(m.intent, AgentIntent::GitDiff);
        assert!(m.confidence >= 0.85);
    }
}

#[test]
fn test_intent_matcher_session_history_and_undo() {
    let history_queries = [
        "show past sessions",
        "view previous session history",
        "browse session history",
        "open chat history",
    ];

    for q in history_queries {
        let m = match_intent(q).expect(&format!("Should match session history for: {}", q));
        assert_eq!(m.intent, AgentIntent::SessionHistory);
        assert!(m.confidence >= 0.85);
    }

    let undo_queries = [
        "undo last turn",
        "revert changes from previous turn",
        "rollback to turn 3",
        "undo",
    ];

    for q in undo_queries {
        let m = match_intent(q).expect(&format!("Should match undo for: {}", q));
        assert_eq!(m.intent, AgentIntent::UndoRollback);
        assert!(m.confidence >= 0.85);
    }
}

#[test]
fn test_command_catalog_registry_integrity() {
    assert!(
        !COMMAND_CATALOG_ITEMS.is_empty(),
        "Command catalog must not be empty"
    );
    let mut names = std::collections::HashSet::new();

    for item in COMMAND_CATALOG_ITEMS {
        assert!(
            item.name.starts_with('/'),
            "Command name must start with /: {}",
            item.name
        );
        assert!(
            !item.description.is_empty(),
            "Command description must not be empty: {}",
            item.name
        );
        assert!(
            !item.category.is_empty(),
            "Command category must not be empty: {}",
            item.name
        );
        assert!(
            !item.example.is_empty(),
            "Command example must not be empty: {}",
            item.name
        );
        assert!(
            names.insert(item.name),
            "Duplicate command name found in catalog: {}",
            item.name
        );
    }
}

#[test]
fn test_command_catalog_filtering() {
    let mut modal = ModalState::new_command_catalog();

    if let ModalState::CommandCatalog {
        ref filtered_indices,
        ..
    } = modal
    {
        assert_eq!(filtered_indices.len(), COMMAND_CATALOG_ITEMS.len());
    } else {
        panic!("Expected ModalState::CommandCatalog");
    }

    if let ModalState::CommandCatalog { ref mut filter, .. } = modal {
        *filter = "diff".to_string();
    }
    modal.update_filter();

    if let ModalState::CommandCatalog {
        ref filtered_indices,
        ..
    } = modal
    {
        assert!(!filtered_indices.is_empty());
        for &idx in filtered_indices {
            let item = &COMMAND_CATALOG_ITEMS[idx];
            let matches = item.name.contains("diff")
                || item.description.to_lowercase().contains("diff")
                || item.category.to_lowercase().contains("diff")
                || item.shortcut.to_lowercase().contains("diff");
            assert!(
                matches,
                "Filtered item does not match query 'diff': {:?}",
                item
            );
        }
    }
}

#[test]
fn test_ndjson_stdin_command_serialization() {
    // ListCommands
    let list_cmd = StdinCommand::ListCommands {};
    let json = serde_json::to_string(&list_cmd).expect("Should serialize ListCommands");
    assert!(json.contains(r#""method":"list_commands""#));
    let parsed: StdinCommand =
        serde_json::from_str(&json).expect("Should deserialize ListCommands");
    assert_eq!(parsed, list_cmd);

    // ExecuteCommand
    let exec_cmd = StdinCommand::ExecuteCommand {
        command: "/plan".to_string(),
        args: Some("migrate auth to JWT".to_string()),
    };
    let json = serde_json::to_string(&exec_cmd).expect("Should serialize ExecuteCommand");
    assert!(json.contains(r#""method":"execute_command""#));
    assert!(json.contains("migrate auth to JWT"));
    let parsed: StdinCommand =
        serde_json::from_str(&json).expect("Should deserialize ExecuteCommand");
    assert_eq!(parsed, exec_cmd);
}

#[test]
fn test_agent_event_intent_routed_serialization() {
    let event = AgentEvent::IntentRouted {
        turn_id: Some(1),
        intent: "MilestonePlan".to_string(),
        query: "OAuth2 migration".to_string(),
        confidence: 0.95,
        suggested_command: Some("/plan".to_string()),
    };

    let json = serde_json::to_string(&event).expect("Should serialize IntentRouted");
    assert!(json.contains(r#""event":"intent_routed""#));
    assert!(json.contains(r#""intent":"MilestonePlan""#));
    assert!(json.contains(r#""query":"OAuth2 migration""#));
    assert!(json.contains(r#""confidence":0.95"#));

    let parsed: AgentEvent = serde_json::from_str(&json).expect("Should deserialize IntentRouted");
    assert_eq!(parsed, event);

    let cmd_list_event = AgentEvent::CommandList {
        commands: vec![CommandDescription {
            name: "/plan".to_string(),
            category: "Workflows & Scaffolding".to_string(),
            shortcut: "".to_string(),
            description: "Break complex feature into milestones".to_string(),
            example: "/plan auth flow".to_string(),
        }],
    };

    let json = serde_json::to_string(&cmd_list_event).expect("Should serialize CommandList");
    assert!(json.contains(r#""event":"command_list""#));
    assert!(json.contains("/plan"));
}
