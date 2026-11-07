//! Suite slash command handlers (`/stack`, `/blocks`, `/vault`, `/tasks`, `/kit`) for `App`.

use super::{AgentCommand, App, CommandAction};
use crate::ui::modal::ModalState;
use anyhow::Result;
use tokio::sync::mpsc;

impl<'a> App<'a> {
    pub(super) async fn handle_suite_slash_command(
        &mut self,
        prompt: &str,
        prompt_trimmed: &str,
        prompt_lower: &str,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) -> Result<Option<CommandAction>> {
        if prompt_lower == "/stack" || prompt_lower == "/stacks" {
            self.modal = ModalState::new_stack_select();
            return Ok(Some(CommandAction::Continue));
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
            return Ok(Some(CommandAction::Continue));
        }

        // MiniVault Agent Skills Warehouse Slash Commands (/vault, /skills, /minivault, /bundle, /bundles)
        if prompt_lower == "/vault"
            || prompt_lower == "/skills"
            || prompt_lower == "/minivault"
            || prompt_lower == "/bundle"
            || prompt_lower == "/bundles"
            || prompt_lower.starts_with("/vault ")
            || prompt_lower.starts_with("/skills ")
            || prompt_lower.starts_with("/minivault ")
            || prompt_lower.starts_with("/bundle ")
            || prompt_lower.starts_with("/bundles ")
        {
            let mut vault_modal = ModalState::new_vault(&self.workspace_root);
            let query = if prompt_lower.starts_with("/minivault ") {
                prompt_trimmed[11..].trim()
            } else if prompt_lower.starts_with("/skills ") {
                prompt_trimmed[8..].trim()
            } else if prompt_lower.starts_with("/vault ") {
                prompt_trimmed[7..].trim()
            } else if prompt_lower.starts_with("/bundles ") {
                prompt_trimmed[9..].trim()
            } else if prompt_lower.starts_with("/bundle ") {
                prompt_trimmed[8..].trim()
            } else {
                ""
            };

            let is_bundle_root = prompt_lower == "/bundle" || prompt_lower == "/bundles";

            if let ModalState::Vault(ref mut state) = vault_modal {
                let query_lower = query.to_lowercase();
                if is_bundle_root
                    || query_lower == "bundle"
                    || query_lower == "bundles"
                    || query_lower == "bundle list"
                    || query_lower == "bundles list"
                {
                    state.active_tab = crate::ui::modals::vault::VaultTab::Bundles;
                    state.search_query.clear();
                    state.refresh_filtered();
                } else if query_lower.starts_with("bundle load ")
                    || query_lower.starts_with("load bundle ")
                {
                    let bundle_name = query[12..].trim();
                    state.active_tab = crate::ui::modals::vault::VaultTab::Bundles;
                    let store = crate::vault::store::VaultStore::new(&self.workspace_root);
                    match store.load_bundle_to_project(bundle_name) {
                        Ok((loaded, failed)) => {
                            self.toasts.push(
                                crate::ui::ToastKind::Skill,
                                "Skill Bundle Loaded",
                                format!("`{}` — {} skills activated", bundle_name, loaded.len()),
                                Some("Bundle".to_string()),
                                Some("skill:loaded".to_string()),
                            );
                            if failed.is_empty() {
                                state.status_message = Some(format!(
                                    "✔ Loaded bundle `{}` ({} skills: {}) into project!",
                                    bundle_name,
                                    loaded.len(),
                                    loaded.join(", ")
                                ));
                            } else {
                                state.status_message = Some(format!(
                                    "✔ Loaded bundle `{}` ({} installed, {} failed/skipped).",
                                    bundle_name,
                                    loaded.len(),
                                    failed.len()
                                ));
                            }
                        }
                        Err(e) => {
                            state.status_message =
                                Some(format!("❌ Failed to load bundle `{}`: {}", bundle_name, e));
                        }
                    }
                    state.refresh_filtered();
                } else if query_lower.starts_with("load ") {
                    let target_name = query[5..].trim();
                    let store = crate::vault::store::VaultStore::new(&self.workspace_root);
                    if store.get_bundle(target_name).is_some() {
                        state.active_tab = crate::ui::modals::vault::VaultTab::Bundles;
                        match store.load_bundle_to_project(target_name) {
                            Ok((loaded, failed)) => {
                                self.toasts.push(
                                    crate::ui::ToastKind::Skill,
                                    "Skill Bundle Loaded",
                                    format!(
                                        "`{}` — {} skills activated",
                                        target_name,
                                        loaded.len()
                                    ),
                                    Some("Bundle".to_string()),
                                    Some("skill:loaded".to_string()),
                                );
                                if failed.is_empty() {
                                    state.status_message = Some(format!(
                                        "✔ Loaded bundle `{}` ({} skills: {}) into project!",
                                        target_name,
                                        loaded.len(),
                                        loaded.join(", ")
                                    ));
                                } else {
                                    state.status_message = Some(format!(
                                        "✔ Loaded bundle `{}` ({} installed, {} failed/skipped).",
                                        target_name,
                                        loaded.len(),
                                        failed.len()
                                    ));
                                }
                            }
                            Err(e) => {
                                state.status_message = Some(format!(
                                    "❌ Failed to load bundle `{}`: {}",
                                    target_name, e
                                ));
                            }
                        }
                    } else {
                        match store.load_to_project(target_name) {
                            Ok(skill) => {
                                self.toasts.push(
                                    crate::ui::ToastKind::Skill,
                                    "Skill Loaded",
                                    format!("`{}` — {}", skill.name, skill.description),
                                    Some("Loaded".to_string()),
                                    Some("skill:loaded".to_string()),
                                );
                                state.status_message = Some(format!(
                                    "✔ Successfully loaded skill `{}` into active project!",
                                    target_name
                                ));
                            }
                            Err(e) => {
                                state.status_message = Some(format!(
                                    "❌ Failed to load skill `{}`: {}",
                                    target_name, e
                                ));
                            }
                        }
                    }
                    state.refresh_filtered();
                } else if query_lower.starts_with("bundle ") {
                    state.active_tab = crate::ui::modals::vault::VaultTab::Bundles;
                    state.search_query = query[7..].trim().to_string();
                    state.refresh_filtered();
                } else if !query.is_empty() {
                    state.search_query = query.to_string();
                    state.refresh_filtered();
                }
            }
            self.modal = vault_modal;
            return Ok(Some(CommandAction::Continue));
        }

        // MiniTask Runtime Orchestrator Slash Commands (/tasks, /minitask, /dev)
        if prompt_lower == "/tasks"
            || prompt_lower == "/minitask"
            || prompt_lower == "/dev"
            || prompt_lower.starts_with("/tasks ")
            || prompt_lower.starts_with("/minitask ")
            || prompt_lower.starts_with("/dev ")
        {
            let sub = if prompt_lower.starts_with("/minitask ") {
                prompt_trimmed[10..].trim()
            } else if prompt_lower.starts_with("/tasks ") {
                prompt_trimmed[7..].trim()
            } else if prompt_lower.starts_with("/dev ") {
                prompt_trimmed[5..].trim()
            } else {
                ""
            };

            let registry = crate::dev::registry::get_global_dev_registry();
            match sub {
                "" | "modal" | "ui" => {
                    let mut state = crate::ui::modals::processes::ProcessesModalState::new(
                        &self.workspace_root,
                    );
                    state.refresh_live().await;
                    self.modal = ModalState::Processes(state);
                }
                "list" | "ps" => {
                    let list = registry
                        .list_all_with_completed(None, Some(&self.workspace_root))
                        .await;
                    let has_registered_browser = list.iter().any(|p| {
                        p.process_type == crate::dev::models::DevProcessType::Chrome
                            || p.id.as_str().eq_ignore_ascii_case("browser")
                            || p.id.as_str().starts_with("browser-")
                    });
                    let browser_info = if has_registered_browser {
                        None
                    } else {
                        crate::tools::browser::BrowserManager::get_live_engine_details().await
                    };
                    if list.is_empty() && browser_info.is_none() {
                        self.timeline.add_status("ℹ No active development processes running. Use 'minitask start' or /tasks to launch one.".to_string());
                    } else {
                        let total = list.len() + if browser_info.is_some() { 1 } else { 0 };
                        let mut msg = format!("🚀 Active Development Processes ({})\n", total);
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
                        if let Some((engine, pid, port, uptime)) = browser_info {
                            msg.push_str(&format!(
                                "• [browser] {} (Browser) - CDP: http://127.0.0.1:{} | PID: {} | Uptime: {}s\n",
                                engine, port, pid, uptime
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
                        self.timeline.add_status("ℹ No active server URL found. Start a server with 'minitask start' or /tasks first.".to_string());
                    }
                }
                "workers" | "subagents" => {
                    let list = registry.list_workers().await;
                    let minipubs = crate::agent::subagent::try_get_global_subagent_pool()
                        .map(|p| p.snapshot_subagents())
                        .unwrap_or_default();
                    if list.is_empty() && minipubs.is_empty() {
                        self.timeline.add_status(
                            "ℹ No active MiniPub subagents or delegated workers running."
                                .to_string(),
                        );
                    } else {
                        let mut msg = format!(
                            "🤖 Active Autonomous Subagents & MiniPub Workers ({})\n",
                            list.len() + minipubs.len()
                        );
                        for s in &minipubs {
                            let done_todos = s.todos.iter().filter(|t| t.completed).count();
                            msg.push_str(&format!(
                                "• [MiniPub: {}] {} ({}) | State: {:?} | Todos: {}/{} | Elapsed: {}s | Heartbeat: {}s ago\n",
                                s.id,
                                s.role.badge(),
                                s.isolation_badge(),
                                s.state,
                                done_todos,
                                s.todos.len(),
                                s.elapsed_secs(),
                                s.last_activity_secs_ago(),
                            ));
                        }
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
                            let suggested = crate::dev::ports::find_next_available_port(
                                port + 1,
                                crate::constants::DEFAULT_PORT_SCAN_RANGE,
                            );
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
                        "ℹ Unknown /tasks subcommand '{}'. Usage: /tasks [list | workers | resources | port <number> | watchdog | logs <id> | stop <id> | kill | screenshot]",
                        unknown
                    ));
                }
            }
            return Ok(Some(CommandAction::Continue));
        }

        // MiniKit Slash Commands (/kit, /stacks, /drift, /sync, /doctor; note: /heal is handled by VerificationLoop below, while `/kit heal` runs MiniKit diff heal)
        if prompt_lower == "/kit"
            || prompt_lower.starts_with("/kit ")
            || prompt_lower.starts_with("/stack ")
            || prompt_lower == "/block"
            || prompt_lower.starts_with("/block ")
            || prompt_lower == "/drift"
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
            } else if prompt_lower == "/sync" {
                "sync".to_string()
            } else if prompt_lower == "/doctor" {
                "doctor".to_string()
            } else {
                String::new()
            };
            let sub = sub_owned.as_str();

            let sub_lower = sub.to_lowercase();
            if sub_lower.is_empty()
                || sub_lower == "stacks"
                || sub_lower == "stack"
                || sub_lower == "stack list"
            {
                self.modal = ModalState::new_stack_select();
                return Ok(Some(CommandAction::Continue));
            } else if sub_lower == "blocks" || sub_lower == "block" || sub_lower == "block list" {
                self.modal = ModalState::new_blocks(&self.workspace_root);
                return Ok(Some(CommandAction::Continue));
            } else if sub_lower == "skills" || sub_lower == "skill list" {
                self.modal = ModalState::new_vault(&self.workspace_root);
                return Ok(Some(CommandAction::Continue));
            } else if sub_lower == "help" {
                self.modal = ModalState::new_stack_select();
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
            } else if sub_lower == "sync" {
                match crate::tools::minikit::sync::MiniKitSyncEngine::sync(&self.workspace_root) {
                    Ok(msg) => self.timeline.add_status(format!("✔ {}", msg)),
                    Err(e) => self.timeline.add_status(format!("✗ Sync failed: {}", e)),
                }
                return Ok(Some(CommandAction::Continue));
            } else if sub_lower == "doctor" {
                let diag = crate::tools::minikit::doctor::MiniKitDoctor::diagnose();
                self.timeline.add_status(diag);
                return Ok(Some(CommandAction::Continue));
            } else if sub_lower == "diff" {
                match crate::tools::minikit::diff::diff_stack(&self.workspace_root, None, false) {
                    Ok(res) => self.timeline.add_status(res.format_report()),
                    Err(e) => self
                        .timeline
                        .add_status(format!("✗ Drift check failed: {}", e)),
                }
                return Ok(Some(CommandAction::Continue));
            } else if sub_lower == "heal" || sub_lower == "diff --apply" {
                match crate::tools::minikit::diff::diff_stack(&self.workspace_root, None, true) {
                    Ok(res) => self.timeline.add_status(res.format_report()),
                    Err(e) => self
                        .timeline
                        .add_status(format!("✗ Drift self-healing failed: {}", e)),
                }
                return Ok(Some(CommandAction::Continue));
            } else if sub_lower.starts_with("add ") {
                let pkg_name = sub.strip_prefix("add ").unwrap_or("").trim();
                if pkg_name.is_empty() {
                    self.timeline
                        .add_status("⚠ Usage: `/kit add <package-name>`".to_string());
                    return Ok(Some(CommandAction::Continue));
                }
                let add_prompt = format!(
                    "Use MiniKit dependency intelligence to verify and add package `{}` to the project: call `kit_info` to inspect metadata and `kit_add` to update dependencies.",
                    pkg_name
                );
                self.timeline.add_user_message(prompt.to_string());
                self.dispatch_agent_prompt(
                    add_prompt,
                    crate::ui::AgentActivity::ExecutingCommand {
                        command: format!("kit add {}", pkg_name),
                    },
                    control_tx,
                );
                return Ok(Some(CommandAction::Continue));
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
                return Ok(Some(CommandAction::Continue));
            }
        }

        Ok(None)
    }
}
