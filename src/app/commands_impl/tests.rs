//! Unit tests for slash command parsing and `/goal` / `/intent` execution.

use super::*;
use crate::config::Config;
use crate::context::memory::intent::{IntentLedger, RequirementStatus};
use tempfile::tempdir;

#[test]
fn test_parse_goal_command_show() {
    assert_eq!(parse_goal_command("/goal"), Some(GoalSubcommand::Show));
    assert_eq!(parse_goal_command("/intent"), Some(GoalSubcommand::Show));
    assert_eq!(parse_goal_command("/goal   "), Some(GoalSubcommand::Show));
    assert_eq!(parse_goal_command("/intent   "), Some(GoalSubcommand::Show));
}

#[test]
fn test_parse_goal_command_add() {
    assert_eq!(
        parse_goal_command("/goal add Build authentication"),
        Some(GoalSubcommand::Add("Build authentication".to_string()))
    );
    assert_eq!(
        parse_goal_command("/intent add Build authentication"),
        Some(GoalSubcommand::Add("Build authentication".to_string()))
    );
    assert_eq!(
        parse_goal_command("/goal add"),
        Some(GoalSubcommand::Add(String::new()))
    );
    assert_eq!(
        parse_goal_command("/intent add"),
        Some(GoalSubcommand::Add(String::new()))
    );
}

#[test]
fn test_parse_goal_command_done() {
    assert_eq!(
        parse_goal_command("/goal done 1"),
        Some(GoalSubcommand::Done("1".to_string()))
    );
    assert_eq!(
        parse_goal_command("/intent done 2"),
        Some(GoalSubcommand::Done("2".to_string()))
    );
    assert_eq!(
        parse_goal_command("/goal done req-1234"),
        Some(GoalSubcommand::Done("req-1234".to_string()))
    );
    assert_eq!(
        parse_goal_command("/goal done"),
        Some(GoalSubcommand::Done(String::new()))
    );
    assert_eq!(
        parse_goal_command("/intent done"),
        Some(GoalSubcommand::Done(String::new()))
    );
}

#[test]
fn test_parse_goal_command_reset() {
    assert_eq!(
        parse_goal_command("/goal reset"),
        Some(GoalSubcommand::Reset)
    );
    assert_eq!(
        parse_goal_command("/intent reset"),
        Some(GoalSubcommand::Reset)
    );
    assert_eq!(
        parse_goal_command("/goal reset   "),
        Some(GoalSubcommand::Reset)
    );
}

#[test]
fn test_parse_goal_command_run() {
    assert_eq!(
        parse_goal_command("/goal run Ship release"),
        Some(GoalSubcommand::Run("Ship release".to_string()))
    );
    assert_eq!(
        parse_goal_command("/intent run Ship release"),
        Some(GoalSubcommand::Run("Ship release".to_string()))
    );
    assert_eq!(
        parse_goal_command("/goal run"),
        Some(GoalSubcommand::Run(String::new()))
    );
    assert_eq!(
        parse_goal_command("/intent run"),
        Some(GoalSubcommand::Run(String::new()))
    );
    // Freeform prompt
    assert_eq!(
        parse_goal_command("/goal Ship release"),
        Some(GoalSubcommand::Run("Ship release".to_string()))
    );
    assert_eq!(
        parse_goal_command("/intent Ship release"),
        Some(GoalSubcommand::Run("Ship release".to_string()))
    );
}

#[test]
fn test_parse_goal_command_non_goal() {
    assert_eq!(parse_goal_command("/help"), None);
    assert_eq!(parse_goal_command("/plan"), None);
    assert_eq!(parse_goal_command("/new"), None);
    assert_eq!(parse_goal_command("hello minicode"), None);
}

#[test]
fn test_format_ledger_timeline() {
    let mut ledger = IntentLedger::new("E-Commerce Store");
    let id1 = ledger.add_item("Dashboard", Some("metrics cards"), vec![]);
    let id2 = ledger.add_item("Customers Page", Some("search, filter"), vec![]);
    let _id3 = ledger.add_item("Support Tickets", None, vec![]);

    ledger.set_status(&id1, RequirementStatus::Completed);
    ledger.set_status(&id2, RequirementStatus::InProgress);

    let formatted = format_ledger_timeline(&ledger);
    assert!(formatted.contains("🎯 Active Goal: E-Commerce Store"));
    assert!(formatted.contains("📋 Living Execution Ledger (1/3 completed):"));
    assert!(formatted.contains("[x] 1. Dashboard (metrics cards)"));
    assert!(formatted.contains("[-] 2. Customers Page (search, filter)"));
    assert!(formatted.contains("[ ] 3. Support Tickets"));
    assert!(formatted.contains(
        "💡 Commands: /goal add <task> | /goal done <index> | /goal reset | /goal run <prompt>"
    ));
}

#[tokio::test]
async fn test_goal_commands_end_to_end() {
    let dir = tempdir().unwrap();
    let config = Config::default();
    let mut app = App::new(dir.path(), config);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    // 1. /goal when no ledger exists opens the Todo / Goal modal without polluting timeline
    let action = app
        .handle_command_or_prompt("/goal", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);
    assert!(matches!(app.modal, ModalState::Todo(_)));
    assert!(app.timeline.entries.is_empty());

    // 2. /goal add
    let action = app
        .handle_command_or_prompt("/goal add First Task", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);
    let persistence_file = dir.path().join(".minicode/intent_anchor.json");
    assert!(persistence_file.exists());

    // 3. /intent add
    let action = app
        .handle_command_or_prompt("/intent add Second Task", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);

    // 4. /goal (opens Todo modal containing the Goal Anchor milestone with 2 tasks)
    let action = app
        .handle_command_or_prompt("/goal", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);
    if let ModalState::Todo(ref todo_state) = app.modal {
        assert!(!todo_state.milestones.is_empty());
        assert_eq!(todo_state.milestones[0].id, "🎯 Goal Anchor");
        assert_eq!(todo_state.milestones[0].tasks.len(), 2);
        assert_eq!(todo_state.milestones[0].tasks[0].title, "First Task");
        assert_eq!(todo_state.milestones[0].tasks[1].title, "Second Task");
    } else {
        panic!("Expected ModalState::Todo");
    }

    // 5. /goal done 1
    let action = app
        .handle_command_or_prompt("/goal done 1", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);

    // Verify done in loaded file
    let ledger = IntentLedger::load_from_disk(&persistence_file).unwrap();
    assert_eq!(ledger.completed_count(), 1);
    assert_eq!(ledger.items[0].status, RequirementStatus::Completed);

    // 6. /intent done 2
    let action = app
        .handle_command_or_prompt("/intent done 2", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);
    let ledger = IntentLedger::load_from_disk(&persistence_file).unwrap();
    assert_eq!(ledger.completed_count(), 2);

    // 7. /goal done invalid index (pushes toast, keeps timeline clean)
    let action = app
        .handle_command_or_prompt("/goal done 99", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);
    assert!(app.timeline.entries.is_empty());

    // 8. /intent reset
    let action = app
        .handle_command_or_prompt("/intent reset", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);
    assert!(!persistence_file.exists());

    // 9. /goal run Launch new site
    let action = app
        .handle_command_or_prompt("/goal run Launch new site", None, &control_tx)
        .await
        .unwrap();
    assert_eq!(action, CommandAction::Continue);
    assert!(persistence_file.exists());

    let cmd = control_rx.try_recv().unwrap();
    if let AgentCommand::Prompt(prompt_text, _) = cmd {
        assert!(prompt_text.contains("<!-- GOAL -->"));
        assert!(prompt_text.contains("Launch new site"));
    } else {
        panic!("Expected AgentCommand::Prompt");
    }
}
