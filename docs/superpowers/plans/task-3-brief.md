# Task 3 Brief: Tool Schema Upgrade & Registry Dispatch in `swarms.rs`

## Overview
Upgrade the `fanout_subagents` tool primitive in `src/tools/registry/agent_tools/swarms.rs` to support the modern Phase 134 parallel swarm orchestration engine:
- Bounded concurrency with `max_concurrency`.
- Join policies: `all` and `race`.
- Sequential merge arbitration via `auto_merge`.
- Modern role presets (`scout`, `coder`, `tester`, `reviewer`, `architect`, `security`, `researcher`, etc.) and `workspace_mode` (`auto`, `worktree`, `shared`).
- Seamless routing of `fanout_subagents` execution to `FanoutOrchestrator::execute_fanout`.

## Files to Modify:
- `src/tools/registry/agent_tools/swarms.rs`

## Interfaces & Requirements:

### 1. Update `ToolSchema` for `fanout_subagents`:
In `src/tools/registry/agent_tools/swarms.rs` `get_schemas()`:
Update `fanout_subagents` schema:
```json
{
  "name": "fanout_subagents",
  "description": "Concurrently dispatch a batch of specialized subagents across isolated Git Worktrees or shared repository threads. Supports race-to-first-success or all-worker join policies, sequential conflict-free merge arbitration, and executive map-reduce reporting.",
  "parameters": {
    "type": "object",
    "properties": {
      "tasks": {
        "type": "array",
        "description": "List of subagent task specifications to execute concurrently",
        "items": {
          "type": "object",
          "properties": {
            "task": {
              "type": "string",
              "description": "Detailed task instructions or prompt for this worker"
            },
            "prompt": {
              "type": "string",
              "description": "Alias for task"
            },
            "role": {
              "type": "string",
              "enum": ["scout", "coder", "tester", "reviewer", "architect", "security", "researcher", "code_reviewer", "test_engineer", "security_auditor", "custom"],
              "description": "Specialized role preset defining worker capabilities and workspace isolation (default: coder)"
            },
            "workspace_mode": {
              "type": "string",
              "enum": ["auto", "worktree", "shared"],
              "description": "Workspace isolation mode (default: auto)"
            },
            "max_iterations": {
              "type": "integer",
              "description": "Maximum autonomous tool iteration steps for this worker"
            },
            "check_cmd": {
              "type": "string",
              "description": "Custom validation command to run before merge (e.g. 'cargo check', or 'skip')"
            }
          },
          "required": ["task"]
        }
      },
      "join_mode": {
        "type": "string",
        "enum": ["all", "race"],
        "description": "Join policy: 'all' awaits all workers; 'race' cancels remaining workers upon first success (default: 'all')"
      },
      "auto_merge": {
        "type": "boolean",
        "description": "If true, sequentially arbitrates and merges successful mutating worktrees into current branch (default: false)"
      },
      "max_concurrency": {
        "type": "integer",
        "description": "Maximum concurrent workers running simultaneously (default: 4, min: 1, max: 16)"
      }
    },
    "required": ["tasks"]
  }
}
```

### 2. Update `dispatch()` for `fanout_subagents`:
In `src/tools/registry/agent_tools/swarms.rs` `dispatch()`:
- Parse `tasks_arr`:
  - For each element in `tasks_arr`:
    - Read `task` (or fallback to `prompt`). If neither exists or is empty, return `ToolError::InvalidArguments`.
    - Read `role`: parse string using `SubagentRole::from_str_loose` (defaults to `SubagentRole::Coder` if missing).
    - Read `workspace_mode`: parse optional string `"worktree"` -> `Some(WorkspaceMode::Worktree)`, `"shared"` -> `Some(WorkspaceMode::Shared)`, `"auto"` -> `Some(WorkspaceMode::Auto)`, else `None`.
    - Read `max_iterations`: optional integer (`param::opt_u64`).
    - Read `check_cmd`: optional string (`param::opt_str`).
    - Construct `FanoutTaskItem`.
- Parse `join_mode`:
  - If string == `"race"`, use `FanoutJoinMode::Race`, else `FanoutJoinMode::All`.
- Parse `auto_merge`:
  - Boolean flag via `param::opt_bool(args, "auto_merge", false)`.
- Parse `max_concurrency`:
  - Optional integer via `param::opt_u64(args, "max_concurrency").unwrap_or(4) as usize`, clamped to `1..=16`.
- Call:
  `FanoutOrchestrator::execute_fanout(workspace_root, tasks, join_mode, auto_merge, max_concurrency).await`
  and map Result to `Result<String>`.

### 3. Unit Tests in `src/tools/registry/agent_tools/swarms.rs`:
- `test_fanout_subagents_schema_structure`:
  - Validates `fanout_subagents` is registered in `get_schemas()`.
  - Validates required fields, parameters, and enum values.
- `test_fanout_subagents_argument_parsing`:
  - Tests parsing of JSON payload containing `tasks` (with `task` and `prompt` alias), `join_mode: "race"`, `auto_merge: true`, `max_concurrency: 8`.
- Ensure `test_total_tool_count` passes:
  `cargo test -j 1 --lib tools::tests::test_total_tool_count` (preserves 135 total tools).

## Constraints:
- ONLY run targeted tests:
  `cargo test -j 1 --lib tools::registry::agent_tools::swarms::tests`
  `cargo test -j 1 --lib tools::tests::test_total_tool_count`
- Zero `.unwrap()` or `.expect()` in non-test code.
- Run `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`.
- Commit with message: `feat(tools): upgrade fanout_subagents tool primitive with modern swarm orchestration (Phase 134)`.
- Write execution report to `docs/superpowers/plans/task-3-report.md`.
