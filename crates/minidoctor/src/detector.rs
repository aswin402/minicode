//! Zero-dependency workspace stack detector, git diff scope resolver, and source file collector.

use crate::config::DoctorConfig;
use crate::types::{DetectedStack, ProjectProfile, ScanScope};
use ignore::WalkBuilder;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Directories always excluded from source scanning to keep analysis fast and noise-free.
pub const DEFAULT_SKIP_DIRS: &[&str] = &[
    ".git",
    ".minicode",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".dart_tool",
    ".pub-cache",
    "migrations",
    "alembic",
    "generated",
    "vendor",
    "coverage",
    ".turbo",
    ".cache",
];

/// Supported source extensions inspected by `minidoctor`.
pub const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "dart", "go", "html", "css",
    "scss", "sql",
];

/// A source file loaded into memory for analysis passes.
#[derive(Debug, Clone)]
pub struct LoadedFile {
    pub rel_path: String,
    pub ext: String,
    pub content: String,
    pub line_count: usize,
    pub is_test: bool,
}

/// Detects stacks & frameworks from workspace manifests and scanned files.
#[must_use]
pub fn detect_project_profile(workspace: &Path, files: &[LoadedFile]) -> ProjectProfile {
    let mut stacks: BTreeSet<DetectedStack> = BTreeSet::new();
    let mut manifest_notes = Vec::new();

    // 1. Rust (`Cargo.toml`)
    if let Ok(cargo) = fs::read_to_string(workspace.join("Cargo.toml")) {
        stacks.insert(DetectedStack::Rust);
        let lower = cargo.to_ascii_lowercase();
        if lower.contains("tokio") {
            stacks.insert(DetectedStack::Tokio);
        }
        if lower.contains("ratatui") {
            stacks.insert(DetectedStack::Ratatui);
        }
        if lower.contains("axum") {
            stacks.insert(DetectedStack::Axum);
        }
        manifest_notes.push("Cargo.toml".to_string());
    }

    // 2. JavaScript / TypeScript (`package.json`)
    if let Ok(pkg) = fs::read_to_string(workspace.join("package.json")) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&pkg) {
            let mut dep_names = HashSet::new();
            for sec in ["dependencies", "devDependencies", "peerDependencies"] {
                if let Some(obj) = val.get(sec).and_then(|v| v.as_object()) {
                    for k in obj.keys() {
                        dep_names.insert(k.to_ascii_lowercase());
                    }
                }
            }
            if dep_names.contains("typescript") || workspace.join("tsconfig.json").exists() {
                stacks.insert(DetectedStack::TypeScript);
            } else {
                stacks.insert(DetectedStack::JavaScript);
            }
            if dep_names.contains("react")
                || dep_names.contains("react-dom")
                || dep_names.contains("react-native")
            {
                stacks.insert(DetectedStack::React);
            }
            if dep_names.contains("next") {
                stacks.insert(DetectedStack::NextJs);
                stacks.insert(DetectedStack::React);
            }
            if dep_names.contains("vite") {
                stacks.insert(DetectedStack::Vite);
            }
            if dep_names.contains("hono") || dep_names.iter().any(|d| d.starts_with("@hono/")) {
                stacks.insert(DetectedStack::Hono);
            }
            if dep_names.contains("express") {
                stacks.insert(DetectedStack::Express);
            }
            if dep_names.contains("prisma") || dep_names.contains("@prisma/client") {
                stacks.insert(DetectedStack::Prisma);
            }
            if dep_names.contains("zod") {
                stacks.insert(DetectedStack::Zod);
            }
            if dep_names.contains("pino") {
                stacks.insert(DetectedStack::Pino);
            }
            manifest_notes.push("package.json".to_string());
        }
    }

    // 3. Python (`pyproject.toml` / `requirements.txt`)
    let mut py_manifest = String::new();
    for rel in ["pyproject.toml", "requirements.txt", "Pipfile"] {
        if let Ok(text) = fs::read_to_string(workspace.join(rel)) {
            stacks.insert(DetectedStack::Python);
            py_manifest.push('\n');
            py_manifest.push_str(&text.to_ascii_lowercase());
            manifest_notes.push(rel.to_string());
        }
    }
    if !py_manifest.is_empty() {
        if py_manifest.contains("fastapi") {
            stacks.insert(DetectedStack::FastApi);
        }
        if py_manifest.contains("django") {
            stacks.insert(DetectedStack::Django);
        }
        if py_manifest.contains("sqlalchemy") {
            stacks.insert(DetectedStack::SqlAlchemy);
        }
        if py_manifest.contains("pydantic") {
            stacks.insert(DetectedStack::Pydantic);
        }
    }

    // 4. Flutter / Dart (`pubspec.yaml`)
    if let Ok(pubspec) = fs::read_to_string(workspace.join("pubspec.yaml")) {
        stacks.insert(DetectedStack::Dart);
        if pubspec.to_ascii_lowercase().contains("flutter") {
            stacks.insert(DetectedStack::Flutter);
        }
        manifest_notes.push("pubspec.yaml".to_string());
    }

    // 5. Go (`go.mod`)
    if workspace.join("go.mod").exists() {
        stacks.insert(DetectedStack::Go);
        manifest_notes.push("go.mod".to_string());
    }

    // 6. Infer additional stacks from actual scanned files
    let mut language_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_loc = 0usize;
    for f in files {
        total_loc = total_loc.saturating_add(f.line_count);
        let lang = match f.ext.as_str() {
            "rs" => {
                stacks.insert(DetectedStack::Rust);
                "Rust"
            }
            "py" => {
                stacks.insert(DetectedStack::Python);
                if f.content.contains("FastAPI(") || f.content.contains("APIRouter(") {
                    stacks.insert(DetectedStack::FastApi);
                }
                "Python"
            }
            "tsx" => {
                stacks.insert(DetectedStack::TypeScript);
                stacks.insert(DetectedStack::React);
                "TypeScript/React"
            }
            "jsx" => {
                stacks.insert(DetectedStack::JavaScript);
                stacks.insert(DetectedStack::React);
                "JavaScript/React"
            }
            "ts" | "mts" | "cts" => {
                stacks.insert(DetectedStack::TypeScript);
                if f.content.contains("new Hono(") {
                    stacks.insert(DetectedStack::Hono);
                }
                "TypeScript"
            }
            "js" | "mjs" | "cjs" => {
                stacks.insert(DetectedStack::JavaScript);
                "JavaScript"
            }
            "dart" => {
                stacks.insert(DetectedStack::Dart);
                if f.content.contains("package:flutter/") {
                    stacks.insert(DetectedStack::Flutter);
                }
                "Dart/Flutter"
            }
            "go" => {
                stacks.insert(DetectedStack::Go);
                "Go"
            }
            "html" | "css" | "scss" => {
                stacks.insert(DetectedStack::HtmlCss);
                "HTML/CSS"
            }
            "sql" => "SQL",
            _ => "Other",
        };
        *language_counts.entry(lang.to_string()).or_insert(0) += 1;
    }

    ProjectProfile {
        stacks: stacks.into_iter().collect(),
        file_count: files.len(),
        total_loc,
        language_counts,
        manifest_notes,
    }
}

/// Returns current short git HEAD hash if workspace is a git repository.
#[must_use]
pub fn current_git_head(workspace: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Resolves changed workspace-relative file paths via git diff + working-tree status.
#[must_use]
pub fn resolve_git_changed_files(workspace: &Path, git_ref: Option<&str>) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();

    if let Some(ref_name) = git_ref {
        if let Ok(out) = Command::new("git")
            .args(["diff", "--name-only", "--diff-filter=ACMR", ref_name])
            .current_dir(workspace)
            .output()
        {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        set.insert(trimmed.replace('\\', "/"));
                    }
                }
            }
        }
    }

    // Always include uncommitted working-tree modified & untracked files in diff mode
    if let Ok(out) = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(workspace)
        .output()
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if line.len() > 3 && !line.starts_with(" D") && !line.starts_with("D ") {
                    let raw = line[3..].trim();
                    let target = raw.rsplit(" -> ").next().unwrap_or(raw).trim_matches('"');
                    if !target.is_empty() {
                        set.insert(target.replace('\\', "/"));
                    }
                }
            }
        }
    }

    set.into_iter().collect()
}

/// Collects and loads source files according to `ScanScope` and `DoctorConfig`.
pub fn collect_source_files(
    workspace: &Path,
    scope: &ScanScope,
    cfg: &DoctorConfig,
    max_files: usize,
) -> Vec<LoadedFile> {
    match scope {
        ScanScope::Files(rel_paths) => rel_paths
            .iter()
            .filter_map(|rel| load_single_file(workspace, rel, cfg))
            .take(max_files)
            .collect(),
        ScanScope::Changed(git_ref) => {
            let changed = resolve_git_changed_files(workspace, git_ref.as_deref());
            changed
                .iter()
                .filter_map(|rel| load_single_file(workspace, rel, cfg))
                .take(max_files)
                .collect()
        }
        ScanScope::Full => collect_full_workspace(workspace, cfg, max_files),
    }
}

fn collect_full_workspace(
    workspace: &Path,
    cfg: &DoctorConfig,
    max_files: usize,
) -> Vec<LoadedFile> {
    let mut results = Vec::new();
    let walker = WalkBuilder::new(workspace)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                if let Some(name) = entry.file_name().to_str() {
                    if DEFAULT_SKIP_DIRS.contains(&name) {
                        return false;
                    }
                }
            }
            true
        })
        .build();

    for entry in walker.flatten() {
        if results.len() >= max_files {
            break;
        }
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(workspace) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if let Some(loaded) = load_path(path, &rel_str, cfg) {
            results.push(loaded);
        }
    }

    results.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    results
}

fn load_single_file(workspace: &Path, rel_path: &str, cfg: &DoctorConfig) -> Option<LoadedFile> {
    let clean_rel = rel_path.trim_start_matches("./").replace('\\', "/");
    if clean_rel
        .split('/')
        .any(|seg| DEFAULT_SKIP_DIRS.contains(&seg))
    {
        return None;
    }
    let full: PathBuf = workspace.join(&clean_rel);
    if !full.is_file() {
        return None;
    }
    load_path(&full, &clean_rel, cfg)
}

fn load_path(full_path: &Path, rel_path: &str, cfg: &DoctorConfig) -> Option<LoadedFile> {
    if cfg.is_file_ignored(rel_path) {
        return None;
    }
    let ext = full_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())?;
    if !SOURCE_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    // Skip generated/minified files and oversized blobs (> 1.5 MiB)
    let meta = fs::metadata(full_path).ok()?;
    if meta.len() > 1_500_000 {
        return None;
    }
    if rel_path.ends_with(".min.js")
        || rel_path.ends_with(".min.css")
        || rel_path.ends_with(".d.ts")
        || rel_path.ends_with(".g.dart")
        || rel_path.ends_with(".freezed.dart")
    {
        return None;
    }
    let content = fs::read_to_string(full_path).ok()?;
    let line_count = content.lines().count();
    let is_test = is_test_file_path(rel_path);
    Some(LoadedFile {
        rel_path: rel_path.to_string(),
        ext,
        content,
        line_count,
        is_test,
    })
}

/// Heuristic to detect dedicated unit/integration test files so production-only rules stay quiet in tests.
#[must_use]
pub fn is_test_file_path(rel_path: &str) -> bool {
    let lower = rel_path.to_ascii_lowercase();
    let fname = lower.rsplit('/').next().unwrap_or(&lower);
    lower.starts_with("tests/")
        || lower.starts_with("test/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("/__tests__/")
        || fname.starts_with("test_")
        || fname.ends_with("_test.rs")
        || fname.ends_with("_test.go")
        || fname.ends_with("_test.py")
        || fname.ends_with("_test.dart")
        || fname.contains(".test.")
        || fname.contains(".spec.")
        || fname == "tests.rs"
}
