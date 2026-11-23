//! Core diagnostic, stack detection, progress, and scoring types for `minidoctor`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Severity level of an individual diagnostic finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Error,
    Warning,
    Info,
}

impl Severity {
    #[must_use]
    pub const fn badge(self) -> &'static str {
        match self {
            Self::Critical => "CRIT",
            Self::Error => "ERR",
            Self::Warning => "WARN",
            Self::Info => "INFO",
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }

    #[must_use]
    pub fn from_str_loose(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "crit" | "critical" => Some(Self::Critical),
            "err" | "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warning),
            "info" | "note" | "hint" => Some(Self::Info),
            _ => None,
        }
    }
}

/// High-level diagnostic dimension used for category sub-scores and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    Correctness,
    Security,
    Performance,
    Architecture,
    DeadCode,
    Maintainability,
}

impl DiagnosticCategory {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Correctness => "Correctness",
            Self::Security => "Security",
            Self::Performance => "Performance",
            Self::Architecture => "Architecture",
            Self::DeadCode => "Dead Code",
            Self::Maintainability => "Maintainability",
        }
    }
}

/// Sub-classification for dead code findings (resolves `flutter_doctor`'s `subType` display).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeadCodeKind {
    UnusedFile,
    UnusedExport,
    DuplicateCode,
    UnusedSymbol,
}

impl DeadCodeKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnusedFile => "UNUSED FILE",
            Self::UnusedExport => "UNUSED EXPORT",
            Self::DuplicateCode => "DUPLICATE",
            Self::UnusedSymbol => "UNUSED SYMBOL",
        }
    }
}

/// A single actionable diagnostic emitted by `minidoctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule: String,
    pub severity: Severity,
    pub category: DiagnosticCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_code_kind: Option<DeadCodeKind>,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub suggestion: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn location(&self) -> String {
        if self.line > 0 {
            format!("{}:{}", self.file, self.line)
        } else {
            self.file.clone()
        }
    }
}

/// Automatically detected language / framework / ecosystem stacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedStack {
    Rust,
    Tokio,
    Ratatui,
    Axum,
    Python,
    FastApi,
    Django,
    SqlAlchemy,
    Pydantic,
    TypeScript,
    JavaScript,
    React,
    NextJs,
    Vite,
    Hono,
    Express,
    Prisma,
    Zod,
    Pino,
    Flutter,
    Dart,
    Go,
    HtmlCss,
}

impl DetectedStack {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Tokio => "Tokio",
            Self::Ratatui => "Ratatui",
            Self::Axum => "Axum",
            Self::Python => "Python",
            Self::FastApi => "FastAPI",
            Self::Django => "Django",
            Self::SqlAlchemy => "SQLAlchemy",
            Self::Pydantic => "Pydantic",
            Self::TypeScript => "TypeScript",
            Self::JavaScript => "JavaScript",
            Self::React => "React",
            Self::NextJs => "Next.js",
            Self::Vite => "Vite",
            Self::Hono => "Hono",
            Self::Express => "Express",
            Self::Prisma => "Prisma",
            Self::Zod => "Zod",
            Self::Pino => "Pino",
            Self::Flutter => "Flutter",
            Self::Dart => "Dart",
            Self::Go => "Go",
            Self::HtmlCss => "HTML/CSS",
        }
    }
}

/// Detected workspace stack profile and file metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectProfile {
    pub stacks: Vec<DetectedStack>,
    pub file_count: usize,
    pub total_loc: usize,
    pub language_counts: BTreeMap<String, usize>,
    pub manifest_notes: Vec<String>,
}

impl ProjectProfile {
    #[must_use]
    pub fn has(&self, stack: DetectedStack) -> bool {
        self.stacks.contains(&stack)
    }

    #[must_use]
    pub fn stack_summary(&self) -> String {
        if self.stacks.is_empty() {
            "Generic Workspace".to_string()
        } else {
            self.stacks
                .iter()
                .map(|s| s.label())
                .collect::<Vec<_>>()
                .join(" · ")
        }
    }
}

/// Scan target scope.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScanScope {
    /// Scan the entire workspace respecting `.gitignore` and config exclusions.
    #[default]
    Full,
    /// Scan only files changed vs `git` ref (or dirty working tree if `None`).
    Changed(Option<String>),
    /// Scan an explicit list of workspace-relative file paths.
    Files(Vec<String>),
}

impl ScanScope {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Full => "full workspace".to_string(),
            Self::Changed(Some(r)) => format!("git diff ({r})"),
            Self::Changed(None) => "changed files".to_string(),
            Self::Files(files) => format!("{} file(s)", files.len()),
        }
    }
}

/// Runtime options passed to [`crate::run_doctor`].
#[derive(Debug, Clone)]
pub struct DoctorOptions {
    pub scope: ScanScope,
    pub run_dead_code: bool,
    pub run_security: bool,
    pub write_workspace_artifacts: bool,
    pub export_md_path: Option<std::path::PathBuf>,
    pub threshold_override: Option<u8>,
    pub max_files: usize,
}

impl Default for DoctorOptions {
    fn default() -> Self {
        Self {
            scope: ScanScope::Full,
            run_dead_code: true,
            run_security: true,
            write_workspace_artifacts: true,
            export_md_path: None,
            threshold_override: None,
            max_files: 2500,
        }
    }
}

/// Live analysis phases streamed to the animated TUI modal and CLI progress display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanPhase {
    DetectingStacks,
    NativeAstRules,
    DeadCodeAndDuplicates,
    SecurityAudit,
    ScoringAndArtifacts,
    Completed,
}

impl ScanPhase {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::DetectingStacks => 0,
            Self::NativeAstRules => 1,
            Self::DeadCodeAndDuplicates => 2,
            Self::SecurityAudit => 3,
            Self::ScoringAndArtifacts => 4,
            Self::Completed => 5,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DetectingStacks => "Stack & Manifest Detection",
            Self::NativeAstRules => "Native Tree-sitter AST Rules",
            Self::DeadCodeAndDuplicates => "Dead Code & Duplicate Graph",
            Self::SecurityAudit => "Security & Secret Hygiene",
            Self::ScoringAndArtifacts => "Scoring & NEEDEDFIX.md Report",
            Self::Completed => "Analysis Complete",
        }
    }
}

/// Live progress packet emitted while a doctor scan runs.
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    pub percent: u8,
    pub detail: String,
    pub detected_stacks: Vec<DetectedStack>,
    pub files_scanned: usize,
    pub findings_so_far: usize,
}

/// Weighted 0–100 health score breakdown with historical delta & sparkline trend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub overall: u8,
    pub grade: String,
    pub label: String,
    pub correctness: u8,
    pub security: u8,
    pub performance: u8,
    pub architecture: u8,
    pub maintainability: u8,
    pub criticals: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub dead_code: usize,
    pub security_issues: usize,
    pub threshold: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_from_previous: Option<i16>,
    #[serde(default)]
    pub trend_history: Vec<u8>,
}

/// Historical 1-line JSONL record stored in `.minicode/minidoctor/history.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_head: Option<String>,
    pub scope: String,
    pub score: u8,
    pub criticals: usize,
    pub errors: usize,
    pub warnings: usize,
    pub dead_code: usize,
    pub files_scanned: usize,
}

/// Complete report returned by `minidoctor` and saved to `.minicode/minidoctor/latest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub version: String,
    pub generated_at: String,
    pub workspace: String,
    pub scope_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_head: Option<String>,
    pub duration_ms: u64,
    pub profile: ProjectProfile,
    pub score: ScoreBreakdown,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neededfix_path: Option<String>,
}

impl DoctorReport {
    #[must_use]
    pub fn passed_threshold(&self) -> bool {
        self.score.overall >= self.score.threshold
    }

    #[must_use]
    pub fn top_priority_diagnostics(&self, limit: usize) -> Vec<&Diagnostic> {
        let mut sorted: Vec<&Diagnostic> = self
            .diagnostics
            .iter()
            .filter(|d| matches!(d.severity, Severity::Critical | Severity::Error))
            .collect();
        sorted.sort_by(|a, b| {
            a.severity
                .cmp(&b.severity)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });
        sorted.into_iter().take(limit).collect()
    }
}

/// Static documentation entry for `rules list` / `rules explain` and the TUI detail drawer.
#[derive(Debug, Clone, Copy)]
pub struct RuleMeta {
    pub id: &'static str,
    pub stack: &'static str,
    pub category: DiagnosticCategory,
    pub default_severity: Severity,
    pub summary: &'static str,
    pub why: &'static str,
    pub how_to_fix: &'static str,
}
