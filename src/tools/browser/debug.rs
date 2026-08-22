use crate::constants::BROWSER_MAX_CONSOLE_LINES;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Severity level of captured console messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// Recorded browser console log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub level: LogLevel,
    pub text: String,
    pub timestamp: String,
}

/// Recorded HTTP network error or failure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkErrorEntry {
    pub method: String,
    pub url: String,
    pub status: u16,
    pub error_text: Option<String>,
}

/// Collector holding runtime console logs and failed network requests
#[derive(Debug, Clone, Default)]
pub struct DebugCollector {
    console_logs: Arc<Mutex<Vec<ConsoleEntry>>>,
    network_errors: Arc<Mutex<Vec<NetworkErrorEntry>>>,
}

impl DebugCollector {
    pub fn new() -> Self {
        Self {
            console_logs: Arc::new(Mutex::new(Vec::new())),
            network_errors: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Records a console message
    pub fn record_console(&self, level: LogLevel, text: &str) {
        let now = chrono::Local::now().format("%H:%M:%S").to_string();
        let mut logs = self.console_logs.lock().unwrap_or_else(|e| e.into_inner());

        if logs.len() >= BROWSER_MAX_CONSOLE_LINES {
            logs.remove(0); // Evict oldest
        }

        logs.push(ConsoleEntry {
            level,
            text: text.to_string(),
            timestamp: now,
        });
    }

    /// Records a network failure or 4xx/5xx HTTP response
    #[allow(dead_code)]
    pub fn record_network_error(
        &self,
        method: &str,
        url: &str,
        status: u16,
        error_text: Option<&str>,
    ) {
        let mut errors = self
            .network_errors
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if errors.len() >= BROWSER_MAX_CONSOLE_LINES {
            errors.remove(0);
        }

        errors.push(NetworkErrorEntry {
            method: method.to_string(),
            url: url.to_string(),
            status,
            error_text: error_text.map(|s| s.to_string()),
        });
    }

    /// Clears recorded diagnostics
    #[allow(dead_code)]
    pub fn clear(&self) {
        if let Ok(mut logs) = self.console_logs.lock() {
            logs.clear();
        }
        if let Ok(mut errors) = self.network_errors.lock() {
            errors.clear();
        }
    }

    /// Formats a complete diagnostics report for agent inspection
    pub fn format_report(&self) -> String {
        let logs = self.console_logs.lock().unwrap_or_else(|e| e.into_inner());
        let errors = self
            .network_errors
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if logs.is_empty() && errors.is_empty() {
            return "No runtime console errors or failed network requests detected on current page.".to_string();
        }

        let mut out = String::from("### Browser Runtime Diagnostics\n\n");

        if !errors.is_empty() {
            out.push_str("#### Failed Network Requests (HTTP 4xx/5xx / Network Errors):\n");
            for err in errors.iter() {
                let err_detail = err
                    .error_text
                    .as_deref()
                    .map(|d| format!(" — {}", d))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "• `[HTTP {}]` {} {}{}\n",
                    err.status, err.method, err.url, err_detail
                ));
            }
            out.push('\n');
        }

        if !logs.is_empty() {
            out.push_str("#### Console Logs & Uncaught Exceptions:\n");
            for log in logs.iter() {
                let badge = match log.level {
                    LogLevel::Error => "[ERROR]",
                    LogLevel::Warn => "[WARN]",
                    LogLevel::Info => "[INFO]",
                };
                out.push_str(&format!("• `{}` `{}` {}\n", log.timestamp, badge, log.text));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_collector_recording_and_formatting() {
        let collector = DebugCollector::new();

        collector.record_console(
            LogLevel::Error,
            "Uncaught TypeError: Cannot read property 'map'",
        );
        collector.record_console(LogLevel::Warn, "Source map not found");
        collector.record_network_error(
            "POST",
            "http://localhost:3000/api/users",
            404,
            Some("Not Found"),
        );

        let report = collector.format_report();
        assert!(report.contains("[ERROR]"));
        assert!(report.contains("Uncaught TypeError"));
        assert!(report.contains("[HTTP 404]"));
        assert!(report.contains("POST http://localhost:3000/api/users"));

        collector.clear();
        let empty_report = collector.format_report();
        assert!(empty_report.contains("No runtime console errors"));
    }
}
