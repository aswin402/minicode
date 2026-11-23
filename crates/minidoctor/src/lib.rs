//! `minidoctor`: Universal multi-stack native Rust codebase health diagnostic & AI remediation engine.
//!
//! Designed as both:
//! 1. An in-process zero-overhead library embedded in `minicode` (post-edit AST micro-guard,
//!    `VerificationBarrier` changed-scope regression gate, `project_doctor` tool, and animated `/doctor` TUI).
//! 2. A standalone native CLI binary (`minidoctor`) for local runs, pre-commit hooks, and CI.

pub mod config;
pub mod dead_code;
pub mod detector;
pub mod report;
pub mod rules;
pub mod scorer;
pub mod types;

pub use config::DoctorConfig;
pub use report::{
    artifacts_dir, format_agent_compact_summary, format_neededfix_markdown, format_sarif_json,
    format_terminal_report, load_latest_report, persist_workspace_artifacts,
};
pub use rules::catalog::{explain_rule, list_rules};
pub use scorer::render_sparkline;
pub use types::{
    DeadCodeKind, DetectedStack, Diagnostic, DiagnosticCategory, DoctorOptions, DoctorReport,
    HistoryEntry, ProjectProfile, RuleMeta, ScanPhase, ScanProgress, ScanScope, ScoreBreakdown,
    Severity,
};

use anyhow::Result;
use chrono::Utc;
use std::path::Path;
use std::time::Instant;

/// Runs a full or scoped health diagnosis without a progress callback.
pub fn run_doctor(workspace: &Path, options: &DoctorOptions) -> Result<DoctorReport> {
    run_doctor_with_progress(workspace, options, |_| {})
}

/// Runs a health diagnosis streaming live [`ScanProgress`] packets across all 5 analysis phases.
pub fn run_doctor_with_progress<F>(
    workspace: &Path,
    options: &DoctorOptions,
    mut on_progress: F,
) -> Result<DoctorReport>
where
    F: FnMut(ScanProgress),
{
    let started = Instant::now();
    let cfg = DoctorConfig::load(workspace);
    let threshold = options.threshold_override.unwrap_or(cfg.threshold);

    on_progress(ScanProgress {
        phase: ScanPhase::DetectingStacks,
        percent: 8,
        detail: "Reading project manifests & resolving scan scope...".to_string(),
        detected_stacks: Vec::new(),
        files_scanned: 0,
        findings_so_far: 0,
    });

    let files = detector::collect_source_files(workspace, &options.scope, &cfg, options.max_files);
    let profile = detector::detect_project_profile(workspace, &files);
    let git_head = detector::current_git_head(workspace);

    on_progress(ScanProgress {
        phase: ScanPhase::NativeAstRules,
        percent: 24,
        detail: format!(
            "Detected {} · scanning {} file(s) with native Tree-sitter AST rules...",
            profile.stack_summary(),
            files.len()
        ),
        detected_stacks: profile.stacks.clone(),
        files_scanned: 0,
        findings_so_far: 0,
    });

    let mut diagnostics = Vec::new();
    let run_security = options.run_security && cfg.security;

    if cfg.lint {
        let total = files.len().max(1);
        for (idx, file) in files.iter().enumerate() {
            let file_diags = rules::analyze_file(file, &profile.stacks, &cfg, run_security);
            diagnostics.extend(file_diags);

            // Emit smooth intermediate progress ticks every ~8 files
            if idx % 8 == 0 || idx + 1 == files.len() {
                let pct = 24 + (((idx + 1) * 44) / total).min(44) as u8;
                on_progress(ScanProgress {
                    phase: ScanPhase::NativeAstRules,
                    percent: pct,
                    detail: format!("Analyzing {}", file.rel_path),
                    detected_stacks: profile.stacks.clone(),
                    files_scanned: idx + 1,
                    findings_so_far: diagnostics.len(),
                });
            }
        }
    }

    // Dead code & duplicate block pass (skipped for tiny single-file diff scopes to avoid false unused-export noise)
    let run_dead =
        options.run_dead_code && cfg.dead_code && matches!(options.scope, ScanScope::Full);
    if run_dead {
        on_progress(ScanProgress {
            phase: ScanPhase::DeadCodeAndDuplicates,
            percent: 74,
            detail: "Building cross-file symbol reference & duplicate block index...".to_string(),
            detected_stacks: profile.stacks.clone(),
            files_scanned: files.len(),
            findings_so_far: diagnostics.len(),
        });
        let dead = dead_code::detect_dead_code_and_duplicates(&files, &cfg);
        diagnostics.extend(dead);
    }

    on_progress(ScanProgress {
        phase: ScanPhase::SecurityAudit,
        percent: 86,
        detail: "Aggregating security & severity risk classifications...".to_string(),
        detected_stacks: profile.stacks.clone(),
        files_scanned: files.len(),
        findings_so_far: diagnostics.len(),
    });

    // Deterministic priority sort: Critical -> Error -> Warning -> Info, then file:line
    diagnostics.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.rule.cmp(&b.rule))
    });

    on_progress(ScanProgress {
        phase: ScanPhase::ScoringAndArtifacts,
        percent: 94,
        detail: "Computing health score & writing .minicode/minidoctor/NEEDEDFIX.md...".to_string(),
        detected_stacks: profile.stacks.clone(),
        files_scanned: files.len(),
        findings_so_far: diagnostics.len(),
    });

    let history = scorer::load_history(workspace);
    let score = scorer::compute_score(&diagnostics, files.len(), threshold, &history);
    let duration_ms = started.elapsed().as_millis() as u64;

    let mut report = DoctorReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        workspace: workspace.display().to_string(),
        scope_label: options.scope.label(),
        git_head,
        duration_ms,
        profile: profile.clone(),
        score,
        diagnostics,
        neededfix_path: None,
    };

    if options.write_workspace_artifacts && cfg.write_md {
        let _ = report::persist_workspace_artifacts(
            workspace,
            &mut report,
            options.export_md_path.as_deref(),
        );
    } else if let Some(ref custom) = options.export_md_path {
        let md = report::format_neededfix_markdown(&report);
        let _ = std::fs::write(custom, md);
        report.neededfix_path = Some(custom.display().to_string());
    }

    on_progress(ScanProgress {
        phase: ScanPhase::Completed,
        percent: 100,
        detail: format!(
            "Health Score {}/100 ({}) in {}ms",
            report.score.overall, report.score.label, report.duration_ms
        ),
        detected_stacks: profile.stacks,
        files_scanned: files.len(),
        findings_so_far: report.diagnostics.len(),
    });

    Ok(report)
}

/// Ultra-fast (<15ms) single-file in-memory AST micro-guard intended for post-edit hooks
/// (`write_file` / `patch_file`). Evaluates only the modified buffer with zero full-repo walk.
#[must_use]
pub fn check_single_file(workspace: &Path, rel_path: &str, content: &str) -> Vec<Diagnostic> {
    let clean_rel = rel_path.trim_start_matches("./").replace('\\', "/");
    if clean_rel
        .split('/')
        .any(|seg| detector::DEFAULT_SKIP_DIRS.contains(&seg))
    {
        return Vec::new();
    }
    let ext = Path::new(&clean_rel)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if !detector::SOURCE_EXTENSIONS.contains(&ext.as_str()) {
        return Vec::new();
    }

    let cfg = DoctorConfig::load(workspace);
    if !cfg.lint || cfg.is_file_ignored(&clean_rel) {
        return Vec::new();
    }

    let loaded = detector::LoadedFile {
        rel_path: clean_rel.clone(),
        ext,
        content: content.to_string(),
        line_count: content.lines().count(),
        is_test: detector::is_test_file_path(&clean_rel),
    };
    let profile = detector::detect_project_profile(workspace, std::slice::from_ref(&loaded));
    let mut diags = rules::analyze_file(&loaded, &profile.stacks, &cfg, cfg.security);
    diags.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.line.cmp(&b.line))
    });
    diags
}

/// Fast changed-files regression audit intended for `VerificationBarrier` (`power_verify`).
/// Does NOT overwrite `.minicode/minidoctor/` workspace reports so verification stays side-effect free.
#[must_use]
pub fn check_changed_files(workspace: &Path, rel_paths: &[String]) -> Option<DoctorReport> {
    if rel_paths.is_empty() {
        return None;
    }
    let opts = DoctorOptions {
        scope: ScanScope::Files(rel_paths.to_vec()),
        run_dead_code: false,
        run_security: true,
        write_workspace_artifacts: false,
        export_md_path: None,
        threshold_override: None,
        max_files: rel_paths.len().max(1),
    };
    run_doctor(workspace, &opts).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_multi_stack_detection_ast_rules_and_artifacts() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Python / FastAPI file with real anti-patterns
        std::fs::write(
            root.join("pyproject.toml"),
            "[project]\ndependencies = [\"fastapi\", \"sqlalchemy\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::write(
            root.join("app/routes.py"),
            r#"
import time
from fastapi import FastAPI, BackgroundTasks
from sqlalchemy.ext.asyncio import AsyncSession

app = FastAPI()
SECRET_KEY = "super_real_hardcoded_prod_secret_98765"
CACHE_KEY = "user_profile_cache_v2_should_be_ignored"

@app.get("/users")
async def get_users(db: AsyncSession, background_tasks: BackgroundTasks):
    time.sleep(2)
    db.execute("SELECT 1")
    background_tasks.add_task(send_email, db)
    return {"ok": True}
"#,
        )
        .unwrap();

        // React / Next.js file with anti-patterns
        std::fs::write(
            root.join("package.json"),
            r#"{"dependencies": {"react": "^19.0.0", "next": "^15.0.0", "hono": "^4.0.0", "pino": "^9.0.0"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/Panel.tsx"),
            r#"
'use client';
import { useEffect } from 'react';
export function Panel({ items }: { items: string[] }) {
  const secret = process.env.DATABASE_URL;
  useEffect(async () => {
    console.log(secret);
  }, []);
  return <div>{items.map((x, index) => <img key={index} src={x} />)}</div>;
}
"#,
        )
        .unwrap();

        // Rust file with unwrap + blocking in async
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n[dependencies]\ntokio = \"1\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/worker.rs"),
            r#"
async fn run_job() {
    std::thread::sleep(std::time::Duration::from_secs(1));
    let _x = Some(1).unwrap();
}
#[cfg(test)]
mod tests {
    #[test]
    fn ok_in_test() {
        let _ = Some(1).unwrap();
    }
}
"#,
        )
        .unwrap();

        // Flutter/Dart file with undisposed controller + BuildContext after await
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(
            root.join("pubspec.yaml"),
            "name: app\ndependencies:\n  flutter:\n    sdk: flutter\n",
        )
        .unwrap();
        std::fs::write(
            root.join("lib/profile.dart"),
            r#"
class _ProfileState extends State<Profile> {
  final TextEditingController _nameCtrl = TextEditingController();
  Future<void> save() async {
    await api.save();
    Navigator.of(context).pop();
  }
}
"#,
        )
        .unwrap();

        let report = run_doctor(root, &DoctorOptions::default()).unwrap();
        assert!(report.profile.has(DetectedStack::FastApi));
        assert!(report.profile.has(DetectedStack::NextJs));
        assert!(report.profile.has(DetectedStack::Rust));
        assert!(report.profile.has(DetectedStack::Flutter));

        let rules: Vec<&str> = report.diagnostics.iter().map(|d| d.rule.as_str()).collect();
        assert!(rules.contains(&"sec/hardcoded-secret"), "rules: {rules:?}");
        assert!(
            rules.contains(&"fastapi/sync-in-async-route"),
            "rules: {rules:?}"
        );
        assert!(rules.contains(&"fastapi/missing-await"), "rules: {rules:?}");
        assert!(
            rules.contains(&"fastapi/background-task-db-session"),
            "rules: {rules:?}"
        );
        assert!(
            rules.contains(&"next/client-secret-env"),
            "rules: {rules:?}"
        );
        assert!(
            rules.contains(&"react/async-use-effect"),
            "rules: {rules:?}"
        );
        assert!(rules.contains(&"react/index-as-key"), "rules: {rules:?}");
        assert!(
            rules.contains(&"rust/blocking-in-async"),
            "rules: {rules:?}"
        );
        assert!(
            rules.contains(&"rust/unwrap-in-non-test"),
            "rules: {rules:?}"
        );
        assert!(
            rules.contains(&"flutter/dispose-controllers"),
            "rules: {rules:?}"
        );
        assert!(
            rules.contains(&"flutter/no-build-context-async"),
            "rules: {rules:?}"
        );

        // Verify SAFE_KEY_EXCLUSIONS prevented false positive on CACHE_KEY
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("CACHE_KEY")),
            "CACHE_KEY must not be flagged as a secret"
        );
        // Verify #[cfg(test)] unwrap was NOT flagged
        assert_eq!(
            report
                .diagnostics
                .iter()
                .filter(|d| d.rule == "rust/unwrap-in-non-test")
                .count(),
            1
        );

        // Verify .minicode/minidoctor/ artifacts were persisted
        let md_path = root.join(".minicode/minidoctor/NEEDEDFIX.md");
        let json_path = root.join(".minicode/minidoctor/latest.json");
        let hist_path = root.join(".minicode/minidoctor/history.jsonl");
        assert!(md_path.is_file());
        assert!(json_path.is_file());
        assert!(hist_path.is_file());

        let md = std::fs::read_to_string(&md_path).unwrap();
        assert!(md.contains("Priority Fixes for AI Agent"));
        assert!(md.contains("fastapi/sync-in-async-route"));
        assert!(load_latest_report(root).is_some());
    }
}
