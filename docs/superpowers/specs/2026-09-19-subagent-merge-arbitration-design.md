# Subagent Merge & Conflict Arbitration Engine — Design Specification

> **Phase 133 Architecture Specification**  
> **Topic:** Autonomous Subagent Merge & Conflict Arbitration Engine (`merge_subagent_worktree`)  
> **Status:** Draft / Ready for Review  
> **Target Version:** `v0.3.34` (136 Tools)

---

## 1. Executive Summary & Problem Statement

In `minicode v0.3.33` (Phase 132), subagents running in `coder` or `tester` roles execute inside isolated, ephemeral Git worktrees located at `.minicode/worktrees/subagent-<id>`. Upon completing their task, the child process generates a Git diff (`git diff HEAD`) and sends structured findings back to the parent agent.

However, landing these changes currently requires either manual user intervention or raw git commands run via `exec_cmd`. This introduces significant friction and failure modes:
1. **Unverified Regressions:** The parent may land broken code without verifying that compiler checks or unit tests pass inside the worktree.
2. **Merge Conflicts:** If the parent workspace or another subagent touched the same files, direct patch application can cause silent corruptions or collision aborts.
3. **Orphaned Worktrees:** Ephemeral worktrees might linger if the child is terminated early or if the parent does not cleanly prune them.

Phase 133 introduces the **Autonomous Subagent Merge & Conflict Arbitration Engine** with a dedicated primitive tool: `merge_subagent_worktree`.

---

## 2. Goals & Non-Goals

### 2.1. Goals
- **Autonomous & Safe Landing:** Enable the parent agent or human to validate and merge subagent worktree branches with a single high-level tool call.
- **Pre-Merge Sandboxed Verification:** Automatically detect and execute compilation and test checks (`cargo check -j 1`, `bun test`, `npm test`, `pytest`) inside the worktree *before* touching the parent workspace.
- **Conflict Arbitration & Fast-Fail Diagnostics:** Perform 3-way merge conflict detection before modifying the parent working tree. If conflicts occur, return rich diagnostics so the parent can autonomously instruct the subagent to rebase or self-heal via `send_message`.
- **Configurable Landing Semantics:** Support both automated Git commits (`commit: true`) with descriptive commit messages and direct working tree patch application (`commit: false`).
- **Guaranteed Cleanup:** Automatically prune the worktree directory and delete the temporary branch upon successful landing.

### 2.2. Non-Goals
- Complex interactive terminal GUI conflict editors (handled via existing diff/editor tooling).
- Long-lived parallel branching beyond the lifetime of the subagent delegation.

---

## 3. Architecture & Component Design

```
                  ┌──────────────────────────────────────────────────────────┐
                  │                      Parent Agent                        │
                  │  1. spawn_subagent(role: "coder")                       │
                  │  2. Reviews diff returned in tool result                 │
                  │  3. merge_subagent_worktree(id, commit: true, check_cmd) │
                  └────────────────────────────┬─────────────────────────────┘
                                               │
               ┌───────────────────────────────┴───────────────────────────────┐
               ▼                                                               ▼
┌───────────────────────────────┐                             ┌─────────────────────────────────┐
│     GitWorktreeManager        │                             │      Arbitration Engine         │
│  - Locates worktree & branch  │                             │  - Auto-detects validation cmd   │
│  - Captures diff / patch      │ ─── Runs Pre-merge Check ──►│  - Executes check in worktree   │
│  - Cleans up worktree & branch│                             │  - Validates 3-way mergeability │
└───────────────────────────────┘                             └────────────────┬────────────────┘
                                                                               │
                                    ┌──────────────────────────────────────────┴────────────────────────┐
                                    ▼                                                                   ▼
                         [Check or Merge Fails]                                               [All Checks Pass]
                                    │                                                                   │
                                    ▼                                                                   ▼
                      Structured Error Output:                                          Apply Changes to Parent:
                      - Compiler stderr / test output                                   - If commit: true -> git commit
                      - Conflicted files list                                           - If commit: false -> apply patch
                      - Suggestions for parent loop                                     - Clean up worktree directory
```

### 3.1. New Module: `src/sandbox/arbitration.rs`

This module provides the core verification, conflict detection, and merge execution primitives:

```rust
pub struct MergeArbitrator;

pub struct ValidationReport {
    pub success: bool,
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

pub struct MergeabilityReport {
    pub can_merge_cleanly: bool,
    pub conflicted_files: Vec<String>,
    pub merge_base: Option<String>,
}

pub struct MergeSuccessReport {
    pub subagent_id: String,
    pub branch_name: String,
    pub files_changed: Vec<String>,
    pub committed: bool,
    pub commit_hash: Option<String>,
    pub commit_message: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ArbitrationError {
    #[error("Subagent worktree not found for ID: {0}")]
    WorktreeNotFound(String),

    #[error("Pre-merge verification failed (command: '{command}', exit: {exit_code}):\n{stderr}")]
    VerificationFailed {
        command: String,
        exit_code: i32,
        stdout: String,
        stderr: String,
    },

    #[error("Merge conflicts detected in files: {conflicted_files:?}")]
    MergeConflict {
        conflicted_files: Vec<String>,
        diff_preview: String,
    },

    #[error("Git execution failed: {0}")]
    GitError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
```

#### Functions in `MergeArbitrator`:
1. `detect_project_validation_cmd(worktree_path: &Path) -> Option<Vec<String>>`:
   - Checks `Cargo.toml` -> `vec!["cargo", "check", "-j", "1"]`
   - Checks `package.json` -> detects if `bun.lockb` exists -> `bun test`, `pnpm-lock.yaml` -> `pnpm test`, else `npm test`
   - Checks `pyproject.toml` or `pytest.ini` -> `pytest`
   - Checks `go.mod` -> `go test ./...`
2. `verify_worktree(worktree_path: &Path, custom_cmd: Option<&str>) -> Result<ValidationReport, ArbitrationError>`:
   - If `custom_cmd == Some("skip")`, returns immediate success.
   - Otherwise spawns the command in `worktree_path` with a 60s timeout, capturing stdout/stderr without corrupting terminal alternate screen.
3. `check_mergeability(repo_root: &Path, branch_name: &str) -> Result<MergeabilityReport, ArbitrationError>`:
   - Uses `git merge-tree $(git merge-base HEAD <branch>) HEAD <branch>` or `git merge --no-commit --no-ff <branch>` test to verify conflict-free mergeability without dirtying working directory.
4. `apply_merge(repo_root: &Path, branch_name: &str, commit: bool, commit_msg: Option<&str>) -> Result<MergeSuccessReport, ArbitrationError>`:
   - Commits or checks out changes into main working tree.

---

### 3.2. Worktree Persistence Configuration in `src/sandbox/worktree.rs`

In Phase 132, child subagents automatically removed worktrees when their run concluded. In Phase 133:
- For `coder` and `tester` roles, the worktree is preserved upon subagent process exit so the parent agent can run `merge_subagent_worktree`.
- Once `merge_subagent_worktree` succeeds (or if the user explicitly cancels via `manage_subagents`), `GitWorktreeManager::remove_worktree` is invoked.
- `GitWorktreeManager::cleanup_stale_worktrees` continues to run on startup to prune any abandoned worktrees.

---

### 3.3. Tool Primitive: `merge_subagent_worktree` in `src/tools/registry/agent_tools/subagents.rs`

```json
{
  "name": "merge_subagent_worktree",
  "description": "Validates and merges code changes from a completed subagent's ephemeral git worktree into the main workspace. Automatically runs project build/test checks before landing changes.",
  "parameters": {
    "type": "object",
    "properties": {
      "subagent_id": {
        "type": "string",
        "description": "The unique ID of the subagent whose worktree changes should be merged (e.g. 'coder-7a7098ec')."
      },
      "commit": {
        "type": "boolean",
        "description": "Whether to create a git commit in the main workspace upon successful merge (default: true). If false, changes are applied to the working directory without committing."
      },
      "check_cmd": {
        "type": "string",
        "description": "Optional explicit validation command to run inside the worktree (e.g. 'cargo test -j 1 --test auth'). If omitted, automatically detects from project manifests (Cargo.toml -> 'cargo check -j 1', package.json -> 'npm test', etc.). Set to 'skip' to bypass validation."
      },
      "commit_message": {
        "type": "string",
        "description": "Custom git commit message. If omitted, defaults to 'merge(subagent-<id>): land verified subagent changes'."
      }
    },
    "required": ["subagent_id"]
  }
}
```

- Increments `TOTAL_TOOL_COUNT` from 135 to 136 in `src/constants.rs`.
- Registers schema in `ToolRegistry::get_tool_schemas()` and handler in `dispatch_tool`.

---

## 4. Error Handling & Diagnostics

When validation fails or conflicts occur, the parent agent receives actionable feedback:

```markdown
❌ Subagent Merge Failed: Pre-Merge Verification Error
• Subagent: coder-4b2a91f0
• Validation Command: cargo check -j 1
• Exit Code: 101

Compiler Output:
error[E0425]: cannot find function `hash_password` in module `crypto`
  --> src/user_service.rs:42:15
   |
42 |     let h = crypto::hash_password(pwd);
   |                     ^^^^^^^^^^^^^ not found in `crypto`

Actionable Next Step:
The parent workspace remains 100% clean and unmodified.
You can send feedback to the subagent to resolve this issue:
`send_message(recipient: "coder-4b2a91f0", message: "Fix compiler error: crypto::hash_password not found", intent: "feedback")`
```

---

## 5. Quality & Resource Constraints

1. **Targeted Testing Only:** Never run full workspace tests; run `cargo test -j 1 --lib sandbox::arbitration::tests` and `cargo test -j 1 --test integration_subagent_merge`.
2. **Concurrency Floors:** `-j 1` on checks and tests, `-j 2` on builds.
3. **Zero Unwraps:** Pure Rust, zero `.unwrap()` or `.expect()` in non-test code.
4. **Clean Worktree Teardown:** Never leave orphaned git worktrees or dangling temporary branches.
