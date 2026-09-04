use minicode::agent::prompt::PromptBuilder;
use minicode::git::GitStatus;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_static_system_prompt_is_immutable_and_cacheable() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    // 1. Static prompt on turn 1
    let prompt1 = PromptBuilder::build_static_system_prompt(workspace, None);

    // 2. Static prompt on turn 2 (with same workspace)
    let prompt2 = PromptBuilder::build_static_system_prompt(workspace, None);

    // Byte-identical guarantee for prefix prompt caching
    assert_eq!(prompt1, prompt2);

    // Core Axioms
    assert!(prompt1.contains("Karpathy Guidelines"));
    assert!(prompt1.contains("Think Before Coding"));
    assert!(prompt1.contains("Simplicity First (The Ponytail Minimalist Ladder)"));
    assert!(prompt1.contains("Surgical Changes"));
    assert!(prompt1.contains("Goal-Driven Verification"));

    // Positive Directives
    assert!(prompt1.contains("Read Before Write"));
    assert!(prompt1.contains("Surgical Search-and-Replace"));
    assert!(prompt1.contains("Pre-Action Thought"));
    assert!(prompt1.contains("Positive Error Handling"));

    // Ensure zero dynamic turn markers in static prompt
    assert!(!prompt1.contains("<workspace_context>"));
    assert!(!prompt1.contains("<task_anchor>"));
    assert!(!prompt1.contains("<git_status"));
}

#[test]
fn test_static_system_prompt_respects_agents_md() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    let agents_md = workspace.join("AGENTS.md");
    let mut file = File::create(&agents_md).unwrap();
    writeln!(
        file,
        "# Project Rules\n- Strictly avoid unwrap\n- Use tracing macros"
    )
    .unwrap();

    let prompt = PromptBuilder::build_static_system_prompt(workspace, None);
    assert!(prompt.contains("# Repository Guidelines (AGENTS.md):"));
    assert!(prompt.contains("Strictly avoid unwrap"));
    assert!(prompt.contains("Use tracing macros"));
}

#[test]
fn test_recency_context_formats_git_and_working_set() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    let git_status = GitStatus {
        branch: "main".to_string(),
        is_clean: false,
        staged: vec!["src/lib.rs".to_string()],
        unstaged: vec!["src/agent/loop.rs".to_string()],
        untracked: vec!["tests/new_feature.rs".to_string()],
        conflicted: vec![],
    };

    let working_set = vec![
        "src/agent/loop.rs".to_string(),
        "src/agent/prompt.rs".to_string(),
    ];

    let anchor =
        "Active Objective: Refactor prompt architecture to Tri-Zone Model\nStep 1/2: In progress";

    let recency = PromptBuilder::build_recency_context(
        workspace,
        Some(anchor),
        &working_set,
        Some(&git_status),
    );

    assert!(recency.contains("<workspace_context>"));
    assert!(recency.contains("<git_status branch=\"main\" clean=\"false\">"));
    assert!(recency.contains("<file path=\"src/lib.rs\" />"));
    assert!(recency.contains("<file path=\"src/agent/loop.rs\" />"));
    assert!(recency.contains("<file path=\"tests/new_feature.rs\" />"));
    assert!(recency.contains("<active_working_set>"));
    assert!(recency.contains("<file path=\"src/agent/prompt.rs\" />"));
    assert!(recency.contains("<task_anchor>"));
    assert!(recency.contains("Refactor prompt architecture to Tri-Zone Model"));
    assert!(recency.contains("</workspace_context>"));
}

#[test]
fn test_legacy_build_system_prompt_backward_compatibility() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    // build_system_prompt still returns a valid prompt containing workspace
    let prompt = PromptBuilder::build_system_prompt(workspace, Some("Additional rule"));
    assert!(prompt.contains("You are minicode"));
    assert!(prompt.contains("Additional rule"));

    // build_system_prompt_with_anchor still works for existing callers
    let anchor = "# Session Memory Anchor (Persistent Context):\nActive Goal: Test backward compat";
    let prompt_anchor =
        PromptBuilder::build_system_prompt_with_anchor(workspace, None, Some(anchor));
    assert!(prompt_anchor.contains("Active Goal: Test backward compat"));
}
