use crate::agent::types::AgentEvent;
use crate::logging::formatter::HonoLogFormatter;
use crate::logging::runtime::{is_pid_alive, ActiveSessionRecord};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

pub struct LogTailer {
    pub formatter: HonoLogFormatter,
    pub json: bool,
    pub raw: bool,
    pub filter: Option<String>,
}

impl LogTailer {
    pub fn new(no_color: bool, json: bool, raw: bool, filter: Option<String>) -> Self {
        Self {
            formatter: HonoLogFormatter::new(no_color),
            json,
            raw,
            filter: filter.map(|s| s.to_lowercase()),
        }
    }

    /// Tails a session JSONL or raw tracing log file.
    /// - `tail_count`: number of initial lines/events to display (e.g. 50, 100, 200).
    /// - `follow`: if true, streams live updates continuously until Ctrl+C or process exit.
    pub async fn tail(
        &self,
        file_path: &Path,
        tail_count: usize,
        follow: bool,
        active_record: Option<&ActiveSessionRecord>,
    ) -> anyhow::Result<()> {
        if !file_path.exists() {
            anyhow::bail!("Log file does not exist: {}", file_path.display());
        }

        // 1. Print header banner unless machine JSON mode is active
        if !self.json && !self.raw {
            let session_id = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            println!(
                "{}\n",
                self.formatter
                    .format_header(session_id, active_record, file_path)
            );
        }

        // 2. Read initial tail batch from disk
        let mut file = File::open(file_path)?;
        let mut current_offset = file.metadata()?.len();

        let initial_lines = self.read_initial_tail(&mut file, tail_count)?;
        for line in initial_lines {
            println!("{}", line);
        }
        std::io::stdout().flush()?;

        // If not following, we're done!
        if !follow {
            return Ok(());
        }

        // 3. Live async following loop
        let pid = active_record.map(|r| r.pid);
        let mut reader = BufReader::new(File::open(file_path)?);
        reader.seek(SeekFrom::Start(current_offset))?;

        let mut check_counter = 0u64;

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    if !self.json {
                        println!("\n\x1b[90m[minicode logs] Detached from log stream\x1b[0m");
                    }
                    break;
                }

                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    check_counter += 1;

                    // Check for file growth
                    if let Ok(meta) = std::fs::metadata(file_path) {
                        let new_len = meta.len();
                        if new_len > current_offset {
                            let mut line = String::new();
                            while reader.read_line(&mut line)? > 0 {
                                let trimmed = line.trim();
                                if !trimmed.is_empty() {
                                    if let Some(formatted) = self.format_raw_or_jsonl(trimmed) {
                                        println!("{}", formatted);
                                    }
                                }
                                line.clear();
                            }
                            current_offset = new_len;
                            std::io::stdout().flush()?;
                        }
                    }

                    // Periodically (every ~1s) check if the tracked PID is still alive
                    if check_counter.is_multiple_of(10) {
                        if let Some(target_pid) = pid {
                            if !is_pid_alive(target_pid) {
                                // Flush any remaining bytes before exiting
                                if let Ok(meta) = std::fs::metadata(file_path) {
                                    if meta.len() > current_offset {
                                        let mut line = String::new();
                                        while reader.read_line(&mut line)? > 0 {
                                            let trimmed = line.trim();
                                            if !trimmed.is_empty() {
                                                if let Some(formatted) = self.format_raw_or_jsonl(trimmed) {
                                                    println!("{}", formatted);
                                                }
                                            }
                                            line.clear();
                                        }
                                        std::io::stdout().flush()?;
                                    }
                                }

                                if !self.json {
                                    println!("\n\x1b[90m● [minicode logs] Session process PID {} exited • Stream ended\x1b[0m", target_pid);
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Reads up to `tail_count` lines from the end of the file.
    pub fn read_initial_tail(
        &self,
        file: &mut File,
        tail_count: usize,
    ) -> anyhow::Result<Vec<String>> {
        file.seek(SeekFrom::Start(0))?;
        let reader = BufReader::new(file);
        let mut buffer = VecDeque::with_capacity(tail_count);

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(formatted) = self.format_raw_or_jsonl(trimmed) {
                if buffer.len() >= tail_count {
                    buffer.pop_front();
                }
                buffer.push_back(formatted);
            }
        }

        Ok(buffer.into_iter().collect())
    }

    /// Formats a single line according to mode (raw tracing line or semantic JSONL event).
    fn format_raw_or_jsonl(&self, line: &str) -> Option<String> {
        let formatted = if self.raw {
            Some(self.format_raw_tracing_line(line))
        } else {
            if line.starts_with("{\"session_meta\":") {
                return None;
            }

            match serde_json::from_str::<AgentEvent>(line) {
                Ok(event) => {
                    if self.json {
                        self.formatter.format_json(&event)
                    } else {
                        self.formatter.format_event(&event)
                    }
                }
                Err(_) => {
                    // If not valid JSON, pass raw line through if in raw mode or fallback
                    if self.raw {
                        Some(line.to_string())
                    } else {
                        None
                    }
                }
            }
        };

        if let (Some(res), Some(ref kw)) = (&formatted, &self.filter) {
            let matches_raw = line.to_lowercase().contains(kw);
            let matches_fmt = res.to_lowercase().contains(kw);
            if !matches_raw && !matches_fmt {
                return None;
            }
        }

        formatted
    }

    /// Colorizes standard Rust `tracing` output lines for `--raw` mode.
    fn format_raw_tracing_line(&self, line: &str) -> String {
        if self.formatter.no_color {
            return line.to_string();
        }

        if line.contains(" ERROR ") {
            line.replace(" ERROR ", " \x1b[1;31mERROR\x1b[0m ")
        } else if line.contains(" WARN ") {
            line.replace(" WARN ", " \x1b[1;33mWARN\x1b[0m ")
        } else if line.contains(" INFO ") {
            line.replace(" INFO ", " \x1b[1;32mINFO\x1b[0m ")
        } else if line.contains(" DEBUG ") {
            line.replace(" DEBUG ", " \x1b[1;34mDEBUG\x1b[0m ")
        } else if line.contains(" TRACE ") {
            line.replace(" TRACE ", " \x1b[90mTRACE\x1b[0m ")
        } else {
            line.to_string()
        }
    }
}
