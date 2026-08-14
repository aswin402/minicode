mod agent;
mod app;
mod config;
mod context;
mod error;
mod logging;
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
use tokio::sync::mpsc;
use ui::ConfigMenu;

#[derive(Parser, Debug)]
#[command(
    name = "minicode",
    version,
    about = "Fast, minimalist Rust-native TUI + CLI coding agent for humans and AI agents"
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    } else if cli.json_stream {
        // Headless machine-readable streaming mode over stdio
        run_ndjson_agent(&workspace_canonical, &config).await?;
    } else {
        // Interactive mode (Aura TUI or Plain REPL)
        run_interactive_mode(&workspace_canonical, &config).await?;
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

    agent.execute_turn(task, tx).await?;
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

                if let Err(e) = agent.execute_turn(&text, tx).await {
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
async fn run_interactive_mode(workspace: &Path, config: &Config) -> Result<()> {
    let api_key = config
        .get_api_key(&config.provider.default)
        .unwrap_or_default();
    let provider = match create_provider(&config.provider.default, &api_key) {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "Note: API key for provider '{}' not found in environment or .env.",
                config.provider.default
            );
            eprintln!("Run `minicode configure` or set OPENROUTER_API_KEY in .env.");
            create_provider("gemini", "mock_key")?
        }
    };

    let agent = AgentLoop::new(workspace, config.clone(), provider);

    if config.ui.plain {
        println!("minicode v0.1.0 (Plain Accessible REPL)");
        println!("Workspace: {}", workspace.display());
        println!(
            "Model: {} ({})\n",
            config.provider.model, config.provider.default
        );
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

            if let Err(e) = mut_agent.execute_turn(trimmed, tx).await {
                eprintln!("Error: {}", e);
            }
            event_consumer.await.ok();
            println!();
        }
    } else {
        // Run the interactive Aura Ratatui TUI
        let mut app = App::new(workspace, config.clone());
        app.run(agent).await?;
    }

    Ok(())
}
