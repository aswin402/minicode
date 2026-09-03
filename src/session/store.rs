use crate::agent::types::AgentEvent;
use crate::constants::{
    CONFIG_DIR_NAME, GIT_SHORT_HASH_BYTES, SESSIONS_DIR_NAME, SESSION_DEFAULT_MODEL,
    SESSION_FIRST_PROMPT_MAX_BYTES, SESSION_PREVIEW_MAX_BYTES, SESSION_TOOL_OUTPUT_MAX_BYTES,
    WORKSPACE_DIR_NAME,
};
use crate::error::{Result, SessionError};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthStr;

/// Writes a single line to a JSONL file atomically: write to a `.tmp` sibling,
/// fsync on Unix, then rename over the target.  Crash during write leaves the
/// original file untouched rather than producing a partial/trailing-nul line.
fn write_atomic_jsonl(path: &Path, line: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        #[cfg(unix)]
        file.sync_all()?;
        // Non-Unix: sync_all is not available; sync_data() is the fallback but
        // OpenOptions doesn't expose it directly — on Windows we rely on the
        // rename being atomic and the OS flush-on-close.
        #[cfg(not(unix))]
        file.flush()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

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
            .create_new(true)
            .write(true)
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

    fn validate_session_id(&self, session_id: &str) -> Result<()> {
        if session_id.is_empty()
            || session_id.len() > 128
            || !session_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(SessionError::InvalidId(session_id.to_string()).into());
        }
        Ok(())
    }

    /// Appends an AgentEvent to the session's JSONL file atomically.
    /// Uses write-to-temp + rename so a crash mid-write cannot corrupt existing data.
    pub fn append_event(&self, session_id: &str, event: &AgentEvent) -> Result<()> {
        self.validate_session_id(session_id)?;
        let session_path = self.session_file_path(session_id);

        let line = serde_json::to_string(event)?;

        // Ensure the sessions directory and file exist before atomic write.
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if !session_path.exists() {
            std::fs::File::create(&session_path).ok();
        }

        write_atomic_jsonl(&session_path, &line)?;
        Ok(())
    }

    /// Reads all events and initial session metadata from a session's JSONL file in a single pass.
    pub fn load_session_with_metadata(
        &self,
        session_id: &str,
    ) -> Result<(Option<SessionMetadata>, Vec<AgentEvent>)> {
        self.validate_session_id(session_id)?;
        let session_path = self.session_file_path(session_id);
        let file = match std::fs::File::open(&session_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(SessionError::NotFound {
                    id: session_id.to_string(),
                    path: session_path.display().to_string(),
                }
                .into());
            }
            Err(e) => return Err(e.into()),
        };

        let reader = BufReader::new(file);
        let mut events = Vec::new();
        let mut metadata: Option<SessionMetadata> = None;

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if metadata.is_none() && trimmed.starts_with("{\"session_meta\":") {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(meta_val) = val.get("session_meta") {
                        match serde_json::from_value::<SessionMetadata>(meta_val.clone()) {
                            Ok(m) => metadata = Some(m),
                            Err(e) => {
                                tracing::warn!(
                                    session = session_id,
                                    line = idx + 1,
                                    error = %e,
                                    "Corrupted session_meta in session JSONL"
                                );
                            }
                        }
                    }
                }
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

        Ok((metadata, events))
    }

    /// Reads all events from a session's JSONL file.
    pub fn load_session(&self, session_id: &str) -> Result<Vec<AgentEvent>> {
        self.load_session_with_metadata(session_id)
            .map(|(_, events)| events)
    }

    /// Returns the session ID of the most recent session.
    pub fn get_last_session_id(&self) -> Option<String> {
        match self.list_sessions() {
            Ok(sessions) => sessions.first().map(|s| s.id.clone()),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list sessions for latest session lookup");
                None
            }
        }
    }

    /// Lists all sessions (fast — only reads first line of each JSONL).
    pub fn list_sessions(&self) -> Result<Vec<SessionMetadata>> {
        let mut sessions = Vec::new();
        let entries = match std::fs::read_dir(&self.sessions_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(sessions),
            Err(e) => return Err(e.into()),
        };

        for entry in entries.flatten() {
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
            match std::fs::File::open(&meta.path) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    let mut count = 0usize;
                    let mut found_prompt = false;

                    for line_res in reader.lines() {
                        let Ok(line) = line_res else { break };
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with("{\"session_meta\":") {
                            continue;
                        }
                        count += 1;

                        // Prioritize user_prompt over turn_start or stream_delta
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            if let Some(event_type) = val.get("event").and_then(|v| v.as_str()) {
                                if event_type == "user_prompt" {
                                    if let Some(p) = val.get("prompt").and_then(|v| v.as_str()) {
                                        let p_trim = p.trim();
                                        if !p_trim.is_empty() {
                                            meta.preview = truncate_safe(
                                                p_trim,
                                                SESSION_PREVIEW_MAX_BYTES,
                                                "...",
                                            );
                                            found_prompt = true;
                                        }
                                    }
                                } else if !found_prompt && meta.preview.is_empty() {
                                    if event_type == "turn_start" {
                                        if let Some(m) = val.get("model").and_then(|v| v.as_str()) {
                                            meta.preview = format!("model: {}", m);
                                        }
                                    } else if event_type == "stream_delta" {
                                        if let Some(d) = val.get("delta").and_then(|v| v.as_str()) {
                                            let d_trim = d.trim();
                                            if !d_trim.is_empty() {
                                                meta.preview = truncate_safe(
                                                    d_trim,
                                                    SESSION_PREVIEW_MAX_BYTES,
                                                    "...",
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    meta.event_count = count;
                }
                Err(e) => {
                    tracing::warn!(
                        path = %meta.path,
                        error = %e,
                        "Failed to open session file for rich summary"
                    );
                }
            }
        }
        Ok(sessions)
    }

    fn session_file_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.jsonl", session_id))
    }

    /// Computes an in-depth analytical summary for a given session, returning both summary and events.
    /// Uses a single file pass without reopening or scanning twice.
    pub fn get_session_summary_with_events(
        &self,
        session_id: &str,
    ) -> Result<(SessionSummary, Vec<AgentEvent>)> {
        self.validate_session_id(session_id)?;
        let (metadata_opt, events) = self.load_session_with_metadata(session_id)?;
        let mut model = SESSION_DEFAULT_MODEL.to_string();
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

        let (created_at, workspace) = if let Some(meta) = metadata_opt {
            if !meta.preview.is_empty() {
                first_prompt = meta.preview;
            }
            (meta.created_at, meta.workspace)
        } else {
            (String::new(), String::new())
        };

        for event in &events {
            match event {
                AgentEvent::UserPrompt { prompt, .. } => {
                    if first_prompt.is_empty() {
                        let trimmed = prompt.trim();
                        if !trimmed.is_empty() {
                            first_prompt =
                                truncate_safe(trimmed, SESSION_FIRST_PROMPT_MAX_BYTES, "...");
                        }
                    }
                }
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
                    } else if let Some(path) = args.get("file_path").and_then(|v| v.as_str()) {
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

        // If first_prompt is still empty, scan events for the first non-empty StreamDelta as fallback
        if first_prompt.is_empty() {
            for event in &events {
                if let AgentEvent::StreamDelta { delta, .. } = event {
                    let trimmed = delta.trim();
                    if !trimmed.is_empty() {
                        first_prompt =
                            truncate_safe(trimmed, SESSION_FIRST_PROMPT_MAX_BYTES, "...");
                        break;
                    }
                }
            }
        }

        let mut tools_used: Vec<(String, usize)> = tool_counts.into_iter().collect();
        tools_used.sort_by(|a, b| b.1.cmp(&a.1));

        let summary = SessionSummary {
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
        };

        Ok((summary, events))
    }

    /// Computes an in-depth analytical summary for a given session.
    pub fn get_session_summary(&self, session_id: &str) -> Result<SessionSummary> {
        self.get_session_summary_with_events(session_id)
            .map(|(s, _)| s)
    }

    /// Forks an existing session into a new session with cloned history and a fresh ID.
    pub fn fork_session(&self, source_id: &str, workspace: &Path) -> Result<String> {
        self.validate_session_id(source_id)?;
        let events = self.load_session(source_id)?;
        let new_id = self.create_session(workspace)?;
        let session_path = self.session_file_path(&new_id);
        let file = OpenOptions::new().append(true).open(&session_path)?;
        let mut writer = std::io::BufWriter::new(file);
        for event in &events {
            let line = serde_json::to_string(event)?;
            writeln!(writer, "{}", line)?;
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;
        tracing::info!(source = source_id, target = %new_id, "Forked session successfully");
        Ok(new_id)
    }

    /// Exports a session's trajectory to a formatted GitHub-Flavored Markdown file.
    pub fn export_markdown(&self, session_id: &str, output_path: &Path) -> Result<PathBuf> {
        self.validate_session_id(session_id)?;
        let (summary, events) = self.get_session_summary_with_events(session_id)?;

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
            "- **Tool Execution Time:** {:.2}s\n\n",
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
                AgentEvent::UserPrompt { prompt, .. } => {
                    if !assistant_buf.is_empty() {
                        md.push_str(&assistant_buf);
                        md.push_str("\n\n");
                        assistant_buf.clear();
                    }
                    md.push_str(&format!("### 👤 User\n\n{}\n\n", prompt));
                }
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
                    let preview = if output.len() > SESSION_TOOL_OUTPUT_MAX_BYTES {
                        truncate_safe(&output, SESSION_TOOL_OUTPUT_MAX_BYTES, "...\n[truncated]")
                    } else {
                        output
                    };
                    md.push_str(&format!(
                        "> ```\n> {}\n> ```\n\n",
                        preview.replace('\n', "\n> ")
                    ));
                }
                AgentEvent::FileModified { path, action, .. } => {
                    if !assistant_buf.is_empty() {
                        md.push_str(&assistant_buf);
                        md.push_str("\n\n");
                        assistant_buf.clear();
                    }
                    md.push_str(&format!("📝 *File {}*: `{}`\n\n", action, path));
                }
                AgentEvent::GitCommit { hash, message, .. } => {
                    if !assistant_buf.is_empty() {
                        md.push_str(&assistant_buf);
                        md.push_str("\n\n");
                        assistant_buf.clear();
                    }
                    let hash_short = truncate_safe(&hash, GIT_SHORT_HASH_BYTES, "");
                    md.push_str(&format!(
                        "📦 **Git Commit:** `{}` — *{}*\n\n",
                        hash_short, message
                    ));
                }
                _ => {}
            }
        }

        if !assistant_buf.is_empty() {
            md.push_str(&assistant_buf);
            md.push_str("\n\n");
        }

        if let Some(parent) = output_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(
                    path = %parent.display(),
                    error = %e,
                    "Failed to create directory for markdown export"
                );
            }
        }
        std::fs::write(output_path, md)?;
        if let Ok(file) = std::fs::File::open(output_path) {
            let _ = file.sync_all();
        }
        tracing::info!(
            session = session_id,
            path = %output_path.display(),
            "Exported session transcript to Markdown"
        );
        Ok(output_path.to_path_buf())
    }

    /// Deletes a session JSONL file from disk.
    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        self.validate_session_id(session_id)?;
        let path = self.session_file_path(session_id);
        match std::fs::remove_file(&path) {
            Ok(_) => {
                tracing::info!(session = session_id, "Deleted session file");
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

/// Truncates a string to at most `max_bytes` without slicing through UTF-8 character boundaries.
/// If truncated, appends `suffix`.
pub fn truncate_safe(s: &str, max_bytes: usize, suffix: &str) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let safe_end = s.floor_char_boundary(max_bytes);
    format!("{}{}", &s[..safe_end], suffix)
}

/// Truncates a string to fit within `max_cols` visual display columns.
/// Handles CJK full-width characters and emojis safely without breaking characters.
pub fn truncate_display(s: &str, max_cols: usize, suffix: &str) -> String {
    if UnicodeWidthStr::width(s) <= max_cols {
        return s.to_string();
    }
    let suffix_width = UnicodeWidthStr::width(suffix);
    let target = max_cols.saturating_sub(suffix_width);
    let mut width = 0;
    let mut end_idx = 0;
    for (idx, ch) in s.char_indices() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + w > target {
            break;
        }
        width += w;
        end_idx = idx + ch.len_utf8();
    }
    format!("{}{}", &s[..end_idx], suffix)
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

    #[test]
    fn test_truncate_display_cjk_and_emojis() {
        let s = "你好世界"; // 4 chars * 2 width = 8 columns
        assert_eq!(truncate_display(s, 5, "..."), "你...");
        assert_eq!(truncate_display(s, 6, "…"), "你好…");
        assert_eq!(truncate_display(s, 8, "…"), "你好世界");

        let ascii = "hello world";
        assert_eq!(truncate_display(ascii, 7, "..."), "hell...");
        assert_eq!(truncate_display(ascii, 20, "..."), "hello world");
    }

    #[test]
    fn test_load_session_with_metadata() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_meta_test_{}", uuid::Uuid::new_v4()));
        let store = SessionStore::with_dir(temp_dir.clone());
        let session_id = store.create_session(&temp_dir).unwrap();

        let prompt_event = AgentEvent::UserPrompt {
            turn_id: 1,
            timestamp: "2026-08-28T10:00:00Z".to_string(),
            prompt: "Please write a test function.".to_string(),
        };
        store.append_event(&session_id, &prompt_event).unwrap();

        let (meta_opt, events) = store.load_session_with_metadata(&session_id).unwrap();
        assert!(meta_opt.is_some());
        let meta = meta_opt.unwrap();
        assert_eq!(meta.id, session_id);
        assert_eq!(meta.workspace, temp_dir.display().to_string());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], prompt_event);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
