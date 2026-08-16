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

    /// Automatically approve file writes and shell executions
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
    if let Some(Commands::Run { task }) = cli.command {
        // Headless one-shot task mode
        run_headless_task(&workspace_canonical, &config, &task, cli.json_stream).await?;
    } else if let Some(Commands::Serve { dir }) = cli.command {
        // Run as MCP server over stdio
        let target_dir = dir.unwrap_or(workspace_canonical);
        mcp::MinicodeMcpServer::run_stdio(&target_dir).await?;
    } else if cli.json_stream {
        // Headless machine-readable streaming mode over stdio
        run_ndjson_agent(&workspace_canonical, &config).await?;
    } else {
        // Interactive mode (Aura TUI or Plain REPL)
        let resume_session_id = if cli.continue_session {
            let store = session::store::SessionStore::new();
            store.get_last_session_id()
        } else {
            cli.resume
        };
        run_interactive_mode(&workspace_canonical, &config, resume_session_id.as_deref()).await?;
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

/// Headless NDJSON agent loop over stdin/stdout for AI orchestrators
async fn run_ndjson_agent(workspace: &Path, config: &Config) -> Result<()> {
    tracing::info!("Starting minicode in NDJSON streaming mode");
    let ready_event = AgentEvent::Heartbeat {
        timestamp: chrono::Utc::now().to_rfc3339(),
        status: "ready".to_string(),
        turn_id: None,
    };
    println!("{}", serde_json::to_string(&ready_event)?);

    let api_key = config.get_api_key(&config.provider.default)?;
    let provider = create_provider(&config.provider.default, &api_key)?;
    let mut agent = AgentLoop::new(workspace, config.clone(), provider);

    // Read commands from stdin line-by-line
    use tokio::io::AsyncBufReadExt;
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();

    while let Some(line) = reader.next_line().await.map_err(error::MinicodeError::Io)? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_str::<StdinCommand>(trimmed) {
            Ok(StdinCommand::UserInput { text }) => {
                let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
                let event_consumer = tokio::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        if let Ok(json) = serde_json::to_string(&event) {
                            println!("{}", json);
                        }
                    }
                });

                if let Err(e) = agent.execute_turn(&text, tx, None).await {
                    let err_event = AgentEvent::Error {
                        turn_id: None,
                        code: "execution_error".to_string(),
                        message: e.to_string(),
                        retrying: false,
                        retry_after_ms: None,
                    };
                    println!("{}", serde_json::to_string(&err_event)?);
                }
                event_consumer.await.ok();
            }
            Ok(StdinCommand::Abort {}) => {
                tracing::info!("Received abort command via stdin");
                break;
            }
            Ok(StdinCommand::Configure {
                auto_approve: _,
                model: _,
            }) => {
                tracing::info!("Received runtime configure command");
            }
            Ok(StdinCommand::ToolResponse { .. }) => {
                tracing::info!("Received tool approval/rejection response");
            }
            Err(e) => {
                let err_event = AgentEvent::Error {
                    turn_id: None,
                    code: "invalid_command".to_string(),
                    message: format!("Failed to parse stdin command: {}", e),
                    retrying: false,
                    retry_after_ms: None,
                };
                println!("{}", serde_json::to_string(&err_event)?);
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
        let store = session::store::SessionStore::new();
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
