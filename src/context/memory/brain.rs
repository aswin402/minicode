//! Native Rust Project Brain engine (`mindmuxai/brain.md` open format compatibility).
//!
//! Stores durable, Git-tracked project knowledge as plain Markdown pages in `brain/pages/<id>.md`
//! with a `BRAIN.md` contract at the workspace root. Enforces correctness-by-construction:
//! - Every page separates rewritable `<!-- compiled_truth -->` from an immutable append-only `## Timeline`.
//! - `update_truth` requires an explicit `summary` and atomically rewrites `compiled_truth`, bumps
//!   `updated`, and appends a corresponding timeline entry via temp-file + rename so silent,
//!   untraced knowledge drift is impossible.
//! - Zero Node.js or external CLI dependencies; invoked natively in-process by the orchestrator.

use crate::error::{Result, ToolError};
use chrono::Utc;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

fn brain_io_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub const BRAIN_CONTRACT_FILE: &str = "BRAIN.md";
pub const BRAIN_DIR_NAME: &str = "brain";
pub const BRAIN_PAGES_SUBDIR: &str = "pages";
pub const COMPILED_TRUTH_MARKER: &str = "<!-- compiled_truth -->";
pub const TIMELINE_HEADING: &str = "## Timeline";

/// Knowledge page category defined by the `brain.md` specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BrainCategory {
    Project,
    Decision,
    Concept,
    Reference,
    Person,
}

impl BrainCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Decision => "decision",
            Self::Concept => "concept",
            Self::Reference => "reference",
            Self::Person => "person",
        }
    }
}

impl FromStr for BrainCategory {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "project" => Ok(Self::Project),
            "decision" => Ok(Self::Decision),
            "concept" => Ok(Self::Concept),
            "reference" => Ok(Self::Reference),
            "person" => Ok(Self::Person),
            other => Err(format!(
                "Invalid brain category '{other}' (expected: project, decision, concept, reference, person)"
            )),
        }
    }
}

/// Lifecycle visibility status for a project brain page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BrainStatus {
    Active,
    Draft,
    Archived,
}

impl BrainStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Draft => "draft",
            Self::Archived => "archived",
        }
    }
}

impl FromStr for BrainStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "draft" => Ok(Self::Draft),
            "archived" => Ok(Self::Archived),
            other => Err(format!(
                "Invalid brain status '{other}' (expected: active, draft, archived)"
            )),
        }
    }
}

/// Event kind inside a page's append-only `## Timeline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimelineKind {
    Decision,
    Evidence,
    Reversal,
    Note,
}

impl TimelineKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Evidence => "evidence",
            Self::Reversal => "reversal",
            Self::Note => "note",
        }
    }
}

impl FromStr for TimelineKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "decision" => Ok(Self::Decision),
            "evidence" => Ok(Self::Evidence),
            "reversal" => Ok(Self::Reversal),
            "note" => Ok(Self::Note),
            other => Err(format!(
                "Invalid timeline kind '{other}' (expected: decision, evidence, reversal, note)"
            )),
        }
    }
}

/// Immutable provenance entry inside a page's `## Timeline`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub time: String,
    pub kind: TimelineKind,
    pub summary: String,
    pub source: Option<String>,
    pub affects: Vec<String>,
}

/// Complete parsed representation of `brain/pages/<id>.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrainPage {
    pub id: String,
    pub title: String,
    pub category: BrainCategory,
    pub status: BrainStatus,
    pub tags: Vec<String>,
    pub created: String,
    pub updated: String,
    pub compiled_truth: String,
    pub timeline: Vec<TimelineEntry>,
}

/// Lightweight summary row for listings and session-start prompt indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrainPageSummary {
    pub id: String,
    pub title: String,
    pub category: BrainCategory,
    pub status: BrainStatus,
    pub tags: Vec<String>,
    pub updated: String,
    pub timeline_count: usize,
}

/// Ranked search hit over project brain pages.
#[derive(Debug, Clone)]
pub struct BrainSearchHit {
    pub summary: BrainPageSummary,
    pub score: f64,
    pub snippet: String,
}

/// Request payload for creating a new brain page.
#[derive(Debug, Clone)]
pub struct CreatePageRequest {
    pub id: String,
    pub title: String,
    pub category: BrainCategory,
    pub status: BrainStatus,
    pub tags: Vec<String>,
    pub compiled_truth: String,
    pub summary: String,
    pub source: Option<String>,
    pub affects: Vec<String>,
}

/// Request payload for atomically rewriting `compiled_truth` + appending a timeline entry.
#[derive(Debug, Clone)]
pub struct UpdateTruthRequest {
    pub id: String,
    pub compiled_truth: String,
    pub summary: String,
    pub kind: TimelineKind,
    pub title: Option<String>,
    pub status: Option<BrainStatus>,
    pub tags: Option<Vec<String>>,
    pub source: Option<String>,
    pub affects: Vec<String>,
}

/// Request payload for appending an immutable provenance entry without touching `compiled_truth`.
#[derive(Debug, Clone)]
pub struct AppendTimelineRequest {
    pub id: String,
    pub kind: TimelineKind,
    pub summary: String,
    pub source: Option<String>,
    pub affects: Vec<String>,
}

/// Structural integrity diagnostic emitted by `ProjectBrain::validate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrainValidationIssue {
    pub severity: &'static str, // "error" | "warning"
    pub page_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct BrainValidationReport {
    pub pages_checked: usize,
    pub issues: Vec<BrainValidationIssue>,
}

impl BrainValidationReport {
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.issues.iter().filter(|i| i.severity == "error").count()
    }

    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == "warning")
            .count()
    }
}

/// Native Rust Project Brain storage, search, bootstrap, and validation engine.
pub struct ProjectBrain;

impl ProjectBrain {
    #[must_use]
    pub fn contract_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(BRAIN_CONTRACT_FILE)
    }

    #[must_use]
    pub fn pages_dir(workspace_root: &Path) -> PathBuf {
        workspace_root.join(BRAIN_DIR_NAME).join(BRAIN_PAGES_SUBDIR)
    }

    #[must_use]
    pub fn page_path(workspace_root: &Path, id: &str) -> PathBuf {
        Self::pages_dir(workspace_root).join(format!("{id}.md"))
    }

    /// Returns true if the workspace has opted into a repo-native Project Brain (`BRAIN.md` or `brain/pages/`).
    #[must_use]
    pub fn is_initialized(workspace_root: &Path) -> bool {
        Self::contract_path(workspace_root).is_file() || Self::pages_dir(workspace_root).is_dir()
    }

    /// Validates strict kebab-case page IDs (`^[a-z0-9]+(?:-[a-z0-9]+)*$`) matching `brain.md`.
    #[must_use]
    pub fn is_valid_page_id(id: &str) -> bool {
        if id.is_empty() || id.len() > 80 {
            return false;
        }
        let mut prev_dash = true;
        for b in id.bytes() {
            if b == b'-' {
                if prev_dash {
                    return false;
                }
                prev_dash = true;
            } else if b.is_ascii_lowercase() || b.is_ascii_digit() {
                prev_dash = false;
            } else {
                return false;
            }
        }
        !prev_dash
    }

    /// Initializes `brain/pages/` and a `BRAIN.md` contract at the workspace root (idempotent;
    /// never overwrites an existing `BRAIN.md`).
    pub fn init(workspace_root: &Path, project_name: Option<&str>) -> Result<String> {
        let _guard = brain_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::init_locked(workspace_root, project_name)
    }

    fn init_locked(workspace_root: &Path, project_name: Option<&str>) -> Result<String> {
        let pages_dir = Self::pages_dir(workspace_root);
        fs::create_dir_all(&pages_dir).map_err(|e| ToolError::FileOp {
            path: pages_dir.display().to_string(),
            source: e,
        })?;

        let contract_path = Self::contract_path(workspace_root);
        let created_contract = if !contract_path.is_file() {
            let repo_name = project_name
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    workspace_root
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "this repository".to_string());
            let contract = format!(
                "# Project Brain (`{repo_name}`)\n\n\
                 This repository maintains durable, Git-tracked project memory under `brain/pages/<id>.md`\n\
                 following the open `brain.md` standard (compatible with `minicode`'s native `project_brain` tool\n\
                 and `mindmuxai/brain.md`).\n\n\
                 ## Page Structure Invariants\n\
                 - Each page has YAML frontmatter (`id`, `title`, `category`, `status`, `tags`, `created`, `updated`).\n\
                 - `<!-- compiled_truth -->` holds the **current best understanding** and is rewritten atomically.\n\
                 - `## Timeline` is **strictly append-only** (`decision | evidence | reversal | note`).\n\
                 - Never silently edit `compiled_truth` without recording *why* in `## Timeline`.\n\n\
                 ## Agent Operating Protocol\n\
                 1. Check the active `<project_brain_index>` or call `project_brain(action=\"list\")` / `search` before making architectural changes.\n\
                 2. Read settled decisions via `project_brain(action=\"read\", id=\"...\")` rather than relitigating them.\n\
                 3. Record durable decisions with `create`, evolve understanding with `update_truth` (mandatory `summary`), and log evidence/reversals with `append_timeline`.\n"
            );
            atomic_write(&contract_path, &contract)?;
            true
        } else {
            false
        };

        Ok(format!(
            "✔ Project Brain ready at `brain/pages/` ({}).",
            if created_contract {
                "created `BRAIN.md` contract"
            } else {
                "preserved existing `BRAIN.md`"
            }
        ))
    }

    /// Uses `WorkspaceProfiler` (covering all 18 canonical manifests and language grammars),
    /// real `toml`/`serde_json` manifest parsing, `README.md`, and verified `.minicode/decisions.jsonl`
    /// records to idempotently seed starter `project-overview` and `tech-stack` pages.
    pub fn bootstrap(workspace_root: &Path) -> Result<String> {
        let _guard = brain_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let init_msg = Self::init_locked(workspace_root, None)?;
        let repo_name = workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        use crate::context::workspace_profile::{WorkspaceProfile, PROJECT_MANIFESTS};

        let mut overview_lines: Vec<String> = Vec::new();
        let mut sources: Vec<String> = Vec::new();

        // 1. Discover present manifests from the canonical 18-manifest WorkspaceProfile table
        let present_manifests: Vec<&str> = PROJECT_MANIFESTS
            .iter()
            .copied()
            .filter(|m| workspace_root.join(m).is_file())
            .collect();
        for &m in &present_manifests {
            sources.push(m.to_string());
        }

        // 2. Parse structured project description cleanly via real `toml` / `serde_json` parsers
        if let Some(desc) = extract_manifest_description(workspace_root, &present_manifests) {
            overview_lines.push(format!("- **Description:** {desc}"));
        } else if let Ok(readme) = fs::read_to_string(workspace_root.join("README.md")) {
            if let Some(para) = readme
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("!["))
            {
                overview_lines.push(format!("- **Summary:** {para}"));
                sources.push("README.md".to_string());
            }
        }

        // 3. Extract unified stack tokens & language distribution via WorkspaceProfile
        let stack_tokens_set = WorkspaceProfile::extract_stack_tokens(workspace_root);
        let mut stack_tokens: Vec<String> = stack_tokens_set
            .into_iter()
            .filter(|t| t.len() >= 2 && !t.contains('/'))
            .collect();
        stack_tokens.sort();
        stack_tokens.truncate(16);

        let (_repo_state, code_files, _doc_files) = WorkspaceProfile::scan(workspace_root);
        let mut lang_counts: BTreeMap<String, usize> = BTreeMap::new();
        for path in &code_files {
            let label = WorkspaceProfile::language_label_for_path(path);
            if !label.is_empty() && label != "Unknown" {
                *lang_counts.entry(label).or_insert(0) += 1;
            }
        }
        let mut top_langs: Vec<(String, usize)> = lang_counts.into_iter().collect();
        top_langs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // 4. Top-level directory layout snapshot (excluding transient/hidden noise)
        let mut top_dirs: Vec<String> = Vec::new();
        if let Ok(entries) = fs::read_dir(workspace_root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.')
                    || crate::context::walker::is_excluded_dir_name(&name)
                    || name == BRAIN_DIR_NAME
                {
                    continue;
                }
                if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                    top_dirs.push(name);
                }
            }
        }
        top_dirs.sort();
        if !top_dirs.is_empty() {
            overview_lines.push(format!(
                "- **Top-level modules:** {}",
                top_dirs
                    .iter()
                    .take(12)
                    .map(|d| format!("`{d}/`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // 5. Carry forward verified historical decisions from `.minicode/decisions.jsonl` if present
        if let Ok(recent_decs) =
            crate::context::decision_record::DecisionLogger::load_recent(workspace_root, 5)
        {
            let verified: Vec<_> = recent_decs.into_iter().filter(|d| d.verified).collect();
            if !verified.is_empty() {
                overview_lines.push(format!(
                    "- **Recent verified decisions:** {}",
                    verified
                        .iter()
                        .take(3)
                        .map(|d| collapse_single_line(&d.intent))
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
                sources.push(".minicode/decisions.jsonl".to_string());
            }
        }

        let source_hint = if sources.is_empty() {
            None
        } else {
            Some(sources.join(", "))
        };
        let mut created_pages = Vec::new();

        if !Self::page_path(workspace_root, "project-overview").is_file() {
            let truth = if overview_lines.is_empty() {
                format!(
                    "`{repo_name}` repository overview bootstrapped for durable project memory."
                )
            } else {
                overview_lines.join("\n")
            };
            Self::create_page_locked(
                workspace_root,
                CreatePageRequest {
                    id: "project-overview".to_string(),
                    title: format!("{repo_name} — Project Overview"),
                    category: BrainCategory::Project,
                    status: BrainStatus::Active,
                    tags: vec!["overview".to_string(), "bootstrap".to_string()],
                    compiled_truth: truth,
                    summary:
                        "Bootstrapped initial project overview from WorkspaceProfiler and manifests"
                            .to_string(),
                    source: source_hint.clone(),
                    affects: Vec::new(),
                },
            )?;
            created_pages.push("project-overview");
        }

        if (!present_manifests.is_empty() || !stack_tokens.is_empty() || !top_langs.is_empty())
            && !Self::page_path(workspace_root, "tech-stack").is_file()
        {
            let mut stack_lines = Vec::new();
            if !present_manifests.is_empty() {
                stack_lines.push(format!(
                    "- **Manifests:** {}",
                    present_manifests
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !top_langs.is_empty() {
                stack_lines.push(format!(
                    "- **Languages:** {}",
                    top_langs
                        .iter()
                        .take(8)
                        .map(|(l, n)| format!("{l} ({n} file(s))"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !stack_tokens.is_empty() {
                stack_lines.push(format!("- **Stack tokens:** {}", stack_tokens.join(", ")));
            }
            Self::create_page_locked(
                workspace_root,
                CreatePageRequest {
                    id: "tech-stack".to_string(),
                    title: "Detected Technology Stack & Build Tooling".to_string(),
                    category: BrainCategory::Reference,
                    status: BrainStatus::Active,
                    tags: vec!["stack".to_string(), "tooling".to_string()],
                    compiled_truth: stack_lines.join("\n"),
                    summary: "Bootstrapped stack reference via WorkspaceProfiler".to_string(),
                    source: source_hint,
                    affects: vec!["project-overview".to_string()],
                },
            )?;
            created_pages.push("tech-stack");
        }

        Ok(format!(
            "{init_msg}\nBootstrapped {} new page(s): {}",
            created_pages.len(),
            if created_pages.is_empty() {
                "none (starter pages already exist)".to_string()
            } else {
                created_pages.join(", ")
            }
        ))
    }

    /// Creates a brand new page in `brain/pages/<id>.md` with an initial timeline entry.
    /// Refuses to overwrite an existing page (call `update_truth` instead to preserve timeline provenance).
    pub fn create_page(workspace_root: &Path, req: CreatePageRequest) -> Result<BrainPage> {
        let _guard = brain_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::create_page_locked(workspace_root, req)
    }

    fn create_page_locked(workspace_root: &Path, req: CreatePageRequest) -> Result<BrainPage> {
        let id = req.id.trim().to_string();
        if !Self::is_valid_page_id(&id) {
            return Err(ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: format!(
                    "Invalid page id '{id}': must be kebab-case ASCII lowercase/digits/hyphens (e.g. 'pure-rust-tls')"
                ),
            }
            .into());
        }
        let title = collapse_single_line(&req.title);
        if title.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: "Page `title` cannot be empty".to_string(),
            }
            .into());
        }
        let truth = req.compiled_truth.trim().to_string();
        if truth.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: "Page `compiled_truth` cannot be empty".to_string(),
            }
            .into());
        }

        // Ensure brain skeleton exists when explicitly creating a page
        if !Self::is_initialized(workspace_root) {
            let _ = Self::init_locked(workspace_root, None)?;
        } else {
            let pages_dir = Self::pages_dir(workspace_root);
            fs::create_dir_all(&pages_dir).map_err(|e| ToolError::FileOp {
                path: pages_dir.display().to_string(),
                source: e,
            })?;
        }

        let path = Self::page_path(workspace_root, &id);
        if path.exists() {
            return Err(ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: format!(
                    "Page '{id}' already exists. Use action='update_truth' (with a change summary) or action='append_timeline' to evolve it without losing provenance."
                ),
            }
            .into());
        }

        let now = now_iso_utc();
        let summary = {
            let s = collapse_single_line(&req.summary);
            if s.is_empty() {
                format!("Created {cat} page '{title}'", cat = req.category.as_str())
            } else {
                s
            }
        };
        let initial_kind = match req.category {
            BrainCategory::Decision => TimelineKind::Decision,
            _ => TimelineKind::Note,
        };
        let page = BrainPage {
            id: id.clone(),
            title,
            category: req.category,
            status: req.status,
            tags: sanitize_string_list(&req.tags),
            created: now.clone(),
            updated: now.clone(),
            compiled_truth: truth,
            timeline: vec![TimelineEntry {
                time: now,
                kind: initial_kind,
                summary,
                source: clean_optional(req.source),
                affects: sanitize_string_list(&req.affects),
            }],
        };

        let rendered = render_page(&page);
        atomic_write(&path, &rendered)?;
        Ok(page)
    }

    /// Atomically rewrites `compiled_truth` (and optional metadata) while forcing a matching
    /// append-only entry onto `## Timeline` in the exact same write.
    pub fn update_truth(workspace_root: &Path, req: UpdateTruthRequest) -> Result<BrainPage> {
        let _guard = brain_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let summary = collapse_single_line(&req.summary);
        if summary.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: "Action 'update_truth' requires a non-empty `summary` explaining why the compiled truth changed".to_string(),
            }
            .into());
        }
        let truth = req.compiled_truth.trim().to_string();
        if truth.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: "`compiled_truth` cannot be empty".to_string(),
            }
            .into());
        }

        let mut page = Self::read_page(workspace_root, &req.id)?;
        let now = now_iso_utc();

        page.compiled_truth = truth;
        page.updated = now.clone();
        if let Some(t) = req.title {
            let clean = collapse_single_line(&t);
            if !clean.is_empty() {
                page.title = clean;
            }
        }
        if let Some(st) = req.status {
            page.status = st;
        }
        if let Some(tags) = req.tags {
            page.tags = sanitize_string_list(&tags);
        }
        page.timeline.push(TimelineEntry {
            time: now,
            kind: req.kind,
            summary,
            source: clean_optional(req.source),
            affects: sanitize_string_list(&req.affects),
        });

        let path = Self::page_path(workspace_root, &page.id);
        let rendered = render_page(&page);
        atomic_write(&path, &rendered)?;
        Ok(page)
    }

    /// Strictly append-only provenance event on `## Timeline` without touching `compiled_truth`.
    pub fn append_timeline(workspace_root: &Path, req: AppendTimelineRequest) -> Result<BrainPage> {
        let _guard = brain_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let summary = collapse_single_line(&req.summary);
        if summary.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: "Action 'append_timeline' requires a non-empty `summary`".to_string(),
            }
            .into());
        }

        let mut page = Self::read_page(workspace_root, &req.id)?;
        let now = now_iso_utc();
        page.updated = now.clone();
        page.timeline.push(TimelineEntry {
            time: now,
            kind: req.kind,
            summary,
            source: clean_optional(req.source),
            affects: sanitize_string_list(&req.affects),
        });

        let path = Self::page_path(workspace_root, &page.id);
        let rendered = render_page(&page);
        atomic_write(&path, &rendered)?;
        Ok(page)
    }

    /// Reads and parses a single page from `brain/pages/<id>.md`.
    pub fn read_page(workspace_root: &Path, id: &str) -> Result<BrainPage> {
        let clean_id = id.trim();
        if !Self::is_valid_page_id(clean_id) {
            return Err(ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: format!("Invalid page id '{clean_id}'"),
            }
            .into());
        }
        let path = Self::page_path(workspace_root, clean_id);
        if !path.is_file() {
            return Err(ToolError::NotFound {
                name: format!("brain/pages/{clean_id}.md"),
            }
            .into());
        }
        let raw = fs::read_to_string(&path).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;
        parse_page(&raw, clean_id).map_err(|reason| {
            ToolError::InvalidArguments {
                name: "project_brain".to_string(),
                reason: format!("Failed to parse brain/pages/{clean_id}.md: {reason}"),
            }
            .into()
        })
    }

    /// Loads all valid pages from `brain/pages/`.
    pub fn load_all_pages(workspace_root: &Path) -> Result<Vec<BrainPage>> {
        let dir = Self::pages_dir(workspace_root);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut pages = Vec::new();
        let entries = fs::read_dir(&dir).map_err(|e| ToolError::FileOp {
            path: dir.display().to_string(),
            source: e,
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(page) = parse_page(&raw, stem) {
                    pages.push(page);
                }
            }
        }
        pages.sort_by(|a, b| {
            a.status
                .cmp(&b.status)
                .then_with(|| a.category.cmp(&b.category))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(pages)
    }

    /// Lists pages with optional category and status filtering.
    pub fn list_pages(
        workspace_root: &Path,
        category: Option<BrainCategory>,
        status: Option<BrainStatus>,
    ) -> Result<Vec<BrainPageSummary>> {
        let pages = Self::load_all_pages(workspace_root)?;
        Ok(pages
            .into_iter()
            .filter(|p| category.is_none_or(|c| p.category == c))
            .filter(|p| status.is_none_or(|s| p.status == s))
            .map(page_to_summary)
            .collect())
    }

    /// Returns reverse-graph incoming references (`other_page_id`s whose timeline `affects` list
    /// mentions `target_id`).
    #[must_use]
    pub fn incoming_affects(workspace_root: &Path, target_id: &str) -> Vec<String> {
        let Ok(pages) = Self::load_all_pages(workspace_root) else {
            return Vec::new();
        };
        let mut refs: Vec<String> = pages
            .into_iter()
            .filter(|p| {
                p.id != target_id
                    && p.timeline
                        .iter()
                        .any(|t| t.affects.iter().any(|a| a == target_id))
            })
            .map(|p| p.id)
            .collect();
        refs.sort();
        refs
    }

    /// Performs multi-field tokenized BM25F (`k1 = 1.2, b = 0.75`) lexical ranking across `id`,
    /// `title`, `tags`, `compiled_truth`, and `timeline` summaries with word-boundary tokenization
    /// (preventing false substring hits like `"go"` matching `"algorithm"` or `"log"`).
    pub fn search_pages(
        workspace_root: &Path,
        query: &str,
        limit: usize,
    ) -> Result<Vec<BrainSearchHit>> {
        let q_terms = tokenize_words(query);
        if q_terms.is_empty() {
            return Ok(Vec::new());
        }
        let pages = Self::load_all_pages(workspace_root)?;
        if pages.is_empty() {
            return Ok(Vec::new());
        }

        // Tokenize each field into word-level token streams once
        struct IndexedDoc {
            page: BrainPage,
            id_toks: Vec<String>,
            title_toks: Vec<String>,
            tag_toks: Vec<String>,
            truth_toks: Vec<String>,
            timeline_toks: Vec<String>,
            all_unique: HashSet<String>,
            doc_len: usize,
        }

        let indexed: Vec<IndexedDoc> = pages
            .into_iter()
            .map(|page| {
                let id_toks = tokenize_words(&page.id);
                let title_toks = tokenize_words(&page.title);
                let tag_toks = tokenize_words(&page.tags.join(" "));
                let truth_toks = tokenize_words(&page.compiled_truth);
                let timeline_joined = page
                    .timeline
                    .iter()
                    .map(|e| e.summary.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let timeline_toks = tokenize_words(&timeline_joined);
                let doc_len = id_toks.len()
                    + title_toks.len()
                    + tag_toks.len()
                    + truth_toks.len()
                    + timeline_toks.len();
                let mut all_unique = HashSet::new();
                for t in id_toks
                    .iter()
                    .chain(&title_toks)
                    .chain(&tag_toks)
                    .chain(&truth_toks)
                    .chain(&timeline_toks)
                {
                    all_unique.insert(t.clone());
                }
                IndexedDoc {
                    page,
                    id_toks,
                    title_toks,
                    tag_toks,
                    truth_toks,
                    timeline_toks,
                    all_unique,
                    doc_len: doc_len.max(1),
                }
            })
            .collect();

        let n_docs = indexed.len() as f64;
        let avg_dl = (indexed.iter().map(|d| d.doc_len).sum::<usize>() as f64 / n_docs).max(1.0);
        const K1: f64 = 1.2;
        const B: f64 = 0.75;

        let token_matches = |tok: &str, term: &str| -> f64 {
            if tok == term {
                1.0
            } else if term.len() >= 3 && tok.starts_with(term) {
                0.45
            } else {
                0.0
            }
        };

        let mut doc_freq: HashMap<&str, usize> = HashMap::new();
        for term in &q_terms {
            let df = indexed
                .iter()
                .filter(|d| {
                    d.all_unique
                        .iter()
                        .any(|tok| token_matches(tok, term.as_str()) > 0.0)
                })
                .count();
            doc_freq.insert(term.as_str(), df);
        }

        let mut hits = Vec::new();
        for doc in indexed {
            let mut score = 0.0f64;
            let len_norm = 1.0 - B + B * (doc.doc_len as f64 / avg_dl);

            for term in &q_terms {
                let df = *doc_freq.get(term.as_str()).unwrap_or(&0) as f64;
                if df == 0.0 {
                    continue;
                }
                let idf = ((n_docs - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.2);

                let field_tf = |toks: &[String]| -> f64 {
                    toks.iter().map(|t| token_matches(t, term.as_str())).sum()
                };
                let pseudo_tf = 4.5 * field_tf(&doc.id_toks)
                    + 3.5 * field_tf(&doc.title_toks)
                    + 3.0 * field_tf(&doc.tag_toks)
                    + 1.5 * field_tf(&doc.truth_toks)
                    + 1.2 * field_tf(&doc.timeline_toks);

                if pseudo_tf > 0.0 {
                    score += idf * ((pseudo_tf * (K1 + 1.0)) / (pseudo_tf + K1 * len_norm));
                }
            }

            if score <= 0.0 {
                continue;
            }
            if doc.page.status == BrainStatus::Archived {
                score *= 0.65;
            }

            let snippet = doc
                .page
                .compiled_truth
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("")
                .chars()
                .take(160)
                .collect();

            hits.push(BrainSearchHit {
                summary: page_to_summary(doc.page),
                score,
                snippet,
            });
        }

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.summary.id.cmp(&b.summary.id))
        });
        hits.truncate(limit.max(1));
        Ok(hits)
    }

    /// Audits structural integrity of every `.md` file in `brain/pages/`:
    /// - Valid YAML frontmatter + enum values
    /// - Filename matches frontmatter `id` and valid kebab-case
    /// - Presence of `<!-- compiled_truth -->` and `## Timeline`
    /// - Dangling `affects: [<target>]` cross-page references
    #[must_use]
    pub fn validate(workspace_root: &Path) -> BrainValidationReport {
        let mut report = BrainValidationReport::default();
        let dir = Self::pages_dir(workspace_root);
        if !dir.is_dir() {
            return report;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            return report;
        };

        let mut parsed_pages: Vec<BrainPage> = Vec::new();
        let mut known_ids: HashSet<String> = HashSet::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            report.pages_checked += 1;

            if !Self::is_valid_page_id(&stem) {
                report.issues.push(BrainValidationIssue {
                    severity: "error",
                    page_id: stem.clone(),
                    message: format!("Filename '{stem}.md' is not valid kebab-case"),
                });
            }

            let Ok(raw) = fs::read_to_string(&path) else {
                report.issues.push(BrainValidationIssue {
                    severity: "error",
                    page_id: stem,
                    message: "Unreadable page file".to_string(),
                });
                continue;
            };

            if !raw.contains(COMPILED_TRUTH_MARKER) {
                report.issues.push(BrainValidationIssue {
                    severity: "warning",
                    page_id: stem.clone(),
                    message: format!("Missing `{COMPILED_TRUTH_MARKER}` delimiter"),
                });
            }
            if !raw.contains(TIMELINE_HEADING) {
                report.issues.push(BrainValidationIssue {
                    severity: "warning",
                    page_id: stem.clone(),
                    message: format!("Missing `{TIMELINE_HEADING}` append-only section"),
                });
            }

            match parse_page(&raw, &stem) {
                Ok(page) => {
                    if page.id != stem {
                        report.issues.push(BrainValidationIssue {
                            severity: "error",
                            page_id: stem.clone(),
                            message: format!(
                                "Frontmatter `id: {}` does not match filename `{stem}.md`",
                                page.id
                            ),
                        });
                    }
                    if page.compiled_truth.trim().is_empty() {
                        report.issues.push(BrainValidationIssue {
                            severity: "warning",
                            page_id: page.id.clone(),
                            message: "Empty `compiled_truth` section".to_string(),
                        });
                    }
                    known_ids.insert(page.id.clone());
                    parsed_pages.push(page);
                }
                Err(e) => {
                    report.issues.push(BrainValidationIssue {
                        severity: "error",
                        page_id: stem,
                        message: format!("Parse error: {e}"),
                    });
                }
            }
        }

        // Cross-page `affects:` referential integrity check
        for page in &parsed_pages {
            for ev in &page.timeline {
                for target in &ev.affects {
                    if !known_ids.contains(target) {
                        report.issues.push(BrainValidationIssue {
                            severity: "warning",
                            page_id: page.id.clone(),
                            message: format!(
                                "Timeline entry ({}) references missing page `{target}` in `affects`",
                                ev.time
                            ),
                        });
                    }
                }
            }
        }

        report
    }

    /// Formats a compact, KV-cache-friendly `<project_brain_index>` for session-start prompt
    /// injection. Returns `None` when the workspace has no `brain/pages/` pages (0 token cost).
    #[must_use]
    pub fn format_index_for_prompt(workspace_root: &Path) -> Option<String> {
        if !Self::is_initialized(workspace_root) {
            return None;
        }
        let summaries = Self::list_pages(workspace_root, None, None).ok()?;
        let visible: Vec<&BrainPageSummary> = summaries
            .iter()
            .filter(|s| s.status != BrainStatus::Archived)
            .collect();
        if visible.is_empty() {
            return None;
        }

        const MAX_INDEX_PAGES: usize = 30;
        let mut out = String::from("<project_brain_index source=\"brain/pages/\">\n");
        out.push_str(
            "Durable Git-tracked project memory (`project_brain` tool: `read`, `search`, `create`, `update_truth`, `append_timeline`):\n",
        );
        for s in visible.iter().take(MAX_INDEX_PAGES) {
            let tags_suffix = if s.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", s.tags.join(", "))
            };
            out.push_str(&format!(
                "- `{}` ({} | {}) — {}{}\n",
                s.id,
                s.category.as_str(),
                s.status.as_str(),
                s.title,
                tags_suffix
            ));
        }
        if visible.len() > MAX_INDEX_PAGES {
            out.push_str(&format!(
                "- ... (+{} more pages; use `project_brain(action=\"search\", query=\"...\")`)\n",
                visible.len() - MAX_INDEX_PAGES
            ));
        }
        out.push_str("</project_brain_index>");
        Some(out)
    }

    /// Renders a readable Markdown view of a single page (including incoming reverse `affects` links).
    #[must_use]
    pub fn format_page_display(workspace_root: &Path, page: &BrainPage) -> String {
        let mut out = render_page(page);
        let incoming = Self::incoming_affects(workspace_root, &page.id);
        if !incoming.is_empty() {
            out.push_str("\n---\nReferenced by (`affects`): ");
            out.push_str(
                &incoming
                    .iter()
                    .map(|id| format!("`{id}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push('\n');
        }
        out
    }
}

// ============================================================================
// Parsing, Rendering, and Atomic Persistence Helpers
// ============================================================================

fn now_iso_utc() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn page_to_summary(page: BrainPage) -> BrainPageSummary {
    BrainPageSummary {
        id: page.id,
        title: page.title,
        category: page.category,
        status: page.status,
        tags: page.tags,
        updated: page.updated,
        timeline_count: page.timeline.len(),
    }
}

fn collapse_single_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_optional(opt: Option<String>) -> Option<String> {
    opt.map(|s| collapse_single_line(&s))
        .filter(|s| !s.is_empty())
}

fn sanitize_string_list(items: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let clean = collapse_single_line(item);
        if !clean.is_empty() && seen.insert(clean.clone()) {
            out.push(clean);
        }
    }
    out
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
        path: parent.display().to_string(),
        source: e,
    })?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("page.md");
    let tmp_path = parent.join(format!(
        ".{file_name}.tmp.{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::write(&tmp_path, content).map_err(|e| ToolError::FileOp {
        path: tmp_path.display().to_string(),
        source: e,
    })?;
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        }
        .into());
    }
    Ok(())
}

fn yaml_scalar(s: &str) -> String {
    let needs_quotes = s.is_empty()
        || s.contains(':')
        || s.contains('#')
        || s.contains('"')
        || s.starts_with('-')
        || s.starts_with('[')
        || s.starts_with('{')
        || s.trim() != s;
    if needs_quotes {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn render_page(page: &BrainPage) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: {}\n", page.id));
    out.push_str(&format!("title: {}\n", yaml_scalar(&page.title)));
    out.push_str(&format!("category: {}\n", page.category.as_str()));
    out.push_str(&format!("status: {}\n", page.status.as_str()));
    if !page.tags.is_empty() {
        let tags_joined = page
            .tags
            .iter()
            .map(|t| yaml_scalar(t))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("tags: [{tags_joined}]\n"));
    }
    out.push_str(&format!("created: \"{}\"\n", page.created));
    out.push_str(&format!("updated: \"{}\"\n", page.updated));
    out.push_str("---\n");
    out.push_str(COMPILED_TRUTH_MARKER);
    out.push('\n');
    out.push_str(page.compiled_truth.trim());
    out.push_str("\n\n");
    out.push_str(TIMELINE_HEADING);
    out.push('\n');

    for entry in &page.timeline {
        out.push_str(&format!("- time: {}\n", entry.time));
        out.push_str(&format!("  kind: {}\n", entry.kind.as_str()));
        out.push_str(&format!("  summary: {}\n", yaml_scalar(&entry.summary)));
        if let Some(ref src) = entry.source {
            out.push_str(&format!("  source: {}\n", yaml_scalar(src)));
        }
        if !entry.affects.is_empty() {
            let aff_joined = entry
                .affects
                .iter()
                .map(|a| yaml_scalar(a))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("  affects: [{aff_joined}]\n"));
        }
    }
    out
}

fn parse_page(raw: &str, fallback_id: &str) -> std::result::Result<BrainPage, String> {
    let normalized = raw.replace("\r\n", "\n");
    let after_open = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| "missing opening YAML frontmatter `---`".to_string())?;
    let close_pos = after_open
        .find("\n---\n")
        .or_else(|| after_open.find("\n---"))
        .ok_or_else(|| "missing closing YAML frontmatter `---`".to_string())?;
    let fm_text = &after_open[..close_pos];
    let rest_start = if after_open[close_pos..].starts_with("\n---\n") {
        close_pos + 5
    } else {
        close_pos + 4
    };
    let body = &after_open[rest_start..];

    let mut fm: BTreeMap<String, String> = BTreeMap::new();
    for line in fm_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            fm.insert(k.trim().to_ascii_lowercase(), unquote_yaml(v.trim()));
        }
    }

    let id = fm
        .get("id")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_id.to_string());
    let title = fm
        .get("title")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing required frontmatter field `title`".to_string())?;
    let category = fm
        .get("category")
        .ok_or_else(|| "missing required frontmatter field `category`".to_string())?
        .parse::<BrainCategory>()?;
    let status = fm
        .get("status")
        .map(|s| s.parse::<BrainStatus>())
        .transpose()?
        .unwrap_or(BrainStatus::Active);
    let tags = fm
        .get("tags")
        .map(|raw_tags| parse_inline_yaml_list(raw_tags))
        .unwrap_or_default();
    let created = fm.get("created").cloned().unwrap_or_else(now_iso_utc);
    let updated = fm
        .get("updated")
        .cloned()
        .unwrap_or_else(|| created.clone());

    // Split body into `compiled_truth` and `## Timeline`
    let (truth_raw, timeline_raw) = if let Some(tl_pos) = find_timeline_heading(body) {
        let after_heading = &body[tl_pos..];
        let tl_body = after_heading
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or("");
        (&body[..tl_pos], tl_body)
    } else {
        (body, "")
    };

    let compiled_truth = truth_raw
        .trim()
        .strip_prefix(COMPILED_TRUTH_MARKER)
        .unwrap_or(truth_raw.trim())
        .trim()
        .to_string();

    let timeline = parse_timeline_section(timeline_raw);

    Ok(BrainPage {
        id,
        title,
        category,
        status,
        tags,
        created,
        updated,
        compiled_truth,
        timeline,
    })
}

fn find_timeline_heading(body: &str) -> Option<usize> {
    if body.starts_with(TIMELINE_HEADING) {
        return Some(0);
    }
    body.find(&format!("\n{TIMELINE_HEADING}"))
        .map(|idx| idx + 1)
}

fn parse_timeline_section(section: &str) -> Vec<TimelineEntry> {
    let mut entries: Vec<TimelineEntry> = Vec::new();
    let mut current: Option<TimelineEntry> = None;

    for raw_line in section.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(after_dash) = trimmed.strip_prefix("- ") {
            if let Some(prev) = current.take() {
                entries.push(prev);
            }
            let mut entry = TimelineEntry {
                time: String::new(),
                kind: TimelineKind::Note,
                summary: String::new(),
                source: None,
                affects: Vec::new(),
            };
            apply_timeline_field(&mut entry, after_dash);
            current = Some(entry);
        } else if let Some(ref mut entry) = current {
            apply_timeline_field(entry, trimmed);
        }
    }
    if let Some(last) = current {
        entries.push(last);
    }
    entries
}

fn apply_timeline_field(entry: &mut TimelineEntry, line: &str) {
    let Some((k, v)) = line.split_once(':') else {
        return;
    };
    let val = v.trim();
    match k.trim().to_ascii_lowercase().as_str() {
        "time" => entry.time = unquote_yaml(val),
        "kind" => {
            if let Ok(parsed) = unquote_yaml(val).parse::<TimelineKind>() {
                entry.kind = parsed;
            }
        }
        "summary" => entry.summary = unquote_yaml(val),
        "source" => {
            let s = unquote_yaml(val);
            if !s.is_empty() {
                entry.source = Some(s);
            }
        }
        "affects" => entry.affects = parse_inline_yaml_list(val),
        _ => {}
    }
}

fn unquote_yaml(raw: &str) -> String {
    let s = raw.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn parse_inline_yaml_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in inner.chars() {
        if in_quotes {
            if escaped {
                cur.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_quotes = false;
            } else {
                cur.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == ',' {
            let clean = unquote_yaml(&cur);
            if !clean.is_empty() {
                out.push(clean);
            }
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    let clean = unquote_yaml(&cur);
    if !clean.is_empty() {
        out.push(clean);
    }
    out
}

fn tokenize_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| t.len() >= 2)
        .collect()
}

/// Extracts a human-readable project description from present manifests using real structured
/// `toml` and `serde_json` parsers (never naive line-prefix scanning).
fn extract_manifest_description(workspace_root: &Path, manifests: &[&str]) -> Option<String> {
    for &manifest in manifests {
        let path = workspace_root.join(manifest);
        if manifest.ends_with(".toml") {
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(parsed) = toml::from_str::<toml::Value>(&raw) else {
                continue;
            };
            let desc = parsed
                .get("package")
                .and_then(|t| t.get("description"))
                .and_then(toml::Value::as_str)
                .or_else(|| {
                    parsed
                        .get("project")
                        .and_then(|t| t.get("description"))
                        .and_then(toml::Value::as_str)
                })
                .or_else(|| parsed.get("description").and_then(toml::Value::as_str));
            if let Some(d) = desc.map(collapse_single_line).filter(|s| !s.is_empty()) {
                return Some(d);
            }
        } else if manifest.ends_with(".json") {
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            if let Some(d) = parsed
                .get("description")
                .and_then(serde_json::Value::as_str)
                .map(collapse_single_line)
                .filter(|s| !s.is_empty())
            {
                return Some(d);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_brain_lifecycle_atomic_truth_timeline_search_and_validation() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(!ProjectBrain::is_initialized(tmp.path()));
        assert!(ProjectBrain::format_index_for_prompt(tmp.path()).is_none());

        // 1. Create a decision page (auto-initializes BRAIN.md + brain/pages/)
        let created = ProjectBrain::create_page(
            tmp.path(),
            CreatePageRequest {
                id: "pure-rust-tls".to_string(),
                title: "Use Pure Rust TLS (rustls)".to_string(),
                category: BrainCategory::Decision,
                status: BrainStatus::Active,
                tags: vec!["networking".to_string(), "portability".to_string()],
                compiled_truth:
                    "Use rustls-tls-webpki-roots so release binaries require zero OS OpenSSL."
                        .to_string(),
                summary: "Eliminated libssl-dev Linux build failures".to_string(),
                source: Some("Cargo.toml".to_string()),
                affects: Vec::new(),
            },
        )
        .expect("create page");
        assert!(ProjectBrain::is_initialized(tmp.path()));
        assert_eq!(created.timeline.len(), 1);
        assert_eq!(created.timeline[0].kind, TimelineKind::Decision);

        // 2. Duplicate creation must fail cleanly
        assert!(ProjectBrain::create_page(
            tmp.path(),
            CreatePageRequest {
                id: "pure-rust-tls".to_string(),
                title: "Dup".to_string(),
                category: BrainCategory::Decision,
                status: BrainStatus::Active,
                tags: Vec::new(),
                compiled_truth: "dup".to_string(),
                summary: "dup".to_string(),
                source: None,
                affects: Vec::new(),
            },
        )
        .is_err());

        // 3. `update_truth` without summary must be rejected (anti-drift invariant!)
        assert!(ProjectBrain::update_truth(
            tmp.path(),
            UpdateTruthRequest {
                id: "pure-rust-tls".to_string(),
                compiled_truth: "New truth".to_string(),
                summary: "   ".to_string(),
                kind: TimelineKind::Reversal,
                title: None,
                status: None,
                tags: None,
                source: None,
                affects: Vec::new(),
            },
        )
        .is_err());

        // 4. Valid `update_truth` atomically rewrites compiled_truth + appends timeline entry
        let updated = ProjectBrain::update_truth(
            tmp.path(),
            UpdateTruthRequest {
                id: "pure-rust-tls".to_string(),
                compiled_truth:
                    "Use rustls-tls-webpki-roots with ring crypto backend across all targets."
                        .to_string(),
                summary: "Pinned ring backend after musl cross-compile verification".to_string(),
                kind: TimelineKind::Evidence,
                title: None,
                status: None,
                tags: None,
                source: Some("CI build #42".to_string()),
                affects: vec!["release-pipeline".to_string()],
            },
        )
        .expect("update truth");
        assert_eq!(updated.timeline.len(), 2);
        assert!(updated.compiled_truth.contains("ring crypto backend"));

        // 5. `append_timeline` preserves compiled_truth byte-for-byte
        let appended = ProjectBrain::append_timeline(
            tmp.path(),
            AppendTimelineRequest {
                id: "pure-rust-tls".to_string(),
                kind: TimelineKind::Note,
                summary: "Confirmed zero libssl linkage via ldd".to_string(),
                source: Some("localupdate.sh".to_string()),
                affects: Vec::new(),
            },
        )
        .expect("append timeline");
        assert_eq!(appended.compiled_truth, updated.compiled_truth);
        assert_eq!(appended.timeline.len(), 3);

        // 6. Tokenized BM25F search + word-boundary immunity + prompt index
        let hits = ProjectBrain::search_pages(tmp.path(), "rustls musl", 5).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].summary.id, "pure-rust-tls");

        // Query `"go"` must NOT false-match `"Cargo"` or `"category"` (word-boundary tokenized BM25!)
        let no_false_hits = ProjectBrain::search_pages(tmp.path(), "go", 5).expect("search go");
        assert!(
            no_false_hits.is_empty(),
            "tokenized BM25 must not substring-match 'go' inside 'Cargo' or 'category'"
        );

        let index = ProjectBrain::format_index_for_prompt(tmp.path()).expect("index block");
        assert!(index.contains("<project_brain_index"));
        assert!(index.contains("`pure-rust-tls` (decision | active)"));

        // 7. Validator flags dangling `affects: [release-pipeline]` warning cleanly
        let report = ProjectBrain::validate(tmp.path());
        assert_eq!(report.pages_checked, 1);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.warning_count(), 1);
        assert!(report.issues[0].message.contains("release-pipeline"));
    }

    #[test]
    fn test_project_brain_bootstrap_reads_manifests_idempotently() {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nedition = \"2021\"\ndescription = \"Demo Rust engine\"\n",
        )
        .expect("write Cargo.toml");
        fs::create_dir_all(tmp.path().join("src")).expect("mkdir src");

        let first = ProjectBrain::bootstrap(tmp.path()).expect("bootstrap 1");
        assert!(first.contains("project-overview"));
        assert!(first.contains("tech-stack"));

        let overview = ProjectBrain::read_page(tmp.path(), "project-overview").expect("read");
        assert!(overview.compiled_truth.contains("Demo Rust engine"));
        let incoming = ProjectBrain::incoming_affects(tmp.path(), "project-overview");
        assert_eq!(incoming, vec!["tech-stack".to_string()]);

        // Idempotent second run creates 0 duplicate pages
        let second = ProjectBrain::bootstrap(tmp.path()).expect("bootstrap 2");
        assert!(second.contains("Bootstrapped 0 new page(s)"));
    }
}
