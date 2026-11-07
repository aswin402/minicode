//! Background task barriers, completion harvesting, and multi-source log retrieval for [`MiniDevRegistry`].

use super::*;
use crate::dev::models::{DevProcessId, DevProcessStatus, DevProcessSummary, DevProcessType};
use crate::dev::process::DevProcessHandle;
use crate::error::{DevError, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

impl MiniDevRegistry {
    /// Lists recently completed non-daemon background tasks (builds, tests, compiles, scripts)
    /// alongside their log file path and command string.
    pub async fn list_recent_completed_tasks(
        &self,
        limit: usize,
    ) -> Vec<(DevProcessSummary, Option<PathBuf>, String)> {
        if limit == 0 {
            return Vec::new();
        }
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };
        let mut completed = Vec::new();
        for handle in handles {
            if handle.is_daemon
                || matches!(
                    handle.process_type,
                    DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                )
            {
                continue;
            }
            let status = handle.status.read().await.clone();
            if !status.is_alive() {
                let summary = self.create_summary(&handle).await;
                completed.push((
                    handle.started_at,
                    summary,
                    handle.log_path.clone(),
                    handle.command.clone(),
                ));
            }
        }
        completed.sort_by(|a, b| b.0.cmp(&a.0));
        completed
            .into_iter()
            .take(limit)
            .map(|(_, s, log_path, cmd)| (s, log_path, cmd))
            .collect()
    }

    /// Returns the number of currently running non-daemon background OS tasks (builds, tests, compiles, scripts, timers).
    pub async fn active_non_daemon_task_count(&self) -> usize {
        let lock = self.processes.read().await;
        let mut count = 0usize;
        for handle in lock.values() {
            if handle.is_daemon
                || matches!(
                    handle.process_type,
                    DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                )
            {
                continue;
            }
            if handle.status.read().await.is_alive() {
                count += 1;
            }
        }
        count
    }

    /// Event-driven wait barrier until all active non-daemon background tasks complete,
    /// a live user steering prompt arrives, or `timeout` elapses.
    pub async fn wait_until_active_tasks_settle(&self, timeout: std::time::Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.task_completion_notify.notified();
            if self.has_steering_prompts() || self.active_non_daemon_task_count().await == 0 {
                break;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            if tokio::time::timeout(deadline - now, notified)
                .await
                .is_err()
            {
                break;
            }
        }
    }

    /// Awaits specific background task IDs (or all active non-daemon background tasks if `ids` is empty)
    /// using `"all"` or `"any"` / `"first"` / `"await_next"` barrier semantics.
    /// Yields early without killing background tasks if a live user steering prompt arrives.
    pub async fn await_tasks(
        &self,
        ids: &[String],
        wait_mode: &str,
        timeout: std::time::Duration,
    ) -> Result<String> {
        let wait_any = matches!(
            wait_mode.trim().to_ascii_lowercase().as_str(),
            "any" | "first" | "next" | "await_next" | "wait_any"
        );
        let target_ids: Vec<DevProcessId> = if ids.is_empty() {
            let lock = self.processes.read().await;
            let mut active = Vec::new();
            for (id, handle) in lock.iter() {
                if !handle.is_daemon
                    && !matches!(
                        handle.process_type,
                        DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                    )
                    && handle.status.read().await.is_alive()
                {
                    active.push(id.clone());
                }
            }
            active
        } else {
            ids.iter().map(|s| DevProcessId::from(s.as_str())).collect()
        };

        if target_ids.is_empty() {
            let recent = self.list_recent_completed_tasks(5).await;
            if recent.is_empty() {
                return Ok("No active or recently completed non-daemon background tasks found in `minitask`.".to_string());
            }
            let mut out =
                String::from("All background tasks in `minitask` have already completed:\n");
            for (summary, log_path, cmd) in recent {
                out.push_str(&format!(
                    "• `{}` ({}) — Status: **{}** | Command: `{}`{}\n",
                    summary.id,
                    summary.name,
                    summary.status,
                    cmd,
                    log_path
                        .as_ref()
                        .map(|p| format!(" | Log: `{}`", p.display()))
                        .unwrap_or_default()
                ));
            }
            return Ok(out);
        }

        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.task_completion_notify.notified();
            if self.has_steering_prompts() {
                break;
            }
            let mut alive_count = 0usize;
            let mut finished_count = 0usize;

            for tid in &target_ids {
                if let Some(handle) = self.get_handle(tid).await {
                    if handle.status.read().await.is_alive() {
                        alive_count += 1;
                    } else {
                        finished_count += 1;
                    }
                }
            }

            if alive_count == 0 || (wait_any && finished_count > 0) {
                break;
            }

            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            if tokio::time::timeout(deadline - now, notified)
                .await
                .is_err()
            {
                break;
            }
        }

        let mut out = String::new();
        let mut any_still_alive = false;
        for tid in &target_ids {
            if let Some(handle) = self.get_handle(tid).await {
                let status = handle.status.read().await.clone();
                if status.is_alive() {
                    any_still_alive = true;
                } else {
                    self.mark_task_reported(&handle.id);
                }
                let logs = handle.get_logs(40, None).await;
                let log_hint = handle
                    .log_path
                    .as_ref()
                    .map(|p| format!(" | Log: `{}`", p.display()))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "### Task `{}` ({})\n- **Status:** `{}` ({}s){}\n- **Command:** `{}`\n",
                    handle.id,
                    handle.name,
                    status,
                    handle.uptime_secs(),
                    log_hint,
                    handle.command
                ));
                if !logs.is_empty() {
                    out.push_str("- **Output (last 40 lines):**\n```\n");
                    out.push_str(&logs.join("\n"));
                    out.push_str("\n```\n");
                }
            } else {
                out.push_str(&format!(
                    "### Task `{}`\n- **Status:** Not found in `minitask`\n",
                    tid
                ));
            }
        }
        if any_still_alive && self.has_steering_prompts() {
            out.push_str(
                "\n⚡ **Live User Prompt Received:** The background task(s) above are still running uninterrupted in `minitask`, and the user sent a live prompt. Handle the user's prompt now; `minitask` will automatically notify you when the background task(s) finish.\n",
            );
        }
        Ok(out)
    }

    /// Returns `true` if there is at least one completed non-daemon background task in `minitask`
    /// that has not yet been reported to the Orchestrator.
    pub async fn has_unreported_completed_tasks(&self) -> bool {
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };
        for handle in handles {
            if handle.is_daemon
                || matches!(
                    handle.process_type,
                    DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                )
            {
                continue;
            }
            if handle.status.read().await.is_alive() {
                continue;
            }
            let already_reported = self
                .reported_completed_tasks
                .lock()
                .map(|set| set.contains(&handle.id))
                .unwrap_or(false);
            if !already_reported {
                return true;
            }
        }
        false
    }

    /// Formats a live telemetry snapshot of all currently running `minitask` processes
    /// (`<active_minitask_processes>...</active_minitask_processes>`) so the Orchestrator can
    /// make context-aware decisions when a live user prompt arrives during background execution.
    pub async fn format_active_tasks_context(&self) -> String {
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };
        let mut active_lines = Vec::new();
        for handle in handles {
            let status = handle.status.read().await.clone();
            if !status.is_alive() {
                continue;
            }
            let pid = handle.pid();
            let usage = if pid > 0 && pid != std::process::id() {
                crate::dev::metrics::sample_process_metrics(pid)
            } else {
                Self::estimate_in_process_usage(&handle).await
            };
            let port_str = handle
                .ports
                .read()
                .await
                .first()
                .copied()
                .map(|p| format!(" port=\"{}\"", p))
                .unwrap_or_default();
            active_lines.push(format!(
                "  <process id=\"{}\" name=\"{}\" type=\"{}\" is_daemon=\"{}\" pid=\"{}\" uptime_secs=\"{}\" cpu_pct=\"{:.1}\" mem_mb=\"{:.1}\"{} working_dir=\"{}\" command={:?} />",
                handle.id,
                handle.name,
                handle.process_type,
                handle.is_daemon,
                handle.pid(),
                handle.uptime_secs(),
                usage.cpu_percent,
                usage.memory_rss_mb,
                port_str,
                handle.working_dir.display(),
                handle.command
            ));
        }
        if active_lines.is_empty() {
            String::new()
        } else {
            format!(
                "<active_minitask_processes>\n{}\n</active_minitask_processes>",
                active_lines.join("\n")
            )
        }
    }

    /// Harvests any newly completed non-daemon background tasks (builds, tests, compiles, scripts, timers)
    /// that have not yet been reported to the Orchestrator, formatting them into a structured
    /// `<completed_background_tasks>...</completed_background_tasks>` notification block.
    pub async fn harvest_newly_completed_tasks_block(
        &self,
        workspace_root: &Path,
    ) -> Option<(Vec<String>, String)> {
        let handles: Vec<Arc<DevProcessHandle>> = {
            let lock = self.processes.read().await;
            lock.values().cloned().collect()
        };

        let mut newly_completed = Vec::new();
        for handle in handles {
            if handle.is_daemon
                || matches!(
                    handle.process_type,
                    DevProcessType::Worker | DevProcessType::Swarm | DevProcessType::Cron
                )
            {
                continue;
            }
            let status = handle.status.read().await.clone();
            if status.is_alive() {
                continue;
            }
            let is_new = if let Ok(mut set) = self.reported_completed_tasks.lock() {
                set.insert(handle.id.clone())
            } else {
                false
            };
            if is_new {
                newly_completed.push((handle, status));
            }
        }

        if newly_completed.is_empty() {
            return None;
        }

        let context_window = crate::tools::exec::resolve_context_window(workspace_root);
        let mut ids = Vec::with_capacity(newly_completed.len());
        let mut block = String::from("<completed_background_tasks>\n");
        for (handle, status) in newly_completed {
            ids.push(handle.id.as_str().to_string());
            let raw_logs = handle.get_logs(120, None).await.join("\n");
            let clean_logs = crate::security::sanitize_text(&raw_logs);
            let exit_code = match status {
                DevProcessStatus::Exited(code) => code,
                DevProcessStatus::Stopped => Some(0),
                _ => Some(1),
            };
            let rtk_res = crate::tools::rtk_filter::RtkFilter::filter_for_context(
                &handle.command,
                &clean_logs,
                exit_code,
                context_window,
            );
            let compacted = crate::tools::compactor::compact_tool_output_for_context(
                &handle.command,
                &rtk_res.content,
                exit_code,
                context_window,
            );
            let fault_hint = match status {
                DevProcessStatus::Exited(Some(code)) if code != 0 => {
                    let frames = crate::context::search::fault_localizer::FaultLocalizer::extract_trace_frames(&clean_logs);
                    if frames.is_empty() {
                        String::new()
                    } else {
                        let mut hint = String::from("\n[Where] Probable Fault Sites:\n");
                        for frame in frames.iter().take(4) {
                            hint.push_str(&format!(
                                "  • `{}:{}`\n",
                                frame.file_path, frame.line_number
                            ));
                        }
                        hint
                    }
                }
                _ => String::new(),
            };
            let log_attr = handle
                .log_path
                .as_ref()
                .map(|p| format!(" log_path=\"{}\"", p.display()))
                .unwrap_or_default();
            block.push_str(&format!(
                "  <task id=\"{}\" name=\"{}\" status=\"{}\" duration_secs=\"{}\" command={:?}{}>\n{}{}\n  </task>\n",
                handle.id,
                handle.name,
                status,
                handle.uptime_secs(),
                handle.command,
                log_attr,
                compacted.trim(),
                fault_hint
            ));
        }
        block.push_str("</completed_background_tasks>");
        Some((ids, block))
    }

    /// Reads recent logs from the process output ring buffer with flexible ID matching.
    pub async fn logs(
        &self,
        id: &DevProcessId,
        tail: usize,
        filter: Option<&str>,
    ) -> Result<Vec<String>> {
        let handle_opt = {
            let lock = self.processes.read().await;
            Self::lookup_handle(&lock, id).cloned()
        };
        if let Some(ref handle) = handle_opt {
            let lines = handle.get_logs(tail, filter).await;
            if !lines.is_empty() {
                return Ok(lines);
            }
        }
        let raw_id = id.as_str().strip_prefix("worker-").unwrap_or(id.as_str());
        let ws_hint_buf = handle_opt
            .as_ref()
            .map(|h| h.working_dir.clone())
            .or_else(|| {
                crate::agent::subagent::try_get_global_subagent_pool()
                    .map(|p| p.workspace_root().to_path_buf())
            })
            .or_else(|| std::env::current_dir().ok());
        let ws_hint = ws_hint_buf.as_deref();
        if let Some(transcript) =
            crate::agent::subagent::transcript::get_global_transcript_store().get(raw_id, ws_hint)
        {
            let mut synth_lines = Vec::new();
            synth_lines.push(format!(
                "[{}] Role: {} | Task: {}",
                transcript.subagent_id,
                transcript.role.badge(),
                crate::utils::truncate_ellipsis(&transcript.prompt, 100)
            ));
            for step in &transcript.steps {
                let status_icon = if step.success { "✔" } else { "✗" };
                synth_lines.push(format!(
                    "[Turn {} • Step {}] {} {} ({}ms): {}",
                    step.turn,
                    step.step_index,
                    status_icon,
                    step.tool_name,
                    step.duration_ms,
                    step.output_snippet.lines().next().unwrap_or("")
                ));
            }
            if let Some(ref report) = transcript.final_report {
                synth_lines.push("─── Final Report ───".to_string());
                for line in report.lines() {
                    synth_lines.push(line.to_string());
                }
            }
            if let Some(q) = filter {
                let q_low = q.to_lowercase();
                synth_lines.retain(|l| l.to_lowercase().contains(&q_low));
            }
            if synth_lines.len() > tail && tail > 0 {
                let skip = synth_lines.len() - tail;
                synth_lines = synth_lines.into_iter().skip(skip).collect();
            }
            return Ok(synth_lines);
        }
        if id.as_str().starts_with("swarm-") {
            let workers = self
                .list_all_with_completed(Some(DevProcessType::Worker), ws_hint)
                .await;
            if !workers.is_empty() {
                let mut wave_lines = Vec::new();
                wave_lines.push(format!(
                    "[{}] MiniPub Swarm Wave ({} workers)",
                    id.as_str(),
                    workers.len()
                ));
                for w in &workers {
                    wave_lines.push(format!(
                        "  {} {} ({}) — {}",
                        if w.status.is_alive() { "●" } else { "✔" },
                        w.id,
                        w.name,
                        w.status
                    ));
                }
                if let Some(q) = filter {
                    let q_low = q.to_lowercase();
                    wave_lines.retain(|l| l.to_lowercase().contains(&q_low));
                }
                if wave_lines.len() > tail && tail > 0 {
                    let skip = wave_lines.len() - tail;
                    wave_lines = wave_lines.into_iter().skip(skip).collect();
                }
                return Ok(wave_lines);
            }
        }
        if let Some(ws) = ws_hint {
            let host_ws = crate::agent::subagent::transcript::resolve_host_workspace_root(ws);
            let log_dir = host_ws.join(".minicode").join("logs").join("tasks");
            for candidate in [id.as_str().to_string(), format!("worker-{}", raw_id)] {
                let log_file = log_dir.join(format!("{}.log", candidate));
                if log_file.exists() {
                    if let Ok(content) = std::fs::read_to_string(&log_file) {
                        let mut file_lines: Vec<String> =
                            content.lines().map(ToString::to_string).collect();
                        if let Some(q) = filter {
                            let q_low = q.to_lowercase();
                            file_lines.retain(|l| l.to_lowercase().contains(&q_low));
                        }
                        if file_lines.len() > tail && tail > 0 {
                            let skip = file_lines.len() - tail;
                            file_lines = file_lines.into_iter().skip(skip).collect();
                        }
                        return Ok(file_lines);
                    }
                }
            }
        }
        if handle_opt.is_some() {
            return Ok(Vec::new());
        }
        Err(DevError::NotFound(id.to_string()).into())
    }
}
