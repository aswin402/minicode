use serde::{Deserialize, Serialize};

/// A GitHub Issue object.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitHubIssue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub author: String,
    pub labels: Vec<String>,
    pub comments_count: usize,
    pub url: String,
    pub created_at: String,
}

/// A GitHub Pull Request object.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitHubPR {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub head_branch: String,
    pub base_branch: String,
    pub author: String,
    pub is_draft: bool,
    pub url: String,
    pub add_count: usize,
    pub del_count: usize,
}

/// A GitHub Review Comment on a pull request.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitHubComment {
    pub id: u64,
    pub author: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub created_at: String,
}

/// A GitHub Actions Workflow Run.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitHubWorkflowRun {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub event: String,
    pub branch: String,
    pub commit_sha: String,
    pub url: String,
    pub created_at: String,
}
