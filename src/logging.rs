use std::path::{Path, PathBuf};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initializes the file-based tracing subsystem.
///
/// Returns a `WorkerGuard` that MUST be held in `main()` until the process terminates
/// to ensure buffered log entries are flushed to disk.
pub fn init_logging(
    log_dir: Option<&Path>,
    log_level: &str,
    enable_stdout: bool,
) -> anyhow::Result<WorkerGuard> {
    let resolved_log_dir = match log_dir {
        Some(dir) => dir.to_path_buf(),
        None => default_log_dir(),
    };

    std::fs::create_dir_all(&resolved_log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&resolved_log_dir, "minicode.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level.to_lowercase()));

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true);

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer);

    if enable_stdout {
        let stdout_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_target(false);
        registry.with(stdout_layer).try_init().ok();
    } else {
        registry.try_init().ok();
    }

    tracing::info!(
        log_dir = %resolved_log_dir.display(),
        level = %log_level,
        "Logging subsystem initialized"
    );

    Ok(guard)
}

fn default_log_dir() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("minicode").join("logs")
    } else {
        PathBuf::from(".minicode").join("logs")
    }
}
