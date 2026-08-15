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
/// Session store subdirectory name (~/.config/minicode/sessions)
pub const SESSIONS_DIR_NAME: &str = "sessions";
/// Workspace backup subdirectory name (.minicode/backups)
pub const BACKUPS_DIR_NAME: &str = "backups";
/// Standard skill definition markdown filename
pub const SKILL_MD_FILE: &str = "SKILL.md";
/// Skills directory name (.skills)
pub const SKILLS_DIR_NAME: &str = ".skills";

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
/// Namespace prefix for tools registered from external MCP servers
pub const MCP_TOOL_PREFIX: &str = "mcp__";

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
/// Maximum allowed length in characters for regular expression search queries
pub const MAX_REGEX_QUERY_LEN: usize = 1024;
/// Default execution timeout in seconds for exec_cmd
pub const EXEC_DEFAULT_TIMEOUT_SECS: u64 = 30;
/// Maximum raw output bytes captured before hard truncation
pub const EXEC_MAX_OUTPUT_BYTES: usize = 512 * 1024;
/// User-Agent header string for HTTP web requests
pub const WEB_USER_AGENT: &str = "minicode/0.0.6 (+https://github.com/aswin402/minicode)";
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

// === Graph / PageRank & Blast Radius ===
/// Random teleport probability damping factor for PageRank
pub const PAGERANK_DAMPING: f64 = 0.85;
/// Number of power-iteration cycles for PageRank convergence
pub const PAGERANK_ITERATIONS: usize = 20;
/// Personalization score boost for files currently open or mentioned
pub const PAGERANK_PERSONALIZATION_BIAS: f64 = 0.3;
/// Maximum search depth (hops) for transitive BFS blast radius analysis
pub const BLAST_RADIUS_MAX_HOPS: usize = 3;
/// Direct dependents count threshold triggering CRITICAL risk rating
pub const BLAST_RADIUS_CRITICAL_DIRECT: usize = 10;
/// Transitive dependents count threshold triggering CRITICAL risk rating
pub const BLAST_RADIUS_CRITICAL_TRANSITIVE: usize = 20;
/// Non-test direct dependents threshold triggering HIGH risk rating
pub const BLAST_RADIUS_HIGH_DIRECT: usize = 5;
/// Non-test direct dependents threshold triggering HIGH risk rating when tests are absent
pub const BLAST_RADIUS_HIGH_NO_TESTS: usize = 2;
/// Non-test direct dependents threshold triggering MEDIUM risk rating
pub const BLAST_RADIUS_MEDIUM_DIRECT: usize = 1;
/// Transitive dependents threshold triggering MEDIUM risk rating
pub const BLAST_RADIUS_MEDIUM_TRANSITIVE: usize = 3;

// === Symbol Index & BM25 Ranking ===
/// Exact symbol name match score
pub const SYMBOL_EXACT_MATCH_SCORE: f64 = 100.0;
/// Prefix symbol name match score
pub const SYMBOL_PREFIX_MATCH_SCORE: f64 = 50.0;
/// BM25 term saturation parameter (k1)
pub const BM25_K1: f64 = 1.2;
/// BM25 document length normalization parameter (b)
pub const BM25_B: f64 = 0.75;
/// Boost added to BM25 score for structural type definitions (struct, class, interface, trait, enum)
pub const SYMBOL_DEF_KIND_BOOST: f64 = 3.0;
/// Boost added to BM25 score for callable function definitions
pub const SYMBOL_FUNC_KIND_BOOST: f64 = 2.0;
/// Penalty multiplier applied to search score for test and mock files
pub const SYMBOL_TEST_PENALTY_FACTOR: f64 = 0.5;

// === Sandbox Environment Whitelist & Blacklist ===
/// Standard environment variables permitted through execution sandbox
pub const WHITELIST_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "USER", "LANG", "LC_ALL", "TERM", "SHELL", "EDITOR", "TMPDIR", "PWD",
];

/// Substrings indicating confidential credentials in environment keys
pub const SECRET_PATTERNS: &[&str] = &[
    "KEY",
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "AUTH",
    "BEARER",
    "PRIVATE",
    "SIGNING",
    "CERTIFICATE",
    "DATABASE_URL",
    "CONN_STR",
    "DSN",
    "SSH_AUTH_SOCK",
    "KUBECONFIG",
    "DOCKER_HOST",
];

/// Vendor key prefixes blocked from execution sandbox
pub const BLOCKED_PREFIXES: &[&str] = &[
    "AWS_",
    "GITHUB_",
    "OPENAI_",
    "GEMINI_",
    "ANTHROPIC_",
    "DEEPSEEK_",
    "MISTRAL_",
    "GROQ_",
    "COHERE_",
    "OLLAMA_",
    "CLERK_",
    "SUPABASE_",
    "FIREBASE_",
    "SENTRY_",
    "VERCEL_",
    "NETLIFY_",
    "HEROKU_",
    "DIGITALOCEAN_",
    "CLOUDFLARE_",
];

// === Timestamp Format ===
/// Standard RFC-like timestamp format for progress and memory logs
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

// === File-Modifying Tools ===
/// List of built-in tool names that modify workspace files
pub const FILE_MODIFYING_TOOLS: &[&str] = &["write_file", "patch_file"];
