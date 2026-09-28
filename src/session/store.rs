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
    fallback_dir: Option<PathBuf>,
    workspace_root: Option<PathBuf>,
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
        Self {
            sessions_dir,
            fallback_dir: None,
            workspace_root: None,
        }
    }

    /// Workspace-local session store.
    ///
    /// Strictly stores sessions under `<workspace>/.minicode/sessions/` — scoped
    /// entirely to the project. Ensures `.minicode/` is protected in `.gitignore`.
    /// Also migrates any legacy sessions for this workspace from `~/.config/minicode/sessions/`
    /// directly into `.minicode/sessions/`.
    pub fn with_workspace(workspace_root: &Path) -> Self {
        let sessions_dir = workspace_root
            .join(WORKSPACE_DIR_NAME)
            .join(SESSIONS_DIR_NAME);

        if let Err(e) = std::fs::create_dir_all(&sessions_dir) {
            tracing::warn!(path = %sessions_dir.display(), error = %e, "Failed to create sessions directory");
        }

        // Auto-ensure .minicode/ is ignored in git
        crate::tools::minikit::sync::MiniKitSyncEngine::ensure_gitignore(workspace_root);

        // One-time auto-migration: if any old sessions for this workspace exist in ~/.config/minicode/sessions,
        // move them cleanly into .minicode/sessions/ so all history lives strictly in the project.
        if let Some(global_dir) =
            dirs::config_dir().map(|c| c.join(CONFIG_DIR_NAME).join(SESSIONS_DIR_NAME))
        {
            if global_dir.exists() && global_dir != sessions_dir {
                Self::migrate_legacy_sessions(workspace_root, &sessions_dir, &global_dir);
            }
        }

        tracing::debug!(
            sessions_dir = %sessions_dir.display(),
            "Session store initialised"
        );
        Self {
            sessions_dir,
            fallback_dir: None,
            workspace_root: Some(workspace_root.to_path_buf()),
        }
    }

    /// Migrates legacy session files for `workspace_root` from `from_dir` into `to_dir`.
    pub fn migrate_legacy_sessions(workspace_root: &Path, to_dir: &Path, from_dir: &Path) {
        let entries = match std::fs::read_dir(from_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let ws_canonical =
            std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let file = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let mut reader = std::io::BufReader::new(file);
            let mut first_line = String::new();
            if std::io::BufRead::read_line(&mut reader, &mut first_line).is_ok() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&first_line) {
                    if let Some(session_meta) = v.get("session_meta") {
                        if let Some(ws_val) = session_meta.get("workspace").and_then(|w| w.as_str())
                        {
                            if is_workspace_match(ws_val, &ws_canonical) {
                                if let Some(file_name) = path.file_name() {
                                    let dest = to_dir.join(file_name);
                                    if !dest.exists() {
                                        if let Err(e) = std::fs::copy(&path, &dest) {
                                            tracing::warn!(src = %path.display(), dest = %dest.display(), error = %e, "Failed to migrate legacy session file");
                                            continue;
                                        }
                                    }
                                    let _ = std::fs::remove_file(&path);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Constructor with an explicit directory, used in tests.
    #[allow(dead_code)]
    pub fn with_dir(dir: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = %e, "Failed to create sessions directory");
        }
        Self {
            sessions_dir: dir,
            fallback_dir: None,
            workspace_root: None,
        }
    }

    /// Generates a new unique session ID without creating any file on disk.
    pub fn generate_session_id(&self) -> String {
        format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
            &uuid::Uuid::new_v4().to_string()[..8]
        )
    }

    /// Initializes the JSONL session file on disk for a given session ID.
    pub fn init_session_file(&self, session_id: &str, workspace: &Path) -> Result<PathBuf> {
        self.validate_session_id(session_id)?;
        let session_path = self.sessions_dir.join(format!("{}.jsonl", session_id));

        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&session_path)?;

        let meta = SessionMetadata {
            id: session_id.to_string(),
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
        file.flush()?;
        file.sync_all()?;

        tracing::info!(session_id = %session_id, "Initialized new session store");
        Ok(session_path)
    }

    /// Generates a new session ID and initializes the JSONL session file.
    pub fn create_session(&self, workspace: &Path) -> Result<String> {
        let session_id = self.generate_session_id();
        self.init_session_file(&session_id, workspace)?;
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

    /// Appends an AgentEvent to the session's JSONL file.
    pub fn append_event(&self, session_id: &str, event: &AgentEvent) -> Result<()> {
        self.validate_session_id(session_id)?;
        let session_path = self.session_file_path(session_id);

        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&session_path)?;

        let line = serde_json::to_string(event)?;
        writeln!(file, "{}", line)?;
        file.flush()?;
        file.sync_data()?;
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

    /// Returns the session ID of the most recent session with recorded events.
    pub fn get_last_session_id(&self) -> Option<String> {
        match self.list_sessions_rich() {
            Ok(sessions) => sessions
                .iter()
                .find(|s| s.event_count > 0)
                .or_else(|| sessions.first())
                .map(|s| s.id.clone()),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list sessions for latest session lookup");
                None
            }
        }
    }

    /// Lists all sessions (fast — only reads first line of each JSONL).
    pub fn list_sessions(&self) -> Result<Vec<SessionMetadata>> {
        let mut sessions = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // 1. Scan primary directory (.minicode/sessions)
        for s in scan_sessions_in_dir(&self.sessions_dir, None) {
            seen_ids.insert(s.id.clone());
            sessions.push(s);
        }

        // 2. Scan fallback directory (e.g. global ~/.config/minicode/sessions)
        if let Some(ref fb_dir) = self.fallback_dir {
            if fb_dir != &self.sessions_dir {
                for s in scan_sessions_in_dir(fb_dir, self.workspace_root.as_deref()) {
                    if !seen_ids.contains(&s.id) {
                        seen_ids.insert(s.id.clone());
                        sessions.push(s);
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }

    /// Lists sessions enriched with event_count and preview (reads more of each file).
    /// Used by the /sessions TUI modal.
    /// Filters out and cleans up empty placeholder 0-event sessions.
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

        // Clean up empty placeholder files from disk and filter them out
        sessions.retain(|meta| {
            if meta.event_count == 0 {
                let _ = std::fs::remove_file(&meta.path);
                tracing::debug!(path = %meta.path, "Pruned empty 0-event session file");
                false
            } else {
                true
            }
        });

        Ok(sessions)
    }

    pub fn session_file_path(&self, session_id: &str) -> PathBuf {
        let primary = self.sessions_dir.join(format!("{}.jsonl", session_id));
        if primary.exists() {
            return primary;
        }
        if let Some(ref fb) = self.fallback_dir {
            let fb_path = fb.join(format!("{}.jsonl", session_id));
            if fb_path.exists() {
                return fb_path;
            }
        }
        primary
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
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(ref fb) = self.fallback_dir {
                    let fb_path = fb.join(format!("{}.jsonl", session_id));
                    if std::fs::remove_file(&fb_path).is_ok() {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }
}

fn scan_sessions_in_dir(dir: &Path, workspace_filter: Option<&Path>) -> Vec<SessionMetadata> {
    let mut sessions = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return sessions,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            let mut meta_opt = None;
            if let Ok(file) = std::fs::File::open(&path) {
                let mut reader = BufReader::new(file);
                let mut first_line = String::new();
                if reader.read_line(&mut first_line).is_ok() {
                    let trimmed = first_line.trim();
                    if trimmed.starts_with("{\"session_meta\":") {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            if let Some(meta_val) = val.get("session_meta") {
                                if let Ok(mut meta) =
                                    serde_json::from_value::<SessionMetadata>(meta_val.clone())
                                {
                                    // Use the actual path where the file was discovered on disk
                                    meta.path = path.display().to_string();
                                    meta_opt = Some(meta);
                                }
                            }
                        }
                    }
                }
            }

            let meta = meta_opt.unwrap_or_else(|| {
                let id = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                SessionMetadata {
                    id,
                    created_at: String::new(),
                    workspace: String::new(),
                    path: path.display().to_string(),
                    event_count: 0,
                    preview: String::new(),
                }
            });

            if let Some(target_ws) = workspace_filter {
                if !meta.workspace.is_empty() && !is_workspace_match(&meta.workspace, target_ws) {
                    continue;
                }
            }

            sessions.push(meta);
        }
    }
    sessions
}

fn is_workspace_match(session_workspace: &str, current_workspace: &Path) -> bool {
    let sess_path = Path::new(session_workspace);
    if sess_path == current_workspace {
        return true;
    }
    if let (Ok(c1), Ok(c2)) = (sess_path.canonicalize(), current_workspace.canonicalize()) {
        if c1 == c2 {
            return true;
        }
    }
    let s1 = sess_path.to_string_lossy();
    let s2 = current_workspace.to_string_lossy();
    s1.trim_end_matches('/') == s2.trim_end_matches('/')
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
    crate::utils::strings::truncate_cols(s, max_cols, suffix)
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

    #[test]
    fn test_empty_sessions_are_pruned_and_filtered() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_prune_test_{}", uuid::Uuid::new_v4()));
        let store = SessionStore::with_dir(temp_dir.clone());

        // Create empty session (0 events)
        let empty_id = store.create_session(&temp_dir).unwrap();
        let empty_path = store.session_file_path(&empty_id);
        assert!(empty_path.exists());

        // Create active session (1 event)
        let active_id = store.create_session(&temp_dir).unwrap();
        let event = AgentEvent::UserPrompt {
            turn_id: 1,
            timestamp: chrono::Utc::now().to_rfc3339(),
            prompt: "Hello world".to_string(),
        };
        store.append_event(&active_id, &event).unwrap();

        // list_sessions_rich should prune the empty session from disk and return only active_id
        let rich = store.list_sessions_rich().unwrap();
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0].id, active_id);
        assert_eq!(rich[0].event_count, 1);
        assert_eq!(rich[0].preview, "Hello world");

        // Assert empty file was deleted from disk
        assert!(!empty_path.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_fallback_dir_discovers_old_sessions() {
        let base_dir =
            std::env::temp_dir().join(format!("minicode_fb_test_{}", uuid::Uuid::new_v4()));
        let ws_dir = base_dir.join("workspace");
        let minicode_dir = ws_dir.join(".minicode");
        let fallback_dir = base_dir.join("global_sessions");
        std::fs::create_dir_all(&minicode_dir).unwrap();
        std::fs::create_dir_all(&fallback_dir).unwrap();

        // Pre-populate a session in fallback_dir that belongs to ws_dir
        let old_id = "20260814T100000Z-old12345";
        let old_path = fallback_dir.join(format!("{}.jsonl", old_id));
        let meta = SessionMetadata {
            id: old_id.to_string(),
            created_at: "2026-08-14T10:00:00Z".to_string(),
            workspace: ws_dir.display().to_string(),
            path: old_path.display().to_string(),
            event_count: 0,
            preview: String::new(),
        };
        let meta_line =
            serde_json::to_string(&serde_json::json!({ "session_meta": meta })).unwrap();
        let event = AgentEvent::UserPrompt {
            turn_id: 1,
            timestamp: "2026-08-14T10:00:01Z".to_string(),
            prompt: "Old session prompt".to_string(),
        };
        let event_line = serde_json::to_string(&event).unwrap();
        std::fs::write(&old_path, format!("{}\n{}\n", meta_line, event_line)).unwrap();

        // Construct SessionStore with primary in minicode_dir and fallback_dir
        let mut store = SessionStore::with_workspace(&ws_dir);
        store.fallback_dir = Some(fallback_dir);

        let sessions = store.list_sessions_rich().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, old_id);
        assert_eq!(sessions[0].preview, "Old session prompt");

        // Verify load_session works seamlessly on fallback session
        let loaded = store.load_session(old_id).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], event);

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_migrate_legacy_sessions_moves_files_to_workspace() {
        let base_dir =
            std::env::temp_dir().join(format!("minicode_mig_test_{}", uuid::Uuid::new_v4()));
        let ws_dir = base_dir.join("workspace");
        let legacy_dir = base_dir.join("legacy_global");
        let dest_sessions_dir = ws_dir.join(".minicode").join("sessions");
        std::fs::create_dir_all(&ws_dir).unwrap();
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::create_dir_all(&dest_sessions_dir).unwrap();

        let old_id = "20260714T120000Z-migrated1";
        let old_path = legacy_dir.join(format!("{}.jsonl", old_id));
        let meta = SessionMetadata {
            id: old_id.to_string(),
            created_at: "2026-07-14T12:00:00Z".to_string(),
            workspace: ws_dir.display().to_string(),
            path: old_path.display().to_string(),
            event_count: 0,
            preview: String::new(),
        };
        let meta_line =
            serde_json::to_string(&serde_json::json!({ "session_meta": meta })).unwrap();
        let event = AgentEvent::UserPrompt {
            turn_id: 1,
            timestamp: "2026-07-14T12:00:01Z".to_string(),
            prompt: "Migrated prompt".to_string(),
        };
        let event_line = serde_json::to_string(&event).unwrap();
        std::fs::write(&old_path, format!("{}\n{}\n", meta_line, event_line)).unwrap();

        // Run migration
        SessionStore::migrate_legacy_sessions(&ws_dir, &dest_sessions_dir, &legacy_dir);

        // Old file in legacy_dir should be removed
        assert!(!old_path.exists());

        // New file in dest_sessions_dir should exist
        let migrated_path = dest_sessions_dir.join(format!("{}.jsonl", old_id));
        assert!(migrated_path.exists());

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
