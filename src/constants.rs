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
#[allow(dead_code)]
pub const SKILL_MD_FILE: &str = "SKILL.md";
/// Skills directory name (.skills)
#[allow(dead_code)]
pub const SKILLS_DIR_NAME: &str = ".skills";
/// Model cache JSON filename
pub const MODELS_CACHE_FILE: &str = "models_cache.json";

// === Agent Loop Limits ===
/// Maximum tool calling steps per turn to prevent infinite loops
pub const DEFAULT_MAX_TOOL_ITERATIONS: usize = 10;
/// Maximum API retry attempts for transient network or rate limit errors
pub const DEFAULT_MAX_RETRIES: usize = 3;
/// Exponential backoff baseline delay in seconds between retries
pub const RETRY_BACKOFF_SECS: u64 = 2;
/// Maximum token budget for conversation history before pruning old messages
pub const CONTEXT_WINDOW_PRUNE_THRESHOLD: usize = 100_000;
/// Minimum messages to always preserve (system + last N exchanges)
pub const CONTEXT_MIN_PRESERVED_MESSAGES: usize = 4;
/// Signal-killed exit code fallback when OS doesn't provide one
pub const SIGNAL_KILLED_EXIT_CODE: i32 = -1;

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
#[allow(dead_code)]
pub const JSONRPC_SERVER_ERROR: i32 = -32000;
/// MCP tools invocation method name
pub const MCP_METHOD_TOOLS_CALL: &str = "tools/call";
/// MCP tools list method name
pub const MCP_METHOD_TOOLS_LIST: &str = "tools/list";
/// MCP initialize method name
pub const MCP_METHOD_INITIALIZE: &str = "initialize";
/// MCP initialized notification method name
pub const MCP_METHOD_INITIALIZED: &str = "notifications/initialized";
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
/// Maximum lines of tool execution output displayed inline in timeline before folding
pub const UI_MAX_TOOL_OUTPUT_LINES: usize = 12;
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
/// Default User-Agent header for web fetching tool
pub const WEB_USER_AGENT: &str = concat!(
    "minicode/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/aswin402/minicode)"
);
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
/// Identifiers too common to form meaningful cross-file dependency edges
pub const CODEGRAPH_IGNORED_IDENTIFIERS: &[&str] = &[
    "new",
    "default",
    "from",
    "into",
    "get",
    "set",
    "init",
    "run",
    "test",
    "id",
    "name",
    "value",
    "result",
    "error",
    "ok",
    "err",
    "self",
    "this",
    "super",
    "None",
    "Some",
    "Ok",
    "Err",
    "true",
    "false",
    "to_string",
    "as_str",
    "clone",
    "unwrap",
    "expect",
    "map",
    "and_then",
    "is_empty",
    "len",
    "push",
    "pop",
    "insert",
    "remove",
    "contains",
    "iter",
    "collect",
];
/// Random teleport probability damping factor for PageRank
pub const PAGERANK_DAMPING: f64 = 0.85;
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

// === Security Limits ===
/// Maximum allowed web response size in bytes (10 MB) to prevent OOM
pub const MAX_WEB_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

// === Model Provider Endpoints & Timeouts ===
/// Default timeout in seconds for fetching live models
pub const MODEL_FETCH_TIMEOUT_SECS: u64 = 8;
/// Default provider timeout for streaming completions (seconds)
pub const PROVIDER_STREAM_TIMEOUT_SECS: u64 = 90;
/// Default provider timeout for non-streaming requests (seconds)
pub const PROVIDER_REQUEST_TIMEOUT_SECS: u64 = 60;
/// Gemini API base URL (without trailing path)
pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
/// OpenRouter API base URL
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// Project repository URL (used for HTTP-Referer headers)
pub const PROJECT_REPO_URL: &str = "https://github.com/aswin402/minicode";
/// OpenRouter live models API endpoint
pub const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
/// Gemini live models API endpoint
pub const GEMINI_MODELS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
/// OpenAI default API base URL
pub const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
/// DeepSeek API base URL
pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/v1";
/// Groq API base URL
pub const GROQ_BASE_URL: &str = "https://api.groq.com/openai/v1";
/// Together AI API base URL
pub const TOGETHER_BASE_URL: &str = "https://api.together.xyz/v1";
/// Ollama default API base URL
pub const OLLAMA_DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";

// === Context, Search & Channel Tuning ===
/// Weight factor for prefix token match in BM25 scoring
pub const BM25_PREFIX_WEIGHT: f64 = 0.6;
/// Number of tool observation lines to retain during context compaction
pub const COMPRESSOR_MASK_LINES: usize = 10;
/// Default capacity for bounded agent event channel
#[allow(dead_code)]
pub const AGENT_EVENT_CHANNEL_CAPACITY: usize = 1024;
/// Default limit of symbols to return for locate_symbol MCP query
pub const DEFAULT_LOCATE_SYMBOL_LIMIT: usize = 10;
/// Supported source file language extensions for AST repomap and graph extraction
pub const SUPPORTED_LANG_EXTENSIONS: &[&str] = &["rs", "py", "js", "ts", "jsx", "tsx"];

// === Web & Network Security ===
/// Default hostnames blocked from web browsing to prevent SSRF
pub const SSRF_BLOCKED_HOSTS: &[&str] = &[
    "localhost",
    "127.0.0.1",
    "::1",
    "0.0.0.0",
    "169.254.169.254",
];

// === Search Index & Process Limits ===
/// Maximum cached file symbol mappings in SearchIndex before FIFO eviction
#[allow(dead_code)]
pub const INDEX_CACHE_MAX_ENTRIES: usize = 1000;
/// Grace period in milliseconds before escalating SIGTERM to SIGKILL for child processes
pub const PROCESS_KILL_GRACE_PERIOD_MS: u64 = 500;

// === Git Operations & Diff Limits ===
/// Maximum recommended length for Git commit summary line
pub const GIT_COMMIT_MSG_MAX_LEN: usize = 72;
/// Maximum bytes of Git diff output to include in LLM context
pub const GIT_DIFF_MAX_BYTES: usize = 50_000;
/// Default number of commits to return in git_log
pub const GIT_LOG_DEFAULT_COUNT: usize = 10;
/// Default timeout in seconds for Git subprocess operations
pub const GIT_TIMEOUT_SECS: u64 = 30;
/// Common lockfiles and generated assets to condense in git diffs
pub const GIT_LOCKFILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "composer.lock",
    "Gemfile.lock",
    "poetry.lock",
    "bun.lockb",
];

// === Task Complexity & Cognitive Decay Tuning ===
/// Keywords indicating high architectural or operational risk
pub const COMPLEXITY_HIGH_RISK_TERMS: &[&str] = &[
    "refactor", "migrate", "rewrite", "database", "schema", "auth", "security", "async", "lock",
    "thread", "breaking", "api", "protocol",
];

/// Keywords indicating medium operational complexity
pub const COMPLEXITY_MEDIUM_RISK_TERMS: &[&str] = &[
    "add",
    "create",
    "implement",
    "update",
    "fix",
    "test",
    "support",
    "tool",
    "parse",
    "format",
    "render",
    "endpoint",
    "cache",
];

/// Transient memory half-life in seconds (1 hour)
pub const MEMORY_DECAY_TRANSIENT_HALF_LIFE_SECS: f32 = 3600.0;
/// Milestone memory half-life in seconds (7 days)
pub const MEMORY_DECAY_MILESTONE_HALF_LIFE_SECS: f32 = 7.0 * 24.0 * 3600.0;
/// Stability step multiplier per memory reinforcement
pub const MEMORY_DECAY_STABILITY_STEP: f32 = 0.5;
