use crate::agent::types::AgentEvent;
use crate::logging::runtime::ActiveSessionRecord;
use serde::Serialize;
use std::path::Path;

/// High-contrast, Hono-inspired semantic log formatter.
#[derive(Debug, Clone)]
pub struct HonoLogFormatter {
    pub no_color: bool,
}

impl Default for HonoLogFormatter {
    fn default() -> Self {
        let no_color_env = std::env::var("NO_COLOR").is_ok();
        Self {
            no_color: no_color_env,
        }
    }
}

impl HonoLogFormatter {
    pub fn new(no_color: bool) -> Self {
        let no_color_env = std::env::var("NO_COLOR").is_ok();
        Self {
            no_color: no_color || no_color_env,
        }
    }

    /// Renders an ANSI color code if coloring is enabled.
    fn color<'a>(&self, ansi: &'a str) -> &'a str {
        if self.no_color {
            ""
        } else {
            ansi
        }
    }

    fn reset(&self) -> &str {
        if self.no_color {
            ""
        } else {
            "\x1b[0m"
        }
    }

    /// Formats a session header banner for attached viewers.
    pub fn format_header(
        &self,
        session_id: &str,
        record: Option<&ActiveSessionRecord>,
        session_path: &Path,
    ) -> String {
        let bold_cyan = self.color("\x1b[1;38;2;130;226;255m");
        let dim = self.color("\x1b[90m");
        let green = self.color("\x1b[1;38;2;97;255;202m");
        let yellow = self.color("\x1b[1;38;2;255;202;133m");
        let reset = self.reset();

        let mut lines = Vec::new();
        lines.push(format!(
            "{}[minicode logs]{} Attaching to session {}{}{}",
            bold_cyan, reset, bold_cyan, session_id, reset
        ));

        if let Some(act) = record {
            lines.push(format!(
                "{}●{} Status    : {}ACTIVE{} (PID {})",
                green, reset, green, reset, act.pid
            ));
            lines.push(format!(
                "{}●{} Workspace : {}{}{}",
                dim, reset, dim, act.workspace, reset
            ));
            lines.push(format!(
                "{}●{} Provider  : {} ({})",
                dim, reset, act.provider, act.model
            ));
            lines.push(format!(
                "{}●{} Started   : {}{}{}",
                dim, reset, dim, act.started_at, reset
            ));
        } else {
            lines.push(format!(
                "{}○{} Status    : {}HISTORICAL / FINISHED{}",
                yellow, reset, yellow, reset
            ));
            lines.push(format!(
                "{}○{} File      : {}{}{}",
                dim,
                reset,
                dim,
                session_path.display(),
                reset
            ));
        }

        lines.push(format!(
            "{}──────────────────────────────────────────────────────────────────────────────{}",
            dim, reset
        ));
        lines.join("\n")
    }

    /// Formats an AgentEvent into a Hono-style server log line.
    /// Returns None if the event is a micro-delta that should not flood the log.
    pub fn format_event(&self, event: &AgentEvent) -> Option<String> {
        let dim = self.color("\x1b[90m");
        let bold_cyan = self.color("\x1b[1;38;2;130;226;255m");
        let arrow_in = if self.no_color {
            "-->"
        } else {
            "\x1b[38;2;130;226;255m-->\x1b[0m"
        };
        let arrow_out_ok = if self.no_color {
            "<--"
        } else {
            "\x1b[38;2;97;255;202m<--\x1b[0m"
        };
        let arrow_out_err = if self.no_color {
            "<--"
        } else {
            "\x1b[38;2;255;110;110m<--\x1b[0m"
        };
        let badge_ok = if self.no_color {
            "200"
        } else {
            "\x1b[1;38;2;97;255;202m200\x1b[0m"
        };
        let badge_err = if self.no_color {
            "500"
        } else {
            "\x1b[1;38;2;255;110;110m500\x1b[0m"
        };
        let badge_warn = if self.no_color {
            "400"
        } else {
            "\x1b[1;38;2;255;202;133m400\x1b[0m"
        };
        let magenta = self.color("\x1b[38;2;246;148;255m");
        let blue = self.color("\x1b[38;2;162;119;255m");
        let yellow = self.color("\x1b[38;2;255;202;133m");
        let reset = self.reset();

        match event {
            AgentEvent::UserPrompt {
                turn_id,
                timestamp,
                prompt,
            } => {
                let time = extract_time(timestamp);
                let snippet = summarize_string(prompt, 64);
                Some(format!(
                    "{}{}  {} {}USER{}   Turn #{}: \"{}\"",
                    dim, time, arrow_in, bold_cyan, reset, turn_id, snippet
                ))
            }

            AgentEvent::TurnStart {
                turn_id,
                timestamp,
                model,
                context_tokens,
            } => {
                let time = extract_time(timestamp);
                Some(format!(
                    "{}{}  {} {}LLM{}    {} [turn #{} • ctx {} tokens]",
                    dim, time, arrow_in, magenta, reset, model, turn_id, context_tokens
                ))
            }

            AgentEvent::ToolCall {
                tool_id: _,
                turn_id: _,
                tool,
                args,
            } => {
                let time = current_time_str();
                let compact_args = summarize_tool_args(tool, args);
                Some(format!(
                    "{}{}  {} {}TOOL{}   {} {}",
                    dim, time, arrow_in, bold_cyan, reset, tool, compact_args
                ))
            }

            AgentEvent::ToolResult {
                tool_id: _,
                turn_id: _,
                tool,
                success,
                output,
                duration_ms,
            } => {
                let time = current_time_str();
                let arrow = if *success {
                    arrow_out_ok
                } else {
                    arrow_out_err
                };
                let status_badge = if *success { badge_ok } else { badge_err };
                let summary = summarize_tool_output(tool, *success, output);
                let dur_str = format_duration(*duration_ms);

                Some(format!(
                    "{}{}  {} {}TOOL{}   {} {} {} [{}]",
                    dim, time, arrow, bold_cyan, reset, tool, status_badge, dur_str, summary
                ))
            }

            AgentEvent::ApprovalRequest {
                turn_id: _,
                tool_id: _,
                tool,
                args: _,
                reason,
            } => {
                let time = current_time_str();
                Some(format!(
                    "{}{}  {}???{} {}AUTH{}   {} [Waiting for approval: {}]",
                    dim, time, yellow, reset, yellow, reset, tool, reason
                ))
            }

            AgentEvent::FileModified {
                turn_id: _,
                path,
                action,
                backup: _,
            } => {
                let time = current_time_str();
                Some(format!(
                    "{}{}  {} {}FILE{}   {} {}",
                    dim, time, arrow_out_ok, blue, reset, action, path
                ))
            }

            AgentEvent::GitCommit {
                turn_id: _,
                hash,
                message,
                files: _,
            } => {
                let time = current_time_str();
                let short_hash = if hash.len() > 7 { &hash[..7] } else { hash };
                Some(format!(
                    "{}{}  {}=== GIT{}    commit {}: \"{}\"",
                    dim, time, blue, reset, short_hash, message
                ))
            }

            AgentEvent::TurnEnd {
                turn_id,
                status,
                total_tokens_used,
                files_modified,
            } => {
                let time = current_time_str();
                let is_ok = status == "complete" || status == "success";
                let arrow = if is_ok { arrow_out_ok } else { arrow_out_err };
                let badge = if is_ok {
                    badge_ok
                } else if status == "cancelled" {
                    badge_warn
                } else {
                    badge_err
                };
                let files_info = if files_modified.is_empty() {
                    "0 files modified".to_string()
                } else {
                    format!("{} files modified", files_modified.len())
                };

                Some(format!(
                    "{}{}  {} {}DONE{}   Turn #{} {} [{} tokens • {}]",
                    dim,
                    time,
                    arrow,
                    self.color("\x1b[1;38;2;97;255;202m"),
                    reset,
                    turn_id,
                    badge,
                    total_tokens_used,
                    files_info
                ))
            }

            AgentEvent::Error {
                turn_id: _,
                code,
                message,
                retrying,
                retry_after_ms: _,
            } => {
                let time = current_time_str();
                let retry_text = if *retrying { " (retrying...)" } else { "" };
                Some(format!(
                    "{}{}  {} {}ERR{}    {}: {}{}",
                    dim,
                    time,
                    arrow_out_err,
                    self.color("\x1b[1;38;2;255;110;110m"),
                    reset,
                    code,
                    message,
                    retry_text
                ))
            }

            AgentEvent::ContextCompacted {
                turn_id: _,
                tier,
                turns_summarized,
                tokens_before,
                tokens_after,
                savings_percent,
            } => {
                let time = current_time_str();
                Some(format!(
                    "{}{}  {}~~~ CMPT{}   Tier {} compacted ({} turns): {} -> {} tokens ({}% savings)",
                    dim, time, blue, reset, tier, turns_summarized, tokens_before, tokens_after, savings_percent
                ))
            }

            AgentEvent::IntentRouted {
                turn_id: _,
                intent,
                query: _,
                confidence,
                suggested_command,
            } => {
                let time = current_time_str();
                let cmd_str = suggested_command
                    .as_deref()
                    .map(|c| format!(" -> {}", c))
                    .unwrap_or_default();
                Some(format!(
                    "{}{}  {} {}ROUTE{}  {} ({}% conf){}",
                    dim,
                    time,
                    arrow_in,
                    blue,
                    reset,
                    intent,
                    (confidence * 100.0) as u32,
                    cmd_str
                ))
            }

            // Micro-deltas (individual streaming token chunks) and heartbeats are suppressed to prevent noisy flooding
            AgentEvent::StreamDelta { .. }
            | AgentEvent::Heartbeat { .. }
            | AgentEvent::CommandList { .. } => None,
        }
    }

    /// Formats an event into a machine-readable JSON object string for `--json` streaming.
    pub fn format_json(&self, event: &AgentEvent) -> Option<String> {
        #[derive(Serialize)]
        struct StructuredLogRecord<'a> {
            timestamp: String,
            event_type: &'a str,
            turn_id: Option<usize>,
            status_code: u16,
            summary: String,
            raw: &'a AgentEvent,
        }

        let (event_type, turn_id, status_code, summary) = match event {
            AgentEvent::UserPrompt {
                turn_id, prompt, ..
            } => (
                "user_prompt",
                Some(*turn_id),
                200,
                summarize_string(prompt, 80),
            ),
            AgentEvent::TurnStart { turn_id, model, .. } => {
                ("turn_start", Some(*turn_id), 200, format!("LLM {}", model))
            }
            AgentEvent::ToolCall {
                turn_id,
                tool,
                args,
                ..
            } => (
                "tool_call",
                Some(*turn_id),
                200,
                format!("{}: {}", tool, summarize_tool_args(tool, args)),
            ),
            AgentEvent::ToolResult {
                turn_id,
                tool,
                success,
                output,
                ..
            } => {
                let code = if *success { 200 } else { 500 };
                (
                    "tool_result",
                    Some(*turn_id),
                    code,
                    format!(
                        "{}: {}",
                        tool,
                        summarize_tool_output(tool, *success, output)
                    ),
                )
            }
            AgentEvent::ApprovalRequest {
                turn_id,
                tool,
                reason,
                ..
            } => (
                "approval_request",
                Some(*turn_id),
                400,
                format!("{}: {}", tool, reason),
            ),
            AgentEvent::FileModified {
                turn_id,
                path,
                action,
                ..
            } => (
                "file_modified",
                Some(*turn_id),
                200,
                format!("{}: {}", action, path),
            ),
            AgentEvent::GitCommit {
                turn_id,
                hash,
                message,
                ..
            } => (
                "git_commit",
                Some(*turn_id),
                200,
                format!("{}: {}", hash, message),
            ),
            AgentEvent::TurnEnd {
                turn_id, status, ..
            } => {
                let code = if status == "complete" || status == "success" {
                    200
                } else if status == "cancelled" {
                    499
                } else {
                    500
                };
                (
                    "turn_end",
                    Some(*turn_id),
                    code,
                    format!("turn {} {}", turn_id, status),
                )
            }
            AgentEvent::Error {
                turn_id,
                code,
                message,
                ..
            } => ("error", *turn_id, 500, format!("{}: {}", code, message)),
            AgentEvent::ContextCompacted {
                turn_id,
                tier,
                savings_percent,
                ..
            } => (
                "context_compacted",
                Some(*turn_id),
                200,
                format!("tier {} ({}% saved)", tier, savings_percent),
            ),
            AgentEvent::IntentRouted {
                turn_id, intent, ..
            } => ("intent_routed", *turn_id, 200, intent.clone()),
            AgentEvent::StreamDelta { .. }
            | AgentEvent::Heartbeat { .. }
            | AgentEvent::CommandList { .. } => return None,
        };

        let record = StructuredLogRecord {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type,
            turn_id,
            status_code,
            summary,
            raw: event,
        };

        serde_json::to_string(&record).ok()
    }
}

fn extract_time(timestamp: &str) -> String {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        parsed.format("%H:%M:%S").to_string()
    } else {
        current_time_str()
    }
}

fn current_time_str() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.2}s", ms as f64 / 1000.0)
    }
}

fn summarize_string(s: &str, max_len: usize) -> String {
    let single_line = s.replace('\n', " ").replace('\r', "");
    let trimmed = single_line.trim();
    if trimmed.chars().count() <= max_len {
        trimmed.to_string()
    } else {
        let truncated: String = trimmed.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

fn summarize_tool_args(_tool: &str, args: &serde_json::Value) -> String {
    if let Some(obj) = args.as_object() {
        if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
            return format!("{{ path: \"{}\" }}", path);
        }
        if let Some(cmd) = obj
            .get("command")
            .or_else(|| obj.get("cmd"))
            .and_then(|v| v.as_str())
        {
            return format!("{{ cmd: \"{}\" }}", summarize_string(cmd, 40));
        }
        if let Some(query) = obj.get("query").and_then(|v| v.as_str()) {
            return format!("{{ query: \"{}\" }}", summarize_string(query, 36));
        }
        if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
            return format!("{{ name: \"{}\" }}", name);
        }
    }

    let raw = args.to_string();
    summarize_string(&raw, 40)
}

fn summarize_tool_output(tool: &str, success: bool, output: &str) -> String {
    if !success {
        return summarize_string(output, 48);
    }

    match tool {
        "read_file" => {
            let line_count = output.lines().count();
            format!("{} lines", line_count)
        }
        "write_file" | "patch_file" => {
            let line_count = output.lines().count();
            format!("applied ({} lines modified)", line_count)
        }
        "exec_cmd" | "sandbox_exec" => {
            if output.contains("test result: ok") {
                "test passed".to_string()
            } else if output.trim().is_empty() {
                "exit 0".to_string()
            } else {
                summarize_string(output, 40)
            }
        }
        "list_dir" => {
            let count = output.lines().count();
            format!("{} entries", count)
        }
        _ => summarize_string(output, 40),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_user_prompt() {
        let formatter = HonoLogFormatter::new(true); // plain text
        let event = AgentEvent::UserPrompt {
            turn_id: 1,
            timestamp: "2026-09-10T11:22:50Z".to_string(),
            prompt: "Build an intro website for me".to_string(),
        };

        let formatted = formatter.format_event(&event).unwrap();
        assert!(formatted.contains("--> USER   Turn #1"));
        assert!(formatted.contains("Build an intro website for me"));
    }

    #[test]
    fn test_format_tool_result_status_codes() {
        let formatter = HonoLogFormatter::new(true);
        let ok_event = AgentEvent::ToolResult {
            tool_id: "t1".to_string(),
            turn_id: 1,
            tool: "read_file".to_string(),
            success: true,
            output: "line 1\nline 2\nline 3".to_string(),
            duration_ms: 12,
        };

        let formatted = formatter.format_event(&ok_event).unwrap();
        assert!(formatted.contains("<-- TOOL   read_file 200 12ms"));
        assert!(formatted.contains("3 lines"));

        let err_event = AgentEvent::ToolResult {
            tool_id: "t2".to_string(),
            turn_id: 1,
            tool: "exec_cmd".to_string(),
            success: false,
            output: "compile error in file".to_string(),
            duration_ms: 450,
        };

        let formatted_err = formatter.format_event(&err_event).unwrap();
        assert!(formatted_err.contains("<-- TOOL   exec_cmd 500 450ms"));
    }

    #[test]
    fn test_json_formatter() {
        let formatter = HonoLogFormatter::new(true);
        let event = AgentEvent::UserPrompt {
            turn_id: 2,
            timestamp: "2026-09-10T11:22:50Z".to_string(),
            prompt: "Run tests".to_string(),
        };

        let json_str = formatter.format_json(&event).unwrap();
        assert!(json_str.contains("\"event_type\":\"user_prompt\""));
        assert!(json_str.contains("\"turn_id\":2"));
    }
}
