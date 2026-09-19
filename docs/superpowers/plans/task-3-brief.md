# Task 3 Brief: `merge_subagent_worktree` Tool Primitive & Registry Integration

## Overview
Implement the `merge_subagent_worktree` autonomous tool primitive, allowing parent agents to validate, conflict-check, and land subagent worktree changes into the primary workspace with automatic cleanup.

## Files to Modify:
- `src/tools/registry/agent_tools/subagents.rs`
- `src/constants.rs` (ensure `TOTAL_TOOL_COUNT` matches live schema count: 135)

## Interfaces & Requirements:

### 1. `MergeSubagentWorktreeArgs` Struct
Define in `src/tools/registry/agent_tools/subagents.rs`:
```rust
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MergeSubagentWorktreeArgs {
    pub subagent_id: String,
    pub commit: Option<bool>,
    pub check_cmd: Option<String>,
    pub commit_message: Option<String>,
}
```

### 2. Update Tool Schema in `ToolRegistry::get_tool_schemas()`:
Update `merge_subagent_worktree` entry with full schema:
- `name`: `"merge_subagent_worktree"`
- `description`: `"Validates and merges code changes from a completed subagent's ephemeral git worktree into the main workspace. Automatically runs project build/test checks before landing changes."`
- Properties:
  - `subagent_id`: string (required)
  - `commit`: boolean (optional, default true)
  - `check_cmd`: string (optional, validation command or 'skip')
  - `commit_message`: string (optional custom commit message)

### 3. Tool Implementation & Dispatch:
In `src/tools/registry/agent_tools/subagents.rs`:
Implement `pub async fn merge_subagent_worktree(workspace_root: &Path, subagent_id: &str, commit: bool, check_cmd: Option<&str>, commit_message: Option<&str>) -> Result<String, ToolError>`:
1. Locate worktree path via `GitWorktreeManager::locate_worktree(workspace_root, &AgentId(subagent_id.to_string()))`.
   If not found, return descriptive `ToolError::ExecutionFailed`.
2. Resolve source branch via `GitWorktreeManager::resolve_branch_for(workspace_root, &AgentId(subagent_id.to_string()))`.
3. Pre-merge verification via `MergeArbitrator::verify_worktree(&worktree_path, check_cmd)`.
   If verification fails, do NOT touch parent workspace or delete worktree; return structured failure report with command, exit code, stdout, and stderr.
4. Mergeability check via `MergeArbitrator::check_mergeability(workspace_root, &branch_name)`.
   If conflicts detected (`!mergeability.can_merge_cleanly`), do NOT touch parent workspace or delete worktree; return structured conflict report with list of conflicted files.
5. Apply merge via `MergeArbitrator::apply_merge(workspace_root, &branch_name, commit, commit_message)`.
6. On successful merge, clean up worktree and temporary branch via `GitWorktreeManager::remove_worktree`.
7. Return formatted markdown success summary detailing subagent ID, branch name, landing mode, files changed, commit hash (if committed), and pre-merge validation duration.

Wire into `dispatch_agent_tool` for `"merge_subagent_worktree"`.

### 4. Unit Tests in `src/tools/registry/agent_tools/subagents.rs`:
- Add `test_merge_subagent_worktree_arg_parsing`:
  ```rust
  #[test]
  fn test_merge_subagent_worktree_arg_parsing() {
      let json = serde_json::json!({
          "subagent_id": "coder-1",
          "commit": true,
          "check_cmd": "cargo check -j 1",
          "commit_message": "merge: auth feature"
      });
      let args: MergeSubagentWorktreeArgs = serde_json::from_value(json).unwrap();
      assert_eq!(args.subagent_id, "coder-1");
      assert_eq!(args.commit, Some(true));
      assert_eq!(args.check_cmd.as_deref(), Some("cargo check -j 1"));
      assert_eq!(args.commit_message.as_deref(), Some("merge: auth feature"));
  }
  ```
- Ensure `test_total_tool_count` passes (`cargo test -j 1 --lib tools::tests::test_total_tool_count`).

## Constraints:
- ONLY run targeted tests: `cargo test -j 1 --lib tools::registry::agent_tools::subagents::tests` and `cargo test -j 1 --lib tools::tests::test_total_tool_count`.
- Zero `.unwrap()` or `.expect()` in non-test code.
- Run `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`.
- Commit with message: `feat(tools): wire merge_subagent_worktree tool primitive with arbitration engine (Phase 133)`
- Write execution report to `docs/superpowers/plans/task-3-report.md`.
