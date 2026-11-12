//! Unified syntax- and manifest-driven workspace profiler.
//!
//! Replaces scattered hardcoded extension arrays and duplicate manifest parsers across
//! the orchestrator, swarm planner, vault skill matcher, intent router, and reflexion loop
//! with a single Syntect grammar + manifest inspection engine.

use crate::agent::orchestrator::RepoState;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Canonical project foundation manifests ordered by specificity.
pub const PROJECT_MANIFESTS: &[&str] = &[
    "onpkg.json",
    "minikit.json",
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
    "deno.json",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "Gemfile",
    "composer.json",
    "pubspec.yaml",
    "mix.exs",
    "CMakeLists.txt",
    "Makefile",
    "index.html",
];

/// Modern ecosystems not included in Syntect 5.2's vintage Sublime default syntax bundle.
fn modern_grammar_label(ext_lower: &str) -> Option<&'static str> {
    match ext_lower {
        "ts" | "mts" | "cts" => Some("TypeScript"),
        "tsx" => Some("TypeScript/React"),
        "jsx" => Some("JavaScript/React"),
        "mjs" | "cjs" => Some("JavaScript"),
        "vue" => Some("Vue"),
        "svelte" => Some("Svelte"),
        "astro" => Some("Astro"),
        "kt" | "kts" => Some("Kotlin"),
        "swift" => Some("Swift"),
        "dart" => Some("Dart"),
        "zig" => Some("Zig"),
        "ex" | "exs" => Some("Elixir"),
        "gleam" => Some("Gleam"),
        "nim" => Some("Nim"),
        "prisma" => Some("Prisma"),
        "proto" => Some("Protobuf"),
        "graphql" | "gql" => Some("GraphQL"),
        "sol" => Some("Solidity"),
        "scss" | "sass" | "less" => Some("CSS/Stylesheet"),
        _ => None,
    }
}

/// Unified workspace inspection and file-classification engine.
pub struct WorkspaceProfile;

impl WorkspaceProfile {
    /// Classifies whether a path represents executable/compilable source code or markup
    /// using Syntect's grammar scope hierarchy plus modern language supplements.
    #[must_use]
    pub fn is_code_path(path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let ext_lower = ext.to_ascii_lowercase();
        if modern_grammar_label(&ext_lower).is_some() {
            return true;
        }
        if matches!(
            ext_lower.as_str(),
            "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "lock" | "xml" | "svg"
        ) {
            return false;
        }
        if let Some(syntax) = syntax_set().find_syntax_by_extension(&ext_lower) {
            let scope = syntax.scope.to_string();
            if scope.starts_with("source.") {
                return true;
            }
            if scope.starts_with("text.html") && !scope.contains("markdown") {
                return true;
            }
        }
        false
    }

    /// Classifies whether a path represents prose/documentation using Syntect scopes + prose formats.
    #[must_use]
    pub fn is_doc_path(path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let ext_lower = ext.to_ascii_lowercase();
        if matches!(
            ext_lower.as_str(),
            "md" | "markdown" | "mdown" | "mdx" | "txt" | "rst" | "adoc" | "org"
        ) {
            return true;
        }
        if let Some(syntax) = syntax_set().find_syntax_by_extension(&ext_lower) {
            let scope = syntax.scope.to_string();
            return scope.contains("markdown")
                || scope.starts_with("text.restructuredtext")
                || scope.starts_with("text.tex");
        }
        false
    }

    /// Classifies whether a path represents a frontend/UI surface file.
    #[must_use]
    pub fn is_ui_path(path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let ext_lower = ext.to_ascii_lowercase();
        if matches!(
            ext_lower.as_str(),
            "tsx" | "jsx" | "vue" | "svelte" | "astro" | "scss" | "sass" | "less"
        ) {
            return true;
        }
        if let Some(syntax) = syntax_set().find_syntax_by_extension(&ext_lower) {
            let scope = syntax.scope.to_string();
            return (scope.starts_with("text.html") && !scope.contains("markdown"))
                || scope.starts_with("source.css");
        }
        false
    }

    /// Resolves a clean human-readable language/domain label for any file path via Syntect + modern grammars.
    #[must_use]
    pub fn language_label_for_path(path: &Path) -> String {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_ascii_lowercase();
            if let Some(modern) = modern_grammar_label(&ext_lower) {
                return modern.to_string();
            }
            if matches!(ext_lower.as_str(), "toml" | "json" | "yaml" | "yml" | "ini") {
                return "Config".to_string();
            }
            if let Some(syntax) = syntax_set().find_syntax_by_extension(&ext_lower) {
                if syntax.name != "Plain Text" {
                    return syntax.name.clone();
                }
            }
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(syntax) = syntax_set().find_syntax_by_extension(name) {
                if syntax.name != "Plain Text" {
                    return syntax.name.clone();
                }
            }
        }
        "General".to_string()
    }

    /// Returns the first canonical project manifest present at `workspace_root`, if any.
    #[must_use]
    pub fn project_manifest(workspace_root: &Path) -> Option<&'static str> {
        // Keep `package.json` / `Cargo.toml` reporting deterministic when standard manifests exist alongside `minikit.json`
        const DISPLAY_ORDER: &[&str] = &[
            "package.json",
            "Cargo.toml",
            "pyproject.toml",
            "requirements.txt",
            "go.mod",
            "deno.json",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "Gemfile",
            "composer.json",
            "pubspec.yaml",
            "mix.exs",
            "CMakeLists.txt",
            "Makefile",
            "minikit.json",
            "onpkg.json",
            "index.html",
        ];
        DISPLAY_ORDER
            .iter()
            .copied()
            .find(|m| workspace_root.join(m).is_file())
    }

    /// Checks whether `workspace_root` exposes a web/UI surface.
    #[must_use]
    pub fn has_ui_surface(workspace_root: &Path, code_files: &[PathBuf]) -> bool {
        if workspace_root.join("package.json").is_file()
            || workspace_root.join("index.html").is_file()
        {
            return true;
        }
        code_files.iter().any(|p| Self::is_ui_path(p))
    }

    /// Scans `workspace_root` using `ignore::WalkBuilder` + Syntect grammar scopes to determine
    /// [`RepoState`] along with representative source and documentation files.
    #[must_use]
    pub fn scan(workspace_root: &Path) -> (RepoState, Vec<PathBuf>, Vec<PathBuf>) {
        if !workspace_root.exists() {
            return (RepoState::FreshWorkspace, Vec::new(), Vec::new());
        }

        let walker = ignore::WalkBuilder::new(workspace_root)
            .hidden(true)
            .git_ignore(true)
            .max_depth(Some(4))
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    "node_modules"
                        | "target"
                        | "dist"
                        | "build"
                        | ".next"
                        | "vendor"
                        | ".venv"
                        | "venv"
                        | "__pycache__"
                        | ".git"
                        | ".minicode"
                )
            })
            .build();

        let mut code_files = Vec::new();
        let mut doc_files = Vec::new();
        let mut has_other_files = false;

        for result in walker.flatten() {
            if code_files.len() >= 30 && doc_files.len() >= 30 && has_other_files {
                break;
            }
            if result.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                let path = result.path();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if name.starts_with('.') {
                    continue;
                }
                if Self::is_code_path(path) {
                    if code_files.len() < 30 {
                        code_files.push(path.to_path_buf());
                    }
                } else if Self::is_doc_path(path) {
                    if doc_files.len() < 30 {
                        doc_files.push(path.to_path_buf());
                    }
                } else {
                    has_other_files = true;
                }
            }
        }

        let state = if !code_files.is_empty() || Self::project_manifest(workspace_root).is_some() {
            RepoState::ExistingCodebase
        } else if !doc_files.is_empty() || has_other_files {
            RepoState::DocsOnly
        } else {
            RepoState::FreshWorkspace
        };

        (state, code_files, doc_files)
    }

    /// Extracts normalized technology, dependency, and framework tokens across all supported manifests.
    #[must_use]
    pub fn extract_stack_tokens(workspace_root: &Path) -> HashSet<String> {
        let mut tokens = HashSet::new();

        // 1. Node.js / Web (package.json)
        if let Ok(content) = fs::read_to_string(workspace_root.join("package.json")) {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                for dep_field in ["dependencies", "devDependencies", "peerDependencies"] {
                    if let Some(deps) = json.get(dep_field).and_then(|d| d.as_object()) {
                        for key in deps.keys() {
                            let lower = key.to_lowercase();
                            tokens.insert(lower.clone());
                            let clean = lower.trim_start_matches('@');
                            if let Some((scope, name)) = clean.split_once('/') {
                                tokens.insert(scope.to_string());
                                tokens.insert(name.to_string());
                                for part in name.split('-') {
                                    if !part.is_empty() {
                                        tokens.insert(part.to_string());
                                    }
                                }
                            } else {
                                for part in clean.split('-') {
                                    if !part.is_empty() {
                                        tokens.insert(part.to_string());
                                    }
                                }
                            }
                            if let Some(stem) = clean.strip_suffix("css") {
                                if stem.len() >= 3 {
                                    tokens.insert(stem.to_string());
                                }
                            }
                            if !clean.contains('/') && !clean.ends_with("js") {
                                tokens.insert(format!("{}js", clean));
                            }
                        }
                    }
                }
            }
        }

        // 2. Rust (Cargo.toml)
        if let Ok(content) = fs::read_to_string(workspace_root.join("Cargo.toml")) {
            if let Ok(toml_val) = toml::from_str::<toml::Value>(&content) {
                for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                    if let Some(deps) = toml_val.get(section).and_then(|d| d.as_table()) {
                        for key in deps.keys() {
                            let lower = key.to_lowercase();
                            tokens.insert(lower.clone());
                            for part in lower.split(['-', '_']) {
                                if !part.is_empty() {
                                    tokens.insert(part.to_string());
                                }
                            }
                        }
                    }
                }
                tokens.insert("rust".to_string());
                tokens.insert("cargo".to_string());
            }
        }

        // 3. Python (pyproject.toml / requirements.txt)
        if let Ok(content) = fs::read_to_string(workspace_root.join("pyproject.toml")) {
            tokens.insert("python".to_string());
            if let Ok(toml_val) = toml::from_str::<toml::Value>(&content) {
                if let Some(deps) = toml_val
                    .get("project")
                    .and_then(|p| p.get("dependencies"))
                    .and_then(|d| d.as_array())
                {
                    for dep in deps.iter().filter_map(|v| v.as_str()) {
                        insert_identifier_prefix(&mut tokens, dep);
                    }
                }
            }
        }
        if let Ok(content) = fs::read_to_string(workspace_root.join("requirements.txt")) {
            tokens.insert("python".to_string());
            for line in content.lines().map(str::trim) {
                if !line.is_empty() && !line.starts_with('#') {
                    insert_identifier_prefix(&mut tokens, line);
                }
            }
        }

        // 4. Flutter / Dart (pubspec.yaml)
        if let Ok(content) = fs::read_to_string(workspace_root.join("pubspec.yaml")) {
            tokens.insert("flutter".to_string());
            tokens.insert("dart".to_string());
            for line in content.lines().map(str::trim) {
                if let Some((k, _)) = line.split_once(':') {
                    let clean_k = k.trim().to_lowercase();
                    if !clean_k.is_empty() && !clean_k.starts_with('#') {
                        tokens.insert(clean_k);
                    }
                }
            }
        }

        // 5. Go (go.mod), TypeScript (tsconfig.json), MiniKit / onpkg manifests
        if let Ok(content) = fs::read_to_string(workspace_root.join("go.mod")) {
            tokens.insert("go".to_string());
            tokens.insert("golang".to_string());
            for line in content.lines().map(str::trim) {
                if let Some(seg) = line.split_whitespace().next() {
                    if seg.contains('/') {
                        if let Some(last) = seg.rsplit('/').next() {
                            tokens.insert(last.to_ascii_lowercase());
                        }
                    }
                }
            }
        }
        if workspace_root.join("tsconfig.json").exists() {
            tokens.insert("typescript".to_string());
            tokens.insert("ts".to_string());
        }
        for manifest_name in ["minikit.json", "onpkg.json"] {
            if let Ok(content) = fs::read_to_string(workspace_root.join(manifest_name)) {
                if let Ok(val) = serde_json::from_str::<Value>(&content) {
                    for field in ["stack", "runtime", "package_manager"] {
                        if let Some(s) = val.get(field).and_then(|v| v.as_str()) {
                            let lower = s.trim().to_lowercase();
                            if !lower.is_empty() {
                                tokens.insert(lower.clone());
                                for part in lower.split(['-', '_']) {
                                    if !part.is_empty() {
                                        tokens.insert(part.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 6. Infrastructure & framework config markers
        for (file_marker, token) in [
            ("Dockerfile", "docker"),
            ("docker-compose.yml", "docker"),
            ("compose.yaml", "docker"),
            ("compose.yml", "docker"),
            ("schema.prisma", "prisma"),
            ("vite.config.ts", "vite"),
            ("vite.config.js", "vite"),
            ("vite.config.mjs", "vite"),
            ("next.config.js", "nextjs"),
            ("next.config.mjs", "nextjs"),
            ("next.config.ts", "nextjs"),
            ("tailwind.config.js", "tailwind"),
            ("tailwind.config.ts", "tailwind"),
            ("tailwind.config.mjs", "tailwind"),
            ("playwright.config.ts", "playwright"),
            ("playwright.config.js", "playwright"),
        ] {
            if workspace_root.join(file_marker).exists() {
                tokens.insert(token.to_string());
            }
        }
        if workspace_root.join("prisma").join("schema.prisma").exists() {
            tokens.insert("prisma".to_string());
        }

        tokens
    }

    /// Formats a structured workspace summary for DAG/Swarm planners without duplicate manifest logic.
    #[must_use]
    pub fn describe_for_planner(workspace_root: &Path) -> String {
        let mut lines = vec!["### WORKSPACE CONTEXT:".to_string()];

        for manifest_name in ["onpkg.json", "minikit.json"] {
            if let Ok(content) = fs::read_to_string(workspace_root.join(manifest_name)) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    let name = json
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("project");
                    let pm = json
                        .get("package_manager")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let runtime = json
                        .get("runtime")
                        .or_else(|| json.get("stack"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    lines.push(format!("- Project Name: {}", name));
                    lines.push(format!("- Runtime / Ecosystem: {}", runtime));
                    lines.push(format!("- Package Manager: {}", pm));
                    break;
                }
            }
        }

        if let Some(manifest) = Self::project_manifest(workspace_root) {
            lines.push(format!("- Primary Manifest: `{}`", manifest));
        }

        let tokens = Self::extract_stack_tokens(workspace_root);
        if !tokens.is_empty() {
            let mut sorted: Vec<&str> = tokens
                .iter()
                .map(String::as_str)
                .filter(|t| t.len() >= 2 && !t.contains('/'))
                .collect();
            sorted.sort_unstable();
            sorted.truncate(14);
            if !sorted.is_empty() {
                lines.push(format!("- Detected Stack Tokens: {}", sorted.join(", ")));
            }
        }

        if let Ok(entries) = fs::read_dir(workspace_root) {
            let mut top_dirs = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.')
                    && !matches!(
                        name.as_str(),
                        "target" | "node_modules" | "venv" | "dist" | "build"
                    )
                {
                    top_dirs.push(name);
                }
            }
            top_dirs.sort_unstable();
            if !top_dirs.is_empty() {
                lines.push(format!("- Existing Entries: {}", top_dirs.join(", ")));
            }
        }

        lines.join("\n")
    }
}

fn insert_identifier_prefix(tokens: &mut HashSet<String>, raw: &str) {
    let name = raw
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .next()
        .unwrap_or_default();
    if !name.is_empty() {
        tokens.insert(name.to_lowercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntect_and_modern_classification() {
        assert!(WorkspaceProfile::is_code_path(Path::new("src/main.rs")));
        assert!(WorkspaceProfile::is_code_path(Path::new("app/page.tsx")));
        assert!(WorkspaceProfile::is_code_path(Path::new("server.py")));
        assert!(WorkspaceProfile::is_code_path(Path::new("index.html")));
        assert!(WorkspaceProfile::is_code_path(Path::new("schema.prisma")));

        assert!(!WorkspaceProfile::is_code_path(Path::new("README.md")));
        assert!(!WorkspaceProfile::is_code_path(Path::new("config.json")));
        assert!(WorkspaceProfile::is_doc_path(Path::new("README.md")));
        assert!(WorkspaceProfile::is_doc_path(Path::new("docs/spec.rst")));

        assert_eq!(
            WorkspaceProfile::language_label_for_path(Path::new("src/lib.rs")),
            "Rust"
        );
        assert_eq!(
            WorkspaceProfile::language_label_for_path(Path::new("Hero.tsx")),
            "TypeScript/React"
        );
        assert_eq!(
            WorkspaceProfile::language_label_for_path(Path::new("Cargo.toml")),
            "Config"
        );
    }
}
