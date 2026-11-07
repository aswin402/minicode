//! Goal/Intent ledger and session persistence slash command handlers (`/goal`, `/resume`, `/export`, `/save`, `/load`).

use super::{AgentCommand, App, CommandAction};
use crate::ui::modal::ModalState;
use anyhow::Result;
use tokio::sync::mpsc;

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
#[allow(dead_code)]
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
    pub(super) async fn handle_goal_and_session_slash_command(
        &mut self,
        prompt: &str,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) -> Result<Option<CommandAction>> {
        if let Some(cmd) = parse_goal_command(prompt) {
            match cmd {
                GoalSubcommand::Show => {
                    self.modal = ModalState::new_todo(&self.workspace_root);
                    return Ok(Some(CommandAction::Continue));
                }
                GoalSubcommand::Add(text) => {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        self.add_timeline_status(
                            "⚠️ Usage: /goal add <task description>".to_string(),
                        );
                        return Ok(Some(CommandAction::Continue));
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
                            self.add_timeline_status(format!(
                                "✔ Added requirement #{}: \"{}\" (id: {})",
                                ledger.items.len(),
                                trimmed,
                                item_id
                            ));
                        }
                        Err(e) => {
                            self.add_timeline_status(format!(
                                "✗ Failed to save updated ledger: {}",
                                e
                            ));
                        }
                    }
                    return Ok(Some(CommandAction::Continue));
                }
                GoalSubcommand::Done(target) => {
                    let trimmed = target.trim();
                    if trimmed.is_empty() {
                        self.add_timeline_status(
                            "⚠️ Usage: /goal done <index_or_id> (e.g. /goal done 1)".to_string(),
                        );
                        return Ok(Some(CommandAction::Continue));
                    }
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    if !persistence_path.exists() {
                        self.add_timeline_status(
                            "ℹ No active goal anchor found. Start a task or use /goal add <task>"
                                .to_string(),
                        );
                        return Ok(Some(CommandAction::Continue));
                    }
                    let mut ledger =
                        match crate::context::memory::intent::IntentLedger::load_from_disk(
                            &persistence_path,
                        ) {
                            Ok(l) => l,
                            Err(e) => {
                                self.add_timeline_status(format!(
                                    "✗ Failed to load intent ledger: {}",
                                    e
                                ));
                                return Ok(Some(CommandAction::Continue));
                            }
                        };
                    let resolved = if let Ok(idx) = trimmed.parse::<usize>() {
                        if idx >= 1 && idx <= ledger.items.len() {
                            Some((
                                ledger.items[idx - 1].id.clone(),
                                ledger.items[idx - 1].title.clone(),
                            ))
                        } else {
                            self.add_timeline_status(format!(
                                "⚠️ Invalid requirement index {}. Ledger currently has {} item(s).",
                                idx,
                                ledger.items.len()
                            ));
                            return Ok(Some(CommandAction::Continue));
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
                                    self.add_timeline_status(format!(
                                        "✔ Marked requirement \"{}\" as completed ({}/{} completed)",
                                        title,
                                        ledger.completed_count(),
                                        ledger.total_count()
                                    ));
                                }
                                Err(e) => {
                                    self.add_timeline_status(format!(
                                        "✗ Failed to save updated ledger: {}",
                                        e
                                    ));
                                }
                            }
                        }
                        None => {
                            self.add_timeline_status(format!(
                                "⚠️ No requirement found matching '{}'. Use /goal to inspect active items.",
                                trimmed
                            ));
                        }
                    }
                    return Ok(Some(CommandAction::Continue));
                }
                GoalSubcommand::Reset => {
                    let persistence_path = self
                        .workspace_root
                        .join(&self.config.agent.intent.persistence_file);
                    if persistence_path.exists() {
                        match std::fs::remove_file(&persistence_path) {
                            Ok(_) => {
                                self.add_timeline_status(
                                    "✔ Reset goal anchor and execution ledger.".to_string(),
                                );
                            }
                            Err(e) => {
                                self.add_timeline_status(format!(
                                    "✗ Failed to remove intent anchor: {}",
                                    e
                                ));
                            }
                        }
                    } else {
                        self.add_timeline_status(
                            "ℹ No active goal anchor or execution ledger found to reset."
                                .to_string(),
                        );
                    }
                    return Ok(Some(CommandAction::Continue));
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
                    self.dispatch_agent_prompt(
                        goal_prompt,
                        crate::ui::AgentActivity::Thinking,
                        control_tx,
                    );
                    return Ok(Some(CommandAction::Continue));
                }
            }
        }

        if prompt == "/resume"
            || prompt.starts_with("/resume ")
            || prompt == "/sessions"
            || prompt.starts_with("/sessions ")
            || prompt == "/session"
            || prompt.starts_with("/session ")
            || prompt == "/history"
            || prompt.starts_with("/history ")
        {
            let store = crate::session::store::SessionStore::with_workspace(&self.workspace_root);
            let arg = if let Some(rest) = prompt.strip_prefix("/resume ") {
                rest.trim()
            } else if let Some(rest) = prompt.strip_prefix("/sessions ") {
                rest.trim()
            } else if let Some(rest) = prompt.strip_prefix("/session ") {
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
                            return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
                    return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
        }

        Ok(None)
    }
}
