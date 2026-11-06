use super::transcript::SubagentTranscript;
use super::types::SubagentRole;
use crate::context::ast_diff::AstDiffEngine;
use crate::context::budget::ccr_cache::CcrCache;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Outcome of a command execution (test, build, lint, etc.) run by the subagent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandVerificationOutcome {
    pub command: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub summary: String,
    pub step_index: usize,
}

/// AST symbol modifications discovered in a modified file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstSymbolImpact {
    pub file_path: String,
    pub added_symbols: Vec<String>,
    pub modified_symbols: Vec<String>,
    pub removed_symbols: Vec<String>,
    pub breaking_changes: Vec<String>,
}

/// Synthesized executive report of an isolated subagent's execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentReport {
    pub subagent_id: String,
    pub role: SubagentRole,
    pub status: String,
    pub success: bool,
    pub executive_summary: String,
    pub files_inspected: Vec<String>,
    pub files_modified: Vec<String>,
    pub ast_impact: Vec<AstSymbolImpact>,
    pub verifications: Vec<CommandVerificationOutcome>,
    pub key_findings: Vec<String>,
    pub tokens_used: usize,
    pub turns_executed: usize,
    pub total_steps: usize,
    pub duration_ms: u64,
    pub worktree_branch: Option<String>,
    pub ccr_id: Option<String>,
    #[serde(default)]
    pub result_alias: Option<String>,
    #[serde(default)]
    pub return_prompt: Option<String>,
    #[serde(default)]
    pub report_status: super::types::WorkerReportStatus,
}

impl SubagentReport {
    /// Formats the executive synthesis into a token-efficient, high-density Markdown report.
    pub fn format_markdown(&self) -> String {
        let status_emoji = if self.success {
            "✔"
        } else if self.status == "canceled" {
            "⏹"
        } else {
            "❌"
        };

        let mut out = format!(
            "### {} Subagent `[ID: {} | Role: {} | Status: {}]` {}\n\n",
            status_emoji,
            self.subagent_id,
            self.role.badge(),
            self.report_status.as_str(),
            if self.success {
                "Completed Successfully"
            } else if self.status == "canceled" {
                "Execution Canceled"
            } else {
                "Execution Failed"
            }
        );

        // Executive Summary
        out.push_str("#### 📌 Executive Summary\n");
        if self.executive_summary.trim().is_empty() {
            out.push_str("_(No summary text provided)_\n\n");
        } else {
            out.push_str(self.executive_summary.trim());
            out.push_str("\n\n");
        }

        // File Activity & AST Semantic Impact
        if !self.files_modified.is_empty() || !self.files_inspected.is_empty() {
            out.push_str("#### 📁 File Activity & AST Changes\n");

            if !self.files_modified.is_empty() {
                out.push_str("**Modified Files**:\n");
                for file in &self.files_modified {
                    let impact = self.ast_impact.iter().find(|i| &i.file_path == file);
                    if let Some(imp) = impact {
                        let mut changes = Vec::new();
                        for a in &imp.added_symbols {
                            changes.push(format!("`+{}`", a));
                        }
                        for m in &imp.modified_symbols {
                            changes.push(format!("`~{}`", m));
                        }
                        for r in &imp.removed_symbols {
                            changes.push(format!("`-{}`", r));
                        }
                        if changes.is_empty() {
                            out.push_str(&format!("- `{}` (content updated)\n", file));
                        } else {
                            out.push_str(&format!("- `{}`: {}\n", file, changes.join(", ")));
                        }
                    } else {
                        out.push_str(&format!("- `{}`\n", file));
                    }
                }
            }

            if !self.files_inspected.is_empty() {
                let inspected_preview = if self.files_inspected.len() <= 6 {
                    self.files_inspected
                        .iter()
                        .map(|f| format!("`{}`", f))
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    format!(
                        "{}, and {} more",
                        self.files_inspected[..5]
                            .iter()
                            .map(|f| format!("`{}`", f))
                            .collect::<Vec<_>>()
                            .join(", "),
                        self.files_inspected.len() - 5
                    )
                };
                out.push_str(&format!("**Inspected Files**: {}\n\n", inspected_preview));
            } else {
                out.push('\n');
            }
        }

        // Verification & Command Execution Outcomes
        if !self.verifications.is_empty() {
            out.push_str("#### 🧪 Verification & Command Execution\n");
            for v in &self.verifications {
                let icon = if v.passed { "✅" } else { "❌" };
                out.push_str(&format!(
                    "- {} `{}` (Step {}) — {}\n",
                    icon, v.command, v.step_index, v.summary
                ));
            }
            out.push('\n');
        }

        // Key Findings / Extracted Insights
        if !self.key_findings.is_empty() {
            out.push_str("#### 💡 Key Discoveries & Insights\n");
            for finding in &self.key_findings {
                out.push_str(&format!("- {}\n", finding));
            }
            out.push('\n');
        }

        // Telemetry & Drilldown Pointer
        out.push_str("#### 📊 Resource Metrics & Transcript Drilldown\n");
        let branch_info = self
            .worktree_branch
            .as_ref()
            .map(|b| format!(" | **Branch**: `{}`", b))
            .unwrap_or_default();

        out.push_str(&format!(
            "• **Turns**: {} | **Steps**: {} | **Tokens**: {} | **Duration**: {}ms{}\n",
            self.turns_executed, self.total_steps, self.tokens_used, self.duration_ms, branch_info
        ));

        if let Some(ref ccr) = self.ccr_id {
            out.push_str(&format!("• **CCR Lossless Cache**: `{}`\n", ccr));
        }

        out.push_str(&format!(
            "• **Drilldown**: Call `minitask(action=\"transcript\", id=\"{}\", filter=\"all\")` or `subagent_transcript_drilldown(subagent_id=\"{}\", step_index=N)` to inspect full report, raw arguments, complete output, or stderr.\n",
            self.subagent_id,
            self.subagent_id
        ));

        let relay_handle = if let Some(ref alias) = self.result_alias {
            format!(
                "`$RESULT[{}]` (or `$RESULT[{}]`, `$RESULT[latest:{}]`, `$RESULT[latest]`)",
                alias,
                self.subagent_id,
                self.role.as_str()
            )
        } else {
            format!(
                "`$RESULT[{}]` (or `$RESULT[latest:{}]`, `$RESULT[latest]`)",
                self.subagent_id,
                self.role.as_str()
            )
        };
        out.push_str(&format!(
            "• **Zero-Copy Result Relay**: Reference {} in any subsequent `delegate_task` / `invoke_subagent` / `dispatch_subagent` prompt to pipe this subagent's full output directly without re-typing.\n\n",
            relay_handle
        ));

        out.push_str("<orchestrator_return_directive>\n");
        if let Some(action_hint) = self
            .report_status
            .orchestrator_action_hint(&self.subagent_id)
        {
            out.push_str(&format!("{}\n", action_hint));
        }
        if let Some(ref custom_return) = self.return_prompt {
            out.push_str(&format!("{}\n", custom_return.trim()));
        } else {
            out.push_str(
                "Review, challenge, and verify the subagent output above against the codebase and overarching user goal before trusting its claims or advancing to the next phase.\n",
            );
        }
        out.push_str("</orchestrator_return_directive>\n");

        out
    }
}

/// Context parameters for synthesizing an isolated subagent run into an executive report.
pub struct SubagentSynthesisContext<'a> {
    pub subagent_id: &'a str,
    pub role: &'a SubagentRole,
    pub final_summary: &'a str,
    pub success: bool,
    pub status: &'a str,
    pub files_inspected: &'a [String],
    pub files_modified: &'a [String],
    pub transcript: &'a SubagentTranscript,
    pub workspace_root: &'a Path,
    pub worktree_branch: Option<String>,
    pub result_alias: Option<String>,
    pub return_prompt: Option<String>,
}

/// Reducer engine that synthesizes ephemeral subagent execution traces into executive reports.
pub struct SubagentSynthesisReducer;

impl SubagentSynthesisReducer {
    /// Reduces a completed subagent run into a structured `SubagentReport`.
    pub async fn reduce(ctx: SubagentSynthesisContext<'_>) -> SubagentReport {
        // 1. Analyze AST impact for modified files
        let mut ast_impact = Vec::new();
        for file in ctx.files_modified {
            if let Ok(report) = AstDiffEngine::diff_file(ctx.workspace_root, file, None) {
                let added: Vec<String> = report
                    .added
                    .iter()
                    .map(|a| format!("{} {}", a.kind, a.name))
                    .collect();
                let modified: Vec<String> = report
                    .modified
                    .iter()
                    .map(|m| format!("{} {}", m.kind, m.name))
                    .collect();
                let removed: Vec<String> = report
                    .removed
                    .iter()
                    .map(|r| format!("{} {}", r.kind, r.name))
                    .collect();
                let breaking: Vec<String> = report
                    .breaking_changes
                    .iter()
                    .map(|b| b.symbol_name.clone())
                    .collect();

                if !added.is_empty()
                    || !modified.is_empty()
                    || !removed.is_empty()
                    || !breaking.is_empty()
                {
                    ast_impact.push(AstSymbolImpact {
                        file_path: file.clone(),
                        added_symbols: added,
                        modified_symbols: modified,
                        removed_symbols: removed,
                        breaking_changes: breaking,
                    });
                }
            }
        }

        // 2. Extract command verifications from transcript
        let mut verifications = Vec::new();
        for step in &ctx.transcript.steps {
            let cmd_opt = step
                .arguments
                .get("cmd")
                .or_else(|| step.arguments.get("command"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let is_exec_step = matches!(
                step.tool_name.as_str(),
                "exec_cmd" | "exec" | "sandbox_exec"
            ) || (step.tool_name == "minitask" && cmd_opt.is_some());

            if is_exec_step {
                let cmd_str = cmd_opt.unwrap_or("unknown").to_string();
                let summary = Self::extract_command_summary(&step.output_full, step.success);
                let exit_code = Self::parse_exit_code(&step.output_full, step.success);
                verifications.push(CommandVerificationOutcome {
                    command: cmd_str,
                    passed: step.success,
                    exit_code,
                    summary,
                    step_index: step.step_index,
                });
            }
        }

        // 3. Extract key findings and executive summary
        let (executive_summary, key_findings) =
            Self::extract_executive_summary_and_findings(ctx.final_summary);

        // 4. Calculate total duration (sum of tool step durations, with wall-clock fallback)
        let steps_duration_ms: u64 = ctx.transcript.steps.iter().map(|s| s.duration_ms).sum();
        let wall_clock_ms: u64 = ctx
            .transcript
            .finished_at_secs
            .unwrap_or(ctx.transcript.started_at_secs)
            .saturating_sub(ctx.transcript.started_at_secs)
            .saturating_mul(1000);
        let duration_ms = steps_duration_ms.max(wall_clock_ms);

        // 5. Store complete transcript into CCR Cache for lossless reversible retrieval
        let serialized_transcript =
            serde_json::to_string_pretty(ctx.transcript).unwrap_or_default();
        let ccr_id = if !serialized_transcript.is_empty() {
            Some(CcrCache::store(&serialized_transcript))
        } else {
            None
        };

        let report_status =
            super::types::WorkerReportStatus::parse_from_summary(ctx.final_summary, ctx.success);

        SubagentReport {
            subagent_id: ctx.subagent_id.to_string(),
            role: *ctx.role,
            status: ctx.status.to_string(),

            success: ctx.success,
            executive_summary,
            files_inspected: ctx.files_inspected.to_vec(),
            files_modified: ctx.files_modified.to_vec(),
            ast_impact,
            verifications,
            key_findings,
            tokens_used: ctx.transcript.total_tokens,
            turns_executed: ctx.transcript.turns_executed,
            total_steps: ctx.transcript.steps.len(),
            duration_ms,
            worktree_branch: ctx.worktree_branch,
            ccr_id,
            result_alias: ctx.result_alias,
            return_prompt: ctx.return_prompt,
            report_status,
        }
    }

    /// Extracts a numeric exit code from structured tool output if present, falling back to `0`/`1`.
    fn parse_exit_code(output: &str, success: bool) -> Option<i32> {
        let lower = output.to_ascii_lowercase();
        for prefix in ["exit code:", "exit code ", "exit status:", "exited("] {
            if let Some(idx) = lower.find(prefix) {
                let after = lower[idx + prefix.len()..]
                    .trim_start_matches([' ', ':', '(']);
                let num_str: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '-')
                    .collect();
                if let Ok(code) = num_str.parse::<i32>() {
                    return Some(code);
                }
            }
        }
        Some(if success { 0 } else { 1 })
    }

    /// Extracts a concise summary from command outputs using ANSI stripping,
    /// quantitative metric-density line scoring, and `SmartDonutTruncator` diagnostic detection.
    fn extract_command_summary(output: &str, success: bool) -> String {
        let cleaned = crate::context::budget::log_pruner::LogPruner::strip_ansi(output);

        // 1. Score non-empty lines from the tail by quantitative metric density (<number> + outcome/unit stems)
        let mut best_line: Option<(&str, usize)> = None;
        for (tail_idx, line) in cleaned
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .rev()
            .enumerate()
        {
            let score = Self::score_terminal_summary_line(line, tail_idx);
            if score >= 6 && best_line.map(|(_, s)| score > s).unwrap_or(true) {
                best_line = Some((line, score));
            }
        }
        if let Some((line, _)) = best_line {
            return crate::utils::truncate_ellipsis(line, 100);
        }

        // 2. If failed, surface the highest-priority diagnostic line via SmartDonutTruncator
        if !success {
            for line in cleaned.lines() {
                let l = line.trim();
                if crate::context::donut::SmartDonutTruncator::is_diagnostic_line(l) {
                    return crate::utils::truncate_ellipsis(l, 100);
                }
            }
        }

        // 3. Fallback: first non-empty informative line
        cleaned
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| crate::utils::truncate_ellipsis(l, 100))
            .unwrap_or_else(|| {
                if success {
                    "Command executed successfully (exit code 0)".to_string()
                } else {
                    "Command failed".to_string()
                }
            })
    }

    /// Scores a terminal output line by quantitative outcome-metric density and tail proximity.
    fn score_terminal_summary_line(line: &str, tail_idx: usize) -> usize {
        const OUTCOME_STEMS: &[(&str, usize)] = &[
            ("passed", 5),
            ("failed", 5),
            ("errors", 4),
            ("warnings", 3),
            ("skipped", 3),
            ("ignored", 3),
            ("filtered", 2),
            ("tests", 3),
            ("suites", 3),
            ("succeeded", 5),
            ("finished", 4),
            ("completed", 3),
            ("verified", 4),
            ("too-big", 5),
            ("needs-confirm", 5),
            ("ambiguous", 5),
            ("regressed", 5),
            ("unavailable", 4),
            ("blocked", 4),
            ("ok", 3),
        ];

        let lower = line.to_ascii_lowercase();
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-')
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            return 0;
        }

        let numeric_tokens = tokens
            .iter()
            .filter(|t| {
                t.bytes()
                    .next()
                    .map(|b| b.is_ascii_digit())
                    .unwrap_or(false)
            })
            .count();

        let mut outcome_score = 0usize;
        for (stem, weight) in OUTCOME_STEMS {
            if tokens.iter().any(|t| *t == *stem || t.starts_with(stem)) {
                outcome_score += *weight;
            }
        }

        if outcome_score == 0 {
            return 0;
        }

        // Quantitative pairing bonus: numbers combined with outcome stems (e.g. "81 passed; 0 failed in 0.12s")
        let quant_bonus = if numeric_tokens >= 2 {
            4
        } else if numeric_tokens == 1 {
            2
        } else {
            0
        };
        let tail_bonus = usize::from(tail_idx < 6);
        outcome_score + quant_bonus + tail_bonus
    }

    fn strip_list_marker(trimmed: &str) -> Option<&str> {
        for prefix in ["- ", "* ", "• "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                return Some(rest.trim());
            }
        }
        let digit_len = trimmed.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digit_len > 0 {
            if let Some(rest) = trimmed[digit_len..].strip_prefix(". ") {
                return Some(rest.trim());
            }
        }
        // Also recognize `caveman-explore` (`FastContext`) bare evidence lines: `path/to/file.ext:START-END  reason`
        // or `cavecrew-builder` terminal bailout tokens (`too-big:`, `needs-confirm:`, `ambiguous:`, `regressed:`)
        if Self::is_fastcontext_or_cavecrew_line(trimmed) {
            return Some(trimmed);
        }
        None
    }

    fn is_fastcontext_or_cavecrew_line(trimmed: &str) -> bool {
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("too-big:")
            || lower.starts_with("needs-confirm:")
            || lower.starts_with("ambiguous:")
            || lower.starts_with("regressed:")
            || lower.starts_with("verified:")
        {
            return true;
        }
        let Some(first_token) = trimmed.split_whitespace().next() else {
            return false;
        };
        let Some((path_part, line_part)) = first_token.split_once(':') else {
            return false;
        };
        let has_path_shape =
            (path_part.contains('/') || path_part.contains('.')) && !path_part.starts_with("http");
        let line_clean = line_part.trim_start_matches('L');
        let has_line_digits = line_clean
            .bytes()
            .next()
            .map(|b| b.is_ascii_digit())
            .unwrap_or(false);
        has_path_shape && has_line_digits && trimmed.len() > first_token.len() + 2
    }

    /// Separates main narrative summary from bulleted key findings with SimHash deduplication.
    fn extract_executive_summary_and_findings(text: &str) -> (String, Vec<String>) {
        let stripped = crate::agent::prompt::strip_thought_blocks(text);
        let mut narrative_lines = Vec::new();
        let mut findings = Vec::new();
        let mut finding_hashes: Vec<u64> = Vec::new();

        for line in stripped.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(cleaned) = Self::strip_list_marker(trimmed) {
                if !cleaned.is_empty() && findings.len() < 24 {
                    let h = crate::context::budget::headroom::AdaptiveSizer::simhash_64(cleaned);
                    if finding_hashes.iter().all(|&existing| {
                        crate::context::budget::headroom::AdaptiveSizer::hamming_distance(
                            h, existing,
                        ) > 6
                    }) {
                        finding_hashes.push(h);
                        findings.push(cleaned.to_string());
                    }
                }
            } else if !trimmed.starts_with('#') {
                narrative_lines.push(trimmed);
            }
        }

        let raw_narrative = if narrative_lines.is_empty() {
            stripped.trim().to_string()
        } else {
            narrative_lines.join("\n")
        };
        let compressed_narrative = if raw_narrative.len() > 2400 {
            crate::agent::caveman::compress_markdown_ast_auto(
                &raw_narrative,
                crate::agent::caveman::CavemanLevel::Full,
            )
        } else {
            raw_narrative
        };
        let exec_summary = crate::utils::truncate_ellipsis(&compressed_narrative, 2400);

        (exec_summary, findings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_executive_summary_strips_thought_tags() {
        let raw = "<thought>Let me inspect the screenshot first.</thought>\n<think>Unclosed or closed reasoning</think>\nVisual layout passes all contrast and alignment checks.\n- Hero section renders cleanly\n- Footer links are aligned";
        let (summary, findings) =
            SubagentSynthesisReducer::extract_executive_summary_and_findings(raw);
        assert_eq!(
            summary,
            "Visual layout passes all contrast and alignment checks."
        );
        assert_eq!(findings.len(), 2);
        assert!(!summary.contains("<thought>"));
        assert!(!summary.contains("<think>"));
    }

    #[test]
    fn test_quantitative_metric_density_command_summary() {
        let pytest_out = "collecting ... collected 42 items\ntests/test_api.py ............\n====== 42 passed, 1 skipped in 0.84s ======";
        let summary = SubagentSynthesisReducer::extract_command_summary(pytest_out, true);
        assert!(summary.contains("42 passed"));

        let jest_out = "PASS src/App.test.tsx\nTest Suites: 4 passed, 4 total\nTests: 19 passed, 0 failed, 19 total\nTime: 1.12 s";
        let summary_jest = SubagentSynthesisReducer::extract_command_summary(jest_out, true);
        assert!(summary_jest.contains("19 passed"));
    }

    #[test]
    fn test_worker_report_status_parsing_and_hints() {
        use super::super::types::WorkerReportStatus;

        assert_eq!(
            WorkerReportStatus::parse_from_summary("STATUS: DONE\nImplemented auth handler.", true),
            WorkerReportStatus::Done
        );
        assert_eq!(
            WorkerReportStatus::parse_from_summary(
                "STATUS: DONE_WITH_CONCERNS\nPasses unit tests, but migration needs prod DB check.",
                true
            ),
            WorkerReportStatus::DoneWithConcerns
        );
        assert_eq!(
            WorkerReportStatus::parse_from_summary(
                "<status>NEEDS_CONTEXT</status>\nWhich OAuth provider should be used?",
                true
            ),
            WorkerReportStatus::NeedsContext
        );
        assert_eq!(
            WorkerReportStatus::parse_from_summary("Status: BLOCKED\nMissing cargo.toml.", true),
            WorkerReportStatus::Blocked
        );
        assert_eq!(
            WorkerReportStatus::parse_from_summary("Timed out", false),
            WorkerReportStatus::Blocked
        );

        let hint = WorkerReportStatus::NeedsContext
            .orchestrator_action_hint("coder-1")
            .unwrap();
        assert!(hint.contains("resume_id=\"coder-1\""));
    }
}
