use crate::agent::types::AgentEvent;
use crate::constants::{CONFIG_DIR_NAME, SESSIONS_DIR_NAME, WORKSPACE_DIR_NAME};
use crate::error::{Result, SessionError};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub created_at: String,
    pub workspace: String,
    pub path: String,
    /// Approximate number of events in this session (populated lazily by list_sessions_rich)
    #[serde(default)]
    pub event_count: usize,
    /// Short preview of the first user message (populated lazily)
    #[serde(default)]
    pub preview: String,
}

pub struct SessionStore {
    sessions_dir: PathBuf,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    /// Global session store — uses `~/.config/minicode/sessions/`.
    pub fn new() -> Self {
        let sessions_dir = if let Some(config_dir) = dirs::config_dir() {
            config_dir.join(CONFIG_DIR_NAME).join(SESSIONS_DIR_NAME)
        } else {
            PathBuf::from(WORKSPACE_DIR_NAME).join(SESSIONS_DIR_NAME)
        };
        if let Err(e) = std::fs::create_dir_all(&sessions_dir) {
            tracing::warn!(path = %sessions_dir.display(), error = %e, "Failed to create sessions directory");
        }
        Self { sessions_dir }
    }

    /// Workspace-local session store.
    ///
    /// If `<workspace>/.minicode/` exists, sessions are stored under
    /// `<workspace>/.minicode/sessions/` — scoped to the project, like Codex / Claude Code.
    /// Falls back to the global `~/.config/minicode/sessions/` otherwise.
    pub fn with_workspace(workspace_root: &Path) -> Self {
        let minicode_dir = workspace_root.join(WORKSPACE_DIR_NAME);
        let sessions_dir = if minicode_dir.exists() {
            minicode_dir.join(SESSIONS_DIR_NAME)
        } else if let Some(config_dir) = dirs::config_dir() {
            config_dir.join(CONFIG_DIR_NAME).join(SESSIONS_DIR_NAME)
        } else {
            PathBuf::from(WORKSPACE_DIR_NAME).join(SESSIONS_DIR_NAME)
        };

        if let Err(e) = std::fs::create_dir_all(&sessions_dir) {
            tracing::warn!(path = %sessions_dir.display(), error = %e, "Failed to create sessions directory");
        }
        tracing::debug!(sessions_dir = %sessions_dir.display(), "Session store initialised");
        Self { sessions_dir }
    }

    /// Constructor with an explicit directory, used in tests.
    #[allow(dead_code)]
    pub fn with_dir(dir: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = %e, "Failed to create sessions directory");
        }
        Self { sessions_dir: dir }
    }

    /// Generates a new session ID and initializes the JSONL session file.
    pub fn create_session(&self, workspace: &Path) -> Result<String> {
        let session_id = format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        let session_path = self.session_file_path(&session_id);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&session_path)?;

        let meta = SessionMetadata {
            id: session_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            workspace: workspace.display().to_string(),
            path: session_path.display().to_string(),
            event_count: 0,
            preview: String::new(),
        };

        let meta_line = serde_json::to_string(&serde_json::json!({
            "session_meta": meta
        }))?;
        writeln!(file, "{}", meta_line)?;
        file.sync_all()?;

        tracing::info!(session_id = %session_id, "Initialized new session store");
        Ok(session_id)
    }

    /// Appends an AgentEvent to the session's JSONL file in O(1) time.
    pub fn append_event(&self, session_id: &str, event: &AgentEvent) -> Result<()> {
        let session_path = self.session_file_path(session_id);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&session_path)?;

        let line = serde_json::to_string(event)?;
        writeln!(file, "{}", line)?;
        file.sync_all()?;
        Ok(())
    }

    /// Reads all events from a session's JSONL file.
    pub fn load_session(&self, session_id: &str) -> Result<Vec<AgentEvent>> {
        let session_path = self.session_file_path(session_id);
        if !session_path.exists() {
            return Err(SessionError::NotFound {
                id: session_id.to_string(),
                path: session_path.display().to_string(),
            }
            .into());
        }

        let file = std::fs::File::open(&session_path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("{\"session_meta\":") {
                continue;
            }

            match serde_json::from_str::<AgentEvent>(trimmed) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!(
                        session = session_id,
                        line = idx + 1,
                        error = %e,
                        "Skipping corrupted line in session JSONL"
                    );
                }
            }
        }

        Ok(events)
    }

    /// Returns the session ID of the most recent session.
    pub fn get_last_session_id(&self) -> Option<String> {
        match self.list_sessions() {
            Ok(mut sessions) => {
                sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                sessions.first().map(|s| s.id.clone())
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list sessions for latest session lookup");
                None
            }
        }
    }

    /// Lists all sessions (fast — only reads first line of each JSONL).
    pub fn list_sessions(&self) -> Result<Vec<SessionMetadata>> {
        let mut sessions = Vec::new();
        if !self.sessions_dir.exists() {
            return Ok(sessions);
        }

        for entry in std::fs::read_dir(&self.sessions_dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                if let Ok(file) = std::fs::File::open(&path) {
                    let mut reader = BufReader::new(file);
                    let mut first_line = String::new();
                    if reader.read_line(&mut first_line).is_ok() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&first_line) {
                            if let Some(meta_val) = val.get("session_meta") {
                                if let Ok(meta) =
                                    serde_json::from_value::<SessionMetadata>(meta_val.clone())
                                {
                                    sessions.push(meta);
                                    continue;
                                }
                            }
                        }
                    }
                }

                // Fallback metadata if first line wasn't session_meta
                let id = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                sessions.push(SessionMetadata {
                    id,
                    created_at: String::new(),
                    workspace: String::new(),
                    path: path.display().to_string(),
                    event_count: 0,
                    preview: String::new(),
                });
            }
        }

        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }

    /// Lists sessions enriched with event_count and preview (reads more of each file).
    /// Used by the /sessions TUI modal.
    pub fn list_sessions_rich(&self) -> Result<Vec<SessionMetadata>> {
        let mut sessions = self.list_sessions()?;
        for meta in &mut sessions {
            if let Ok(file) = std::fs::File::open(&meta.path) {
                let reader = BufReader::new(file);
                let mut count = 0usize;
                for line_res in reader.lines() {
                    let Ok(line) = line_res else { break };
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with("{\"session_meta\":") {
                        continue;
                    }
                    count += 1;
                    // Extract the first TurnStart prompt as preview
                    if meta.preview.is_empty() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            if val.get("TurnStart").is_some() {
                                // Will be filled from subsequent Prompt event
                            }
                            // Look for a StreamDelta containing user content or a prompt marker
                            if let Some(obj) = val.as_object() {
                                if obj.contains_key("TurnStart") {
                                    if let Some(m) = obj
                                        .get("TurnStart")
                                        .and_then(|v| v.get("model"))
                                        .and_then(|v| v.as_str())
                                    {
                                        meta.preview = format!("model: {}", m);
                                    }
                                }
                            }
                        }
                    }
                }
                meta.event_count = count;
            }
        }
        Ok(sessions)
    }

    fn session_file_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.jsonl", session_id))
    }

    /// Computes an in-depth analytical summary for a given session.
    pub fn get_session_summary(&self, session_id: &str) -> Result<SessionSummary> {
        let events = self.load_session(session_id)?;
        let mut model = "unknown".to_string();
        let mut total_turns = 0usize;
        let total_events = events.len();
        let mut total_tokens = 0usize;
        let mut total_duration_ms = 0u64;
        let mut first_prompt = String::new();
        let mut last_response = String::new();
        let mut current_turn_response = String::new();
        let mut tool_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut files_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        for event in &events {
            match event {
                AgentEvent::TurnStart {
                    model: m, turn_id, ..
                } => {
                    model = m.clone();
                    total_turns = total_turns.max(*turn_id);
                    if !current_turn_response.is_empty() {
                        last_response = current_turn_response.clone();
                        current_turn_response.clear();
                    }
                }
                AgentEvent::StreamDelta { delta, .. } => {
                    current_turn_response.push_str(delta);
                }
                AgentEvent::ToolCall { tool, args, .. } => {
                    *tool_counts.entry(tool.clone()).or_insert(0) += 1;
                    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                        files_set.insert(path.to_string());
                    } else if let Some(path) = args.get("target_path").and_then(|v| v.as_str()) {
                        files_set.insert(path.to_string());
                    }
                }
                AgentEvent::ToolResult { duration_ms, .. } => {
                    total_duration_ms += duration_ms;
                }
                AgentEvent::FileModified { path, .. } => {
                    files_set.insert(path.clone());
                }
                AgentEvent::TurnEnd {
                    total_tokens_used,
                    files_modified,
                    ..
                } => {
                    total_tokens += total_tokens_used;
                    for f in files_modified {
                        files_set.insert(f.clone());
                    }
                }
                _ => {}
            }
        }

        if !current_turn_response.is_empty() {
            last_response = current_turn_response;
        }

        // Also check if we have the session metadata for creation date & workspace
        let mut created_at = String::new();
        let mut workspace = String::new();
        let session_path = self.session_file_path(session_id);
        if let Ok(file) = std::fs::File::open(&session_path) {
            let mut reader = BufReader::new(file);
            let mut first_line = String::new();
            if reader.read_line(&mut first_line).is_ok() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&first_line) {
                    if let Some(meta_val) = val.get("session_meta") {
                        if let Ok(meta) =
                            serde_json::from_value::<SessionMetadata>(meta_val.clone())
                        {
                            created_at = meta.created_at;
                            workspace = meta.workspace;
                            first_prompt = meta.preview;
                        }
                    }
                }
            }
        }

        let mut tools_used: Vec<(String, usize)> = tool_counts.into_iter().collect();
        tools_used.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(SessionSummary {
            id: session_id.to_string(),
            created_at,
            workspace,
            model,
            total_turns,
            total_events,
            total_tokens,
            total_duration_ms,
            first_prompt,
            last_response,
            tools_used,
            files_touched: files_set.into_iter().collect(),
        })
    }

    /// Forks an existing session into a new session with cloned history and a fresh ID.
    pub fn fork_session(&self, source_id: &str, workspace: &Path) -> Result<String> {
        let events = self.load_session(source_id)?;
        let new_id = self.create_session(workspace)?;
        for event in &events {
            self.append_event(&new_id, event)?;
        }
        tracing::info!(source = source_id, target = %new_id, "Forked session successfully");
        Ok(new_id)
    }

    /// Exports a session's trajectory to a formatted GitHub-Flavored Markdown file.
    pub fn export_markdown(&self, session_id: &str, output_path: &Path) -> Result<PathBuf> {
        let summary = self.get_session_summary(session_id)?;
        let events = self.load_session(session_id)?;

        let mut md = String::new();
        md.push_str(&format!(
            "# minicode Session Transcript — `{}`\n\n",
            session_id
        ));
        md.push_str(&format!("- **Created:** {}\n", summary.created_at));
        md.push_str(&format!("- **Workspace:** `{}`\n", summary.workspace));
        md.push_str(&format!("- **Model:** `{}`\n", summary.model));
        md.push_str(&format!("- **Total Turns:** {}\n", summary.total_turns));
        md.push_str(&format!(
            "- **Events Recorded:** {}\n",
            summary.total_events
        ));
        md.push_str(&format!(
            "- **Tokens Consumed:** ~{}\n",
            summary.total_tokens
        ));
        md.push_str(&format!(
            "- **Duration:** {:.2}s\n\n",
            summary.total_duration_ms as f64 / 1000.0
        ));

        if !summary.tools_used.is_empty() {
            md.push_str("### 🛠️ Tools Invoked\n");
            for (tool, count) in &summary.tools_used {
                md.push_str(&format!("- **`{}`**: {} call(s)\n", tool, count));
            }
            md.push('\n');
        }

        if !summary.files_touched.is_empty() {
            md.push_str("### 📁 Files Touched\n");
            for file in &summary.files_touched {
                md.push_str(&format!("- `{}`\n", file));
            }
            md.push('\n');
        }

        md.push_str("---\n\n## 📜 Conversation Timeline\n\n");

        let mut assistant_buf = String::new();

        for event in events {
            match event {
                AgentEvent::TurnStart {
                    turn_id,
                    timestamp,
                    model,
                    ..
                } => {
                    if !assistant_buf.is_empty() {
                        md.push_str(&assistant_buf);
                        md.push_str("\n\n");
                        assistant_buf.clear();
                    }
                    md.push_str(&format!(
                        "### 🎯 Turn {} (`{}` — {})\n\n",
                        turn_id, model, timestamp
                    ));
                }
                AgentEvent::StreamDelta { delta, .. } => {
                    assistant_buf.push_str(&delta);
                }
                AgentEvent::ToolCall { tool, args, .. } => {
                    if !assistant_buf.is_empty() {
                        md.push_str(&assistant_buf);
                        md.push_str("\n\n");
                        assistant_buf.clear();
                    }
                    md.push_str(&format!("> **Tool Call:** `{}`\n", tool));
                    md.push_str(&format!(
                        "> ```json\n> {}\n> ```\n\n",
                        serde_json::to_string_pretty(&args)
                            .unwrap_or_default()
                            .replace('\n', "\n> ")
                    ));
                }
                AgentEvent::ToolResult {
                    tool,
                    success,
                    output,
                    duration_ms,
                    ..
                } => {
                    let status = if success { "✔ Success" } else { "✗ Failed" };
                    md.push_str(&format!(
                        "> **Tool Result (`{}` — {} in {}ms):**\n",
                        tool, status, duration_ms
                    ));
                    let preview = if output.len() > 1000 {
                        format!("{}...\n[truncated]", &output[..1000])
                    } else {
                        output
                    };
                    md.push_str(&format!(
                        "> ```\n> {}\n> ```\n\n",
                        preview.replace('\n', "\n> ")
                    ));
                }
                AgentEvent::FileModified { path, action, .. } => {
                    md.push_str(&format!("📝 *File {}*: `{}`\n\n", action, path));
                }
                AgentEvent::GitCommit { hash, message, .. } => {
                    md.push_str(&format!(
                        "📦 **Git Commit:** `{}` — *{}*\n\n",
                        &hash[..7.min(hash.len())],
                        message
                    ));
                }
                _ => {}
            }
        }

        if !assistant_buf.is_empty() {
            md.push_str(&assistant_buf);
            md.push_str("\n\n");
        }

        std::fs::write(output_path, md)?;
        tracing::info!(
            session = session_id,
            path = %output_path.display(),
            "Exported session transcript to Markdown"
        );
        Ok(output_path.to_path_buf())
    }

    /// Deletes a session JSONL file from disk.
    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        let path = self.session_file_path(session_id);
        if path.exists() {
            std::fs::remove_file(&path)?;
            tracing::info!(session = session_id, "Deleted session file");
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Analytical summary of a completed or active conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: String,
    pub workspace: String,
    pub model: String,
    pub total_turns: usize,
    pub total_events: usize,
    pub total_tokens: usize,
    pub total_duration_ms: u64,
    pub first_prompt: String,
    pub last_response: String,
    pub tools_used: Vec<(String, usize)>,
    pub files_touched: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_store_lifecycle() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_store_test_{}", uuid::Uuid::new_v4()));
        let store = SessionStore::with_dir(temp_dir.clone());

        let session_id = store.create_session(&temp_dir).unwrap();
        assert!(!session_id.is_empty());

        let event = AgentEvent::TurnStart {
            turn_id: 1,
            timestamp: chrono::Utc::now().to_rfc3339(),
            model: "gemini-2.5-pro".to_string(),
            context_tokens: 500,
        };
        store.append_event(&session_id, &event).unwrap();

        let loaded = store.load_session(&session_id).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], event);

        let last_id = store.get_last_session_id();
        assert_eq!(last_id, Some(session_id));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_with_workspace_uses_minicode_dir() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_ws_test_{}", uuid::Uuid::new_v4()));
        let minicode_dir = temp_dir.join(".minicode");
        std::fs::create_dir_all(&minicode_dir).unwrap();

        let store = SessionStore::with_workspace(&temp_dir);
        let session_id = store.create_session(&temp_dir).unwrap();
        assert!(!session_id.is_empty());

        // Session file should live inside .minicode/sessions/
        let expected_sessions_dir = minicode_dir.join("sessions");
        assert!(expected_sessions_dir.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_list_sessions_rich_returns_event_count() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_rich_test_{}", uuid::Uuid::new_v4()));
        let store = SessionStore::with_dir(temp_dir.clone());
        let session_id = store.create_session(&temp_dir).unwrap();

        let event = AgentEvent::TurnStart {
            turn_id: 1,
            timestamp: chrono::Utc::now().to_rfc3339(),
            model: "gemini-2.5-pro".to_string(),
            context_tokens: 500,
        };
        store.append_event(&session_id, &event).unwrap();
        store.append_event(&session_id, &event).unwrap();

        let rich = store.list_sessions_rich().unwrap();
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0].event_count, 2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
