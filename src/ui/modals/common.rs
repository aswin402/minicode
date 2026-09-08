//! Shared helper types and utilities for TUI modals.

pub use crate::ui::layout_utils::compute_scroll_offset;

#[derive(Debug, Clone)]
pub struct TurnCheckpointInfo {
    pub turn_id: usize,
    pub prompt: String,
    #[allow(dead_code)]
    pub timestamp: String,
    pub time_ago: String,
    pub files: Vec<String>,
    pub is_latest: bool,
}

pub fn format_time_ago(rfc3339_ts: &str) -> String {
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(rfc3339_ts) {
        let now = chrono::Utc::now();
        let diff = now.signed_duration_since(ts.with_timezone(&chrono::Utc));
        let secs = diff.num_seconds();
        if secs < 60 {
            format!("{}s ago", secs.max(1))
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else if secs < 86400 {
            format!("{}h ago", secs / 3600)
        } else {
            format!("{}d ago", secs / 86400)
        }
    } else {
        "recently".to_string()
    }
}
