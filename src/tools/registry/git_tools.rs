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
            name: "git_review".to_string(),
            description: "Perform an automated multi-dimensional code review on current uncommitted or staged changes across security, correctness, architecture, and tests.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "staged_only": {
                        "type": "boolean",
                        "description": "If true, only review staged changes. If false, review all uncommitted changes."
                    }
                }
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
            name: "github_issue_view".to_string(),
            description: "View a GitHub issue's title, body, state, author, labels, and discussion comments.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "issue_number": {
                        "type": "integer",
                        "description": "The issue number to view"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Optional GitHub repository slug ('owner/repo'). Defaults to current workspace repository."
                    }
                },
                "required": ["issue_number"]
            }),
        },
        ToolSchema {
            name: "github_issue_list".to_string(),
            description: "List issues in a GitHub repository with state and limit filters.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "state": {
                        "type": "string",
                        "enum": ["open", "closed", "all"],
                        "description": "State filter (default: 'open')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum issues to return (default: 10)"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Optional GitHub repository slug ('owner/repo')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "github_issue_create".to_string(),
            description: "Create a new issue in a GitHub repository with title, markdown body, and labels.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Title of the issue"
                    },
                    "body": {
                        "type": "string",
                        "description": "Markdown body of the issue"
                    },
                    "labels": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of label names"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Optional GitHub repository slug ('owner/repo')"
                    }
                },
                "required": ["title", "body"]
            }),
        },
        ToolSchema {
            name: "github_pr_view".to_string(),
            description: "View details of a GitHub pull request including branches, additions/deletions, and description.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pr_number": {
                        "type": "integer",
                        "description": "The pull request number"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Optional GitHub repository slug ('owner/repo')"
                    }
                },
                "required": ["pr_number"]
            }),
        },
        ToolSchema {
            name: "github_pr_diff".to_string(),
            description: "Fetch the raw unified git diff of a GitHub pull request.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pr_number": {
                        "type": "integer",
                        "description": "The pull request number"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Optional GitHub repository slug ('owner/repo')"
                    }
                },
                "required": ["pr_number"]
            }),
        },
        ToolSchema {
            name: "github_pr_create".to_string(),
            description: "Open a pull request from the current branch against a base branch in GitHub.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Pull request title"
                    },
                    "body": {
                        "type": "string",
                        "description": "Markdown formatted description of the pull request"
                    },
                    "base": {
                        "type": "string",
                        "description": "Target base branch (default: 'main')"
                    },
                    "draft": {
                        "type": "boolean",
                        "description": "Whether to create as draft (default: false)"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Optional GitHub repository slug ('owner/repo')"
                    }
                },
                "required": ["title", "body"]
            }),
        },
        ToolSchema {
            name: "github_ci_status".to_string(),
            description: "Inspect GitHub Actions CI workflow runs (success, failure, running) for the repository or branch.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "branch": {
                        "type": "string",
                        "description": "Optional branch name to filter workflow runs"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum workflow runs to return (default: 5)"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Optional GitHub repository slug ('owner/repo')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "github_ci_logs".to_string(),
            description: "Fetch failed job error logs for a specific GitHub Actions workflow run to diagnose and fix CI breaks.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "run_id": {
                        "type": "integer",
                        "description": "The workflow run ID"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Optional GitHub repository slug ('owner/repo')"
                    }
                },
                "required": ["run_id"]
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
        "git_review" => Some(
            async {
                let staged_only = args
                    .get("staged_only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let report =
                    crate::git::GitReviewer::review_workspace(workspace_root, staged_only).await?;
                Ok(crate::git::GitReviewer::format_report(&report))
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
        "github_issue_view" => Some(
            async {
                let issue_num = parse_u64_param(args.get("issue_number")).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "github_issue_view".to_string(),
                        reason: "Missing required argument 'issue_number'".to_string(),
                    }
                })?;
                let repo = args.get("repo").and_then(|v| v.as_str());
                crate::tools::github::GitHubService::view_issue(workspace_root, repo, issue_num)
                    .await
            }
            .await,
        ),
        "github_issue_list" => Some(
            async {
                let state = args.get("state").and_then(|v| v.as_str());
                let limit = parse_u64_param(args.get("limit")).unwrap_or(10) as usize;
                let repo = args.get("repo").and_then(|v| v.as_str());
                crate::tools::github::GitHubService::list_issues(workspace_root, repo, state, limit)
                    .await
            }
            .await,
        ),
        "github_issue_create" => Some(
            async {
                let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "github_issue_create".to_string(),
                        reason: "Missing required argument 'title'".to_string(),
                    }
                })?;
                let body = args.get("body").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "github_issue_create".to_string(),
                        reason: "Missing required argument 'body'".to_string(),
                    }
                })?;
                let labels: Vec<String> = args
                    .get("labels")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| s.as_str().map(|v| v.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let repo = args.get("repo").and_then(|v| v.as_str());
                crate::tools::github::GitHubService::create_issue(
                    workspace_root,
                    repo,
                    title,
                    body,
                    &labels,
                )
                .await
            }
            .await,
        ),
        "github_pr_view" => Some(
            async {
                let pr_num = parse_u64_param(args.get("pr_number")).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "github_pr_view".to_string(),
                        reason: "Missing required argument 'pr_number'".to_string(),
                    }
                })?;
                let repo = args.get("repo").and_then(|v| v.as_str());
                crate::tools::github::GitHubService::view_pr(workspace_root, repo, pr_num).await
            }
            .await,
        ),
        "github_pr_diff" => Some(
            async {
                let pr_num = parse_u64_param(args.get("pr_number")).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "github_pr_diff".to_string(),
                        reason: "Missing required argument 'pr_number'".to_string(),
                    }
                })?;
                let repo = args.get("repo").and_then(|v| v.as_str());
                crate::tools::github::GitHubService::view_pr_diff(workspace_root, repo, pr_num)
                    .await
            }
            .await,
        ),
        "github_pr_create" => Some(
            async {
                let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "github_pr_create".to_string(),
                        reason: "Missing required argument 'title'".to_string(),
                    }
                })?;
                let body = args.get("body").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "github_pr_create".to_string(),
                        reason: "Missing required argument 'body'".to_string(),
                    }
                })?;
                let base = args.get("base").and_then(|v| v.as_str()).unwrap_or("main");
                let draft = args.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);
                let repo = args.get("repo").and_then(|v| v.as_str());
                crate::tools::github::GitHubService::create_pr(
                    workspace_root,
                    repo,
                    title,
                    body,
                    base,
                    draft,
                )
                .await
            }
            .await,
        ),
        "github_ci_status" => Some(
            async {
                let branch = args.get("branch").and_then(|v| v.as_str());
                let limit = parse_u64_param(args.get("limit")).unwrap_or(5) as usize;
                let repo = args.get("repo").and_then(|v| v.as_str());
                crate::tools::github::GitHubService::get_ci_status(
                    workspace_root,
                    repo,
                    branch,
                    limit,
                )
                .await
            }
            .await,
        ),
        "github_ci_logs" => Some(
            async {
                let run_id = parse_u64_param(args.get("run_id")).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "github_ci_logs".to_string(),
                        reason: "Missing required argument 'run_id'".to_string(),
                    }
                })?;
                let repo = args.get("repo").and_then(|v| v.as_str());
                crate::tools::github::GitHubService::get_ci_logs(workspace_root, repo, run_id).await
            }
            .await,
        ),
        _ => None,
    }
}
