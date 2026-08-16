pub mod browser;
pub mod compactor;
pub mod exec;
pub mod fs;
pub mod search;
pub mod web;
pub mod web_search;

use crate::agent::provider::ToolSchema;
use crate::agent::types::ToolResult;
use crate::error::{Result, ToolError};
use crate::session::backup::BackupManager;
use serde_json::json;
use std::path::Path;
use std::time::Instant;

/// Robust numeric parser that handles both JSON numbers and stringified integers
pub fn parse_u64_param(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
    })
}

pub struct ToolRegistry;

impl ToolRegistry {
    /// Returns the schemas of all built-in tools (primitives, core memory, working memory) for the LLM.
    pub fn get_tool_schemas() -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "read_file".to_string(),
                description: "Read the contents of a file in the workspace within an optional 1-indexed line range.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file within the workspace"
                        },
                        "start_line": {
                            "type": "integer",
                            "description": "Optional 1-indexed starting line number"
                        },
                        "end_line": {
                            "type": "integer",
                            "description": "Optional 1-indexed ending line number (inclusive)"
                        }
                    },
                    "required": ["path"]
                }),
            },
            ToolSchema {
                name: "patch_file".to_string(),
                description: "Apply a precise search-and-replace block edit to a file. Provide the exact text to replace and the new content.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file to modify"
                        },
                        "search_block": {
                            "type": "string",
                            "description": "The exact unique code block in the file to replace"
                        },
                        "replace_block": {
                            "type": "string",
                            "description": "The new replacement code block"
                        }
                    },
                    "required": ["path", "search_block", "replace_block"]
                }),
            },
            ToolSchema {
                name: "write_file".to_string(),
                description: "Create a new file or completely overwrite an existing file with the provided content.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file"
                        },
                        "content": {
                            "type": "string",
                            "description": "The complete text content to write"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolSchema {
                name: "exec_cmd".to_string(),
                description: "Execute a shell command inside the sandboxed workspace environment (with timeout and environment sanitization).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command string to execute"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Optional execution timeout in seconds (default: 30)"
                        }
                    },
                    "required": ["command"]
                }),
            },
            ToolSchema {
                name: "grep_search".to_string(),
                description: "Search for regex patterns or text across workspace files respecting .gitignore.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The text or regular expression to search for"
                        },
                        "is_regex": {
                            "type": "boolean",
                            "description": "Whether to treat query as a regex (default: false)"
                        },
                        "file_pattern": {
                            "type": "string",
                            "description": "Optional glob filter for file names (e.g. '*.rs')"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolSchema {
                name: "fetch_or_browse".to_string(),
                description: "Fetch web documentation or public web pages and convert HTML to readable Markdown.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The full HTTP/HTTPS URL to fetch"
                        }
                    },
                    "required": ["url"]
                }),
            },
            ToolSchema {
                name: "remember_fact".to_string(),
                description: "Save a persistent fact, convention, or developer preference to Core Memory (survives across sessions).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Unique identifier for this memory (e.g. 'code_style', 'architecture')"
                        },
                        "value": {
                            "type": "string",
                            "description": "The fact, convention, or preference to remember"
                        },
                        "is_global": {
                            "type": "boolean",
                            "description": "Whether to store globally across all projects (~/.config/minicode/memory.json) or locally (.minicode/memory.json). Default: false (local)"
                        },
                        "category": {
                            "type": "string",
                            "enum": ["preference", "project_fact", "pattern"],
                            "description": "Category of memory (default: 'project_fact')"
                        }
                    },
                    "required": ["key", "value"]
                }),
            },
            ToolSchema {
                name: "update_fact".to_string(),
                description: "Update an existing fact or preference in Core Memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Key of the memory to update"
                        },
                        "new_value": {
                            "type": "string",
                            "description": "The updated fact or preference text"
                        }
                    },
                    "required": ["key", "new_value"]
                }),
            },
            ToolSchema {
                name: "forget_fact".to_string(),
                description: "Remove a fact or preference from Core Memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Key of the memory to remove"
                        }
                    },
                    "required": ["key"]
                }),
            },
            ToolSchema {
                name: "create_plan".to_string(),
                description: "Initialize an active multi-step task plan in Working Memory (.minicode/plan/task_plan.md).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Short, descriptive title of the task"
                        },
                        "steps": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Ordered list of action steps to complete the task"
                        }
                    },
                    "required": ["steps"]
                }),
            },
            ToolSchema {
                name: "read_plan".to_string(),
                description: "Read the active task plan, progress tracker, and findings from Working Memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSchema {
                name: "log_finding".to_string(),
                description: "Record an architectural discovery, symbol location, or observation into findings.md.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "finding": {
                            "type": "string",
                            "description": "The observation, discovery, or architectural note to record"
                        }
                    },
                    "required": ["finding"]
                }),
            },
            ToolSchema {
                name: "update_progress".to_string(),
                description: "Update the status of a specific task step in progress.md (e.g. 'Completed', 'Blocked', 'In Progress').".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "step": {
                            "type": "string",
                            "description": "The description of the step matching the task plan"
                        },
                        "status": {
                            "type": "string",
                            "description": "The status (e.g. 'Completed', 'In Progress', 'Blocked')"
                        }
                    },
                    "required": ["step"]
                }),
            },
            ToolSchema {
                name: "archive_plan".to_string(),
                description: "Archive the completed task plan and clear the active Working Memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSchema {
                name: "impact_analysis".to_string(),
                description: "Analyze the architectural blast radius and downstream dependencies of modifying a symbol or file.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "string",
                            "description": "Symbol name (e.g. 'verify_token') or relative file path (e.g. 'src/auth.rs') to analyze"
                        }
                    },
                    "required": ["target"]
                }),
            },
            ToolSchema {
                name: "locate_symbol".to_string(),
                description: "Instantly locate symbol declarations, signatures, and doc comments across the workspace without full grep scans.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "The exact or partial symbol name to locate"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of matches to return (default: 10)"
                        }
                    },
                    "required": ["name"]
                }),
            },
            ToolSchema {
                name: "repo_map".to_string(),
                description: "Generate a compact AST repository skeleton map of symbols ranked by PageRank importance.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "max_tokens": {
                            "type": "integer",
                            "description": "Optional token budget for the skeleton map (default: 1024)"
                        }
                    }
                }),
            },
            ToolSchema {
                name: "git_status".to_string(),
                description: "Get the current git working tree status (branch, clean/dirty state, staged, unstaged, untracked, and conflicted files).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSchema {
                name: "git_diff".to_string(),
                description: "Get the git diff of uncommitted changes with automatic lockfile condensation and token budgeting.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "staged_only": {
                            "type": "boolean",
                            "description": "If true, only show staged changes"
                        },
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional list of specific file paths to diff"
                        }
                    }
                }),
            },
            ToolSchema {
                name: "git_commit".to_string(),
                description: "Stage files and create a git commit with a descriptive message.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Commit message (Conventional Commits format preferred, e.g. 'feat: ...' or 'fix: ...')"
                        },
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional specific file paths to stage and commit. If omitted, stages all changes."
                        }
                    },
                    "required": ["message"]
                }),
            },
            ToolSchema {
                name: "git_log".to_string(),
                description: "Show recent git commit history for the repository.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "count": {
                            "type": "integer",
                            "description": "Number of commits to return (default: 10)"
                        }
                    }
                }),
            },
            ToolSchema {
                name: "git_conflicts".to_string(),
                description: "Detect and extract merge conflict markers from repository files.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSchema {
                name: "create_pr".to_string(),
                description: "Create a GitHub pull request using the system's gh CLI with title, description, and base branch.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Pull request title"
                        },
                        "body": {
                            "type": "string",
                            "description": "Markdown formatted description of the pull request changes"
                        },
                        "draft": {
                            "type": "boolean",
                            "description": "If true, creates the pull request as a draft"
                        }
                    },
                    "required": ["title", "body"]
                }),
            },
            ToolSchema {
                name: "delegate_task".to_string(),
                description: "Delegate a subtask to an autonomous child AI agent in an isolated Git Worktree. Use for parallel research, refactoring, or independent tasks without corrupting current workspace files.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "Clear and detailed task instructions for the subagent"
                        },
                        "isolate_branch": {
                            "type": "boolean",
                            "description": "If true (default), creates a dedicated Git Worktree branch for isolation"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Maximum seconds to wait for subagent to complete (default: 120)"
                        }
                    },
                    "required": ["task"]
                }),
            },
            ToolSchema {
                name: "lsp_diagnostics".to_string(),
                description: "Fetch compiler and linter diagnostics across the workspace (or specific files) to check for compile errors and warnings.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "max_items": {
                            "type": "integer",
                            "description": "Maximum number of error items to display in detail (default: 8)"
                        }
                    }
                }),
            },
            ToolSchema {
                name: "lsp_goto_definition".to_string(),
                description: "Resolve the exact file path and line location where a code symbol (function, struct, type) is defined using LSP.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative file path containing the symbol usage"
                        },
                        "line": {
                            "type": "integer",
                            "description": "Line number (1-indexed) where the symbol appears"
                        },
                        "character": {
                            "type": "integer",
                            "description": "Column character offset (1-indexed) of the symbol"
                        }
                    },
                    "required": ["path", "line", "character"]
                }),
            },
            ToolSchema {
                name: "lsp_find_references".to_string(),
                description: "Locate all reference usages and call sites of a symbol across the workspace using LSP.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative file path containing the symbol"
                        },
                        "line": {
                            "type": "integer",
                            "description": "Line number (1-indexed)"
                        },
                        "character": {
                            "type": "integer",
                            "description": "Column character offset (1-indexed)"
                        }
                    },
                    "required": ["path", "line", "character"]
                }),
            },
            ToolSchema {
                name: "search_web".to_string(),
                description: "Search the web for up-to-date documentation, API references, library examples, and programming solutions using search engine queries.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search keywords or query string"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of search results to return (default: 5)"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolSchema {
                name: "create_task_dag".to_string(),
                description: "Initialize or replace a topological Task DAG with dependency resolution and complexity scoring.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "tasks": {
                            "type": "array",
                            "description": "List of task objects with id, title, description, and optional dependencies array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string", "description": "Unique task identifier (e.g. task-1)" },
                                    "title": { "type": "string", "description": "Brief task title" },
                                    "description": { "type": "string", "description": "Detailed task requirements" },
                                    "dependencies": {
                                        "type": "array",
                                        "items": { "type": "string" },
                                        "description": "Array of task IDs that must be completed before this task can execute"
                                    }
                                },
                                "required": ["id", "title", "description"]
                            }
                        }
                    },
                    "required": ["tasks"]
                }),
            },
            ToolSchema {
                name: "get_next_task".to_string(),
                description: "Retrieve all currently unblocked and executable tasks from the active Task DAG.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSchema {
                name: "complete_task".to_string(),
                description: "Update the lifecycle status of a task in the DAG to completed or failed, unlocking downstream dependencies.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The task ID to update"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["completed", "failed", "in_progress"],
                            "description": "The new status of the task (default: completed)"
                        }
                    },
                    "required": ["task_id"]
                }),
            },
            ToolSchema {
                name: "critic_review".to_string(),
                description: "Run an automated Actor-Critic evaluation pass over current workspace changes (compiler diagnostics, linter warnings, git status) to verify code quality.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSchema {
                name: "sequential_thinking".to_string(),
                description: "Execute a dynamic Graph of Thoughts (GoT) reasoning step to branch hypotheses, score confidence, revise earlier conclusions, and synthesize complex solutions.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "thought_number": {
                            "type": "integer",
                            "description": "Current thought number in the sequence (1-indexed)"
                        },
                        "total_thoughts": {
                            "type": "integer",
                            "description": "Estimated total thoughts required (adaptive)"
                        },
                        "thought": {
                            "type": "string",
                            "description": "The reasoning content, hypothesis analysis, or evaluation"
                        },
                        "is_revision": {
                            "type": "boolean",
                            "description": "Whether this thought revises a prior thought"
                        },
                        "revises_thought": {
                            "type": "integer",
                            "description": "The thought number being revised if is_revision is true"
                        },
                        "branch_from_thought": {
                            "type": "integer",
                            "description": "The thought number to branch off from if exploring an alternative hypothesis"
                        },
                        "branch_id": {
                            "type": "string",
                            "description": "Identifier name for this reasoning branch (e.g. 'hypothesis_a')"
                        },
                        "needs_more_thoughts": {
                            "type": "boolean",
                            "description": "Whether more thinking steps are needed before reaching a conclusion"
                        },
                        "score": {
                            "type": "number",
                            "description": "Optional confidence score between 0.0 and 1.0"
                        }
                    },
                    "required": ["thought_number", "total_thoughts", "thought", "needs_more_thoughts"]
                }),
            },
            ToolSchema {
                name: "wiki_write".to_string(),
                description: "Write or update a persistent Markdown knowledge document in the repository knowledge wiki (.minicode/wiki/<topic>.md).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "topic": {
                            "type": "string",
                            "description": "Topic slug (e.g. 'architecture-database', 'oauth-flow')"
                        },
                        "title": {
                            "type": "string",
                            "description": "Human-readable title of the wiki document"
                        },
                        "content": {
                            "type": "string",
                            "description": "Full Markdown content, guidelines, decisions, or instructions"
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of search tags and categorization keywords"
                        },
                        "references": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Associated file paths or related wiki topic slugs"
                        }
                    },
                    "required": ["topic", "title", "content"]
                }),
            },
            ToolSchema {
                name: "wiki_read".to_string(),
                description: "Read a specific knowledge wiki document from .minicode/wiki/<topic>.md by topic slug.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "topic": {
                            "type": "string",
                            "description": "Topic slug to read"
                        }
                    },
                    "required": ["topic"]
                }),
            },
            ToolSchema {
                name: "wiki_search".to_string(),
                description: "Search across repository knowledge wiki documents matching topic, title, tags, or content keywords.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search keyword or phrase"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolSchema {
                name: "browser_navigate".to_string(),
                description: "Navigate to a web page or local development server (e.g. http://localhost:3000) and extract an interactive ARIA accessibility tree with numbered element references (@e1, @e2).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The web URL or localhost address to navigate to"
                        }
                    },
                    "required": ["url"]
                }),
            },
            ToolSchema {
                name: "browser_snapshot".to_string(),
                description: "Capture an accessible ARIA DOM snapshot of a given HTML string or URL to inspect interactive UI components.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL of the page"
                        },
                        "html": {
                            "type": "string",
                            "description": "Raw HTML string to parse into accessibility tree (optional)"
                        }
                    },
                    "required": ["url"]
                }),
            },
        ]
    }

    /// Dispatches and executes a tool call by name with safety checkpointing.
    pub async fn dispatch(
        workspace_root: &Path,
        tool_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        backup_manager: Option<&BackupManager>,
        turn_id: usize,
    ) -> ToolResult {
        let start = Instant::now();

        let result =
            Self::dispatch_tool(workspace_root, tool_name, args, backup_manager, turn_id).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => ToolResult {
                tool_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                success: true,
                output,
                duration_ms,
            },
            Err(err) => ToolResult {
                tool_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                success: false,
                output: format!("Error executing {}: {}", tool_name, err),
                duration_ms,
            },
        }
    }

    async fn dispatch_tool(
        workspace_root: &Path,
        tool_name: &str,
        args: &serde_json::Value,
        backup_manager: Option<&BackupManager>,
        turn_id: usize,
    ) -> Result<String> {
        if let Some(err_msg) = args.get("__json_parse_error").and_then(|v| v.as_str()) {
            let raw = args
                .get("__raw")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return Err(ToolError::InvalidArguments {
                name: tool_name.to_string(),
                reason: format!("{}. Raw arguments: '{}'", err_msg, raw),
            }
            .into());
        }

        match tool_name {
            "read_file" => {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "read_file".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                })?;
                let start_line =
                    parse_u64_param(args.get("start_line")).and_then(|v| usize::try_from(v).ok());
                let end_line =
                    parse_u64_param(args.get("end_line")).and_then(|v| usize::try_from(v).ok());
                fs::read_file(workspace_root, path, start_line, end_line)
            }
            "write_file" => {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "write_file".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                })?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "write_file".to_string(),
                        reason: "Missing required argument 'content'".to_string(),
                    })?;

                let validated_path = crate::sandbox::path::validate_path_in_workspace(
                    workspace_root,
                    Path::new(path),
                )?;

                // Safety checkpoint before modifying
                if let Some(mgr) = backup_manager {
                    if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id)
                    {
                        tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before write_file");
                    }
                }

                fs::write_file(workspace_root, path, content)
            }
            "patch_file" => {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "patch_file".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                })?;
                let search = args
                    .get("search_block")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "patch_file".to_string(),
                        reason: "Missing required argument 'search_block'".to_string(),
                    })?;
                let replace = args
                    .get("replace_block")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "patch_file".to_string(),
                        reason: "Missing required argument 'replace_block'".to_string(),
                    })?;

                let validated_path = crate::sandbox::path::validate_path_in_workspace(
                    workspace_root,
                    Path::new(path),
                )?;

                // Safety checkpoint before patching
                if let Some(mgr) = backup_manager {
                    if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id)
                    {
                        tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before patch_file");
                    }
                }

                fs::patch_file(workspace_root, path, search, replace)
            }
            "exec_cmd" => {
                let cmd = args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "exec_cmd".to_string(),
                        reason: "Missing required argument 'command'".to_string(),
                    })?;
                let timeout = parse_u64_param(args.get("timeout_secs"));
                exec::exec_cmd(workspace_root, cmd, timeout).await
            }
            "grep_search" => {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "grep_search".to_string(),
                        reason: "Missing required argument 'query'".to_string(),
                    }
                })?;
                let is_regex = args
                    .get("is_regex")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let pattern = args.get("file_pattern").and_then(|v| v.as_str());
                search::grep_search(workspace_root, query, is_regex, pattern)
            }
            "fetch_or_browse" => {
                let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "fetch_or_browse".to_string(),
                        reason: "Missing required argument 'url'".to_string(),
                    }
                })?;
                web::fetch_or_browse(url).await
            }
            "remember_fact" => {
                let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "remember_fact".to_string(),
                        reason: "Missing required argument 'key'".to_string(),
                    }
                })?;
                let value = args.get("value").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "remember_fact".to_string(),
                        reason: "Missing required argument 'value'".to_string(),
                    }
                })?;
                let is_global = args
                    .get("is_global")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let cat_str = args
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("project_fact");
                let category = match cat_str {
                    "preference" => crate::context::memory::MemoryCategory::Preference,
                    "pattern" => crate::context::memory::MemoryCategory::Pattern,
                    _ => crate::context::memory::MemoryCategory::ProjectFact,
                };
                let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
                mem.remember(workspace_root, key, value, is_global, category)
                    .map(|_| {
                        format!(
                            "✔ Remembered '{}' ({})",
                            key,
                            if is_global { "global" } else { "local" }
                        )
                    })
            }
            "update_fact" => {
                let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "update_fact".to_string(),
                        reason: "Missing required argument 'key'".to_string(),
                    }
                })?;
                let new_value =
                    args.get("new_value")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "update_fact".to_string(),
                            reason: "Missing required argument 'new_value'".to_string(),
                        })?;
                let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
                mem.update(workspace_root, key, new_value).map(|updated| {
                    if updated {
                        format!("✔ Updated fact '{}'", key)
                    } else {
                        format!("ℹ Fact '{}' not found to update", key)
                    }
                })
            }
            "forget_fact" => {
                let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "forget_fact".to_string(),
                        reason: "Missing required argument 'key'".to_string(),
                    }
                })?;
                let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
                mem.forget(workspace_root, key).map(|forgotten| {
                    if forgotten {
                        format!("✔ Removed fact '{}' from memory", key)
                    } else {
                        format!("ℹ Fact '{}' not found in memory", key)
                    }
                })
            }
            "create_plan" => {
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Task Plan");
                let steps: Vec<String> = args
                    .get("steps")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| item.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "create_plan".to_string(),
                        reason: "Missing required argument 'steps'".to_string(),
                    })?;
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                wm.init_plan(title, &steps).map(|_| {
                    format!(
                        "✔ Created active task plan with {} steps in .minicode/plan/task_plan.md",
                        steps.len()
                    )
                })
            }
            "read_plan" => {
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                match wm.read_plan() {
                    Ok(Some(plan)) => Ok(plan),
                    Ok(None) => Ok("ℹ No active task plan found in .minicode/plan/".to_string()),
                    Err(e) => Err(e),
                }
            }
            "log_finding" => {
                let finding = args
                    .get("finding")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "log_finding".to_string(),
                        reason: "Missing required argument 'finding'".to_string(),
                    })?;
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                wm.append_finding(finding)
                    .map(|_| "✔ Logged observation into .minicode/plan/findings.md".to_string())
            }
            "update_progress" => {
                let step = args.get("step").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "update_progress".to_string(),
                        reason: "Missing required argument 'step'".to_string(),
                    }
                })?;
                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Completed");
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                wm.update_progress(step, status)
                    .map(|_| format!("✔ Updated step '{}' status to '{}'", step, status))
            }
            "archive_plan" => {
                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                match wm.archive_plan() {
                    Ok(Some(archive_path)) => Ok(format!(
                        "✔ Archived completed task plan to {}",
                        archive_path.display()
                    )),
                    Ok(None) => Ok("ℹ No active task plan to archive".to_string()),
                    Err(e) => Err(e),
                }
            }
            "impact_analysis" => {
                let target = args.get("target").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "impact_analysis".to_string(),
                        reason: "Missing required argument 'target'".to_string(),
                    }
                })?;
                let mut graph = crate::context::graph::CodeGraph::new();
                graph.build_graph(workspace_root)?;
                let report = graph.get_blast_radius(target, workspace_root)?;
                Ok(report.summary)
            }
            "locate_symbol" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "locate_symbol".to_string(),
                        reason: "Missing required argument 'name'".to_string(),
                    }
                })?;
                let limit = parse_u64_param(args.get("limit"))
                    .unwrap_or(crate::constants::DEFAULT_LOCATE_SYMBOL_LIMIT as u64)
                    as usize;
                let mut index = crate::context::index::SymbolIndex::new();
                index.build_index(workspace_root)?;
                let matches = if name.contains(' ') {
                    index.search_symbols(name, limit)
                } else {
                    let mut res = index.locate_symbol(name);
                    if res.is_empty() {
                        res = index.search_symbols(name, limit);
                    }
                    res.truncate(limit);
                    res
                };
                Ok(index.format_matches(&matches, workspace_root))
            }
            "repo_map" => {
                let max_tokens = parse_u64_param(args.get("max_tokens"))
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(crate::constants::DEFAULT_MAP_TOKENS);
                let mut graph = crate::context::graph::CodeGraph::new();
                graph.build_graph(workspace_root)?;
                Ok(graph.format_repomap(workspace_root, &[], max_tokens))
            }
            "git_status" => {
                let git = crate::git::GitService::new(workspace_root.to_path_buf());
                if !git.is_git_repo().await {
                    return Ok("ℹ Workspace is not a git repository".to_string());
                }
                let status = git.get_status().await?;
                let mut out = format!(
                    "Branch: {}\nStatus: {}\n",
                    status.branch,
                    if status.is_clean { "Clean" } else { "Dirty" }
                );
                if !status.staged.is_empty() {
                    out.push_str(&format!(
                        "Staged ({}):\n  • {}\n",
                        status.staged.len(),
                        status.staged.join("\n  • ")
                    ));
                }
                if !status.unstaged.is_empty() {
                    out.push_str(&format!(
                        "Unstaged ({}):\n  • {}\n",
                        status.unstaged.len(),
                        status.unstaged.join("\n  • ")
                    ));
                }
                if !status.untracked.is_empty() {
                    out.push_str(&format!(
                        "Untracked ({}):\n  • {}\n",
                        status.untracked.len(),
                        status.untracked.join("\n  • ")
                    ));
                }
                if !status.conflicted.is_empty() {
                    out.push_str(&format!(
                        "Conflicted ({}):\n  • {}\n",
                        status.conflicted.len(),
                        status.conflicted.join("\n  • ")
                    ));
                }
                Ok(out)
            }
            "git_diff" => {
                let git = crate::git::GitService::new(workspace_root.to_path_buf());
                if !git.is_git_repo().await {
                    return Ok("ℹ Workspace is not a git repository".to_string());
                }
                let staged_only = args
                    .get("staged_only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let paths: Option<Vec<String>> =
                    args.get("paths").and_then(|v| v.as_array()).map(|arr| {
                        arr.iter()
                            .filter_map(|s| s.as_str().map(|str_val| str_val.to_string()))
                            .collect()
                    });
                let diff_output = git.diff(staged_only, paths.as_deref()).await?;
                if diff_output.trim().is_empty() {
                    Ok("ℹ No changes detected".to_string())
                } else {
                    Ok(diff_output)
                }
            }
            "git_commit" => {
                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "git_commit".to_string(),
                        reason: "Missing required argument 'message'".to_string(),
                    })?;
                let paths: Option<Vec<String>> =
                    args.get("paths").and_then(|v| v.as_array()).map(|arr| {
                        arr.iter()
                            .filter_map(|s| s.as_str().map(|str_val| str_val.to_string()))
                            .collect()
                    });
                let git = crate::git::GitService::new(workspace_root.to_path_buf());
                if !git.is_git_repo().await {
                    return Ok("ℹ Workspace is not a git repository".to_string());
                }
                let commit_svc = crate::git::GitCommitService::new(&git);
                let commit_hash = commit_svc.commit(message, paths.as_deref()).await?;
                crate::ui::status::StatusWidgets::invalidate_git_cache();
                Ok(format!(
                    "✔ Created commit {} with message: \"{}\"",
                    commit_hash, message
                ))
            }
            "git_log" => {
                let git = crate::git::GitService::new(workspace_root.to_path_buf());
                if !git.is_git_repo().await {
                    return Ok("ℹ Workspace is not a git repository".to_string());
                }
                let count = parse_u64_param(args.get("count"))
                    .unwrap_or(crate::constants::GIT_LOG_DEFAULT_COUNT as u64)
                    as usize;
                let log = git.log(count).await?;
                if log.trim().is_empty() {
                    Ok("ℹ No commit history found".to_string())
                } else {
                    Ok(log)
                }
            }
            "git_conflicts" => {
                let git = crate::git::GitService::new(workspace_root.to_path_buf());
                if !git.is_git_repo().await {
                    return Ok("ℹ Workspace is not a git repository".to_string());
                }
                let conflicts = git.find_conflicts().await?;
                if conflicts.is_empty() {
                    Ok("✔ No merge conflicts detected in workspace".to_string())
                } else {
                    let mut out = format!("⚠ Found {} conflicted file(s):\n", conflicts.len());
                    for c in conflicts {
                        out.push_str(&format!(
                            "\nFile: {} ({} conflict marker(s))\nSnippet:\n{}\n",
                            c.path, c.conflict_markers_count, c.snippet
                        ));
                    }
                    Ok(out)
                }
            }
            "create_pr" => {
                let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "create_pr".to_string(),
                        reason: "Missing required argument 'title'".to_string(),
                    }
                })?;
                let body = args.get("body").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "create_pr".to_string(),
                        reason: "Missing required argument 'body'".to_string(),
                    }
                })?;
                let base = args.get("base").and_then(|v| v.as_str());
                let draft = args.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);

                let git = crate::git::GitService::new(workspace_root.to_path_buf());
                if !git.is_git_repo().await {
                    return Ok("ℹ Workspace is not a git repository".to_string());
                }
                let pr_url = git.create_pull_request(title, body, base, draft).await?;
                Ok(format!("✔ Created Pull Request: {}", pr_url))
            }
            "delegate_task" => {
                let task = args.get("task").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "delegate_task".to_string(),
                        reason: "Missing required argument 'task'".to_string(),
                    }
                })?;
                let isolate = args
                    .get("isolate_branch")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let timeout_secs = parse_u64_param(args.get("timeout_secs"));

                let res = crate::agent::orchestrator::MultiAgentOrchestrator::delegate(
                    workspace_root,
                    task,
                    isolate,
                    timeout_secs,
                )
                .await?;
                Ok(crate::agent::orchestrator::MultiAgentOrchestrator::format_result(&res))
            }
            "lsp_diagnostics" => {
                let max_items = parse_u64_param(args.get("max_items")).unwrap_or(8) as usize;
                let report = crate::lsp::LspEngine::run_diagnostics(workspace_root).await?;
                Ok(report.format_for_agent(workspace_root, max_items))
            }
            "lsp_goto_definition" => {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "lsp_goto_definition".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                })?;
                let line = parse_u64_param(args.get("line")).unwrap_or(1) as u32;
                let character = parse_u64_param(args.get("character")).unwrap_or(1) as u32;

                // LSP uses 0-indexed positions
                let lsp_line = line.saturating_sub(1);
                let lsp_col = character.saturating_sub(1);

                let locations = crate::lsp::LspEngine::goto_definition(
                    workspace_root,
                    Path::new(path),
                    lsp_line,
                    lsp_col,
                )
                .await?;
                if locations.is_empty() {
                    Ok(format!(
                        "ℹ No definition found for '{}:{}:{}' via LSP",
                        path, line, character
                    ))
                } else {
                    let mut out = format!("✔ Found {} definition location(s):\n", locations.len());
                    for loc in locations {
                        let rel = loc
                            .file_path
                            .strip_prefix(workspace_root)
                            .unwrap_or(&loc.file_path);
                        out.push_str(&format!(
                            "  • {}:{}:{}\n",
                            rel.display(),
                            loc.line + 1,
                            loc.character + 1
                        ));
                    }
                    Ok(out)
                }
            }
            "lsp_find_references" => {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "lsp_find_references".to_string(),
                        reason: "Missing required argument 'path'".to_string(),
                    }
                })?;
                let line = parse_u64_param(args.get("line")).unwrap_or(1) as u32;
                let character = parse_u64_param(args.get("character")).unwrap_or(1) as u32;

                let lsp_line = line.saturating_sub(1);
                let lsp_col = character.saturating_sub(1);

                let locations = crate::lsp::LspEngine::find_references(
                    workspace_root,
                    Path::new(path),
                    lsp_line,
                    lsp_col,
                )
                .await?;
                if locations.is_empty() {
                    Ok(format!(
                        "ℹ No references found for '{}:{}:{}' via LSP",
                        path, line, character
                    ))
                } else {
                    let mut out = format!("✔ Found {} reference usage(s):\n", locations.len());
                    for loc in locations {
                        let rel = loc
                            .file_path
                            .strip_prefix(workspace_root)
                            .unwrap_or(&loc.file_path);
                        out.push_str(&format!(
                            "  • {}:{}:{}\n",
                            rel.display(),
                            loc.line + 1,
                            loc.character + 1
                        ));
                    }
                    Ok(out)
                }
            }
            "search_web" => {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "search_web".to_string(),
                        reason: "Missing required argument 'query'".to_string(),
                    }
                })?;
                let max_results = parse_u64_param(args.get("max_results")).unwrap_or(5) as usize;
                let results_md =
                    crate::tools::web_search::WebSearchService::search(query, max_results).await?;
                Ok(results_md)
            }
            "create_task_dag" => {
                let tasks_array =
                    args.get("tasks")
                        .and_then(|v| v.as_array())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "create_task_dag".to_string(),
                            reason: "Missing required argument 'tasks' array".to_string(),
                        })?;

                let mut dag = crate::agent::task_dag::TaskDag::new();
                for item in tasks_array {
                    let task: crate::agent::task_dag::TaskItem =
                        serde_json::from_value(item.clone()).map_err(|e| {
                            ToolError::InvalidArguments {
                                name: "create_task_dag".to_string(),
                                reason: format!("Invalid task schema: {}", e),
                            }
                        })?;
                    dag.add_task(task);
                }

                // Validate no cycles exist
                let order = dag.topological_order()?;
                dag.save(workspace_root)?;

                Ok(format!(
                    "✔ Task DAG initialized with {} tasks (Topological Order: {})\n\n{}",
                    dag.tasks.len(),
                    order.join(" ➔ "),
                    dag.generate_report()
                ))
            }
            "get_next_task" => {
                let dag = crate::agent::task_dag::TaskDag::load(workspace_root)?;
                if dag.tasks.is_empty() {
                    return Ok("ℹ No active Task DAG found in workspace. Use 'create_task_dag' to initialize one.".to_string());
                }

                let next_tasks = dag.next_executable_tasks();
                if next_tasks.is_empty() {
                    let report = dag.generate_report();
                    if dag
                        .tasks
                        .values()
                        .all(|t| t.status == crate::agent::task_dag::TaskStatus::Completed)
                    {
                        Ok(format!("🎉 All tasks in DAG are completed!\n\n{}", report))
                    } else {
                        Ok(format!("⏸ No tasks currently unblocked. Check in-progress tasks or dependencies.\n\n{}", report))
                    }
                } else {
                    let mut out = format!(
                        "🎯 Next Executable Task(s) ({} unblocked):\n\n",
                        next_tasks.len()
                    );
                    for task in next_tasks {
                        out.push_str(&format!(
                            "• `{}`: **{}** (Complexity: {}/10)\n  {}\n\n",
                            task.id, task.title, task.complexity_score, task.description
                        ));
                    }
                    Ok(out)
                }
            }
            "complete_task" => {
                let task_id = args
                    .get("task_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "complete_task".to_string(),
                        reason: "Missing required argument 'task_id'".to_string(),
                    })?;

                let status_str = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed");
                let status = match status_str {
                    "in_progress" => crate::agent::task_dag::TaskStatus::InProgress,
                    "failed" => crate::agent::task_dag::TaskStatus::Failed,
                    _ => crate::agent::task_dag::TaskStatus::Completed,
                };

                let mut dag = crate::agent::task_dag::TaskDag::load(workspace_root)?;
                dag.set_task_status(task_id, status)?;
                dag.save(workspace_root)?;

                let next = dag.next_executable_tasks();
                let next_desc = if next.is_empty() {
                    "None (all remaining tasks are blocked or completed)".to_string()
                } else {
                    next.iter()
                        .map(|t| format!("`{}` ({})", t.id, t.title))
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                Ok(format!(
                    "✔ Updated task `{}` to status {:?}.\n👉 Newly unblocked executable task(s): {}\n\n{}",
                    task_id,
                    status,
                    next_desc,
                    dag.generate_report()
                ))
            }
            "critic_review" => {
                let report =
                    crate::agent::critic::CriticValidator::review_workspace(workspace_root).await?;
                let verdict_str = if report.is_approved {
                    "✔ [CRITIC APPROVED]"
                } else {
                    "❌ [CRITIC REJECTED]"
                };

                let mut out = format!("🔍 Critic Evaluation Summary: {}\n\n", verdict_str);
                out.push_str(&format!("• Status: {}\n", report.suggested_feedback));
                out.push_str(&format!("• Compiler Errors: {}\n", report.compiler_errors));
                out.push_str(&format!(
                    "• Compiler Warnings: {}\n",
                    report.compiler_warnings
                ));
                if !report.uncommitted_files.is_empty() {
                    out.push_str(&format!(
                        "• Modified Files ({}):\n",
                        report.uncommitted_files.len()
                    ));
                    for file in &report.uncommitted_files {
                        out.push_str(&format!("    - {}\n", file));
                    }
                }
                Ok(out)
            }
            "sequential_thinking" => {
                let thought_node: crate::agent::sequential_thinking::ThoughtNode =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        ToolError::InvalidArguments {
                            name: "sequential_thinking".to_string(),
                            reason: format!("Invalid thought node parameters: {}", e),
                        }
                    })?;
                let output =
                    crate::agent::sequential_thinking::ThinkingSession::step(thought_node)?;
                Ok(output)
            }
            "wiki_write" => {
                let topic = args["topic"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "wiki_write".to_string(),
                        reason: "Missing 'topic'".to_string(),
                    })?;
                let title = args["title"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "wiki_write".to_string(),
                        reason: "Missing 'title'".to_string(),
                    })?;
                let content =
                    args["content"]
                        .as_str()
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "wiki_write".to_string(),
                            reason: "Missing 'content'".to_string(),
                        })?;
                let tags: Vec<String> = args["tags"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let references: Vec<String> = args["references"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let res = crate::context::wiki::WikiManager::write_entry(
                    workspace_root,
                    topic,
                    title,
                    content,
                    &tags,
                    &references,
                )?;
                Ok(res)
            }
            "wiki_read" => {
                let topic = args["topic"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "wiki_read".to_string(),
                        reason: "Missing 'topic'".to_string(),
                    })?;
                let content = crate::context::wiki::WikiManager::read_entry(workspace_root, topic)?;
                Ok(content)
            }
            "wiki_search" => {
                let query = args["query"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "wiki_search".to_string(),
                        reason: "Missing 'query'".to_string(),
                    })?;
                let results =
                    crate::context::wiki::WikiManager::search_entries(workspace_root, query)?;
                Ok(results)
            }
            "browser_navigate" => {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_navigate".to_string(),
                        reason: "Missing 'url'".to_string(),
                    })?;
                let snapshot = crate::tools::browser::BrowserController::navigate(url).await?;
                let report =
                    crate::tools::browser::BrowserController::format_snapshot_report(&snapshot);
                Ok(report)
            }
            "browser_snapshot" => {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "browser_snapshot".to_string(),
                        reason: "Missing 'url'".to_string(),
                    })?;
                let html_opt = args["html"].as_str();
                let snapshot = if let Some(html) = html_opt {
                    crate::tools::browser::BrowserController::parse_html_to_aria_snapshot(url, html)
                } else {
                    crate::tools::browser::BrowserController::navigate(url).await?
                };
                let report =
                    crate::tools::browser::BrowserController::format_snapshot_report(&snapshot);
                Ok(report)
            }
            unknown => Err(ToolError::NotFound {
                name: unknown.to_string(),
            }
            .into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_schemas_count() {
        let schemas = ToolRegistry::get_tool_schemas();
        assert_eq!(schemas.len(), 38);
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"patch_file"));
        assert!(names.contains(&"lsp_diagnostics"));
        assert!(names.contains(&"create_task_dag"));
        assert!(names.contains(&"get_next_task"));
        assert!(names.contains(&"complete_task"));
        assert!(names.contains(&"critic_review"));
        assert!(names.contains(&"browser_navigate"));
        assert!(names.contains(&"browser_snapshot"));
        assert!(names.contains(&"sequential_thinking"));
        assert!(names.contains(&"wiki_write"));
        assert!(names.contains(&"wiki_read"));
        assert!(names.contains(&"wiki_search"));
        assert!(names.contains(&"lsp_goto_definition"));
        assert!(names.contains(&"lsp_find_references"));
        assert!(names.contains(&"search_web"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"exec_cmd"));
        assert!(names.contains(&"grep_search"));
        assert!(names.contains(&"fetch_or_browse"));
        assert!(names.contains(&"remember_fact"));
        assert!(names.contains(&"update_fact"));
        assert!(names.contains(&"forget_fact"));
        assert!(names.contains(&"create_plan"));
        assert!(names.contains(&"read_plan"));
        assert!(names.contains(&"log_finding"));
        assert!(names.contains(&"update_progress"));
        assert!(names.contains(&"archive_plan"));
        assert!(names.contains(&"impact_analysis"));
        assert!(names.contains(&"locate_symbol"));
        assert!(names.contains(&"repo_map"));
        assert!(names.contains(&"git_status"));
        assert!(names.contains(&"git_diff"));
        assert!(names.contains(&"git_commit"));
        assert!(names.contains(&"git_log"));
        assert!(names.contains(&"git_conflicts"));
        assert!(names.contains(&"create_pr"));
        assert!(names.contains(&"delegate_task"));
    }

    #[test]
    fn test_parse_u64_param() {
        let num_val = json!(42);
        assert_eq!(parse_u64_param(Some(&num_val)), Some(42));

        let str_val = json!("100");
        assert_eq!(parse_u64_param(Some(&str_val)), Some(100));

        let invalid_str = json!("abc");
        assert_eq!(parse_u64_param(Some(&invalid_str)), None);

        assert_eq!(parse_u64_param(None), None);
    }

    #[tokio::test]
    async fn test_dispatch_invalid_json_arguments() {
        let root = Path::new(".");
        let args = json!({
            "__json_parse_error": "Invalid syntax at line 1 column 4",
            "__raw": "{\"a\":"
        });
        let res = ToolRegistry::dispatch(root, "call_1", "read_file", &args, None, 1).await;
        assert!(!res.success);
        assert!(res.output.contains("Invalid syntax"));
        assert!(res.output.contains("Raw arguments"));
    }
}
