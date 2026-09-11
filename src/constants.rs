//! Centralized constants for minicode to eliminate magic numbers, hardcoded paths, and protocol literals.
#![allow(dead_code)]

// === Directory & File Names ===
/// Name of global configuration directory (~/.config/minicode)
pub const CONFIG_DIR_NAME: &str = "minicode";
/// Application binary and package name
pub const APP_NAME: &str = "minicode";
/// Name of workspace-local hidden configuration directory (.minicode)
pub const WORKSPACE_DIR_NAME: &str = ".minicode";
/// Standard configuration file name (config.toml)
pub const CONFIG_FILE_NAME: &str = "config.toml";
/// Default environment variable file (.env)
pub const ENV_FILE_NAME: &str = ".env";
/// Memory storage JSON file name
pub const MEMORY_FILE: &str = "memory.json";
/// Progressive memory local JSON file name (.minicode/progressive_memory.json)
pub const PROGRESSIVE_MEMORY_FILE: &str = "progressive_memory.json";
/// Global progressive memory JSON file name (~/.config/minicode/global_memory.json)
pub const GLOBAL_PROGRESSIVE_MEMORY_FILE: &str = "global_memory.json";
/// Maximum progressive memory entries to retain per tier
pub const MAX_PROGRESSIVE_TIER_ENTRIES: usize = 100;
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
/// Active runtime process registry subdirectory name (~/.config/minicode/runtime)
pub const RUNTIME_DIR_NAME: &str = "runtime";
/// Default line count for `minicode logs` tailing
pub const DEFAULT_LOG_TAIL_LINES: usize = 50;
/// Workspace backup subdirectory name (.minicode/backups)
pub const BACKUPS_DIR_NAME: &str = "backups";
/// Workspace transactions subdirectory name (.minicode/transactions)
pub const TRANSACTIONS_DIR_NAME: &str = "transactions";
/// Workspace active transaction pointer filename (active_tx.json)
pub const TRANSACTION_ACTIVE_FILE: &str = "active_tx.json";
/// Transaction manifest filename (manifest.json)
pub const TRANSACTION_MANIFEST_FILE: &str = "manifest.json";
/// Transaction backup subdirectory name (backup)
pub const TRANSACTION_BACKUP_DIR_NAME: &str = "backup";
/// Standard skill definition markdown filename
#[allow(dead_code)]
pub const SKILL_MD_FILE: &str = "SKILL.md";
/// Skills directory name (.skills)
#[allow(dead_code)]
pub const SKILLS_DIR_NAME: &str = ".skills";
/// Model cache JSON filename
pub const MODELS_CACHE_FILE: &str = "models_cache.json";
/// Subdirectory storing active reproducer test records (.minicode/reproducers)
pub const REPRODUCER_DIR_NAME: &str = "reproducers";
/// Prefix required for standalone reproducer test files
pub const REPRODUCER_PREFIX: &str = "repro_";
/// Default timeout for single reproducer test execution in milliseconds (15s)
pub const REPRODUCER_TIMEOUT_MS: u64 = 15000;
/// Subdirectory storing diagnostic and application logs (.minicode/logs)
pub const LOGS_DIR_NAME: &str = "logs";
/// Subdirectory storing exported artifacts (.minicode/exports)
pub const EXPORTS_DIR_NAME: &str = "exports";
/// Subdirectory storing isolated git worktrees (.minicode/worktrees)
pub const WORKTREES_DIR_NAME: &str = "worktrees";
/// Subdirectory storing crawled web documentation (.minicode/crawled)
pub const CRAWLED_CACHE_DIR: &str = "crawled";
/// Subdirectory storing local workspace wiki (.minicode/wiki)
pub const WIKI_DIR_NAME: &str = "wiki";
/// Hypotheses record JSON filename (.minicode/hypotheses.json)
pub const HYPOTHESES_FILE_NAME: &str = "hypotheses.json";
/// Task DAG execution JSON filename (.minicode/task_dag.json)
pub const TASK_DAG_FILE_NAME: &str = "task_dag.json";
/// Subagent scratchpad state JSON filename (.minicode/scratchpad.json)
pub const SCRATCHPAD_FILE_NAME: &str = "scratchpad.json";
/// Binary vector embeddings cache filename (.minicode/embeddings.bin)
pub const EMBEDDINGS_CACHE_FILE: &str = "embeddings.bin";
/// Episodic conversation memory JSON filename (.minicode/episodic_memory.json)
pub const EPISODIC_MEMORY_FILE: &str = "episodic_memory.json";
/// Cached code graph JSON filename (.minicode/graph.json)
pub const GRAPH_FILE_NAME: &str = "graph.json";
/// Package manifest filename for onpkg integration
pub const ONPKG_MANIFEST_FILE: &str = "onpkg.json";
/// Documentation directory for onpkg skills
pub const ONPKG_DOCS_DIR: &str = "onpkg_docs";
/// Standard git repository hidden directory name (.git)
pub const GIT_DIR_NAME: &str = ".git";

// === Agent Loop Limits ===
/// Maximum tool calling steps per turn to prevent infinite loops
pub const DEFAULT_MAX_TOOL_ITERATIONS: usize = 10;
/// Maximum API retry attempts for transient network or rate limit errors
pub const DEFAULT_MAX_RETRIES: usize = 3;
/// Exponential backoff baseline delay in seconds between retries
pub const RETRY_BACKOFF_SECS: u64 = 2;
/// Maximum token budget for conversation history before pruning old messages
#[allow(dead_code)]
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
/// Maximum autocomplete candidates rendered in input dock
#[allow(dead_code)]
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
/// Minimum gap between best and second-best fuzzy match to prevent ambiguous edits
pub const FUZZY_UNIQUENESS_GAP: f64 = 0.12;
/// Maximum search results returned by grep tool
pub const MAX_SEARCH_RESULTS: usize = 50;
/// Maximum allowed length in characters for regular expression search queries
pub const MAX_REGEX_QUERY_LEN: usize = 1024;
/// Maximum milliseconds allowed for scoped auto-compiler / linter checks
pub const AUTO_LINT_TIMEOUT_MS: u64 = 4000;
/// Maximum number of diagnostic error snippets injected into tool output
pub const MAX_COMPILER_DIAGNOSTICS: usize = 3;
/// Maximum output lines for each compiler diagnostic snippet
pub const MAX_COMPILER_ERROR_LINES: usize = 8;
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

// === Stuck Detector & Loop Breaker ===
/// Number of consecutive identical tool calls before circuit breaker triggers
pub const STUCK_CONSECUTIVE_TOOL_CALL_THRESHOLD: usize = 3;
/// Number of consecutive identical failing tool calls before circuit breaker triggers
pub const STUCK_CONSECUTIVE_FAILURE_THRESHOLD: usize = 2;
/// Maximum entries maintained in tool call ring buffer for loop detection
pub const STUCK_MAX_HISTORY_ENTRIES: usize = 16;
/// Minimum oscillation cycles before alternating ping-pong loop is flagged
pub const STUCK_OSCILLATION_MIN_CYCLES: usize = 2;

// === Smart Donut Truncator (Phase 88) ===
/// Total line threshold before Smart Donut truncation activates
pub const DONUT_THRESHOLD_LINES: usize = 300;
/// Number of initial lines preserved from the start of tool output
pub const DONUT_HEAD_LINES: usize = 100;
/// Number of trailing lines preserved from the end of tool output
pub const DONUT_TAIL_LINES: usize = 200;
/// Maximum error and diagnostic lines extracted from the omitted middle donut section
pub const DONUT_MAX_ERROR_LINES: usize = 60;
/// Maximum character length allowed for any single line before in-line truncation
pub const DONUT_MAX_LINE_CHARS: usize = 2000;
/// Error keywords and signatures searched inside the omitted middle section
pub const DONUT_ERROR_CUES: &[&str] = &[
    "error:",
    "error[",
    "failed:",
    "failure:",
    "fatal:",
    "panic:",
    "panicked at",
    "exception:",
    "traceback (most recent call last):",
    "assertionerror",
    "undefined reference",
    "cannot find",
    "no such file",
    "syntaxerror",
    "typeerror",
    "referenceerror",
    "[error]",
    "err!",
    "critical:",
    "segmentation fault",
    "aborted",
];

// === Context Budget Bar (Phase 88) ===
/// Visual width in characters for the context budget progress bar
pub const BUDGET_PROGRESS_BAR_WIDTH: usize = 20;
/// Utilization percentage threshold that triggers high context pressure warning
pub const BUDGET_PRESSURE_HIGH_THRESHOLD: f64 = 80.0;
/// Utilization percentage threshold that triggers moderate context pressure advisory
pub const BUDGET_PRESSURE_MODERATE_THRESHOLD: f64 = 60.0;

// === 4-Gate Pre-Completion Verification Barrier (Phase 89) ===
/// Maximum verification re-prompt attempts before allowing completion fallback
pub const VERIFICATION_MAX_ATTEMPTS: usize = 2;
/// Raw debug statements forbidden in production code (Gate 4)
pub const VERIFICATION_DEBUG_PATTERNS: &[&str] =
    &["println!", "eprintln!", "console.log(", "debugger;"];
/// Git merge conflict markers that trigger failure (Gate 3)
pub const VERIFICATION_CONFLICT_MARKERS: &[&str] = &["<<<<<<<", "=======", ">>>>>>>"];
/// Timeout in milliseconds for scoped reproducer test execution
pub const VERIFICATION_TEST_TIMEOUT_MS: u64 = 5000;

// === Context / Compressor ===
/// Context window safety headroom margin below provider hard limit
/// Used in: `CompactionConfig::safety_margin` (via `ContextCompressor::compact_history`)
pub const COMPRESSOR_SAFETY_MARGIN: f64 = 0.15;
/// Preserved head and tail lines when masking verbose observations
pub const COMPRESSOR_HEAD_TAIL_LINES: usize = 15;

// === Tiered Auto-Compaction (Phase 55) ===
/// Ratio of model context window that triggers Tier 1 (Observation Masking)
pub const COMPACT_TIER1_RATIO: f64 = 0.60;
/// Ratio of model context window that triggers Tier 2 (Turn Summarization)
pub const COMPACT_TIER2_RATIO: f64 = 0.80;
/// Ratio of model context window that triggers Tier 3 (Memory Anchor & Aggressive Prune)
pub const COMPACT_TIER3_RATIO: f64 = 0.95;
/// Minimum recent messages to always preserve untouched in Tier 0 (3 turns = 6 messages)
pub const COMPACT_PRESERVE_RECENT_MESSAGES: usize = 6;
/// Maximum number of key decisions to retain in memory anchor
pub const COMPACT_MAX_DECISIONS_IN_ANCHOR: usize = 8;
/// Maximum number of file states to retain in memory anchor
pub const COMPACT_MAX_FILES_IN_ANCHOR: usize = 12;

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

// === Symbol-Level Code Graph Constants ===
/// Maximum node capacity for in-memory symbol graph
#[allow(dead_code)]
pub const SYMBOL_GRAPH_MAX_NODES: usize = 50_000;
/// Maximum edge capacity for in-memory symbol graph
#[allow(dead_code)]
pub const SYMBOL_GRAPH_MAX_EDGES: usize = 200_000;
/// Minimum identifier character length to consider for symbol cross-reference
pub const SYMBOL_REFERENCE_MIN_LEN: usize = 3;

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

// === Reciprocal Rank Fusion (RRF) Constants ===
/// RRF smoothing constant (k) to dampen outlier rankings
pub const RRF_K: f64 = 60.0;
/// Weight multiplier for BM25 lexical ranking in RRF fusion
pub const RRF_WEIGHT_LEXICAL: f64 = 1.0;
/// Weight multiplier for vector semantic similarity ranking in RRF fusion
pub const RRF_WEIGHT_VECTOR: f64 = 1.0;
/// Weight multiplier for PageRank architectural centrality in RRF fusion
pub const RRF_WEIGHT_PAGERANK: f64 = 0.5;

// === Fault Localization & Surgical Repair ===
/// Default number of candidate fault locations returned by `locate_fault`
pub const FAULT_LOCALIZATION_DEFAULT_TOP_N: usize = 3;
/// Maximum number of candidate fault locations returned by `locate_fault`
pub const FAULT_LOCALIZATION_MAX_TOP_N: usize = 10;
/// Default timeout for surgical repair verification execution in seconds (60s)
pub const REPAIR_VERIFY_TIMEOUT_SECS: u64 = 60;
/// Maximum context lines returned per candidate fault location
pub const FAULT_LOCALIZATION_CONTEXT_LINES: usize = 12;

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

// === Secret Redaction ===
/// Placeholder text used to replace detected secrets in tool outputs
pub const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

// === Timestamp Format ===
/// Standard RFC-like timestamp format for progress and memory logs
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

// === File-Modifying Tools ===
/// List of built-in tool names that modify workspace files
pub const FILE_MODIFYING_TOOLS: &[&str] = &["write_file", "patch_file"];

// === Speculative & Parallel Tool Execution ===
/// Default maximum number of concurrent read-only tools executed in parallel
pub const DEFAULT_MAX_PARALLEL_TOOLS: usize = 4;
/// Minimum bound for parallel tool execution concurrency
pub const MIN_PARALLEL_TOOLS: usize = 1;
/// Maximum ceiling for parallel tool execution concurrency
pub const MAX_PARALLEL_TOOLS_CAP: usize = 16;

// === Approval Enforcement ===
/// Tools that require user approval before dispatch when running in strict mode
pub const APPROVAL_REQUIRED_TOOLS: &[&str] =
    &["write_file", "patch_file", "exec_cmd", "sandbox_exec"];

// === Security Limits ===
/// Maximum allowed web response size in bytes (10 MB) to prevent OOM
#[allow(dead_code)]
pub const MAX_WEB_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

// === Search Engine & Web Endpoints ===
/// DuckDuckGo HTML search endpoint
pub const DUCKDUCKGO_SEARCH_URL: &str = "https://html.duckduckgo.com/html/";
/// Tavily AI search API endpoint
pub const TAVILY_SEARCH_URL: &str = "https://api.tavily.com/search";
/// Brave Search API endpoint
pub const BRAVE_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";
/// GitHub API base endpoint
pub const GITHUB_API_BASE_URL: &str = "https://api.github.com";

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
/// MiniMax API base URL
pub const MINIMAX_BASE_URL: &str = "https://api.minimaxi.chat/v1";
/// Zhipu / BigModel / Z.ai API base URL
pub const ZHIPU_BASE_URL: &str = "https://open.bigmodel.cn/api/paas/v4";
/// Mistral AI API base URL
pub const MISTRAL_BASE_URL: &str = "https://api.mistral.ai/v1";
/// Ollama default API base URL
pub const OLLAMA_DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
/// Default Ollama raw host endpoint without API version suffix
pub const DEFAULT_OLLAMA_HOST: &str = "http://localhost:11434";
/// LM Studio default OpenAI-compatible base URL
pub const LMSTUDIO_DEFAULT_BASE_URL: &str = "http://localhost:1234/v1";
/// vLLM default OpenAI-compatible base URL
pub const VLLM_DEFAULT_BASE_URL: &str = "http://localhost:8000/v1";
/// LocalAI / llama.cpp / Jan default OpenAI-compatible base URL
pub const LOCALAI_DEFAULT_BASE_URL: &str = "http://localhost:8080/v1";
/// Chrome DevTools Protocol loopback prefix
pub const CDP_HOST_PREFIX: &str = "http://127.0.0.1:";

/// Default local model name fallback
pub const DEFAULT_LOCAL_MODEL_NAME: &str = "local-model";
/// Default fallback model name for custom providers
pub const DEFAULT_FALLBACK_MODEL_NAME: &str = "default-model";

/// Default provider name
pub const DEFAULT_PROVIDER: &str = "gemini";
/// Default Gemini model
pub const DEFAULT_MODEL_GEMINI: &str = "gemini-2.5-pro";
/// OpenRouter default model used when config omits one
#[allow(dead_code)]
pub const OPENROUTER_DEFAULT_MODEL: &str = "anthropic/claude-3.5-sonnet";
/// OpenAI default model used when config omits one
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-4o";
/// DeepSeek default model used when config omits one
pub const DEEPSEEK_DEFAULT_MODEL: &str = "deepseek-coder";
/// MiniMax default model used when config omits one
pub const MINIMAX_DEFAULT_MODEL: &str = "MiniMax-Text-01";
/// Anthropic API base URL
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
/// Anthropic API version header value
pub const ANTHROPIC_VERSION_HEADER: &str = "2023-06-01";
/// Anthropic default model (Claude 3.7 Sonnet hybrid reasoning)
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-3-7-sonnet-20250219";
/// Anthropic live models API endpoint
pub const ANTHROPIC_MODELS_URL: &str = "https://api.anthropic.com/v1/models";
/// Default thinking token budget when reasoning is enabled
#[allow(dead_code)]
pub const DEFAULT_THINKING_BUDGET_TOKENS: usize = 16_000;
/// Minimum token budget required by Anthropic extended thinking
pub const MIN_THINKING_BUDGET_TOKENS: usize = 1024;
/// Maximum thinking token budget allowed
pub const MAX_THINKING_BUDGET_TOKENS: usize = 64_000;
/// Z.ai / Zhipu GLM default model (Free tier available)
pub const ZHIPU_DEFAULT_MODEL: &str = "glm-4-flash";
/// Mistral default model used when config omits one
pub const MISTRAL_DEFAULT_MODEL: &str = "codestral-latest";
/// Groq default model used when config omits one
pub const GROQ_DEFAULT_MODEL: &str = "llama-3.3-70b-versatile";
/// Together AI default model used when config omits one
pub const TOGETHER_DEFAULT_MODEL: &str = "meta-llama/Llama-3.3-70B-Instruct-Turbo";
/// Ollama default model used when config omits one
pub const OLLAMA_DEFAULT_MODEL: &str = "qwen2.5-coder";
/// vLLM default model used when config omits one
pub const VLLM_DEFAULT_MODEL: &str = "default";

/// Default fallback retry delay in seconds when 429 response omits retry-after
pub const PROVIDER_RATE_LIMIT_RETRY_DELAY_SECS: u64 = 5;
/// Additional token headroom above thinking budget required by Anthropic
pub const ANTHROPIC_THINKING_HEADROOM_TOKENS: usize = 4096;
/// Initial dummy user message used when conversation history starts with assistant
pub const ANTHROPIC_INIT_USER_PROMPT: &str = "Begin conversation.";
/// OpenAI reasoning effort token budget thresholds
pub const OPENAI_REASONING_LOW_MAX_TOKENS: usize = 4096;
pub const OPENAI_REASONING_MEDIUM_MAX_TOKENS: usize = 16_000;
/// Environment variable name for custom OpenAI API base URL override
pub const ENV_OPENAI_BASE_URL: &str = "OPENAI_BASE_URL";

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
    "metadata.google.internal",
    "instance-data",
];
/// Hostnames blocked from browser navigation (cloud metadata services, while permitting localhost dev servers)
pub const BROWSER_BLOCKED_HOSTS: &[&str] = &[
    "169.254.169.254",
    "metadata.google.internal",
    "instance-data",
];

// === Search Index & Process Limits ===
/// Maximum cached file symbol mappings in SearchIndex before FIFO eviction
#[allow(dead_code)]
pub const INDEX_CACHE_MAX_ENTRIES: usize = 1000;
/// Grace period in milliseconds before escalating SIGTERM to SIGKILL for child processes
pub const PROCESS_KILL_GRACE_PERIOD_MS: u64 = 500;

// === Network Resiliency, Circuit Breakers & Retries ===
/// Default failure threshold before tripping circuit breaker
pub const CB_DEFAULT_FAILURE_THRESHOLD: u32 = 3;
/// Default circuit breaker cooldown duration in seconds
pub const CB_DEFAULT_COOLDOWN_SECS: u64 = 10;
/// Default successful probe threshold to close half-open circuit breaker
pub const CB_DEFAULT_HALF_OPEN_SUCCESS: u32 = 2;
/// Default initial retry delay in milliseconds
pub const DEFAULT_RETRY_INITIAL_DELAY_MS: u64 = 400;
/// Default maximum retry backoff delay in seconds
pub const DEFAULT_RETRY_MAX_DELAY_SECS: u64 = 5;
/// Default exponential backoff multiplier
pub const DEFAULT_RETRY_BACKOFF_MULTIPLIER: f64 = 2.0;

/// Default timeout in seconds for GitHub REST API calls
pub const GITHUB_API_TIMEOUT_SECS: u64 = 15;
/// Default timeout in seconds for headless browser HTTP management calls
pub const BROWSER_HTTP_TIMEOUT_SECS: u64 = 10;
/// Default TTL in seconds for web search cache (15 minutes)
pub const WEB_SEARCH_CACHE_TTL_SECS: u64 = 15 * 60;
/// Maximum time window in milliseconds between double-Escape presses to exit
pub const DOUBLE_ESC_EXIT_WINDOW_MS: u64 = 1500;
/// Timeout for python syntax check in compiler tools (milliseconds)
pub const PYTHON_SYNTAX_TIMEOUT_MS: u64 = 2000;
/// Timeout for LSP client request roundtrips in seconds
pub const LSP_REQUEST_TIMEOUT_SECS: u64 = 5;
/// Timeout for LSP diagnostic drain in seconds
pub const LSP_DIAGNOSTICS_TIMEOUT_SECS: u64 = 4;
/// Timeout for GitHub API client requests in seconds
pub const GITHUB_CLIENT_TIMEOUT_SECS: u64 = 15;

// === Environment Variable Names ===
pub mod env_vars {
    pub const MINICODE_MODEL: &str = "MINICODE_MODEL";
    pub const MINICODE_PROVIDER: &str = "MINICODE_PROVIDER";
    pub const MINICODE_AUTO_APPROVE: &str = "MINICODE_AUTO_APPROVE";
    pub const MINICODE_APPROVAL_POLICY: &str = "MINICODE_APPROVAL_POLICY";
    pub const MINICODE_TEMPERATURE: &str = "MINICODE_TEMPERATURE";
    pub const MINICODE_MAX_TOKENS: &str = "MINICODE_MAX_TOKENS";
    pub const MINICODE_TIMEOUT: &str = "MINICODE_TIMEOUT";
    pub const MINICODE_PLAIN: &str = "MINICODE_PLAIN";
    pub const MINICODE_THEME: &str = "MINICODE_THEME";
    pub const MINICODE_ANIMATION: &str = "MINICODE_ANIMATION";
    pub const MINICODE_LOG_LEVEL: &str = "MINICODE_LOG_LEVEL";
    pub const MINICODE_PARALLEL_TOOLS: &str = "MINICODE_PARALLEL_TOOLS";
    pub const MINICODE_SPECULATIVE_EXECUTION: &str = "MINICODE_SPECULATIVE_EXECUTION";
    pub const MINICODE_MAX_PARALLEL_TOOLS: &str = "MINICODE_MAX_PARALLEL_TOOLS";
    pub const MINICODE_BROWSER: &str = "MINICODE_BROWSER";

    pub const OPENAI_BASE_URL: &str = "OPENAI_BASE_URL";
    pub const HOME: &str = "HOME";
    pub const TERM: &str = "TERM";
    pub const COLORTERM: &str = "COLORTERM";
    pub const TAVILY_API_KEY: &str = "TAVILY_API_KEY";
    pub const BRAVE_API_KEY: &str = "BRAVE_API_KEY";
    pub const GITHUB_TOKEN: &str = "GITHUB_TOKEN";
    pub const GH_TOKEN: &str = "GH_TOKEN";
}

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

/// File count threshold triggering high-complexity task score increase
pub const COMPLEXITY_HIGH_FILE_COUNT_THRESHOLD: usize = 5;
/// File count threshold triggering medium-complexity task score increase
pub const COMPLEXITY_MEDIUM_FILE_COUNT_THRESHOLD: usize = 2;
/// Heuristic token multiplier per predicted file in task estimation
pub const COMPLEXITY_TOKENS_PER_FILE: usize = 1200;
/// Base token overhead for task execution
pub const COMPLEXITY_BASE_TOKENS: usize = 1500;
/// Minimum floor for estimated task tokens
pub const COMPLEXITY_MIN_ESTIMATED_TOKENS: usize = 2000;

/// Transient memory half-life in seconds (1 hour)
pub const MEMORY_DECAY_TRANSIENT_HALF_LIFE_SECS: f32 = 3600.0;
/// Milestone memory half-life in seconds (7 days)
pub const MEMORY_DECAY_MILESTONE_HALF_LIFE_SECS: f32 = 7.0 * 24.0 * 3600.0;
/// Stability step multiplier per memory reinforcement
pub const MEMORY_DECAY_STABILITY_STEP: f32 = 0.5;

// === Browser Engine & CDP Automation ===
/// Default base port for Browser CDP debugging
pub const BROWSER_CDP_BASE_PORT: u16 = 9222;
/// Timeout in milliseconds for browser process startup and CDP readiness
pub const BROWSER_STARTUP_TIMEOUT_MS: u64 = 8000;
/// Timeout in milliseconds for page navigation
pub const BROWSER_NAVIGATE_TIMEOUT_MS: u64 = 15000;
/// Maximum console error lines to retain in debug collector
#[allow(dead_code)]
pub const BROWSER_MAX_CONSOLE_LINES: usize = 50;
/// Maximum screenshot size in bytes (2 MB)
#[allow(dead_code)]
pub const BROWSER_MAX_SCREENSHOT_BYTES: usize = 2 * 1024 * 1024;
/// Relative directory inside workspace for browser isolated profiles
pub const BROWSER_PROFILES_DIR: &str = ".minicode/browser_profiles";
/// Relative directory inside workspace for browser screenshots
pub const BROWSER_SCREENSHOTS_DIR: &str = ".minicode/screenshots";

// === Session & History ===
/// Maximum byte length for session preview in list_sessions_rich
pub const SESSION_PREVIEW_MAX_BYTES: usize = 60;
/// Maximum byte length for first prompt/response preview in summary
pub const SESSION_FIRST_PROMPT_MAX_BYTES: usize = 120;
/// Maximum byte length for tool output in markdown export before truncation
pub const SESSION_TOOL_OUTPUT_MAX_BYTES: usize = 1000;
/// Display columns for session ID in TUI session browser list
pub const SESSION_ID_DISPLAY_COLS: usize = 14;
/// Git commit hash short display length in bytes
pub const GIT_SHORT_HASH_BYTES: usize = 7;
/// Height in lines of each session list item (for viewport calculation)
pub const SESSION_LIST_ITEM_HEIGHT: usize = 2;
/// Maximum sessions shown in /load listing
pub const SESSION_LOAD_LIST_MAX: usize = 10;
/// Default model name when no TurnStart event found
pub const SESSION_DEFAULT_MODEL: &str = "unknown";

// === Code Health & Hardening (Phase 56) ===
/// Maximum decisions extracted per turn during summarization
pub const COMPACT_MAX_DECISIONS_PER_TURN: usize = 6;
/// Maximum errors extracted per turn during summarization
pub const COMPACT_MAX_ERRORS_PER_TURN: usize = 4;
/// Maximum characters per decision string in turn summary
pub const COMPACT_MAX_DECISION_CHARS: usize = 120;
/// Maximum characters per error trace line in turn summary
pub const COMPACT_MAX_ERROR_CHARS: usize = 150;
/// Maximum characters per working context line in memory anchor
#[allow(dead_code)]
pub const COMPACT_MAX_CONTEXT_CHARS: usize = 140;
/// Message framing token overhead in OpenAI-compatible serialization
pub const MESSAGE_FRAMING_TOKEN_OVERHEAD: usize = 4;

/// Minimum messages required in history before compaction can trigger
pub const MIN_COMPACTABLE_MESSAGES: usize = 6;

/// Minimum consecutive lines required to trigger observation deduplication
pub const MIN_LINES_FOR_DEDUPLICATION: usize = 10;
/// FNV-1a 64-bit hash offset basis
pub const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
/// FNV-1a 64-bit hash prime
pub const FNV_PRIME: u64 = 0x100000001b3;

/// Maximum matched symbol entries returned in code explore search
pub const MAX_MATCHED_ENTRIES: usize = 5;
/// Maximum source lines rendered in code explore symbol definition
pub const MAX_SOURCE_LINES: usize = 60;
/// Maximum incoming callers listed in code explore symbol view
pub const MAX_CALLERS: usize = 8;
/// Maximum outgoing callees listed in code explore symbol view
pub const MAX_CALLEES: usize = 8;

/// Total number of built-in and extended tool schemas in registry
pub const TOTAL_TOOL_COUNT: usize = 132;

// === Hierarchical Fault Localization (Phase 92) ===
/// Default candidate files to evaluate in hierarchical fault localization
pub const DEFAULT_FAULT_LOCALIZE_MAX_FILES: usize = 3;
/// Maximum candidate files to evaluate in hierarchical fault localization
pub const MAX_FAULT_LOCALIZE_FILES: usize = 5;
/// Maximum suspicious symbols localized per candidate file
pub const FAULT_LOCALIZE_MAX_SYMBOLS_PER_FILE: usize = 3;
/// Maximum lines to extract for code envelope slices
pub const FAULT_LOCALIZE_MAX_SLICE_LINES: usize = 30;
/// Line margin above and below symbol definition for envelope context
pub const FAULT_LOCALIZE_ENVELOPE_MARGIN: usize = 2;

/// Milliseconds per animation frame for TUI thinking spinner
pub const SPINNER_FRAME_MS: u64 = 80;
/// Number of lines scrolled per normal arrow/wheel event
pub const SCROLL_LINES_NORMAL: u16 = 3;

/// Maximum file size in bytes to snapshot for inline diff preview (512 KB)
pub const MAX_DIFF_SNAPSHOT_BYTES: usize = 512 * 1024;

// === Modal Layout Dimensions & Viewport (Phase 60.8) ===
/// Exit confirmation modal dialog width in columns
pub const EXIT_CONFIRM_MODAL_WIDTH: u16 = 54;
/// Exit confirmation modal dialog height in rows
pub const EXIT_CONFIRM_MODAL_HEIGHT: u16 = 8;
/// Workspace analysis modal dialog width in columns
pub const WORKSPACE_ANALYSIS_WIDTH: u16 = 78;
/// Workspace analysis modal dialog height in rows
pub const WORKSPACE_ANALYSIS_HEIGHT: u16 = 10;
/// Minimum width for session history spotlight browser
pub const SESSION_BROWSER_MIN_WIDTH: u16 = 58;
/// Maximum width for session history spotlight browser
pub const SESSION_BROWSER_MAX_WIDTH: u16 = 82;
/// Minimum height for session history spotlight browser
pub const SESSION_BROWSER_MIN_HEIGHT: u16 = 14;
/// Maximum height for session history spotlight browser
pub const SESSION_BROWSER_MAX_HEIGHT: u16 = 22;

/// Maximum visible undo checkpoints shown in viewport list before scrolling
pub const UNDO_CHECKPOINT_MAX_VISIBLE: usize = 5;
/// Maximum visible themes shown in viewport list before scrolling
pub const THEME_MODAL_MAX_VISIBLE: usize = 5;
/// Maximum files displayed in stack preview before summary truncation
pub const STACK_PREVIEW_MAX_FILES: usize = 12;
/// Maximum character length of checkpoint prompt display before truncation
pub const CHECKPOINT_PROMPT_MAX_CHARS: usize = 55;
/// Preview slice length for truncated checkpoint prompts
pub const CHECKPOINT_PROMPT_PREVIEW_CHARS: usize = CHECKPOINT_PROMPT_MAX_CHARS - 3;
/// Number of files shown inline for undo checkpoints before "+N more"
pub const CHECKPOINT_FILES_PREVIEW: usize = 2;

/// Width in columns for theme name column in theme selector
pub const THEME_NAME_DISPLAY_COLS: usize = 22;
/// Width in columns for command name in catalog list
pub const COMMAND_NAME_DISPLAY_COLS: usize = 10;
/// Width in columns for layer badge in code explorer
pub const EXPLORER_BADGE_DISPLAY_COLS: usize = 10;
/// Width in columns for time-ago column in session history browser
pub const SESSION_TIME_AGO_COLS: usize = 8;

/// Default placeholder text displayed in the interactive input dock textarea
pub const DEFAULT_INPUT_PLACEHOLDER: &str = "Ask MiniCode to do anything...";
/// Height in rows of the floating spotlight command palette
pub const COMMAND_PALETTE_HEIGHT: u16 = 10;
/// Width percentage of screen for floating spotlight command palette
pub const COMMAND_PALETTE_WIDTH_PCT: u16 = 64;
/// Minimum width in columns for floating spotlight command palette
pub const COMMAND_PALETTE_MIN_WIDTH: u16 = 62;
/// Maximum width in columns for floating spotlight command palette
pub const COMMAND_PALETTE_MAX_WIDTH: u16 = 78;

// === Modal Percentage Dimensions (Screen % Width and Height) ===
pub const PROVIDER_SELECT_WIDTH_PCT: u16 = 50;
pub const PROVIDER_SELECT_HEIGHT_PCT: u16 = 45;
pub const STREAMING_SELECT_HEIGHT_PCT: u16 = 35;
pub const MODEL_SELECT_WIDTH_PCT: u16 = 75;
pub const MODEL_SELECT_HEIGHT_PCT: u16 = 70;
pub const UNDO_CHECKPOINT_WIDTH_PCT: u16 = 72;
pub const UNDO_CHECKPOINT_HEIGHT_PCT: u16 = 65;
pub const THEME_SELECT_WIDTH_PCT: u16 = 74;
pub const THEME_SELECT_HEIGHT_PCT: u16 = 68;
pub const HELP_WIDTH_PCT: u16 = 60;
pub const HELP_HEIGHT_PCT: u16 = 50;
pub const STACK_SELECT_WIDTH_PCT: u16 = 84;
pub const STACK_SELECT_HEIGHT_PCT: u16 = 76;
pub const COMMAND_CATALOG_WIDTH_PCT: u16 = 82;
pub const COMMAND_CATALOG_HEIGHT_PCT: u16 = 74;
pub const GIT_DIFF_WIDTH_PCT: u16 = 88;
pub const GIT_DIFF_HEIGHT_PCT: u16 = 84;
pub const CODE_EXPLORER_WIDTH_PCT: u16 = 85;
pub const CODE_EXPLORER_HEIGHT_PCT: u16 = 80;
pub const ARCHITECTURE_MODAL_WIDTH_PCT: u16 = 82;
pub const ARCHITECTURE_MODAL_HEIGHT_PCT: u16 = 80;
pub const API_KEY_MODAL_WIDTH_PCT: u16 = 65;
pub const API_KEY_MODAL_HEIGHT_PCT: u16 = 30;

/// Two-column modal primary column width percentage (e.g. skills/tools left list)
pub const MODAL_SPLIT_PRIMARY_PERCENT: u16 = 42;
/// Two-column modal secondary column width percentage (e.g. skills/tools right details)
pub const MODAL_SPLIT_SECONDARY_PERCENT: u16 = 58;
/// Standard modal search input height in lines
pub const MODAL_SEARCH_INPUT_HEIGHT: u16 = 3;
/// Percentage of screen height allocated to the terminal drawer overlay
pub const PTY_DRAWER_HEIGHT_PERCENT: u16 = 40;
/// Percentage of screen height allocated to the subagent swarm activity drawer overlay
pub const SUBAGENT_DRAWER_HEIGHT_PERCENT: u16 = 45;
/// Default subagent task execution timeout in seconds
pub const DEFAULT_SUBAGENT_TIMEOUT_SECS: u64 = 120;
/// Width in columns for right-aligned token metrics and dollar spend in bottom status bar
pub const STATUS_BAR_RIGHT_METRICS_WIDTH: u16 = 36;
/// Maximum lines of diff preview shown in interactive approval modal
pub const APPROVAL_DIFF_PREVIEW_LINES: usize = 6;
/// Default context lines shown around diff hunks
pub const DIFF_CONTEXT_LINES: usize = 3;

// === UI Animation ===
/// Standard 10-frame braille spinner sequence for async tool/agent activity
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// === Flaky Test Quarantine (Phase 102) ===
/// Default number of burn-in runs to evaluate for statistical test variance
pub const DEFAULT_FLAKY_RUNS: usize = 5;
/// Maximum allowed burn-in runs for test variance analysis
pub const MAX_FLAKY_RUNS: usize = 10;
/// Minimum allowed burn-in runs for test variance analysis
pub const MIN_FLAKY_RUNS: usize = 2;
/// Execution timeout in seconds for a single burn-in test run
pub const FLAKY_TEST_TIMEOUT_SECS: u64 = 15;
/// Persistent quarantine filename under .minicode/
pub const QUARANTINE_FILE_NAME: &str = "quarantine.json";

// === Semantic Commit Synthesis (Phase 103) ===
/// Default max length for synthesized conventional commit title summary
pub const MAX_COMMIT_SUMMARY_LEN: usize = 72;

// === Architectural Governance (Phase 105) ===
/// Maximum permitted LOC before a source file is flagged as a high-complexity God File
pub const ARCH_GOD_FILE_LOC_THRESHOLD: usize = 1_000;
/// Outgoing import threshold before a source file is flagged as a Fan-Out spike
pub const ARCH_FAN_OUT_THRESHOLD: usize = 15;
/// Minimum architecture health score required to pass pre-completion verification
pub const ARCH_MIN_HEALTH_SCORE: u32 = 70;

// === Automated Tool Count Validation ===
// This test ensures TOTAL_TOOL_COUNT stays in sync with the live registry.
// If the count is wrong, update TOTAL_TOOL_COUNT to match the actual schema count.
#[cfg(test)]
mod tool_count_validation {
    use super::*;

    #[test]
    fn total_tool_count_matches_registry() {
        let actual = crate::tools::ToolRegistry::get_tool_schemas().len();
        assert_eq!(
            actual, TOTAL_TOOL_COUNT,
            "TOTAL_TOOL_COUNT ({}) doesn't match actual tool schemas ({}). Update the constant in constants.rs.",
            TOTAL_TOOL_COUNT, actual
        );
    }
}
