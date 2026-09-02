// ============================================================
// Token resolution — single source of truth for GITHUB_TOKEN / GH_TOKEN
// ============================================================
const GITHUB_TOKEN_VARS: &[&str] = &["GITHUB_TOKEN", "GH_TOKEN"];

/// Returns the first available GitHub token from the environment,
/// or an error if neither is set.
pub fn get_github_token() -> crate::error::Result<String> {
    let token = GITHUB_TOKEN_VARS
        .iter()
        .find_map(|v| std::env::var(v).ok())
        .ok_or_else(|| {
            crate::error::ToolError::CommandExec(
                "GitHub authentication required: set GITHUB_TOKEN or GH_TOKEN".into(),
            )
        })?;
    if token.is_empty() {
        return Err(crate::error::ToolError::CommandExec(
            "GitHub authentication required: set GITHUB_TOKEN or GH_TOKEN".into(),
        )
        .into());
    }
    Ok(token)
}

use crate::error::{Result, ToolError};
use std::path::Path;
use std::process::Command;

/// Client abstraction supporting both GitHub CLI (`gh`) and REST API fallback.
pub struct GitHubClient;

impl GitHubClient {
    /// Extracts `owner/repo` from a remote Git URL.
    pub fn parse_repo_slug(url: &str) -> Option<String> {
        let trimmed = url.trim();
        let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);

        // Format 1: git@github.com:owner/repo
        if let Some(idx) = trimmed.find("github.com:") {
            let slug = &trimmed[idx + "github.com:".len()..];
            return Some(slug.trim_start_matches('/').to_string());
        }

        // Format 2: https://github.com/owner/repo
        if let Some(idx) = trimmed.find("github.com/") {
            let slug = &trimmed[idx + "github.com/".len()..];
            return Some(slug.trim_start_matches('/').to_string());
        }

        None
    }

    /// Auto-detects the current GitHub repository slug from git config in the workspace.
    pub fn detect_repo(workspace_root: &Path) -> Result<String> {
        let output = Command::new("git")
            .current_dir(workspace_root)
            .args(["config", "--get", "remote.origin.url"])
            .output()
            .map_err(|e| ToolError::CommandExec(format!("Failed to run git config: {}", e)))?;

        if !output.status.success() {
            return Err(ToolError::CommandExec(
                "No git remote origin configured in the current repository.".to_string(),
            )
            .into());
        }

        let raw_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Self::parse_repo_slug(&raw_url).ok_or_else(|| {
            ToolError::CommandExec(format!(
                "Could not extract GitHub owner/repo slug from remote URL: `{}`",
                raw_url
            ))
            .into()
        })
    }

    /// Checks if the GitHub CLI (`gh`) is installed and authenticated.
    pub fn has_gh_cli() -> bool {
        Command::new("gh")
            .args(["auth", "status"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Executes a `gh` command in the workspace directory.
    pub fn exec_gh(args: &[&str], workspace_root: &Path) -> Result<String> {
        let output = Command::new("gh")
            .current_dir(workspace_root)
            .args(args)
            .output()
            .map_err(|e| ToolError::CommandExec(format!("Failed to execute `gh`: {}", e)))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::CommandExec(format!("`gh` error: {}", err.trim())).into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Executes a call to the GitHub REST API using the shared token helper.
    pub async fn rest_api(
        endpoint: &str,
        method: reqwest::Method,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let token = get_github_token()?;

        let url = if endpoint.starts_with("https://") {
            endpoint.to_string()
        } else {
            format!(
                "https://api.github.com/{}",
                endpoint.trim_start_matches('/')
            )
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(format!("minicode-agent/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();

        let mut req = client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ToolError::CommandExec(format!("GitHub API request failed: {}", e)))?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(ToolError::CommandExec(format!(
                "GitHub API error (HTTP {}): {}",
                status, text
            ))
            .into());
        }

        let json_val: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            ToolError::CommandExec(format!("Failed to parse GitHub API JSON: {}", e))
        })?;

        Ok(json_val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_repo_slug() {
        assert_eq!(
            GitHubClient::parse_repo_slug("git@github.com:aswin402/minicode.git"),
            Some("aswin402/minicode".to_string())
        );
        assert_eq!(
            GitHubClient::parse_repo_slug("https://github.com/aswin402/minicode.git"),
            Some("aswin402/minicode".to_string())
        );
        assert_eq!(
            GitHubClient::parse_repo_slug("https://github.com/tokio-rs/tokio"),
            Some("tokio-rs/tokio".to_string())
        );
        assert_eq!(
            GitHubClient::parse_repo_slug("https://gitlab.com/user/project"),
            None
        );
    }
}
