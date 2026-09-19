# Task 3 Brief: Subagent Tool Primitives & Process Orchestrator

## Scope & Objective
Implement the `SubagentOrchestrator` in `src/agent/subagent/orchestrator.rs` and register the `spawn_subagent` tool primitive along with enhanced `send_message` and `manage_subagents` in `src/tools/registry/agent_tools/subagents.rs`. Connect them with `GitWorktreeManager` for worktree isolation and `AgentMailbox` for durable messaging. Update `TOTAL_TOOL_COUNT` to 135 in `src/constants.rs`.

## Files to Create/Modify
- Create: `src/agent/subagent/orchestrator.rs`
- Modify: `src/agent/subagent/mod.rs` (export `pub mod orchestrator;`)
- Modify: `src/tools/registry/agent_tools/subagents.rs` (add `spawn_subagent` schema & dispatch wiring, enhance `send_message` with `AgentMailbox` posting)
- Modify: `src/constants.rs` (update `TOTAL_TOOL_COUNT` to 135)

## Specifications & Requirements

### 1. `src/agent/subagent/orchestrator.rs`
- Implement `SubagentOrchestrator`:
  - `pub async fn spawn_subagent(workspace_root: &Path, task: &str, role: SubagentRole, workspace_mode: WorkspaceMode, max_iterations: Option<usize>) -> Result<String, crate::error::ToolError>`:
    1. Generate child `AgentId::new_subagent(role.as_str())`.
    2. Determine whether to isolate via Git worktree:
       - If `workspace_mode == WorkspaceMode::Worktree` or (`workspace_mode == WorkspaceMode::Auto` && role.default_workspace_mode() == WorkspaceMode::Worktree), attempt `GitWorktreeManager::create_worktree(workspace_root, &agent_id)`.
       - If worktree succeeds, target directory is `worktree_handle.worktree_path`.
       - If worktree is not used or fails gracefully (e.g. non-git directory), target directory is `workspace_root.to_path_buf()`.
    3. Initialize child agent mailbox directory `.minicode/agents/<agent_id>/`.
    4. Post initial `AgentMessage` with `intent: MessageIntent::TaskInit` from `AgentId::parent()` to child mailbox.
    5. Spawn headless child `minicode` process:
       - Command: `std::env::current_exe()?`
       - Arguments:
         `["run", "-d", target_dir.to_str(), "-y", "--json-stream", "--tools", role.tool_filter_mode()]`
         If `max_iterations` provided: `--max-iterations <N>`
         Task arg: `task`
       - Set `stdout(Stdio::piped())`, `stderr(Stdio::piped())`, `kill_on_drop(true)`.
       - On Unix: configure `process_group(0)`.
    6. Asynchronously read lines from child stdout using `tokio::io::BufReader`:
       - Parse NDJSON lines into events (track tokens used, tools executed, stream deltas, turn ends).
    7. Await child completion or timeout (e.g. 120s):
       - If child succeeded:
         - If worktree was used, capture diff with `GitWorktreeManager::capture_diff(&handle)` and remove worktree with `GitWorktreeManager::remove_worktree(&handle)`.
         - Format a clean structured report containing subagent ID, role badge, tokens used, tools executed, files modified/diff, and summary.
         - Return formatted report string.
       - If child failed or timed out:
         - Clean up worktree if one was created.
         - Return informative `ToolError::ExecutionFailed`.

  - `pub fn send_message(workspace_root: &Path, sender: &AgentId, recipient: &AgentId, message: &str, intent: Option<MessageIntent>) -> Result<String, crate::error::ToolError>`:
    1. Resolve recipient agent directory:
       - If `recipient.is_parent()`: `workspace_root.join(".minicode").join("agents").join("parent")`.
       - Else: `workspace_root.join(".minicode").join("agents").join(&recipient.0)`.
    2. Create or load `AgentMailbox::new(recipient.clone(), &agent_dir)`.
    3. Construct `AgentMessage` with unique UUID, `sender`, `recipient`, `intent.unwrap_or(MessageIntent::StatusUpdate)`, `message`, and current timestamp.
    4. Call `mailbox.post(msg)`.
    5. Return confirmation string: `"✔ Message delivered to agent `<recipient>` mailbox."`.

### 2. `src/tools/registry/agent_tools/subagents.rs`
- Add `spawn_subagent` schema to `get_schemas()`:
  - `name`: `"spawn_subagent"`
  - `description`: `"Spawn an autonomous background subagent with a specialized role ('scout', 'coder', 'tester', 'reviewer') to execute a scoped subtask in an isolated workspace or worktree."`
  - `parameters`:
    - `task` (string, required): Task instructions.
    - `role` (string, required): Enum `["scout", "coder", "tester", "reviewer"]`.
    - `workspace_mode` (string, optional): Enum `["auto", "worktree", "shared"]`.
    - `max_iterations` (integer, optional): Maximum tool loop iterations.
- In `dispatch()`:
  - Map `"spawn_subagent"` to invoke `SubagentOrchestrator::spawn_subagent(...)`.
  - In `"send_message"`:
    - Extract `recipient` (or `subagent_id`), `message`, and optional `intent`.
    - Invoke `SubagentOrchestrator::send_message(...)`.
  - In `"manage_subagents"`:
    - Support `"list"`, `"status"`, `"await"`/`"wait"`, `"kill"`.

### 3. `src/constants.rs`
- Update `TOTAL_TOOL_COUNT` constant:
  - Currently 134. With `spawn_subagent` added to `subagents.rs`, increment by 1 -> `pub const TOTAL_TOOL_COUNT: usize = 135;`.

### 4. Unit Tests Required
In `src/agent/subagent/orchestrator.rs` and `src/tools/registry/agent_tools/subagents.rs`:
- `test_spawn_subagent_schema_valid`: verify `spawn_subagent` exists in `ToolRegistry::get_tool_schemas()`.
- `test_total_tool_count_matches`: ensure `ToolRegistry::get_tool_schemas().len() == TOTAL_TOOL_COUNT` passes.
- `test_send_message_routes_to_mailbox`: verify `send_message` creates and appends to target agent's `mailbox.jsonl`.

## Critical Constraints
1. ONLY run targeted tests:
   - `cargo test -j 1 --lib agent::subagent::orchestrator::tests`
   - `cargo test -j 1 --lib tools::tests::test_total_tool_count`
   NEVER run the full test suite.
2. Error handling: `ToolError` / `Result<T, ToolError>`. Zero `.unwrap()` or `.expect()` in non-test code.
3. Concurrency: Use `-j 1` for `cargo check` and `cargo test`.
4. Verification: `cargo fmt` and `cargo clippy -j 1 --bin minicode -- -D warnings`.
5. Commit message: `feat(tools): add spawn_subagent primitive and wire orchestrator with worktree and mailbox (Phase 132)`.
