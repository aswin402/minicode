use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Unique identifier for an agent (coordinator or subagent)
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

#[allow(dead_code)]
impl AgentId {
    /// Returns the standard identifier for the coordinator / parent agent
    pub fn parent() -> Self {
        AgentId("parent".to_string())
    }

    /// Generates a unique subagent identifier with a role prefix and short UUID
    pub fn new_subagent(prefix: &str) -> Self {
        let simple = uuid::Uuid::new_v4().simple().to_string();
        AgentId(format!("{}-{}", prefix, &simple[..8]))
    }

    /// Returns true if this agent represents the coordinator / parent agent
    pub fn is_parent(&self) -> bool {
        self.0 == "parent"
    }

    /// Returns a string slice of the agent id
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        AgentId(s.to_string())
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        AgentId(s)
    }
}

/// Workspace isolation mode for a subagent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    Auto,
    Worktree,
    Shared,
}

#[allow(dead_code)]
impl WorkspaceMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceMode::Auto => "auto",
            WorkspaceMode::Worktree => "worktree",
            WorkspaceMode::Shared => "shared",
        }
    }
}

/// Specialized role preset defining subagent capabilities and workspace isolation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentRole {
    Scout,
    Architect,
    Planner,
    Designer,
    Frontend,
    Backend,
    Coder,
    Vision,
    ContentWriter,
    Debugger,
    Reviewer,
    Security,
    Tester,
    Simplifier,
}

struct RoleMeta {
    slug: &'static str,
    badge: &'static str,
    aliases: &'static [&'static str],
    capability_keywords: &'static [&'static str],
    is_mutating: bool,
    token_budget: usize,
    max_turns: usize,
    spec: &'static str,
}

#[allow(dead_code)]
impl SubagentRole {
    /// Returns all 14 built-in MiniPub specialist roles
    pub const fn all_roles() -> &'static [SubagentRole] {
        &[
            SubagentRole::Scout,
            SubagentRole::Architect,
            SubagentRole::Planner,
            SubagentRole::Designer,
            SubagentRole::Frontend,
            SubagentRole::Backend,
            SubagentRole::Coder,
            SubagentRole::Vision,
            SubagentRole::ContentWriter,
            SubagentRole::Debugger,
            SubagentRole::Reviewer,
            SubagentRole::Security,
            SubagentRole::Tester,
            SubagentRole::Simplifier,
        ]
    }

    const fn meta(&self) -> RoleMeta {
        let default_budget = crate::constants::DEFAULT_MINIPUB_TOKEN_BUDGET;
        let default_turns = crate::constants::DEFAULT_MINIPUB_MAX_TURNS;
        match self {
            SubagentRole::Scout => RoleMeta {
                slug: "scout",
                badge: "Scout",
                aliases: &[
                    "explore",
                    "explorer",
                    "librarian",
                    "researcher",
                    "research",
                    "document_specialist",
                    "doc_specialist",
                ],
                capability_keywords: &[
                    "discover",
                    "locate",
                    "find",
                    "search",
                    "investigate",
                    "survey",
                    "crawl",
                    "map",
                    "inventory",
                ],
                is_mutating: false,
                token_budget: 32_000,
                max_turns: 12,
                spec: include_str!("agents/scout.md"),
            },
            SubagentRole::Architect => RoleMeta {
                slug: "architect",
                badge: "Architect",
                aliases: &[
                    "architecture",
                    "system_architect",
                    "analyst",
                    "oracle",
                    "metis",
                    "ultrabrain",
                    "deep_reasoning",
                ],
                capability_keywords: &[
                    "architecture",
                    "blast",
                    "radius",
                    "dependency",
                    "coupling",
                    "layer",
                    "module",
                    "boundary",
                    "tradeoff",
                    "invariant",
                ],
                is_mutating: false,
                token_budget: 36_000,
                max_turns: 10,
                spec: include_str!("agents/architect.md"),
            },
            SubagentRole::Planner => RoleMeta {
                slug: "planner",
                badge: "Planner",
                aliases: &["plan", "task_planner", "critic", "prometheus", "momus"],
                capability_keywords: &[
                    "roadmap",
                    "milestone",
                    "breakdown",
                    "sequence",
                    "decompose",
                    "specification",
                    "brainstorm",
                    "checklist",
                ],
                is_mutating: false,
                token_budget: 36_000,
                max_turns: 10,
                spec: include_str!("agents/planner.md"),
            },
            SubagentRole::Designer => RoleMeta {
                slug: "designer",
                badge: "Designer",
                aliases: &["ui_designer", "ux_designer", "ui_ux", "design", "artistry"],
                capability_keywords: &[
                    "palette",
                    "typography",
                    "spacing",
                    "wireframe",
                    "theme",
                    "gradient",
                    "aesthetic",
                    "tokens",
                ],
                is_mutating: true,
                token_budget: default_budget,
                max_turns: default_turns,
                spec: include_str!("agents/designer.md"),
            },
            SubagentRole::Frontend => RoleMeta {
                slug: "frontend",
                badge: "Frontend",
                aliases: &[
                    "frontend_specialist",
                    "ui_specialist",
                    "web_frontend",
                    "react",
                    "visual_engineering",
                    "ui_engineering",
                ],
                capability_keywords: &[
                    "component",
                    "navbar",
                    "hero",
                    "sidebar",
                    "modal",
                    "tailwind",
                    "css",
                    "html",
                    "tsx",
                    "jsx",
                    "dom",
                    "responsive",
                ],
                is_mutating: true,
                token_budget: default_budget,
                max_turns: default_turns,
                spec: include_str!("agents/frontend.md"),
            },
            SubagentRole::Backend => RoleMeta {
                slug: "backend",
                badge: "Backend",
                aliases: &[
                    "backend_specialist",
                    "api_specialist",
                    "systems",
                    "server",
                    "database",
                ],
                capability_keywords: &[
                    "endpoint",
                    "router",
                    "middleware",
                    "sql",
                    "postgres",
                    "sqlite",
                    "schema",
                    "migration",
                    "auth",
                    "handler",
                    "grpc",
                ],
                is_mutating: true,
                token_budget: default_budget,
                max_turns: default_turns,
                spec: include_str!("agents/backend.md"),
            },
            SubagentRole::Coder => RoleMeta {
                slug: "coder",
                badge: "Coder",
                aliases: &[
                    "executor",
                    "sisyphus_junior",
                    "hephaestus",
                    "atlas",
                    "quick",
                    "deep",
                    "unspecified_low",
                    "unspecified_high",
                    "developer",
                    "engineer",
                    "implementer",
                ],
                capability_keywords: &[
                    "implement",
                    "build",
                    "create",
                    "feature",
                    "write",
                    "code",
                    "add",
                    "integrate",
                ],
                is_mutating: true,
                token_budget: default_budget,
                max_turns: default_turns,
                spec: include_str!("agents/coder.md"),
            },
            SubagentRole::Vision => RoleMeta {
                slug: "vision",
                badge: "Vision",
                aliases: &[
                    "looker",
                    "multimodal",
                    "multimodal_looker",
                    "visual",
                    "visual_qa",
                    "image_inspector",
                ],
                capability_keywords: &[
                    "screenshot",
                    "pixel",
                    "contrast",
                    "overflow",
                    "viewport",
                    "alignment",
                    "image",
                    "render",
                ],
                is_mutating: false,
                token_budget: 32_000,
                max_turns: 12,
                spec: include_str!("agents/vision.md"),
            },
            SubagentRole::ContentWriter => RoleMeta {
                slug: "content_writer",
                badge: "Writer",
                aliases: &[
                    "writer",
                    "writing",
                    "copywriter",
                    "docs",
                    "documentation",
                    "technical_writer",
                ],
                capability_keywords: &[
                    "readme",
                    "changelog",
                    "guide",
                    "tutorial",
                    "docstring",
                    "markdown",
                    "copywriting",
                ],
                is_mutating: true,
                token_budget: 28_000,
                max_turns: 12,
                spec: include_str!("agents/content-writer.md"),
            },
            SubagentRole::Debugger => RoleMeta {
                slug: "debugger",
                badge: "Debugger",
                aliases: &["debug", "tracer", "bug_fixer", "root_cause"],
                capability_keywords: &[
                    "crash",
                    "panic",
                    "regression",
                    "backtrace",
                    "deadlock",
                    "leak",
                    "race",
                    "broken",
                    "diagnose",
                    "fault",
                ],
                is_mutating: true,
                token_budget: 40_000,
                max_turns: 14,
                spec: include_str!("agents/debugger.md"),
            },
            SubagentRole::Reviewer => RoleMeta {
                slug: "reviewer",
                badge: "Reviewer",
                aliases: &["code_reviewer", "code_review", "review"],
                capability_keywords: &[
                    "diff",
                    "critique",
                    "compliance",
                    "rubric",
                    "inspect",
                    "approve",
                    "pr",
                ],
                is_mutating: false,
                token_budget: 28_000,
                max_turns: 10,
                spec: include_str!("agents/reviewer.md"),
            },
            SubagentRole::Security => RoleMeta {
                slug: "security",
                badge: "Security",
                aliases: &[
                    "security_reviewer",
                    "security_auditor",
                    "security_audit",
                    "secops",
                    "pentester",
                ],
                capability_keywords: &[
                    "vulnerability",
                    "injection",
                    "xss",
                    "csrf",
                    "secret",
                    "credential",
                    "permission",
                    "sanitize",
                    "cve",
                    "threat",
                ],
                is_mutating: false,
                token_budget: 28_000,
                max_turns: 10,
                spec: include_str!("agents/security.md"),
            },
            SubagentRole::Tester => RoleMeta {
                slug: "tester",
                badge: "Tester",
                aliases: &[
                    "test_engineer",
                    "test",
                    "test_verification",
                    "verifier",
                    "qa",
                    "qa_tester",
                ],
                capability_keywords: &[
                    "assertion",
                    "coverage",
                    "unit",
                    "integration",
                    "e2e",
                    "tdd",
                    "fixture",
                    "benchmark",
                ],
                is_mutating: true,
                token_budget: 40_000,
                max_turns: 14,
                spec: include_str!("agents/tester.md"),
            },
            SubagentRole::Simplifier => RoleMeta {
                slug: "simplifier",
                badge: "Simplifier",
                aliases: &["code_simplifier", "refactor", "refactorer", "cleaner"],
                capability_keywords: &[
                    "simplify",
                    "deduplicate",
                    "cleanup",
                    "complexity",
                    "smell",
                    "codemod",
                    "prune",
                ],
                is_mutating: true,
                token_budget: 40_000,
                max_turns: 12,
                spec: include_str!("agents/simplifier.md"),
            },
        }
    }

    /// Returns the string representation of this role
    pub fn as_str(&self) -> &'static str {
        self.meta().slug
    }

    /// Returns true if this role modifies workspace files by default
    pub fn is_mutating(&self) -> bool {
        self.meta().is_mutating
    }

    /// Returns the default workspace isolation mode for this role
    pub fn default_workspace_mode(&self) -> WorkspaceMode {
        if self.is_mutating() {
            WorkspaceMode::Worktree
        } else {
            WorkspaceMode::Shared
        }
    }

    /// Returns the tool filtering mode ("read_only" vs "standard")
    pub fn tool_filter_mode(&self) -> &'static str {
        if self.is_mutating() {
            "standard"
        } else {
            "read_only"
        }
    }

    /// Human-readable badge for UI rendering
    pub fn badge(&self) -> &'static str {
        self.meta().badge
    }

    /// Declarative role & semantic category resolver supporting OMC, oh-my-opencode, and intent scoring.
    pub fn from_str_loose(s: &str) -> Self {
        let normalized = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        for role in Self::all_roles() {
            let meta = role.meta();
            if meta.slug == normalized || meta.aliases.contains(&normalized.as_str()) {
                return *role;
            }
        }
        Self::infer_from_intent(s, None)
    }

    /// Infers the best-matching `SubagentRole` from a free-form task description or intent string
    /// using capability-vector token overlap and an optional mutating hint.
    pub fn infer_from_intent(query: &str, mutating_hint: Option<bool>) -> Self {
        let lower = query.to_ascii_lowercase();
        let tokens: HashSet<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|w| w.len() >= 2)
            .collect();
        let sub_tokens: HashSet<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 2)
            .collect();

        let mut best_role: Option<SubagentRole> = None;
        let mut best_score: usize = 0;

        for role in Self::all_roles() {
            let meta = role.meta();
            if let Some(expect_mutating) = mutating_hint {
                if meta.is_mutating != expect_mutating {
                    continue;
                }
            }

            let mut score = 0usize;
            if tokens.contains(meta.slug) || sub_tokens.contains(meta.slug) {
                score += 6;
            }
            for alias in meta.aliases {
                if tokens.contains(alias) || sub_tokens.contains(alias) {
                    score += 5;
                }
            }
            for kw in meta.capability_keywords {
                if tokens.contains(kw) || sub_tokens.contains(kw) {
                    score += 3;
                }
            }

            if score > best_score {
                best_score = score;
                best_role = Some(*role);
            }
        }

        if let Some(role) = best_role {
            return role;
        }

        match mutating_hint {
            Some(false) => Self::Scout,
            _ => Self::Coder,
        }
    }

    /// Returns the compiled-in OMC-style `.md` specification for this role
    pub fn default_markdown_spec(&self) -> &'static str {
        self.meta().spec
    }

    /// Loads the role prompt from workspace override (`.minicode/agents/<role>.md` or
    /// `minikit_docs/agents/<role>.md`) if present, otherwise uses the embedded OMC-style `.md` spec.
    pub fn load_role_prompt(&self, workspace_root: Option<&std::path::Path>) -> String {
        let role_slug = self.as_str();
        let kebab_slug = role_slug.replace('_', "-");
        let raw_md = if let Some(root) = workspace_root {
            let mut candidates = Vec::with_capacity(4);
            for dir in [
                root.join(".minicode").join("agents"),
                root.join("minikit_docs").join("agents"),
            ] {
                candidates.push(dir.join(format!("{}.md", role_slug)));
                if kebab_slug != role_slug {
                    candidates.push(dir.join(format!("{}.md", kebab_slug)));
                }
            }
            candidates
                .iter()
                .find_map(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_else(|| self.default_markdown_spec().to_string())
        } else {
            self.default_markdown_spec().to_string()
        };

        strip_yaml_frontmatter(&raw_md).trim().to_string()
    }

    /// Default tool whitelist for this role, combining shared read-only search/AST/codegraph/vault
    /// capabilities with role-scoped domain tools.
    pub fn default_tool_whitelist(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        // Shared read-only exploration, AST search, CodeGraph, MiniVault & coordination tools
        for common in &[
            "read_file",
            "batch_read",
            "look_at",
            "view_outline",
            "grep_search",
            "file_search",
            "locate_symbol",
            "hybrid_search",
            "semantic_search",
            "search_symbols_semantic",
            "ast_query",
            "ast_extract_symbol",
            "ast_diff",
            "ast_grep_search",
            "code_explore",
            "diff_impact",
            "code_explain",
            "code_trace",
            "code_impact",
            "kit_skill_show",
            "kit_info",
            "vault_search",
            "vault_show",
            "vault_bundle_list",
            "wiki_read",
            "wiki_search",
            "okf_attest",
            "dox_tree",
            "scratchpad_read",
            "scratchpad_write",
            "scratchpad_list",
            "send_message",
            "send_worker_message",
            "read_worker_messages",
        ] {
            set.insert((*common).to_string());
        }

        let role_tools: &[&str] = match self {
            SubagentRole::Scout => &[
                "webfetch",
                "fetch",
                "fetch_or_browse",
                "search_web",
                "crawl_documentation",
                "crawl_sitemap",
                "search_crawled_docs",
                "kit_stack_list",
                "block_search",
                "block_get",
                "block_palettes",
            ],
            SubagentRole::Architect => &[
                "get_repo_map",
                "get_blast_radius",
                "find_references",
                "check_architecture_drift",
                "analyze_complexity",
                "find_dead_code",
                "kit_stack_list",
                "kit_stack_diff",
            ],
            SubagentRole::Planner => &[
                "get_repo_map",
                "get_blast_radius",
                "find_references",
                "kit_stack_list",
                "power_plan",
                "power_brainstorm",
                "power_diagnose",
                "create_plan",
                "read_plan",
                "update_progress",
            ],
            SubagentRole::Designer => &[
                "write_file",
                "patch_file",
                "replace_in_files",
                "block_palettes",
                "block_gradients",
                "block_search",
                "block_get",
                "block_insert",
                "block_scaffold",
                "browser_snapshot",
                "browser_inspect_dom",
                "browser_screenshot",
                "webfetch",
                "fetch",
                "fetch_or_browse",
                "search_web",
            ],
            SubagentRole::Frontend => &[
                "write_file",
                "patch_file",
                "replace_in_files",
                "ast_grep_rewrite",
                "exec_cmd",
                "block_search",
                "block_get",
                "block_insert",
                "block_palettes",
                "block_gradients",
                "block_scaffold",
                "browser_navigate",
                "browser_snapshot",
                "browser_inspect_dom",
                "browser_console",
                "browser_eval",
                "browser_screenshot",
                "browser_batch",
                "kit_add",
                "power_verify",
            ],
            SubagentRole::Backend => &[
                "write_file",
                "patch_file",
                "replace_in_files",
                "ast_grep_rewrite",
                "exec_cmd",
                "find_references",
                "get_blast_radius",
                "kit_add",
                "kit_stack_diff",
                "webfetch",
                "fetch",
                "fetch_or_browse",
                "search_web",
                "power_verify",
            ],
            SubagentRole::Coder => &[
                "write_file",
                "patch_file",
                "replace_in_files",
                "ast_grep_rewrite",
                "webfetch",
                "fetch",
                "fetch_or_browse",
                "search_web",
                "exec_cmd",
                "kit_add",
                "kit_stack_diff",
                "kit_stack_list",
                "block_search",
                "block_get",
                "block_insert",
                "block_palettes",
                "browser_inspect",
                "browser_screenshot",
                "browser_console",
                "browser_evaluate",
                "power_verify",
                "power_review",
                "wiki_write",
                "okf_bundle",
            ],
            SubagentRole::Vision => &[
                "browser_navigate",
                "browser_snapshot",
                "browser_screenshot",
                "browser_badges",
                "browser_inspect_dom",
                "browser_inspect",
                "browser_metrics",
                "browser_emulate",
                "browser_qa_audit",
                "browser_debug_bundle",
                "browser_scroll",
                "browser_scroll_horizontal",
                "block_palettes",
            ],
            SubagentRole::ContentWriter => &[
                "write_file",
                "patch_file",
                "replace_in_files",
                "get_repo_map",
                "webfetch",
                "fetch",
                "fetch_or_browse",
                "search_web",
                "crawl_documentation",
                "wiki_write",
                "okf_bundle",
            ],
            SubagentRole::Debugger => &[
                "write_file",
                "patch_file",
                "replace_in_files",
                "ast_grep_rewrite",
                "exec_cmd",
                "find_references",
                "get_blast_radius",
                "locate_fault",
                "browser_debug_bundle",
                "browser_debug_logs",
                "browser_console",
                "browser_eval",
                "power_verify",
                "power_diagnose",
            ],
            SubagentRole::Reviewer => &[
                "get_repo_map",
                "find_references",
                "check_architecture_drift",
                "find_dead_code",
                "analyze_complexity",
                "browser_inspect",
                "browser_screenshot",
                "browser_console",
                "browser_evaluate",
                "browser_snapshot",
                "browser_eval",
                "kit_stack_diff",
                "power_review",
                "power_verify",
                "power_diagnose",
            ],
            SubagentRole::Security => &[
                "security_scan",
                "audit_dependencies",
                "scan_unsafe_blocks",
                "verify_ChangeGuard",
                "find_references",
                "get_blast_radius",
                "kit_stack_diff",
                "power_review",
            ],
            SubagentRole::Tester => &[
                "exec_cmd",
                "write_file",
                "patch_file",
                "replace_in_files",
                "ast_grep_rewrite",
                "browser_navigate",
                "browser_snapshot",
                "browser_click",
                "browser_fill",
                "browser_batch",
                "browser_qa_audit",
                "browser_debug_bundle",
                "browser_inspect",
                "browser_screenshot",
                "browser_console",
                "browser_evaluate",
                "kit_stack_diff",
                "power_verify",
            ],
            SubagentRole::Simplifier => &[
                "write_file",
                "patch_file",
                "replace_in_files",
                "ast_grep_rewrite",
                "exec_cmd",
                "find_dead_code",
                "analyze_complexity",
                "check_architecture_drift",
                "find_references",
                "power_verify",
            ],
        };

        for t in role_tools {
            set.insert((*t).to_string());
        }
        set
    }

    /// Default token budget for this role
    pub fn default_token_budget(&self) -> usize {
        self.meta().token_budget
    }

    /// Default max turns for this role
    pub fn default_max_turns(&self) -> usize {
        self.meta().max_turns
    }
}

fn strip_yaml_frontmatter(md: &str) -> &str {
    let trimmed = md.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end_idx) = rest.find("\n---") {
            let after_closing = &rest[end_idx + 4..];
            return after_closing.trim_start_matches(['\r', '\n']);
        }
    }
    md
}

/// Lifecycle state of an active or finished subagent worker
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentState {
    Starting,
    Running,
    WaitingForInput,
    Completed,
    Failed(String),
    Terminated,
}

#[allow(dead_code)]
impl SubagentState {
    pub fn as_str(&self) -> &str {
        match self {
            SubagentState::Starting => "starting",
            SubagentState::Running => "running",
            SubagentState::WaitingForInput => "waiting_for_input",
            SubagentState::Completed => "completed",
            SubagentState::Failed(_) => "failed",
            SubagentState::Terminated => "terminated",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            SubagentState::Starting | SubagentState::Running | SubagentState::WaitingForInput
        )
    }
}

/// Structured todo checklist item assigned by the `minicode` Orchestrator to a `minipub` worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniPubTodoItem {
    pub id: usize,
    pub task: String,
    #[serde(default)]
    pub completed: bool,
}

impl MiniPubTodoItem {
    pub fn from_strings(items: &[String]) -> Vec<Self> {
        items
            .iter()
            .enumerate()
            .filter_map(|(idx, s)| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(Self {
                        id: idx + 1,
                        task: trimmed.to_string(),
                        completed: false,
                    })
                }
            })
            .collect()
    }
}

/// Configuration settings for instantiating a `minipub` (subagent) worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    pub role: SubagentRole,
    #[serde(default)]
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub fallback_1: Option<crate::config::ModelSlot>,
    #[serde(default)]
    pub fallback_2: Option<crate::config::ModelSlot>,
    #[serde(default)]
    pub use_orchestrator_model: bool,
    pub token_budget: usize,
    pub max_turns: usize,
    pub tool_whitelist: HashSet<String>,
    pub system_prompt_override: Option<String>,
    /// Activity-heartbeat stall timeout in seconds (resets on every LLM token or tool output)
    #[serde(default = "default_inactivity_timeout_secs")]
    pub inactivity_timeout_secs: u64,
    /// Structured todo checklist assigned by the `minicode` Orchestrator
    #[serde(default)]
    pub todos: Vec<MiniPubTodoItem>,
    /// Optional relative file/directory boundaries restricting where a `coder`/`tester` `minipub` may write
    #[serde(default)]
    pub file_boundaries: Vec<String>,
    /// Optional semantic intent category (e.g. `ultrabrain`, `visual-engineering`, `quick`, `explore`, `librarian`, `security-audit`, `test-verification`)
    #[serde(default)]
    pub category: Option<String>,
    /// Optional list of domain skill names from `VaultStore` to inject into this `MiniPub`'s system prompt
    #[serde(default)]
    pub load_skills: Vec<String>,
    /// Optional named result alias (`as`) so subsequent subagents can reference this worker's output via `$RESULT[<alias>]`
    #[serde(default)]
    pub result_alias: Option<String>,
    /// Optional completion condition (`until`) evaluated in an iterative verification loop (`<subtask loop="break"/>`)
    #[serde(default)]
    pub until_condition: Option<String>,
    /// Maximum number of conditional verification loop iterations when `until_condition` is active (default: 3)
    #[serde(default = "default_subagent_loop_max")]
    pub loop_max: usize,
    /// Optional custom return directive appended to the subagent report for the Orchestrator
    #[serde(default)]
    pub return_prompt: Option<String>,
}

fn default_subagent_loop_max() -> usize {
    3
}

fn default_inactivity_timeout_secs() -> u64 {
    crate::constants::DEFAULT_MINIPUB_INACTIVITY_TIMEOUT_SECS
}

impl SubagentConfig {
    pub fn for_role(role: SubagentRole) -> Self {
        let tool_whitelist = role.default_tool_whitelist();
        let token_budget = role.default_token_budget();
        let max_turns = role.default_max_turns();

        Self {
            role,
            provider: None,
            model: None,
            fallback_1: None,
            fallback_2: None,
            use_orchestrator_model: false,
            token_budget,
            max_turns,
            tool_whitelist,
            system_prompt_override: None,
            inactivity_timeout_secs: crate::constants::DEFAULT_MINIPUB_INACTIVITY_TIMEOUT_SECS,
            todos: Vec::new(),
            file_boundaries: Vec::new(),
            category: None,
            load_skills: Vec::new(),
            result_alias: None,
            until_condition: None,
            loop_max: default_subagent_loop_max(),
            return_prompt: None,
        }
    }

    /// Creates a `SubagentConfig` (`MiniPubConfig`) incorporating project/global `[agent.minipub]` settings.
    pub fn for_role_with_minipub_config(
        role: SubagentRole,
        mp: &crate::config::MiniPubConfig,
    ) -> Self {
        let mut cfg = Self::for_role(role);
        if mp.default_token_budget > 0 {
            cfg.token_budget = mp.default_token_budget;
        }
        if mp.default_max_turns > 0 {
            cfg.max_turns = mp.default_max_turns;
        }
        if mp.inactivity_timeout_secs > 0 {
            cfg.inactivity_timeout_secs = mp.inactivity_timeout_secs;
        }
        for extra in &mp.extra_allowed_tools {
            let trimmed = extra.trim();
            if !trimmed.is_empty() {
                cfg.tool_whitelist.insert(trimmed.to_string());
            }
        }
        cfg
    }

    /// Attaches an Orchestrator-assigned todo checklist to this `SubagentConfig`.
    pub fn with_todos(mut self, todos: Vec<String>) -> Self {
        self.todos = MiniPubTodoItem::from_strings(&todos);
        self
    }

    /// Attaches cooperative file boundaries to this `SubagentConfig`.
    pub fn with_file_boundaries(mut self, file_boundaries: Vec<String>) -> Self {
        self.file_boundaries = file_boundaries
            .into_iter()
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty())
            .collect();
        self
    }

    /// Attaches a semantic intent category (`ultrabrain`, `visual-engineering`, `quick`, `explore`, `librarian`, etc.)
    /// and tunes the `MiniPub`'s turn ceiling and token budget accordingly.
    pub fn with_category(mut self, category: Option<String>) -> Self {
        if let Some(raw_cat) = category {
            let trimmed = raw_cat.trim().to_lowercase();
            if !trimmed.is_empty() {
                let norm = trimmed.replace(['-', ' '], "_");
                match norm.as_str() {
                    "quick" | "unspecified_low" => {
                        self.max_turns = self.max_turns.min(8);
                        self.token_budget = self.token_budget.min(24_000);
                    }
                    "ultrabrain" | "deep" | "unspecified_high" | "deep_reasoning" => {
                        self.max_turns = self.max_turns.max(18);
                        self.token_budget = self.token_budget.max(56_000);
                    }
                    "visual_engineering" | "artistry" | "ui_engineering" => {
                        self.max_turns = self.max_turns.max(16);
                        self.token_budget = self.token_budget.max(48_000);
                    }
                    "explore" | "librarian" => {
                        self.max_turns = self.max_turns.max(12);
                        self.token_budget = self.token_budget.max(32_000);
                    }
                    "security_audit" => {
                        self.max_turns = self.max_turns.max(12);
                        self.token_budget = self.token_budget.max(36_000);
                    }
                    "test_verification" => {
                        self.max_turns = self.max_turns.max(16);
                        self.token_budget = self.token_budget.max(44_000);
                    }
                    _ => {}
                }
                self.category = Some(trimmed);
            }
        }
        self
    }

    /// Attaches explicit `VaultStore` domain skill names to inject into this `MiniPub`.
    pub fn with_load_skills(mut self, load_skills: Vec<String>) -> Self {
        self.load_skills = load_skills
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self
    }

    /// Attaches a named result alias (`as`) so subsequent subagents can reference this worker's output via `$RESULT[<alias>]`.
    pub fn with_result_alias(mut self, alias: Option<String>) -> Self {
        self.result_alias = alias
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty());
        self
    }

    /// Attaches a conditional verification loop target (`until`) and optional maximum loop count (`loop_max`).
    pub fn with_until_loop(mut self, until: Option<String>, loop_max: Option<usize>) -> Self {
        self.until_condition = until
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty());
        if let Some(m) = loop_max {
            self.loop_max = m.clamp(1, 8);
        }
        self
    }

    /// Attaches a custom post-subtask return directive for the Orchestrator.
    pub fn with_return_prompt(mut self, return_prompt: Option<String>) -> Self {
        self.return_prompt = return_prompt
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());
        self
    }

    /// Returns a specialized behavioral directive for the configured semantic intent category.
    pub fn category_directive(&self) -> Option<&'static str> {
        let cat = self.category.as_deref()?;
        let norm = cat.replace(['-', ' '], "_");
        match norm.as_str() {
            "ultrabrain" | "deep" | "deep_reasoning" => Some(
                "CATEGORY DIRECTIVE (`ultrabrain` / Deep Reasoning): Perform exhaustive root-cause and architectural analysis before acting. Trace cross-module invariants, verify edge cases, and never take shallow shortcuts.",
            ),
            "visual_engineering" | "artistry" | "ui_engineering" => Some(
                "CATEGORY DIRECTIVE (`visual-engineering` / UI Taste v2.1): Follow the dealt `<taste_art_direction>` brief. Keep hero `<h1>` <= 2 lines (<= 10 words), maintain symmetric outer margins, never leave lopsided sections (>= 42% empty width) or ragged card row heights, include one live interactive `data-ui=\"true\"` product fragment on Pages (or `tabular-nums` + right-aligned numbers on App Screens), and eliminate all AI-slop tells (`bg-clip-text`, unrequested indigo/purple gradients, `01/02/03` badges, `Lorem ipsum`, stock buzzwords, `transition-all`).",
            ),
            "quick" | "unspecified_low" => Some(
                "CATEGORY DIRECTIVE (`quick` / Fast Surgical Execution): Execute the requested single-file or targeted change with minimal tool calls, zero scope creep, and immediate compiler/syntax verification.",
            ),
            "explore" | "librarian" => Some(
                "CATEGORY DIRECTIVE (`explore` / Fast Codebase & Docs Reconnaissance): Perform rapid, parallel symbol and grep lookups across the codebase and return exact file paths, line ranges, and signatures.",
            ),
            "security_audit" => Some(
                "CATEGORY DIRECTIVE (`security-audit`): Audit for injection vectors, auth bypasses, secret leaks, unsafe deserialization, and boundary violations with concrete file:line evidence.",
            ),
            "test_verification" => Some(
                "CATEGORY DIRECTIVE (`test-verification`): Write and execute deterministic unit/integration tests covering happy paths, boundary conditions, and regressions.",
            ),
            "writing" => Some(
                "CATEGORY DIRECTIVE (`writing`): Produce clear, technically accurate, well-structured documentation grounded in the actual codebase symbols and file paths.",
            ),
            _ => None,
        }
    }
}

/// Live metadata and dynamic runtime controls for a `minipub` (subagent) worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentInfo {
    pub id: String,
    pub role: SubagentRole,
    pub state: SubagentState,
    pub prompt: String,
    pub tokens_used: usize,
    pub turns_executed: usize,
    pub started_at_secs: u64,
    pub finished_at_secs: Option<u64>,
    #[serde(default)]
    pub last_activity_secs: u64,
    #[serde(default)]
    pub current_tool: Option<String>,
    #[serde(default)]
    pub status_message: Option<String>,
    #[serde(default)]
    pub isolate_worktree: bool,
    #[serde(default)]
    pub final_summary: Option<String>,
    #[serde(default)]
    pub todos: Vec<MiniPubTodoItem>,
    #[serde(default)]
    pub file_boundaries: Vec<String>,
    /// Live activity-heartbeat timeout in seconds (dynamically modifiable by Orchestrator)
    #[serde(default = "default_inactivity_timeout_secs")]
    pub inactivity_timeout_secs: u64,
    /// Live token budget ceiling (dynamically modifiable by Orchestrator)
    #[serde(default)]
    pub token_budget: usize,
    /// Live max turns ceiling (dynamically modifiable by Orchestrator)
    #[serde(default)]
    pub max_turns: usize,
    /// Queued in-flight steering instructions injected by the Orchestrator
    #[serde(default)]
    pub pending_steer_messages: Vec<String>,
    /// Live provider override assigned by Orchestrator
    #[serde(default)]
    pub provider_override: Option<String>,
    /// Live model override assigned by Orchestrator
    #[serde(default)]
    pub model_override: Option<String>,
    /// Live 1st fallback override assigned by Orchestrator
    #[serde(default)]
    pub fallback_1_override: Option<crate::config::ModelSlot>,
    /// Live 2nd fallback override assigned by Orchestrator
    #[serde(default)]
    pub fallback_2_override: Option<crate::config::ModelSlot>,
    /// Force this worker to use the Orchestrator's active primary + fallback chain
    #[serde(default)]
    pub use_orchestrator_model: bool,
    /// Human-readable active `provider:model` label currently driving this MiniPub
    #[serde(default)]
    pub active_model_label: Option<String>,
    /// Monotonic counter incremented when the Orchestrator hot-swaps model/provider at runtime
    #[serde(default)]
    pub model_generation: u64,
}

/// Dynamic in-flight modification payload for an active `MiniPub` worker (Orchestrator CRUD Update)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MiniPubUpdate {
    /// New inactivity timeout in seconds (`15..=7200`)
    pub inactivity_timeout_secs: Option<u64>,
    /// New token budget ceiling
    pub token_budget: Option<usize>,
    /// New max turns ceiling
    pub max_turns: Option<usize>,
    /// Replace the worker's todo checklist
    pub todos: Option<Vec<String>>,
    /// Append additional todo items to the worker's checklist
    pub append_todos: Vec<String>,
    /// Mark specific 1-indexed todo item IDs as completed (`[x]`)
    pub complete_todo_ids: Vec<usize>,
    /// Update cooperative file boundaries
    pub file_boundaries: Option<Vec<String>>,
    /// Inject a live steering prompt into the worker's next turn without restarting it
    pub steer_prompt: Option<String>,
    /// Dynamically switch the running MiniPub's primary provider at runtime
    pub provider: Option<String>,
    /// Dynamically switch the running MiniPub's primary model at runtime (supports `"provider:model"` or `"model"`)
    pub model: Option<String>,
    /// Dynamically update the running MiniPub's 1st fallback slot
    pub fallback_1: Option<crate::config::ModelSlot>,
    /// Dynamically update the running MiniPub's 2nd fallback slot
    pub fallback_2: Option<crate::config::ModelSlot>,
    /// Dynamically reset/switch the running MiniPub to use the Orchestrator's active provider & model chain
    pub use_orchestrator_model: Option<bool>,
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[allow(dead_code)]
impl SubagentInfo {
    pub fn new(id: String, role: SubagentRole, prompt: String) -> Self {
        let now = unix_now_secs();

        Self {
            id,
            role,
            state: SubagentState::Running,
            prompt,
            tokens_used: 0,
            turns_executed: 0,
            started_at_secs: now,
            finished_at_secs: None,
            last_activity_secs: now,
            current_tool: None,
            status_message: Some("Initialized".to_string()),
            isolate_worktree: false,
            final_summary: None,
            todos: Vec::new(),
            file_boundaries: Vec::new(),
            inactivity_timeout_secs: crate::constants::DEFAULT_MINIPUB_INACTIVITY_TIMEOUT_SECS,
            token_budget: role.default_token_budget(),
            max_turns: role.default_max_turns(),
            pending_steer_messages: Vec::new(),
            provider_override: None,
            model_override: None,
            fallback_1_override: None,
            fallback_2_override: None,
            use_orchestrator_model: false,
            active_model_label: None,
            model_generation: 0,
        }
    }

    pub fn with_isolation(mut self, isolate_worktree: bool) -> Self {
        self.isolate_worktree = isolate_worktree;
        self
    }

    pub fn with_todos(mut self, todos: Vec<MiniPubTodoItem>) -> Self {
        self.todos = todos;
        self
    }

    pub fn with_file_boundaries(mut self, file_boundaries: Vec<String>) -> Self {
        self.file_boundaries = file_boundaries;
        self
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.finished_at_secs
            .unwrap_or_else(unix_now_secs)
            .saturating_sub(self.started_at_secs)
    }

    pub fn last_activity_secs_ago(&self) -> u64 {
        unix_now_secs().saturating_sub(self.last_activity_secs)
    }

    pub fn isolation_badge(&self) -> &'static str {
        if self.isolate_worktree {
            "Worktree"
        } else if !self.file_boundaries.is_empty() {
            "File-Lease"
        } else {
            "Shared"
        }
    }
}

/// Execution outcome returned by a finished `minipub` (subagent) task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentResult {
    pub id: String,
    pub task_id: String,
    pub role: SubagentRole,
    pub success: bool,
    pub final_summary: String,
    pub tokens_used: usize,
    pub turns_executed: usize,
    pub files_inspected: Vec<String>,
    pub files_modified: Vec<String>,
    pub worktree_branch: Option<String>,
}

/// Specification for dispatching a `minipub` worker in a concurrent swarm fan-out
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTaskSpec {
    pub role: SubagentRole,
    pub prompt: String,
    #[serde(default)]
    pub isolate_worktree: Option<bool>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub todos: Vec<String>,
    #[serde(default)]
    pub file_boundaries: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub load_skills: Vec<String>,
    #[serde(default)]
    pub result_alias: Option<String>,
    #[serde(default)]
    pub until_condition: Option<String>,
    #[serde(default)]
    pub loop_max: Option<usize>,
    #[serde(default)]
    pub return_prompt: Option<String>,
}

/// First-class `MiniPub` type aliases for `minicode`'s lightweight in-process subagent system
#[allow(dead_code)]
pub type MiniPubRole = SubagentRole;
#[allow(dead_code)]
pub type MiniPubState = SubagentState;
#[allow(dead_code)]
pub type MiniPubConfig = SubagentConfig;
#[allow(dead_code)]
pub type MiniPubInfo = SubagentInfo;
#[allow(dead_code)]
pub type MiniPubResult = SubagentResult;
#[allow(dead_code)]
pub type MiniPubTaskSpec = SubagentTaskSpec;
