mod agent;
mod app;
mod config;
mod constants;
mod context;
mod error;
pub mod git;
mod logging;
pub mod lsp;
mod mcp;
mod sandbox;
mod session;
mod tools;
mod ui;

use agent::types::{AgentEvent, StdinCommand};
use agent::{create_provider, AgentLoop};
use app::App;
use clap::{Parser, Subcommand};
use config::Config;
use error::Result;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::sync::mpsc;
use ui::ConfigMenu;

fn get_version_banner() -> &'static str {
    static BANNER: OnceLock<String> = OnceLock::new();
    BANNER.get_or_init(|| {
        format!(
            "\n\x1b[38;2;162;119;255m   ___ ___                           _     \x1b[0m\n\
             \x1b[38;2;162;119;255m  |   Y   | _   ___  _   ___  ___  _| | ___ \x1b[0m\n\
             \x1b[38;2;246;148;255m  |.      || | |   || | |  _|| . || . || -_|\x1b[0m\n\
             \x1b[38;2;97;255;202m  |. \\_/  ||_| |_|_||_| |___||___||___||___|\x1b[0m\n\n\
             \x1b[1m\x1b[38;2;162;119;255m• {} v{}\x1b[0m — {}\n\
             \x1b[38;2;130;226;255m• Repository\x1b[0m : {}\n\
             \x1b[38;2;255;202;133m• Author    \x1b[0m : {}\n\
             \x1b[38;2;97;255;202m• Runtime   \x1b[0m : Pure Rust (Tokio Async Engine)\n\
             \x1b[38;2;162;119;255m• License   \x1b[0m : {}\n",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            env!("CARGO_PKG_REPOSITORY"),
            env!("CARGO_PKG_AUTHORS"),
            env!("CARGO_PKG_LICENSE"),
        )
    })
}

#[derive(Parser, Debug)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version = get_version_banner(),
    about = env!("CARGO_PKG_DESCRIPTION")
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Target workspace directory (defaults to current directory)
    #[arg(short = 'd', long, global = true)]
    dir: Option<PathBuf>,

    /// Override the default LLM model
    #[arg(short = 'm', long, global = true)]
    model: Option<String>,

    /// Override the LLM provider (openrouter, gemini, anthropic, openai, ollama)
    #[arg(short = 'p', long, global = true)]
    provider: Option<String>,

    /// Enable machine-readable NDJSON streaming over stdout for AI agents
    #[arg(long, global = true)]
    json_stream: bool,

    /// Auto-approve dangerous tools (write_file, patch_file, exec_cmd); required for them in headless mode
    #[arg(short = 'y', long, global = true)]
    yes: bool,

    /// Disable full-screen TUI and use accessible scrolling REPL
    #[arg(long, alias = "accessible", global = true)]
    plain: bool,

    /// Force soft dark color theme
    #[arg(long, global = true)]
    soft: bool,

    /// Resume a previous session by session ID
    #[arg(long, global = true)]
    resume: Option<String>,

    /// Resume the most recent session
    #[arg(long, alias = "continue", global = true)]
    continue_session: bool,

    /// Shell command execution timeout in seconds
    #[arg(short = 't', long, global = true)]
    timeout: Option<u64>,

    /// Enable verbose logging to ~/.config/minicode/logs/
    #[arg(short = 'v', long, global = true)]
    verbose: bool,

    /// Set explicit logging level (error, warn, info, debug, trace)
    #[arg(long, global = true)]
    log_level: Option<String>,

    /// Path to custom config file
    #[arg(long, global = true)]
    config: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Interactive configuration wizard (select provider, model, API keys, approval policy)
    Configure,

    /// Execute a one-shot autonomous task non-interactively
    Run {
        /// Task description or prompt for the agent to execute
        task: String,
    },

    /// Run minicode as a Model Context Protocol (MCP) server over stdio
    Serve {
        /// Workspace directory to expose to MCP clients (default: current directory)
        #[arg(short = 'd', long)]
        dir: Option<PathBuf>,
    },

    /// Manage, list, and scaffold native onpkg architecture stacks
    Stack {
        #[command(subcommand)]
        action: Option<StackCommands>,

        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },

    /// Inspect formatted or machine-readable git diffs
    Diff {
        /// Only show staged changes (--cached)
        #[arg(short = 's', long)]
        staged: bool,

        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },

    /// Perform a multi-agent adversarial code review on git changes
    Review {
        /// Only review staged changes
        #[arg(short = 's', long)]
        staged: bool,

        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },

    /// Run multi-runtime environment health checks and diagnostics
    Doctor,

    /// Synchronize onpkg.json and AGENTS.md with current workspace
    Sync,

    /// Generate an autonomous milestone implementation plan in onpkg_docs/todo.md
    Plan {
        /// The feature or task specification to plan
        prompt: Option<String>,
    },

    /// View conversation session history and analytical summaries
    History {
        /// Output in machine-readable JSON format
        #[arg(long)]
        json: bool,
    },

    /// Export a session trajectory to a Markdown transcript
    Export {
        /// Session ID to export (defaults to most recent session)
        session_id: Option<String>,

        /// Output file path (defaults to .minicode/exports/<session_id>.md)
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum StackCommands {
    /// List all available built-in and custom stacks
    List,

    /// Show detailed metadata and package list for a stack
    Show {
        /// Name of the stack (e.g. 'fastapi', 'react-vite-gsap', 'next-template')
        name: String,
    },

    /// Scaffold a stack into the current workspace or target subdirectory
    Add {
        /// Name of the stack to scaffold
        name: String,

        /// Target subdirectory to create
        #[arg(short = 'd', long)]
        dir: Option<String>,

        /// Skip post-scaffold package manager installation
        #[arg(long)]
        no_install: bool,
    },
}

/// Installs a panic hook to restore the terminal if the application crashes in TUI mode
fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show,
        );
        original_hook(panic_info);
    }));
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_panic_hook();
    let cli = Cli::parse();

    // 1. Resolve workspace root
    let workspace_dir = cli
        .dir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let workspace_canonical = std::fs::canonicalize(&workspace_dir).unwrap_or(workspace_dir);

    // 2. Dispatch configure command immediately if requested
    if let Some(Commands::Configure) = cli.command {
        ConfigMenu::run_interactive(&workspace_canonical).await?;
        return Ok(());
    }

    // 3. Load configuration hierarchy
    let mut config = Config::load(Some(&workspace_canonical), cli.config.as_deref())?;

    // Apply CLI overrides to config
    if let Some(m) = cli.model {
        config.provider.model = m;
    }
    if let Some(p) = cli.provider {
        config.provider.default = p;
    }
    if cli.yes {
        config.agent.auto_approve = true;
    }
    if cli.plain {
        config.ui.plain = true;
    }
    if cli.soft {
        config.ui.theme = "soft".to_string();
    }
    if let Some(t) = cli.timeout {
        config.agent.timeout = t;
    }
    if let Some(lvl) = cli.log_level {
        config.logging.level = lvl;
    } else if cli.verbose {
        config.logging.level = "debug".to_string();
    }

    // 4. Initialize logging subsystem
    let _log_guard = logging::init_logging(None, &config.logging.level, false)?;
    tracing::info!(
        workspace = %workspace_canonical.display(),
        provider = %config.provider.default,
        model = %config.provider.model,
        "Starting minicode"
    );

    // 5. Dispatch execution mode
    match cli.command {
        Some(Commands::Configure) => unreachable!(), // Handled earlier
        Some(Commands::Run { task }) => {
            run_headless_task(&workspace_canonical, &config, &task, cli.json_stream).await?;
        }
        Some(Commands::Serve { dir }) => {
            let target_dir = dir.unwrap_or(workspace_canonical);
            mcp::MinicodeMcpServer::run_stdio(&target_dir, &config).await?;
        }
        Some(Commands::Stack { action, json }) => {
            handle_stack_cli(&workspace_canonical, action, json).await?;
        }
        Some(Commands::Diff { staged, json }) => {
            handle_diff_cli(&workspace_canonical, staged, json).await?;
        }
        Some(Commands::Review { staged, json }) => {
            handle_review_cli(&workspace_canonical, staged, json).await?;
        }
        Some(Commands::Doctor) => {
            let report = tools::onpkg::doctor::OnpkgDoctor::diagnose();
            println!("{}", report);
        }
        Some(Commands::Sync) => {
            match tools::onpkg::sync::OnpkgSyncEngine::sync(&workspace_canonical) {
                Ok(msg) => println!("✔ {}", msg),
                Err(e) => eprintln!("✗ Sync failed: {}", e),
            }
        }
        Some(Commands::Plan { prompt }) => {
            let plan_task = if let Some(p) = prompt {
                format!("Plan and break down the following implementation into actionable verifiable tasks in onpkg_docs/todo.md: {}", p)
            } else {
                "Inspect current repository architecture and generate a structured, verifiable milestone implementation plan in onpkg_docs/todo.md and onpkg_docs/implementation.md.".to_string()
            };
            run_headless_task(&workspace_canonical, &config, &plan_task, cli.json_stream).await?;
        }
        Some(Commands::History { json }) => {
            let store = session::store::SessionStore::with_workspace(&workspace_canonical);
            let sessions = store.list_sessions_rich()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else {
                println!(
                    "\n📜 Session History for `{}` ({})\n",
                    workspace_canonical.display(),
                    sessions.len()
                );
                if sessions.is_empty() {
                    println!("  (No sessions recorded yet)");
                } else {
                    for (i, s) in sessions.iter().enumerate() {
                        let summary = store.get_session_summary(&s.id).ok();
                        let model_info = summary
                            .as_ref()
                            .map(|sm| sm.model.as_str())
                            .unwrap_or("unknown");
                        let turns = summary.as_ref().map(|sm| sm.total_turns).unwrap_or(0);
                        let tokens = summary.as_ref().map(|sm| sm.total_tokens).unwrap_or(0);
                        println!("  {}. \x1b[1m\x1b[38;2;162;119;255m{}\x1b[0m", i + 1, s.id);
                        println!(
                            "     Created: {} | Model: \x1b[38;2;97;255;202m{}\x1b[0m | Turns: {} | Tokens: ~{}",
                            s.created_at, model_info, turns, tokens
                        );
                        if !s.preview.is_empty() {
                            println!("     Prompt : \x1b[38;2;140;140;150m{}\x1b[0m", s.preview);
                        }
                    }
                    println!(
                        "\n💡 Run `minicode export <session-id>` to export full Markdown transcript.\n"
                    );
                }
            }
        }
        Some(Commands::Export { session_id, output }) => {
            let store = session::store::SessionStore::with_workspace(&workspace_canonical);
            let target_id = match session_id {
                Some(id) => id,
                None => match store.get_last_session_id() {
                    Some(id) => id,
                    None => {
                        eprintln!("✗ No sessions found to export in this workspace.");
                        return Ok(());
                    }
                },
            };
            let out_path = match output {
                Some(p) => p,
                None => {
                    let export_dir = workspace_canonical.join(".minicode").join("exports");
                    let _ = std::fs::create_dir_all(&export_dir);
                    export_dir.join(format!("{}.md", target_id))
                }
            };
            let exported = store.export_markdown(&target_id, &out_path)?;
            println!("✔ Exported session transcript to {}", exported.display());
        }
        None => {
            if cli.json_stream {
                run_ndjson_agent(&workspace_canonical, &config).await?;
            } else {
                let resume_session_id = if cli.continue_session {
                    let store = session::store::SessionStore::with_workspace(&workspace_canonical);
                    store.get_last_session_id()
                } else {
                    cli.resume
                };
                run_interactive_mode(&workspace_canonical, &config, resume_session_id.as_deref())
                    .await?;
            }
        }
    }

    Ok(())
}

async fn handle_stack_cli(
    workspace: &Path,
    action: Option<StackCommands>,
    json_mode: bool,
) -> anyhow::Result<()> {
    match action {
        None | Some(StackCommands::List) => {
            let stacks = tools::onpkg::scaffolder::OnpkgScaffolder::get_all_stacks();
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&stacks)?);
            } else {
                println!(
                    "\n📦 Built-in & Available Architecture Stacks ({})\n",
                    stacks.len()
                );
                for s in &stacks {
                    println!(
                        "  • \x1b[1m\x1b[38;2;162;119;255m{:<24}\x1b[0m [{}] ({} files)",
                        s.name,
                        s.runtime,
                        s.files.len()
                    );
                    println!("    \x1b[38;2;140;140;150m{}\x1b[0m", s.description);
                }
                println!("\n💡 Run `minicode stack add <name> [--dir <path>]` to scaffold.\n");
            }
        }
        Some(StackCommands::Show { name }) => {
            let stacks = tools::onpkg::scaffolder::OnpkgScaffolder::get_all_stacks();
            if let Some(s) = stacks
                .into_iter()
                .find(|s| s.name.eq_ignore_ascii_case(&name))
            {
                if json_mode {
                    println!("{}", serde_json::to_string_pretty(&s)?);
                } else {
                    println!(
                        "\n📦 Stack: \x1b[1m\x1b[38;2;162;119;255m{}\x1b[0m (Runtime: {})",
                        s.name, s.runtime
                    );
                    println!("📝 {}", s.description);
                    println!(
                        "⚡ Packages: {}",
                        if s.packages.is_empty() {
                            "none".to_string()
                        } else {
                            s.packages.join(", ")
                        }
                    );
                    println!("📁 Files ({} files):", s.files.len());
                    for f in &s.files {
                        println!("  ├── {}", f.path);
                    }
                }
            } else {
                eprintln!(
                    "✗ Stack `{}` not found. Run `minicode stack list` for all available stacks.",
                    name
                );
            }
        }
        Some(StackCommands::Add {
            name,
            dir,
            no_install,
        }) => {
            let res = tools::onpkg::scaffolder::OnpkgScaffolder::scaffold(
                workspace,
                &name,
                dir.as_deref(),
                no_install,
            )
            .await?;
            println!("{}", res);
        }
    }
    Ok(())
}

async fn handle_diff_cli(workspace: &Path, staged: bool, json_mode: bool) -> anyhow::Result<()> {
    let diff_files = git::diff_viewer::GitDiffViewer::load_diffs(workspace, staged).await?;
    if json_mode {
        println!("{}", serde_json::to_string_pretty(&diff_files)?);
    } else {
        let view_name = if staged {
            "Staged (--cached)"
        } else {
            "Working Tree (unstaged)"
        };
        if diff_files.is_empty() {
            println!("✔ {} is clean — no diffs found.", view_name);
        } else {
            println!(
                "\n🔍 Git Diff ({}) — {} modified file(s):\n",
                view_name,
                diff_files.len()
            );
            for f in &diff_files {
                println!(
                    "  • \x1b[1m{}\x1b[0m [{}] (+{} -{})",
                    f.path, f.status_char, f.additions, f.deletions
                );
                for l in &f.lines {
                    match l.tag {
                        '+' => println!("\x1b[38;2;97;255;202m+ {}\x1b[0m", l.content),
                        '-' => println!("\x1b[38;2;255;107;128m- {}\x1b[0m", l.content),
                        '@' => println!("\x1b[38;2;162;119;255m{}\x1b[0m", l.content),
                        _ => println!("  {}", l.content),
                    }
                }
                println!();
            }
        }
    }
    Ok(())
}

async fn handle_review_cli(workspace: &Path, staged: bool, json_mode: bool) -> anyhow::Result<()> {
    let report = git::reviewer::GitReviewer::review_workspace(workspace, staged).await?;
    if json_mode {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", git::reviewer::GitReviewer::format_report(&report));
    }
    Ok(())
}

/// Executes a one-shot task non-interactively using the ReAct AgentLoop
async fn run_headless_task(
    workspace: &Path,
    config: &Config,
    task: &str,
    emit_ndjson: bool,
) -> Result<()> {
    let api_key = config.get_api_key(&config.provider.default)?;
    let provider = create_provider(&config.provider.default, &api_key)?;

    let mut agent = AgentLoop::new(workspace, config.clone(), provider);
    // No interactive approval sink exists in one-shot mode: dangerous tools
    // are refused unless auto_approve was set via --yes / MINICODE_AUTO_APPROVE.
    agent.set_interactive_approvals(false);

    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

    // Spawn event consumer for stdout printing (NDJSON or Human Plain)
    let is_ndjson = emit_ndjson;
    let event_consumer = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if is_ndjson {
                if let Ok(json) = serde_json::to_string(&event) {
                    println!("{}", json);
                }
            } else {
                match event {
                    AgentEvent::TurnStart { turn_id, model, .. } => {
                        println!("⚡ minicode turn #{} [{}]", turn_id, model);
                        println!("─────────────────────────────────────────");
                    }
                    AgentEvent::StreamDelta { delta, .. } => {
                        print!("{}", delta);
                        use std::io::Write;
                        std::io::stdout().flush().ok();
                    }
                    AgentEvent::ToolCall { tool, args, .. } => {
                        println!("\n▶ [Tool: {}({})]", tool, args);
                    }
                    AgentEvent::ToolResult {
                        tool,
                        success,
                        output,
                        duration_ms,
                        ..
                    } => {
                        let status_symbol = if success { "✓" } else { "✗" };
                        println!(
                            "{} [{}] ({}ms): {}",
                            status_symbol,
                            tool,
                            duration_ms,
                            output.trim()
                        );
                    }
                    AgentEvent::TurnEnd {
                        total_tokens_used,
                        files_modified,
                        ..
                    } => {
                        println!("\n─────────────────────────────────────────");
                        println!(
                            "✓ Completed (tokens: {}, modified: {:?})",
                            total_tokens_used, files_modified
                        );
                    }
                    AgentEvent::Error { message, .. } => {
                        eprintln!("\n✗ Error: {}", message);
                    }
                    _ => {}
                }
            }
        }
    });

    agent.execute_turn(task, tx, None).await?;
    event_consumer.await.ok();

    Ok(())
}
/// Prints an invalid-command NDJSON error event to stdout.
fn emit_invalid_command(message: &str) {
    let err_event = AgentEvent::Error {
        turn_id: None,
        code: "invalid_command".to_string(),
        message: message.to_string(),
        retrying: false,
        retry_after_ms: None,
    };
    println!("{}", serde_json::to_string(&err_event).unwrap_or_default());
}

/// Headless NDJSON agent loop over stdin/stdout for AI orchestrators
async fn run_ndjson_agent(workspace: &Path, config: &Config) -> Result<()> {
    tracing::info!("Starting minicode in NDJSON streaming mode");
    // Resolve provider BEFORE announcing readiness: a misconfigured host must
    // receive an error event, not a "ready" heartbeat followed by death.
    let api_key = match config.get_api_key(&config.provider.default) {
        Ok(key) => key,
        Err(e) => {
            emit_invalid_command(&format!(
                "Startup failed: {}. Fix the configuration, then reconnect.",
                e
            ));
            return Err(e);
        }
    };
    let provider = create_provider(&config.provider.default, &api_key)?;
    let agent = AgentLoop::new(workspace, config.clone(), provider);
    let approvals = agent.approval_registry();
    let agent = std::sync::Arc::new(tokio::sync::Mutex::new(agent));

    let ready_event = AgentEvent::Heartbeat {
        timestamp: chrono::Utc::now().to_rfc3339(),
        status: "ready".to_string(),
        turn_id: None,
    };
    println!("{}", serde_json::to_string(&ready_event)?);

    // Read commands from stdin line-by-line
    use tokio::io::AsyncBufReadExt;
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();

    while let Some(line) = reader.next_line().await.map_err(error::MinicodeError::Io)? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse via serde_json::Value first: the adjacent-tag streaming
        // deserializer mis-reports "missing field `params`" for valid input,
        // while buffered from_value deserialization handles it correctly.
        let value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                emit_invalid_command(&e.to_string());
                continue;
            }
        };
        // Tolerate bare {"method":"abort"} without params.
        if value.get("method").and_then(|m| m.as_str()) == Some("abort") {
            tracing::info!("Received abort command via stdin");
            break;
        }
        let command: StdinCommand = match serde_json::from_value(value) {
            Ok(cmd) => cmd,
            Err(e) => {
                emit_invalid_command(&e.to_string());
                continue;
            }
        };

        match command {
            StdinCommand::UserInput { text } => {
                // Spawn so stdin stays readable while a turn (or its approval
                // gate) is blocked; ToolResponse lines must get through.
                let agent = agent.clone();
                tokio::spawn(async move {
                    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
                    let event_consumer = tokio::spawn(async move {
                        while let Some(event) = rx.recv().await {
                            if let Ok(json) = serde_json::to_string(&event) {
                                println!("{}", json);
                            }
                        }
                    });

                    let mut guard = agent.lock().await;
                    if let Err(e) = guard.execute_turn(&text, tx, None).await {
                        let err_event = AgentEvent::Error {
                            turn_id: None,
                            code: "execution_error".to_string(),
                            message: e.to_string(),
                            retrying: false,
                            retry_after_ms: None,
                        };
                        if let Ok(json) = serde_json::to_string(&err_event) {
                            println!("{}", json);
                        }
                    }
                    drop(guard);
                    event_consumer.await.ok();
                });
            }
            StdinCommand::Abort {} => {
                tracing::info!("Received abort command via stdin");
                break;
            }
            StdinCommand::Configure {
                auto_approve,
                model,
            } => {
                tracing::info!(?auto_approve, ?model, "Received runtime configure command");
            }
            StdinCommand::ToolResponse {
                tool_id, action, ..
            } => {
                let decision = match action.as_str() {
                    "approve" => Some(crate::agent::types::ApprovalDecision::Approve),
                    "reject" => Some(crate::agent::types::ApprovalDecision::Reject),
                    _ => None,
                };
                match decision {
                    Some(decision) => match approvals
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&tool_id)
                    {
                        Some(sender) => {
                            let _ = sender.send(decision);
                        }
                        None => {
                            tracing::warn!(tool_id = %tool_id, "ToolResponse for unknown tool_id");
                        }
                    },
                    None => {
                        tracing::warn!(action = %action, "Unknown ToolResponse action");
                    }
                }
            }
        }
    }

    Ok(())
}

/// Interactive mode entrypoint (Plain REPL or full-screen Aura Ratatui TUI)
async fn run_interactive_mode(
    workspace: &Path,
    config: &Config,
    resume_session_id: Option<&str>,
) -> Result<()> {
    let api_key = config.get_api_key(&config.provider.default)?;
    let provider = create_provider(&config.provider.default, &api_key)?;
    let agent = AgentLoop::new(workspace, config.clone(), provider);

    let past_events = if let Some(sid) = resume_session_id {
        let store = session::store::SessionStore::with_workspace(workspace);
        match store.load_session(sid) {
            Ok(events) => {
                tracing::info!(
                    session_id = sid,
                    count = events.len(),
                    "Resumed previous session history"
                );
                events
            }
            Err(e) => {
                tracing::warn!(session_id = sid, error = %e, "Failed to load previous session to resume");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    if config.ui.plain {
        println!(
            "minicode v{} (Plain Accessible REPL)",
            env!("CARGO_PKG_VERSION")
        );
        println!("Workspace: {}", workspace.display());
        println!(
            "Model: {} ({})\n",
            config.provider.model, config.provider.default
        );
        if let Some(sid) = resume_session_id {
            println!(
                "Resumed session: {} ({} events loaded)\n",
                sid,
                past_events.len()
            );
        }
        println!("Type a prompt to begin, or /exit to quit.\n");

        use std::io::{self, BufRead, Write};
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        let mut mut_agent = agent;

        loop {
            print!("❯ ");
            io::stdout().flush().ok();

            let mut input = String::new();
            if handle.read_line(&mut input).is_err() || input.is_empty() {
                break;
            }

            let trimmed = input.trim();
            if trimmed == "/exit" || trimmed == "/quit" {
                println!("Goodbye!");
                break;
            }
            if trimmed.is_empty() {
                continue;
            }

            let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
            let event_consumer = tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        AgentEvent::TurnStart { turn_id, model, .. } => {
                            println!("⚡ minicode turn #{} [{}]", turn_id, model);
                            println!("─────────────────────────────────────────");
                        }
                        AgentEvent::StreamDelta { delta, .. } => {
                            print!("{}", delta);
                            io::stdout().flush().ok();
                        }
                        AgentEvent::ToolCall { tool, args, .. } => {
                            println!("\n▶ [Tool: {}({})]", tool, args);
                        }
                        AgentEvent::ToolResult {
                            tool,
                            success,
                            output,
                            duration_ms,
                            ..
                        } => {
                            let status_symbol = if success { "✓" } else { "✗" };
                            println!(
                                "{} [{}] ({}ms): {}",
                                status_symbol,
                                tool,
                                duration_ms,
                                output.trim()
                            );
                        }
                        AgentEvent::TurnEnd {
                            total_tokens_used,
                            files_modified,
                            ..
                        } => {
                            println!("\n─────────────────────────────────────────");
                            println!(
                                "✓ Completed (tokens: {}, modified: {:?})",
                                total_tokens_used, files_modified
                            );
                        }
                        AgentEvent::Error { message, .. } => {
                            eprintln!("\n✗ Error: {}", message);
                        }
                        _ => {}
                    }
                }
            });

            if let Err(e) = mut_agent.execute_turn(trimmed, tx, None).await {
                eprintln!("Error: {}", e);
            }
            event_consumer.await.ok();
            println!();
        }
    } else {
        // Run the interactive Aura Ratatui TUI
        let mut app = App::new(workspace, config.clone());
        if !past_events.is_empty() {
            app.hydrate_session(&past_events);
        }
        app.run(agent).await?;
    }

    Ok(())
}
