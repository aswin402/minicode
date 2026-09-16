use super::types::SubagentRole;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// A single step record in an isolated subagent's execution trace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentStepRecord {
    pub step_index: usize,
    pub turn: usize,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub output_snippet: String,
    pub output_full: String,
    pub success: bool,
    pub duration_ms: u64,
    pub timestamp_secs: u64,
}

impl SubagentStepRecord {
    pub fn new(
        step_index: usize,
        turn: usize,
        tool_name: String,
        arguments: serde_json::Value,
        output_full: String,
        success: bool,
        duration_ms: u64,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Create a crisp snippet (first ~200 chars or first line)
        let snippet = Self::make_snippet(&output_full, 240);

        Self {
            step_index,
            turn,
            tool_name,
            arguments,
            output_snippet: snippet,
            output_full,
            success,
            duration_ms,
            timestamp_secs: now,
        }
    }

    fn make_snippet(text: &str, max_chars: usize) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return "(empty output)".to_string();
        }

        // Check if there is a first non-empty line
        let first_line = trimmed.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        if first_line.len() <= max_chars {
            first_line.to_string()
        } else {
            format!("{}...", &first_line[..max_chars.saturating_sub(3)])
        }
    }
}

/// Full execution transcript of a subagent worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentTranscript {
    pub subagent_id: String,
    pub role: SubagentRole,
    pub prompt: String,
    pub steps: Vec<SubagentStepRecord>,
    pub total_tokens: usize,
    pub turns_executed: usize,
    pub started_at_secs: u64,
    pub finished_at_secs: Option<u64>,
}

impl SubagentTranscript {
    pub fn new(subagent_id: &str, role: SubagentRole, prompt: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            subagent_id: subagent_id.to_string(),
            role,
            prompt,
            steps: Vec::new(),
            total_tokens: 0,
            turns_executed: 0,
            started_at_secs: now,
            finished_at_secs: None,
        }
    }

    pub fn add_step(&mut self, record: SubagentStepRecord) {
        self.steps.push(record);
    }

    pub fn get_step(&self, step_index: usize) -> Option<&SubagentStepRecord> {
        if step_index == 0 {
            return None;
        }
        self.steps.iter().find(|s| s.step_index == step_index)
    }

    #[allow(dead_code)]
    pub fn filter_steps<'a>(&'a self, filter: &str) -> Vec<&'a SubagentStepRecord> {
        match filter.to_lowercase().as_str() {
            "errors" | "error" | "failed" => self.steps.iter().filter(|s| !s.success).collect(),
            "tools" => self.steps.iter().collect(),
            "commands" | "exec" => self
                .steps
                .iter()
                .filter(|s| s.tool_name == "exec_cmd" || s.tool_name == "exec")
                .collect(),
            "files" | "fs" => self
                .steps
                .iter()
                .filter(|s| {
                    s.tool_name.contains("file")
                        || s.tool_name.contains("patch")
                        || s.tool_name.contains("write")
                })
                .collect(),
            _ => self.steps.iter().collect(),
        }
    }

    /// Formats a concise Markdown overview table of all steps.
    pub fn format_overview(&self, max_steps: Option<usize>) -> String {
        let mut out = format!(
            "### 📜 Subagent Execution Transcript: `{}`\n• **Role**: {}\n• **Steps Executed**: {}\n• **Turns**: {}\n\n",
            self.subagent_id,
            self.role.badge(),
            self.steps.len(),
            self.turns_executed
        );

        if self.steps.is_empty() {
            out.push_str("_(No tool steps recorded)_\n");
            return out;
        }

        out.push_str("| # | Turn | Tool | Status | Duration | Preview |\n");
        out.push_str("|---|------|------|--------|----------|---------|\n");

        let limit = max_steps.unwrap_or(self.steps.len());
        for step in self.steps.iter().take(limit) {
            let status_badge = if step.success { "✅ OK" } else { "❌ ERR" };
            let preview = step.output_snippet.replace('|', "\\|").replace('\n', " ");
            let preview_trunc = if preview.len() > 60 {
                format!("{}...", &preview[..57])
            } else {
                preview
            };
            out.push_str(&format!(
                "| {} | {} | `{}` | {} | {}ms | {} |\n",
                step.step_index,
                step.turn,
                step.tool_name,
                status_badge,
                step.duration_ms,
                preview_trunc
            ));
        }

        if self.steps.len() > limit {
            out.push_str(&format!(
                "\n_... and {} more steps. Call `subagent_transcript_drilldown(subagent_id=\"{}\", step_index=N)` to inspect a specific step._\n",
                self.steps.len() - limit,
                self.subagent_id
            ));
        }

        out
    }

    /// Formats detailed step inspection for drilldown.
    pub fn format_step_detail(&self, step_idx: usize, max_lines: Option<usize>) -> Option<String> {
        let step = self.get_step(step_idx)?;
        let max_l = max_lines.unwrap_or(120);

        let mut out = format!(
            "### 🔍 Step {} Details (`{}`)\n• **Subagent ID**: `{}`\n• **Turn**: {}\n• **Tool**: `{}`\n• **Status**: {}\n• **Duration**: {}ms\n\n",
            step.step_index,
            step.tool_name,
            self.subagent_id,
            step.turn,
            step.tool_name,
            if step.success { "✅ Success" } else { "❌ Failed" },
            step.duration_ms
        );

        out.push_str("#### Parameters\n```json\n");
        out.push_str(&serde_json::to_string_pretty(&step.arguments).unwrap_or_default());
        out.push_str("\n```\n\n");

        out.push_str("#### Tool Output\n```\n");
        let lines: Vec<&str> = step.output_full.lines().collect();
        if lines.len() <= max_l {
            out.push_str(&step.output_full);
        } else {
            for line in lines.iter().take(max_l) {
                out.push_str(line);
                out.push('\n');
            }
            out.push_str(&format!(
                "\n... [Truncated {} additional lines. Use max_lines parameter to view more]",
                lines.len() - max_l
            ));
        }
        out.push_str("\n```\n");

        Some(out)
    }

    /// Formats all failed/error steps for rapid diagnostic review.
    pub fn format_errors(&self) -> String {
        let errors: Vec<_> = self.steps.iter().filter(|s| !s.success).collect();
        if errors.is_empty() {
            return format!("✔ No errors recorded for subagent `{}`.", self.subagent_id);
        }

        let mut out = format!(
            "### ⚠️ Failed Tool Steps in Subagent `{}` ({} error(s))\n\n",
            self.subagent_id,
            errors.len()
        );

        for err_step in errors {
            out.push_str(&format!(
                "- **Step {}** (`{}` in turn {}): {}\n  ```\n  {}\n  ```\n\n",
                err_step.step_index,
                err_step.tool_name,
                err_step.turn,
                err_step.output_snippet,
                crate::utils::truncate_ellipsis(&err_step.output_full, 300)
            ));
        }

        out
    }
}

/// Thread-safe in-memory and disk-backed store for subagent execution transcripts.
#[derive(Debug, Default)]
pub struct SubagentTranscriptStore {
    cache: RwLock<HashMap<String, SubagentTranscript>>,
}

impl SubagentTranscriptStore {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Saves a transcript to memory and asynchronously/synchronously writes to `.minicode/subagents/<id>/transcript.json`.
    pub fn store(&self, transcript: SubagentTranscript, workspace_root: Option<&Path>) {
        let id = transcript.subagent_id.clone();

        // Write to disk if workspace_root is provided
        if let Some(root) = workspace_root {
            let dir = root.join(".minicode").join("subagents").join(&id);
            if let Ok(()) = std::fs::create_dir_all(&dir) {
                let file_path = dir.join("transcript.json");
                if let Ok(serialized) = serde_json::to_string_pretty(&transcript) {
                    let _ = std::fs::write(&file_path, serialized);
                }
            }
        }

        // Store in memory
        let mut lock = self.cache.write().unwrap_or_else(|e| e.into_inner());
        lock.insert(id, transcript);
    }

    /// Retrieves a transcript from memory, falling back to disk cache if not in memory.
    pub fn get(
        &self,
        subagent_id: &str,
        workspace_root: Option<&Path>,
    ) -> Option<SubagentTranscript> {
        {
            let lock = self.cache.read().unwrap_or_else(|e| e.into_inner());
            if let Some(t) = lock.get(subagent_id) {
                return Some(t.clone());
            }
        }

        // Fallback: check disk
        if let Some(root) = workspace_root {
            let file_path = root
                .join(".minicode")
                .join("subagents")
                .join(subagent_id)
                .join("transcript.json");
            if file_path.exists() {
                if let Ok(data) = std::fs::read_to_string(&file_path) {
                    if let Ok(transcript) = serde_json::from_str::<SubagentTranscript>(&data) {
                        let mut lock = self.cache.write().unwrap_or_else(|e| e.into_inner());
                        lock.insert(subagent_id.to_string(), transcript.clone());
                        return Some(transcript);
                    }
                }
            }
        }

        None
    }

    /// Lists all subagent IDs currently cached in memory or on disk.
    pub fn list_subagent_ids(&self, workspace_root: Option<&Path>) -> Vec<String> {
        let mut ids = Vec::new();
        {
            let lock = self.cache.read().unwrap_or_else(|e| e.into_inner());
            ids.extend(lock.keys().cloned());
        }

        if let Some(root) = workspace_root {
            let dir = root.join(".minicode").join("subagents");
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if !ids.contains(&name) {
                                ids.push(name);
                            }
                        }
                    }
                }
            }
        }

        ids
    }
}

static GLOBAL_TRANSCRIPT_STORE: OnceLock<SubagentTranscriptStore> = OnceLock::new();

/// Returns the global singleton SubagentTranscriptStore.
pub fn get_global_transcript_store() -> &'static SubagentTranscriptStore {
    GLOBAL_TRANSCRIPT_STORE.get_or_init(SubagentTranscriptStore::new)
}
