use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "delegate_task".to_string(),
            description: "Delegate a subtask to an autonomous MiniPub child AI agent (`minipub`) in an isolated Git Worktree or shared workspace. Supports semantic intent categories (`category`: 'ultrabrain', 'visual-engineering', 'quick', 'explore', 'librarian', 'security-audit', 'test-verification'), domain skill injection (`load_skills`), Boomerang todo checklists (`todos`), and cooperative file boundaries (`file_boundaries`).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Clear and detailed task instructions for the MiniPub subagent"
                    },
                    "category": {
                        "type": "string",
                        "enum": [
                            "ultrabrain", "visual-engineering", "quick", "deep", "explore",
                            "librarian", "artistry", "writing", "security-audit", "test-verification"
                        ],
                        "description": "Optional semantic intent category that automatically selects the optimal MiniPub role, budget, and behavioral directives"
                    },
                    "role": {
                        "type": "string",
                        "description": "Optional explicit MiniPub role preset ('scout', 'architect', 'planner', 'designer', 'frontend', 'backend', 'coder', 'vision', 'content_writer', 'debugger', 'reviewer', 'security', 'tester', 'simplifier')"
                    },
                    "load_skills": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of specialized MiniVault skill names (e.g. ['rust-tokio', 'frontend-design']) to inject into the MiniPub worker"
                    },
                    "isolate_branch": {
                        "type": "boolean",
                        "description": "If true (default), creates a dedicated Git Worktree branch under .minicode/worktrees/ for isolation"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional explicit wall-clock timeout in seconds (otherwise governed by dynamic activity heartbeat)"
                    },
                    "todos": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional Boomerang todo checklist items assigned by the Orchestrator for the MiniPub worker to follow and report on"
                    },
                    "file_boundaries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional allowed file/directory path prefixes for cooperative write isolation"
                    }
                },
                "required": ["task"]
            }),
        },
        ToolSchema {
            name: "invoke_subagent".to_string(),
            description: "Invoke a specialized, capability-sandboxed MiniPub subagent worker (`minipub`: scout, architect, planner, designer, frontend, backend, coder, vision, content_writer, debugger, reviewer, security, tester, simplifier) or semantic category ('ultrabrain', 'visual-engineering', 'quick', 'explore', 'librarian') with automatic MiniVault skill injection and session wisdom inheritance.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "role": {
                        "type": "string",
                        "enum": [
                            "scout", "architect", "planner", "designer", "frontend", "backend",
                            "coder", "vision", "content_writer", "writer", "debugger", "reviewer",
                            "security", "tester", "simplifier",
                            "ultrabrain", "visual-engineering", "quick", "explore", "librarian",
                            "researcher", "code_reviewer", "test_engineer", "security_auditor", "custom"
                        ],
                        "description": "Specialized MiniPub role preset or semantic intent category"
                    },
                    "category": {
                        "type": "string",
                        "description": "Optional semantic intent category ('ultrabrain', 'visual-engineering', 'quick', 'explore', 'librarian', 'security-audit', 'test-verification')"
                    },
                    "load_skills": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of specialized MiniVault skill names to inject into this MiniPub worker"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Clear and detailed task instructions for the MiniPub subagent worker"
                    },
                    "provider": {
                        "type": "string",
                        "description": "Optional provider override for this MiniPub (e.g. 'anthropic', 'openai', 'gemini', 'openrouter', 'deepseek', 'groq', 'ollama')"
                    },
                    "model": {
                        "type": "string",
                        "description": "Optional model override for this MiniPub (supports 'provider:model' or 'model')"
                    },
                    "fallback_1": {
                        "type": "string",
                        "description": "Optional 1st fallback 'provider:model' if Primary fails or hits rate limits"
                    },
                    "fallback_2": {
                        "type": "string",
                        "description": "Optional 2nd fallback 'provider:model' if 1st Fallback fails"
                    },
                    "use_orchestrator_model": {
                        "type": "boolean",
                        "description": "Force this MiniPub to use the Orchestrator's active Primary + Fallback model chain"
                    },
                    "token_budget": {
                        "type": "integer",
                        "description": "Maximum token budget for the subagent task"
                    },
                    "max_turns": {
                        "type": "integer",
                        "description": "Maximum tool execution turns before finalizing"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "Optional custom system prompt override"
                    },
                    "isolate_worktree": {
                        "type": "boolean",
                        "description": "Whether to run inside an isolated Git Worktree branch. Default: false for read-only roles (scout, architect, planner, vision, reviewer, security), true for mutating roles (designer, frontend, backend, coder, content_writer, debugger, tester, simplifier)."
                    },
                    "todos": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional Boomerang todo checklist items assigned by the Orchestrator"
                    },
                    "file_boundaries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional allowed file/directory path prefixes for cooperative write isolation"
                    }
                },
                "required": ["role", "prompt"]
            }),
        },
        ToolSchema {
            name: "dispatch_subagent".to_string(),
            description: "Dispatch an autonomous background MiniPub subagent worker (`minipub`: scout, architect, planner, designer, frontend, backend, coder, vision, content_writer, debugger, reviewer, security, tester, simplifier) or semantic category ('ultrabrain', 'visual-engineering', 'quick', 'explore', 'librarian') concurrently in the background (up to max_parallel=4 by default) with automatic MiniVault skill injection and session wisdom inheritance.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "role": {
                        "type": "string",
                        "enum": [
                            "scout", "architect", "planner", "designer", "frontend", "backend",
                            "coder", "vision", "content_writer", "writer", "debugger", "reviewer",
                            "security", "tester", "simplifier",
                            "ultrabrain", "visual-engineering", "quick", "explore", "librarian",
                            "researcher", "code_reviewer", "test_engineer", "security_auditor", "custom"
                        ],
                        "description": "Specialized MiniPub role preset or semantic intent category"
                    },
                    "category": {
                        "type": "string",
                        "description": "Optional semantic intent category ('ultrabrain', 'visual-engineering', 'quick', 'explore', 'librarian', 'security-audit', 'test-verification')"
                    },
                    "load_skills": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of specialized MiniVault skill names to inject into this MiniPub worker"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Clear and detailed task instructions for the MiniPub subagent worker"
                    },
                    "provider": {
                        "type": "string",
                        "description": "Optional provider override for this MiniPub (e.g. 'anthropic', 'openai', 'gemini', 'openrouter', 'deepseek', 'groq', 'ollama')"
                    },
                    "model": {
                        "type": "string",
                        "description": "Optional model override for the subagent worker (supports 'provider:model' or 'model')"
                    },
                    "fallback_1": {
                        "type": "string",
                        "description": "Optional 1st fallback 'provider:model' if Primary fails or hits rate limits"
                    },
                    "fallback_2": {
                        "type": "string",
                        "description": "Optional 2nd fallback 'provider:model' if 1st Fallback fails"
                    },
                    "use_orchestrator_model": {
                        "type": "boolean",
                        "description": "Force this MiniPub to use the Orchestrator's active Primary + Fallback model chain"
                    },
                    "token_budget": {
                        "type": "integer",
                        "description": "Maximum token budget for the subagent task (default: 50,000)"
                    },
                    "max_turns": {
                        "type": "integer",
                        "description": "Maximum tool execution turns before finalizing"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "Optional custom system prompt override"
                    },
                    "isolate_worktree": {
                        "type": "boolean",
                        "description": "Whether to run inside an isolated Git Worktree branch. Default: false for read-only roles (scout, architect, planner, vision, reviewer, security), true for mutating roles (designer, frontend, backend, coder, content_writer, debugger, tester, simplifier)."
                    },
                    "todos": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional Boomerang todo checklist items assigned by the Orchestrator"
                    },
                    "file_boundaries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional allowed file/directory path prefixes for cooperative write isolation"
                    }
                },
                "required": ["role", "prompt"]
            }),
        },
        ToolSchema {
            name: "spawn_subagent".to_string(),
            description: "Spawn an autonomous background subagent (`minipub`) with a specialized role ('scout', 'architect', 'planner', 'designer', 'frontend', 'backend', 'coder', 'vision', 'content_writer', 'debugger', 'reviewer', 'security', 'tester', 'simplifier') to execute a scoped subtask and optional Boomerang todo checklist in an isolated workspace or worktree.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Clear and detailed task instructions for the subagent"
                    },
                    "role": {
                        "type": "string",
                        "enum": [
                            "scout", "architect", "planner", "designer", "frontend", "backend",
                            "coder", "vision", "content_writer", "writer", "debugger", "reviewer",
                            "security", "tester", "simplifier"
                        ],
                        "description": "Specialized role preset defining subagent capabilities and workspace isolation"
                    },
                    "workspace_mode": {
                        "type": "string",
                        "enum": ["auto", "worktree", "shared"],
                        "description": "Workspace isolation mode: 'auto' (default based on role), 'worktree' (isolated Git worktree branch), or 'shared' (in-place execution)"
                    },
                    "max_iterations": {
                        "type": "integer",
                        "description": "Maximum tool loop iterations for the subagent before terminating"
                    },
                    "todos": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional Boomerang todo checklist items assigned by the Orchestrator"
                    },
                    "file_boundaries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional allowed file/directory path prefixes for cooperative write isolation"
                    }
                },
                "required": ["task", "role"]
            }),
        },
        ToolSchema {
            name: "send_message".to_string(),
            description: "Send a typed message or follow-up instruction to a MiniPub subagent or parent Orchestrator via durable mailbox.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "recipient": {
                        "type": "string",
                        "description": "The unique identifier of the recipient agent (or 'parent' for coordinator)"
                    },
                    "subagent_id": {
                        "type": "string",
                        "description": "Alias for recipient: the unique identifier of the target subagent"
                    },
                    "message": {
                        "type": "string",
                        "description": "The instruction or message content to deliver"
                    },
                    "intent": {
                        "type": "string",
                        "enum": [
                            "task_init",
                            "status_update",
                            "clarification_request",
                            "clarification_response",
                            "feedback",
                            "handoff",
                            "task_complete"
                        ],
                        "description": "Optional high-level intent of the message (default: 'status_update')"
                    }
                },
                "required": ["message"]
            }),
        },
        ToolSchema {
            name: "manage_subagents".to_string(),
            description: "Inspect, list, monitor Boomerang todo progress, dynamically modify timeouts/budgets/todos/steer directives in-flight, await completion, or terminate active MiniPub (`minipub`) subagent workers in the swarm pool.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "status", "update", "modify", "configure", "await", "wait", "transcript", "drilldown", "kill", "kill_all"],
                        "description": "Management action: 'list' all MiniPub subagents, 'status' of specific subagent, 'update'/'modify'/'configure' a MiniPub worker or pool/stream timeouts in-flight, 'await'/'wait' for completion, 'transcript'/'drilldown' for step-by-step trace inspection, 'kill' a subagent, or 'kill_all'"
                    },
                    "subagent_id": {
                        "type": "string",
                        "description": "Identifier of the target MiniPub subagent (required for 'status', 'kill', 'transcript'; optional for 'await' and 'update'/'configure')"
                    },
                    "subagent_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of MiniPub worker IDs to watch when action is 'await' (if omitted along with subagent_id, watches all active workers)"
                    },
                    "wait_mode": {
                        "type": "string",
                        "enum": ["all", "any", "first", "race"],
                        "description": "Join policy when action is 'await': 'all' (default) waits for all watched workers; 'any'/'first'/'race' wakes immediately as soon as the first watched worker completes or fails"
                    },
                    "step_index": {
                        "type": "integer",
                        "description": "Optional step number (1-indexed) when action is 'transcript' or 'drilldown'"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Maximum seconds to wait when action is 'await', or new inactivity timeout (in seconds) when action is 'update'/'modify'/'configure'"
                    },
                    "stream_idle_timeout_secs": {
                        "type": "integer",
                        "description": "Dynamic model stream idle timeout in seconds (30-7200) for Orchestrator and MiniPub workers when action is 'update'/'modify'/'configure'"
                    },
                    "max_parallel": {
                        "type": "integer",
                        "description": "Dynamic MiniPub pool concurrency limit (1-16) when action is 'update'/'modify'/'configure'"
                    },
                    "token_budget": {
                        "type": "integer",
                        "description": "New token budget when action is 'update'/'modify'"
                    },
                    "max_turns": {
                        "type": "integer",
                        "description": "New maximum turns when action is 'update'/'modify'"
                    },
                    "todos": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Replace the MiniPub worker's todo checklist in-flight"
                    },
                    "append_todos": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Append additional todo checklist items to the MiniPub worker in-flight"
                    },
                    "complete_todo_ids": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Mark specific 1-indexed todo item IDs as completed"
                    },
                    "file_boundaries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Update cooperative file/directory boundary leases for the MiniPub worker"
                    },
                    "steer_prompt": {
                        "type": "string",
                        "description": "Live steering instruction injected into the running MiniPub worker on its next turn"
                    },
                    "provider": {
                        "type": "string",
                        "description": "Dynamically switch a running MiniPub's primary provider at runtime, or set the provider for 'target_role'"
                    },
                    "model": {
                        "type": "string",
                        "description": "Dynamically hot-swap a running MiniPub's primary model at runtime (supports 'provider:model' or 'model'), or set Slot 1 Primary for 'target_role'"
                    },
                    "fallback_1": {
                        "type": "string",
                        "description": "Set Slot 2 (1st Fallback 'provider:model') for a running MiniPub or 'target_role'"
                    },
                    "fallback_2": {
                        "type": "string",
                        "description": "Set Slot 3 (2nd Fallback 'provider:model') for a running MiniPub or 'target_role'"
                    },
                    "global_last_fallback": {
                        "type": "string",
                        "description": "Set Slot 4 (Global Last Fallback ★ 'provider:model') shared across the Orchestrator and ALL MiniPubs"
                    },
                    "use_orchestrator_model": {
                        "type": "boolean",
                        "description": "Dynamically switch a running MiniPub worker to use the Orchestrator's active Primary + Fallback model chain"
                    },
                    "target_role": {
                        "type": "string",
                        "description": "Target scope when configuring the 4-slot model chain: 'global' (Orchestrator + Default MiniPubs), 'all_minipubs', or any MiniPub role ('scout', 'architect', 'planner', 'designer', 'frontend', 'backend', 'coder', 'vision', 'content_writer', 'debugger', 'reviewer', 'security', 'tester', 'simplifier')"
                    },
                    "sync_all_minipubs": {
                        "type": "boolean",
                        "description": "When true, synchronizes all 14 MiniPubs to inherit the Global Orchestrator 4-slot model & fallback chain"
                    }
                },
                "required": ["action"]
            }),
        },
        ToolSchema {
            name: "scratchpad_write".to_string(),
            description: "Write or update an entry on the shared multi-agent scratchpad blackboard for inter-worker knowledge sharing.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Unique key identifier (e.g. 'api_specs', 'failing_tests', 'auth_plan')"
                    },
                    "title": {
                        "type": "string",
                        "description": "Short descriptive title of this finding or note"
                    },
                    "content": {
                        "type": "string",
                        "description": "Detailed text content, code snippet, or structured findings"
                    },
                    "author": {
                        "type": "string",
                        "description": "Optional author identifier (default: 'orchestrator')"
                    }
                },
                "required": ["key", "title", "content"]
            }),
        },
        ToolSchema {
            name: "scratchpad_read".to_string(),
            description: "Read a specific entry from the shared multi-agent scratchpad blackboard by key.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key identifier to read"
                    }
                },
                "required": ["key"]
            }),
        },
        ToolSchema {
            name: "scratchpad_list".to_string(),
            description: "List all active entries currently published on the shared multi-agent scratchpad blackboard.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "merge_subagent_worktree".to_string(),
            description: "Validates and merges code changes from a completed subagent's ephemeral git worktree into the main workspace. Automatically runs project build/test checks before landing changes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "subagent_id": {
                        "type": "string",
                        "description": "Unique identifier of the subagent whose worktree to merge (e.g. 'coder-1' or 'task-a1b2c3d4')"
                    },
                    "commit": {
                        "type": "boolean",
                        "description": "Whether to automatically commit the merged changes (default: true). If false, stages changes in the working tree without committing."
                    },
                    "check_cmd": {
                        "type": "string",
                        "description": "Optional pre-merge validation command to run in the worktree (e.g. 'cargo check -j 1' or 'skip'). Defaults to auto-detecting project check command."
                    },
                    "commit_message": {
                        "type": "string",
                        "description": "Optional custom commit message when commit is true"
                    }
                },
                "required": ["subagent_id"]
            }),
        },
        ToolSchema {
            name: "subagent_transcript_drilldown".to_string(),
            description: "Inspect the raw execution transcript of an isolated subagent worker. Allows lossless inspection of tool parameters, complete stdout/stderr, compiler logs, and step-by-step diagnostics on demand without bloating parent context.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "subagent_id": {
                        "type": "string",
                        "description": "Unique identifier of the subagent to inspect (e.g. 'researcher-1', 'testengineer-2')"
                    },
                    "step_index": {
                        "type": "integer",
                        "description": "Optional step number (1-indexed) to inspect in full detail. If omitted, returns an overview table of all steps."
                    },
                    "filter": {
                        "type": "string",
                        "enum": ["summary", "all", "errors", "tools", "exec"],
                        "description": "Filter mode: 'summary' (overview table), 'all' (all steps), 'errors' (only failed steps), 'tools' (all tools), or 'exec' (only command executions)"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Maximum output lines to display for a single step (default: 120)"
                    }
                },
                "required": ["subagent_id"]
            }),
        },
    ]
}

pub(crate) fn extract_opt_string_list(args: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    args.get(key).and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|item| item.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect()
    })
}

pub(crate) fn extract_string_list(args: &serde_json::Value, key: &str) -> Vec<String> {
    extract_opt_string_list(args, key).unwrap_or_default()
}

pub(crate) fn build_subagent_config_from_args(
    args: &serde_json::Value,
    role: crate::agent::subagent::SubagentRole,
    app_cfg: &crate::config::Config,
) -> (
    crate::agent::subagent::SubagentConfig,
    Vec<String>,
    Vec<String>,
) {
    let todos = extract_string_list(args, "todos");
    let file_boundaries = extract_string_list(args, "file_boundaries");
    let load_skills = extract_string_list(args, "load_skills");
    let category = param::opt_str(args, "category").map(|s| s.to_string());

    let mut config = crate::agent::subagent::SubagentConfig::for_role_with_minipub_config(
        role,
        &app_cfg.agent.minipub,
    )
    .with_todos(todos.clone())
    .with_file_boundaries(file_boundaries.clone())
    .with_category(category)
    .with_load_skills(load_skills);

    if let Some(prov) = param::opt_str(args, "provider") {
        config.provider = Some(prov.trim().to_lowercase());
    }
    if let Some(model) = param::opt_str(args, "model") {
        config.model = Some(model.to_string());
    }
    let def_prov = config
        .provider
        .as_deref()
        .unwrap_or(&app_cfg.provider.default);
    if let Some(f1) = param::opt_str(args, "fallback_1") {
        config.fallback_1 = crate::config::ModelSlot::parse_loose(f1, def_prov);
    }
    if let Some(f2) = param::opt_str(args, "fallback_2") {
        config.fallback_2 = crate::config::ModelSlot::parse_loose(f2, def_prov);
    }
    if let Some(use_orch) = param::get_bool(args, "use_orchestrator_model") {
        config.use_orchestrator_model = use_orch;
    }
    if let Some(t_secs) =
        param::opt_u64(args, "timeout_secs").or_else(|| param::opt_u64(args, "duration_seconds"))
    {
        config.inactivity_timeout_secs = t_secs;
    }
    if let Some(budget) = param::opt_u64(args, "token_budget") {
        config.token_budget = budget as usize;
    }
    if let Some(max_t) = param::opt_u64(args, "max_turns") {
        config.max_turns = max_t as usize;
    }
    if let Some(sys_prompt) = param::opt_str(args, "system_prompt") {
        config.system_prompt_override = Some(sys_prompt.to_string());
    }

    (config, todos, file_boundaries)
}

pub(crate) fn build_minipub_update_from_args(
    args: &serde_json::Value,
    def_prov: &str,
) -> crate::agent::subagent::MiniPubUpdate {
    let complete_todo_ids = args
        .get("complete_todo_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_u64().map(|n| n as usize))
                .collect::<Vec<usize>>()
        })
        .unwrap_or_default();

    crate::agent::subagent::MiniPubUpdate {
        inactivity_timeout_secs: param::opt_u64(args, "timeout_secs")
            .or_else(|| param::opt_u64(args, "duration_seconds")),
        token_budget: param::opt_u64(args, "token_budget").map(|n| n as usize),
        max_turns: param::opt_u64(args, "max_turns").map(|n| n as usize),
        todos: extract_opt_string_list(args, "todos"),
        append_todos: extract_string_list(args, "append_todos"),
        complete_todo_ids,
        file_boundaries: extract_opt_string_list(args, "file_boundaries"),
        steer_prompt: param::opt_str(args, "steer_prompt")
            .or_else(|| param::opt_str(args, "prompt"))
            .map(|s| s.to_string()),
        provider: param::opt_str(args, "provider").map(|s| s.to_string()),
        model: param::opt_str(args, "model").map(|s| s.to_string()),
        fallback_1: param::opt_str(args, "fallback_1")
            .and_then(|s| crate::config::ModelSlot::parse_loose(s, def_prov)),
        fallback_2: param::opt_str(args, "fallback_2")
            .and_then(|s| crate::config::ModelSlot::parse_loose(s, def_prov)),
        use_orchestrator_model: param::get_bool(args, "use_orchestrator_model"),
    }
}

pub(crate) async fn execute_await_workers(
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Result<String> {
    let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
    let mut target_ids = extract_opt_string_list(args, "subagent_ids")
        .or_else(|| extract_opt_string_list(args, "ids"))
        .unwrap_or_default();
    if let Some(single_id) = param::opt_str(args, "subagent_id")
        .or_else(|| param::opt_str(args, "id"))
        .or_else(|| param::opt_str(args, "task_id"))
        .or_else(|| param::opt_str(args, "target"))
    {
        let trimmed = single_id.trim();
        if !trimmed.is_empty() && trimmed != "all" && !target_ids.iter().any(|t| t == trimmed) {
            target_ids.push(trimmed.to_string());
        }
    }
    let wait_mode = param::opt_str(args, "wait_mode").unwrap_or("all");
    let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
    let timeout_secs = param::opt_u64(args, "timeout_secs")
        .or_else(|| param::opt_u64(args, "duration_seconds"))
        .unwrap_or(app_cfg.agent.minipub.inactivity_timeout_secs);

    Ok(pool
        .await_workers(&target_ids, wait_mode, timeout_secs)
        .await)
}

pub(crate) fn execute_transcript_drilldown(
    id_raw: &str,
    step_index: Option<usize>,
    max_lines: Option<usize>,
    filter: &str,
    workspace_root: &Path,
    label: &str,
) -> Result<String> {
    let id = id_raw.strip_prefix("worker-").unwrap_or(id_raw);
    let store = crate::agent::subagent::transcript::get_global_transcript_store();
    if let Some(t) = store.get(id, Some(workspace_root)) {
        if let Some(step_idx) = step_index {
            if let Some(detail) = t.format_step_detail(step_idx, max_lines) {
                Ok(detail)
            } else {
                Ok(format!(
                    "ℹ Step {} not found in {} `{}` transcript. Total steps recorded: {}.",
                    step_idx,
                    label,
                    id,
                    t.steps.len()
                ))
            }
        } else if filter == "errors" || filter == "error" {
            Ok(t.format_errors())
        } else {
            Ok(t.format_overview(Some(50)))
        }
    } else {
        Ok(format!(
            "ℹ No execution transcript found for {} `{}`. Available: {:?}",
            label,
            id,
            store.list_subagent_ids(Some(workspace_root))
        ))
    }
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "delegate_task" => Some(async {
            let task = param::require_str_with_aliases(
                args,
                &["task", "prompt", "instructions"],
                "delegate_task",
            )?;
            let category_opt = param::opt_str(args, "category").or_else(|| param::opt_str(args, "role"));
            let default_role = if let Some(cat_str) = category_opt {
                crate::agent::subagent::SubagentRole::from_str_loose(cat_str)
            } else if param::opt_bool(args, "isolate_branch", true) {
                crate::agent::subagent::SubagentRole::Coder
            } else {
                crate::agent::subagent::SubagentRole::Scout
            };
            let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
            let (config, _todos, file_boundaries) =
                build_subagent_config_from_args(args, default_role, &app_cfg);
            let isolate = param::get_bool(args, "isolate_branch")
                .or_else(|| param::get_bool(args, "isolate_worktree"))
                .unwrap_or(
                    file_boundaries.is_empty()
                        && default_role.default_workspace_mode()
                            == crate::agent::subagent::WorkspaceMode::Worktree,
                );
            let timeout_secs = param::opt_u64(args, "timeout_secs");

            let res = crate::agent::orchestrator::MultiAgentOrchestrator::delegate_with_config(
                workspace_root,
                task,
                isolate,
                timeout_secs,
                Some(config),
            )
            .await?;
            Ok(crate::agent::orchestrator::MultiAgentOrchestrator::format_result(&res))
        }.await),
        "invoke_subagent" => Some(async {
            let role_str = param::opt_str(args, "role")
                .or_else(|| param::opt_str(args, "category"))
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "invoke_subagent".to_string(),
                    reason: "Missing required argument 'role' (or 'category')".to_string(),
                })?;
            let prompt = param::require_str_with_aliases(
                args,
                &["prompt", "task"],
                "invoke_subagent",
            )?;

            let role = crate::agent::subagent::SubagentRole::from_str_loose(role_str);
            let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
            let (config, _todos, file_boundaries) =
                build_subagent_config_from_args(args, role, &app_cfg);

            let isolate_worktree = param::get_bool(args, "isolate_worktree").unwrap_or(
                file_boundaries.is_empty()
                    && role.default_workspace_mode()
                        == crate::agent::subagent::WorkspaceMode::Worktree,
            );

            let res =
                crate::agent::orchestrator::MultiAgentOrchestrator::delegate_with_config(
                    workspace_root,
                    prompt,
                    isolate_worktree,
                    None,
                    Some(config),
                )
                .await?;

            if res.final_summary.starts_with("### ") {
                Ok(res.final_summary)
            } else {
                let report = format!(
                    "✔ MiniPub Subagent `[ID: {} | Role: {}]` completed task successfully!\n• Tokens Used: {}\n• Files Modified: {}\n\n### Findings & Response Summary\n{}",
                    res.id,
                    role.badge(),
                    res.tokens_used,
                    if res.files_modified.is_empty() { "None (Read-Only)".to_string() } else { res.files_modified.join(", ") },
                    res.final_summary
                );
                Ok(report)
            }
        }.await),
        "dispatch_subagent" => Some(async {
            let role_str = param::opt_str(args, "role")
                .or_else(|| param::opt_str(args, "category"))
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "dispatch_subagent".to_string(),
                    reason: "Missing required argument 'role' (or 'category')".to_string(),
                })?;
            let prompt = param::require_str_with_aliases(
                args,
                &["prompt", "task"],
                "dispatch_subagent",
            )?;

            let role = crate::agent::subagent::SubagentRole::from_str_loose(role_str);
            let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
            let (config, todos, file_boundaries) =
                build_subagent_config_from_args(args, role, &app_cfg);

            let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
            let isolate_worktree = param::get_bool(args, "isolate_worktree").unwrap_or(
                file_boundaries.is_empty()
                    && role.default_workspace_mode()
                        == crate::agent::subagent::WorkspaceMode::Worktree,
            );

            let provider = pool.get_or_create_provider().await;
            let id = pool
                .spawn_background_worker(role, prompt, Some(config), provider, isolate_worktree)
                .await?;

            let isolation_label = if isolate_worktree {
                format!("Dedicated Git Worktree (`subagent/{}`)", id)
            } else if !file_boundaries.is_empty() {
                format!("Cooperative File Lease (`{}`)", file_boundaries.join(", "))
            } else {
                "Shared Read-Only Context".to_string()
            };

            let todos_line = if todos.is_empty() {
                String::new()
            } else {
                format!("\n• **Assigned Todos**: {} item(s)", todos.len())
            };

            let out = format!(
                "✔ Background MiniPub subagent (`minipub`) spawned successfully!\n\
                 • **Worker ID**: `{}`\n\
                 • **Role**: {}\n\
                 • **Isolation Mode**: {}{}\n\
                 • **Status**: ◉ Running in background (max parallel: {})\n\
                 • **Live Telemetry**: Press `Ctrl+S` in TUI or run `/swarm` to open the Activity Drawer.\n\
                 • **Await / Supervise**: Call `manage_subagents(action=\"await\", subagent_id=\"{}\")`, `manage_subagents(action=\"status\", subagent_id=\"{}\")`, or `minitask(action=\"workers\")`.",
                id,
                role.badge(),
                isolation_label,
                todos_line,
                pool.max_parallel(),
                id,
                id
            );

            Ok(out)
        }.await),
        "spawn_subagent" => Some(async {
            let task = param::require_str(args, "task", "spawn_subagent")?;
            let role_str = param::require_str(args, "role", "spawn_subagent")?;
            let role = crate::agent::subagent::SubagentRole::from_str_loose(role_str);

            let mode_str = param::opt_str(args, "workspace_mode").unwrap_or("auto");
            let workspace_mode = match mode_str.to_lowercase().as_str() {
                "worktree" => crate::agent::subagent::WorkspaceMode::Worktree,
                "shared" => crate::agent::subagent::WorkspaceMode::Shared,
                _ => crate::agent::subagent::WorkspaceMode::Auto,
            };

            let max_iterations = param::opt_u64(args, "max_iterations").map(|n| n as usize);
            let todos = extract_string_list(args, "todos");
            let file_boundaries = extract_string_list(args, "file_boundaries");

            let report = crate::agent::subagent::orchestrator::SubagentOrchestrator::spawn_subagent_with_options(
                workspace_root,
                task,
                role,
                workspace_mode,
                max_iterations,
                todos,
                file_boundaries,
            )
            .await?;

            Ok(report)
        }.await),
        "send_message" => Some(async {
            let recipient_str = param::opt_str(args, "recipient")
                .or_else(|| param::opt_str(args, "subagent_id"))
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "send_message".to_string(),
                    reason: "Missing required argument 'recipient' or 'subagent_id'".to_string(),
                })?;
            let message = param::require_str(args, "message", "send_message")?;
            let intent = param::opt_str(args, "intent").map(|s| match s {
                "task_init" => crate::agent::subagent::MessageIntent::TaskInit,
                "clarification_request" => crate::agent::subagent::MessageIntent::ClarificationRequest,
                "clarification_response" => crate::agent::subagent::MessageIntent::ClarificationResponse,
                "feedback" => crate::agent::subagent::MessageIntent::Feedback,
                "handoff" => crate::agent::subagent::MessageIntent::Handoff,
                "task_complete" => crate::agent::subagent::MessageIntent::TaskComplete,
                _ => crate::agent::subagent::MessageIntent::StatusUpdate,
            });

            let sender = crate::agent::subagent::AgentId::parent();
            let recipient = crate::agent::subagent::AgentId::from(recipient_str);

            let confirm = crate::agent::subagent::orchestrator::SubagentOrchestrator::send_message(
                workspace_root,
                &sender,
                &recipient,
                message,
                intent,
            )?;

            Ok(confirm)
        }.await),
        "manage_subagents" => Some(async {
            let action = param::opt_str(args, "action").unwrap_or("list");
            let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);

            match action {
                "list" => {
                    let summary = pool.format_swarm_summary().await;
                    Ok(summary)
                }
                "status" => {
                    let id = param::require_str(args, "subagent_id", "manage_subagents")?;
                    if let Some(info) = pool.get_subagent(id).await {
                        let todos_section = if info.todos.is_empty() {
                            String::new()
                        } else {
                            let list_str = info
                                .todos
                                .iter()
                                .map(|t| {
                                    format!(
                                        "  - [{}] {}",
                                        if t.completed { "x" } else { " " },
                                        t.task
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            format!("\n• Assigned Todos:\n{}", list_str)
                        };
                        let boundaries_section = if info.file_boundaries.is_empty() {
                            String::new()
                        } else {
                            format!("\n• File Boundaries: `{}`", info.file_boundaries.join(", "))
                        };
                        Ok(format!(
                            "### MiniPub Subagent `{}` Details\n• Role: {}\n• State: {:?}\n• Turns Executed: {}\n• Tokens Used: {}\n• Initial Prompt: {}\n• Started At: {}s\n• Last Activity: {}s{}{}",
                            info.id,
                            info.role.badge(),
                            info.state,
                            info.turns_executed,
                            info.tokens_used,
                            info.prompt,
                            info.started_at_secs,
                            info.last_activity_secs,
                            boundaries_section,
                            todos_section
                        ))
                    } else {
                        Ok(format!("ℹ No subagent found with ID '{}'", id))
                    }
                }
                "await" | "wait" => execute_await_workers(args, workspace_root).await,
                "transcript" | "drilldown" => {
                    let id = param::require_str(args, "subagent_id", "manage_subagents")?;
                    let step_index = param::opt_u64(args, "step_index").map(|n| n as usize);
                    let max_lines = param::opt_u64(args, "max_lines").map(|n| n as usize);
                    let filter = param::opt_str(args, "filter").unwrap_or("summary");
                    execute_transcript_drilldown(
                        id,
                        step_index,
                        max_lines,
                        filter,
                        workspace_root,
                        "subagent",
                    )
                }
                "update" | "modify" | "configure" => {
                    let mut changes = Vec::new();

                    if let Some(stream_secs) = param::opt_u64(args, "stream_idle_timeout_secs") {
                        let clamped = stream_secs.clamp(30, 7200);
                        let conn_secs = crate::config::get_active_connect_timeout_secs();
                        crate::config::set_active_provider_timeouts_override(conn_secs, clamped);
                        changes.push(format!(
                            "• Orchestrator & MiniPub Model Stream Idle Timeout ➔ `{}s`",
                            clamped
                        ));
                    }

                    if let Some(max_p) = param::opt_u64(args, "max_parallel")
                        .or_else(|| param::opt_u64(args, "max_parallel_minipubs"))
                    {
                        let applied = pool.set_max_parallel(max_p as usize);
                        changes.push(format!("• MiniPub Pool Max Parallel Concurrency ➔ `{}`", applied));
                    }

                    // Check if the Orchestrator is configuring the persistent 4-slot chain (globally or per-MiniPub role)
                    let target_role_opt = param::opt_str(args, "target_role");
                    let sync_all_opt = param::get_bool(args, "sync_all_minipubs");
                    let global_last_opt = param::opt_str(args, "global_last_fallback");
                    let has_no_subagent_id = param::opt_str(args, "subagent_id")
                        .or_else(|| param::opt_str(args, "id"))
                        .is_none();

                    if target_role_opt.is_some()
                        || sync_all_opt.is_some()
                        || global_last_opt.is_some()
                        || (has_no_subagent_id
                            && (param::opt_str(args, "model").is_some()
                                || param::opt_str(args, "fallback_1").is_some()
                                || param::opt_str(args, "fallback_2").is_some()))
                    {
                        let mut app_cfg = crate::config::Config::load_for_workspace(workspace_root);
                        let def_prov = param::opt_str(args, "provider")
                            .map(|s| s.trim().to_lowercase())
                            .unwrap_or_else(|| {
                                if !app_cfg.provider.default.trim().is_empty() {
                                    app_cfg.provider.default.clone()
                                } else {
                                    "openrouter".to_string()
                                }
                            });
                        let target_scope = target_role_opt.unwrap_or("global");

                        if sync_all_opt == Some(true) {
                            app_cfg.sync_all_minipubs_to_orchestrator();
                            changes.push(
                                "• Synced ALL 14 MiniPubs to inherit Global Orchestrator 4-slot chain"
                                    .to_string(),
                            );
                        }
                        if let Some(m) = param::opt_str(args, "model")
                            .or_else(|| param::opt_str(args, "primary"))
                        {
                            let slot = crate::config::ModelSlot::parse_loose(m, &def_prov);
                            app_cfg.set_slot_for_target(target_scope, 1, slot.clone());
                            changes.push(format!(
                                "• [{target_scope}] Slot 1 (Primary) ➔ `{}`",
                                slot.map(|s| s.display_short())
                                    .unwrap_or_else(|| "cleared".to_string())
                            ));
                        }
                        if let Some(f1) = param::opt_str(args, "fallback_1") {
                            let slot = crate::config::ModelSlot::parse_loose(f1, &def_prov);
                            app_cfg.set_slot_for_target(target_scope, 2, slot.clone());
                            changes.push(format!(
                                "• [{target_scope}] Slot 2 (1st Fallback) ➔ `{}`",
                                slot.map(|s| s.display_short())
                                    .unwrap_or_else(|| "cleared".to_string())
                            ));
                        }
                        if let Some(f2) = param::opt_str(args, "fallback_2") {
                            let slot = crate::config::ModelSlot::parse_loose(f2, &def_prov);
                            app_cfg.set_slot_for_target(target_scope, 3, slot.clone());
                            changes.push(format!(
                                "• [{target_scope}] Slot 3 (2nd Fallback) ➔ `{}`",
                                slot.map(|s| s.display_short())
                                    .unwrap_or_else(|| "cleared".to_string())
                            ));
                        }
                        if let Some(glf) = global_last_opt {
                            let slot = crate::config::ModelSlot::parse_loose(glf, &def_prov);
                            app_cfg.set_slot_for_target("global", 4, slot.clone());
                            changes.push(format!(
                                "• [Global Shared ★] Slot 4 (Global Last Fallback ★) ➔ `{}`",
                                slot.map(|s| s.display_short())
                                    .unwrap_or_else(|| "cleared".to_string())
                            ));
                        }
                        let _ = app_cfg.save(Some(workspace_root));
                    }

                    if let Some(id_raw) = param::opt_str(args, "subagent_id")
                        .or_else(|| param::opt_str(args, "id"))
                    {
                        let minipub_id = id_raw.strip_prefix("worker-").unwrap_or(id_raw);
                        let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
                        let def_prov = param::opt_str(args, "provider")
                            .unwrap_or(&app_cfg.provider.default);
                        let update = build_minipub_update_from_args(args, def_prov);

                        let info = pool.update_subagent(minipub_id, update).await?;
                        let done_todos = info.todos.iter().filter(|t| t.completed).count();
                        let model_badge = info
                            .active_model_label
                            .as_deref()
                            .unwrap_or("default-chain");
                        changes.push(format!(
                            "• Updated MiniPub `{}` ({}): State={:?} | Model=`{}` | Timeout={}s | Turns={}/{} | Tokens={}/{} | Todos={}/{} | Queued Steer={}",
                            info.id,
                            info.role.badge(),
                            info.state,
                            model_badge,
                            info.inactivity_timeout_secs,
                            info.turns_executed,
                            info.max_turns,
                            info.tokens_used,
                            info.token_budget,
                            done_todos,
                            info.todos.len(),
                            info.pending_steer_messages.len()
                        ));
                    }

                    if changes.is_empty() {
                        return Err(ToolError::InvalidArguments {
                            name: "manage_subagents".to_string(),
                            reason: "Action 'update'/'modify'/'configure' requires at least one of 'subagent_id' (with provider, model, fallback_1, fallback_2, use_orchestrator_model, timeout_secs, token_budget, max_turns, todos, file_boundaries, or steer_prompt), 'target_role' (with model, fallback_1, fallback_2, global_last_fallback), 'sync_all_minipubs', 'stream_idle_timeout_secs', or 'max_parallel'.".to_string(),
                        }
                        .into());
                    }

                    Ok(format!(
                        "⚡ MiniPub & Orchestrator Runtime Updated:\n{}",
                        changes.join("\n")
                    ))
                }
                "kill" => {
                    let id = param::require_str(args, "subagent_id", "manage_subagents")?;
                    let minipub_id = id.strip_prefix("worker-").unwrap_or(id);
                    pool.kill_subagent(minipub_id).await?;
                    let reg = crate::dev::registry::get_global_dev_registry();
                    let _ = reg
                        .stop(&crate::dev::models::DevProcessId::from(format!("worker-{}", minipub_id)))
                        .await;
                    Ok(format!("✔ MiniPub subagent `{}` successfully terminated", minipub_id))
                }
                "kill_all" => {
                    pool.kill_all().await;
                    Ok("✔ All active MiniPub subagents in swarm pool have been terminated".to_string())
                }
                other => Err(ToolError::InvalidArguments {
                    name: "manage_subagents".to_string(),
                    reason: format!("Unknown action '{}'. Valid actions: list, status, update, modify, configure, await, wait, transcript, drilldown, kill, kill_all", other),
                }.into()),
            }
        }.await),
        "scratchpad_write" => Some(async {
            let key = param::require_str(args, "key", "scratchpad_write")?;
            let title = param::require_str(args, "title", "scratchpad_write")?;
            let content = param::require_str(args, "content", "scratchpad_write")?;
            let author = param::opt_str(args, "author").unwrap_or("orchestrator");

            let sp = crate::agent::subagent::get_global_scratchpad();
            let entry = sp.write_entry(key, title, content, author);
            let _ = sp.save_to_disk(workspace_root);

            Ok(format!("✔ Scratchpad entry `{}` published successfully by `{}`.", entry.key, entry.author))
        }.await),
        "scratchpad_read" => Some(async {
            let key = param::require_str(args, "key", "scratchpad_read")?;

            let sp = crate::agent::subagent::get_global_scratchpad();
            if let Some(entry) = sp.read_entry(key) {
                Ok(format!(
                    "📋 Scratchpad `{}`: **{}** (by `{}` at {}s)\n\n{}",
                    entry.key, entry.title, entry.author, entry.updated_at_secs, entry.content
                ))
            } else {
                Ok(format!("ℹ Scratchpad key `{}` not found.", key))
            }
        }.await),
        "scratchpad_list" => Some(async {
            let sp = crate::agent::subagent::get_global_scratchpad();
            let entries = sp.list_entries();
            if entries.is_empty() {
                Ok("ℹ Shared scratchpad blackboard is currently empty.".to_string())
            } else {
                let mut out = format!("📋 Shared Scratchpad Blackboard ({} entries):\n\n", entries.len());
                for (i, e) in entries.iter().enumerate() {
                    out.push_str(&format!("{}. `{}` — **{}** (author: `{}`)\n", i + 1, e.key, e.title, e.author));
                }
                Ok(out)
            }
        }.await),
        "merge_subagent_worktree" => Some(async {
            let parsed: MergeSubagentWorktreeArgs = serde_json::from_value(args.clone())
                .map_err(|e| ToolError::InvalidArguments {
                    name: "merge_subagent_worktree".to_string(),
                    reason: format!("Failed to parse arguments: {}", e),
                })?;

            merge_subagent_worktree(
                workspace_root,
                &parsed.subagent_id,
                parsed.commit.unwrap_or(true),
                parsed.check_cmd.as_deref(),
                parsed.commit_message.as_deref(),
            )
            .await
            .map_err(Into::into)
        }.await),
        "subagent_transcript_drilldown" => Some(async {
            let subagent_id = param::require_str(args, "subagent_id", "subagent_transcript_drilldown")?;
            let step_index = param::opt_u64(args, "step_index").map(|n| n as usize);
            let filter = param::opt_str(args, "filter").unwrap_or("summary");
            let max_lines = param::opt_u64(args, "max_lines").map(|n| n as usize);

            execute_transcript_drilldown(
                subagent_id,
                step_index,
                max_lines,
                filter,
                workspace_root,
                "subagent",
            )
        }.await),
        _ => None,
    }
}

/// Arguments for `merge_subagent_worktree` tool.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MergeSubagentWorktreeArgs {
    pub subagent_id: String,
    pub commit: Option<bool>,
    pub check_cmd: Option<String>,
    pub commit_message: Option<String>,
}

/// Validates and merges code changes from a completed subagent's ephemeral git worktree
/// into the main workspace with automatic pre-merge verification, conflict detection, and cleanup.
pub async fn merge_subagent_worktree(
    workspace_root: &Path,
    subagent_id: &str,
    commit: bool,
    check_cmd: Option<&str>,
    commit_message: Option<&str>,
) -> std::result::Result<String, ToolError> {
    // 1. Locate worktree path via GitWorktreeManager::locate_worktree
    let agent_id = crate::agent::subagent::AgentId(subagent_id.to_string());
    let worktree_path =
        crate::sandbox::GitWorktreeManager::locate_worktree(workspace_root, &agent_id).ok_or_else(
            || {
                ToolError::ExecutionFailed(format!(
                    "Worktree not found for subagent '{}' under .minicode/worktrees/",
                    subagent_id
                ))
            },
        )?;

    // 2. Resolve source branch via GitWorktreeManager::resolve_branch_for
    let branch_name =
        crate::sandbox::GitWorktreeManager::resolve_branch_for(workspace_root, &agent_id);

    // 3. Pre-merge verification via MergeArbitrator::verify_worktree
    let validation = crate::sandbox::MergeArbitrator::verify_worktree(&worktree_path, check_cmd)
        .map_err(|e| ToolError::ExecutionFailed(format!("Pre-merge verification error: {}", e)))?;

    if !validation.success {
        let mut report = format!(
            "❌ Pre-merge verification failed for subagent `{}` in worktree `{}`.\n\
             • Command: `{}`\n\
             • Exit Code: {}\n\
             • Duration: {}ms\n\
             • Status: Parent workspace untouched and worktree preserved.\n",
            subagent_id,
            worktree_path.display(),
            validation.command,
            validation.exit_code,
            validation.duration_ms
        );
        if !validation.stdout.trim().is_empty() {
            report.push_str(&format!(
                "\n### Stdout\n```\n{}\n```\n",
                validation.stdout.trim()
            ));
        }
        if !validation.stderr.trim().is_empty() {
            report.push_str(&format!(
                "\n### Stderr\n```\n{}\n```\n",
                validation.stderr.trim()
            ));
        }
        return Ok(report);
    }

    // 4. Mergeability check via MergeArbitrator::check_mergeability
    let mergeability =
        crate::sandbox::MergeArbitrator::check_mergeability(workspace_root, &branch_name)
            .map_err(|e| ToolError::ExecutionFailed(format!("Mergeability check failed: {}", e)))?;

    if !mergeability.can_merge_cleanly {
        let mut report = format!(
            "⚠️ Merge conflicts detected between branch `{}` and HEAD for subagent `{}`.\n\
             • Worktree: `{}`\n\
             • Conflicted Files ({}):\n",
            branch_name,
            subagent_id,
            worktree_path.display(),
            mergeability.conflicted_files.len()
        );
        for file in &mergeability.conflicted_files {
            report.push_str(&format!("  - `{}`\n", file));
        }
        report.push_str(
            "\n• Status: Merge aborted. Parent workspace untouched and worktree preserved.\n",
        );
        return Ok(report);
    }

    // 5. Apply merge via MergeArbitrator::apply_merge
    let merge_report = crate::sandbox::MergeArbitrator::apply_merge(
        workspace_root,
        &branch_name,
        commit,
        commit_message,
    )
    .map_err(|e| match e {
        crate::sandbox::ArbitrationError::MergeConflict(conflicts) => ToolError::ExecutionFailed(
            format!("Merge conflict encountered during apply: {:?}", conflicts),
        ),
        other => ToolError::ExecutionFailed(format!("Failed to apply merge: {}", other)),
    })?;

    // 6. On successful merge, clean up worktree and temporary branch via GitWorktreeManager::remove_worktree
    let handle = crate::sandbox::WorktreeHandle {
        worktree_path: worktree_path.clone(),
        branch_name: branch_name.clone(),
        agent_id: agent_id.clone(),
        repo_root: workspace_root.to_path_buf(),
    };
    let cleanup_warning = match crate::sandbox::GitWorktreeManager::remove_worktree(&handle) {
        Ok(_) => None,
        Err(e) => {
            tracing::warn!("Failed to clean up worktree after merge: {}", e);
            Some(e.to_string())
        }
    };

    // 7. Return formatted markdown success summary
    let landing_mode = if merge_report.committed {
        "Committed (`--no-ff`)"
    } else {
        "Staged without committing (`--no-commit`)"
    };

    let mut commit_details = String::new();
    if let Some(ref hash) = merge_report.commit_hash {
        commit_details.push_str(&format!("\n• **Commit Hash**: `{}`", hash));
    }
    if let Some(ref msg) = merge_report.commit_message {
        commit_details.push_str(&format!("\n• **Commit Message**: {}", msg));
    }

    let files_summary = if merge_report.files_changed.is_empty() {
        "  - None (empty diff)".to_string()
    } else {
        merge_report
            .files_changed
            .iter()
            .map(|f| format!("  - `{}`", f))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let cleanup_detail = match cleanup_warning {
        Some(w) => format!(
            "⚠️ Warning cleaning worktree `{}`: {}",
            worktree_path.display(),
            w
        ),
        None => format!(
            "Worktree `{}` and branch `{}` removed",
            worktree_path.display(),
            branch_name
        ),
    };

    let summary = format!(
        "✔ Successfully merged subagent worktree changes!\n\
         • **Subagent ID**: `{}`\n\
         • **Branch**: `{}`\n\
         • **Landing Mode**: {}{}\n\
         • **Pre-Merge Validation**: Passed (`{}` in {}ms)\n\
         • **Cleanup**: {}\n\n\
         ### Files Changed ({})\n\
         {}\n",
        subagent_id,
        branch_name,
        landing_mode,
        commit_details,
        validation.command,
        validation.duration_ms,
        cleanup_detail,
        merge_report.files_changed.len(),
        files_summary
    );

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn test_merge_subagent_worktree_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let res = merge_subagent_worktree(temp.path(), "missing-agent", true, None, None).await;
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("Worktree not found for subagent 'missing-agent'"));
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_clean_merge() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // git init
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("hello.txt"), "base\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let agent_id = crate::agent::subagent::AgentId("coder-test".to_string());
        let handle = crate::sandbox::GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

        // Add a commit in the worktree
        std::fs::write(
            handle.worktree_path.join("feature.txt"),
            "new feature content\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "feat: add feature"])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();

        let res = merge_subagent_worktree(
            root,
            "coder-test",
            true,
            Some("skip"),
            Some("merge: feat add feature"),
        )
        .await;

        assert!(res.is_ok());
        let summary = res.unwrap();
        assert!(summary.contains("Successfully merged subagent worktree changes"));
        assert!(summary.contains("coder-test"));
        assert!(summary.contains("feature.txt"));

        // Worktree should be cleaned up
        assert!(crate::sandbox::GitWorktreeManager::locate_worktree(root, &agent_id).is_none());
        assert!(root.join("feature.txt").exists());
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_staged_no_commit() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("hello.txt"), "base\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let agent_id = crate::agent::subagent::AgentId("staged-test".to_string());
        let handle = crate::sandbox::GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

        std::fs::write(handle.worktree_path.join("staged.txt"), "staged content\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "branch staged"])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();

        let res = merge_subagent_worktree(root, "staged-test", false, Some("skip"), None).await;

        assert!(res.is_ok());
        let summary = res.unwrap();
        assert!(summary.contains("Staged without committing"));
        assert!(root.join("staged.txt").exists());
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_verification_failure() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // git init
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("hello.txt"), "base\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let agent_id = crate::agent::subagent::AgentId("failing-verifier".to_string());
        let _handle = crate::sandbox::GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

        // Verification command that fails
        let res =
            merge_subagent_worktree(root, "failing-verifier", true, Some("false"), None).await;

        assert!(res.is_ok());
        let report = res.unwrap();
        assert!(report.contains("Pre-merge verification failed"));
        assert!(report.contains("Parent workspace untouched and worktree preserved"));

        // Worktree MUST still exist
        assert!(crate::sandbox::GitWorktreeManager::locate_worktree(root, &agent_id).is_some());
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_conflict() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // git init
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("conflict.txt"), "line original\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let agent_id = crate::agent::subagent::AgentId("conflicting-subagent".to_string());
        let handle = crate::sandbox::GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

        // Branch commit
        std::fs::write(
            handle.worktree_path.join("conflict.txt"),
            "line branch edit\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "branch edit"])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();

        // Main commit
        std::fs::write(root.join("conflict.txt"), "line main edit\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "main edit"])
            .current_dir(root)
            .output()
            .unwrap();

        let res =
            merge_subagent_worktree(root, "conflicting-subagent", true, Some("skip"), None).await;

        assert!(res.is_ok());
        let report = res.unwrap();
        assert!(report.contains("Merge conflicts detected"));
        assert!(report.contains("conflict.txt"));
        assert!(report.contains("Parent workspace untouched and worktree preserved"));

        // Worktree MUST still exist
        assert!(crate::sandbox::GitWorktreeManager::locate_worktree(root, &agent_id).is_some());
    }
}
