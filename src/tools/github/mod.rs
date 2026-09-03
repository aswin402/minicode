pub mod client;
pub mod types;

use crate::error::{Result, ToolError};
#[allow(unused_imports)]
pub use client::{get_github_token, GitHubClient};
use std::path::Path;
#[allow(unused_imports)]
pub use types::{GitHubComment, GitHubIssue, GitHubPR, GitHubWorkflowRun};

/// High-level operations for issues, pull requests, diffs, and CI workflow runs.
pub struct GitHubService;

impl GitHubService {
    /// Resolves target repo slug or auto-detects from workspace.
    fn resolve_repo(repo_opt: Option<&str>, workspace_root: &Path) -> Result<String> {
        match repo_opt {
            Some(r) if !r.trim().is_empty() => Ok(r.trim().to_string()),
            _ => GitHubClient::detect_repo(workspace_root),
        }
    }

    /// Views details of a specific issue.
    pub async fn view_issue(
        workspace_root: &Path,
        repo: Option<&str>,
        issue_number: u64,
    ) -> Result<String> {
        let repo_slug = Self::resolve_repo(repo, workspace_root)?;

        if GitHubClient::has_gh_cli() {
            let num_str = issue_number.to_string();
            let out = GitHubClient::exec_gh(
                &[
                    "issue",
                    "view",
                    &num_str,
                    "--repo",
                    &repo_slug,
                    "--json",
                    "number,title,body,state,author,labels,comments",
                ],
                workspace_root,
            )?;
            let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
            let title = v["title"].as_str().unwrap_or("Untitled");
            let state = v["state"].as_str().unwrap_or("unknown");
            let author = v["author"]["login"].as_str().unwrap_or("unknown");
            let body = v["body"].as_str().unwrap_or("");

            let labels: Vec<String> = v["labels"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let label_str = if labels.is_empty() {
                String::new()
            } else {
                format!(" [{}]", labels.join(", "))
            };

            let mut res = format!(
                "🎫 Issue #{} in `{}` [State: {}]{}\n**{}** (by `{}`)\n\n{}\n",
                issue_number, repo_slug, state, label_str, title, author, body
            );

            if let Some(comments) = v["comments"].as_array() {
                if !comments.is_empty() {
                    res.push_str(&format!("\n---\n### Comments ({}):\n\n", comments.len()));
                    for (i, c) in comments.iter().enumerate() {
                        let c_author = c["author"]["login"].as_str().unwrap_or("unknown");
                        let c_body = c["body"].as_str().unwrap_or("").trim();
                        res.push_str(&format!("{}. **{}**: {}\n\n", i + 1, c_author, c_body));
                    }
                }
            }

            return Ok(res);
        }

        // REST fallback
        let endpoint = format!("/repos/{}/issues/{}", repo_slug, issue_number);
        let v = GitHubClient::rest_api(&endpoint, reqwest::Method::GET, None).await?;
        let title = v["title"].as_str().unwrap_or("Untitled");
        let state = v["state"].as_str().unwrap_or("unknown");
        let author = v["user"]["login"].as_str().unwrap_or("unknown");
        let body = v["body"].as_str().unwrap_or("");

        Ok(format!(
            "🎫 Issue #{} in `{}` [State: {}]\n**{}** (by `{}`)\n\n{}\n",
            issue_number, repo_slug, state, title, author, body
        ))
    }

    /// Lists repository issues matching state and limit.
    pub async fn list_issues(
        workspace_root: &Path,
        repo: Option<&str>,
        state: Option<&str>,
        limit: usize,
    ) -> Result<String> {
        let repo_slug = Self::resolve_repo(repo, workspace_root)?;
        let state_val = state.unwrap_or("open");

        if GitHubClient::has_gh_cli() {
            let limit_str = limit.to_string();
            let out = GitHubClient::exec_gh(
                &[
                    "issue",
                    "list",
                    "--repo",
                    &repo_slug,
                    "--state",
                    state_val,
                    "--limit",
                    &limit_str,
                    "--json",
                    "number,title,state,author,labels,updatedAt",
                ],
                workspace_root,
            )?;
            let list: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap_or_default();
            if list.is_empty() {
                return Ok(format!(
                    "ℹ No `{}` issues found in `{}`.",
                    state_val, repo_slug
                ));
            }

            let mut res = format!(
                "🎫 Issues in `{}` [State: {}] ({} issues):\n\n",
                repo_slug,
                state_val,
                list.len()
            );
            for item in list {
                let num = item["number"].as_u64().unwrap_or(0);
                let title = item["title"].as_str().unwrap_or("");
                let author = item["author"]["login"].as_str().unwrap_or("unknown");
                res.push_str(&format!("• #{} **{}** (by `{}`)\n", num, title, author));
            }
            return Ok(res);
        }

        // REST fallback
        let endpoint = format!(
            "/repos/{}/issues?state={}&per_page={}",
            repo_slug, state_val, limit
        );
        let list = GitHubClient::rest_api(&endpoint, reqwest::Method::GET, None).await?;
        let items = list.as_array().ok_or_else(|| {
            ToolError::CommandExec("Expected JSON array from GitHub API".to_string())
        })?;

        if items.is_empty() {
            return Ok(format!(
                "ℹ No `{}` issues found in `{}`.",
                state_val, repo_slug
            ));
        }

        let mut res = format!(
            "🎫 Issues in `{}` [State: {}] ({} issues):\n\n",
            repo_slug,
            state_val,
            items.len()
        );
        for item in items {
            let num = item["number"].as_u64().unwrap_or(0);
            let title = item["title"].as_str().unwrap_or("");
            let author = item["user"]["login"].as_str().unwrap_or("unknown");
            res.push_str(&format!("• #{} **{}** (by `{}`)\n", num, title, author));
        }
        Ok(res)
    }

    /// Creates a new issue.
    pub async fn create_issue(
        workspace_root: &Path,
        repo: Option<&str>,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<String> {
        let repo_slug = Self::resolve_repo(repo, workspace_root)?;

        if GitHubClient::has_gh_cli() {
            let mut args = vec![
                "issue", "create", "--repo", &repo_slug, "--title", title, "--body", body,
            ];
            let label_joined = labels.join(",");
            if !labels.is_empty() {
                args.push("--label");
                args.push(&label_joined);
            }

            let out = GitHubClient::exec_gh(&args, workspace_root)?;
            return Ok(format!(
                "✔ Issue created successfully in `{}`:\n{}",
                repo_slug, out
            ));
        }

        // REST fallback
        let endpoint = format!("/repos/{}/issues", repo_slug);
        let body_json = serde_json::json!({
            "title": title,
            "body": body,
            "labels": labels,
        });

        let resp =
            GitHubClient::rest_api(&endpoint, reqwest::Method::POST, Some(body_json)).await?;
        let num = resp["number"].as_u64().unwrap_or(0);
        let html_url = resp["html_url"].as_str().unwrap_or("");

        Ok(format!(
            "✔ Issue #{} created in `{}`: {}",
            num, repo_slug, html_url
        ))
    }

    /// Views pull request metadata and status.
    pub async fn view_pr(
        workspace_root: &Path,
        repo: Option<&str>,
        pr_number: u64,
    ) -> Result<String> {
        let repo_slug = Self::resolve_repo(repo, workspace_root)?;

        if GitHubClient::has_gh_cli() {
            let num_str = pr_number.to_string();
            let out = GitHubClient::exec_gh(
                &[
                    "pr",
                    "view",
                    &num_str,
                    "--repo",
                    &repo_slug,
                    "--json",
                    "number,title,body,state,author,headRefName,baseRefName,additions,deletions,url",
                ],
                workspace_root,
            )?;
            let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
            let title = v["title"].as_str().unwrap_or("Untitled PR");
            let state = v["state"].as_str().unwrap_or("unknown");
            let author = v["author"]["login"].as_str().unwrap_or("unknown");
            let head = v["headRefName"].as_str().unwrap_or("");
            let base = v["baseRefName"].as_str().unwrap_or("");
            let adds = v["additions"].as_u64().unwrap_or(0);
            let dels = v["deletions"].as_u64().unwrap_or(0);
            let body = v["body"].as_str().unwrap_or("").trim();
            let url = v["url"].as_str().unwrap_or("");

            return Ok(format!(
                "🔀 Pull Request #{} in `{}` [State: {}]\n**{}** (by `{}`)\nBranch: `{}` ➔ `{}` (+{} / -{})\nURL: {}\n\n{}\n",
                pr_number, repo_slug, state, title, author, head, base, adds, dels, url, body
            ));
        }

        // REST fallback
        let endpoint = format!("/repos/{}/pulls/{}", repo_slug, pr_number);
        let v = GitHubClient::rest_api(&endpoint, reqwest::Method::GET, None).await?;
        let title = v["title"].as_str().unwrap_or("Untitled PR");
        let state = v["state"].as_str().unwrap_or("unknown");
        let author = v["user"]["login"].as_str().unwrap_or("unknown");
        let head = v["head"]["ref"].as_str().unwrap_or("");
        let base = v["base"]["ref"].as_str().unwrap_or("");
        let body = v["body"].as_str().unwrap_or("").trim();
        let url = v["html_url"].as_str().unwrap_or("");

        Ok(format!(
            "🔀 Pull Request #{} in `{}` [State: {}]\n**{}** (by `{}`)\nBranch: `{}` ➔ `{}`\nURL: {}\n\n{}\n",
            pr_number, repo_slug, state, title, author, head, base, url, body
        ))
    }

    /// Fetches the unified diff of a pull request.
    pub async fn view_pr_diff(
        workspace_root: &Path,
        repo: Option<&str>,
        pr_number: u64,
    ) -> Result<String> {
        let repo_slug = Self::resolve_repo(repo, workspace_root)?;

        if GitHubClient::has_gh_cli() {
            let num_str = pr_number.to_string();
            let diff = GitHubClient::exec_gh(
                &["pr", "diff", &num_str, "--repo", &repo_slug],
                workspace_root,
            )?;
            return Ok(format!(
                "📄 Diff for PR #{} in `{}`:\n```diff\n{}\n```",
                pr_number, repo_slug, diff
            ));
        }

        let token = crate::tools::github::client::get_github_token()?;

        let url = format!(
            "https://api.github.com/repos/{}/pulls/{}",
            repo_slug, pr_number
        );
        let client = reqwest::Client::builder().build().unwrap_or_default();
        let resp = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.github.v3.diff")
            .header("User-Agent", "minicode-agent")
            .send()
            .await
            .map_err(|e| ToolError::CommandExec(format!("Failed to fetch PR diff: {}", e)))?;

        let diff_text = resp.text().await.unwrap_or_default();
        Ok(format!(
            "📄 Diff for PR #{} in `{}`:\n```diff\n{}\n```",
            pr_number, repo_slug, diff_text
        ))
    }

    /// Opens a new pull request.
    pub async fn create_pr(
        workspace_root: &Path,
        repo: Option<&str>,
        title: &str,
        body: &str,
        base: &str,
        draft: bool,
    ) -> Result<String> {
        let repo_slug = Self::resolve_repo(repo, workspace_root)?;

        if GitHubClient::has_gh_cli() {
            let mut args = vec![
                "pr", "create", "--repo", &repo_slug, "--title", title, "--body", body, "--base",
                base,
            ];
            if draft {
                args.push("--draft");
            }

            let out = GitHubClient::exec_gh(&args, workspace_root)?;
            return Ok(format!(
                "✔ Pull Request created in `{}`:\n{}",
                repo_slug, out
            ));
        }

        // REST fallback
        let current_branch = std::process::Command::new("git")
            .current_dir(workspace_root)
            .args(["branch", "--show-current"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "HEAD".to_string());

        let endpoint = format!("/repos/{}/pulls", repo_slug);
        let body_json = serde_json::json!({
            "title": title,
            "body": body,
            "head": current_branch,
            "base": base,
            "draft": draft,
        });

        let resp =
            GitHubClient::rest_api(&endpoint, reqwest::Method::POST, Some(body_json)).await?;
        let num = resp["number"].as_u64().unwrap_or(0);
        let html_url = resp["html_url"].as_str().unwrap_or("");

        Ok(format!(
            "✔ Pull Request #{} opened in `{}`: {}",
            num, repo_slug, html_url
        ))
    }

    /// Fetches GitHub Actions CI workflow runs and status.
    pub async fn get_ci_status(
        workspace_root: &Path,
        repo: Option<&str>,
        branch: Option<&str>,
        limit: usize,
    ) -> Result<String> {
        let repo_slug = Self::resolve_repo(repo, workspace_root)?;

        if GitHubClient::has_gh_cli() {
            let limit_str = limit.to_string();
            let mut args = vec![
                "run",
                "list",
                "--repo",
                &repo_slug,
                "--limit",
                &limit_str,
                "--json",
                "databaseId,name,status,conclusion,event,headBranch,headSha,url,createdAt",
            ];
            if let Some(b) = branch {
                args.push("--branch");
                args.push(b);
            }

            let out = GitHubClient::exec_gh(&args, workspace_root)?;
            let runs: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap_or_default();
            if runs.is_empty() {
                return Ok(format!("ℹ No CI workflow runs found in `{}`.", repo_slug));
            }

            let mut res = format!(
                "⚙ GitHub Actions CI Runs in `{}` ({} runs):\n\n",
                repo_slug,
                runs.len()
            );
            for r in runs {
                let id = r["databaseId"].as_u64().unwrap_or(0);
                let name = r["name"].as_str().unwrap_or("Workflow");
                let status = r["status"].as_str().unwrap_or("unknown");
                let conclusion = r["conclusion"].as_str().unwrap_or("running");
                let b = r["headBranch"].as_str().unwrap_or("");
                let icon = match conclusion {
                    "success" => "✔",
                    "failure" => "✗",
                    "cancelled" => "○",
                    _ => "⠋",
                };
                res.push_str(&format!(
                    "{} Run #{} **{}** [{}/{}] (branch: `{}`)\n",
                    icon, id, name, status, conclusion, b
                ));
            }
            return Ok(res);
        }

        // REST fallback
        let endpoint = format!("/repos/{}/actions/runs?per_page={}", repo_slug, limit);
        let resp = GitHubClient::rest_api(&endpoint, reqwest::Method::GET, None).await?;
        let runs = resp["workflow_runs"].as_array().ok_or_else(|| {
            ToolError::CommandExec("Missing workflow_runs array in GitHub API response".to_string())
        })?;

        if runs.is_empty() {
            return Ok(format!("ℹ No CI workflow runs found in `{}`.", repo_slug));
        }

        let mut res = format!(
            "⚙ GitHub Actions CI Runs in `{}` ({} runs):\n\n",
            repo_slug,
            runs.len()
        );
        for r in runs {
            let id = r["id"].as_u64().unwrap_or(0);
            let name = r["name"].as_str().unwrap_or("Workflow");
            let status = r["status"].as_str().unwrap_or("unknown");
            let conclusion = r["conclusion"].as_str().unwrap_or("running");
            let b = r["head_branch"].as_str().unwrap_or("");
            let icon = match conclusion {
                "success" => "✔",
                "failure" => "✗",
                "cancelled" => "○",
                _ => "⠋",
            };
            res.push_str(&format!(
                "{} Run #{} **{}** [{}/{}] (branch: `{}`)\n",
                icon, id, name, status, conclusion, b
            ));
        }
        Ok(res)
    }

    /// Fetches failing job logs for a specific workflow run.
    pub async fn get_ci_logs(
        workspace_root: &Path,
        repo: Option<&str>,
        run_id: u64,
    ) -> Result<String> {
        let repo_slug = Self::resolve_repo(repo, workspace_root)?;

        if GitHubClient::has_gh_cli() {
            let id_str = run_id.to_string();
            let logs = GitHubClient::exec_gh(
                &["run", "view", &id_str, "--repo", &repo_slug, "--log-failed"],
                workspace_root,
            );

            match logs {
                Ok(l) if !l.trim().is_empty() => {
                    let snippet = if l.len() > 3000 {
                        let offset = l.floor_char_boundary(l.len().saturating_sub(3000));
                        format!("... (truncated)\n{}", &l[offset..])
                    } else {
                        l
                    };
                    return Ok(format!(
                        "📋 Failed CI Logs for Run #{} in `{}`:\n```\n{}\n```",
                        run_id, repo_slug, snippet
                    ));
                }
                _ => {
                    return Ok(format!("ℹ No failure logs found for CI run #{} (it may have succeeded or expired).", run_id));
                }
            }
        }

        // REST API check
        let endpoint = format!("/repos/{}/actions/runs/{}/jobs", repo_slug, run_id);
        let resp = GitHubClient::rest_api(&endpoint, reqwest::Method::GET, None).await?;
        let jobs = resp["jobs"].as_array().cloned().unwrap_or_default();

        let mut res = format!("📋 CI Jobs for Run #{} in `{}`:\n\n", run_id, repo_slug);
        for j in jobs {
            let name = j["name"].as_str().unwrap_or("Job");
            let conclusion = j["conclusion"].as_str().unwrap_or("running");
            res.push_str(&format!(
                "• Job **{}** [Conclusion: {}]\n",
                name, conclusion
            ));
        }
        Ok(res)
    }
}
