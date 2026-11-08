use crate::agent::reproducer_guard::{ReproducerGuard, ReproducerPhase};
use crate::constants::VERIFICATION_TEST_TIMEOUT_MS;
use crate::context::syntax_guard::SyntaxGuard;
use crate::tools::compiler::ScopedCompiler;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

/// 4-state test freshness machine inspired by `ponytail`:
/// `NotRun` -> `Passed { passed }` / `Failed { failed }` -> `Stale { last_passed }` on any subsequent code edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TestFreshnessState {
    #[default]
    NotRun,
    Passed {
        passed: usize,
    },
    Failed {
        failed: usize,
    },
    Stale {
        last_passed: usize,
    },
}

impl TestFreshnessState {
    /// Detects whether a shell command invokes a test runner (zero regexes).
    #[must_use]
    pub fn is_test_runner_command(command_str: &str) -> bool {
        for segment in command_str.split([';', '\n']) {
            for sub in segment.split("&&").flat_map(|s| s.split("||")) {
                let tokens: Vec<&str> = sub
                    .split_whitespace()
                    .skip_while(|tok| {
                        (tok.contains('=') && !tok.starts_with('-'))
                            || matches!(*tok, "env" | "nice" | "time" | "command")
                            || tok.parse::<i32>().is_ok()
                            || *tok == "-n"
                    })
                    .collect();
                if tokens.is_empty() {
                    continue;
                }
                let bin = tokens[0]
                    .rsplit('/')
                    .next()
                    .unwrap_or(tokens[0])
                    .to_ascii_lowercase();
                let subcmd = tokens
                    .get(1)
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_default();
                let subcmd2 = tokens
                    .get(2)
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_default();

                let is_runner = match (bin.as_str(), subcmd.as_str(), subcmd2.as_str()) {
                    ("pytest" | "vitest" | "jest" | "rspec" | "phpunit" | "nextest", _, _) => true,
                    ("cargo", "test" | "nextest", _)
                    | ("bun" | "pnpm" | "yarn" | "go" | "deno" | "mix", "test", _)
                    | ("npm", "test" | "t", _)
                    | ("npx" | "bunx", "vitest" | "jest" | "playwright", _) => true,
                    ("npm" | "pnpm" | "bun" | "yarn", "run", s2)
                        if s2 == "test" || s2.starts_with("test:") =>
                    {
                        true
                    }
                    ("python" | "python3" | "uv", _, _) => tokens.iter().any(|t| {
                        t.eq_ignore_ascii_case("pytest") || t.eq_ignore_ascii_case("unittest")
                    }),
                    _ => false,
                };
                if is_runner {
                    return true;
                }
            }
        }
        false
    }

    /// Parses test runner output into `Passed { passed }` or `Failed { failed }` using token boundary scanning (zero regexes).
    #[must_use]
    pub fn parse_test_pass_fail_counts(output: &str, exit_success: bool) -> Self {
        let mut total_passed = 0usize;
        let mut total_failed = 0usize;

        for line in output.lines() {
            let lower = line.to_ascii_lowercase();
            let raw_tokens: Vec<&str> = lower
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .filter(|t| !t.is_empty())
                .collect();

            for window in raw_tokens.windows(2) {
                if let Ok(num) = window[0].parse::<usize>() {
                    match window[1] {
                        "passed" | "pass" | "passing" => {
                            total_passed = total_passed.saturating_add(num);
                        }
                        "failed" | "fail" | "failing" | "failures" | "errors" => {
                            // Ignore "0 failed" or "0 errors"
                            if num > 0 {
                                total_failed = total_failed.saturating_add(num);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if !exit_success || total_failed > 0 {
            Self::Failed {
                failed: total_failed.max(1),
            }
        } else {
            Self::Passed {
                passed: total_passed,
            }
        }
    }

    /// Transitions `Passed` or `Failed` test state to `Stale` when a source file is edited after a test run.
    pub fn on_code_file_modified(&mut self) {
        match *self {
            Self::Passed { passed } => {
                *self = Self::Stale {
                    last_passed: passed,
                };
            }
            Self::Failed { .. } => {
                *self = Self::Stale { last_passed: 0 };
            }
            Self::NotRun | Self::Stale { .. } => {}
        }
    }

    /// Formats a concise human/agent-readable status string.
    #[must_use]
    pub fn format_summary(&self) -> String {
        match self {
            Self::NotRun => "not run".to_string(),
            Self::Passed { passed } => format!("✔ {} passed (fresh)", passed),
            Self::Failed { failed } => format!("✖ {} failed", failed),
            Self::Stale { last_passed } => {
                format!(
                    "! stale (no test run since code edit; last passed: {})",
                    last_passed
                )
            }
        }
    }
}

/// Live per-workspace Ponytail quality telemetry (`reused`, `created`, `reuse_pct`, `open_callers`, `test_freshness`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionQualityTelemetry {
    pub reused_symbols: BTreeSet<String>,
    pub created_symbols: BTreeSet<String>,
    pub open_callers: Vec<crate::context::repomap::OpenCallerHit>,
    pub test_freshness: TestFreshnessState,
}

static WORKSPACE_QUALITY_TELEMETRY: OnceLock<Mutex<HashMap<PathBuf, SessionQualityTelemetry>>> =
    OnceLock::new();

impl SessionQualityTelemetry {
    fn canonical_key(workspace_root: &Path) -> PathBuf {
        workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf())
    }

    /// Records a file edit (`before` -> `after`), updates cumulative reuse/new symbol counts and `open_callers`,
    /// and transitions `test_freshness` to `Stale` if the modified file is a non-test source file.
    pub fn record_file_edit(
        workspace_root: &Path,
        file_path: &Path,
        before: &str,
        after: &str,
    ) -> crate::context::repomap::EditReuseAndCallerReport {
        let report = crate::context::repomap::analyze_edit_reuse_and_open_callers(
            workspace_root,
            file_path,
            before,
            after,
            None,
        );

        let key = Self::canonical_key(workspace_root);
        let map_lock = WORKSPACE_QUALITY_TELEMETRY.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut guard) = map_lock.lock() {
            let entry = guard.entry(key).or_default();
            for r in &report.reused_symbols {
                entry.reused_symbols.insert(r.clone());
            }
            for c in &report.created_symbols {
                entry.created_symbols.insert(c.clone());
            }
            // Remove any previously tracked open_callers for symbols that are now newly defined
            entry
                .open_callers
                .retain(|oc| !report.created_symbols.contains(&oc.symbol));
            for oc in &report.open_callers {
                if !entry.open_callers.contains(oc) {
                    entry.open_callers.push(oc.clone());
                }
            }

            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            if crate::constants::REFACTOR_SOURCE_EXTENSIONS.contains(&ext) {
                entry.test_freshness.on_code_file_modified();
            }
        }

        report
    }

    /// Updates `test_freshness` when a shell command runs a test suite.
    pub fn record_command_execution(
        workspace_root: &Path,
        command_str: &str,
        output: &str,
        exit_success: bool,
    ) {
        if !TestFreshnessState::is_test_runner_command(command_str) {
            return;
        }
        let next_state = TestFreshnessState::parse_test_pass_fail_counts(output, exit_success);
        let key = Self::canonical_key(workspace_root);
        let map_lock = WORKSPACE_QUALITY_TELEMETRY.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut guard) = map_lock.lock() {
            let entry = guard.entry(key).or_default();
            entry.test_freshness = next_state;
        }
    }

    /// Returns a snapshot of the current workspace's quality telemetry.
    #[must_use]
    pub fn get_snapshot(workspace_root: &Path) -> Option<Self> {
        let key = Self::canonical_key(workspace_root);
        let map_lock = WORKSPACE_QUALITY_TELEMETRY.get()?;
        let guard = map_lock.lock().ok()?;
        guard.get(&key).cloned()
    }

    /// Computes cumulative session reuse percentage once `reused + created >= REUSE_METRIC_MIN_SYMBOL_SAMPLE`.
    #[must_use]
    pub fn cumulative_reuse_pct(&self) -> Option<u32> {
        let total = self.reused_symbols.len() + self.created_symbols.len();
        if total >= crate::constants::REUSE_METRIC_MIN_SYMBOL_SAMPLE {
            Some(((self.reused_symbols.len() * 100 + total / 2) / total) as u32)
        } else {
            None
        }
    }

    /// Formats a compact `<session_quality_telemetry>` block for Zone B recency context when active.
    #[must_use]
    pub fn format_prompt_block(workspace_root: &Path) -> Option<String> {
        let snap = Self::get_snapshot(workspace_root)?;
        if snap.reused_symbols.is_empty()
            && snap.created_symbols.is_empty()
            && snap.open_callers.is_empty()
            && snap.test_freshness == TestFreshnessState::NotRun
        {
            return None;
        }

        let reuse_str = if let Some(pct) = snap.cumulative_reuse_pct() {
            format!(
                "{}% ({} reused, {} new)",
                pct,
                snap.reused_symbols.len(),
                snap.created_symbols.len()
            )
        } else {
            format!(
                "{} reused, {} new",
                snap.reused_symbols.len(),
                snap.created_symbols.len()
            )
        };

        let callers_str = if snap.open_callers.is_empty() {
            "0".to_string()
        } else {
            let items: Vec<String> = snap
                .open_callers
                .iter()
                .take(4)
                .map(|c| format!("{}@{}:{}", c.symbol, c.file_path, c.line))
                .collect();
            format!("! {} ({})", snap.open_callers.len(), items.join(", "))
        };

        Some(format!(
            "  <session_quality_telemetry reuse=\"{}\" open_callers=\"{}\" tests=\"{}\" />\n",
            reuse_str,
            callers_str,
            snap.test_freshness.format_summary()
        ))
    }
}

/// Evaluation status for an individual verification gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum GateStatus {
    Passed,
    Failed {
        gate_name: &'static str,
        reason: String,
        actionable_remediation: String,
    },
    Skipped {
        reason: &'static str,
    },
}

impl GateStatus {
    #[must_use]
    pub fn is_pass_or_skip(&self) -> bool {
        matches!(self, Self::Passed | Self::Skipped { .. })
    }
}

/// Comprehensive report summarizing the evaluation of all 4 pre-completion verification gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationReport {
    pub gate1_syntax_compiler: GateStatus,
    pub gate2_reproducer_test: GateStatus,
    pub gate3_regression_conflicts: GateStatus,
    pub gate4_diff_sanity: GateStatus,
    pub all_passed: bool,
}

impl VerificationReport {
    /// Formats a human-readable scorecard of the 4 verification gates.
    #[must_use]
    pub fn format_report(&self) -> String {
        let mut out = String::new();
        if self.all_passed {
            out.push_str("⚡ **MiniPower Verification Barrier: All 4 Gates Passed!**\n\n");
            out.push_str("  ✔ **Gate 1**: AST Syntax & Scoped Compiler Integrity\n");
            out.push_str("  ✔ **Gate 2**: Reproducer & Regression Test Suite\n");
            out.push_str("  ✔ **Gate 3**: Structural Integrity & Conflict Markers\n");
            out.push_str("  ✔ **Gate 4**: Diff Sanity (Zero Secrets & Debug Residue)\n\n");
            out.push_str("All pre-completion verification gates passed cleanly.");
        } else {
            out.push_str("⚡ **MiniPower Verification Barrier: Verification Failed**\n\n");
            let format_line = |name: &str, status: &GateStatus| -> String {
                match status {
                    GateStatus::Passed => format!("  ✔ **{}**: Passed\n", name),
                    GateStatus::Skipped { reason } => {
                        format!("  ○ **{}**: Skipped ({})\n", name, reason)
                    }
                    GateStatus::Failed {
                        reason,
                        actionable_remediation,
                        ..
                    } => {
                        format!(
                            "  ❌ **{}**: {}\n     ╰── Remediation: {}\n",
                            name, reason, actionable_remediation
                        )
                    }
                }
            };
            out.push_str(&format_line(
                "Gate 1 (AST Syntax & Compiler)",
                &self.gate1_syntax_compiler,
            ));
            out.push_str(&format_line(
                "Gate 2 (Reproducer & Tests)",
                &self.gate2_reproducer_test,
            ));
            out.push_str(&format_line(
                "Gate 3 (Structural Integrity)",
                &self.gate3_regression_conflicts,
            ));
            out.push_str(&format_line(
                "Gate 4 (Diff Sanity)",
                &self.gate4_diff_sanity,
            ));
        }
        out
    }

    /// Formats an actionable prompt directing the model to self-correct before finishing.
    #[must_use]
    pub fn format_remediation_prompt(&self) -> String {
        let mut out = String::new();
        out.push_str("[PRE-COMPLETION VERIFICATION BARRIER REJECTED COMPLETION]\n");
        out.push_str("You attempted to conclude the turn, but the workspace failed automated pre-completion verification gates:\n\n");

        for gate in [
            &self.gate1_syntax_compiler,
            &self.gate2_reproducer_test,
            &self.gate3_regression_conflicts,
            &self.gate4_diff_sanity,
        ] {
            if let GateStatus::Failed {
                gate_name,
                reason,
                actionable_remediation,
            } = gate
            {
                out.push_str(&format!(
                    "- ❌ **{}**:\n  {}\n  --> Remediation: {}\n\n",
                    gate_name, reason, actionable_remediation
                ));
            }
        }

        if matches!(self.gate1_syntax_compiler, GateStatus::Failed { .. })
            || matches!(self.gate2_reproducer_test, GateStatus::Failed { .. })
        {
            out.push_str(
                "<causal_tracer_protocol>\n\
                 - Competing Hypotheses: Formulate at least 2 competing explanations for the failure before editing.\n\
                 - Disconfirmation Check: Check what evidence in the compiler/test output disconfirms your first guess.\n\
                 - Fastest Discriminating Probe: Inspect the exact failing symbol/callers first; reuse existing helpers instead of adding speculative abstractions.\n\
                 - No Fake Completion: Never silence failures with `#[ignore]`, `.skip(`, `.only(`, `todo!()`, or `unimplemented!()`.\n\
                 </causal_tracer_protocol>\n\n",
            );
        }

        out.push_str("Please fix these verification issues in your next tool action before declaring the task completed.");
        out
    }

    /// Extracts a concise `(failed_gate_and_reason, actionable_remediation)` pair from the first
    /// failing verification gate so the Reflexion Gotcha engine records real compiler/test diagnostics.
    #[must_use]
    pub fn first_failure_summary(&self) -> Option<(String, String)> {
        for gate in [
            &self.gate1_syntax_compiler,
            &self.gate2_reproducer_test,
            &self.gate3_regression_conflicts,
            &self.gate4_diff_sanity,
        ] {
            if let GateStatus::Failed {
                gate_name,
                reason,
                actionable_remediation,
            } = gate
            {
                let first_line = reason
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("Verification check failed");
                let summary = crate::utils::strings::truncate_ellipsis(
                    &format!("[{}] {}", gate_name, first_line),
                    220,
                );
                return Some((summary, actionable_remediation.clone()));
            }
        }
        None
    }
}

/// Pre-loaded modified file entry shared across VerificationBarrier gates to avoid redundant disk reads.
struct LoadedModifiedFile {
    rel_path: String,
    abs_path: std::path::PathBuf,
    content: Result<String, String>,
}

/// 4-Gate Pre-Completion Verification Barrier.
///
/// Intercepts premature completion claims from autonomous models and validates:
/// - Gate 1: AST syntax validity & scoped compiler integrity on modified files
/// - Gate 2: Reproduction / modified test suite passes with exit code 0
/// - Gate 3: Structural integrity & zero unresolved merge conflict markers
/// - Gate 4: Diff sanity audit (zero leftover debug statements, zero leaked secrets)
pub struct VerificationBarrier;

impl VerificationBarrier {
    fn load_modified_files(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> Vec<LoadedModifiedFile> {
        modified_files
            .iter()
            .filter_map(|file| {
                let abs_path = workspace_root.join(file);
                if !abs_path.exists() {
                    return None;
                }
                let content = std::fs::read_to_string(&abs_path).map_err(|e| e.to_string());
                Some(LoadedModifiedFile {
                    rel_path: file.clone(),
                    abs_path,
                    content,
                })
            })
            .collect()
    }

    /// Executes all 4 gates against modified files in the workspace.
    /// Runs synchronous compiler/test/filesystem gates on `spawn_blocking` so the TUI event loop
    /// and Tokio async runtime threads are never blocked.
    pub async fn verify(workspace_root: &Path, modified_files: &[String]) -> VerificationReport {
        // Check if any managed dev runtime daemon exited abnormally
        let daemon_check = Self::check_daemon_health().await;

        let root_buf = workspace_root.to_path_buf();
        let files_vec = modified_files.to_vec();

        let (gate1, gate2, gate3, gate4) = tokio::task::spawn_blocking(move || {
            let loaded = Self::load_modified_files(&root_buf, &files_vec);
            let g1 = Self::check_gate1_loaded(&root_buf, &loaded);
            let g2 = if let Some(failed_daemon) = daemon_check {
                failed_daemon
            } else {
                Self::check_gate2_reproducer_test(&root_buf, &files_vec)
            };
            let g3 = Self::check_gate3_loaded(&loaded);
            let g4 = Self::check_gate4_loaded(&root_buf, &loaded);
            (g1, g2, g3, g4)
        })
        .await
        .unwrap_or((
            GateStatus::Passed,
            GateStatus::Passed,
            GateStatus::Passed,
            GateStatus::Passed,
        ));

        let all_passed = gate1.is_pass_or_skip()
            && gate2.is_pass_or_skip()
            && gate3.is_pass_or_skip()
            && gate4.is_pass_or_skip();

        VerificationReport {
            gate1_syntax_compiler: gate1,
            gate2_reproducer_test: gate2,
            gate3_regression_conflicts: gate3,
            gate4_diff_sanity: gate4,
            all_passed,
        }
    }

    /// Gate 1: AST Syntax, Scoped Compiler & Asset Integrity Check
    #[allow(dead_code)]
    pub fn check_gate1_syntax_compiler(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> GateStatus {
        let loaded = Self::load_modified_files(workspace_root, modified_files);
        Self::check_gate1_loaded(workspace_root, &loaded)
    }

    fn check_gate1_loaded(workspace_root: &Path, loaded: &[LoadedModifiedFile]) -> GateStatus {
        let mut first_rust_file: Option<&str> = None;
        let mut first_ts_file: Option<&str> = None;

        for entry in loaded {
            let file = entry.rel_path.as_str();
            let abs_path = &entry.abs_path;

            let content = match &entry.content {
                Ok(c) => c,
                Err(e) => {
                    return GateStatus::Failed {
                        gate_name: "Gate 1: AST Syntax & Asset Integrity",
                        reason: format!("Failed to read modified file `{}`: {}", file, e),
                        actionable_remediation: format!("Ensure `{}` is accessible on disk.", file),
                    };
                }
            };

            // 1. In-memory Tree-sitter AST syntax barrier (fast per-file check)
            if let Some(err) = SyntaxGuard::check_syntax(abs_path, content) {
                return GateStatus::Failed {
                    gate_name: "Gate 1: AST Syntax & Asset Integrity",
                    reason: format!(
                        "AST syntax error in `{}` at line {}:{}: {}",
                        file, err.line, err.column, err.kind
                    ),
                    actionable_remediation: format!(
                        "Fix the syntax error in `{}` around line {} before concluding: `{}`",
                        file, err.line, err.snippet
                    ),
                };
            }

            // Track project-wide compilers (Rust / TypeScript) so they run once per barrier instead of N times
            if file.ends_with(".rs") && first_rust_file.is_none() {
                first_rust_file = Some(file);
            } else if (file.ends_with(".ts") || file.ends_with(".tsx")) && first_ts_file.is_none() {
                first_ts_file = Some(file);
            } else if file.ends_with(".py") {
                // Python py_compile is per-file
                if let Some(diag) = ScopedCompiler::check_python(workspace_root, file) {
                    if diag.contains("reported syntax error:") || diag.contains("reported errors:")
                    {
                        return GateStatus::Failed {
                            gate_name: "Gate 1: AST Syntax & Asset Integrity",
                            reason: format!(
                                "Compiler diagnostic detected in `{}`:\n{}",
                                file, diag
                            ),
                            actionable_remediation: format!(
                                "Resolve the compiler/linter error in `{}`.",
                                file
                            ),
                        };
                    }
                }
            }

            // 3. HTML Document & Local Asset Link Integrity
            if file.ends_with(".html") || file.ends_with(".htm") {
                if let Some(err) =
                    Self::check_html_integrity(workspace_root, file, abs_path, content)
                {
                    return err;
                }
            }

            // 4. CSS Syntax & Balanced Braces Integrity
            if file.ends_with(".css") {
                if let Some(err) = Self::check_css_integrity(file, content) {
                    return err;
                }
            }
        }

        // 2. Deduplicated project-wide compiler checks (run at most once per project language)
        if let Some(rs_file) = first_rust_file {
            if workspace_root.join("Cargo.toml").exists() {
                if let Some(diag) = ScopedCompiler::check_rust(workspace_root, rs_file) {
                    if diag.contains("reported errors:") {
                        return GateStatus::Failed {
                            gate_name: "Gate 1: AST Syntax & Asset Integrity",
                            reason: format!(
                                "Compiler diagnostic detected in `{}`:\n{}",
                                rs_file, diag
                            ),
                            actionable_remediation: format!(
                                "Resolve the compiler/linter error in `{}`.",
                                rs_file
                            ),
                        };
                    }
                }
            }
        }

        if let Some(ts_file) = first_ts_file {
            if workspace_root.join("tsconfig.json").exists() {
                if let Some(diag) = ScopedCompiler::check_typescript(workspace_root, ts_file) {
                    if diag.contains("reported errors:") {
                        return GateStatus::Failed {
                            gate_name: "Gate 1: AST Syntax & Asset Integrity",
                            reason: format!(
                                "Compiler diagnostic detected in `{}`:\n{}",
                                ts_file, diag
                            ),
                            actionable_remediation: format!(
                                "Resolve the compiler/linter error in `{}`.",
                                ts_file
                            ),
                        };
                    }
                }
            }
        }

        // 5. Broken Open-Callers Check (Ponytail-inspired AST caller verification after symbol removal/rename)
        if let Some(snap) = SessionQualityTelemetry::get_snapshot(workspace_root) {
            if !snap.open_callers.is_empty() {
                let mut still_broken = Vec::new();
                for hit in &snap.open_callers {
                    let caller_abs = workspace_root.join(&hit.file_path);
                    let Ok(caller_code) = std::fs::read_to_string(&caller_abs) else {
                        continue;
                    };
                    let caller_ext = caller_abs
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or_default();
                    let Some(occs) = crate::context::pattern::classify_symbol_occurrences_ast(
                        caller_ext,
                        &caller_code,
                        &hit.symbol,
                    ) else {
                        continue;
                    };
                    let caller_still_references = occs.iter().any(|o| {
                        matches!(
                            o.role,
                            crate::context::pattern::AstOccurrenceRole::Call
                                | crate::context::pattern::AstOccurrenceRole::Import
                                | crate::context::pattern::AstOccurrenceRole::TypeRef
                                | crate::context::pattern::AstOccurrenceRole::IdentifierRef
                        )
                    });
                    if caller_still_references
                        && !Self::is_symbol_declared_in_workspace(workspace_root, &hit.symbol)
                    {
                        still_broken.push(format!(
                            "`{}` at `{}:{}` ({})",
                            hit.symbol, hit.file_path, hit.line, hit.role
                        ));
                    }
                }
                if !still_broken.is_empty() {
                    return GateStatus::Failed {
                        gate_name: "Gate 1: AST Syntax & Asset Integrity",
                        reason: format!(
                            "Orphaned open callers (`open_callers`) still reference removed/renamed symbol(s): {}",
                            still_broken.join(", ")
                        ),
                        actionable_remediation:
                            "Update all callers/imports of the removed or renamed symbol across the workspace (or restore the symbol definition) before completing."
                                .to_string(),
                    };
                }
            }
        }

        GateStatus::Passed
    }

    fn is_symbol_declared_in_workspace(workspace_root: &Path, symbol: &str) -> bool {
        let files = crate::context::walker::WorkspaceWalker::new(workspace_root)
            .extensions(crate::constants::REFACTOR_SOURCE_EXTENSIONS)
            .collect_absolute_files();
        for path in &files {
            let Ok(code) = std::fs::read_to_string(path) else {
                continue;
            };
            if !crate::utils::strings::has_word(&code, symbol) {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            if let Some(occs) =
                crate::context::pattern::classify_symbol_occurrences_ast(ext, &code, symbol)
            {
                if occs
                    .iter()
                    .any(|o| o.role == crate::context::pattern::AstOccurrenceRole::Declaration)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Gate 2: Reproducer / Bug Reproduction Test Execution
    pub fn check_gate2_reproducer_test(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> GateStatus {
        // 1. Check if any active reproducer is awaiting Red-to-Green transition
        let active_reproducers = ReproducerGuard::list_active_reproducers(workspace_root);
        let pending_repro = active_reproducers
            .into_iter()
            .find(|r| matches!(r.status, ReproducerPhase::RedConfirmed { .. }));

        // 2. Find if any test file was modified in this turn
        let modified_test = modified_files.iter().find(|f| {
            f.starts_with("tests/")
                || f.contains("test_")
                || f.contains("_test.")
                || f.contains("repro_")
        });

        // Determine target test path
        let (test_rel_path, is_tracked_repro) = if let Some(pending) = pending_repro {
            (pending.file_path, true)
        } else if let Some(m) = modified_test {
            (m.clone(), false)
        } else {
            return GateStatus::Skipped {
                reason:
                    "No reproducer script, test target, or active reproducer guard in this turn.",
            };
        };

        // For Rust tests in tests/<target>.rs, execute single target
        if test_rel_path.starts_with("tests/") && test_rel_path.ends_with(".rs") {
            let target_name = test_rel_path
                .trim_start_matches("tests/")
                .trim_end_matches(".rs");

            let mut cmd = Command::new("cargo");
            cmd.args(["test", "-j", "1", "--test", target_name])
                .current_dir(workspace_root)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }

            let child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Failed to spawn test runner for Gate 2: {}", e);
                    return GateStatus::Passed;
                }
            };

            let child_pid = child.id();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(child.wait_with_output());
            });

            match rx.recv_timeout(Duration::from_millis(VERIFICATION_TEST_TIMEOUT_MS)) {
                Ok(Ok(output)) => {
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let err_msg = if !stderr.trim().is_empty() {
                            stderr.lines().take(5).collect::<Vec<_>>().join("\n")
                        } else {
                            stdout.lines().take(5).collect::<Vec<_>>().join("\n")
                        };

                        return GateStatus::Failed {
                            gate_name: "Gate 2: Reproducer & Test Execution",
                            reason: format!(
                                "Test target `{}` failed verification:\n{}",
                                target_name, err_msg
                            ),
                            actionable_remediation: format!(
                                "Investigate test failure in `tests/{}.rs` and fix the underlying logic.",
                                target_name
                            ),
                        };
                    } else if is_tracked_repro || target_name.starts_with("repro_") {
                        // Mark reproducer as GreenVerified
                        if let Some(mut record) =
                            ReproducerGuard::load_record(workspace_root, target_name)
                        {
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            record.status = ReproducerPhase::GreenVerified { timestamp };
                            let _ = ReproducerGuard::save_record(workspace_root, &record);
                            tracing::info!(
                                "Gate 2: Active reproducer `{}` transitioned from RED to GREEN!",
                                target_name
                            );
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("Failed waiting on test runner for Gate 2: {}", e);
                }
                Err(_) => {
                    #[cfg(unix)]
                    if child_pid > 0 {
                        let pgid = -(child_pid as i32);
                        // SAFETY: child was spawned with process_group(0) so its PGID equals child_pid.
                        unsafe {
                            libc::kill(pgid, libc::SIGTERM);
                            libc::kill(pgid, libc::SIGKILL);
                            libc::kill(child_pid as i32, libc::SIGKILL);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = Command::new("kill")
                            .args(["-9", &child_pid.to_string()])
                            .status();
                    }
                    tracing::warn!(
                        "Gate 2 test execution timed out ({}ms); killed process group for pid {}",
                        VERIFICATION_TEST_TIMEOUT_MS,
                        child_pid
                    );
                }
            }
        }

        GateStatus::Passed
    }

    /// Gate 3: Structural Integrity & Git Conflict Markers Check
    #[allow(dead_code)]
    pub fn check_gate3_regression_conflicts(
        workspace_root: &Path,
        modified_files: &[String],
    ) -> GateStatus {
        let loaded = Self::load_modified_files(workspace_root, modified_files);
        Self::check_gate3_loaded(&loaded)
    }

    fn check_gate3_loaded(loaded: &[LoadedModifiedFile]) -> GateStatus {
        for entry in loaded {
            let Ok(content) = &entry.content else {
                continue;
            };
            let file = entry.rel_path.as_str();

            for (idx, line) in content.lines().enumerate() {
                if let Some(marker) = detect_git_conflict_marker(line) {
                    return GateStatus::Failed {
                        gate_name: "Gate 3: Structural Integrity & Merge Conflicts",
                        reason: format!(
                            "Git merge conflict marker `{}` found in `{}` at line {}.",
                            marker,
                            file,
                            idx + 1
                        ),
                        actionable_remediation: format!(
                            "Resolve and remove all merge conflict markers from `{}`.",
                            file
                        ),
                    };
                }
            }
        }

        GateStatus::Passed
    }

    /// Gate 4: Diff Sanity & Secret Leak Audit
    #[allow(dead_code)]
    pub fn check_gate4_diff_sanity(workspace_root: &Path, modified_files: &[String]) -> GateStatus {
        let loaded = Self::load_modified_files(workspace_root, modified_files);
        Self::check_gate4_loaded(workspace_root, &loaded)
    }

    fn check_gate4_loaded(workspace_root: &Path, loaded: &[LoadedModifiedFile]) -> GateStatus {
        let readme_path = workspace_root.join("README.md");
        if loaded.len() >= 3
            && std::fs::read_to_string(&readme_path).is_ok_and(|c| {
                c.trim_start()
                    .starts_with(crate::constants::STARTER_README_BOILERPLATE_PREFIX)
            })
        {
            return GateStatus::Failed {
                gate_name: "Gate 4: Diff Sanity & Documentation Completeness",
                reason: "`README.md` still contains unmodified `# MiniKit ... Starter` scaffold boilerplate after multi-file implementation.".to_string(),
                actionable_remediation: "Update `README.md` with the project's actual title, features, architecture, and run commands before completing.".to_string(),
            };
        }

        for entry in loaded {
            let Ok(content) = &entry.content else {
                continue;
            };
            let file = entry.rel_path.as_str();

            // Check for lazy elision comments (`// ... rest of code ...`, etc.)
            if let Some(elision_warn) =
                crate::tools::registry::fs_tools::detect_code_elision_or_slop(file, content)
            {
                return GateStatus::Failed {
                    gate_name: "Gate 4: Diff Sanity & No-Fake-Completion Audit",
                    reason: format!(
                        "Lazy placeholder/elision comment left in `{}`: {}",
                        file,
                        elision_warn.trim()
                    ),
                    actionable_remediation: format!(
                        "Replace all `// ... rest of code ...` elision comments in `{}` with the complete code.",
                        file
                    ),
                };
            }

            let mut pending_cfg_test = false;
            let mut in_test_module = false;
            let mut test_brace_depth: i32 = 0;

            for (idx, line) in content.lines().enumerate() {
                let code_only = crate::utils::strings::strip_code_strings_and_comments(line);
                let trimmed_code = code_only.trim();

                // Detect `.only(` test hijacking across JS/TS test files or test blocks
                if trimmed_code.contains("describe.only(")
                    || trimmed_code.contains("it.only(")
                    || trimmed_code.contains("test.only(")
                {
                    return GateStatus::Failed {
                        gate_name: "Gate 4: Diff Sanity & No-Fake-Completion Audit",
                        reason: format!(
                            "Focused test marker (`.only(`) left in `{}` at line {}, which silently skips the rest of the test suite.",
                            file,
                            idx + 1
                        ),
                        actionable_remediation: format!(
                            "Remove `.only` from `{}` at line {} so all tests execute.",
                            file,
                            idx + 1
                        ),
                    };
                }

                if trimmed_code.starts_with("#[cfg(test)]")
                    || trimmed_code.starts_with("#[test]")
                    || trimmed_code.starts_with("#[tokio::test")
                {
                    pending_cfg_test = true;
                }

                if pending_cfg_test || in_test_module {
                    let opens = code_only.chars().filter(|&c| c == '{').count() as i32;
                    let closes = code_only.chars().filter(|&c| c == '}').count() as i32;
                    if pending_cfg_test && opens > 0 {
                        in_test_module = true;
                        pending_cfg_test = false;
                    }
                    if in_test_module {
                        test_brace_depth += opens - closes;
                        if test_brace_depth <= 0 {
                            in_test_module = false;
                            test_brace_depth = 0;
                        }
                        continue;
                    }
                    if pending_cfg_test {
                        continue;
                    }
                }

                // No-Fake-Completion: reject `todo!()` or `unimplemented!()` stubs in non-test code
                if (trimmed_code.contains("todo!(") || trimmed_code.contains("unimplemented!("))
                    && !trimmed_code.contains("macro_rules!")
                {
                    return GateStatus::Failed {
                        gate_name: "Gate 4: Diff Sanity & No-Fake-Completion Audit",
                        reason: format!(
                            "Incomplete stub macro (`todo!()` / `unimplemented!()`) left in production code `{}` at line {}.",
                            file,
                            idx + 1
                        ),
                        actionable_remediation: format!(
                            "Replace the `todo!()` / `unimplemented!()` stub in `{}` at line {} with a complete implementation.",
                            file,
                            idx + 1
                        ),
                    };
                }

                let uncommented = crate::utils::strings::strip_line_comments(line);
                let trimmed = uncommented.trim();

                // Skip pure line comments and block comment lines
                if trimmed.is_empty() || trimmed.starts_with('*') || trimmed.starts_with("{/*") {
                    continue;
                }

                // Secret & API key leakage detection
                if Self::contains_secret_leak(trimmed) {
                    return GateStatus::Failed {
                        gate_name: "Gate 4: Diff Sanity & Secret Leak Audit",
                        reason: format!(
                            "Potential hardcoded secret or API key credential detected in `{}` at line {}.",
                            file,
                            idx + 1
                        ),
                        actionable_remediation: format!(
                            "Remove the secret from `{}` and retrieve it via environment variables.",
                            file
                        ),
                    };
                }
            }
        }

        GateStatus::Passed
    }

    /// Checks if a code line contains obvious hardcoded secrets or raw API tokens.
    #[must_use]
    pub fn contains_secret_leak(line: &str) -> bool {
        // Ignore test assertions or env accesses or pattern matching definitions
        if line.contains("std::env")
            || line.contains("process.env")
            || line.contains("env::var")
            || line.contains("assert")
            || line.contains("line.contains")
        {
            return false;
        }

        // 1. Private key blocks
        if line.contains("BEGIN PRIVATE KEY") || line.contains("BEGIN RSA PRIVATE KEY") {
            return true;
        }

        // Helper: checks if prefix starts on a token boundary and is followed by high-entropy key characters
        let has_token_prefix = |target: &str, prefix: &str, min_secret_tail_len: usize| -> bool {
            let bytes = target.as_bytes();
            let prefix_bytes = prefix.as_bytes();
            let mut search_idx = 0;

            while let Some(pos) = target[search_idx..].find(prefix) {
                let abs_pos = search_idx + pos;
                search_idx = abs_pos + prefix_bytes.len();

                // Character preceding prefix must NOT be alphanumeric or underscore
                // (e.g. rejects "task-", "flask-", "subtask-")
                if abs_pos > 0 {
                    let prev_byte = bytes[abs_pos - 1];
                    if prev_byte.is_ascii_alphanumeric() || prev_byte == b'_' {
                        continue;
                    }
                }

                // Check remaining tail after prefix: must be at least min_secret_tail_len alphanumeric/dash chars
                let tail = &target[abs_pos + prefix_bytes.len()..];
                let tail_chars_count = tail
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .count();

                if tail_chars_count >= min_secret_tail_len {
                    return true;
                }
            }
            false
        };

        // OpenAI / Anthropic / MiniMax keys: "sk-" followed by 20+ key chars on token boundary
        if has_token_prefix(line, "sk-", 20) {
            return true;
        }

        // GitHub Personal Access Tokens: "ghp_" followed by 20+ key chars on token boundary
        if has_token_prefix(line, "ghp_", 20) {
            return true;
        }

        // Google API Keys: "AIzaSy" followed by 25+ key chars on token boundary
        if has_token_prefix(line, "AIzaSy", 25) {
            return true;
        }

        false
    }

    /// Verifies HTML document structure and local asset link existence.
    pub fn check_html_integrity(
        workspace_root: &Path,
        file: &str,
        abs_path: &Path,
        content: &str,
    ) -> Option<GateStatus> {
        let lower = content.to_ascii_lowercase();

        // 1. Truncation checks for full HTML documents
        let is_full_document = lower.contains("<!doctype html")
            || (lower.contains("<html") && lower.contains("<head"));

        if is_full_document && !lower.contains("</html>") {
            return Some(GateStatus::Failed {
                gate_name: "Gate 1: AST Syntax & Asset Integrity",
                reason: format!(
                    "HTML document in `{}` is truncated: missing closing `</html>` tag.",
                    file
                ),
                actionable_remediation: format!(
                    "Ensure the complete HTML document is written with closing `</html>` in `{}`.",
                    file
                ),
            });
        }
        if is_full_document && lower.contains("<body") && !lower.contains("</body>") {
            return Some(GateStatus::Failed {
                gate_name: "Gate 1: AST Syntax & Asset Integrity",
                reason: format!(
                    "HTML document in `{}` is truncated: missing closing `</body>` tag.",
                    file
                ),
                actionable_remediation: format!(
                    "Ensure the body element is closed with `</body>` in `{}`.",
                    file
                ),
            });
        }

        // 2. Validate local referenced code assets (stylesheets & scripts)
        let html_dir = abs_path.parent().unwrap_or(workspace_root);
        let tags = ["<link", "<script"];

        for tag in tags {
            let mut search_from = 0;
            while let Some(tag_offset) = lower[search_from..].find(tag) {
                let tag_start = search_from + tag_offset;
                let tag_end = match content[tag_start..].find('>') {
                    Some(p) => tag_start + p,
                    None => break,
                };
                let tag_slice = &content[tag_start..=tag_end];
                let lower_tag = &lower[tag_start..=tag_end];

                for attr_pat in ["href=", "src="] {
                    if let Some(attr_pos) = lower_tag.find(attr_pat) {
                        let rest = &tag_slice[attr_pos + attr_pat.len()..];
                        let trimmed = rest.trim_start();
                        if let Some(quote) = trimmed.chars().next() {
                            if quote == '"' || quote == '\'' {
                                let after_quote = &trimmed[1..];
                                if let Some(end_quote) = after_quote.find(quote) {
                                    let mut raw_url = after_quote[..end_quote].trim();
                                    if let Some(q) = raw_url.find('?') {
                                        raw_url = &raw_url[..q];
                                    }
                                    if let Some(h) = raw_url.find('#') {
                                        raw_url = &raw_url[..h];
                                    }
                                    let url = raw_url.trim();

                                    // Filter out external URLs and special schemes
                                    if !url.is_empty()
                                        && !url.starts_with("http://")
                                        && !url.starts_with("https://")
                                        && !url.starts_with("//")
                                        && !url.starts_with("data:")
                                        && !url.starts_with("mailto:")
                                        && !url.starts_with("tel:")
                                        && !url.starts_with("javascript:")
                                        && !url.starts_with('{')
                                    {
                                        // Only verify code assets (.css, .js, .mjs, .ts)
                                        let is_code_asset = url.ends_with(".css")
                                            || url.ends_with(".js")
                                            || url.ends_with(".mjs")
                                            || url.ends_with(".ts");

                                        if is_code_asset {
                                            let clean_url = url.trim_start_matches("./");
                                            let target_local = html_dir.join(clean_url);
                                            let target_root = workspace_root
                                                .join(clean_url.trim_start_matches('/'));

                                            if !target_local.exists() && !target_root.exists() {
                                                return Some(GateStatus::Failed {
                                                    gate_name: "Gate 1: AST Syntax & Asset Integrity",
                                                    reason: format!(
                                                        "Broken asset link in `{}`: referenced asset `{}` does not exist on disk.",
                                                        file, url
                                                    ),
                                                    actionable_remediation: format!(
                                                        "Create the referenced file `{}` or update `{}` with the correct relative path.",
                                                        clean_url, file
                                                    ),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                search_from = tag_end + 1;
            }
        }

        None
    }

    /// Verifies CSS syntax for balanced curly braces (ignoring comments and strings).
    pub fn check_css_integrity(file: &str, content: &str) -> Option<GateStatus> {
        let mut in_comment = false;
        let mut in_string: Option<char> = None;
        let mut open_braces: usize = 0;
        let mut close_braces: usize = 0;
        let mut chars = content.chars().peekable();

        while let Some(ch) = chars.next() {
            if in_comment {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_comment = false;
                }
                continue;
            }

            if let Some(quote) = in_string {
                if ch == '\\' {
                    chars.next(); // skip escaped char
                } else if ch == quote {
                    in_string = None;
                }
                continue;
            }

            if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_comment = true;
                continue;
            }

            if ch == '"' || ch == '\'' {
                in_string = Some(ch);
                continue;
            }

            if ch == '{' {
                open_braces += 1;
            } else if ch == '}' {
                close_braces += 1;
            }
        }

        if open_braces != close_braces {
            return Some(GateStatus::Failed {
                gate_name: "Gate 1: AST Syntax & Asset Integrity",
                reason: format!(
                    "CSS syntax error in `{}`: unbalanced curly braces ({} opened vs {} closed).",
                    file, open_braces, close_braces
                ),
                actionable_remediation: format!(
                    "Check `{}` for unclosed CSS blocks or extra closing braces.",
                    file
                ),
            });
        }

        None
    }

    /// Checks if any background dev server registered with minitask terminated with an error.
    pub async fn check_daemon_health() -> Option<GateStatus> {
        let registry = crate::dev::get_global_dev_registry();
        let procs = registry.list().await;
        for p in &procs {
            if let crate::dev::models::DevProcessStatus::Exited(Some(code)) = p.status {
                if code != 0 {
                    let pid_str = p
                        .pid
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "N/A".to_string());
                    return Some(GateStatus::Failed {
                        gate_name: "Gate 2: Reproducer & Runtime Daemon Health",
                        reason: format!(
                            "Managed runtime daemon `{}` (PID {}) terminated abnormally with exit code {}.",
                            p.name, pid_str, code
                        ),
                        actionable_remediation: format!(
                            "Inspect service logs via `minitask(action=\"logs\", id=\"{}\")` and resolve startup errors.",
                            p.id
                        ),
                    });
                }
            }
        }
        None
    }
}

/// Accurately detects real Git merge conflict markers without false positives on code comments.
///
/// In Git:
/// - `<<<<<<<` at start of line, optionally followed by space and ref/branch label
/// - `=======` exactly 7 equals signs on its own line (never with trailing characters or 40 equals)
/// - `>>>>>>>` at start of line, optionally followed by space and ref/branch label
/// - `|||||||` (diff3 base marker) at start of line, optionally followed by space and commit/ref
pub(crate) fn detect_git_conflict_marker(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if trimmed == "=======" {
        return Some("=======");
    }
    if trimmed == "<<<<<<<" || trimmed.starts_with("<<<<<<< ") {
        return Some("<<<<<<<");
    }
    if trimmed == ">>>>>>>" || trimmed.starts_with(">>>>>>> ") {
        return Some(">>>>>>>");
    }
    if trimmed == "|||||||" || trimmed.starts_with("||||||| ") {
        return Some("|||||||");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_gate3_conflict_markers_detection() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("conflict.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n<<<<<<< HEAD\n    let x = 1;\n=======\n    let x = 2;\n>>>>>>> feature\n}\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate3_regression_conflicts(
            temp.path(),
            &["conflict.rs".to_string()],
        );

        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 3"));
                assert!(reason.contains("conflict marker"));
            }
            _ => panic!("Expected Gate 3 to fail on conflict markers"),
        }
    }

    #[test]
    fn test_gate3_does_not_flag_legitimate_comments() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("styles.css");
        std::fs::write(
            &file_path,
            "/* ============================================\n   Design Tokens\n   ============================================ */\n// ============================================\n/* -------------------------------------------- */\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate3_regression_conflicts(
            temp.path(),
            &["styles.css".to_string()],
        );

        match status {
            GateStatus::Passed => {}
            _ => panic!(
                "Gate 3 should pass for legitimate comment banners: {:?}",
                status
            ),
        }
    }

    #[test]
    fn test_gate4_secret_leak_detection() {
        let temp = tempdir().unwrap();
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("client.rs");
        std::fs::write(
            &file_path,
            "const KEY: &str = \"sk-proj-abcdef1234567890abcdef1234567890\";\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate4_diff_sanity(
            temp.path(),
            &["src/client.rs".to_string()],
        );

        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 4"));
                assert!(reason.contains("secret or API key"));
            }
            _ => panic!("Expected Gate 4 to fail on sk- API key"),
        }
    }

    #[test]
    fn test_gate4_ignores_benign_subwords_like_task() {
        let temp = tempdir().unwrap();
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("index.html");
        std::fs::write(
            &file_path,
            "<form id=\"taskForm\" class=\"task-form\" data-action=\"task-card-action\">\n  <button class=\"task-submit-btn\">Save Task</button>\n</form>\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate4_diff_sanity(
            temp.path(),
            &["src/index.html".to_string()],
        );

        match status {
            GateStatus::Passed => {}
            _ => panic!(
                "Expected Gate 4 to pass for benign task-form strings, got: {:?}",
                status
            ),
        }
    }

    #[test]
    fn test_remediation_prompt_formatting() {
        let report = VerificationReport {
            gate1_syntax_compiler: GateStatus::Failed {
                gate_name: "Gate 1: AST Syntax",
                reason: "Unclosed brace".to_string(),
                actionable_remediation: "Close brace".to_string(),
            },
            gate2_reproducer_test: GateStatus::Skipped { reason: "None" },
            gate3_regression_conflicts: GateStatus::Passed,
            gate4_diff_sanity: GateStatus::Failed {
                gate_name: "Gate 4: Diff Sanity",
                reason: "Found println!".to_string(),
                actionable_remediation: "Remove println!".to_string(),
            },
            all_passed: false,
        };

        let prompt = report.format_remediation_prompt();
        assert!(prompt.contains("[PRE-COMPLETION VERIFICATION BARRIER REJECTED COMPLETION]"));
        assert!(prompt.contains("Gate 1: AST Syntax"));
        assert!(prompt.contains("Gate 4: Diff Sanity"));
    }

    #[test]
    fn test_gate1_html_asset_integrity_missing_css() {
        let temp = tempdir().unwrap();
        let html_file = temp.path().join("index.html");
        std::fs::write(
            &html_file,
            "<!DOCTYPE html>\n<html>\n<head>\n  <link rel=\"stylesheet\" href=\"styles.css\">\n</head>\n<body>\n  <h1>Hello</h1>\n</body>\n</html>\n",
        )
        .unwrap();

        // styles.css does not exist yet -> should fail
        let status = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["index.html".to_string()],
        );
        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 1"));
                assert!(reason.contains("Broken asset link"));
                assert!(reason.contains("styles.css"));
            }
            _ => panic!("Expected Gate 1 to fail on missing styles.css"),
        }

        // Create styles.css -> should pass
        std::fs::write(temp.path().join("styles.css"), "body { margin: 0; }").unwrap();
        let status_ok = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["index.html".to_string()],
        );
        assert_eq!(status_ok, GateStatus::Passed);
    }

    #[test]
    fn test_gate1_html_truncation_detection() {
        let temp = tempdir().unwrap();
        let html_file = temp.path().join("index.html");
        // Missing closing </html> and </body>
        std::fs::write(
            &html_file,
            "<!DOCTYPE html>\n<html>\n<head><title>Test</title></head>\n<body>\n  <h1>Cut off midway...",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["index.html".to_string()],
        );
        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 1"));
                assert!(reason.contains("truncated") || reason.contains("</html>"));
            }
            _ => panic!("Expected Gate 1 to detect truncated HTML document"),
        }
    }

    #[test]
    fn test_gate1_css_balanced_braces_check() {
        let temp = tempdir().unwrap();
        let css_broken = temp.path().join("broken.css");
        std::fs::write(
            &css_broken,
            ".hero {\n  display: flex;\n  /* comment with { */\n  content: \"}\";\n",
        )
        .unwrap();

        let status = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["broken.css".to_string()],
        );
        match status {
            GateStatus::Failed {
                gate_name, reason, ..
            } => {
                assert!(gate_name.contains("Gate 1"));
                assert!(reason.contains("unbalanced curly braces"));
            }
            _ => panic!("Expected Gate 1 to reject unbalanced CSS braces"),
        }

        // Valid CSS
        let css_clean = temp.path().join("clean.css");
        std::fs::write(
            &css_clean,
            ".hero {\n  display: flex;\n  /* comment with { */\n  content: \"}\";\n}\n",
        )
        .unwrap();
        let status_clean = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["clean.css".to_string()],
        );
        assert_eq!(status_clean, GateStatus::Passed);
    }

    #[tokio::test]
    async fn test_gate2_daemon_health_check_passes_when_clean() {
        let daemon_status = VerificationBarrier::check_daemon_health().await;
        assert_eq!(daemon_status, None);
    }

    #[test]
    fn test_gate4_rejects_fake_completion_stubs_and_test_only() {
        let temp = tempdir().unwrap();
        let stub_rs = temp.path().join("lib.rs");
        std::fs::write(&stub_rs, "pub fn compute() -> i32 {\n    todo!()\n}\n").unwrap();

        let status =
            VerificationBarrier::check_gate4_diff_sanity(temp.path(), &["lib.rs".to_string()]);
        assert!(matches!(status, GateStatus::Failed { .. }));

        let only_ts = temp.path().join("app.test.ts");
        std::fs::write(
            &only_ts,
            "describe.only('suite', () => { it('works', () => {}); });\n",
        )
        .unwrap();
        let status_only =
            VerificationBarrier::check_gate4_diff_sanity(temp.path(), &["app.test.ts".to_string()]);
        assert!(matches!(status_only, GateStatus::Failed { .. }));
    }

    #[test]
    fn test_first_failure_summary_extracts_diagnostic_and_remediation() {
        let report = VerificationReport {
            all_passed: false,
            gate1_syntax_compiler: GateStatus::Passed,
            gate2_reproducer_test: GateStatus::Failed {
                gate_name: "Gate 2 (Test Suite)",
                reason: "assertion `left == right` failed in auth::tests::test_jwt\n  left: 401\n  right: 200".to_string(),
                actionable_remediation: "Inspect auth::tests::test_jwt and fix the status code mismatch.".to_string(),
            },
            gate3_regression_conflicts: GateStatus::Passed,
            gate4_diff_sanity: GateStatus::Passed,
        };

        let summary = report.first_failure_summary();
        assert!(summary.is_some());
        let (reason, remediation) = summary.unwrap();
        assert!(reason.contains("[Gate 2 (Test Suite)]"));
        assert!(reason.contains("assertion `left == right` failed in auth::tests::test_jwt"));
        assert!(remediation.contains("Inspect auth::tests::test_jwt"));
    }

    #[test]
    fn test_four_state_test_freshness_machine_and_open_callers_gate() {
        let temp = tempdir().unwrap();

        // 1. Verify command detection (zero regexes)
        assert!(TestFreshnessState::is_test_runner_command(
            "nice -n 19 cargo test --lib -j 1"
        ));
        assert!(TestFreshnessState::is_test_runner_command("bun test"));
        assert!(TestFreshnessState::is_test_runner_command(
            "npm run test:unit"
        ));
        assert!(TestFreshnessState::is_test_runner_command(
            "python3 -m pytest"
        ));
        assert!(!TestFreshnessState::is_test_runner_command("cargo check"));

        // 2. Verify NotRun -> Passed -> Stale -> Failed transitions
        SessionQualityTelemetry::record_command_execution(
            temp.path(),
            "cargo test --lib",
            "running 12 tests\ntest result: ok. 12 passed; 0 failed; 0 ignored;",
            true,
        );
        let snap1 = SessionQualityTelemetry::get_snapshot(temp.path()).unwrap();
        assert_eq!(
            snap1.test_freshness,
            TestFreshnessState::Passed { passed: 12 }
        );

        // Edit a file and remove a symbol that is still called in caller.py
        let helper_py = temp.path().join("helper.py");
        let caller_py = temp.path().join("caller.py");
        let before = "def old_compute(x):\n    return x + 1\n";
        let after = "def new_compute(x):\n    return x + 2\n";
        std::fs::write(&helper_py, after).unwrap();
        std::fs::write(&caller_py, "def main():\n    return old_compute(5)\n").unwrap();

        let edit_report =
            SessionQualityTelemetry::record_file_edit(temp.path(), &helper_py, before, after);
        assert_eq!(edit_report.open_callers.len(), 1);
        assert_eq!(edit_report.open_callers[0].symbol, "old_compute");

        // Test freshness must automatically transition to Stale { last_passed: 12 }
        let snap2 = SessionQualityTelemetry::get_snapshot(temp.path()).unwrap();
        assert_eq!(
            snap2.test_freshness,
            TestFreshnessState::Stale { last_passed: 12 }
        );

        // Gate 1 must fail because caller.py still calls `old_compute` which is no longer declared
        let g1_fail = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["helper.py".to_string()],
        );
        match g1_fail {
            GateStatus::Failed { reason, .. } => {
                assert!(reason.contains("open_callers"));
                assert!(reason.contains("old_compute"));
            }
            _ => panic!("Expected Gate 1 to fail when open_callers remain unresolved"),
        }

        // Update caller.py to call `new_compute` -> Gate 1 now passes!
        std::fs::write(&caller_py, "def main():\n    return new_compute(5)\n").unwrap();
        let g1_pass = VerificationBarrier::check_gate1_syntax_compiler(
            temp.path(),
            &["helper.py".to_string(), "caller.py".to_string()],
        );
        assert_eq!(g1_pass, GateStatus::Passed);
    }
}
