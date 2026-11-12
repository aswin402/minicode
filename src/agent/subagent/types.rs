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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentRole {
    Scout,
    Architect,
    Planner,
    Designer,
    Frontend,
    Backend,
    #[default]
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
    is_mutating: bool,
    default_workspace_mode: WorkspaceMode,
    tool_categories: &'static [crate::tools::category::ToolCategory],
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
        use crate::tools::category::ToolCategory;
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
                is_mutating: false,
                default_workspace_mode: WorkspaceMode::Shared,
                tool_categories: &[
                    ToolCategory::Web,
                    ToolCategory::MiniKit,
                    ToolCategory::Blocks,
                    ToolCategory::Dev,
                ],
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
                is_mutating: false,
                default_workspace_mode: WorkspaceMode::Shared,
                tool_categories: &[ToolCategory::Codegraph, ToolCategory::MiniKit],
                spec: include_str!("agents/architect.md"),
            },
            SubagentRole::Planner => RoleMeta {
                slug: "planner",
                badge: "Planner",
                aliases: &["plan", "task_planner", "critic", "prometheus", "momus"],
                is_mutating: false,
                default_workspace_mode: WorkspaceMode::Shared,
                tool_categories: &[
                    ToolCategory::Codegraph,
                    ToolCategory::MiniPower,
                    ToolCategory::Memory,
                ],
                spec: include_str!("agents/planner.md"),
            },
            SubagentRole::Designer => RoleMeta {
                slug: "designer",
                badge: "Designer",
                aliases: &["ui_designer", "ux_designer", "ui_ux", "design", "artistry"],
                is_mutating: true,
                default_workspace_mode: WorkspaceMode::Worktree,
                tool_categories: &[ToolCategory::Files, ToolCategory::Blocks, ToolCategory::Web],
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
                is_mutating: true,
                default_workspace_mode: WorkspaceMode::Worktree,
                tool_categories: &[
                    ToolCategory::Files,
                    ToolCategory::Exec,
                    ToolCategory::Dev,
                    ToolCategory::Blocks,
                    ToolCategory::Web,
                    ToolCategory::MiniKit,
                    ToolCategory::MiniPower,
                ],
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
                is_mutating: true,
                default_workspace_mode: WorkspaceMode::Worktree,
                tool_categories: &[
                    ToolCategory::Files,
                    ToolCategory::Exec,
                    ToolCategory::Dev,
                    ToolCategory::Web,
                    ToolCategory::MiniKit,
                    ToolCategory::MiniPower,
                ],
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
                is_mutating: true,
                default_workspace_mode: WorkspaceMode::Worktree,
                tool_categories: &[
                    ToolCategory::Files,
                    ToolCategory::Exec,
                    ToolCategory::Dev,
                    ToolCategory::Web,
                    ToolCategory::Blocks,
                    ToolCategory::MiniKit,
                    ToolCategory::MiniPower,
                    ToolCategory::Memory,
                ],
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
                is_mutating: false,
                default_workspace_mode: WorkspaceMode::Shared,
                tool_categories: &[ToolCategory::Web, ToolCategory::Blocks, ToolCategory::Dev],
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
                is_mutating: true,
                default_workspace_mode: WorkspaceMode::Worktree,
                tool_categories: &[ToolCategory::Files, ToolCategory::Web, ToolCategory::Memory],
                spec: include_str!("agents/content-writer.md"),
            },
            SubagentRole::Debugger => RoleMeta {
                slug: "debugger",
                badge: "Debugger",
                aliases: &["debug", "tracer", "bug_fixer", "root_cause"],
                is_mutating: true,
                default_workspace_mode: WorkspaceMode::Worktree,
                tool_categories: &[
                    ToolCategory::Files,
                    ToolCategory::Exec,
                    ToolCategory::Dev,
                    ToolCategory::Web,
                    ToolCategory::MiniPower,
                ],
                spec: include_str!("agents/debugger.md"),
            },
            SubagentRole::Reviewer => RoleMeta {
                slug: "reviewer",
                badge: "Reviewer",
                aliases: &["code_reviewer", "code_review", "review"],
                is_mutating: false,
                default_workspace_mode: WorkspaceMode::Shared,
                tool_categories: &[
                    ToolCategory::Exec,
                    ToolCategory::Dev,
                    ToolCategory::Web,
                    ToolCategory::Codegraph,
                    ToolCategory::MiniPower,
                ],
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
                is_mutating: false,
                default_workspace_mode: WorkspaceMode::Shared,
                tool_categories: &[
                    ToolCategory::Exec,
                    ToolCategory::Codegraph,
                    ToolCategory::MiniPower,
                ],
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
                is_mutating: true,
                default_workspace_mode: WorkspaceMode::Shared,
                tool_categories: &[
                    ToolCategory::Files,
                    ToolCategory::Exec,
                    ToolCategory::Dev,
                    ToolCategory::Web,
                    ToolCategory::MiniPower,
                ],
                spec: include_str!("agents/tester.md"),
            },
            SubagentRole::Simplifier => RoleMeta {
                slug: "simplifier",
                badge: "Simplifier",
                aliases: &["code_simplifier", "refactor", "refactorer", "cleaner"],
                is_mutating: true,
                default_workspace_mode: WorkspaceMode::Worktree,
                tool_categories: &[
                    ToolCategory::Files,
                    ToolCategory::Exec,
                    ToolCategory::Codegraph,
                    ToolCategory::MiniPower,
                ],
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
        self.meta().default_workspace_mode
    }

    /// Determines whether a task for this role should run inside an isolated Git Worktree
    /// from the explicit flag, cooperative file boundaries, and the role's default workspace mode
    /// (without re-inferring the role from prompt keywords).
    pub fn should_isolate_worktree(
        &self,
        _prompt: &str,
        file_boundaries: &[String],
        explicit_isolate: Option<bool>,
    ) -> bool {
        if let Some(flag) = explicit_isolate {
            return flag;
        }
        if !file_boundaries.is_empty() {
            return false;
        }
        self.default_workspace_mode() == WorkspaceMode::Worktree
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

    /// Resolves a `SubagentRole` strictly by canonical `slug` or registered `aliases` without
    /// falling back to free-form intent inference.
    pub fn from_slug_or_alias(s: &str) -> Option<Self> {
        let normalized = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        for role in Self::all_roles() {
            let meta = role.meta();
            if meta.slug == normalized || meta.aliases.contains(&normalized.as_str()) {
                return Some(*role);
            }
        }
        None
    }

    /// Declarative role & semantic category resolver supporting OMC, oh-my-opencode, and intent scoring.
    pub fn from_str_loose(s: &str) -> Self {
        Self::from_slug_or_alias(s).unwrap_or_else(|| Self::infer_from_intent(s, None))
    }

    /// Infers the best-matching `SubagentRole` and its quantized BM25 capability score from a
    /// free-form task description using a runtime BM25 index over embedded `.md` role specs.
    pub fn infer_from_intent_scored(query: &str, mutating_hint: Option<bool>) -> (Self, usize) {
        RoleBm25Index::global().infer(query, mutating_hint)
    }

    /// Infers the best-matching `SubagentRole` from a free-form task description or intent string
    /// using capability-vector token overlap and an optional mutating hint.
    pub fn infer_from_intent(query: &str, mutating_hint: Option<bool>) -> Self {
        Self::infer_from_intent_scored(query, mutating_hint).0
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

    /// Default tool whitelist for this role, dynamically assembled from `ToolCategory` schemas
    /// and `ToolSafetyLevel` classification rather than hardcoded tool lists.
    pub fn default_tool_whitelist(&self) -> HashSet<String> {
        use crate::tools::category::ToolCategory;
        let meta = self.meta();
        let mut set = HashSet::new();

        // 1. Baseline read-only exploration, AST, CodeGraph, MiniVault & Memory schemas
        for base_cat in [
            ToolCategory::Files,
            ToolCategory::Search,
            ToolCategory::Codegraph,
            ToolCategory::Vault,
            ToolCategory::Memory,
        ] {
            for schema in base_cat.get_schemas() {
                if crate::tools::concurrency::classify_tool(&schema.name).is_read_only() {
                    set.insert(schema.name);
                }
            }
        }

        // 2. Role-declared `ToolCategory` bundles from `RoleMeta`
        for cat in meta.tool_categories {
            for schema in cat.get_schemas() {
                let allow_schema = meta.is_mutating
                    || matches!(
                        cat,
                        ToolCategory::Web | ToolCategory::Exec | ToolCategory::Dev
                    )
                    || crate::tools::concurrency::classify_tool(&schema.name).is_read_only();
                if allow_schema {
                    set.insert(schema.name);
                }
            }
        }

        // 3. Inter-worker coordination schemas from `ToolCategory::Agent` (excluding recursive subagent spawning)
        for schema in ToolCategory::Agent.get_schemas() {
            if schema.name.starts_with("scratchpad_")
                || schema.name.ends_with("_worker_message")
                || schema.name.ends_with("_worker_messages")
            {
                set.insert(schema.name);
            }
        }

        // 4. Canonical registered tool aliases whose underlying target is permitted for this role
        for &(alias, canonical) in crate::tools::meta::TOOL_ALIASES {
            if set.contains(canonical) {
                set.insert(alias.to_string());
            }
        }

        set
    }

    /// Default max turns for this role, computed algorithmically from its structural
    /// capability surface (`is_mutating`, `tool_categories`) and `DEFAULT_MINIPUB_MAX_TURNS`.
    pub fn default_max_turns(&self) -> usize {
        let m = self.meta();
        let base = crate::constants::DEFAULT_MINIPUB_MAX_TURNS;
        let surface_bonus = m.tool_categories.len() / 6;
        if m.is_mutating {
            base + surface_bonus
        } else {
            ((base * 3) / 4) + surface_bonus
        }
    }

    /// Default token budget for this role, computed algorithmically from `default_max_turns`
    /// and `DEFAULT_MINIPUB_TOKEN_BUDGET`.
    pub fn default_token_budget(&self) -> usize {
        let base_turns = crate::constants::DEFAULT_MINIPUB_MAX_TURNS.max(1);
        let base_budget = crate::constants::DEFAULT_MINIPUB_TOKEN_BUDGET;
        let m = self.meta();
        if m.is_mutating && m.tool_categories.len() < 6 {
            base_budget
        } else {
            let scaled = (self.default_max_turns().saturating_mul(base_budget)) / base_turns;
            // Round to nearest 1,000 tokens for clean telemetry
            ((scaled + 500) / 1_000) * 1_000
        }
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

struct RoleDoc {
    role: SubagentRole,
    is_mutating: bool,
    tf: std::collections::HashMap<String, f64>,
    len: f64,
}

/// Runtime BM25 index built directly from embedded `agents/*.md` role specifications and metadata.
struct RoleBm25Index {
    docs: Vec<RoleDoc>,
    df: std::collections::HashMap<String, usize>,
    avgdl: f64,
}

impl RoleBm25Index {
    fn global() -> &'static Self {
        static INDEX: std::sync::OnceLock<RoleBm25Index> = std::sync::OnceLock::new();
        INDEX.get_or_init(Self::build)
    }

    fn tokenize_stems(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 2)
            .map(|w| w.to_ascii_lowercase())
            .filter(|w| {
                !matches!(
                    w.as_str(),
                    "the"
                        | "and"
                        | "for"
                        | "with"
                        | "you"
                        | "are"
                        | "your"
                        | "this"
                        | "that"
                        | "from"
                        | "into"
                        | "via"
                        | "when"
                        | "not"
                        | "all"
                        | "any"
                        | "use"
                )
            })
            .map(|w| crate::vault::models::VaultSkill::normalize_stem(&w).to_string())
            .filter(|s| s.len() >= 2)
            .collect()
    }

    fn add_weighted(tf: &mut std::collections::HashMap<String, f64>, text: &str, weight: f64) {
        for stem in Self::tokenize_stems(text) {
            *tf.entry(stem).or_insert(0.0) += weight;
        }
    }

    fn build() -> Self {
        let roles = SubagentRole::all_roles();
        let mut docs = Vec::with_capacity(roles.len());
        let mut df: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut total_len = 0.0;

        for &role in roles {
            let meta = role.meta();
            let mut tf = std::collections::HashMap::new();
            Self::add_weighted(&mut tf, meta.slug, 8.0);
            Self::add_weighted(&mut tf, meta.badge, 5.0);
            for alias in meta.aliases {
                Self::add_weighted(&mut tf, alias, 6.0);
            }
            for line in meta.spec.lines() {
                let trimmed = line.trim();
                let lower = trimmed.to_ascii_lowercase();
                if lower.contains("not responsible for") || lower.starts_with("never ") {
                    continue;
                }
                let weight = if lower.starts_with("description:") {
                    5.0
                } else {
                    1.5
                };
                Self::add_weighted(&mut tf, trimmed, weight);
            }
            let len: f64 = tf.values().sum();
            for term in tf.keys() {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
            total_len += len;
            docs.push(RoleDoc {
                role,
                is_mutating: meta.is_mutating,
                tf,
                len,
            });
        }

        let avgdl = (total_len / (docs.len().max(1) as f64)).max(1.0);
        Self { docs, df, avgdl }
    }

    fn infer(&self, query: &str, mutating_hint: Option<bool>) -> (SubagentRole, usize) {
        let query_stems: HashSet<String> = Self::tokenize_stems(query).into_iter().collect();
        let n_docs = self.docs.len() as f64;
        let k1 = crate::constants::BM25_K1;
        let b = crate::constants::BM25_B;

        let mut best_role: Option<SubagentRole> = None;
        let mut best_score = 0.0_f64;

        for doc in &self.docs {
            if let Some(expect_mutating) = mutating_hint {
                if doc.is_mutating != expect_mutating {
                    continue;
                }
            }
            let mut score = 0.0;
            for stem in &query_stems {
                let Some(&tf) = doc.tf.get(stem) else {
                    continue;
                };
                let doc_freq = *self.df.get(stem).unwrap_or(&1) as f64;
                let idf = (1.0 + (n_docs - doc_freq + 0.5) / (doc_freq + 0.5)).ln();
                let denom = tf + k1 * (1.0 - b + b * (doc.len / self.avgdl));
                score += idf * ((tf * (k1 + 1.0)) / denom.max(1e-6));
            }
            if score > best_score {
                best_score = score;
                best_role = Some(doc.role);
            }
        }

        if let Some(role) = best_role.filter(|_| best_score >= 1.2) {
            return (role, (best_score * 10.0).round() as usize);
        }

        let fallback = match mutating_hint {
            Some(false) => SubagentRole::Scout,
            _ => SubagentRole::Coder,
        };
        (fallback, 0)
    }
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

/// Structured 4-status completion contract returned by a `MiniPub` worker (`DONE`, `DONE_WITH_CONCERNS`, `NEEDS_CONTEXT`, `BLOCKED`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerReportStatus {
    #[default]
    Done,
    DoneWithConcerns,
    NeedsContext,
    Blocked,
}

impl WorkerReportStatus {
    const TOKEN_TABLE: &'static [(&'static str, WorkerReportStatus)] = &[
        ("done_with_concerns", WorkerReportStatus::DoneWithConcerns),
        ("done with concerns", WorkerReportStatus::DoneWithConcerns),
        ("needs_context", WorkerReportStatus::NeedsContext),
        ("needs context", WorkerReportStatus::NeedsContext),
        ("blocked", WorkerReportStatus::Blocked),
        ("done", WorkerReportStatus::Done),
    ];

    pub const fn as_str(&self) -> &'static str {
        match self {
            WorkerReportStatus::Done => "DONE",
            WorkerReportStatus::DoneWithConcerns => "DONE_WITH_CONCERNS",
            WorkerReportStatus::NeedsContext => "NEEDS_CONTEXT",
            WorkerReportStatus::Blocked => "BLOCKED",
        }
    }

    pub const fn badge(&self) -> &'static str {
        match self {
            WorkerReportStatus::Done => "✔ DONE",
            WorkerReportStatus::DoneWithConcerns => "⚠ DONE_WITH_CONCERNS",
            WorkerReportStatus::NeedsContext => "❓ NEEDS_CONTEXT",
            WorkerReportStatus::Blocked => "⛔ BLOCKED",
        }
    }

    /// Single-pass table-driven parser over `<status>...</status>` tags and leading `STATUS:` lines.
    /// Falls back to `Blocked` on failure and `Done` on success when no explicit status is found.
    pub fn parse_from_summary(summary: &str, success: bool) -> Self {
        Self::parse_explicit_within(summary, 12).unwrap_or(if success {
            WorkerReportStatus::Done
        } else {
            WorkerReportStatus::Blocked
        })
    }

    /// Returns the explicitly reported status anywhere in `report` (`<status>` tag or a
    /// `STATUS:` line), or `None` when the worker never stated one. Callers must not treat
    /// `None` as success.
    pub fn parse_explicit(report: &str) -> Option<Self> {
        Self::parse_explicit_within(report, usize::MAX)
    }

    fn parse_explicit_within(summary: &str, max_lines: usize) -> Option<Self> {
        let lower = summary.to_ascii_lowercase();
        if let Some(s_idx) = lower.find("<status>") {
            let after = &lower[s_idx + 8..];
            if let Some(e_idx) = after.find("</status>") {
                let token = after[..e_idx].trim();
                for &(pat, status) in Self::TOKEN_TABLE {
                    if token == pat || token.contains(pat) {
                        return Some(status);
                    }
                }
            }
        }

        for line in lower.lines().take(max_lines) {
            let cleaned = line
                .trim()
                .trim_start_matches(['#', '*', '-', '•', '>', '`', ' ', '[']);
            if let Some(rest) = cleaned
                .strip_prefix("status:")
                .or_else(|| cleaned.strip_prefix("report_status:"))
                .or_else(|| cleaned.strip_prefix("worker_status:"))
            {
                let val = rest.trim().trim_matches(['*', '`', '[', ']', ' ']);
                for &(pat, status) in Self::TOKEN_TABLE {
                    if val.starts_with(pat) {
                        return Some(status);
                    }
                }
            }
        }
        None
    }

    /// Returns an actionable Orchestrator continuation hint when a worker finishes with non-trivial status.
    pub fn orchestrator_action_hint(&self, subagent_id: &str) -> Option<String> {
        match self {
            WorkerReportStatus::Done => None,
            WorkerReportStatus::DoneWithConcerns => Some(format!(
                "Worker `{}` reported `DONE_WITH_CONCERNS`. Review the noted concerns before merging/advancing, or resume its warm session via `delegate_task(resume_id=\"{}\", task=\"...\")`.",
                subagent_id, subagent_id
            )),
            WorkerReportStatus::NeedsContext => Some(format!(
                "Worker `{}` reported `NEEDS_CONTEXT`. Supply the missing context and resume its warm session via `delegate_task(resume_id=\"{}\", task=\"...\")` or `manage_subagents(action=\"resume\", subagent_id=\"{}\", prompt=\"...\")`.",
                subagent_id, subagent_id, subagent_id
            )),
            WorkerReportStatus::Blocked => Some(format!(
                "Worker `{}` reported `BLOCKED`. Resolve the upstream blocker or re-scope the task before resuming `{}`.",
                subagent_id, subagent_id
            )),
        }
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
    /// True when the Orchestrator explicitly specified `max_turns` for this worker
    #[serde(default)]
    pub explicit_max_turns: bool,
    /// True when the Orchestrator explicitly specified `token_budget` for this worker
    #[serde(default)]
    pub explicit_token_budget: bool,
    /// True once `apply_dynamic_scope_scaling` has been applied to this config
    #[serde(default)]
    pub scope_scaled: bool,
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
            explicit_max_turns: false,
            explicit_token_budget: false,
            scope_scaled: false,
        }
    }

    /// Creates a `SubagentConfig` (`MiniPubConfig`) incorporating project/global `[agent.minipub]` settings.
    pub fn for_role_with_minipub_config(
        role: SubagentRole,
        mp: &crate::config::MiniPubConfig,
    ) -> Self {
        let mut cfg = Self::for_role(role);
        let default_role_budget = role.default_token_budget();
        let default_role_turns = role.default_max_turns();
        if mp.default_token_budget > 0
            && mp.default_token_budget != crate::constants::DEFAULT_MINIPUB_TOKEN_BUDGET
        {
            // Scale role budget proportionally to user's configured default_token_budget
            cfg.token_budget = (default_role_budget.saturating_mul(mp.default_token_budget))
                / crate::constants::DEFAULT_MINIPUB_TOKEN_BUDGET.max(1);
        }
        if mp.default_max_turns > 0
            && mp.default_max_turns != crate::constants::DEFAULT_MINIPUB_MAX_TURNS
        {
            cfg.max_turns = (default_role_turns.saturating_mul(mp.default_max_turns))
                / crate::constants::DEFAULT_MINIPUB_MAX_TURNS.max(1);
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

    /// Sets an explicit Orchestrator `max_turns` override and marks it as pinned.
    pub fn with_explicit_max_turns(mut self, max_turns: Option<usize>) -> Self {
        if let Some(mt) = max_turns {
            self.max_turns = mt.max(1);
            self.explicit_max_turns = true;
        }
        self
    }

    /// Sets an explicit Orchestrator `token_budget` override and marks it as pinned.
    pub fn with_explicit_token_budget(mut self, token_budget: Option<usize>) -> Self {
        if let Some(tb) = token_budget {
            self.token_budget = tb.max(4_096);
            self.explicit_token_budget = true;
        }
        self
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
    /// and scales the `MiniPub`'s turn ceiling and token budget proportionally to its baseline.
    pub fn with_category(mut self, category: Option<String>) -> Self {
        if let Some(raw_cat) = category {
            let trimmed = raw_cat.trim().to_lowercase();
            if !trimmed.is_empty() {
                let norm = trimmed.replace(['-', ' '], "_");
                let base_turns = crate::constants::DEFAULT_MINIPUB_MAX_TURNS.max(1);
                let base_budget = crate::constants::DEFAULT_MINIPUB_TOKEN_BUDGET;
                let (numer, denom, is_cap) = match norm.as_str() {
                    "quick" | "unspecified_low" => (4, 7, true),
                    "ultrabrain" | "deep" | "unspecified_high" | "deep_reasoning" => (7, 5, false),
                    "visual_engineering" | "artistry" | "ui_engineering" | "test_verification" => {
                        (6, 5, false)
                    }
                    "explore" | "librarian" | "security_audit" => (1, 1, false),
                    _ => (0, 1, false),
                };
                if numer > 0 {
                    let tokens_per_turn = ((base_budget / base_turns) / 1_000) * 1_000;
                    let target_turns = (base_turns.saturating_mul(numer)) / denom;
                    let target_budget = target_turns.saturating_mul(tokens_per_turn);
                    if is_cap {
                        if !self.explicit_max_turns {
                            self.max_turns = self.max_turns.min(target_turns.max(6));
                        }
                        if !self.explicit_token_budget {
                            self.token_budget = self.token_budget.min(target_budget.max(16_000));
                        }
                    } else {
                        if !self.explicit_max_turns {
                            self.max_turns = self.max_turns.max(target_turns);
                        }
                        if !self.explicit_token_budget {
                            self.token_budget = self.token_budget.max(target_budget);
                        }
                    }
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

    /// Algorithmically scales `max_turns` and `token_budget` based on the task's structural
    /// scope (`todos`, `file_boundaries`, prompt checklist items, verification loops, and tool surface)
    /// while respecting any explicit Orchestrator overrides.
    pub fn with_dynamic_scope(mut self, prompt: &str) -> Self {
        self.apply_dynamic_scope_scaling(prompt);
        self
    }

    /// Computes dynamic turn and token budgets proportional to the task's structural complexity
    /// across all 14 MiniPub roles.
    pub fn apply_dynamic_scope_scaling(&mut self, prompt: &str) {
        if self.scope_scaled {
            return;
        }
        self.scope_scaled = true;

        let tokens_per_turn = (crate::constants::DEFAULT_MINIPUB_TOKEN_BUDGET
            / crate::constants::DEFAULT_MINIPUB_MAX_TURNS.max(1))
        .max(2_500);

        // If the Orchestrator explicitly pinned one dimension (`max_turns` or `token_budget`),
        // ensure the unpinned dimension scales proportionally so the worker never starves.
        if self.explicit_max_turns && !self.explicit_token_budget {
            self.token_budget = self
                .token_budget
                .max(self.max_turns.saturating_mul(tokens_per_turn))
                .clamp(
                    crate::constants::MIN_MINIPUB_TOKEN_BUDGET,
                    crate::constants::MAX_MINIPUB_TOKEN_BUDGET,
                );
            return;
        }
        if self.explicit_token_budget && !self.explicit_max_turns {
            let implied_turns = self.token_budget / tokens_per_turn.max(1);
            self.max_turns = self.max_turns.max(implied_turns).clamp(
                crate::constants::MIN_MINIPUB_TURNS,
                crate::constants::MAX_MINIPUB_TURNS,
            );
            return;
        }
        if self.explicit_max_turns && self.explicit_token_budget {
            return;
        }

        let is_quick = self
            .category
            .as_deref()
            .is_some_and(|c| matches!(c, "quick" | "unspecified_low" | "unspecified-low"));
        if is_quick {
            return;
        }

        let todo_count = self.todos.len();
        let boundary_count = self.file_boundaries.len();
        let prompt_checklist_items = prompt
            .lines()
            .filter(|line| {
                let t = line.trim();
                t.starts_with("- ")
                    || t.starts_with("* ")
                    || t.starts_with("• ")
                    || (t.bytes().next().is_some_and(|b| b.is_ascii_digit())
                        && (t.contains(". ") || t.contains(") ")))
            })
            .count();

        let effective_items = todo_count.max(prompt_checklist_items.min(10));
        let item_turn_bonus = effective_items.saturating_sub(1).saturating_mul(2);
        let boundary_turn_bonus = boundary_count.saturating_sub(1);
        let verification_turn_bonus = if self.until_condition.is_some() {
            self.loop_max.saturating_mul(2)
        } else {
            0
        };
        let e2e_surface_bonus = if self.tool_whitelist.contains("browser_navigate")
            && self.tool_whitelist.contains("exec_cmd")
        {
            4
        } else if self.tool_whitelist.contains("browser_navigate")
            || self.tool_whitelist.contains("exec_cmd")
        {
            2
        } else {
            0
        };
        let prompt_depth_bonus = (prompt.len() / 800).min(6);

        let total_turn_bonus = item_turn_bonus
            + boundary_turn_bonus
            + verification_turn_bonus
            + e2e_surface_bonus
            + prompt_depth_bonus;

        if total_turn_bonus > 0 {
            let auto_max_turns = (crate::constants::MAX_MINIPUB_TURNS * 3) / 4;
            let auto_max_budget = (crate::constants::MAX_MINIPUB_TOKEN_BUDGET * 5) / 8;
            self.max_turns = (self.max_turns + total_turn_bonus)
                .clamp(crate::constants::MIN_MINIPUB_TURNS, auto_max_turns);
            self.token_budget = self
                .token_budget
                .max(self.max_turns.saturating_mul(tokens_per_turn))
                .clamp(crate::constants::MIN_MINIPUB_TOKEN_BUDGET, auto_max_budget);
        }
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
    /// Structured 4-status completion classification (`DONE`, `DONE_WITH_CONCERNS`, `NEEDS_CONTEXT`, `BLOCKED`)
    #[serde(default)]
    pub report_status: WorkerReportStatus,
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
            report_status: WorkerReportStatus::Done,
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
    pub max_turns: Option<usize>,
    #[serde(default)]
    pub token_budget: Option<usize>,
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

impl SubagentTaskSpec {
    /// Converts this `SubagentTaskSpec` into a fully configured `SubagentConfig` preserving
    /// all dynamic resource overrides, todos, boundaries, skills, and verification loops.
    pub fn to_subagent_config(&self, minipub_cfg: &crate::config::MiniPubConfig) -> SubagentConfig {
        let mut config = SubagentConfig::for_role_with_minipub_config(self.role, minipub_cfg)
            .with_todos(self.todos.clone())
            .with_file_boundaries(self.file_boundaries.clone())
            .with_explicit_max_turns(self.max_turns)
            .with_explicit_token_budget(self.token_budget)
            .with_category(self.category.clone())
            .with_load_skills(self.load_skills.clone())
            .with_result_alias(self.result_alias.clone())
            .with_until_loop(self.until_condition.clone(), self.loop_max)
            .with_return_prompt(self.return_prompt.clone())
            .with_dynamic_scope(&self.prompt);
        if let Some(ref m) = self.model {
            config.model = Some(m.clone());
        }
        if let Some(t_secs) = self.timeout_secs {
            config.inactivity_timeout_secs = t_secs;
        }
        config
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_scope_scaling_scales_turns_and_tokens_across_all_14_minipubs() {
        for &role in SubagentRole::all_roles() {
            let base = SubagentConfig::for_role(role);
            assert!(
                base.max_turns >= 10,
                "Role {:?} baseline max_turns should be >= 10, got {}",
                role,
                base.max_turns
            );
            assert!(
                base.token_budget >= 24_000,
                "Role {:?} baseline token_budget should be >= 24000, got {}",
                role,
                base.token_budget
            );

            let scaled = SubagentConfig::for_role(role)
                .with_todos(vec![
                    "Step 1".to_string(),
                    "Step 2".to_string(),
                    "Step 3".to_string(),
                    "Step 4".to_string(),
                ])
                .with_file_boundaries(vec!["src/a".to_string(), "src/b".to_string()])
                .with_until_loop(Some("cargo test".to_string()), Some(2))
                .with_dynamic_scope("Execute multi-step task and verify.");

            assert!(
                scaled.max_turns > base.max_turns,
                "Expected dynamic max_turns > baseline ({}) for role {:?}, got {}",
                base.max_turns,
                role,
                scaled.max_turns
            );
            assert!(
                scaled.token_budget > base.token_budget,
                "Expected dynamic token_budget > baseline ({}) for role {:?}, got {}",
                base.token_budget,
                role,
                scaled.token_budget
            );

            // Explicit max_turns override automatically scales token_budget proportionally
            let pinned_turns = SubagentConfig::for_role(role)
                .with_explicit_max_turns(Some(32))
                .with_dynamic_scope("Deep multi-phase task");
            assert_eq!(pinned_turns.max_turns, 32);
            assert!(
                pinned_turns.token_budget >= 80_000,
                "Role {:?} with explicit max_turns=32 should scale token_budget >= 80000, got {}",
                role,
                pinned_turns.token_budget
            );
        }

        let quick_coder = SubagentConfig::for_role(SubagentRole::Coder)
            .with_category(Some("quick".to_string()))
            .with_todos(vec!["Fix typo".to_string(), "Verify".to_string()])
            .with_dynamic_scope("Fix single-line typo in README.md");
        assert_eq!(quick_coder.max_turns, 8);
    }

    #[test]
    fn test_role_inference_and_worktree_isolation_decoupled() {
        let (role, score) =
            SubagentRole::infer_from_intent_scored("Implement authentication middleware", None);
        assert_eq!(role, SubagentRole::Backend);
        assert!(role.is_mutating());
        assert!(score > 0);

        // Coder keeps its default Worktree isolation even when the prompt mentions "review" or "test"
        assert!(SubagentRole::Coder.should_isolate_worktree(
            "Implement feature and review test output",
            &[],
            None
        ));
        // Explicit isolate_branch=false runs Coder in Shared mode without demoting role to Scout
        assert!(!SubagentRole::Coder.should_isolate_worktree(
            "Implement feature",
            &[],
            Some(false)
        ));
    }
}
