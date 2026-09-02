use crate::agent::types::AgentEvent;
use crate::constants::WORKSPACE_DIR_NAME;
use crate::context::working_memory::WorkingMemory;
use crate::error::{ContextError, Result, ToolError};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CHECKPOINTS_DIR: &str = "checkpoints";

/// Metadata describing a specific named session checkpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointInfo {
    pub id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub label: String,
    pub description: Option<String>,
    pub timestamp: String,
    pub event_count: usize,
    pub has_working_plan: bool,
}

/// Full snapshot payload containing conversation events and working memory states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointData {
    pub info: CheckpointInfo,
    pub events: Vec<AgentEvent>,
    pub plan_content: Option<String>,
    pub findings_content: Option<String>,
    pub progress_content: Option<String>,
}

/// Result report from a state rewind or time-travel operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RewindReport {
    pub checkpoint_id: String,
    pub session_id: String,
    pub label: String,
    pub restored_events: usize,
    pub restored_plan: bool,
    pub status: String,
}

impl RewindReport {
    pub fn format_markdown(&self) -> String {
        format!(
            "# ⏪ Session Time-Travel Rewind Report\n\n\
            **Status:** {}\n\
            - **Restored Checkpoint:** `{}` (`{}`)\n\
            - **Session ID:** `{}`\n\
            - **Restored Events:** {}\n\
            - **Working Memory Restored:** {}\n",
            self.status,
            self.label,
            self.checkpoint_id,
            self.session_id,
            self.restored_events,
            if self.restored_plan {
                "Yes (TASK_PLAN / FINDINGS)"
            } else {
                "No"
            }
        )
    }
}

pub struct SessionCheckpointer;

impl SessionCheckpointer {
    pub fn checkpoints_dir(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(WORKSPACE_DIR_NAME)
            .join(CHECKPOINTS_DIR)
    }

    /// Captures a point-in-time snapshot of the active session and working memory
    pub fn create_checkpoint(
        workspace_root: &Path,
        session_id: &str,
        label: &str,
        description: Option<&str>,
        events: &[AgentEvent],
    ) -> Result<CheckpointInfo> {
        let dir = Self::checkpoints_dir(workspace_root);
        fs::create_dir_all(&dir).map_err(|e| ToolError::FileOp {
            path: dir.display().to_string(),
            source: e,
        })?;

        let timestamp = Utc::now().to_rfc3339();
        let checkpoint_id = format!("ckpt_{}", Utc::now().timestamp_millis());

        // Working memory capture
        let wm = WorkingMemory::new(workspace_root);
        let plan_content = fs::read_to_string(wm.task_plan_path()).ok();
        let findings_content = fs::read_to_string(wm.findings_path()).ok();
        let progress_content = fs::read_to_string(wm.progress_path()).ok();

        let info = CheckpointInfo {
            id: checkpoint_id.clone(),
            session_id: session_id.to_string(),
            parent_id: None,
            label: label.to_string(),
            description: description.map(|s| s.to_string()),
            timestamp,
            event_count: events.len(),
            has_working_plan: plan_content.is_some(),
        };

        let data = CheckpointData {
            info: info.clone(),
            events: events.to_vec(),
            plan_content,
            findings_content,
            progress_content,
        };

        let file_path = dir.join(format!("{}.json", checkpoint_id));
        let json_str = serde_json::to_string_pretty(&data)
            .map_err(|e| ContextError::Memory(format!("Failed to serialize checkpoint: {}", e)))?;

        fs::write(&file_path, json_str).map_err(|e| ToolError::FileOp {
            path: file_path.display().to_string(),
            source: e,
        })?;

        Ok(info)
    }

    /// Lists all available checkpoints for the workspace or specific session
    pub fn list_checkpoints(
        workspace_root: &Path,
        filter_session_id: Option<&str>,
    ) -> Result<Vec<CheckpointInfo>> {
        let dir = Self::checkpoints_dir(workspace_root);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        let entries = fs::read_dir(&dir).map_err(|e| ToolError::FileOp {
            path: dir.display().to_string(),
            source: e,
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<CheckpointData>(&content) {
                        if let Some(sid) = filter_session_id {
                            if data.info.session_id != sid {
                                continue;
                            }
                        }
                        out.push(data.info);
                    }
                }
            }
        }

        out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(out)
    }

    /// Rewinds working memory and returns the snapshot event payload
    pub fn rewind_checkpoint(
        workspace_root: &Path,
        _session_id: &str,
        checkpoint_id: &str,
    ) -> Result<(RewindReport, Vec<AgentEvent>)> {
        let dir = Self::checkpoints_dir(workspace_root);
        let file_path = dir.join(format!("{}.json", checkpoint_id));

        if !file_path.exists() {
            return Err(ContextError::Memory(format!(
                "Checkpoint `{}` not found in `{}`",
                checkpoint_id,
                dir.display()
            ))
            .into());
        }

        let content = fs::read_to_string(&file_path).map_err(|e| ToolError::FileOp {
            path: file_path.display().to_string(),
            source: e,
        })?;

        let data: CheckpointData = serde_json::from_str(&content)
            .map_err(|e| ContextError::Memory(format!("Corrupt checkpoint data: {}", e)))?;

        // Restore Working Memory
        let wm = WorkingMemory::new(workspace_root);
        let _ = fs::create_dir_all(wm.plan_dir());

        let mut restored_plan = false;
        if let Some(plan) = data.plan_content {
            let _ = fs::write(wm.task_plan_path(), plan);
            restored_plan = true;
        }
        if let Some(findings) = data.findings_content {
            let _ = fs::write(wm.findings_path(), findings);
        }
        if let Some(progress) = data.progress_content {
            let _ = fs::write(wm.progress_path(), progress);
        }

        let report = RewindReport {
            checkpoint_id: data.info.id,
            session_id: data.info.session_id,
            label: data.info.label,
            restored_events: data.events.len(),
            restored_plan,
            status: "✔ State successfully rewound to target checkpoint".to_string(),
        };

        Ok((report, data.events))
    }

    /// Forks an existing checkpoint into a new branch checkpoint
    pub fn fork_checkpoint(
        workspace_root: &Path,
        source_checkpoint_id: &str,
        new_label: Option<&str>,
    ) -> Result<CheckpointInfo> {
        let dir = Self::checkpoints_dir(workspace_root);
        let file_path = dir.join(format!("{}.json", source_checkpoint_id));

        if !file_path.exists() {
            return Err(ContextError::Memory(format!(
                "Source checkpoint `{}` not found",
                source_checkpoint_id
            ))
            .into());
        }

        let content = fs::read_to_string(&file_path).map_err(|e| ToolError::FileOp {
            path: file_path.display().to_string(),
            source: e,
        })?;

        let mut data: CheckpointData = serde_json::from_str(&content)
            .map_err(|e| ContextError::Memory(format!("Corrupt checkpoint data: {}", e)))?;

        let new_id = format!("ckpt_fork_{}", Utc::now().timestamp_millis());
        let label = new_label
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Fork of {}", data.info.label));

        data.info.parent_id = Some(data.info.id.clone());
        data.info.id = new_id.clone();
        data.info.label = label;
        data.info.timestamp = Utc::now().to_rfc3339();

        let target_path = dir.join(format!("{}.json", new_id));
        let json_str = serde_json::to_string_pretty(&data).map_err(|e| {
            ContextError::Memory(format!("Failed to serialize forked checkpoint: {}", e))
        })?;

        fs::write(&target_path, json_str).map_err(|e| ToolError::FileOp {
            path: target_path.display().to_string(),
            source: e,
        })?;

        Ok(data.info)
    }
}
