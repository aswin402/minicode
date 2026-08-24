/// Integration tests for Phase 34: Actor-Critic Dual-Agent Code Verification Loop
///
/// Tests multi-dimensional validation: compiler diagnostics, anti-patterns,
/// secret leaks, verdict formatting, and tool schema registration.
use minicode::agent::critic::{
    CriticIssue, CriticReport, CriticValidator, CriticVerdict, IssueSeverity,
};
use minicode::tools::registry::agent_tools;
use tempfile::tempdir;

#[tokio::test]
async fn test_critic_clean_workspace_approved() {
    let dir = tempdir().unwrap();
    let report = CriticValidator::review_workspace(dir.path()).await.unwrap();

    assert!(report.is_approved);
    assert_eq!(report.verdict, CriticVerdict::Approved);
    assert_eq!(report.compiler_errors, 0);
    assert!(report.compiler_clean);
    assert!(report.conventions_clean);
    assert!(report.security_clean);
    assert!(report.issues.is_empty());
}

#[tokio::test]
async fn test_critic_report_formatting_with_issues() {
    let report = CriticReport {
        verdict: CriticVerdict::Rejected,
        is_approved: false,
        compiler_clean: false,
        conventions_clean: true,
        security_clean: false,
        compiler_errors: 1,
        compiler_warnings: 0,
        uncommitted_files: vec!["src/agent/loop.rs".to_string()],
        issues: vec![
            CriticIssue {
                category: "compiler_error".to_string(),
                severity: IssueSeverity::Critical,
                file_path: Some("src/agent/loop.rs".to_string()),
                line_number: Some(42),
                description: "mismatched types: expected `Result<()>`, found `()`".to_string(),
                recommendation: "Wrap return value in `Ok(())`.".to_string(),
            },
            CriticIssue {
                category: "security_secret_leak".to_string(),
                severity: IssueSeverity::Critical,
                file_path: Some("src/config.rs".to_string()),
                line_number: None,
                description: "File contains unredacted API key or secret token.".to_string(),
                recommendation: "Remove or mask hardcoded secret.".to_string(),
            },
        ],
        summary: "Critic rejected: 2 critical issues".to_string(),
    };

    let md = report.format_for_agent();
    assert!(md.contains("Critic Verification: ✗ Rejected"));
    assert!(md.contains("[CRITICAL]"));
    assert!(md.contains("compiler_error"));
    assert!(md.contains("security_secret_leak"));
    assert!(md.contains("src/agent/loop.rs:42"));
}

#[tokio::test]
async fn test_critic_verdict_severity_logic() {
    assert!(CriticVerdict::Approved.is_approved());
    assert!(CriticVerdict::ApprovedWithWarnings.is_approved());
    assert!(!CriticVerdict::Rejected.is_approved());

    assert_eq!(IssueSeverity::Critical.badge(), "[CRITICAL]");
    assert_eq!(IssueSeverity::Warning.badge(), "[WARNING]");
    assert_eq!(IssueSeverity::Info.badge(), "[INFO]");
}

#[test]
fn test_critic_tool_schema_registered() {
    let schemas = agent_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"critic_review".to_string()));
}
