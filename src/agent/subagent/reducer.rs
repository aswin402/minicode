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
            "### {} Subagent `[ID: {} | Role: {}]` {}\n\n",
            status_emoji,
            self.subagent_id,
            self.role.badge(),
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
            "• **Drilldown**: Call `subagent_transcript_drilldown(subagent_id=\"{}\", step_index=N)` to inspect raw arguments, complete output, or stderr.\n",
            self.subagent_id
        ));

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
            if step.tool_name == "exec_cmd" || step.tool_name == "exec" {
                let cmd_str = step
                    .arguments
                    .get("cmd")
                    .or_else(|| step.arguments.get("command"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let summary = Self::extract_command_summary(&step.output_full, step.success);
                verifications.push(CommandVerificationOutcome {
                    command: cmd_str,
                    passed: step.success,
                    exit_code: if step.success { Some(0) } else { Some(1) },
                    summary,
                    step_index: step.step_index,
                });
            }
        }

        // 3. Extract key findings and executive summary
        let (executive_summary, key_findings) =
            Self::extract_executive_summary_and_findings(ctx.final_summary);

        // 4. Calculate total duration
        let duration_ms: u64 = ctx.transcript.steps.iter().map(|s| s.duration_ms).sum();

        // 5. Store complete transcript into CCR Cache for lossless reversible retrieval
        let serialized_transcript =
            serde_json::to_string_pretty(ctx.transcript).unwrap_or_default();
        let ccr_id = if !serialized_transcript.is_empty() {
            Some(CcrCache::store(&serialized_transcript))
        } else {
            None
        };

        SubagentReport {
            subagent_id: ctx.subagent_id.to_string(),
            role: ctx.role.clone(),
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
        }
    }

    /// Extracts a concise summary from command outputs (e.g. `test result: ok`, `error: ...`).
    fn extract_command_summary(output: &str, success: bool) -> String {
        let lines: Vec<&str> = output.lines().collect();

        // Check for cargo test / test runner summary
        for line in lines.iter().rev() {
            let l = line.trim();
            if l.starts_with("test result:") {
                return l.to_string();
            }
            if l.contains("passed") && l.contains("failed") {
                return l.to_string();
            }
            if l.starts_with("Finished") || l.starts_with("Compiling") {
                return l.to_string();
            }
        }

        // Check for error lines if failed
        if !success {
            for line in &lines {
                let l = line.trim();
                if l.starts_with("error[") || l.starts_with("error:") || l.starts_with("FAILED") {
                    return crate::utils::truncate_ellipsis(l, 80);
                }
            }
        }

        // Fallback: first non-empty line
        lines
            .iter()
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
            .map(|l| crate::utils::truncate_ellipsis(l, 80))
            .unwrap_or_else(|| {
                if success {
                    "Command executed successfully (exit code 0)".to_string()
                } else {
                    "Command failed".to_string()
                }
            })
    }

    /// Separates main narrative summary from bulleted key findings.
    fn extract_executive_summary_and_findings(text: &str) -> (String, Vec<String>) {
        let mut narrative_lines = Vec::new();
        let mut findings = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Detect bullet points or numbered lists
            if trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("• ")
                || (trimmed.len() > 3
                    && trimmed
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
                    && trimmed.chars().nth(1) == Some('.'))
            {
                let cleaned = trimmed
                    .trim_start_matches(|c: char| {
                        c == '-'
                            || c == '*'
                            || c == '•'
                            || c.is_ascii_digit()
                            || c == '.'
                            || c.is_whitespace()
                    })
                    .trim();
                if !cleaned.is_empty() && findings.len() < 8 {
                    findings.push(cleaned.to_string());
                }
            } else if !trimmed.starts_with('#') {
                narrative_lines.push(trimmed);
            }
        }

        let exec_summary = if narrative_lines.is_empty() {
            crate::utils::truncate_ellipsis(text.trim(), 400)
        } else {
            crate::utils::truncate_ellipsis(&narrative_lines.join(" "), 400)
        };

        (exec_summary, findings)
    }
}
