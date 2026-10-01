//! Executive Map-Reduce report generation for multi-agent swarms.
//!
//! Synthesizes interactive Mermaid DAG graphs, execution scorecards, verification traces,
//! and merge arbitration outcomes into `.minicode/swarms/<swarm_id>/REPORT.md`.

use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::agent::swarm::bus::SwarmMessageBus;
use crate::agent::swarm::models::{SwarmError, SwarmExecutionState, SwarmPlan, SwarmTaskStatus};

pub struct SwarmReporter;

impl SwarmReporter {
    /// Formats an executive Markdown report summarizing the entire swarm run.
    #[allow(dead_code)]
    pub fn format_report(
        plan: &SwarmPlan,
        state: &SwarmExecutionState,
        merge_summary: Option<&str>,
    ) -> String {
        let default_dir = Path::new(".minicode").join("swarms").join(&plan.id);
        let swarm_dir = if default_dir.exists() {
            Some(default_dir.as_path())
        } else {
            None
        };
        Self::format_report_with_bus(plan, state, merge_summary, swarm_dir)
    }

    /// Generates executive Markdown report summarizing the swarm run,
    /// optionally reading inter-worker messages from the provided swarm directory.
    #[allow(dead_code)]
    pub fn generate_report_markdown(
        plan: &SwarmPlan,
        state: &SwarmExecutionState,
        merge_summary: Option<&str>,
        swarm_dir: Option<&Path>,
    ) -> String {
        Self::format_report_with_bus(plan, state, merge_summary, swarm_dir)
    }

    /// Formats an executive Markdown report summarizing the entire swarm run,
    /// optionally reading inter-worker coordination messages from the provided swarm directory.
    #[allow(dead_code)]
    pub fn format_report_with_bus(
        plan: &SwarmPlan,
        state: &SwarmExecutionState,
        merge_summary: Option<&str>,
        swarm_dir: Option<&Path>,
    ) -> String {
        let mut doc = String::new();

        // 1. Executive Header
        doc.push_str(&format!("# 🐝 Swarm Execution Report: {}\n\n", plan.title));
        doc.push_str(&format!("**Swarm ID:** `{}`  \n", plan.id));
        doc.push_str(&format!("**Objective:** {}\n\n", plan.objective));

        let duration_secs = state
            .total_duration_ms
            .map(|ms| format!("{:.2}s", ms as f64 / 1000.0))
            .unwrap_or_else(|| "N/A".to_string());

        let total_tasks = plan.tasks.len();
        let completed = state.count_by_status(SwarmTaskStatus::Completed);
        let failed = state.count_by_status(SwarmTaskStatus::Failed);
        let blocked = state.count_by_status(SwarmTaskStatus::Blocked);

        doc.push_str("### Executive Summary\n\n");
        doc.push_str(&format!("- **Total Wall-Clock Time:** {}\n", duration_secs));
        doc.push_str(&format!("- **Total Workers / Tasks:** {}\n", total_tasks));
        doc.push_str(&format!(
            "- **Results:** ✔ {} Completed | ✖ {} Failed | ⊘ {} Blocked\n",
            completed, failed, blocked
        ));
        doc.push_str(&format!(
            "- **Total Tokens Consumed:** {}\n\n",
            state.total_tokens
        ));

        // 2. Mermaid DAG Diagram
        doc.push_str("### Swarm Dependency Graph (DAG)\n\n");
        doc.push_str("```mermaid\ngraph TD\n");
        for task in &plan.tasks {
            let status = state
                .task_statuses
                .get(&task.id)
                .copied()
                .unwrap_or(SwarmTaskStatus::Pending);

            let status_icon = match status {
                SwarmTaskStatus::Completed => "✔",
                SwarmTaskStatus::Failed => "✖",
                SwarmTaskStatus::Blocked => "⊘",
                SwarmTaskStatus::Running => "●",
                _ => "○",
            };

            doc.push_str(&format!(
                "    {}[\"{} {}: {}\"]\n",
                task.id, status_icon, task.id, task.title
            ));

            for dep in &task.dependencies {
                doc.push_str(&format!("    {} --> {}\n", dep, task.id));
            }
        }
        doc.push_str("```\n\n");

        // 3. Task Scorecard Table
        doc.push_str("### Worker Task Scorecard\n\n");
        doc.push_str("| Task ID | Specialized Role | Status | Duration | Verification Check | Files Changed |\n");
        doc.push_str("| :--- | :--- | :--- | :--- | :--- | :--- |\n");

        for task in &plan.tasks {
            let status = state
                .task_statuses
                .get(&task.id)
                .copied()
                .unwrap_or(SwarmTaskStatus::Pending);

            let outcome = state.outcomes.get(&task.id);
            let dur_str = outcome
                .map(|o| format!("{:.2}s", o.duration_ms as f64 / 1000.0))
                .unwrap_or_else(|| "-".to_string());

            let check_str = match &task.check_command {
                Some(cmd) => {
                    let passed = outcome.map(|o| o.verification_passed).unwrap_or(false);
                    if passed {
                        format!("✔ `{}`", cmd)
                    } else if outcome.is_some() {
                        format!("✖ `{}`", cmd)
                    } else {
                        format!("○ `{}`", cmd)
                    }
                }
                None => "None (Unchecked)".to_string(),
            };

            let files_count = outcome.map(|o| o.files_modified.len()).unwrap_or(0);
            let files_str = if files_count > 0 {
                format!("{} file(s)", files_count)
            } else {
                "-".to_string()
            };

            doc.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} |\n",
                task.id,
                task.role_title,
                status.badge(),
                dur_str,
                check_str,
                files_str
            ));
        }
        doc.push('\n');

        // 4. Inter-Worker Coordination Log
        if let Some(dir) = swarm_dir {
            let bus = SwarmMessageBus::new(dir).ok();
            let messages = bus.and_then(|b| b.all_messages().ok()).unwrap_or_default();

            if !messages.is_empty() {
                doc.push_str("## 💬 Inter-Worker Coordination Log\n\n");
                doc.push_str("| Timestamp | From | To | Intent | Topic | Preview |\n");
                doc.push_str("| :--- | :--- | :--- | :--- | :--- | :--- |\n");
                for msg in &messages {
                    let target = msg.to_task.as_deref().unwrap_or("Wave Broadcast");
                    let time_fmt = msg.timestamp.split('T').nth(1).unwrap_or(&msg.timestamp);
                    let clean_time = time_fmt.split('.').next().unwrap_or(time_fmt);
                    let clean_topic = msg.topic.replace('|', "\\|");
                    let preview = msg.payload.replace('\n', " ").replace('`', "'");
                    let truncated_preview = if preview.chars().count() > 60 {
                        let mut s: String = preview.chars().take(57).collect();
                        s.push_str("...");
                        s
                    } else {
                        preview
                    };
                    doc.push_str(&format!(
                        "| `{}` | `{}` | `{}` | {} | `{}` | `{}` |\n",
                        clean_time,
                        msg.from_task,
                        target,
                        msg.intent.badge(),
                        clean_topic,
                        truncated_preview
                    ));
                }
                doc.push('\n');
            }
        }

        // 5. Detailed Task Outputs & Artifacts
        doc.push_str("### Detailed Worker Outcomes\n\n");
        for task in &plan.tasks {
            if let Some(outcome) = state.outcomes.get(&task.id) {
                doc.push_str(&format!("#### `{}` — {}\n\n", task.id, task.title));
                doc.push_str(&format!("- **Role:** {}\n", outcome.role_title));
                doc.push_str(&format!("- **Status:** {}\n", outcome.status.badge()));
                doc.push_str(&format!(
                    "- **Duration:** {:.2}s\n",
                    outcome.duration_ms as f64 / 1000.0
                ));

                if !outcome.files_modified.is_empty() {
                    doc.push_str(&format!(
                        "- **Files Modified:** `{}`\n",
                        outcome.files_modified.join("`, `")
                    ));
                }

                if !outcome.generated_artifacts.is_empty() {
                    doc.push_str("- **Published Artifacts:**\n");
                    for path in outcome.generated_artifacts.keys() {
                        doc.push_str(&format!("  * `{}`\n", path));
                    }
                }

                if let Some(err) = &outcome.error {
                    doc.push_str(&format!("> [!WARNING]\n> **Worker Error:** {}\n\n", err));
                }

                if !outcome.summary.is_empty() {
                    doc.push_str(&format!("**Summary:**\n{}\n\n", outcome.summary));
                }
            }
        }

        // 6. Merge Arbitration Summary
        if let Some(merge) = merge_summary {
            doc.push_str("### Git Worktree Merge Arbitration\n\n");
            doc.push_str(merge);
            doc.push_str("\n\n");
        }

        doc
    }

    /// Persists the report to `.minicode/swarms/<swarm_id>/REPORT.md`.
    pub fn save_report(
        workspace_root: &Path,
        plan: &SwarmPlan,
        state: &SwarmExecutionState,
        merge_summary: Option<&str>,
    ) -> Result<PathBuf, SwarmError> {
        let swarms_dir = workspace_root
            .join(".minicode")
            .join("swarms")
            .join(&plan.id);
        fs::create_dir_all(&swarms_dir)?;

        let report_path = swarms_dir.join("REPORT.md");
        let content = Self::format_report_with_bus(plan, state, merge_summary, Some(&swarms_dir));
        fs::write(&report_path, content)?;

        info!(path = ?report_path, "Swarm execution report successfully saved");
        Ok(report_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::swarm::bus::{SwarmMessage, SwarmMessageIntent};
    use tempfile::tempdir;

    #[test]
    fn test_format_report_with_coordination_log() {
        let dir = tempdir().unwrap();
        let swarm_dir = dir.path().to_path_buf();
        let bus = SwarmMessageBus::new(&swarm_dir).unwrap();

        let msg1 = SwarmMessage::new(
            "swarm-test",
            "worker_1",
            Some("worker_2"),
            SwarmMessageIntent::PublishContract,
            "AuthContract",
            "pub struct AuthToken(pub String);",
        );
        let msg2 = SwarmMessage::new(
            "swarm-test",
            "worker_2",
            None,
            SwarmMessageIntent::CoordinationNote,
            "DB Notice",
            "Database migration applied successfully.",
        );
        let long_payload = "A".repeat(80);
        let msg3 = SwarmMessage::new(
            "swarm-test",
            "worker_3",
            None,
            SwarmMessageIntent::QueryInterface,
            "LongTopic",
            long_payload,
        );
        bus.post_message(msg1).unwrap();
        bus.post_message(msg2).unwrap();
        bus.post_message(msg3).unwrap();

        let plan = SwarmPlan {
            id: "swarm-test".to_string(),
            title: "Test Swarm".to_string(),
            objective: "Integration test".to_string(),
            created_at: "2026-10-01T00:00:00Z".to_string(),
            metadata: std::collections::HashMap::new(),
            tasks: vec![],
        };
        let state = SwarmExecutionState::new(&plan);

        let report = SwarmReporter::format_report_with_bus(&plan, &state, None, Some(&swarm_dir));

        assert!(report.contains("## 💬 Inter-Worker Coordination Log"));
        assert!(report.contains("| Timestamp | From | To | Intent | Topic | Preview |"));
        assert!(report.contains("`worker_1`"));
        assert!(report.contains("`worker_2`"));
        assert!(report.contains("`Wave Broadcast`"));
        assert!(report.contains("📜 Contract"));
        assert!(report.contains("📢 Note"));
        assert!(report.contains("❓ Query"));
        assert!(report.contains("AuthContract"));
        assert!(report.contains("pub struct AuthToken(pub String);"));
        // msg3 should be truncated with ellipsis
        assert!(report.contains("..."));
    }

    #[test]
    fn test_format_report_empty_messages_omits_log_table() {
        let plan = SwarmPlan {
            id: "swarm-empty".to_string(),
            title: "Empty Swarm".to_string(),
            objective: "Empty test".to_string(),
            created_at: "2026-10-01T00:00:00Z".to_string(),
            metadata: std::collections::HashMap::new(),
            tasks: vec![],
        };
        let state = SwarmExecutionState::new(&plan);

        let report = SwarmReporter::format_report(&plan, &state, None);
        assert!(!report.contains("## 💬 Inter-Worker Coordination Log"));
    }
}
