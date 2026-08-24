/// Integration tests for Phase 45: Native GitHub Integration & CI Workflow Diagnoser
///
/// Tests repository URL parsing, GitHub schema registration, and issue/PR/CI data structures.
use minicode::tools::github::client::GitHubClient;
use minicode::tools::github::types::{GitHubIssue, GitHubPR, GitHubWorkflowRun};
use minicode::tools::registry::git_tools;

#[test]
fn test_github_repo_slug_parsing() {
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
        GitHubClient::parse_repo_slug("git@github.com:rust-lang/rust.git"),
        Some("rust-lang/rust".to_string())
    );
    assert_eq!(
        GitHubClient::parse_repo_slug("https://gitlab.com/user/project"),
        None
    );
}

#[test]
fn test_github_schemas_registered_in_registry() {
    let schemas = git_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"github_issue_view".to_string()));
    assert!(names.contains(&"github_issue_list".to_string()));
    assert!(names.contains(&"github_issue_create".to_string()));
    assert!(names.contains(&"github_pr_view".to_string()));
    assert!(names.contains(&"github_pr_diff".to_string()));
    assert!(names.contains(&"github_pr_create".to_string()));
    assert!(names.contains(&"github_ci_status".to_string()));
    assert!(names.contains(&"github_ci_logs".to_string()));
}

#[test]
fn test_github_data_types_serialization() {
    let issue = GitHubIssue {
        number: 42,
        title: "Memory leak in connection pool buffer".to_string(),
        body: "Reproducible with 10 concurrent requests.".to_string(),
        state: "open".to_string(),
        author: "aswin".to_string(),
        labels: vec!["bug".to_string(), "priority-high".to_string()],
        comments_count: 2,
        url: "https://github.com/aswin402/minicode/issues/42".to_string(),
        created_at: "2026-08-25T01:00:00Z".to_string(),
    };

    let serialized = serde_json::to_string(&issue).unwrap();
    let deserialized: GitHubIssue = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.number, 42);
    assert_eq!(deserialized.labels.len(), 2);

    let pr = GitHubPR {
        number: 101,
        title: "feat: subagent shared scratchpad".to_string(),
        body: "Adds SharedScratchpad blackboard.".to_string(),
        state: "open".to_string(),
        head_branch: "feat/scratchpad".to_string(),
        base_branch: "main".to_string(),
        author: "aswin".to_string(),
        is_draft: false,
        url: "https://github.com/aswin402/minicode/pull/101".to_string(),
        add_count: 250,
        del_count: 10,
    };
    assert_eq!(pr.add_count, 250);

    let run = GitHubWorkflowRun {
        id: 998877,
        name: "Rust CI".to_string(),
        status: "completed".to_string(),
        conclusion: Some("success".to_string()),
        event: "push".to_string(),
        branch: "main".to_string(),
        commit_sha: "abc1234".to_string(),
        url: "https://github.com/aswin402/minicode/actions/runs/998877".to_string(),
        created_at: "2026-08-25T01:10:00Z".to_string(),
    };
    assert_eq!(run.conclusion.as_deref(), Some("success"));
}
