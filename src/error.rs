#![allow(dead_code)]

use thiserror::Error;

/// Root error enum for minicode operations across all subsystems.
#[derive(Error, Debug)]
pub enum MinicodeError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("LLM Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Tool execution error: {0}")]
    Tool(#[from] ToolError),

    #[error("Context & Graph error: {0}")]
    Context(#[from] ContextError),

    #[error("Session & Storage error: {0}")]
    Session(#[from] SessionError),

    #[error("Security / Sandbox error: {0}")]
    Security(#[from] SecurityError),

    #[error("Channel communication error: {0}")]
    Channel(String),

    #[error("UI error: {0}")]
    Ui(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file at {path}: {source}")]
    FileRead {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to parse TOML configuration: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Missing required API key for provider '{provider}'. Please set {env_var} or configure it in .env")]
    MissingApiKey { provider: String, env_var: String },

    #[error("Invalid configuration value for '{key}': {reason}")]
    InvalidValue { key: String, reason: String },
}

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Rate limit exceeded (HTTP 429). Retry after {retry_after_secs:?} seconds")]
    RateLimited { retry_after_secs: Option<u64> },

    #[error("Stream decoding error: {0}")]
    StreamDecode(String),

    #[error("Unsupported model '{model}' for provider '{provider}'")]
    UnsupportedModel { model: String, provider: String },

    #[error("Tool call parsing error: {0}")]
    ToolCallParse(String),

    #[error("Context window exceeded: prompt uses {used} tokens, max is {limit}")]
    ContextWindowExceeded { used: usize, limit: usize },
}

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool '{name}' not found")]
    NotFound { name: String },

    #[error("Invalid arguments for tool '{name}': {reason}")]
    InvalidArguments { name: String, reason: String },

    #[error("File operation failed on '{path}': {source}")]
    FileOp {
        path: String,
        source: std::io::Error,
    },

    #[error("Patch application failed on '{path}': {reason}")]
    PatchFailed { path: String, reason: String },

    #[error("Command execution failed: {0}")]
    CommandExec(String),

    #[error("Command timed out after {timeout_secs} seconds")]
    CommandTimeout { timeout_secs: u64 },

    #[error("Tool execution was rejected by user: {reason}")]
    Rejected { reason: String },
}

#[derive(Error, Debug)]
pub enum ContextError {
    #[error("Tree-sitter parse error: {0}")]
    TreeSitter(String),

    #[error("Unsupported language for AST parsing: {0}")]
    UnsupportedLanguage(String),

    #[error("Token counting error: {0}")]
    TokenCount(String),

    #[error("PageRank graph computation error: {0}")]
    Graph(String),
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Session '{id}' not found at {path}")]
    NotFound { id: String, path: String },

    #[error("Failed to write session checkpoint: {0}")]
    WriteCheckpoint(String),

    #[error("No backup checkpoint available to undo for turn {turn_id}")]
    NoBackupAvailable { turn_id: usize },

    #[error("Corrupted session file at {path}: line {line_number}")]
    CorruptedFile { path: String, line_number: usize },
}

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Path traversal detected: '{path}' escapes workspace boundary '{workspace_root}'")]
    PathEscapesWorkspace {
        path: String,
        workspace_root: String,
    },

    #[error("Forbidden dangerous command: '{command}'")]
    ForbiddenCommand { command: String },

    #[error("Landlock sandbox enforcement error: {0}")]
    Landlock(String),
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for MinicodeError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        MinicodeError::Channel(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, MinicodeError>;
