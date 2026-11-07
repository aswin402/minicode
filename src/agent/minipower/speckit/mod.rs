//! Native Rust implementation of `github/spec-kit` Specification-Driven Development (SDD)
//! inside `MiniPower`:
//!
//! 1. [`traceability`]: Bipartite & Multipartite Cross-Artifact Traceability Graph (`speckit.analyze`)
//!    and 5-Dimension Requirements-Quality Checklist Evaluator (`speckit.checklist` — "Unit Tests for Requirements").
//! 2. [`convergence`]: Append-Only Implementation Convergence Engine (`speckit.converge`) with
//!    strict `Converged` (`✅ CONVERGED`, byte-for-byte unchanged `todo.md`) vs `TasksAppended` (`## Convergence`) semantics.
//! 3. [`workflows`]: Scoped 3-Stage Bug Triage State Machine (`bug.assess` → `bug.fix` → `bug.test` with
//!    set-difference `## Deviations from Assessment` and reproduction downgrade to `PARTIAL`) and
//!    5-Stage Evidence-Weighted Idea Assessment Scorecard (`intake` → `research` → `define` → `shape` → `decide`).

pub mod convergence;
pub mod traceability;
pub mod workflows;

pub use convergence::{converge_workspace, ConvergenceReport, ConvergenceVerdict};
pub use traceability::{
    analyze_workspace_artifacts, evaluate_requirements_checklist, AmbiguityMarkerHit,
    ArtifactLayer, ArtifactNode, ChecklistDimensionResult, CrossArtifactAnalysisReport,
    RequirementsChecklistReport, TraceabilityEdge, TraceableIssuePayload,
};
pub use workflows::{
    execute_bug_workflow_stage, execute_idea_assessment, AssessmentDecision, AssessmentRating,
    AssessmentStage, BugStage, BugStageInput, BugVerdict, BugWorkflowState, IdeaAssessmentInput,
    IdeaAssessmentReport,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_artifact_traceability_and_checklist() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let core_dir = root.join("minikit_docs/core");
        std::fs::create_dir_all(&core_dir).expect("create core_dir");
        std::fs::write(root.join("AGENTS.md"), "# Constitution\n").expect("write constitution");

        std::fs::write(
            core_dir.join("spec.md"),
            r#"# Authentication Spec

## Functional Requirements
- The system must validate JWT bearer tokens in `src/auth.rs` and reject expired signatures.
- The system shall rate-limit login attempts in `src/rate_limit.rs` [NEEDS CLARIFICATION: max requests per minute?].

## Non-Goals & Out of Scope
- OAuth2 social login is out of scope for v1.
"#,
        )
        .expect("write spec");

        std::fs::write(
            core_dir.join("architecture.md"),
            r#"# Architecture
## Security Layer
- Implement JWT token verification middleware in `src/auth.rs`.
"#,
        )
        .expect("write arch");

        std::fs::write(
            core_dir.join("todo.md"),
            r#"# Tasks
- [x] Implement JWT bearer token validation in `src/auth.rs`
- [ ] Add rainbow confetti animation to `src/confetti.rs`
"#,
        )
        .expect("write todo");

        let report = analyze_workspace_artifacts(root);
        assert_eq!(report.requirements.len(), 2);
        assert_eq!(report.plan_components.len(), 1);
        assert_eq!(report.tasks.len(), 2);
        assert_eq!(report.unmapped_requirements.len(), 1);
        assert!(report.unmapped_requirements[0]
            .text
            .contains("rate-limit login attempts"));
        assert_eq!(report.orphan_tasks.len(), 1);
        assert!(report.orphan_tasks[0].text.contains("confetti"));
        assert_eq!(report.ambiguity_hits.len(), 1);
        assert_eq!(report.ambiguity_hits[0].marker, "NEEDS CLARIFICATION");
        assert_eq!(report.coverage_pct, 50);
        assert!(!report.passed);

        let checklist = evaluate_requirements_checklist(root, true);
        assert!(core_dir.join("checklists/requirements.md").exists());
        assert!(!checklist.passed);
        // RQ-1 fails due to [NEEDS CLARIFICATION], RQ-3 passes due to Non-Goals section
        let rq1 = checklist
            .dimensions
            .iter()
            .find(|d| d.id == "RQ-1")
            .expect("RQ-1");
        let rq3 = checklist
            .dimensions
            .iter()
            .find(|d| d.id == "RQ-3")
            .expect("RQ-3");
        assert!(!rq1.passed);
        assert!(rq3.passed);

        let issues = report.synthesize_traceable_issues();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].title.contains("confetti"));
    }

    #[test]
    fn test_append_only_convergence_engine() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let core_dir = root.join("minikit_docs/core");
        let src_dir = root.join("src");
        std::fs::create_dir_all(&core_dir).expect("create core_dir");
        std::fs::create_dir_all(&src_dir).expect("create src_dir");

        std::fs::write(
            core_dir.join("spec.md"),
            r#"# Storage Spec
## Requirements
- Persist session state to `src/storage.rs` and `src/index.rs`.
"#,
        )
        .expect("write spec");

        let initial_todo = "# Tasks\n- [x] Persist session state to `src/storage.rs`\n";
        std::fs::write(core_dir.join("todo.md"), initial_todo).expect("write todo");
        std::fs::write(src_dir.join("storage.rs"), "pub struct Storage;\n").expect("write storage");

        // `src/index.rs` is missing on disk -> should append delta task under `## Convergence`
        let conv1 = converge_workspace(root, true);
        assert_eq!(conv1.verdict, ConvergenceVerdict::TasksAppended);
        assert_eq!(conv1.missing_deliverables, vec!["src/index.rs".to_string()]);
        assert_eq!(conv1.appended_tasks.len(), 1);

        let todo_after_append =
            std::fs::read_to_string(core_dir.join("todo.md")).expect("read todo");
        assert!(todo_after_append.starts_with(initial_todo));
        assert!(todo_after_append.contains("## Convergence"));
        assert!(todo_after_append.contains("src/index.rs"));

        // Now create `src/index.rs` and mark all tasks `[x]` -> must converge and leave `todo.md` byte-for-byte unchanged
        std::fs::write(src_dir.join("index.rs"), "pub struct Index;\n").expect("write index");
        let completed_todo = todo_after_append.replace("- [ ] ", "- [x] ");
        std::fs::write(core_dir.join("todo.md"), &completed_todo).expect("write completed todo");

        let conv2 = converge_workspace(root, true);
        assert_eq!(conv2.verdict, ConvergenceVerdict::Converged);
        let todo_after_converged =
            std::fs::read_to_string(core_dir.join("todo.md")).expect("read todo after converged");
        assert_eq!(todo_after_converged, completed_todo);
    }

    #[test]
    fn test_scoped_bug_triage_state_machine_and_deviation_detection() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        // Stage 1: Assess
        let s1 = execute_bug_workflow_stage(
            root,
            BugStageInput {
                title: "Panic on Empty Header".to_string(),
                stage: Some(BugStage::Assess),
                root_cause: Some("Unchecked slice index in parser".to_string()),
                reproduction_command: Some("cargo test test_empty_header".to_string()),
                target_files: vec!["src/parser.rs".to_string()],
                ..Default::default()
            },
        )
        .expect("assess stage");
        assert_eq!(s1.stage, BugStage::Assess);
        assert!(s1.artifact_path.ends_with("assessment.md"));

        // Stage 2: Fix (modifying both `src/parser.rs` AND out-of-scope `src/http.rs`)
        let s2 = execute_bug_workflow_stage(
            root,
            BugStageInput {
                title: "Panic on Empty Header".to_string(),
                stage: Some(BugStage::Fix),
                modified_files: vec!["src/parser.rs".to_string(), "src/http.rs".to_string()],
                fix_summary: Some("Replaced slice index with `.get()`".to_string()),
                deviation_rationale: Some(
                    "Updated caller error variant in `src/http.rs`".to_string(),
                ),
                ..Default::default()
            },
        )
        .expect("fix stage");
        assert_eq!(s2.out_of_scope_deviations, vec!["src/http.rs".to_string()]);
        let fix_md = std::fs::read_to_string(&s2.artifact_path).expect("read fix.md");
        assert!(fix_md.contains("## Deviations from Assessment"));
        assert!(fix_md.contains("src/http.rs"));

        // Stage 3a: Test with tests_passed=true but repro_exercised=false -> must downgrade to PARTIAL
        let s3_partial = execute_bug_workflow_stage(
            root,
            BugStageInput {
                title: "Panic on Empty Header".to_string(),
                stage: Some(BugStage::Test),
                tests_passed: Some(true),
                repro_exercised: Some(false),
                ..Default::default()
            },
        )
        .expect("test partial");
        assert_eq!(s3_partial.verdict, Some(BugVerdict::Partial));

        // Stage 3b: Test with both tests_passed=true and repro_exercised=true -> VERIFIED
        let s3_verified = execute_bug_workflow_stage(
            root,
            BugStageInput {
                title: "Panic on Empty Header".to_string(),
                stage: Some(BugStage::Test),
                tests_passed: Some(true),
                repro_exercised: Some(true),
                ..Default::default()
            },
        )
        .expect("test verified");
        assert_eq!(s3_verified.verdict, Some(BugVerdict::Verified));
    }

    #[test]
    fn test_idea_assessment_scorecard() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        // 1. Intake with an unresolved `[ASSUMPTION]` marker -> NeedsClarification
        let r_clarify = execute_idea_assessment(
            root,
            IdeaAssessmentInput {
                topic: "GraphQL Gateway".to_string(),
                stage: Some(AssessmentStage::Intake),
                content: Some(
                    "Users want GraphQL [ASSUMPTION: mobile clients need subfield selection]"
                        .to_string(),
                ),
                problem_validity: Some(AssessmentRating::Strong),
                evidence_strength: Some(AssessmentRating::Strong),
                counter_evidence_documented: Some(true),
                concept_viability: Some(AssessmentRating::Strong),
                assumptions: Vec::new(),
            },
        )
        .expect("intake with assumption");
        assert_eq!(r_clarify.decision, AssessmentDecision::NeedsClarification);

        // 2. Weak problem validity -> Kill
        let r_kill = execute_idea_assessment(
            root,
            IdeaAssessmentInput {
                topic: "Blockchain Logger".to_string(),
                stage: Some(AssessmentStage::Decide),
                problem_validity: Some(AssessmentRating::Weak),
                evidence_strength: Some(AssessmentRating::Adequate),
                counter_evidence_documented: Some(true),
                concept_viability: Some(AssessmentRating::Adequate),
                ..Default::default()
            },
        )
        .expect("kill decision");
        assert_eq!(r_kill.decision, AssessmentDecision::Kill);

        // 3. Strong/Adequate across all dimensions and 0 unresolved assumptions -> Go
        let r_go = execute_idea_assessment(
            root,
            IdeaAssessmentInput {
                topic: "AST Trigram Index".to_string(),
                stage: Some(AssessmentStage::Decide),
                content: Some(
                    "Validated 4x latency reduction on 100k LOC repositories.".to_string(),
                ),
                problem_validity: Some(AssessmentRating::Strong),
                evidence_strength: Some(AssessmentRating::Strong),
                counter_evidence_documented: Some(true),
                concept_viability: Some(AssessmentRating::Adequate),
                assumptions: Vec::new(),
            },
        )
        .expect("go decision");
        assert_eq!(r_go.decision, AssessmentDecision::Go);
    }
}
