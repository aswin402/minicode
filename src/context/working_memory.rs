use crate::constants::{
    ARCHIVE_DIR, FINDINGS_FILE, MAX_PLAN_LINES_IN_PROMPT, PLAN_DIR, PROGRESS_FILE,
    PROGRESS_TRUNCATE_THRESHOLD, TASK_PLAN_FILE, TIMESTAMP_FORMAT, WORKSPACE_DIR_NAME,
};
use crate::error::{ContextError, Result};
use chrono::Utc;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Filesystem Working Memory for complex multi-step coding tasks.
/// Stored in `.minicode/plan/` with auto-archiving when finished.
#[derive(Debug, Clone)]
pub struct WorkingMemory {
    workspace_root: PathBuf,
}

impl WorkingMemory {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    pub fn plan_dir(&self) -> PathBuf {
        self.workspace_root.join(WORKSPACE_DIR_NAME).join(PLAN_DIR)
    }

    pub fn task_plan_path(&self) -> PathBuf {
        self.plan_dir().join(TASK_PLAN_FILE)
    }

    pub fn findings_path(&self) -> PathBuf {
        self.plan_dir().join(FINDINGS_FILE)
    }

    pub fn progress_path(&self) -> PathBuf {
        self.plan_dir().join(PROGRESS_FILE)
    }

    pub fn archive_dir(&self) -> PathBuf {
        self.plan_dir().join(ARCHIVE_DIR)
    }

    /// Checks if there is an active task plan in progress without TOCTOU race
    pub fn has_active_plan(&self) -> bool {
        let plan_path = self.task_plan_path();
        match fs::read_to_string(&plan_path) {
            Ok(s) => !s.trim().is_empty(),
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::warn!(path = %plan_path.display(), error = %e, "Failed to inspect task plan");
                }
                false
            }
        }
    }

    /// Initializes a new task plan with structured steps
    pub fn init_plan(&self, title: &str, steps: &[String]) -> Result<()> {
        let dir = self.plan_dir();
        fs::create_dir_all(&dir).map_err(|e| ContextError::Memory(e.to_string()))?;

        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();

        let mut plan_content = format!(
            "# Task Plan: {}\n\n> Created: {}\n\n## Objectives & Steps:\n",
            title, timestamp
        );
        for (idx, step) in steps.iter().enumerate() {
            plan_content.push_str(&format!("{}. [ ] {}\n", idx + 1, step));
        }

        fs::write(self.task_plan_path(), plan_content)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        let initial_progress = format!(
            "# Progress Tracker\n\n> Initialized: {}\n\n- Active Goal: {}\n- Status: In Progress\n",
            timestamp, title
        );
        fs::write(self.progress_path(), initial_progress)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        let initial_findings = format!(
            "# Discoveries & Code Findings\n\n> Workspace: {}\n\n",
            self.workspace_root.display()
        );
        fs::write(self.findings_path(), initial_findings)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        Ok(())
    }

    /// Reads the current active plan if available
    pub fn read_plan(&self) -> Result<Option<String>> {
        let path = self.task_plan_path();
        match fs::read_to_string(&path) {
            Ok(content) => Ok(Some(content)),
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    Ok(None)
                } else {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to read task plan");
                    Err(ContextError::Memory(e.to_string()).into())
                }
            }
        }
    }

    /// Appends a new architectural finding or observation
    pub fn append_finding(&self, finding: &str) -> Result<()> {
        let path = self.findings_path();
        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
        let entry = format!("\n### [{}] Observation\n{}\n", timestamp, finding);

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        file.write_all(entry.as_bytes())
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        Ok(())
    }

    /// Updates the progress status of a task step
    pub fn update_progress(&self, step: &str, status: &str) -> Result<()> {
        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
        let entry = format!("\n- [{}] **{}**: {}\n", timestamp, status, step);

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.progress_path())
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        file.write_all(entry.as_bytes())
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        // If the task plan exists, update checkboxes if matching
        if let Ok(Some(plan_content)) = self.read_plan() {
            if status.eq_ignore_ascii_case("completed") || status.eq_ignore_ascii_case("done") {
                let mut updated_lines = Vec::new();
                let mut modified = false;
                let step_trimmed = step.trim();
                for line in plan_content.lines() {
                    let trimmed = line.trim();
                    if !modified
                        && (trimmed.starts_with("- [ ]")
                            || (trimmed.contains("[ ]") && trimmed.contains(step_trimmed)))
                        && trimmed.contains(step_trimmed)
                    {
                        updated_lines.push(line.replacen("[ ]", "[x]", 1));
                        modified = true;
                    } else {
                        updated_lines.push(line.to_string());
                    }
                }
                if modified {
                    let updated = updated_lines.join("\n");
                    if let Err(e) = fs::write(self.task_plan_path(), updated) {
                        tracing::warn!(error = %e, "Failed to update checkbox in task plan");
                    }
                }
            }
        }

        Ok(())
    }

    /// Archives the active plan once all steps are completed
    pub fn archive_plan(&self) -> Result<Option<PathBuf>> {
        if !self.has_active_plan() {
            return Ok(None);
        }

        let archive_dir = self.archive_dir();
        fs::create_dir_all(&archive_dir).map_err(|e| ContextError::Memory(e.to_string()))?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let archive_file = archive_dir.join(format!("{}_archived_plan.md", timestamp));

        let plan = fs::read_to_string(self.task_plan_path()).map_err(|e| {
            ContextError::Memory(format!("Failed to read task plan during archive: {}", e))
        })?;

        let progress = if self.progress_path().exists() {
            fs::read_to_string(self.progress_path()).map_err(|e| {
                ContextError::Memory(format!("Failed to read progress during archive: {}", e))
            })?
        } else {
            String::new()
        };

        let findings = if self.findings_path().exists() {
            fs::read_to_string(self.findings_path()).map_err(|e| {
                ContextError::Memory(format!("Failed to read findings during archive: {}", e))
            })?
        } else {
            String::new()
        };

        let combined = format!(
            "# Archived Plan ({})\n\n{}\n\n---\n\n{}\n\n---\n\n{}",
            timestamp, plan, progress, findings
        );

        fs::write(&archive_file, combined).map_err(|e| ContextError::Memory(e.to_string()))?;

        // Clean up active working memory files
        if let Err(e) = fs::remove_file(self.task_plan_path()) {
            tracing::warn!(error = %e, "Failed to remove task_plan.md after archiving");
        }
        if let Err(e) = fs::remove_file(self.progress_path()) {
            tracing::warn!(error = %e, "Failed to remove progress.md after archiving");
        }
        if let Err(e) = fs::remove_file(self.findings_path()) {
            tracing::warn!(error = %e, "Failed to remove findings.md after archiving");
        }

        Ok(Some(archive_file))
    }

    /// Renders active working memory as an in-context `<working_memory>` XML block (~200 tokens)
    pub fn to_prompt_block(&self) -> String {
        if !self.has_active_plan() {
            return String::new();
        }

        let mut block = String::with_capacity(512);
        block.push_str("<working_memory>\n");

        if let Ok(Some(plan)) = self.read_plan() {
            block.push_str("# Active Task Plan:\n");
            // Include up to first MAX_PLAN_LINES_IN_PROMPT lines of plan
            for line in plan.lines().take(MAX_PLAN_LINES_IN_PROMPT) {
                block.push_str(line);
                block.push('\n');
            }
        }

        if let Ok(progress) = fs::read_to_string(self.progress_path()) {
            block.push_str("\n# Recent Progress:\n");
            let lines: Vec<&str> = progress.lines().collect();
            let recent = if lines.len() > PROGRESS_TRUNCATE_THRESHOLD {
                &lines[lines.len() - PROGRESS_TRUNCATE_THRESHOLD..]
            } else {
                &lines[..]
            };
            for line in recent {
                block.push_str(line);
                block.push('\n');
            }
        }

        if let Ok(findings) = fs::read_to_string(self.findings_path()) {
            let non_empty_lines: Vec<&str> = findings
                .lines()
                .filter(|l| {
                    !l.trim().is_empty()
                        && !l.starts_with("# Discoveries")
                        && !l.starts_with("> Workspace:")
                })
                .collect();
            if !non_empty_lines.is_empty() {
                block.push_str("\n# Key Findings & Discoveries:\n");
                let recent = if non_empty_lines.len() > 10 {
                    &non_empty_lines[non_empty_lines.len() - 10..]
                } else {
                    &non_empty_lines[..]
                };
                for line in recent {
                    block.push_str(line);
                    block.push('\n');
                }
            }
        }

        block.push_str("</working_memory>");
        block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory_lifecycle() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_wm_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        assert!(!wm.has_active_plan());

        // Initialize plan
        let steps = vec![
            "Scaffold API endpoints".to_string(),
            "Write integration tests".to_string(),
            "Run cargo clippy".to_string(),
        ];
        wm.init_plan("Build v0.1.0 API", &steps).unwrap();
        assert!(wm.has_active_plan());

        // Read plan
        let plan = wm.read_plan().unwrap().unwrap();
        assert!(plan.contains("Build v0.1.0 API"));
        assert!(plan.contains("1. [ ] Scaffold API endpoints"));

        // Append finding
        wm.append_finding("Discovered existing axum router in src/routes.rs")
            .unwrap();
        let findings = fs::read_to_string(wm.findings_path()).unwrap();
        assert!(findings.contains("axum router in src/routes.rs"));

        // Update progress & check box
        wm.update_progress("Scaffold API endpoints", "Completed")
            .unwrap();
        let updated_plan = wm.read_plan().unwrap().unwrap();
        assert!(updated_plan.contains("[x] Scaffold API endpoints"));

        // Prompt block formatting
        let block = wm.to_prompt_block();
        assert!(block.contains("<working_memory>"));
        assert!(block.contains("Active Task Plan:"));

        // Archive plan
        let archived = wm.archive_plan().unwrap().unwrap();
        assert!(archived.exists());
        assert!(!wm.has_active_plan());

        fs::remove_dir_all(temp_dir).ok();
    }
}
