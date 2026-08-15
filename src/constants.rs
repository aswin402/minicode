//! Centralized constants for minicode to eliminate magic numbers, hardcoded paths, and protocol literals.

// === Directory & File Names ===
/// Name of global configuration directory (~/.config/minicode)
pub const CONFIG_DIR_NAME: &str = "minicode";
/// Name of workspace-local hidden configuration directory (.minicode)
pub const WORKSPACE_DIR_NAME: &str = ".minicode";
/// Standard configuration file name (config.toml)
pub const CONFIG_FILE_NAME: &str = "config.toml";
/// Default environment variable file (.env)
pub const ENV_FILE_NAME: &str = ".env";
/// Memory storage JSON file name
pub const MEMORY_FILE: &str = "memory.json";
/// Subdirectory storing active task plans
pub const PLAN_DIR: &str = "plan";
/// Active task plan markdown filename
pub const TASK_PLAN_FILE: &str = "task_plan.md";
/// Active task findings markdown filename
pub const FINDINGS_FILE: &str = "findings.md";
/// Progress tracking markdown filename
pub const PROGRESS_FILE: &str = "progress.md";
/// Subdirectory storing archived plans
pub const ARCHIVE_DIR: &str = "archive";
/// Workspace repository instructions filename
pub const AGENTS_MD_FILE: &str = "AGENTS.md";
/// MCP server configuration JSON filename
pub const MCP_CONFIG_FILE: &str = "mcp.json";

// === Agent Loop Limits ===
/// Maximum tool calling steps per turn to prevent infinite loops
pub const DEFAULT_MAX_TOOL_ITERATIONS: usize = 10;
/// Maximum API retry attempts for transient network or rate limit errors
pub const DEFAULT_MAX_RETRIES: usize = 3;
/// Exponential backoff baseline delay in seconds between retries
pub const RETRY_BACKOFF_SECS: u64 = 2;

// === MCP Protocol ===
/// Standard JSON-RPC protocol version string
pub const JSONRPC_VERSION: &str = "2.0";
/// Supported Model Context Protocol specification date version
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
/// JSON-RPC parse error code
pub const JSONRPC_PARSE_ERROR: i32 = -32700;
/// JSON-RPC invalid request error code
pub const JSONRPC_INVALID_REQUEST: i32 = -32600;
/// JSON-RPC method not found error code
pub const JSONRPC_METHOD_NOT_FOUND: i32 = -32601;
/// JSON-RPC invalid parameters error code
pub const JSONRPC_INVALID_PARAMS: i32 = -32602;
/// JSON-RPC server error code
pub const JSONRPC_SERVER_ERROR: i32 = -32000;
/// MCP tools invocation method name
pub const MCP_METHOD_TOOLS_CALL: &str = "tools/call";
/// Default timeout for external MCP tool invocations
pub const DEFAULT_MCP_TIMEOUT_SECS: u64 = 30;

// === Compactor Thresholds ===
/// Line count threshold for compacting verbose git diff output
pub const GIT_DIFF_COMPACT_THRESHOLD: usize = 100;
/// Maximum lines preserved in git log compaction
pub const GIT_LOG_MAX_LINES: usize = 40;
/// Generic command output compaction line threshold
pub const GENERIC_COMPACT_THRESHOLD: usize = 50;
/// Preserved head lines when truncating generic command output
pub const GENERIC_HEAD_LINES: usize = 30;
/// Preserved tail lines when truncating generic command output
pub const GENERIC_TAIL_LINES: usize = 15;

// === Working Memory Prompt Limits ===
/// Maximum lines of active task plan injected into system prompt
pub const MAX_PLAN_LINES_IN_PROMPT: usize = 20;
/// Maximum entries before progress log is truncated in system prompt
pub const PROGRESS_TRUNCATE_THRESHOLD: usize = 10;
/// Maximum bytes of AGENTS.md injected into system prompt
pub const MAX_AGENTS_MD_BYTES: usize = 8192;

// === UI ===
/// TUI event poll tick rate in milliseconds
pub const TICK_RATE_MS: u64 = 50;
/// Number of lines scrolled per PageUp / PageDown keypress
pub const PAGE_SCROLL_LINES: u16 = 4;
/// Maximum autocomplete candidates rendered in input dock
pub const MAX_AUTOCOMPLETE_ROWS: usize = 4;
/// Default token budget for AST repository map
pub const DEFAULT_MAP_TOKENS: usize = 1024;
/// Cache TTL in seconds for background git branch queries
pub const GIT_BRANCH_CACHE_TTL_SECS: u64 = 5;
/// ASCII banner wordmark lines rendered in welcome timeline
pub const ASCII_WORDMARK_LINES: &[&str] = &[
    "   ___ ___                           _     ",
    "  |   Y   | _   ___  _   ___  ___  _| | ___ ",
    "  |.      || | |   || | |  _|| . || . || -_|",
    "  |. \\_/  ||_| |_|_||_| |___||___||___||___|",
];

// === Tools Configuration ===
/// Similarity threshold for sliding-window fuzzy file patching
pub const FUZZY_MATCH_THRESHOLD: f64 = 0.85;
/// Maximum search results returned by grep tool
pub const MAX_SEARCH_RESULTS: usize = 50;
/// Default execution timeout in seconds for exec_cmd
pub const EXEC_DEFAULT_TIMEOUT_SECS: u64 = 30;
/// Maximum raw output bytes captured before hard truncation
pub const EXEC_MAX_OUTPUT_BYTES: usize = 512 * 1024;
/// User-Agent header string for HTTP web requests
pub const WEB_USER_AGENT: &str = "minicode/0.0.4 (+https://github.com/aswin402/minicode)";
/// HTTP request timeout in seconds for web fetching
pub const WEB_TIMEOUT_SECS: u64 = 15;
/// Maximum response body bytes retained from web pages
pub const WEB_MAX_BODY_BYTES: usize = 40 * 1024;

// === Context / Compressor ===
/// Context window utilization threshold to trigger observation masking
pub const COMPRESSOR_WARNING_THRESHOLD: f64 = 0.70;
/// Context window safety headroom margin below provider hard limit
pub const COMPRESSOR_SAFETY_MARGIN: f64 = 0.15;
/// Preserved head and tail lines when masking verbose observations
pub const COMPRESSOR_HEAD_TAIL_LINES: usize = 15;

// === Graph / PageRank ===
/// Random teleport probability damping factor for PageRank
pub const PAGERANK_DAMPING: f64 = 0.85;
/// Number of power-iteration cycles for PageRank convergence
pub const PAGERANK_ITERATIONS: usize = 20;
/// Personalization score boost for files currently open or mentioned
pub const PAGERANK_PERSONALIZATION_BIAS: f64 = 0.3;

// === Timestamp Format ===
/// Standard RFC-like timestamp format for progress and memory logs
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

// === File-Modifying Tools ===
/// List of built-in tool names that modify workspace files
pub const FILE_MODIFYING_TOOLS: &[&str] = &["write_file", "patch_file"];
