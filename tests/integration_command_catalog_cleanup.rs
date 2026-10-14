//! Integration tests verifying command catalog and spotlight palette cleanup,
//! ensuring /todo and /blocks are properly registered with shortcuts and /plan is removed.

use minicode::ui::input::PALETTE_COMMANDS;
use minicode::ui::modals::command_catalog::COMMAND_CATALOG_ITEMS;

#[test]
fn test_palette_commands_cleaned_and_aligned() {
    let palette_names: Vec<&str> = PALETTE_COMMANDS.iter().map(|c| c.slash_name).collect();

    // 1. /todo must be present with F8 shortcut
    let todo_cmd = PALETTE_COMMANDS
        .iter()
        .find(|c| c.slash_name == "/todo")
        .expect("/todo must be present in PALETTE_COMMANDS");
    assert_eq!(todo_cmd.shortcut, Some("F8"));

    // 2. /blocks must be present with F6 shortcut
    let blocks_cmd = PALETTE_COMMANDS
        .iter()
        .find(|c| c.slash_name == "/blocks")
        .expect("/blocks must be present in PALETTE_COMMANDS");
    assert_eq!(blocks_cmd.shortcut, Some("F6"));

    // 3. /tasks must be present with F7 shortcut (consolidating /processes and /dev)
    let tasks_cmd = PALETTE_COMMANDS
        .iter()
        .find(|c| c.slash_name == "/tasks")
        .expect("/tasks must be present in PALETTE_COMMANDS");
    assert_eq!(tasks_cmd.shortcut, Some("F7"));

    // 4. Redundant and duplicate commands must NOT be in the spotlight palette
    for removed in &[
        "/plan",
        "/config",
        "/configure",
        "/session",
        "/processes",
        "/dev",
        "/drift",
        "/power",
    ] {
        assert!(
            !palette_names.contains(removed),
            "{} duplicate must be removed from PALETTE_COMMANDS",
            removed
        );
    }
}

#[test]
fn test_command_catalog_items_cleaned_and_aligned() {
    let catalog_names: Vec<&str> = COMMAND_CATALOG_ITEMS.iter().map(|c| c.name).collect();

    // 1. /todo must be present with F8 shortcut
    let todo_item = COMMAND_CATALOG_ITEMS
        .iter()
        .find(|c| c.name == "/todo")
        .expect("/todo must be present in COMMAND_CATALOG_ITEMS");
    assert_eq!(todo_item.shortcut, "F8");

    // 2. /blocks must be present with F6
    let blocks_item = COMMAND_CATALOG_ITEMS
        .iter()
        .find(|c| c.name == "/blocks")
        .expect("/blocks must be present in COMMAND_CATALOG_ITEMS");
    assert_eq!(blocks_item.shortcut, "F6");

    // 3. /tasks must be present with F7
    let tasks_item = COMMAND_CATALOG_ITEMS
        .iter()
        .find(|c| c.name == "/tasks")
        .expect("/tasks must be present in COMMAND_CATALOG_ITEMS");
    assert_eq!(tasks_item.shortcut, "F7");

    // 4. Consolidated duplicates must NOT be in the catalog
    for removed in &[
        "/plan",
        "/config",
        "/configure",
        "/session",
        "/processes",
        "/dev",
        "/vault",
        "/todo-style",
        "/power",
    ] {
        assert!(
            !catalog_names.contains(removed),
            "{} duplicate must be removed from COMMAND_CATALOG_ITEMS",
            removed
        );
    }
}
