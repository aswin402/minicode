use crate::constants::DEFAULT_LOG_TAIL_LINES;
use crate::logging::runtime::{
    find_session_by_id_or_prefix, list_active_sessions, resolve_default_session,
};
use crate::logging::tail::LogTailer;
use crate::session::store::SessionStore;
use std::path::{Path, PathBuf};

/// Command line arguments for `minicode logs`.
#[derive(Debug, Clone, Default)]
pub struct LogsCliArgs {
    pub session_id: Option<String>,
    pub follow: bool,
    pub tail: usize,
    pub list: bool,
    pub json: bool,
    pub no_color: bool,
    pub raw: bool,
    pub filter: Option<String>,
}

/// Dispatches the `minicode logs` CLI command.
pub async fn handle_logs_cli(workspace: &Path, args: LogsCliArgs) -> anyhow::Result<()> {
    let effective_tail = if args.tail == 0 {
        DEFAULT_LOG_TAIL_LINES
    } else {
        args.tail
    };

    // 1. If --list was requested, display running and past sessions
    if args.list {
        return handle_list_sessions(workspace, args.json, args.no_color);
    }

    // 2. If --raw was requested, tail raw system tracing logs
    if args.raw {
        return handle_raw_tracing_logs(
            effective_tail,
            args.follow,
            args.no_color,
            args.json,
            args.filter,
        )
        .await;
    }

    // 3. Resolve target session (explicit ID, prefix, active workspace session, or latest)
    let target = if let Some(query) = args.session_id {
        match find_session_by_id_or_prefix(&query, Some(workspace)) {
            Some(res) => res,
            None => {
                eprintln!(
                    "\x1b[1;31mError:\x1b[0m Session '{}' not found in active runtimes or session history.",
                    query
                );
                eprintln!("\nRun \x1b[1;36mminicode logs --list\x1b[0m to view all active and past sessions.");
                return Ok(());
            }
        }
    } else {
        match resolve_default_session(workspace) {
            Some(res) => res,
            None => {
                eprintln!("\x1b[1;33mNo active or past sessions found.\x1b[0m");
                eprintln!("Start an agent with \x1b[1;36mminicode\x1b[0m to begin a session.");
                return Ok(());
            }
        }
    };

    let (session_path, active_record) = target;
    let tailer = LogTailer::new(args.no_color, args.json, false, args.filter);
    tailer
        .tail(
            &session_path,
            effective_tail,
            args.follow,
            active_record.as_ref(),
        )
        .await
}

/// Lists all active running agents and past sessions across all workspaces.
fn handle_list_sessions(workspace: &Path, json: bool, no_color: bool) -> anyhow::Result<()> {
    let active = list_active_sessions();
    let store = SessionStore::with_workspace(workspace);
    let past = store.list_sessions_rich().unwrap_or_default();

    if json {
        let out = serde_json::json!({
            "active_agents": active,
            "recent_sessions": past.iter().take(20).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let cyan = if no_color {
        ""
    } else {
        "\x1b[1;38;2;130;226;255m"
    };
    let green = if no_color {
        ""
    } else {
        "\x1b[1;38;2;97;255;202m"
    };
    let dim = if no_color { "" } else { "\x1b[90m" };
    let reset = if no_color { "" } else { "\x1b[0m" };

    println!(
        "\n{}minicode Active Agents & Session Observability Catalog{}\n",
        cyan, reset
    );

    // Active Sessions
    if active.is_empty() {
        println!(
            "{}No agents currently running on this system.{}",
            dim, reset
        );
    } else {
        println!("{}ACTIVE AGENTS ({}):{}", green, active.len(), reset);
        for act in &active {
            println!("  {}●{} {} (PID {})", green, reset, act.session_id, act.pid);
            println!("    Workspace : {}", act.workspace);
            println!("    Provider  : {} ({})", act.provider, act.model);
            println!("    Started   : {}", act.started_at);
            println!();
        }
    }

    // Historical Sessions
    println!(
        "{}RECENT SESSIONS (Showing last {}):{}",
        cyan,
        past.len().min(10),
        reset
    );
    for meta in past.iter().take(10) {
        let is_active = active.iter().any(|a| a.session_id == meta.id);
        if is_active {
            continue; // Already printed above
        }
        println!("  {}○{} {}", dim, reset, meta.id);
        if !meta.workspace.is_empty() {
            println!("    Workspace : {}", meta.workspace);
        }
        if meta.event_count > 0 {
            println!("    Events    : {}", meta.event_count);
        }
        if !meta.preview.is_empty() {
            println!("    Prompt    : \"{}\"", meta.preview);
        }
        println!();
    }

    println!("{}Commands:{}", dim, reset);
    println!("  Stream live logs   : minicode logs <session-id> -f");
    println!("  View last 100 logs : minicode logs <session-id> -n 100");
    println!("  Stream current repo: minicode logs -f\n");

    Ok(())
}

/// Locates and tails the latest raw daily tracing log file.
async fn handle_raw_tracing_logs(
    tail: usize,
    follow: bool,
    no_color: bool,
    json: bool,
    filter: Option<String>,
) -> anyhow::Result<()> {
    let logs_dir = if let Some(config_dir) = dirs::config_dir() {
        config_dir
            .join(crate::constants::CONFIG_DIR_NAME)
            .join(crate::constants::LOGS_DIR_NAME)
    } else {
        PathBuf::from(crate::constants::WORKSPACE_DIR_NAME).join(crate::constants::LOGS_DIR_NAME)
    };

    let entries = std::fs::read_dir(&logs_dir)?;
    let mut log_files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("minicode.log") {
                    log_files.push(path);
                }
            }
        }
    }

    log_files.sort();
    let latest = match log_files.last() {
        Some(f) => f,
        None => {
            eprintln!("No log files found in {}", logs_dir.display());
            return Ok(());
        }
    };

    let tailer = LogTailer::new(no_color, json, true, filter);
    tailer.tail(latest, tail, follow, None).await
}
