use crate::error::{MinicodeError, Result, ToolError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Statistical outcome verdict for evaluated test
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlakinessVerdict {
    /// 100% pass rate across all burn-in runs
    DeterministicPass,
    /// 100% failure rate across all burn-in runs (genuine bug/regression)
    DeterministicFail,
    /// Intermittent pass/fail behavior (flaky test detected)
    FlakyIntermittent,
}

impl FlakinessVerdict {
    #[allow(dead_code)]
    pub fn badge(&self) -> &'static str {
        match self {
            Self::DeterministicPass => "🟢 Stable Pass",
            Self::DeterministicFail => "🔴 Stable Fail",
            Self::FlakyIntermittent => "⚠️ Flaky Intermittent",
        }
    }
}

/// Heuristic category of non-deterministic failure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlakySignature {
    /// Timeouts, race conditions, unsynchronized sleep deadlines
    TimingJitter,
    /// Port collisions, file locks, address already in use
    ResourceContention,
    /// Unordered collection iteration, PRNG entropy, fluctuating assertions
    AssertionVariance,
    /// Unclassified or generic failure
    Unknown,
}

impl FlakySignature {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::TimingJitter => "⏱️ Timing Jitter",
            Self::ResourceContention => "🔒 Resource Contention",
            Self::AssertionVariance => "🔀 Assertion Variance",
            Self::Unknown => "❓ Unknown",
        }
    }

    /// Detects failure signature from captured stdout/stderr
    pub fn from_output(output: &str) -> Self {
        let lower = output.to_lowercase();
        if lower.contains("timed out")
            || lower.contains("timeout")
            || lower.contains("deadline")
            || lower.contains("elapsed")
            || lower.contains("took too long")
        {
            Self::TimingJitter
        } else if lower.contains("address already in use")
            || lower.contains("os error 98")
            || lower.contains("os error 48")
            || lower.contains("port")
            || lower.contains("bind")
            || lower.contains("lock")
            || lower.contains("busy")
            || lower.contains("permission denied")
        {
            Self::ResourceContention
        } else if lower.contains("assertion")
            || lower.contains("left == right")
            || lower.contains("panicked at")
            || lower.contains("expected:")
            || lower.contains("diff")
        {
            Self::AssertionVariance
        } else {
            Self::Unknown
        }
    }
}

/// A single execution run in a burn-in series
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SingleTestRun {
    pub run_index: usize,
    pub passed: bool,
    pub duration_ms: u64,
    pub error_snippet: Option<String>,
}

/// Comprehensive statistical variance analysis report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlakyAnalysisReport {
    pub test_name: String,
    pub total_runs: usize,
    pub passes: usize,
    pub failures: usize,
    pub flakiness_ratio: f32,
    pub verdict: FlakinessVerdict,
    pub signature: FlakySignature,
    pub avg_duration_ms: u64,
    pub runs: Vec<SingleTestRun>,
    pub stabilization_advice: Vec<String>,
}

impl FlakyAnalysisReport {
    #[allow(dead_code)]
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🧪 Statistical Test Flakiness Report: `{}`\n\n",
            self.test_name
        );
        out.push_str(&format!(
            "- **Verdict:** {} (Passes: {}/{}, Failures: {}/{})\n",
            self.verdict.badge(),
            self.passes,
            self.total_runs,
            self.failures,
            self.total_runs
        ));
        out.push_str(&format!(
            "- **Flakiness Ratio:** {:.1}% (Signature: {})\n",
            self.flakiness_ratio * 100.0,
            self.signature.badge()
        ));
        out.push_str(&format!(
            "- **Average Execution Duration:** {}ms\n\n",
            self.avg_duration_ms
        ));

        out.push_str("### 📊 Burn-in Execution History\n");
        for run in &self.runs {
            let status_icon = if run.passed { "✔ Pass" } else { "✗ Fail" };
            out.push_str(&format!(
                "- Run #{}: **{}** ({}ms)",
                run.run_index, status_icon, run.duration_ms
            ));
            if let Some(err) = &run.error_snippet {
                out.push_str(&format!(" ➔ `{}`", err.replace('\n', " ")));
            }
            out.push('\n');
        }
        out.push('\n');

        if !self.stabilization_advice.is_empty() {
            out.push_str("### 💡 Recommended Stabilization Actions\n");
            for (idx, advice) in self.stabilization_advice.iter().enumerate() {
                out.push_str(&format!("{}. {}\n", idx + 1, advice));
            }
            out.push('\n');
        }

        out
    }
}

/// Quarantined test entry persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuarantinedTest {
    pub test_name: String,
    pub quarantined_at: String,
    pub flakiness_ratio: f32,
    pub signature: FlakySignature,
    pub reason: String,
    pub runs_evaluated: usize,
}

/// JSON store representation in `.minicode/quarantine.json`
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QuarantineStore {
    pub tests: Vec<QuarantinedTest>,
}

/// Persistent quarantine manager storing isolated tests in `.minicode/quarantine.json`
pub struct QuarantineManager;

impl QuarantineManager {
    pub fn quarantine_file_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::QUARANTINE_FILE_NAME)
    }

    /// Loads the quarantine store from disk
    pub fn load(workspace_root: &Path) -> QuarantineStore {
        let path = Self::quarantine_file_path(workspace_root);
        if !path.exists() {
            return QuarantineStore::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to read quarantine store file");
                QuarantineStore::default()
            }
        }
    }

    /// Saves the quarantine store to disk
    pub fn save(workspace_root: &Path, store: &QuarantineStore) -> Result<()> {
        let path = Self::quarantine_file_path(workspace_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                MinicodeError::Tool(ToolError::FileOp {
                    path: parent.display().to_string(),
                    source: e,
                })
            })?;
        }

        let content = serde_json::to_string_pretty(store).map_err(|e| {
            MinicodeError::Tool(ToolError::CommandExec(format!(
                "Failed to serialize quarantine store: {}",
                e
            )))
        })?;

        std::fs::write(&path, content).map_err(|e| {
            MinicodeError::Tool(ToolError::FileOp {
                path: path.display().to_string(),
                source: e,
            })
        })?;

        Ok(())
    }

    /// Adds or updates a test in quarantine
    pub fn quarantine(
        workspace_root: &Path,
        test_name: &str,
        flakiness_ratio: f32,
        signature: FlakySignature,
        reason: &str,
        runs_evaluated: usize,
    ) -> Result<QuarantinedTest> {
        let mut store = Self::load(workspace_root);

        let entry = QuarantinedTest {
            test_name: test_name.to_string(),
            quarantined_at: chrono::Utc::now().to_rfc3339(),
            flakiness_ratio,
            signature,
            reason: reason.to_string(),
            runs_evaluated,
        };

        if let Some(pos) = store.tests.iter().position(|t| t.test_name == test_name) {
            store.tests[pos] = entry.clone();
        } else {
            store.tests.push(entry.clone());
        }

        Self::save(workspace_root, &store)?;
        Ok(entry)
    }

    /// Removes a test from quarantine
    pub fn unquarantine(workspace_root: &Path, test_name: &str) -> Result<bool> {
        let mut store = Self::load(workspace_root);
        let len_before = store.tests.len();
        store.tests.retain(|t| t.test_name != test_name);

        if store.tests.len() < len_before {
            Self::save(workspace_root, &store)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clears all quarantined tests from the quarantine store
    pub fn clear(workspace_root: &Path) -> Result<usize> {
        let mut store = Self::load(workspace_root);
        let count = store.tests.len();
        store.tests.clear();
        Self::save(workspace_root, &store)?;
        Ok(count)
    }

    /// Checks if a test is currently quarantined
    #[allow(dead_code)]
    pub fn is_quarantined(workspace_root: &Path, test_name: &str) -> bool {
        let store = Self::load(workspace_root);
        store.tests.iter().any(|t| t.test_name == test_name)
    }

    /// Formats an overview of currently quarantined tests
    pub fn format_report(store: &QuarantineStore) -> String {
        let mut out = String::new();
        out.push_str("# 🛡️ Quarantined Test Registry (`.minicode/quarantine.json`)\n\n");

        if store.tests.is_empty() {
            out.push_str(
                "✅ No tests are currently quarantined. All tests are considered stable.\n",
            );
            return out;
        }

        out.push_str(&format!(
            "Currently isolating **{}** non-deterministic test(s) to prevent false-positive regressions:\n\n",
            store.tests.len()
        ));

        for (idx, t) in store.tests.iter().enumerate() {
            out.push_str(&format!(
                "### {}. `{}` ({})\n",
                idx + 1,
                t.test_name,
                t.signature.badge()
            ));
            out.push_str(&format!(
                "- **Flakiness Ratio:** {:.1}% across {} burn-in runs\n",
                t.flakiness_ratio * 100.0,
                t.runs_evaluated
            ));
            out.push_str(&format!("- **Quarantined At:** {}\n", t.quarantined_at));
            out.push_str(&format!("- **Reason:** {}\n\n", t.reason));
        }

        out
    }
}

/// Flaky test analyzer and burn-in executor
pub struct FlakyTestDetector;

impl FlakyTestDetector {
    /// Computes statistical metrics from collected runs and generates stabilization advice
    pub fn analyze_runs(test_name: &str, runs: Vec<SingleTestRun>) -> FlakyAnalysisReport {
        let total_runs = runs.len();
        if total_runs == 0 {
            return FlakyAnalysisReport {
                test_name: test_name.to_string(),
                total_runs: 0,
                passes: 0,
                failures: 0,
                flakiness_ratio: 0.0,
                verdict: FlakinessVerdict::DeterministicPass,
                signature: FlakySignature::Unknown,
                avg_duration_ms: 0,
                runs: Vec::new(),
                stabilization_advice: Vec::new(),
            };
        }

        let passes = runs.iter().filter(|r| r.passed).count();
        let failures = total_runs - passes;

        let verdict = if failures == 0 {
            FlakinessVerdict::DeterministicPass
        } else if passes == 0 {
            FlakinessVerdict::DeterministicFail
        } else {
            FlakinessVerdict::FlakyIntermittent
        };

        let flakiness_ratio = if total_runs > 0 {
            failures.min(passes) as f32 / total_runs as f32
        } else {
            0.0
        };

        let total_duration_ms: u64 = runs.iter().map(|r| r.duration_ms).sum();
        let avg_duration_ms = total_duration_ms / total_runs as u64;

        let mut timing_count = 0;
        let mut assertion_count = 0;
        let mut resource_count = 0;

        for run in &runs {
            if let Some(ref err) = run.error_snippet {
                let sig = FlakySignature::from_output(err);
                match sig {
                    FlakySignature::TimingJitter => timing_count += 1,
                    FlakySignature::AssertionVariance => assertion_count += 1,
                    FlakySignature::ResourceContention => resource_count += 1,
                    FlakySignature::Unknown => {}
                }
            }
        }

        let signature = if timing_count >= assertion_count
            && timing_count >= resource_count
            && timing_count > 0
        {
            FlakySignature::TimingJitter
        } else if assertion_count >= resource_count && assertion_count > 0 {
            FlakySignature::AssertionVariance
        } else if resource_count > 0 {
            FlakySignature::ResourceContention
        } else {
            FlakySignature::Unknown
        };

        let mut stabilization_advice = Vec::new();
        match signature {
            FlakySignature::TimingJitter => {
                stabilization_advice.push(
                    "Replace arbitrary `tokio::time::sleep` calls with `tokio::sync::Notify` or condition variables."
                        .to_string(),
                );
                stabilization_advice.push(
                    "Wrap asynchronous operations in `tokio::time::timeout` with exponential backoff polling."
                        .to_string(),
                );
            }
            FlakySignature::ResourceContention => {
                stabilization_advice.push(
                    "Bind dynamic ephemeral socket ports (`std::net::TcpListener::bind(\"127.0.0.1:0\")`) instead of static port constants."
                        .to_string(),
                );
                stabilization_advice.push(
                    "Use isolated per-test temporary directories via `tempfile::tempdir()` instead of fixed workspace paths."
                        .to_string(),
                );
            }
            FlakySignature::AssertionVariance => {
                stabilization_advice.push(
                    "Sort unordered collections (HashMap / HashSet keys) before asserting equality."
                        .to_string(),
                );
                stabilization_advice.push(
                    "Seed pseudo-random number generators with a deterministic constant in test fixtures."
                        .to_string(),
                );
            }
            FlakySignature::Unknown => {
                if verdict == FlakinessVerdict::FlakyIntermittent {
                    stabilization_advice.push(
                        "Inspect test for hidden global static variables or shared state between tests."
                            .to_string(),
                    );
                    stabilization_advice.push(
                        "Run test with `-- --test-threads=1` to isolate concurrency issues."
                            .to_string(),
                    );
                }
            }
        }

        FlakyAnalysisReport {
            test_name: test_name.to_string(),
            total_runs,
            passes,
            failures,
            flakiness_ratio,
            verdict,
            signature,
            avg_duration_ms,
            runs,
            stabilization_advice,
        }
    }

    /// Executes $N$ burn-in runs of a target test using cargo test runner
    #[allow(dead_code)]
    pub async fn execute_burn_in(
        workspace_root: &Path,
        test_target: &str,
        runs: usize,
        per_run_timeout_secs: u64,
    ) -> Result<FlakyAnalysisReport> {
        let runs_clamped = runs.clamp(
            crate::constants::MIN_FLAKY_RUNS,
            crate::constants::MAX_FLAKY_RUNS,
        );
        let mut collected_runs = Vec::new();

        for i in 1..=runs_clamped {
            let start = Instant::now();

            let mut cmd = tokio::process::Command::new("cargo");
            cmd.arg("test")
                .arg("-j")
                .arg("3")
                .arg("--")
                .arg(test_target)
                .current_dir(workspace_root);

            let timeout_duration = std::time::Duration::from_secs(per_run_timeout_secs);
            let child = cmd.output();

            match tokio::time::timeout(timeout_duration, child).await {
                Ok(Ok(output)) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    let passed = output.status.success();
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    let error_snippet = if !passed {
                        let combined = format!("{}\n{}", stdout, stderr);
                        let snippet = combined
                            .lines()
                            .filter(|l| {
                                l.contains("FAILED")
                                    || l.contains("panicked at")
                                    || l.contains("timeout")
                                    || l.contains("error:")
                            })
                            .take(3)
                            .collect::<Vec<_>>()
                            .join(" | ");

                        if snippet.is_empty() {
                            Some("Non-zero test exit code".to_string())
                        } else {
                            Some(snippet)
                        }
                    } else {
                        None
                    };

                    collected_runs.push(SingleTestRun {
                        run_index: i,
                        passed,
                        duration_ms,
                        error_snippet,
                    });
                }
                Ok(Err(e)) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    collected_runs.push(SingleTestRun {
                        run_index: i,
                        passed: false,
                        duration_ms,
                        error_snippet: Some(format!("Execution failed: {}", e)),
                    });
                }
                Err(_) => {
                    let duration_ms = per_run_timeout_secs * 1000;
                    collected_runs.push(SingleTestRun {
                        run_index: i,
                        passed: false,
                        duration_ms,
                        error_snippet: Some(format!(
                            "Execution timed out after {}s",
                            per_run_timeout_secs
                        )),
                    });
                }
            }
        }

        Ok(Self::analyze_runs(test_target, collected_runs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_signature_detection() {
        assert_eq!(
            FlakySignature::from_output("thread panicked at 'timed out waiting for lock'"),
            FlakySignature::TimingJitter
        );
        assert_eq!(
            FlakySignature::from_output(
                "failed to bind socket: Address already in use (os error 98)"
            ),
            FlakySignature::ResourceContention
        );
        assert_eq!(
            FlakySignature::from_output("assertion `left == right` failed"),
            FlakySignature::AssertionVariance
        );
        assert_eq!(
            FlakySignature::from_output("some other message"),
            FlakySignature::Unknown
        );
    }

    #[test]
    fn test_statistical_variance_analysis() {
        let runs = vec![
            SingleTestRun {
                run_index: 1,
                passed: true,
                duration_ms: 50,
                error_snippet: None,
            },
            SingleTestRun {
                run_index: 2,
                passed: false,
                duration_ms: 120,
                error_snippet: Some("operation timed out".to_string()),
            },
            SingleTestRun {
                run_index: 3,
                passed: true,
                duration_ms: 55,
                error_snippet: None,
            },
            SingleTestRun {
                run_index: 4,
                passed: true,
                duration_ms: 45,
                error_snippet: None,
            },
        ];

        let report = FlakyTestDetector::analyze_runs("test_async_fetch", runs);
        assert_eq!(report.verdict, FlakinessVerdict::FlakyIntermittent);
        assert_eq!(report.passes, 3);
        assert_eq!(report.failures, 1);
        assert_eq!(report.flakiness_ratio, 0.25);
        assert_eq!(report.signature, FlakySignature::TimingJitter);
        assert!(!report.stabilization_advice.is_empty());
    }

    #[test]
    fn test_quarantine_store_crud() {
        let dir = tempdir().expect("tempdir");

        assert!(!QuarantineManager::is_quarantined(
            dir.path(),
            "test_socket"
        ));

        let quarantined = QuarantineManager::quarantine(
            dir.path(),
            "test_socket",
            0.40,
            FlakySignature::ResourceContention,
            "Intermittent port binding failure",
            5,
        )
        .expect("quarantine success");

        assert_eq!(quarantined.test_name, "test_socket");
        assert!(QuarantineManager::is_quarantined(dir.path(), "test_socket"));

        let store = QuarantineManager::load(dir.path());
        assert_eq!(store.tests.len(), 1);

        let report = QuarantineManager::format_report(&store);
        assert!(report.contains("test_socket"));
        assert!(report.contains("Resource Contention"));

        let removed =
            QuarantineManager::unquarantine(dir.path(), "test_socket").expect("unquarantine");
        assert!(removed);
        assert!(!QuarantineManager::is_quarantined(
            dir.path(),
            "test_socket"
        ));
    }
}
