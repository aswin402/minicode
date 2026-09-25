//! Interactive command and prompt dispatch logic for minicode TUI

use super::{AgentCommand, App};
use crate::ui::modal::ModalState;
use anyhow::Result;
use std::time::Instant;
use tokio::sync::mpsc;

/// Result of dispatching a command or prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    /// Continue running the interactive event loop
    Continue,
    /// Exit the application immediately
    #[allow(dead_code)]
    Exit,
}

/// Parsed goal or intent slash command action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalSubcommand {
    /// Inspect the active goal and living execution ledger (/goal or /intent).
    Show,
    /// Add a new requirement item to the ledger (/goal add <task>).
    Add(String),
    /// Mark a requirement item completed by 1-based index or ID (/goal done <id>).
    Done(String),
    /// Reset the active goal anchor and execution ledger (/goal reset).
    Reset,
    /// Execute autonomously with freeform or explicit goal prompt (/goal run <prompt> or /goal <prompt>).
    Run(String),
}

/// Parses a user input prompt into a `GoalSubcommand` if it begins with `/goal` or `/intent`.
pub fn parse_goal_command(input: &str) -> Option<GoalSubcommand> {
    let trimmed = input.trim();
    let remainder = if trimmed == "/goal" {
        ""
    } else if let Some(rest) = trimmed.strip_prefix("/goal ") {
        rest.trim()
    } else if trimmed == "/intent" {
        ""
    } else if let Some(rest) = trimmed.strip_prefix("/intent ") {
        rest.trim()
    } else {
        return None;
    };

    if remainder.is_empty() {
        return Some(GoalSubcommand::Show);
    }

    if remainder == "reset" {
        return Some(GoalSubcommand::Reset);
    }

    if remainder == "add" {
        return Some(GoalSubcommand::Add(String::new()));
    }
    if let Some(rest) = remainder.strip_prefix("add ") {
        return Some(GoalSubcommand::Add(rest.trim().to_string()));
    }

    if remainder == "done" {
        return Some(GoalSubcommand::Done(String::new()));
    }
    if let Some(rest) = remainder.strip_prefix("done ") {
        return Some(GoalSubcommand::Done(rest.trim().to_string()));
    }

    if remainder == "run" {
        return Some(GoalSubcommand::Run(String::new()));
    }
    if let Some(rest) = remainder.strip_prefix("run ") {
        return Some(GoalSubcommand::Run(rest.trim().to_string()));
    }

    Some(GoalSubcommand::Run(remainder.to_string()))
}

/// Formats an `IntentLedger` into a user-friendly timeline status message.
pub fn format_ledger_timeline(ledger: &crate::context::memory::intent::IntentLedger) -> String {
    let mut out = format!("🎯 Active Goal: {}\n", ledger.root_objective);
    out.push_str(&format!(
        "📋 Living Execution Ledger ({}/{} completed):\n",
        ledger.completed_count(),
        ledger.total_count()
    ));

    if ledger.items.is_empty() {
        out.push_str("   (No tracked requirement items)\n");
    } else {
        for (idx, item) in ledger.items.iter().enumerate() {
            let marker = match item.status {
                crate::context::memory::intent::RequirementStatus::Completed => "[x]",
                crate::context::memory::intent::RequirementStatus::InProgress => "[-]",
                crate::context::memory::intent::RequirementStatus::Blocked => "[!]",
                crate::context::memory::intent::RequirementStatus::Skipped => "[s]",
                crate::context::memory::intent::RequirementStatus::Pending => "[ ]",
            };
            let title_line = match &item.description {
                Some(desc) if !desc.trim().is_empty() => {
                    format!("{}. {} ({})", idx + 1, item.title.trim(), desc.trim())
                }
                _ => format!("{}. {}", idx + 1, item.title.trim()),
            };
            out.push_str(&format!("   {} {}\n", marker, title_line));
        }
    }

    out.push_str(
        "💡 Commands: /goal add <task> | /goal done <index> | /goal reset | /goal run <prompt>",
    );
    out
}

impl<'a> App<'a> {
    /// Handles user submitted text, routing to slash commands or background agent execution
    pub async fn handle_command_or_prompt(
        &mut self,
        prompt: &str,
        display_prompt: Option<&str>,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) -> Result<CommandAction> {
        if prompt == "/exit" || prompt == "/quit" {
            self.modal = ModalState::new_exit_confirm(&self.workspace_root);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/terminal" {
            self.pty_drawer.toggle();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/subagents" || prompt == "/swarm" || prompt == "/workers" {
            self.subagent_drawer.toggle();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/copy" || prompt.starts_with("/copy ") {
            let copy_all = prompt.contains("all");
            let text_to_copy = if copy_all {
                self.timeline.get_all_transcript_text()
            } else {
                self.timeline
                    .get_last_assistant_response()
                    .unwrap_or_default()
            };

            if text_to_copy.trim().is_empty() {
                self.timeline
                    .add_status("ℹ Nothing to copy yet".to_string());
            } else {
                let ok = crate::ui::clipboard::copy_to_clipboard(&text_to_copy);
                if ok {
                    let label = if copy_all {
                        "entire conversation"
                    } else {
                        "latest assistant response"
                    };
                    self.timeline
                        .add_status(format!("✔ Copied {} to clipboard", label));
                } else {
                    self.timeline
                        .add_status("✗ Failed to copy to clipboard".to_string());
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/new" {
            self.timeline = crate::ui::TimelineView::new();
            self.timeline
                .add_status("✨ Started a new session".to_string());
            return Ok(CommandAction::Continue);
        }

        let prompt_trimmed = prompt.trim();
        let prompt_lower = prompt_trimmed.to_lowercase();
        if prompt_lower == "/settings"
            || prompt_lower.starts_with("/settings ")
            || prompt_lower == "/config"
            || prompt_lower.starts_with("/config ")
            || prompt_lower == "/preferences"
            || prompt_lower.starts_with("/preferences ")
        {
            let remainder = if let Some(r) = prompt_trimmed.strip_prefix("/settings") {
                r.trim()
            } else if let Some(r) = prompt_trimmed.strip_prefix("/config") {
                r.trim()
            } else {
                prompt_trimmed
                    .strip_prefix("/preferences")
                    .unwrap_or("")
                    .trim()
            };

            if remainder.is_empty() {
                self.modal = ModalState::new_settings(&self.config, &self.workspace_root);
            } else {
                let parts: Vec<&str> = remainder.split_whitespace().collect();
                match parts.as_slice() {
                    ["model", prov, model] => {
                        self.config
                            .provider
                            .default_models
                            .insert((*prov).to_string(), (*model).to_string());
                        let _ = self.config.save(Some(&self.workspace_root));
                        self.timeline.add_status(format!(
                            "✔ Default model for provider '{}' set to '{}'",
                            prov, model
                        ));
                    }
                    ["auto_approve", val] => {
                        let enabled = matches!(val.to_lowercase().as_str(), "on" | "true" | "1");
                        self.config.agent.auto_approve = enabled;
                        let _ = self.config.save(Some(&self.workspace_root));
                        self.timeline.add_status(format!(
                            "✔ Auto-approve {}",
                            if enabled { "enabled" } else { "disabled" }
                        ));
                    }
                    ["thinking", tokens_str] => {
                        let budget = match tokens_str.to_lowercase().as_str() {
                            "off" | "none" | "0" => 0,
                            "4k" => 4096,
                            "8k" => 8192,
                            "16k" => 16384,
                            "32k" => 32768,
                            other => other.parse::<usize>().unwrap_or(0),
                        };
                        self.config.provider.thinking_budget =
                            if budget == 0 { None } else { Some(budget) };
                        let _ = self.config.save(Some(&self.workspace_root));
                        self.timeline
                            .add_status(format!("✔ Thinking budget set to {} tokens", budget));
                    }
                    _ => {
                        self.modal = ModalState::new_settings(&self.config, &self.workspace_root);
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/configure"
            || prompt == "/setup"
            || prompt == "/keys"
            || prompt == "/key"
            || prompt == "/api"
        {
            self.modal = ModalState::new_provider_select();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/clear" {
            self.timeline.entries.clear();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/commands" {
            self.modal = ModalState::new_command_catalog();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/help" {
            self.modal = ModalState::Help;
            return Ok(CommandAction::Continue);
        }

        if prompt_lower == "/stack" || prompt_lower == "/stacks" {
            self.modal = ModalState::new_stack_select();
            return Ok(CommandAction::Continue);
        }

        if prompt_lower == "/blocks"
            || prompt_lower == "/miniblocks"
            || prompt_lower.starts_with("/blocks ")
            || prompt_lower.starts_with("/miniblocks ")
        {
            let mut blocks_modal = ModalState::new_blocks(&self.workspace_root);
            let query = if prompt_lower.starts_with("/miniblocks") {
                prompt_trimmed[11..].trim()
            } else if prompt_lower.starts_with("/blocks") {
                prompt_trimmed[7..].trim()
            } else {
                ""
            };
            if !query.is_empty() {
                if let ModalState::Blocks(ref mut state) = blocks_modal {
                    state.search_query = query.to_string();
                    state.refresh_filtered();
                }
            }
            self.modal = blocks_modal;
            return Ok(CommandAction::Continue);
        }

        // MiniDev Runtime Orchestrator Slash Commands (/dev, /processes, /serve)
        if prompt_lower == "/dev"
            || prompt_lower == "/processes"
            || prompt_lower == "/serve"
            || prompt_lower.starts_with("/dev ")
            || prompt_lower.starts_with("/processes ")
            || prompt_lower.starts_with("/serve ")
        {
            let sub = if prompt_lower.starts_with("/dev ") {
                prompt_trimmed[5..].trim()
            } else if prompt_lower.starts_with("/processes ") {
                prompt_trimmed[11..].trim()
            } else if prompt_lower.starts_with("/serve ") {
                prompt_trimmed[7..].trim()
            } else {
                ""
            };

            let registry = crate::dev::registry::get_global_dev_registry();
            match sub {
                "" | "modal" | "ui" => {
                    let list = registry.list().await;
                    let res = registry.resources().await;
                    let logs = if let Some(first) = list.first() {
                        registry
                            .logs(&first.id, 100, None)
                            .await
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    self.modal = ModalState::Processes(
                        crate::ui::modals::processes::ProcessesModalState::with_data(
                            &self.workspace_root,
                            list,
                            Some(res),
                            logs,
                        ),
                    );
                }
                "list" | "ps" => {
                    let list = registry.list().await;
                    if list.is_empty() {
                        self.timeline.add_status("ℹ No active development processes running. Use 'mini_dev start' to launch one.".to_string());
                    } else {
                        let mut msg = format!("🚀 Active Development Processes ({})\n", list.len());
                        for p in list {
                            let url_disp = p.url.as_deref().unwrap_or("-");
                            msg.push_str(&format!(
                                "• [{}] {} ({:?}) - URL: {} | PID: {} | CPU: {:.1}% | RSS: {:.1}MB\n",
                                p.id,
                                p.name,
                                p.process_type,
                                url_disp,
                                p.pid.unwrap_or(0),
                                p.cpu_percent,
                                p.memory_rss_mb,
                            ));
                        }
                        self.timeline.add_status(msg);
                    }
                }
                "resources" => {
                    let res = registry.resources().await;
                    self.timeline.add_status(format!(
                        "📈 Runtime Resource Telemetry:\n• Active Processes: {}\n• Total CPU Usage: {:.1}%\n• Total RSS Memory: {:.1} MB\n• Listening Ports: {:?}",
                        res.total_active_processes,
                        res.total_cpu_percent,
                        res.total_memory_rss_mb,
                        res.active_ports,
                    ));
                }
                "kill" | "kill_all" | "stop all" => {
                    let count = registry.kill_all().await.unwrap_or(0);
                    self.timeline.add_status(format!(
                        "✔ Terminated {} active development processes.",
                        count
                    ));
                }
                other if other.starts_with("stop ") => {
                    let id_str = other[5..].trim();
                    let id = crate::dev::models::DevProcessId::from(id_str);
                    match registry.stop(&id).await {
                        Ok(_) => self
                            .timeline
                            .add_status(format!("✔ Stopped development process '{}'.", id_str)),
                        Err(e) => self
                            .timeline
                            .add_status(format!("❌ Failed to stop process '{}': {}", id_str, e)),
                    }
                }
                other if other.starts_with("logs ") => {
                    let id_str = other[5..].trim();
                    let id = crate::dev::models::DevProcessId::from(id_str);
                    match registry.logs(&id, 20, None).await {
                        Ok(logs) => {
                            if logs.is_empty() {
                                self.timeline
                                    .add_status(format!("ℹ No logs for process '{}'.", id_str));
                            } else {
                                self.timeline.add_status(format!(
                                    "📜 Logs for '{}':\n{}",
                                    id_str,
                                    logs.join("\n")
                                ));
                            }
                        }
                        Err(e) => self
                            .timeline
                            .add_status(format!("❌ Error fetching logs: {}", e)),
                    }
                }
                other if other == "screenshot" || other.starts_with("screenshot ") => {
                    let target_id = other.strip_prefix("screenshot").unwrap_or("").trim();
                    let target_url = if !target_id.is_empty() {
                        let id = crate::dev::models::DevProcessId::from(target_id);
                        registry.get(&id).await.and_then(|p| p.url)
                    } else {
                        let list = registry.list().await;
                        list.into_iter().find_map(|p| p.url)
                    };

                    if let Some(url) = target_url {
                        let mode = crate::tools::browser::BrowserMode::Headless;
                        self.timeline
                            .add_status(format!("📸 Capturing screenshot for '{}'...", url));
                        let res = async {
                            let _ =
                                crate::tools::browser::BrowserController::navigate_and_snapshot(
                                    &url,
                                    mode,
                                    &self.workspace_root,
                                )
                                .await?;
                            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                            crate::tools::browser::BrowserController::take_screenshot(
                                mode,
                                &self.workspace_root,
                                None,
                            )
                            .await
                        }
                        .await;

                        match res {
                            Ok(msg) => self
                                .timeline
                                .add_status(format!("✔ Screenshot saved: {}", msg)),
                            Err(e) => self
                                .timeline
                                .add_status(format!("❌ Screenshot failed: {}", e)),
                        }
                    } else {
                        self.timeline.add_status("ℹ No active server URL found. Start a server with 'mini_dev start' first.".to_string());
                    }
                }
                "workers" | "subagents" => {
                    let list = registry.list_workers().await;
                    if list.is_empty() {
                        self.timeline.add_status(
                            "ℹ No active autonomous subagents or delegated workers running."
                                .to_string(),
                        );
                    } else {
                        let mut msg = format!(
                            "🤖 Active Autonomous Subagents & Workers ({})\n",
                            list.len()
                        );
                        for p in list {
                            msg.push_str(&format!(
                                "• [{}] {} | Status: {:?} | PID: {} | CPU: {:.1}% | RSS: {:.1}MB | Uptime: {}s\n",
                                p.id,
                                p.name,
                                p.status,
                                p.pid.unwrap_or(0),
                                p.cpu_percent,
                                p.memory_rss_mb,
                                p.uptime_secs,
                            ));
                        }
                        self.timeline.add_status(msg);
                    }
                }
                other if other.starts_with("port ") || other.starts_with("probe ") => {
                    let port_str = other
                        .strip_prefix("port ")
                        .or_else(|| other.strip_prefix("probe "))
                        .unwrap_or("")
                        .trim();
                    if let Ok(port) = port_str.parse::<u16>() {
                        let is_listening = crate::dev::ports::is_port_listening(port);
                        if is_listening {
                            let pid = crate::dev::ports::find_pid_by_port(port);
                            let (comm, cmd) = if let Some(p) = pid {
                                crate::dev::ports::get_process_info(p)
                            } else {
                                (None, None)
                            };
                            let suggested =
                                crate::dev::ports::find_next_available_port(port + 1, 100);
                            let pid_str = pid
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            let proc_str = comm.as_deref().unwrap_or("unknown");
                            let cmd_str = cmd.as_deref().unwrap_or("-");
                            let sugg_str = suggested
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "none".to_string());

                            self.timeline.add_status(format!(
                                "⚠️ Port {} is OCCUPIED:\n• Conflicting PID: {}\n• Process: {}\n• Command: {}\n• Next Available Port: {}",
                                port, pid_str, proc_str, cmd_str, sugg_str
                            ));
                        } else {
                            self.timeline.add_status(format!(
                                "✔ Port {} is AVAILABLE (ready for binding)",
                                port
                            ));
                        }
                    } else {
                        self.timeline
                            .add_status(format!("❌ Invalid port number '{}'.", port_str));
                    }
                }
                "watchdog" => {
                    let list = registry.list().await;
                    if list.is_empty() {
                        self.timeline.add_status(
                            "ℹ No active development processes running under watchdog.".to_string(),
                        );
                    } else {
                        let mut msg = format!(
                            "🛡 Watchdog Process Supervision ({} processes):\n\n",
                            list.len()
                        );
                        for p in list {
                            let pol_str = match &p.restart_policy {
                                crate::dev::models::RestartPolicy::Never => "Never".to_string(),
                                crate::dev::models::RestartPolicy::OnFailure {
                                    max_retries,
                                    backoff_ms,
                                } => {
                                    format!(
                                        "OnFailure (max: {}, backoff: {}ms)",
                                        max_retries, backoff_ms
                                    )
                                }
                                crate::dev::models::RestartPolicy::Always {
                                    max_retries,
                                    backoff_ms,
                                } => {
                                    format!(
                                        "Always (max: {}, backoff: {}ms)",
                                        max_retries, backoff_ms
                                    )
                                }
                            };
                            let shift_str = match &p.port_resolution {
                                Some(crate::dev::models::PortResolution::Shifted {
                                    requested,
                                    resolved,
                                    ..
                                }) => {
                                    format!(" | Port shifted: {} -> {}", requested, resolved)
                                }
                                _ => String::new(),
                            };
                            msg.push_str(&format!(
                                "• [{}] {} (PID {:?}) | Status: {:?} | Restarts: {} | Policy: {}{}\n",
                                p.id, p.name, p.pid, p.status, p.restart_count, pol_str, shift_str
                            ));
                        }
                        self.timeline.add_status(msg);
                    }
                }
                unknown => {
                    self.timeline.add_status(format!(
                        "ℹ Unknown /dev subcommand '{}'. Usage: /dev [list | workers | resources | port <number> | watchdog | logs <id> | stop <id> | kill | screenshot]",
                        unknown
                    ));
                }
            }
            return Ok(CommandAction::Continue);
        }

        // MiniKit Slash Commands (/kit, /stacks, /skills, /drift, /heal, /sync, /doctor)
        if prompt_lower == "/kit"
            || prompt_lower.starts_with("/kit ")
            || prompt_lower.starts_with("/stack ")
            || prompt_lower == "/block"
            || prompt_lower.starts_with("/block ")
            || prompt_lower == "/drift"
            || prompt_lower == "/heal"
            || prompt_lower == "/skills"
            || prompt_lower == "/sync"
            || prompt_lower == "/doctor"
        {
            let sub_owned = if prompt_lower.starts_with("/kit ") {
                prompt_trimmed
                    .strip_prefix("/kit")
                    .unwrap_or("")
                    .trim()
                    .to_string()
            } else if prompt_lower.starts_with("/stack ") {
                let rest = prompt_trimmed.strip_prefix("/stack").unwrap_or("").trim();
                format!("stack {}", rest)
            } else if prompt_lower.starts_with("/block ") {
                let rest = prompt_trimmed.strip_prefix("/block").unwrap_or("").trim();
                format!("block {}", rest)
            } else if prompt_lower == "/block" {
                "blocks".to_string()
            } else if prompt_lower == "/drift" {
                "diff".to_string()
            } else if prompt_lower == "/heal" {
                "heal".to_string()
            } else if prompt_lower == "/skills" {
                "skills".to_string()
            } else if prompt_lower == "/sync" {
                "sync".to_string()
            } else if prompt_lower == "/doctor" {
                "doctor".to_string()
            } else {
                String::new()
            };
            let sub = sub_owned.as_str();

            let sub_lower = sub.to_lowercase();
            if sub_lower.is_empty() || sub_lower == "help" {
                let help =
                    "⚡ **MiniKit Engine** (Autonomous Architecture Stacks, Blocks, Packages & Skills):\n\
  • `/kit stacks` (or `/stacks`)          — Interactive stack picker and scaffold wizard\n\
  • `/kit show <name>`                    — Inspect files and dependencies of a stack\n\
  • `/kit stack add <name> [--dir <p>]`   — Directly scaffold a template into workspace\n\
  • `/kit blocks` (or `/blocks`)          — List all available modular architecture blocks\n\
  • `/kit block <name>`                   — Inspect files and dependencies of an architecture block\n\
  • `/kit block add <name> [--force]`     — Add modular block (docker, ci, tailwind, etc.) into workspace\n\
  • `/kit snapshot <name> [--global]`     — Snapshot workspace into a reusable stack template\n\
  • `/kit new <name> [--runtime <rt>]`    — Create a new custom stack template specification\n\
  • `/kit stack remove <name>`            — Remove a custom stack template specification\n\
  • `/kit skills` (or `/skills`)          — List all installed & built-in domain skills\n\
  • `/kit skill <name>`                   — Inspect guidelines & patterns for a technology\n\
  • `/kit skill install <name>`           — Install a domain skill package into the project\n\
  • `/kit skill remove <name>`            — Uninstall a domain skill from the project\n\
  • `/kit diff` (or `/drift`)             — Inspect workspace architecture drift from template\n\
  • `/kit heal` (or `/heal`)              — Automatically restore missing stack template files\n\
  • `/kit sync` (or `/sync`)              — Scan project, sync `minikit.json`, AGENTS.md & docs\n\
  • `/kit doctor` (or `/doctor`)          — Diagnose runtimes (bun, uv, cargo, flutter, npm)\n\
  • `/kit add <pkg>`                      — Prompt agent to verify and add external dependency\n\
  • `/kit remove <pkg>` (or `/kit rm`)    — Remove external dependency from manifest";
                self.timeline.add_status(help.to_string());
                return Ok(CommandAction::Continue);
            } else if sub_lower == "stacks" || sub_lower == "stack" || sub_lower == "stack list" {
                self.modal = ModalState::new_stack_select();
                return Ok(CommandAction::Continue);
            } else if sub_lower == "blocks" || sub_lower == "block" || sub_lower == "block list" {
                match crate::tools::minikit::MiniKitService::list_blocks(&self.workspace_root).await
                {
                    Ok(list) => self.timeline.add_status(list),
                    Err(e) => self
                        .timeline
                        .add_status(format!("✗ Failed to list architecture blocks: {}", e)),
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("block show ")
                || (sub_lower.starts_with("block ") && !sub_lower.starts_with("block add "))
            {
                let block_name = sub
                    .strip_prefix("block show ")
                    .or_else(|| sub.strip_prefix("block "))
                    .unwrap_or("")
                    .trim();
                if block_name.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit block show <block-name>`".to_string());
                } else {
                    match crate::tools::minikit::MiniKitService::show_block(
                        &self.workspace_root,
                        block_name,
                    )
                    .await
                    {
                        Ok(content) => self.timeline.add_status(content),
                        Err(e) => self
                            .timeline
                            .add_status(format!("✗ Failed to show block `{}`: {}", block_name, e)),
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("block add ") {
                let remainder = sub.strip_prefix("block add ").unwrap_or("").trim();
                if remainder.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit block add <block-name> [--force]`".to_string());
                } else {
                    let force = remainder.contains("--force") || remainder.contains("-f");
                    let block_name = remainder
                        .replace("--force", "")
                        .replace("-f", "")
                        .trim()
                        .to_string();
                    match crate::tools::minikit::MiniKitService::add_block(
                        &self.workspace_root,
                        &block_name,
                        force,
                    )
                    .await
                    {
                        Ok(msg) => self.timeline.add_status(msg),
                        Err(e) => self.timeline.add_status(format!(
                            "✗ Failed to add architecture block `{}`: {}",
                            block_name, e
                        )),
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("show ") || sub_lower.starts_with("stack show ") {
                let stack_name = sub
                    .strip_prefix("stack show ")
                    .or_else(|| sub.strip_prefix("show "))
                    .unwrap_or("")
                    .trim();
                if stack_name.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit show <stack-name>`".to_string());
                } else {
                    match crate::tools::minikit::MiniKitService::show_stack(
                        &self.workspace_root,
                        stack_name,
                    )
                    .await
                    {
                        Ok(content) => self.timeline.add_status(content),
                        Err(e) => self
                            .timeline
                            .add_status(format!("✗ Failed to show stack `{}`: {}", stack_name, e)),
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("stack add ") || sub_lower.starts_with("scaffold ") {
                let remainder = sub
                    .strip_prefix("stack add ")
                    .or_else(|| sub.strip_prefix("scaffold "))
                    .unwrap_or("")
                    .trim();
                if remainder.is_empty() {
                    self.timeline.add_status(
                        "⚠ Usage: `/kit stack add <template-name> [--dir <path>] [--no-install]`"
                            .to_string(),
                    );
                } else {
                    let parts: Vec<&str> = remainder.split_whitespace().collect();
                    let stack_name = parts[0];
                    let mut target_dir = None;
                    let mut no_install = false;
                    let mut i = 1;
                    while i < parts.len() {
                        if (parts[i] == "--dir" || parts[i] == "-d") && i + 1 < parts.len() {
                            target_dir = Some(parts[i + 1]);
                            i += 2;
                        } else if parts[i] == "--no-install" {
                            no_install = true;
                            i += 1;
                        } else {
                            i += 1;
                        }
                    }
                    match crate::tools::minikit::MiniKitService::add_stack(
                        &self.workspace_root,
                        stack_name,
                        target_dir,
                        no_install,
                    )
                    .await
                    {
                        Ok(msg) => self.timeline.add_status(format!("✔ {}", msg)),
                        Err(e) => self.timeline.add_status(format!(
                            "✗ Failed to scaffold stack `{}`: {}",
                            stack_name, e
                        )),
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("stack remove ")
                || sub_lower.starts_with("stack rm ")
                || sub_lower.starts_with("stack delete ")
            {
                let remainder = sub_lower
                    .strip_prefix("stack remove ")
                    .or_else(|| sub_lower.strip_prefix("stack rm "))
                    .or_else(|| sub_lower.strip_prefix("stack delete "))
                    .unwrap_or("")
                    .trim();
                let parts: Vec<&str> = remainder.split_whitespace().collect();
                if parts.is_empty() {
                    self.timeline.add_status(
                        "⚠ Usage: `/kit stack remove <template-name> [--global]`".to_string(),
                    );
                } else {
                    let stack_name = parts[0];
                    let global = parts.contains(&"--global") || parts.contains(&"-g");
                    match crate::tools::minikit::scaffolder::MiniKitScaffolder::delete_custom_stack(
                        &self.workspace_root,
                        stack_name,
                        global,
                    ) {
                        Ok(msg) => self.timeline.add_status(format!("✔ {}", msg)),
                        Err(e) => self
                            .timeline
                            .add_status(format!("✗ Failed to remove stack template: {}", e)),
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("snapshot ") || sub_lower.starts_with("stack snapshot ")
            {
                let remainder = sub
                    .strip_prefix("stack snapshot ")
                    .or_else(|| sub.strip_prefix("snapshot "))
                    .unwrap_or("")
                    .trim();
                if remainder.is_empty() {
                    self.timeline.add_status(
                        "⚠ Usage: `/kit snapshot <template-name> [--desc <text>] [--global]`"
                            .to_string(),
                    );
                } else {
                    let parts: Vec<&str> = remainder.split_whitespace().collect();
                    let stack_name = parts[0];
                    let mut desc = None;
                    let mut global = false;
                    let mut i = 1;
                    while i < parts.len() {
                        if (parts[i] == "--desc" || parts[i] == "-d" || parts[i] == "--description")
                            && i + 1 < parts.len()
                        {
                            desc = Some(parts[i + 1]);
                            i += 2;
                        } else if parts[i] == "--global" || parts[i] == "-g" {
                            global = true;
                            i += 1;
                        } else {
                            i += 1;
                        }
                    }
                    match crate::tools::minikit::MiniKitService::snapshot_stack(
                        &self.workspace_root,
                        stack_name,
                        desc,
                        global,
                    )
                    .await
                    {
                        Ok(res) => self.timeline.add_status(res),
                        Err(e) => self.timeline.add_status(format!(
                            "✗ Failed to snapshot stack template `{}`: {}",
                            stack_name, e
                        )),
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("new ") || sub_lower.starts_with("stack new ") {
                let remainder = sub_lower
                    .strip_prefix("stack new ")
                    .or_else(|| sub_lower.strip_prefix("new "))
                    .unwrap_or("")
                    .trim();
                if remainder.is_empty() {
                    self.timeline.add_status(
                        "⚠ Usage: `/kit new <template-name> [--runtime <bun|uv|cargo|flutter>] [--global]`".to_string(),
                    );
                } else {
                    let parts: Vec<&str> = remainder.split_whitespace().collect();
                    let stack_name = parts[0];
                    let mut runtime = "bun";
                    let mut global = false;
                    let mut i = 1;
                    while i < parts.len() {
                        if (parts[i] == "--runtime" || parts[i] == "-r") && i + 1 < parts.len() {
                            runtime = parts[i + 1];
                            i += 2;
                        } else if parts[i] == "--global" || parts[i] == "-g" {
                            global = true;
                            i += 1;
                        } else {
                            i += 1;
                        }
                    }
                    match crate::tools::minikit::scaffolder::MiniKitScaffolder::create_custom_stack(
                        &self.workspace_root,
                        stack_name,
                        runtime,
                        global,
                    ) {
                        Ok(p) => {
                            self.timeline.add_status(format!(
                                "✔ Created custom stack template `{}` at `{}`\n💡 Edit this JSON file to customize template files, dependencies, and architecture.",
                                stack_name,
                                p.display()
                            ));
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Failed to create custom stack: {}", e));
                        }
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower == "skills" || sub_lower == "skill list" {
                let list = crate::tools::minikit::skills::MiniKitSkillsManager::list_skills(
                    &self.workspace_root,
                );
                self.timeline.add_status(list);
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("skill remove ") || sub_lower.starts_with("skill rm ") {
                let skill_name = sub
                    .strip_prefix("skill remove ")
                    .or_else(|| sub.strip_prefix("skill rm "))
                    .unwrap_or("")
                    .trim();
                if skill_name.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit skill remove <skill-name>`".to_string());
                } else {
                    match crate::tools::minikit::skills::MiniKitSkillsManager::remove_skill(
                        &self.workspace_root,
                        skill_name,
                    ) {
                        Ok(msg) => self.timeline.add_status(format!("✔ {}", msg)),
                        Err(e) => self.timeline.add_status(format!(
                            "✗ Failed to remove skill `{}`: {}",
                            skill_name, e
                        )),
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("skill install ") || sub_lower.starts_with("skill add ")
            {
                let skill_name = sub
                    .strip_prefix("skill install ")
                    .or_else(|| sub.strip_prefix("skill add "))
                    .unwrap_or("")
                    .trim();
                if skill_name.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit skill install <skill-name>`".to_string());
                } else {
                    match crate::tools::minikit::skills::MiniKitSkillsManager::install_skill(
                        &self.workspace_root,
                        skill_name,
                    ) {
                        Ok(msg) => self.timeline.add_status(format!("✔ {}", msg)),
                        Err(e) => self.timeline.add_status(format!(
                            "✗ Failed to install skill `{}`: {}",
                            skill_name, e
                        )),
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("skill match ") {
                let file_path = sub.strip_prefix("skill match ").unwrap_or("").trim();
                if file_path.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit skill match <file-path>`".to_string());
                } else {
                    let out =
                        crate::tools::minikit::skills::MiniKitSkillsManager::format_skill_matches(
                            &self.workspace_root,
                            file_path,
                        );
                    self.timeline.add_status(out);
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("skill ") {
                let skill_name = sub.strip_prefix("skill ").unwrap_or("").trim();
                match crate::tools::minikit::skills::MiniKitSkillsManager::show_skill(
                    &self.workspace_root,
                    skill_name,
                ) {
                    Ok(content) => {
                        self.timeline.add_status(content);
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("✗ Failed to show skill `{}`: {}", skill_name, e));
                    }
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower == "sync" {
                match crate::tools::minikit::sync::MiniKitSyncEngine::sync(&self.workspace_root) {
                    Ok(msg) => self.timeline.add_status(format!("✔ {}", msg)),
                    Err(e) => self.timeline.add_status(format!("✗ Sync failed: {}", e)),
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower == "doctor" {
                let diag = crate::tools::minikit::doctor::MiniKitDoctor::diagnose();
                self.timeline.add_status(diag);
                return Ok(CommandAction::Continue);
            } else if sub_lower == "diff" {
                match crate::tools::minikit::diff::diff_stack(&self.workspace_root, None, false) {
                    Ok(res) => self.timeline.add_status(res.format_report()),
                    Err(e) => self
                        .timeline
                        .add_status(format!("✗ Drift check failed: {}", e)),
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower == "heal" || sub_lower == "diff --apply" {
                match crate::tools::minikit::diff::diff_stack(&self.workspace_root, None, true) {
                    Ok(res) => self.timeline.add_status(res.format_report()),
                    Err(e) => self
                        .timeline
                        .add_status(format!("✗ Drift self-healing failed: {}", e)),
                }
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("add ") {
                let pkg_name = sub.strip_prefix("add ").unwrap_or("").trim();
                if pkg_name.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit add <package-name>`".to_string());
                    return Ok(CommandAction::Continue);
                }
                let add_prompt = format!(
                    "Use MiniKit dependency intelligence to verify and add package `{}` to the project: call `kit_info` to inspect metadata and `kit_add` to update dependencies.",
                    pkg_name
                );
                self.timeline.add_user_message(prompt.to_string());
                self.is_working = true;
                self.current_activity = Some(crate::ui::AgentActivity::ExecutingCommand {
                    command: format!("kit add {}", pkg_name),
                });
                self.work_start = Some(Instant::now());
                let cancel = tokio_util::sync::CancellationToken::new();
                self.cancel_token = Some(cancel.clone());
                let _ = control_tx.send(AgentCommand::Prompt(add_prompt, Some(cancel)));
                return Ok(CommandAction::Continue);
            } else if sub_lower.starts_with("remove ") || sub_lower.starts_with("rm ") {
                let pkg_name = sub
                    .strip_prefix("remove ")
                    .or_else(|| sub.strip_prefix("rm "))
                    .unwrap_or("")
                    .trim();
                if pkg_name.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit remove <package-name>`".to_string());
                } else {
                    let registry = crate::tools::minikit::pkg::PkgRegistry::new();
                    match registry.remove_from_project(&self.workspace_root, pkg_name, None) {
                        Ok(msg) => self.timeline.add_status(format!("✔ {}", msg)),
                        Err(e) => self.timeline.add_status(format!(
                            "✗ Failed to remove package `{}`: {}",
                            pkg_name, e
                        )),
                    }
                }
                return Ok(CommandAction::Continue);
            }
        }

        if prompt == "/explore" || prompt.starts_with("/explore ") {
            let query = prompt.trim_start_matches("/explore").trim();
            if query.is_empty() {
                self.modal = ModalState::new_code_explorer(&self.workspace_root);
            } else {
                let explore_prompt = format!(
                    "Surgically explore the following symbol or architecture question in the codebase using `code_explore`: {}",
                    query
                );
                self.timeline.add_user_message(prompt.to_string());
                self.is_working = true;
                self.current_activity = Some(crate::ui::AgentActivity::RepoResearch {
                    target: query.to_string(),
                });
                self.work_start = Some(Instant::now());
                let cancel = tokio_util::sync::CancellationToken::new();
                self.cancel_token = Some(cancel.clone());
                let _ = control_tx.send(AgentCommand::Prompt(explore_prompt, Some(cancel)));
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/plan" || prompt.starts_with("/plan ") {
            let query = prompt.trim_start_matches("/plan").trim();
            let docs_dir = crate::tools::minikit::resolve_docs_dir(&self.workspace_root);
            let docs_name = docs_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(crate::constants::MINIKIT_DOCS_DIR);
            let plan_prompt = if query.is_empty() {
                format!(
                    "Inspect the current repository architecture and generate a structured, verifiable milestone implementation plan in {}/core/todo.md and {}/core/implementation.md.",
                    docs_name, docs_name
                )
            } else {
                format!(
                    "Plan and break down the following implementation into actionable verifiable tasks in {}/core/todo.md: {}",
                    docs_name, query
                )
            };
            self.timeline.add_user_message(prompt.to_string());
            self.is_working = true;
            self.current_activity = Some(crate::ui::AgentActivity::Thinking);
            self.work_start = Some(Instant::now());
            let cancel = tokio_util::sync::CancellationToken::new();
            self.cancel_token = Some(cancel.clone());
            let _ = control_tx.send(AgentCommand::Prompt(plan_prompt, Some(cancel)));
            return Ok(CommandAction::Continue);
        }

        if let Some(cmd) = parse_goal_command(prompt) {
            match cmd {
                GoalSubcommand::Show => {
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    if persistence_path.exists() {
                        match crate::context::memory::intent::IntentLedger::load_from_disk(
                            &persistence_path,
                        ) {
                            Ok(ledger) => {
                                self.timeline.add_status(format_ledger_timeline(&ledger));
                            }
                            Err(e) => {
                                self.timeline
                                    .add_status(format!("✗ Failed to load goal anchor: {}", e));
                            }
                        }
                    } else {
                        self.timeline.add_status(
                            "ℹ No active goal anchor found. Start a task or use /goal add <task>"
                                .to_string(),
                        );
                    }
                    return Ok(CommandAction::Continue);
                }
                GoalSubcommand::Add(text) => {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        self.timeline
                            .add_status("⚠️ Usage: /goal add <task description>".to_string());
                        return Ok(CommandAction::Continue);
                    }
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    let mut ledger = if persistence_path.exists() {
                        match crate::context::memory::intent::IntentLedger::load_from_disk(
                            &persistence_path,
                        ) {
                            Ok(l) => l,
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    "Failed to load existing intent ledger, initializing new one"
                                );
                                crate::context::memory::intent::IntentLedger::new(trimmed)
                            }
                        }
                    } else {
                        crate::context::memory::intent::IntentLedger::new(trimmed)
                    };
                    if ledger.root_objective.trim().is_empty() {
                        ledger.root_objective = trimmed.to_string();
                    }
                    let item_id = ledger.add_item(trimmed, None, vec![]);
                    match ledger.save_to_disk(&persistence_path) {
                        Ok(_) => {
                            self.timeline.add_status(format!(
                                "✔ Added requirement #{}: \"{}\" (id: {})",
                                ledger.items.len(),
                                trimmed,
                                item_id
                            ));
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Failed to save updated ledger: {}", e));
                        }
                    }
                    return Ok(CommandAction::Continue);
                }
                GoalSubcommand::Done(target) => {
                    let trimmed = target.trim();
                    if trimmed.is_empty() {
                        self.timeline.add_status(
                            "⚠️ Usage: /goal done <index_or_id> (e.g. /goal done 1)".to_string(),
                        );
                        return Ok(CommandAction::Continue);
                    }
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    if !persistence_path.exists() {
                        self.timeline.add_status(
                            "ℹ No active goal anchor found. Start a task or use /goal add <task>"
                                .to_string(),
                        );
                        return Ok(CommandAction::Continue);
                    }
                    let mut ledger =
                        match crate::context::memory::intent::IntentLedger::load_from_disk(
                            &persistence_path,
                        ) {
                            Ok(l) => l,
                            Err(e) => {
                                self.timeline
                                    .add_status(format!("✗ Failed to load intent ledger: {}", e));
                                return Ok(CommandAction::Continue);
                            }
                        };
                    let resolved = if let Ok(idx) = trimmed.parse::<usize>() {
                        if idx >= 1 && idx <= ledger.items.len() {
                            Some((
                                ledger.items[idx - 1].id.clone(),
                                ledger.items[idx - 1].title.clone(),
                            ))
                        } else {
                            self.timeline.add_status(format!(
                                "⚠️ Invalid requirement index {}. Ledger currently has {} item(s).",
                                idx,
                                ledger.items.len()
                            ));
                            return Ok(CommandAction::Continue);
                        }
                    } else {
                        ledger
                            .items
                            .iter()
                            .find(|item| item.id == trimmed || item.id.starts_with(trimmed))
                            .map(|item| (item.id.clone(), item.title.clone()))
                    };
                    match resolved {
                        Some((id, title)) => {
                            ledger.set_status(
                                &id,
                                crate::context::memory::intent::RequirementStatus::Completed,
                            );
                            match ledger.save_to_disk(&persistence_path) {
                                Ok(_) => {
                                    self.timeline.add_status(format!(
                                        "✔ Marked requirement \"{}\" as completed ({}/{} completed)",
                                        title,
                                        ledger.completed_count(),
                                        ledger.total_count()
                                    ));
                                }
                                Err(e) => {
                                    self.timeline.add_status(format!(
                                        "✗ Failed to save updated ledger: {}",
                                        e
                                    ));
                                }
                            }
                        }
                        None => {
                            self.timeline.add_status(format!(
                                "⚠️ No requirement found matching '{}'. Use /goal to inspect active items.",
                                trimmed
                            ));
                        }
                    }
                    return Ok(CommandAction::Continue);
                }
                GoalSubcommand::Reset => {
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    if persistence_path.exists() {
                        match std::fs::remove_file(&persistence_path) {
                            Ok(_) => {
                                self.timeline.add_status(
                                    "✔ Reset goal anchor and execution ledger.".to_string(),
                                );
                            }
                            Err(e) => {
                                self.timeline
                                    .add_status(format!("✗ Failed to remove intent anchor: {}", e));
                            }
                        }
                    } else {
                        self.timeline.add_status(
                            "ℹ No active goal anchor or execution ledger found to reset."
                                .to_string(),
                        );
                    }
                    return Ok(CommandAction::Continue);
                }
                GoalSubcommand::Run(query) => {
                    let trimmed = query.trim();
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    if !trimmed.is_empty() {
                        let max_items = self.config.agent.intent.max_ledger_items;
                        let ledger = crate::context::memory::intent::IntentLedger::from_prompt(
                            trimmed, max_items,
                        );
                        if let Err(e) = ledger.save_to_disk(&persistence_path) {
                            tracing::warn!(
                                error = %e,
                                path = %persistence_path.display(),
                                "Failed to persist goal intent ledger"
                            );
                        }
                    }
                    let docs_dir = crate::tools::minikit::resolve_docs_dir(&self.workspace_root);
                    let docs_name = docs_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(crate::constants::MINIKIT_DOCS_DIR);
                    let goal_prompt = if trimmed.is_empty() {
                        format!("<!-- GOAL --> Execute all pending tasks in {}/core/todo.md autonomously. Run verifications after each step and continue until all tasks are marked [x].", docs_name)
                    } else {
                        format!(
                            "<!-- GOAL --> Execute the following goal autonomously to completion: {}\nUpdate {}/core/todo.md, execute step-by-step, verify with tests, and do not stop until fully achieved.",
                            trimmed, docs_name
                        )
                    };
                    self.timeline.add_user_message(prompt.to_string());
                    self.last_user_prompt = Some(prompt.to_string());
                    self.is_working = true;
                    self.current_activity = Some(crate::ui::AgentActivity::Thinking);
                    self.work_start = Some(Instant::now());
                    let cancel = tokio_util::sync::CancellationToken::new();
                    self.cancel_token = Some(cancel.clone());
                    let _ = control_tx.send(AgentCommand::Prompt(goal_prompt, Some(cancel)));
                    return Ok(CommandAction::Continue);
                }
            }
        }

        if prompt == "/diff" || prompt == "/diffs" {
            let ws = self.workspace_root.clone();
            match crate::git::GitDiffViewer::load_diffs(&ws, false).await {
                Ok(diff_files) => {
                    self.modal = ModalState::new_git_diff(diff_files, false);
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Failed to load git diff: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/review" || prompt.starts_with("/review ") {
            let staged_only = prompt
                .split_whitespace()
                .any(|w| w == "--staged" || w == "-s");
            self.timeline.add_user_message(prompt.to_string());
            self.timeline
                .add_status("🛡️ Running multi-agent adversarial code review...".to_string());

            match crate::git::GitReviewer::review_workspace(&self.workspace_root, staged_only).await
            {
                Ok(report) => {
                    let formatted = crate::git::GitReviewer::format_report(&report);
                    self.timeline
                        .entries
                        .push(crate::ui::view::TimelineEntry::AssistantMarkdown(formatted));
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Code review error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/power" || prompt.starts_with("/power ") {
            let arg = prompt.trim_start_matches("/power").trim();
            self.timeline.add_user_message(prompt.to_string());
            if arg.is_empty() || arg == "status" || arg == "modal" {
                self.modal = ModalState::new_minipower(&self.workspace_root);
            } else if let Some(task_prompt) = arg.strip_prefix("task") {
                let task_prompt = task_prompt.trim();
                if task_prompt.is_empty() {
                    self.timeline.add_status(
                        "⚠️ Please provide a task description. Usage: `/power task <prompt>`"
                            .to_string(),
                    );
                } else {
                    self.timeline.add_status(format!(
                        "⚡ Launching isolated MiniPower worktree task: '{}'...",
                        task_prompt
                    ));
                    let ws = self.workspace_root.clone();
                    let t_prompt = task_prompt.to_string();
                    let task_item = crate::agent::subagent::fanout::FanoutTaskItem {
                        task: t_prompt,
                        role: crate::agent::subagent::types::SubagentRole::Coder,
                        workspace_mode: Some(
                            crate::agent::subagent::types::WorkspaceMode::Worktree,
                        ),
                        max_iterations: Some(15),
                        check_cmd: None,
                    };
                    match crate::agent::subagent::fanout::FanoutOrchestrator::execute_fanout(
                        &ws,
                        vec![task_item],
                        crate::agent::subagent::fanout::FanoutJoinMode::All,
                        true,
                        1,
                    )
                    .await
                    {
                        Ok(report) => {
                            self.timeline
                                .entries
                                .push(crate::ui::view::TimelineEntry::AssistantMarkdown(report));
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ MiniPower worktree task failed: {}", e));
                        }
                    }
                }
            } else if let Some(topic) = arg.strip_prefix("brainstorm") {
                let topic = topic.trim();
                if topic.is_empty() {
                    self.timeline.add_status(
                        "⚠️ Please provide a topic to brainstorm. Usage: `/power brainstorm <feature or idea>`"
                            .to_string(),
                    );
                } else {
                    let prompt_text =
                        crate::agent::minipower::MiniPowerEngine::format_brainstorm_prompt(
                            &self.workspace_root,
                            topic,
                        );
                    self.is_working = true;
                    self.current_activity = Some(crate::ui::AgentActivity::Thinking);
                    self.work_start = Some(Instant::now());
                    let cancel = tokio_util::sync::CancellationToken::new();
                    self.cancel_token = Some(cancel.clone());
                    let _ = control_tx.send(AgentCommand::Prompt(prompt_text, Some(cancel)));
                }
            } else if let Some(topic) = arg.strip_prefix("plan") {
                let topic = topic.trim();
                if topic.is_empty() {
                    self.timeline.add_status(
                        "⚠️ Please provide a topic to plan. Usage: `/power plan <feature or task>`"
                            .to_string(),
                    );
                } else {
                    let prompt_text = crate::agent::minipower::MiniPowerEngine::format_plan_prompt(
                        &self.workspace_root,
                        topic,
                    );
                    self.is_working = true;
                    self.current_activity = Some(crate::ui::AgentActivity::Thinking);
                    self.work_start = Some(Instant::now());
                    let cancel = tokio_util::sync::CancellationToken::new();
                    self.cancel_token = Some(cancel.clone());
                    let _ = control_tx.send(AgentCommand::Prompt(prompt_text, Some(cancel)));
                }
            } else if arg == "review" || arg.starts_with("review ") {
                let staged_only = arg.split_whitespace().any(|w| w == "--staged" || w == "-s");
                self.timeline
                    .add_status("🛡️ Running multi-agent adversarial code review...".to_string());
                match crate::git::GitReviewer::review_workspace(&self.workspace_root, staged_only)
                    .await
                {
                    Ok(report) => {
                        let formatted = crate::git::GitReviewer::format_report(&report);
                        self.timeline
                            .entries
                            .push(crate::ui::view::TimelineEntry::AssistantMarkdown(formatted));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("✗ Code review error: {}", e));
                    }
                }
            } else if arg == "verify" {
                self.timeline.add_status(
                    "⚡ Running 4-Gate Pre-Completion Verification Barrier...".to_string(),
                );
                let git = crate::git::GitService::new(self.workspace_root.clone());
                let modified_files = if git.is_git_repo().await {
                    if let Ok(st) = git.get_status().await {
                        let mut all = st.staged;
                        all.extend(st.unstaged);
                        all.sort();
                        all.dedup();
                        all
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };
                let report = crate::agent::verification_barrier::VerificationBarrier::verify(
                    &self.workspace_root,
                    &modified_files,
                )
                .await;
                let formatted = report.format_report();
                self.timeline
                    .entries
                    .push(crate::ui::view::TimelineEntry::AssistantMarkdown(formatted));
            } else {
                self.timeline.add_status(
                    "💡 MiniPower Usage:\n  • `/power` or `/power status` — Interactive methodology & verification modal\n  • `/power task <prompt>` — Execute mutating task in isolated Git worktree\n  • `/power brainstorm <topic>` — Socratic brainstorm & spec refinement\n  • `/power plan <topic>` — Structured implementation plan builder\n  • `/power review [--staged]` — Adversarial multi-agent code review\n  • `/power verify` — 4-Gate pre-completion verification barrier".to_string(),
                );
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/model" || prompt == "/models" || prompt == "/provider" {
            self.modal = ModalState::new_provider_select();
            return Ok(CommandAction::Continue);
        }

        if prompt == "/undo" {
            let backup_mgr = crate::session::backup::BackupManager::new(&self.workspace_root);
            let checkpoints = backup_mgr.list_checkpoints();
            if checkpoints.is_empty() {
                self.timeline
                    .add_status("ℹ No recorded checkpoints available to undo".to_string());
            } else {
                self.modal = ModalState::new_undo_checkpoint(checkpoints);
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/theme" || prompt == "/themes" {
            self.modal =
                ModalState::new_theme_select(&self.config.ui.theme, &self.config.ui.animation);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/context" || prompt == "/ctx" || prompt == "/kv" || prompt == "/cache" {
            let data = crate::ui::modals::context_diagnostics::ContextDiagnosticsData::gather(
                &self.workspace_root,
                &self.config,
                self.last_turn_tokens,
                self.cumulative_tokens,
                self.last_turn_cached_tokens,
                self.timeline.entries.len(),
            );
            self.modal = ModalState::new_context_diagnostics(data);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/tokens" {
            let model_limit =
                crate::agent::models::get_model_context_limit(&self.config.provider.model);
            let card = format!(
                "📊 Token & Context Metrics:\n  • Provider: {}\n  • Active Model: {}\n  • Context Limit: {} tokens\n  • Last Turn Usage: {} tokens\n  • Compaction Threshold: {:.0}%",
                self.config.provider.default,
                self.config.provider.model,
                model_limit,
                self.last_turn_tokens,
                self.config.agent.warning_threshold * 100.0
            );
            self.timeline.add_status(card);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/streaming" {
            self.modal = ModalState::new_streaming_select(self.config.agent.streaming);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/parallel" || prompt.starts_with("/parallel ") {
            let args = prompt.strip_prefix("/parallel").unwrap_or("").trim();
            if args.is_empty() {
                let status_msg = format!(
                    "⚡ **Speculative Parallel Tool Execution Pipeline**\n\
                     • **Parallel Tools**: {}\n\
                     • **Speculative Pre-Execution**: {}\n\
                     • **Max Concurrency**: {} threads\n\
                     • **Safety Engine**: Barrier isolation (Mutating & Barrier tools serialized)\n\n\
                     *Available commands:*\n\
                     • `/parallel on` — Enable parallel read-only tool execution\n\
                     • `/parallel off` — Disable parallel execution (strictly sequential)\n\
                     • `/parallel speculative on` — Enable streaming speculative pre-execution\n\
                     • `/parallel speculative off` — Disable streaming speculative pre-execution\n\
                     • `/parallel <1-16>` — Set maximum parallel worker limit",
                    if self.config.agent.parallel_tools { "ENABLED" } else { "DISABLED" },
                    if self.config.agent.speculative_execution { "ENABLED (Streaming Overlap)" } else { "DISABLED" },
                    self.config.agent.max_parallel_tools,
                );
                self.timeline.add_status(status_msg);
            } else {
                match args.to_lowercase().as_str() {
                    "on" | "enable" | "true" | "1" => {
                        self.config.agent.parallel_tools = true;
                        self.timeline
                            .add_status("⚡ Parallel tool execution enabled.".to_string());
                    }
                    "off" | "disable" | "false" | "0" => {
                        self.config.agent.parallel_tools = false;
                        self.timeline.add_status(
                            "⚡ Parallel tool execution disabled (strictly sequential)."
                                .to_string(),
                        );
                    }
                    "speculative on" | "speculative true" => {
                        self.config.agent.speculative_execution = true;
                        self.timeline.add_status(
                            "⚡ Speculative streaming pre-execution enabled.".to_string(),
                        );
                    }
                    "speculative off" | "speculative false" => {
                        self.config.agent.speculative_execution = false;
                        self.timeline.add_status(
                            "⚡ Speculative streaming pre-execution disabled.".to_string(),
                        );
                    }
                    other => {
                        if let Ok(num) = other.parse::<usize>() {
                            let clamped = num.clamp(
                                crate::constants::MIN_PARALLEL_TOOLS,
                                crate::constants::MAX_PARALLEL_TOOLS_CAP,
                            );
                            self.config.agent.max_parallel_tools = clamped;
                            self.timeline.add_status(format!(
                                "⚡ Maximum parallel tools limit set to {} concurrent workers.",
                                clamped
                            ));
                        } else {
                            self.timeline.add_status(format!(
                                "⚠️ Invalid argument '{}'. Use `on`, `off`, `speculative on`, `speculative off`, or a number (1-16).",
                                other
                            ));
                        }
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/tx"
            || prompt.starts_with("/tx ")
            || prompt == "/transaction"
            || prompt.starts_with("/transaction ")
        {
            let remainder = if let Some(r) = prompt.strip_prefix("/transaction") {
                r.trim()
            } else {
                prompt.strip_prefix("/tx").unwrap_or("").trim()
            };

            match remainder.to_lowercase().as_str() {
                "" | "status" => {
                    match crate::session::transaction::TransactionManager::status(
                        &self.workspace_root,
                        None,
                    ) {
                        Ok(Some(receipt)) => {
                            self.timeline.add_status(receipt.format_receipt());
                        }
                        Ok(None) => {
                            self.timeline.add_status("ℹ No active workspace transaction. Workspace is in direct modification mode.\n\nUse `/tx begin <description>` to start an atomic transaction.".to_string());
                        }
                        Err(e) => {
                            self.timeline.add_status(format!(
                                "✗ Error retrieving transaction status: {}",
                                e
                            ));
                        }
                    }
                }
                "commit" => {
                    match crate::session::transaction::TransactionManager::commit(
                        &self.workspace_root,
                        None,
                    ) {
                        Ok(receipt) => {
                            self.timeline.add_status(receipt.format_receipt());
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Transaction commit failed: {}", e));
                        }
                    }
                }
                "rollback" => {
                    match crate::session::transaction::TransactionManager::rollback(
                        &self.workspace_root,
                        None,
                        Some("Manual user rollback from /tx"),
                    ) {
                        Ok(receipt) => {
                            self.timeline.add_status(receipt.format_receipt());
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Transaction rollback failed: {}", e));
                        }
                    }
                }
                other if other.starts_with("begin ") => {
                    let desc = other.strip_prefix("begin ").unwrap_or("").trim();
                    match crate::session::transaction::TransactionManager::begin(
                        &self.workspace_root,
                        desc,
                    ) {
                        Ok(manifest) => {
                            self.timeline.add_status(format!(
                                "✔ Began atomic workspace transaction '{}' for: {}\nAll subsequent file modifications will be journaled in the WAL and can be atomically rolled back.",
                                manifest.tx_id, manifest.description
                            ));
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("✗ Failed to begin transaction: {}", e));
                        }
                    }
                }
                _ => {
                    self.timeline.add_status(
                        "📦 **Workspace Transaction Commands:**\n\
                         • `/tx` — Show active transaction status and affected files\n\
                         • `/tx begin <desc>` — Start a new atomic workspace transaction\n\
                         • `/tx commit` — Commit active transaction and seal WAL journal\n\
                         • `/tx rollback` — Revert all modified/created/deleted files in transaction"
                            .to_string(),
                    );
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/dag" || prompt.starts_with("/dag ") {
            self.timeline.add_status(
                "⚡ **Dynamic Execution DAG & JSONPath Pipelining (`execute_dag`)**\n\
                 Compose multiple dependent tool calls into an atomic, wave-scheduled DAG.\n\n\
                 **Key Features:**\n\
                 • **Topological Waves**: Independent nodes execute concurrently in parallel waves\n\
                 • **JSONPath Pipelining**: Reference upstream outputs via `$node_id.path` or `${node_id.path}`\n\
                 • **Failure Isolation**: Upstream node failures automatically skip downstream dependents (`SkippedDependencyFailed`)\n\
                 • **Cycle Protection**: Strict Kahn's algorithm cycle detection\n\n\
                 **Example Tool Call Schema:**\n\
                 ```json\n\
                 {\n  \"name\": \"locate_and_read\",\n  \"nodes\": [\n    {\n      \"id\": \"locate\",\n      \"tool\": \"locate_fault\",\n      \"args\": { \"query\": \"auth_handler\" }\n    },\n    {\n      \"id\": \"read\",\n      \"tool\": \"read_file\",\n      \"args\": { \"path\": \"$locate.matches[0].path\", \"start_line\": \"$locate.matches[0].line\" },\n      \"depends_on\": [\"locate\"]\n    }\n  ]\n}\n\
                 ```".to_string()
            );
            return Ok(CommandAction::Continue);
        }

        if prompt == "/heal" || prompt.starts_with("/heal ") {
            let args = prompt.strip_prefix("/heal").unwrap_or("").trim();
            let dry_run = args == "dry" || args == "--dry-run";
            self.timeline.add_status(
                "⏳ Running workspace diagnostic triage & self-healing pass...".to_string(),
            );
            match crate::agent::self_healing::SelfHealingEngine::heal(
                &self.workspace_root,
                3,
                true,
                dry_run,
            )
            .await
            {
                Ok(report) => {
                    self.timeline
                        .add_status(report.format_summary(&self.workspace_root));
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Self-healing diagnostic error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/sandbox"
            || prompt.starts_with("/sandbox ")
            || prompt == "/sb"
            || prompt.starts_with("/sb ")
        {
            let raw_args = if let Some(stripped) = prompt.strip_prefix("/sandbox") {
                stripped.trim()
            } else {
                prompt.strip_prefix("/sb").unwrap_or("").trim()
            };

            if raw_args.is_empty() {
                let bwrap_status = if crate::sandbox::is_bwrap_available() {
                    "Bubblewrap (unprivileged namespaces) AVAILABLE"
                } else {
                    "Bubblewrap not installed (using Landlock / process isolation)"
                };
                self.timeline.add_status(format!(
                    "🛡️ **Dynamic Code Sandbox (`/sandbox`)**\n\
                     • **Active Backend**: {}\n\
                     • **Default Isolation**: Network BLOCKED, Workspace Read-Write, Ephemeral Off\n\
                     • **Usage Options**:\n\
                       - `/sandbox <cmd>`: Run command with network isolated\n\
                       - `/sandbox --net <cmd>`: Allow network access\n\
                       - `/sandbox --ro <cmd>`: Read-only workspace protection\n\
                       - `/sandbox --ephemeral <cmd>`: Discard all disk writes upon exit",
                    bwrap_status
                ));
                return Ok(CommandAction::Continue);
            }

            let mut policy = crate::sandbox::SandboxPolicy::default();
            let mut cmd_to_run = raw_args;
            loop {
                if let Some(rest) = cmd_to_run.strip_prefix("--net ") {
                    policy.allow_network = true;
                    cmd_to_run = rest.trim();
                } else if let Some(rest) = cmd_to_run.strip_prefix("--ro ") {
                    policy.read_only_workspace = true;
                    cmd_to_run = rest.trim();
                } else if let Some(rest) = cmd_to_run.strip_prefix("--ephemeral ") {
                    policy.ephemeral_overlay = true;
                    cmd_to_run = rest.trim();
                } else {
                    break;
                }
            }

            self.timeline.add_status(format!(
                "⏳ Running sandboxed command: `{}` (net: {}, ro: {}, ephemeral: {})...",
                cmd_to_run,
                policy.allow_network,
                policy.read_only_workspace,
                policy.ephemeral_overlay
            ));

            match crate::sandbox::run_sandboxed(&self.workspace_root, cmd_to_run, &policy).await {
                Ok(res) => {
                    self.timeline
                        .add_status(crate::sandbox::format_sandbox_result(&res));
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Sandbox execution error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/retrieve"
            || prompt.starts_with("/retrieve ")
            || prompt == "/hr"
            || prompt.starts_with("/hr ")
        {
            let query = if let Some(stripped) = prompt.strip_prefix("/retrieve") {
                stripped.trim()
            } else {
                prompt.strip_prefix("/hr").unwrap_or("").trim()
            };

            if query.is_empty() {
                self.timeline.add_status(
                    "ℹ **Usage**: `/retrieve <query>` (e.g. `/retrieve transaction rollback` or `/retrieve authentication`).\n\
                     Executes multi-modal fusion across AST CodeGraph, BM25, semantic vectors, wiki, and episodic memory.".to_string()
                );
                return Ok(CommandAction::Continue);
            }

            self.timeline.add_status(format!(
                "🔍 Retrieving multi-modal knowledge for: `{}`...",
                query
            ));
            match crate::context::fusion::KnowledgeFusionEngine::retrieve(
                &self.workspace_root,
                query,
                5,
                true,
                true,
                true,
            ) {
                Ok(bundle) => {
                    self.timeline
                        .add_status(crate::context::fusion::format_fused_bundle(&bundle));
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Knowledge retrieval error: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/route"
            || prompt.starts_with("/route ")
            || prompt == "/ro"
            || prompt.starts_with("/ro ")
        {
            let arg = if let Some(stripped) = prompt.strip_prefix("/route") {
                stripped.trim()
            } else {
                prompt.strip_prefix("/ro").unwrap_or("").trim()
            };

            let mut router = crate::agent::router::AdaptiveModelRouter::new();

            if arg.is_empty() || arg == "status" {
                self.timeline.add_status(router.format_status_report());
            } else if arg.eq_ignore_ascii_case("auto") {
                router.set_forced_tier(None);
                self.timeline.add_status(
                    "⚡ **Model Router Mode**: Set to Adaptive Auto-Routing (Complexity-Driven)."
                        .to_string(),
                );
            } else if let Some(tier) = crate::agent::router::ModelTier::parse_tier(arg) {
                router.set_forced_tier(Some(tier));
                self.timeline.add_status(format!(
                    "🔒 **Model Router Mode**: Manually locked to `{}` tier ({}).\nRun `/route auto` to restore adaptive mode.",
                    tier.badge(),
                    tier.description()
                ));
            } else {
                let decision = router.route_turn(arg, 0, false, 0, 0);
                let mut out = format!(
                    "# 🔀 Model Routing Assessment: {}\n\n",
                    decision.tier.badge()
                );
                out.push_str(&format!("📋 **Query/Task:** `{}`\n", arg));
                out.push_str(&format!(
                    "🎯 **Recommended Tier:** `{}` ({})\n",
                    decision.tier.badge(),
                    decision.tier.description()
                ));
                out.push_str(&format!("💡 **Reasoning:** {}\n", decision.reason));
                out.push_str(&format!(
                    "💰 **Relative Cost Factor:** {:.2}x\n\n",
                    decision.estimated_cost_factor
                ));
                out.push_str(&format!(
                    "🚀 **Primary Target:** `{}` / `{}` (Priority: {}, Context: {}k)\n\n",
                    decision.primary_endpoint.provider_name,
                    decision.primary_endpoint.model_name,
                    decision.primary_endpoint.priority,
                    decision.primary_endpoint.max_context / 1000,
                ));
                if !decision.fallback_chain.is_empty() {
                    out.push_str("🛡️ **Fallback Failover Chain:**\n");
                    for (idx, fb) in decision.fallback_chain.iter().enumerate() {
                        out.push_str(&format!(
                            "{}. `{}` / `{}` (Priority: {}, Context: {}k{})\n",
                            idx + 1,
                            fb.provider_name,
                            fb.model_name,
                            fb.priority,
                            fb.max_context / 1000,
                            if fb.is_local { ", Local" } else { "" }
                        ));
                    }
                }
                self.timeline.add_status(out);
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/quarantine"
            || prompt.starts_with("/quarantine ")
            || prompt == "/q"
            || prompt.starts_with("/q ")
        {
            let arg = if let Some(stripped) = prompt.strip_prefix("/quarantine") {
                stripped.trim()
            } else {
                prompt.strip_prefix("/q").unwrap_or("").trim()
            };

            if arg.is_empty() || arg == "list" || arg == "status" {
                let store = crate::context::flaky::QuarantineManager::load(&self.workspace_root);
                self.timeline
                    .add_status(crate::context::flaky::QuarantineManager::format_report(
                        &store,
                    ));
            } else if arg == "clear" {
                match crate::context::flaky::QuarantineManager::clear(&self.workspace_root) {
                    Ok(count) => {
                        self.timeline.add_status(format!(
                            "✅ Cleared {} quarantined test(s). All tests restored to active test runs.",
                            count
                        ));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("❌ Failed to clear quarantine: {}", e));
                    }
                }
            } else if let Some(target) = arg
                .strip_prefix("remove ")
                .or_else(|| arg.strip_prefix("rm "))
            {
                let test_name = target.trim();
                match crate::context::flaky::QuarantineManager::unquarantine(
                    &self.workspace_root,
                    test_name,
                ) {
                    Ok(true) => {
                        self.timeline.add_status(format!(
                            "✅ Removed `{}` from quarantine. It will now execute in standard test suites.",
                            test_name
                        ));
                    }
                    Ok(false) => {
                        self.timeline.add_status(format!(
                            "ℹ `{}` was not found in active quarantine list.",
                            test_name
                        ));
                    }
                    Err(e) => {
                        self.timeline.add_status(format!(
                            "❌ Failed to remove `{}` from quarantine: {}",
                            test_name, e
                        ));
                    }
                }
            } else if let Some(target) = arg.strip_prefix("add ") {
                let parts: Vec<&str> = target.splitn(2, ' ').collect();
                let test_name = parts[0].trim();
                let reason = if parts.len() > 1 {
                    parts[1].trim()
                } else {
                    "Manual quarantine via /quarantine command"
                };
                match crate::context::flaky::QuarantineManager::quarantine(
                    &self.workspace_root,
                    test_name,
                    1.0,
                    crate::context::flaky::FlakySignature::Unknown,
                    reason,
                    1,
                ) {
                    Ok(_) => {
                        self.timeline.add_status(format!(
                            "🛡️ Successfully quarantined `{}`.\nReason: {}",
                            test_name, reason
                        ));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("❌ Failed to quarantine `{}`: {}", test_name, e));
                    }
                }
            } else {
                let trimmed = arg.strip_prefix("detect ").unwrap_or(arg).trim();
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                let test_name = parts.first().copied().unwrap_or("");
                let runs = parts
                    .get(1)
                    .and_then(|r| r.parse::<usize>().ok())
                    .unwrap_or(crate::constants::DEFAULT_FLAKY_RUNS);

                if test_name.is_empty() {
                    self.timeline.add_status(
                        "❌ Please specify a test name to analyze: `/quarantine detect <test_name> [runs]`"
                            .to_string(),
                    );
                } else {
                    self.timeline.add_status(format!(
                        "🔬 Initiating {} burn-in executions for test `{}`...",
                        runs, test_name
                    ));
                    match crate::context::flaky::FlakyTestDetector::execute_burn_in(
                        &self.workspace_root,
                        test_name,
                        runs,
                        crate::constants::FLAKY_TEST_TIMEOUT_SECS,
                    )
                    .await
                    {
                        Ok(report) => {
                            let mut out = report.format_markdown();
                            if report.verdict
                                == crate::context::flaky::FlakinessVerdict::FlakyIntermittent
                            {
                                let _ = crate::context::flaky::QuarantineManager::quarantine(
                                    &self.workspace_root,
                                    test_name,
                                    report.flakiness_ratio,
                                    report.signature,
                                    &format!(
                                        "Automated quarantine: {:.1}% failure variance across {} burn-in runs",
                                        report.flakiness_ratio * 100.0,
                                        report.total_runs
                                    ),
                                    report.total_runs,
                                );
                                out.push_str(&format!(
                                    "\n\n🛡️ **Auto-Quarantine Applied**: Test `{}` has been quarantined in `.minicode/quarantine.json`.",
                                    test_name
                                ));
                            }
                            self.timeline.add_status(out);
                        }
                        Err(e) => {
                            self.timeline
                                .add_status(format!("❌ Burn-in analysis failed: {}", e));
                        }
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/commit"
            || prompt.starts_with("/commit ")
            || prompt == "/ci"
            || prompt.starts_with("/ci ")
        {
            let arg = if let Some(stripped) = prompt.strip_prefix("/commit") {
                stripped.trim()
            } else if let Some(stripped) = prompt.strip_prefix("/ci") {
                stripped.trim()
            } else {
                ""
            };

            if arg == "help" {
                let help_msg = "📝 **Semantic Commit Synthesis & Changelog Commands**\n\n\
                    • `/commit` or `/commit preview [task hint]` — Synthesize conventional commit proposals & atomic sequence\n\
                    • `/commit now [task hint]` or `/commit -y` — Synthesize message and immediately commit working tree changes\n\
                    • `/commit changelog [version]` — Synthesize Keep-a-Changelog release draft from current diff\n\
                    • `/commit help` — Display this usage reference";
                self.timeline.add_status(help_msg.to_string());
                return Ok(CommandAction::Continue);
            }

            if arg == "changelog" || arg.starts_with("changelog ") {
                let ver = arg.strip_prefix("changelog").unwrap_or("").trim();
                let version = if ver.is_empty() { None } else { Some(ver) };
                match crate::git::commit_synth::SemanticCommitSynthesizer::synthesize(
                    &self.workspace_root,
                    None,
                    None,
                )
                .await
                {
                    Ok(mut report) => {
                        if let Some(v) = version {
                            report.changelog_markdown = crate::git::commit_synth::SemanticCommitSynthesizer::generate_changelog_draft(
                                report.unified_proposal.commit_type,
                                &report.unified_proposal.summary,
                                &report.affected_files,
                                &report.unified_proposal.scope,
                                Some(v),
                            );
                        }
                        self.timeline.add_status(format!(
                            "📜 **Keep-a-Changelog Release Draft**:\n\n```markdown\n{}\n```",
                            report.changelog_markdown
                        ));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("❌ Failed to synthesize changelog: {}", e));
                    }
                }
                return Ok(CommandAction::Continue);
            }

            if arg == "now" || arg.starts_with("now ") || arg == "-y" || arg.starts_with("-y ") {
                let task_hint = if let Some(stripped) = arg.strip_prefix("now") {
                    let s = stripped.trim();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else if let Some(stripped) = arg.strip_prefix("-y") {
                    let s = stripped.trim();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else {
                    None
                };

                match crate::git::commit_synth::SemanticCommitSynthesizer::synthesize(
                    &self.workspace_root,
                    None,
                    task_hint,
                )
                .await
                {
                    Ok(report) => {
                        let commit_msg = report.unified_proposal.format_full_message();
                        match crate::git::commit_synth::SemanticCommitSynthesizer::execute_commit(
                            &self.workspace_root,
                            &commit_msg,
                            None,
                        )
                        .await
                        {
                            Ok(res) => {
                                self.timeline.add_status(format!(
                                    "✅ **Successfully Committed Working Tree Changes**\n\n{}\n\n**Commit Message:**\n```\n{}\n```",
                                    res,
                                    commit_msg
                                ));
                            }
                            Err(e) => {
                                self.timeline
                                    .add_status(format!("❌ Commit execution failed: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("❌ Commit synthesis failed: {}", e));
                    }
                }
                return Ok(CommandAction::Continue);
            }

            // Default or /commit preview [task_hint]
            let task_hint = if let Some(stripped) = arg.strip_prefix("preview") {
                let s = stripped.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            } else if arg.is_empty() {
                None
            } else {
                Some(arg)
            };

            match crate::git::commit_synth::SemanticCommitSynthesizer::synthesize(
                &self.workspace_root,
                None,
                task_hint,
            )
            .await
            {
                Ok(report) => {
                    self.timeline.add_status(report.format_markdown());
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("❌ Commit synthesis failed: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/thinking" || prompt.starts_with("/thinking ") {
            let args = prompt.strip_prefix("/thinking").unwrap_or("").trim();
            if args.is_empty() {
                let current_budget = match self.config.provider.thinking_budget {
                    Some(b) if b >= crate::constants::MIN_THINKING_BUDGET_TOKENS => {
                        format!("{} tokens (ENABLED)", b)
                    }
                    Some(b) => format!("{} tokens (below min threshold)", b),
                    None => "Disabled".to_string(),
                };
                let effort = self
                    .config
                    .provider
                    .reasoning_effort
                    .as_deref()
                    .unwrap_or("auto");
                let status_msg = format!(
                    "🧠 **Extended Thinking / Reasoning Configuration**\n\
                     • **Provider**: {}\n\
                     • **Model**: {}\n\
                     • **Thinking Budget**: {}\n\
                     • **Reasoning Effort**: {}\n\n\
                     *Available commands:*\n\
                     • `/thinking off` — Disable extended thinking\n\
                     • `/thinking 4k` — 4,096 tokens (fast reasoning)\n\
                     • `/thinking 8k` — 8,192 tokens (balanced reasoning)\n\
                     • `/thinking 16k` — 16,000 tokens (standard extended thinking)\n\
                     • `/thinking 32k` — 32,000 tokens (deep architectural refactors)\n\
                     • `/thinking 64k` — 64,000 tokens (maximum frontier budget)",
                    self.config.provider.default,
                    self.config.provider.model,
                    current_budget,
                    effort,
                );
                self.timeline.add_status(status_msg);
            } else {
                match args.to_lowercase().as_str() {
                    "off" | "disable" | "none" | "0" => {
                        self.config.provider.thinking_budget = None;
                        self.config.provider.reasoning_effort = None;
                        self.timeline.add_status(
                            "🧠 Extended thinking / test-time reasoning disabled.".to_string(),
                        );
                    }
                    "4k" => {
                        self.config.provider.thinking_budget = Some(4096);
                        self.config.provider.reasoning_effort = Some("low".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 4,096 tokens (effort: low)."
                                .to_string(),
                        );
                    }
                    "8k" => {
                        self.config.provider.thinking_budget = Some(8192);
                        self.config.provider.reasoning_effort = Some("medium".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 8,192 tokens (effort: medium)."
                                .to_string(),
                        );
                    }
                    "16k" | "default" => {
                        self.config.provider.thinking_budget = Some(16000);
                        self.config.provider.reasoning_effort = Some("medium".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 16,000 tokens (effort: medium)."
                                .to_string(),
                        );
                    }
                    "32k" => {
                        self.config.provider.thinking_budget = Some(32000);
                        self.config.provider.reasoning_effort = Some("high".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 32,000 tokens (effort: high)."
                                .to_string(),
                        );
                    }
                    "64k" | "max" => {
                        self.config.provider.thinking_budget = Some(64000);
                        self.config.provider.reasoning_effort = Some("high".to_string());
                        self.timeline.add_status(
                            "🧠 Extended thinking budget set to 64,000 tokens (effort: high)."
                                .to_string(),
                        );
                    }
                    other => {
                        if let Ok(num) = other.parse::<usize>() {
                            let clamped = num.clamp(
                                crate::constants::MIN_THINKING_BUDGET_TOKENS,
                                crate::constants::MAX_THINKING_BUDGET_TOKENS,
                            );
                            let effort = if clamped <= 4096 {
                                "low"
                            } else if clamped <= 16000 {
                                "medium"
                            } else {
                                "high"
                            };
                            self.config.provider.thinking_budget = Some(clamped);
                            self.config.provider.reasoning_effort = Some(effort.to_string());
                            self.timeline.add_status(format!(
                                "🧠 Extended thinking budget set to {} tokens (effort: {}).",
                                clamped, effort
                            ));
                        } else {
                            self.timeline.add_status(format!(
                                "⚠️ Invalid thinking budget '{}'. Use off, 4k, 8k, 16k, 32k, 64k, or a number between 1024 and 64000.",
                                other
                            ));
                        }
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        let is_analysis_keyword = prompt.eq_ignore_ascii_case("analyze the full project")
            || prompt.eq_ignore_ascii_case("analyze full project")
            || prompt.eq_ignore_ascii_case("analyze the project")
            || prompt.eq_ignore_ascii_case("analyze project")
            || prompt.eq_ignore_ascii_case("analyze the repo")
            || prompt.eq_ignore_ascii_case("analyze repo")
            || prompt.eq_ignore_ascii_case("analyze codebase")
            || prompt.eq_ignore_ascii_case("index the repository")
            || prompt.eq_ignore_ascii_case("index repository")
            || prompt.eq_ignore_ascii_case("index codebase")
            || prompt.eq_ignore_ascii_case("generate code graph")
            || prompt.eq_ignore_ascii_case("reindex")
            || prompt.eq_ignore_ascii_case("re-index")
            || prompt.eq_ignore_ascii_case("rebuild the graph");

        if prompt == "/init" || prompt == "/index" || prompt == "/analyze" || is_analysis_keyword {
            self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/map" {
            let mut graph = crate::context::graph::CodeGraph::new();
            if let Err(e) = graph.build_graph(&self.workspace_root) {
                self.timeline
                    .add_status(format!("✗ Failed to build repo map: {}", e));
            } else {
                let repomap =
                    graph.format_repomap(&self.workspace_root, &[], self.config.agent.map_tokens);
                self.timeline
                    .add_status(format!("🗺️ AST PageRank Repository Map:\n\n{}", repomap));
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/arch" || prompt == "/architecture" {
            match crate::context::governance::ArchitectureGovernor::scan_workspace(
                &self.workspace_root,
            ) {
                Ok(report) => {
                    self.modal = ModalState::new_architecture_audit(report);
                }
                Err(e) => {
                    self.timeline
                        .add_status(format!("✗ Failed to audit architecture: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/compact" {
            let model_limit =
                crate::agent::models::get_model_context_limit(&self.config.provider.model);
            let t1 = (model_limit as f64 * crate::constants::COMPACT_TIER1_RATIO) as usize;
            let t2 = (model_limit as f64 * crate::constants::COMPACT_TIER2_RATIO) as usize;
            let t3 = (model_limit as f64 * crate::constants::COMPACT_TIER3_RATIO) as usize;
            let status_msg = format!(
                "🗜️ **Context Window Status** (`{}`)\n• **Window Limit**: {} tokens\n• **Tier 1 (Masking)**: {} tokens (60%)\n• **Tier 2 (Summary)**: {} tokens (80%)\n• **Tier 3 (Anchor)**: {} tokens (95%)\n• *Auto-compaction evaluates automatically before each turn.*",
                self.config.provider.model, model_limit, t1, t2, t3
            );
            self.timeline.add_status(status_msg);
            return Ok(CommandAction::Continue);
        }

        if prompt == "/resume"
            || prompt.starts_with("/resume ")
            || prompt == "/sessions"
            || prompt.starts_with("/sessions ")
            || prompt == "/history"
            || prompt.starts_with("/history ")
        {
            let store = crate::session::store::SessionStore::with_workspace(&self.workspace_root);
            let arg = if let Some(rest) = prompt.strip_prefix("/resume ") {
                rest.trim()
            } else if let Some(rest) = prompt.strip_prefix("/sessions ") {
                rest.trim()
            } else if let Some(rest) = prompt.strip_prefix("/history ") {
                rest.trim()
            } else {
                ""
            };

            if arg.is_empty() || arg == "list" {
                match store.list_sessions_rich() {
                    Ok(sessions) => {
                        let initial_summary = sessions
                            .first()
                            .and_then(|s| store.get_session_summary(&s.id).ok());
                        self.modal = ModalState::new_session_browser(sessions, initial_summary);
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("✗ Failed to list sessions: {}", e));
                    }
                }
            } else {
                let target_id = if arg == "last" || arg == "latest" {
                    match store.get_last_session_id() {
                        Some(id) => id,
                        None => {
                            self.timeline.add_status(
                                "ℹ No previous sessions found in this workspace to resume"
                                    .to_string(),
                            );
                            return Ok(CommandAction::Continue);
                        }
                    }
                } else {
                    arg.to_string()
                };

                match store.load_session(&target_id) {
                    Ok(events) => {
                        let count = events.len();
                        self.timeline.entries.clear();
                        self.hydrate_session(&events);
                        let _ = control_tx.send(AgentCommand::HydrateSession {
                            session_id: target_id.clone(),
                            events,
                        });
                        self.timeline.add_status(format!(
                            "✔ Resumed session '{}' and restored agent memory ({} events)",
                            target_id, count
                        ));
                    }
                    Err(e) => {
                        self.timeline.add_status(format!(
                            "✗ Failed to resume session '{}': {}",
                            target_id, e
                        ));
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/export" || prompt.starts_with("/export ") {
            let target_path = prompt.strip_prefix("/export").unwrap_or("").trim();
            let store = crate::session::store::SessionStore::with_workspace(&self.workspace_root);
            let session_id = match store.get_last_session_id() {
                Some(id) => id,
                None => {
                    self.timeline.add_status(
                        "ℹ No recorded sessions to export in this workspace.".to_string(),
                    );
                    return Ok(CommandAction::Continue);
                }
            };
            let export_file = if target_path.is_empty() {
                let export_dir = self.workspace_root.join(".minicode").join("exports");
                if let Err(e) = std::fs::create_dir_all(&export_dir) {
                    tracing::warn!(error = %e, "Failed to create exports directory");
                }
                export_dir.join(format!("{}.md", session_id))
            } else {
                let p = self.workspace_root.join(target_path);
                if let Some(parent) = p.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        tracing::warn!(error = %e, "Failed to create parent directory");
                    }
                }
                p
            };
            match store.export_markdown(&session_id, &export_file) {
                Ok(p) => {
                    self.timeline
                        .add_status(format!("📄 Exported session Markdown to {}", p.display()));
                }
                Err(e) => {
                    self.timeline.add_status(format!("✗ Export failed: {}", e));
                }
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/save" || prompt.starts_with("/save ") {
            let target_path = prompt.strip_prefix("/save").unwrap_or("").trim();
            let export_file = if target_path.is_empty() {
                let export_dir = self.workspace_root.join(".minicode").join("exports");
                if let Err(e) = std::fs::create_dir_all(&export_dir) {
                    tracing::warn!(error = %e, "Failed to create exports directory");
                }
                export_dir.join(format!(
                    "session_{}.md",
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                ))
            } else {
                self.workspace_root.join(target_path)
            };
            let mut md = format!(
                "# minicode Session Export — {}\n\n",
                chrono::Utc::now().to_rfc3339()
            );
            for entry in &self.timeline.entries {
                match entry {
                    crate::ui::view::TimelineEntry::UserPrompt(text) => {
                        md.push_str(&format!("### 👤 User\n{}\n\n", text));
                    }
                    crate::ui::view::TimelineEntry::AssistantMarkdown(text) => {
                        md.push_str(&format!("### 🤖 Assistant\n{}\n\n", text));
                    }
                    crate::ui::view::TimelineEntry::ToolStart {
                        name,
                        command_or_path,
                    } => {
                        md.push_str(&format!(
                            "*🔧 Tool Started:* `{}` (`{}`)\n\n",
                            name, command_or_path
                        ));
                    }
                    crate::ui::view::TimelineEntry::ToolFinished {
                        name,
                        command_or_path,
                        success,
                        output,
                        ..
                    } => {
                        md.push_str(&format!(
                            "*🔧 Tool Finished:* `{}` (`{}` - {})\n```\n{}\n```\n\n",
                            name,
                            command_or_path,
                            if *success { "success" } else { "failure" },
                            output
                        ));
                    }
                    crate::ui::view::TimelineEntry::SystemStatus(text) => {
                        md.push_str(&format!("*ℹ Status:* {}\n\n", text));
                    }
                    _ => {}
                }
            }
            if let Err(e) = std::fs::write(&export_file, md) {
                self.timeline
                    .add_status(format!("✗ Failed to save session: {}", e));
            } else {
                self.timeline.add_status(format!(
                    "✔ Saved conversation export to {}",
                    export_file.display()
                ));
            }
            return Ok(CommandAction::Continue);
        }

        if prompt == "/load" || prompt.starts_with("/load ") {
            let target_id = prompt.strip_prefix("/load").unwrap_or("").trim();
            let store = crate::session::store::SessionStore::with_workspace(&self.workspace_root);
            if target_id.is_empty() {
                match store.list_sessions() {
                    Ok(sessions) if !sessions.is_empty() => {
                        let mut list_msg = format!("📂 Available Sessions ({}):\n", sessions.len());
                        for s in sessions
                            .iter()
                            .take(crate::constants::SESSION_LOAD_LIST_MAX)
                        {
                            list_msg.push_str(&format!("  • {} ({})\n", s.id, s.created_at));
                        }
                        list_msg.push_str("\nUse `/load <session_id>` to view a session.");
                        self.timeline.add_status(list_msg);
                    }
                    Ok(_) => {
                        self.timeline
                            .add_status("ℹ No past sessions found".to_string());
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("✗ Failed to list sessions: {}", e));
                    }
                }
            } else {
                match store.load_session(target_id) {
                    Ok(events) => {
                        self.timeline.entries.clear();
                        self.hydrate_session(&events);
                        let _ = control_tx.send(AgentCommand::HydrateSession {
                            session_id: target_id.to_string(),
                            events: events.clone(),
                        });
                        self.timeline.add_status(format!(
                            "✔ Loaded session '{}' and restored agent memory ({} events)",
                            target_id,
                            events.len()
                        ));
                    }
                    Err(e) => {
                        self.timeline
                            .add_status(format!("✗ Failed to load session '{}': {}", target_id, e));
                    }
                }
            }
            return Ok(CommandAction::Continue);
        }

        // Handle /retry or standard user prompt
        let prompt_to_run = if prompt == "/retry" {
            if let Some(ref last) = self.last_user_prompt {
                self.timeline
                    .add_status(format!("🔄 Retrying last prompt: \"{}\"", last));
                last.clone()
            } else {
                self.timeline
                    .add_status("ℹ No previous user prompt to retry".to_string());
                return Ok(CommandAction::Continue);
            }
        } else {
            self.last_user_prompt = Some(prompt.to_string());
            prompt.to_string()
        };

        let message_to_display = display_prompt.unwrap_or(&prompt_to_run);

        // ====================================================================
        // GATE 1: JIT Provider & Model Authentication Gate
        // If the user hasn't configured a key or selected an active provider,
        // intercept the prompt, preserve it, and display the Setup modal.
        // ====================================================================
        if self.config.provider.default.is_empty() {
            self.pending_submission = Some(crate::app::PendingSubmission {
                prompt: prompt_to_run.clone(),
                display: message_to_display.to_string(),
            });
            self.modal = ModalState::new_provider_setup_required("", &prompt_to_run);
            return Ok(CommandAction::Continue);
        }

        let is_provider_configured = if self.config.is_local_provider(&self.config.provider.default)
        {
            true
        } else {
            match self.config.get_api_key(&self.config.provider.default) {
                Ok(key) => !key.trim().is_empty(),
                Err(_) => false,
            }
        };

        if !is_provider_configured {
            self.pending_submission = Some(crate::app::PendingSubmission {
                prompt: prompt_to_run.clone(),
                display: message_to_display.to_string(),
            });
            self.modal = ModalState::new_provider_setup_required(
                &self.config.provider.default,
                &prompt_to_run,
            );
            return Ok(CommandAction::Continue);
        }

        // ====================================================================
        // GATE 2: JIT Repository CRUD & Codebase Modification Gate
        // On new repos, only prompt for full indexing when the user requests
        // code modifications, file edits, or repository mutations. General
        // questions (e.g. "what is a mutex?", "/help") execute immediately.
        // On existing repos with drift, ask permission before sync/rebuild.
        // ====================================================================
        let matched_intent = crate::agent::intent::match_intent(&prompt_to_run);
        let is_crud = crate::agent::intent::is_repository_crud_intent(
            &prompt_to_run,
            matched_intent.as_ref(),
        );

        if is_crud {
            let graph_file = self.workspace_root.join(".minicode").join("graph.json");
            if !graph_file.exists() && !self.session_skipped_indexing {
                self.pending_submission = Some(crate::app::PendingSubmission {
                    prompt: prompt_to_run.clone(),
                    display: message_to_display.to_string(),
                });
                self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
                return Ok(CommandAction::Continue);
            } else if graph_file.exists() && !self.session_skipped_drift {
                let mut graph = crate::context::graph::CodeGraph::new();
                if graph.load_cached(&self.workspace_root) {
                    if let Ok(drift) = graph.check_drift(&self.workspace_root) {
                        if drift.is_stale {
                            self.pending_submission = Some(crate::app::PendingSubmission {
                                prompt: prompt_to_run.clone(),
                                display: message_to_display.to_string(),
                            });
                            self.modal =
                                ModalState::new_workspace_drift(&self.workspace_root, &drift);
                            return Ok(CommandAction::Continue);
                        } else if drift.total_drift()
                            >= crate::constants::DEFAULT_DRIFT_MINOR_SYNC_THRESHOLD
                        {
                            tracing::info!(
                                "Seamlessly updating minor code graph drift ({} files changed)",
                                drift.total_drift()
                            );
                            if graph.incremental_update(&self.workspace_root).is_ok() {
                                let _ = graph.save_to_disk(&self.workspace_root);
                            }
                        }
                    }
                } else {
                    // Corrupted or unparseable graph.json: propose fresh workspace re-analysis
                    tracing::warn!(
                        "Existing .minicode/graph.json corrupted or unreadable; proposing workspace re-analysis"
                    );
                    self.pending_submission = Some(crate::app::PendingSubmission {
                        prompt: prompt_to_run.clone(),
                        display: message_to_display.to_string(),
                    });
                    self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
                    return Ok(CommandAction::Continue);
                }
            }
        }

        self.timeline
            .add_user_message(message_to_display.to_string());

        // Check for recognized autonomous intent to notify the user
        if let Some(m) = matched_intent {
            if m.confidence >= 0.85 && m.intent != crate::agent::intent::AgentIntent::GeneralQuery {
                let (icon, label) = m.intent.badge();
                self.timeline
                    .add_status(format!("{} Autonomous Intent: {}", icon, label));
            }
        }

        self.is_working = true;
        self.current_activity = Some(crate::ui::AgentActivity::Thinking);
        self.work_start = Some(Instant::now());
        if self.last_turn_tokens == 0 {
            self.last_turn_tokens = prompt_to_run.len().max(4) / 4;
        }

        let cancel = tokio_util::sync::CancellationToken::new();
        self.cancel_token = Some(cancel.clone());

        // Dispatch asynchronously to agent background actor
        if let Err(e) = control_tx.send(AgentCommand::Prompt(prompt_to_run, Some(cancel))) {
            tracing::error!(error = %e, "Failed to dispatch prompt to agent actor");
        }

        Ok(CommandAction::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::context::memory::intent::{IntentLedger, RequirementStatus};
    use tempfile::tempdir;

    #[test]
    fn test_parse_goal_command_show() {
        assert_eq!(parse_goal_command("/goal"), Some(GoalSubcommand::Show));
        assert_eq!(parse_goal_command("/intent"), Some(GoalSubcommand::Show));
        assert_eq!(parse_goal_command("/goal   "), Some(GoalSubcommand::Show));
        assert_eq!(parse_goal_command("/intent   "), Some(GoalSubcommand::Show));
    }

    #[test]
    fn test_parse_goal_command_add() {
        assert_eq!(
            parse_goal_command("/goal add Build authentication"),
            Some(GoalSubcommand::Add("Build authentication".to_string()))
        );
        assert_eq!(
            parse_goal_command("/intent add Build authentication"),
            Some(GoalSubcommand::Add("Build authentication".to_string()))
        );
        assert_eq!(
            parse_goal_command("/goal add"),
            Some(GoalSubcommand::Add(String::new()))
        );
        assert_eq!(
            parse_goal_command("/intent add"),
            Some(GoalSubcommand::Add(String::new()))
        );
    }

    #[test]
    fn test_parse_goal_command_done() {
        assert_eq!(
            parse_goal_command("/goal done 1"),
            Some(GoalSubcommand::Done("1".to_string()))
        );
        assert_eq!(
            parse_goal_command("/intent done 2"),
            Some(GoalSubcommand::Done("2".to_string()))
        );
        assert_eq!(
            parse_goal_command("/goal done req-1234"),
            Some(GoalSubcommand::Done("req-1234".to_string()))
        );
        assert_eq!(
            parse_goal_command("/goal done"),
            Some(GoalSubcommand::Done(String::new()))
        );
        assert_eq!(
            parse_goal_command("/intent done"),
            Some(GoalSubcommand::Done(String::new()))
        );
    }

    #[test]
    fn test_parse_goal_command_reset() {
        assert_eq!(
            parse_goal_command("/goal reset"),
            Some(GoalSubcommand::Reset)
        );
        assert_eq!(
            parse_goal_command("/intent reset"),
            Some(GoalSubcommand::Reset)
        );
        assert_eq!(
            parse_goal_command("/goal reset   "),
            Some(GoalSubcommand::Reset)
        );
    }

    #[test]
    fn test_parse_goal_command_run() {
        assert_eq!(
            parse_goal_command("/goal run Ship release"),
            Some(GoalSubcommand::Run("Ship release".to_string()))
        );
        assert_eq!(
            parse_goal_command("/intent run Ship release"),
            Some(GoalSubcommand::Run("Ship release".to_string()))
        );
        assert_eq!(
            parse_goal_command("/goal run"),
            Some(GoalSubcommand::Run(String::new()))
        );
        assert_eq!(
            parse_goal_command("/intent run"),
            Some(GoalSubcommand::Run(String::new()))
        );
        // Freeform prompt
        assert_eq!(
            parse_goal_command("/goal Ship release"),
            Some(GoalSubcommand::Run("Ship release".to_string()))
        );
        assert_eq!(
            parse_goal_command("/intent Ship release"),
            Some(GoalSubcommand::Run("Ship release".to_string()))
        );
    }

    #[test]
    fn test_parse_goal_command_non_goal() {
        assert_eq!(parse_goal_command("/help"), None);
        assert_eq!(parse_goal_command("/plan"), None);
        assert_eq!(parse_goal_command("/new"), None);
        assert_eq!(parse_goal_command("hello minicode"), None);
    }

    #[test]
    fn test_format_ledger_timeline() {
        let mut ledger = IntentLedger::new("E-Commerce Store");
        let id1 = ledger.add_item("Dashboard", Some("metrics cards"), vec![]);
        let id2 = ledger.add_item("Customers Page", Some("search, filter"), vec![]);
        let _id3 = ledger.add_item("Support Tickets", None, vec![]);

        ledger.set_status(&id1, RequirementStatus::Completed);
        ledger.set_status(&id2, RequirementStatus::InProgress);

        let formatted = format_ledger_timeline(&ledger);
        assert!(formatted.contains("🎯 Active Goal: E-Commerce Store"));
        assert!(formatted.contains("📋 Living Execution Ledger (1/3 completed):"));
        assert!(formatted.contains("[x] 1. Dashboard (metrics cards)"));
        assert!(formatted.contains("[-] 2. Customers Page (search, filter)"));
        assert!(formatted.contains("[ ] 3. Support Tickets"));
        assert!(formatted.contains(
            "💡 Commands: /goal add <task> | /goal done <index> | /goal reset | /goal run <prompt>"
        ));
    }

    #[tokio::test]
    async fn test_goal_commands_end_to_end() {
        let dir = tempdir().unwrap();
        let config = Config::default();
        let mut app = App::new(dir.path(), config);
        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();

        // 1. /goal when no ledger exists
        let action = app
            .handle_command_or_prompt("/goal", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);
        let last_status = app.timeline.entries.last().unwrap();
        if let crate::ui::view::TimelineEntry::SystemStatus(msg) = last_status {
            assert!(msg.contains("No active goal anchor found"));
        } else {
            panic!("Expected SystemStatus timeline entry");
        }

        // 2. /goal add
        let action = app
            .handle_command_or_prompt("/goal add First Task", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);
        let persistence_file = dir.path().join(".minicode/intent_anchor.json");
        assert!(persistence_file.exists());

        // 3. /intent add
        let action = app
            .handle_command_or_prompt("/intent add Second Task", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);

        // 4. /goal (display active ledger)
        let action = app
            .handle_command_or_prompt("/goal", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);
        let last_status = app.timeline.entries.last().unwrap();
        if let crate::ui::view::TimelineEntry::SystemStatus(msg) = last_status {
            assert!(msg.contains("Living Execution Ledger (0/2 completed)"));
            assert!(msg.contains("[ ] 1. First Task"));
            assert!(msg.contains("[ ] 2. Second Task"));
        } else {
            panic!("Expected SystemStatus timeline entry");
        }

        // 5. /goal done 1
        let action = app
            .handle_command_or_prompt("/goal done 1", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);

        // Verify done in loaded file
        let ledger = IntentLedger::load_from_disk(&persistence_file).unwrap();
        assert_eq!(ledger.completed_count(), 1);
        assert_eq!(ledger.items[0].status, RequirementStatus::Completed);

        // 6. /intent done 2
        let action = app
            .handle_command_or_prompt("/intent done 2", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);
        let ledger = IntentLedger::load_from_disk(&persistence_file).unwrap();
        assert_eq!(ledger.completed_count(), 2);

        // 7. /goal done invalid index
        let action = app
            .handle_command_or_prompt("/goal done 99", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);
        let last_status = app.timeline.entries.last().unwrap();
        if let crate::ui::view::TimelineEntry::SystemStatus(msg) = last_status {
            assert!(msg.contains("Invalid requirement index 99"));
        } else {
            panic!("Expected SystemStatus timeline entry");
        }

        // 8. /intent reset
        let action = app
            .handle_command_or_prompt("/intent reset", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);
        assert!(!persistence_file.exists());

        // 9. /goal run Launch new site
        let action = app
            .handle_command_or_prompt("/goal run Launch new site", None, &control_tx)
            .await
            .unwrap();
        assert_eq!(action, CommandAction::Continue);
        assert!(persistence_file.exists());

        let cmd = control_rx.try_recv().unwrap();
        if let AgentCommand::Prompt(prompt_text, _) = cmd {
            assert!(prompt_text.contains("<!-- GOAL -->"));
            assert!(prompt_text.contains("Launch new site"));
        } else {
            panic!("Expected AgentCommand::Prompt");
        }
    }
}
