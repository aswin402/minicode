use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::parse_u64_param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
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
    ]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "git_status" => Some(
            async {
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
            .await,
        ),
        "git_diff" => Some(
            async {
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
            .await,
        ),
        "git_commit" => Some(
            async {
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
            .await,
        ),
        "git_log" => Some(
            async {
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
            .await,
        ),
        "git_conflicts" => Some(
            async {
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
            .await,
        ),
        "create_pr" => Some(
            async {
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
            .await,
        ),
        _ => None,
    }
}
