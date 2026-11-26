//! Unified declarative grammar- and manifest-driven workspace profiler.
//!
//! Replaces scattered hardcoded extension arrays, heavyweight Sublime TextMate grammar bundles,
//! and duplicate manifest parsers across the orchestrator, swarm planner, vault skill matcher,
//! intent router, and reflexion loop with a zero-allocation declarative rule table + manifest engine.

use crate::agent::orchestrator::RepoState;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileSurface {
    Code,
    CodeUi,
    Doc,
    Config,
}

struct LanguageRule {
    extensions: &'static [&'static str],
    filenames: &'static [&'static str],
    label: &'static str,
    surface: FileSurface,
}

/// Zero-allocation declarative language & surface table.
const LANGUAGE_RULES: &[LanguageRule] = &[
    // Frontend / UI code & stylesheets
    LanguageRule {
        extensions: &["tsx"],
        filenames: &[],
        label: "TypeScript/React",
        surface: FileSurface::CodeUi,
    },
    LanguageRule {
        extensions: &["jsx"],
        filenames: &[],
        label: "JavaScript/React",
        surface: FileSurface::CodeUi,
    },
    LanguageRule {
        extensions: &["vue"],
        filenames: &[],
        label: "Vue",
        surface: FileSurface::CodeUi,
    },
    LanguageRule {
        extensions: &["svelte"],
        filenames: &[],
        label: "Svelte",
        surface: FileSurface::CodeUi,
    },
    LanguageRule {
        extensions: &["astro"],
        filenames: &[],
        label: "Astro",
        surface: FileSurface::CodeUi,
    },
    LanguageRule {
        extensions: &["html", "htm", "xhtml"],
        filenames: &[],
        label: "HTML",
        surface: FileSurface::CodeUi,
    },
    LanguageRule {
        extensions: &["css"],
        filenames: &[],
        label: "CSS",
        surface: FileSurface::CodeUi,
    },
    LanguageRule {
        extensions: &["scss", "sass", "less", "pcss"],
        filenames: &[],
        label: "CSS/Stylesheet",
        surface: FileSurface::CodeUi,
    },
    // Core systems, backend, scripting & schema languages
    LanguageRule {
        extensions: &["rs"],
        filenames: &[],
        label: "Rust",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["ts", "mts", "cts"],
        filenames: &[],
        label: "TypeScript",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["js", "mjs", "cjs"],
        filenames: &[],
        label: "JavaScript",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["py", "pyi", "pyw", "pyx"],
        filenames: &[],
        label: "Python",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["go"],
        filenames: &[],
        label: "Go",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["c", "h"],
        filenames: &[],
        label: "C",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["cc", "cpp", "cxx", "hpp", "hh", "hxx", "ino"],
        filenames: &[],
        label: "C++",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["cs", "csx"],
        filenames: &[],
        label: "C#",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["java"],
        filenames: &[],
        label: "Java",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["kt", "kts"],
        filenames: &[],
        label: "Kotlin",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["swift"],
        filenames: &[],
        label: "Swift",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["dart"],
        filenames: &[],
        label: "Dart",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["zig", "zon"],
        filenames: &[],
        label: "Zig",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["ex", "exs", "heex", "leex"],
        filenames: &[],
        label: "Elixir",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["erl", "hrl"],
        filenames: &[],
        label: "Erlang",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["rb", "rake", "gemspec"],
        filenames: &["Gemfile", "Rakefile"],
        label: "Ruby",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["php", "phtml"],
        filenames: &[],
        label: "PHP",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["lua", "luau"],
        filenames: &[],
        label: "Lua",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["r", "rmd"],
        filenames: &[],
        label: "R",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["jl"],
        filenames: &[],
        label: "Julia",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["hs", "lhs"],
        filenames: &[],
        label: "Haskell",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["scala", "sc", "sbt"],
        filenames: &[],
        label: "Scala",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["clj", "cljs", "cljc", "edn"],
        filenames: &[],
        label: "Clojure",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["ml", "mli", "rei", "res"],
        filenames: &[],
        label: "OCaml",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["gleam"],
        filenames: &[],
        label: "Gleam",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["nim", "nims"],
        filenames: &[],
        label: "Nim",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["pl", "pm"],
        filenames: &[],
        label: "Perl",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["sh", "bash", "zsh", "fish", "ksh"],
        filenames: &[],
        label: "Shell",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["ps1", "psm1", "psd1"],
        filenames: &[],
        label: "PowerShell",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["bat", "cmd"],
        filenames: &[],
        label: "Batch",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["sql", "psql", "plsql"],
        filenames: &[],
        label: "SQL",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["prisma"],
        filenames: &[],
        label: "Prisma",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["proto"],
        filenames: &[],
        label: "Protobuf",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["graphql", "gql"],
        filenames: &[],
        label: "GraphQL",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["sol", "vy"],
        filenames: &[],
        label: "Solidity",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["tf", "tfvars", "hcl"],
        filenames: &[],
        label: "Terraform",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["nix"],
        filenames: &[],
        label: "Nix",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["wgsl", "glsl", "vert", "frag", "hlsl", "metal"],
        filenames: &[],
        label: "Shader",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["asm", "s"],
        filenames: &[],
        label: "Assembly",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["groovy", "gradle"],
        filenames: &[],
        label: "Groovy",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["fs", "fsi", "fsx"],
        filenames: &[],
        label: "F#",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["mk", "mak"],
        filenames: &["Makefile", "GNUmakefile", "Justfile"],
        label: "Makefile",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["cmake"],
        filenames: &["CMakeLists.txt"],
        label: "CMake",
        surface: FileSurface::Code,
    },
    LanguageRule {
        extensions: &["dockerfile", "containerfile"],
        filenames: &["Dockerfile", "Containerfile"],
        label: "Dockerfile",
        surface: FileSurface::Code,
    },
    // Documentation & prose formats
    LanguageRule {
        extensions: &["md", "markdown", "mdown", "mdx"],
        filenames: &[],
        label: "Markdown",
        surface: FileSurface::Doc,
    },
    LanguageRule {
        extensions: &["rst"],
        filenames: &[],
        label: "reStructuredText",
        surface: FileSurface::Doc,
    },
    LanguageRule {
        extensions: &["adoc", "asciidoc"],
        filenames: &[],
        label: "AsciiDoc",
        surface: FileSurface::Doc,
    },
    LanguageRule {
        extensions: &["tex", "bib", "sty", "cls"],
        filenames: &[],
        label: "LaTeX",
        surface: FileSurface::Doc,
    },
    LanguageRule {
        extensions: &["org"],
        filenames: &[],
        label: "Org",
        surface: FileSurface::Doc,
    },
    LanguageRule {
        extensions: &["txt", "text"],
        filenames: &[],
        label: "General",
        surface: FileSurface::Doc,
    },
    // Structured configuration & data formats
    LanguageRule {
        extensions: &[
            "toml",
            "json",
            "jsonc",
            "json5",
            "yaml",
            "yml",
            "ini",
            "cfg",
            "conf",
            "lock",
            "xml",
            "svg",
            "properties",
        ],
        filenames: &[],
        label: "Config",
        surface: FileSurface::Config,
    },
];

#[inline]
fn classify_path(path: &Path) -> Option<&'static LanguageRule> {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        for rule in LANGUAGE_RULES {
            if rule
                .filenames
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
            {
                return Some(rule);
            }
        }
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        for rule in LANGUAGE_RULES {
            if rule
                .extensions
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
            {
                return Some(rule);
            }
        }
    }
    None
}

/// Unified workspace inspection and file-classification engine.
pub struct WorkspaceProfile;

impl WorkspaceProfile {
    /// Classifies whether a path represents executable/compilable source code or UI markup.
    #[must_use]
    pub fn is_code_path(path: &Path) -> bool {
        matches!(
            classify_path(path).map(|r| r.surface),
            Some(FileSurface::Code | FileSurface::CodeUi)
        )
    }

    /// Classifies whether a path represents prose/documentation.
    #[must_use]
    pub fn is_doc_path(path: &Path) -> bool {
        matches!(
            classify_path(path).map(|r| r.surface),
            Some(FileSurface::Doc)
        )
    }

    /// Classifies whether a path represents a frontend/UI surface file.
    #[must_use]
    pub fn is_ui_path(path: &Path) -> bool {
        matches!(
            classify_path(path).map(|r| r.surface),
            Some(FileSurface::CodeUi)
        )
    }

    /// Resolves a clean human-readable language/domain label for any file path.
    #[must_use]
    pub fn language_label_for_path(path: &Path) -> String {
        classify_path(path)
            .map_or("General", |r| r.label)
            .to_string()
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
                            insert_package_specifier_tokens(&mut tokens, key);
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
                    for pkg_array_field in ["packages", "dev_packages"] {
                        if let Some(arr) = val.get(pkg_array_field).and_then(|v| v.as_array()) {
                            for pkg in arr.iter().filter_map(|v| v.as_str()) {
                                insert_package_specifier_tokens(&mut tokens, pkg);
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

        expand_canonical_ecosystem_aliases(&mut tokens);
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

    /// Resolves the declarative [`CommentSyntax`] profile for `path` (used by the `tokei`-inspired
    /// byte-level FSM line counter without regexes or ad-hoc extension checks).
    #[must_use]
    pub fn comment_syntax_for_path(path: &Path) -> CommentSyntax {
        let label = Self::language_label_for_path(path);
        const SYNTAX_BY_LABEL: &[(&[&str], CommentSyntax)] = &[
            (
                &["Rust", "Swift", "Kotlin", "Scala", "Zig"],
                SYNTAX_NESTED_C,
            ),
            (
                &[
                    "TypeScript/React",
                    "JavaScript/React",
                    "TypeScript",
                    "JavaScript",
                    "Go",
                    "C",
                    "C++",
                    "C#",
                    "Java",
                    "Dart",
                    "PHP",
                    "CSS",
                    "CSS/Stylesheet",
                    "Solidity",
                    "Protobuf",
                    "Shader",
                    "Groovy",
                    "Prisma",
                    "Gleam",
                ],
                SYNTAX_C_STYLE,
            ),
            (
                &[
                    "Python",
                    "Ruby",
                    "Shell",
                    "YAML",
                    "TOML",
                    "Makefile",
                    "CMake",
                    "Dockerfile",
                    "Nix",
                    "Perl",
                    "Nim",
                    "Elixir",
                    "GraphQL",
                    "Terraform",
                    "PowerShell",
                ],
                SYNTAX_HASH,
            ),
            (&["SQL", "Lua", "Haskell"], SYNTAX_DASH_LINE),
            (&["HTML", "Vue", "Svelte", "Astro", "XML"], SYNTAX_HTML),
            (&["OCaml", "F#"], SYNTAX_OCAML),
        ];

        for &(labels, syntax) in SYNTAX_BY_LABEL {
            if labels.contains(&label.as_str()) {
                return syntax;
            }
        }
        SYNTAX_NONE
    }

    /// Computes exact per-language Code / Comments / Blanks LOC metrics across `workspace_root`
    /// using a pure byte-level finite state machine (`XAMPPRocky/tokei` algorithm, zero regexes).
    #[must_use]
    pub fn compute_code_stats(workspace_root: &Path) -> CodeStatsReport {
        use std::collections::HashMap;
        let rel_files =
            crate::context::walker::WorkspaceWalker::new(workspace_root).collect_relative_files();
        let mut by_lang: HashMap<String, LanguageLocStats> = HashMap::new();

        for rel in rel_files {
            let full = workspace_root.join(&rel);
            let label = Self::language_label_for_path(&full);
            if matches!(label.as_str(), "General" | "Config") && !Self::is_code_path(&full) {
                continue;
            }
            let Ok(meta) = fs::metadata(&full) else {
                continue;
            };
            if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
                continue;
            }
            let Ok(bytes) = fs::read(&full) else {
                continue;
            };
            // Skip binary files quickly via leading NUL-byte sniff
            if bytes[..bytes.len().min(4096)].contains(&0) {
                continue;
            }
            let syntax = Self::comment_syntax_for_path(&full);
            let stats = count_bytes_tokei_fsm(&bytes, syntax);
            let entry = by_lang
                .entry(label.clone())
                .or_insert_with(|| LanguageLocStats {
                    language: label,
                    files: 0,
                    code: 0,
                    comments: 0,
                    blanks: 0,
                });
            entry.files += 1;
            entry.code += stats.code;
            entry.comments += stats.comments;
            entry.blanks += stats.blanks;
        }

        let mut languages: Vec<LanguageLocStats> = by_lang.into_values().collect();
        languages.sort_by(|a, b| {
            b.code
                .cmp(&a.code)
                .then_with(|| a.language.cmp(&b.language))
        });

        let total_files = languages.iter().map(|l| l.files).sum();
        let total_code = languages.iter().map(|l| l.code).sum();
        let total_comments = languages.iter().map(|l| l.comments).sum();
        let total_blanks = languages.iter().map(|l| l.blanks).sum();

        CodeStatsReport {
            languages,
            total_files,
            total_code,
            total_comments,
            total_blanks,
        }
    }
}

/// Declarative lexical comment & string syntax specification for the byte-level LOC FSM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentSyntax {
    pub line_comment: &'static str,
    pub block_open: &'static str,
    pub block_close: &'static str,
    pub nested_blocks: bool,
    pub string_quotes: &'static [u8],
    pub has_backslash_escape: bool,
}

const SYNTAX_NESTED_C: CommentSyntax = CommentSyntax {
    line_comment: "//",
    block_open: "/*",
    block_close: "*/",
    nested_blocks: true,
    string_quotes: b"\"",
    has_backslash_escape: true,
};

const SYNTAX_C_STYLE: CommentSyntax = CommentSyntax {
    line_comment: "//",
    block_open: "/*",
    block_close: "*/",
    nested_blocks: false,
    string_quotes: b"\"'`",
    has_backslash_escape: true,
};

const SYNTAX_HASH: CommentSyntax = CommentSyntax {
    line_comment: "#",
    block_open: "",
    block_close: "",
    nested_blocks: false,
    string_quotes: b"\"'",
    has_backslash_escape: true,
};

const SYNTAX_DASH_LINE: CommentSyntax = CommentSyntax {
    line_comment: "--",
    block_open: "/*",
    block_close: "*/",
    nested_blocks: false,
    string_quotes: b"'\"",
    has_backslash_escape: true,
};

const SYNTAX_HTML: CommentSyntax = CommentSyntax {
    line_comment: "",
    block_open: "<!--",
    block_close: "-->",
    nested_blocks: false,
    string_quotes: b"\"'",
    has_backslash_escape: false,
};

const SYNTAX_OCAML: CommentSyntax = CommentSyntax {
    line_comment: "",
    block_open: "(*",
    block_close: "*)",
    nested_blocks: true,
    string_quotes: b"\"",
    has_backslash_escape: true,
};

const SYNTAX_NONE: CommentSyntax = CommentSyntax {
    line_comment: "",
    block_open: "",
    block_close: "",
    nested_blocks: false,
    string_quotes: b"",
    has_backslash_escape: false,
};

/// Exact per-file line counts produced by [`count_bytes_tokei_fsm`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileLineStats {
    pub code: usize,
    pub comments: usize,
    pub blanks: usize,
}

/// Aggregated per-language LOC breakdown (`XAMPPRocky/tokei` format).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LanguageLocStats {
    pub language: String,
    pub files: usize,
    pub code: usize,
    pub comments: usize,
    pub blanks: usize,
}

/// Full workspace code/comment/blank line-count report.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CodeStatsReport {
    pub languages: Vec<LanguageLocStats>,
    pub total_files: usize,
    pub total_code: usize,
    pub total_comments: usize,
    pub total_blanks: usize,
}

impl CodeStatsReport {
    pub fn format_markdown(&self) -> String {
        if self.languages.is_empty() {
            return String::new();
        }
        let mut out = String::from(
            "### 📏 Codebase Language & LOC Breakdown (Byte-FSM)\n\n\
             | Language | Files | Code | Comments | Blanks | Comment % |\n\
             | :--- | ---: | ---: | ---: | ---: | ---: |\n",
        );
        for lang in self.languages.iter().take(15) {
            let denom = lang.code + lang.comments;
            let pct = if denom > 0 {
                (lang.comments as f64 / denom as f64) * 100.0
            } else {
                0.0
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {:.1}% |\n",
                lang.language, lang.files, lang.code, lang.comments, lang.blanks, pct
            ));
        }
        let total_denom = self.total_code + self.total_comments;
        let total_pct = if total_denom > 0 {
            (self.total_comments as f64 / total_denom as f64) * 100.0
        } else {
            0.0
        };
        out.push_str(&format!(
            "| **Total** | **{}** | **{}** | **{}** | **{}** | **{:.1}%** |\n\n",
            self.total_files, self.total_code, self.total_comments, self.total_blanks, total_pct
        ));
        out
    }
}

/// Deterministic single-pass byte-level finite state machine for counting Code, Comment, and
/// Blank lines (`XAMPPRocky/tokei` algorithm, zero regexes).
///
/// Correctly handles nested block comments (e.g. Rust `/* /* */ */`), comment markers inside
/// string literals (`"https://..."`), escaped quotes (`\"`), and mixed code + trailing comments.
#[must_use]
pub fn count_bytes_tokei_fsm(bytes: &[u8], syntax: CommentSyntax) -> FileLineStats {
    if bytes.is_empty() {
        return FileLineStats::default();
    }

    let line_c = syntax.line_comment.as_bytes();
    let blk_open = syntax.block_open.as_bytes();
    let blk_close = syntax.block_close.as_bytes();

    let mut stats = FileLineStats::default();
    let mut block_depth: usize = 0;
    let mut in_string: Option<u8> = None;

    for raw_line in bytes.split(|&b| b == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        let len = line.len();
        let mut i = 0usize;
        let mut has_code = false;
        let mut has_comment = false;

        // Skip leading whitespace
        while i < len && line[i].is_ascii_whitespace() {
            i += 1;
        }
        if i == len {
            stats.blanks += 1;
            continue;
        }

        while i < len {
            let b = line[i];

            // Inside string literal
            if let Some(quote) = in_string {
                has_code = true;
                if syntax.has_backslash_escape && b == b'\\' && i + 1 < len {
                    i += 2;
                    continue;
                }
                if b == quote {
                    in_string = None;
                }
                i += 1;
                continue;
            }

            // Inside block comment
            if block_depth > 0 {
                has_comment = true;
                if syntax.nested_blocks && !blk_open.is_empty() && line[i..].starts_with(blk_open) {
                    block_depth += 1;
                    i += blk_open.len();
                    continue;
                }
                if !blk_close.is_empty() && line[i..].starts_with(blk_close) {
                    block_depth -= 1;
                    i += blk_close.len();
                    continue;
                }
                i += 1;
                continue;
            }

            // Normal code state
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if !line_c.is_empty() && line[i..].starts_with(line_c) {
                has_comment = true;
                break; // Remainder of line is a line comment
            }
            if !blk_open.is_empty() && line[i..].starts_with(blk_open) {
                has_comment = true;
                block_depth = 1;
                i += blk_open.len();
                continue;
            }
            if syntax.string_quotes.contains(&b) {
                has_code = true;
                in_string = Some(b);
                i += 1;
                continue;
            }

            has_code = true;
            i += 1;
        }

        // Standard `tokei` classification rule: a line with ANY code counts as code (even with trailing comment)
        if has_code {
            stats.code += 1;
        } else if has_comment {
            stats.comments += 1;
        } else {
            stats.blanks += 1;
        }
    }

    // Avoid counting a phantom blank line caused by a trailing `\n` at EOF
    if bytes.ends_with(b"\n") && stats.blanks > 0 {
        stats.blanks -= 1;
    }

    stats
}

pub(crate) fn insert_package_specifier_tokens(tokens: &mut HashSet<String>, raw_spec: &str) {
    let lower = raw_spec.trim().to_lowercase();
    if lower.is_empty() {
        return;
    }
    tokens.insert(lower.clone());
    let clean = lower.trim_start_matches('@');
    let push_stem_rules = |seg: &str, out: &mut HashSet<String>| {
        if let Some(stem) = seg.strip_suffix("css") {
            if stem.len() >= 5 {
                out.insert(stem.to_string());
            }
        }
        if !seg.is_empty() && !seg.ends_with("js") {
            out.insert(format!("{seg}js"));
        }
    };
    if let Some((scope, name)) = clean.split_once('/') {
        tokens.insert(scope.to_string());
        tokens.insert(name.to_string());
        push_stem_rules(scope, tokens);
        push_stem_rules(name, tokens);
        for part in name.split('-') {
            if !part.is_empty() {
                tokens.insert(part.to_string());
            }
        }
    } else {
        push_stem_rules(clean, tokens);
        for part in clean.split('-') {
            if !part.is_empty() {
                tokens.insert(part.to_string());
            }
        }
    }
}

fn expand_canonical_ecosystem_aliases(tokens: &mut HashSet<String>) {
    const CANONICAL_PACKAGE_ALIASES: &[(&[&str], &str)] = &[
        (
            &["pg", "postgresql", "psycopg", "psycopg2", "asyncpg"],
            "postgres",
        ),
        (&["mongoose", "pymongo"], "mongodb"),
        (&["tailwindcss"], "tailwind"),
        (&["next"], "nextjs"),
    ];
    for (variants, canonical) in CANONICAL_PACKAGE_ALIASES {
        if variants.iter().any(|v| tokens.contains(*v)) {
            tokens.insert((*canonical).to_string());
        }
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
        assert!(WorkspaceProfile::is_code_path(Path::new("CMakeLists.txt")));
        assert!(WorkspaceProfile::is_code_path(Path::new("Dockerfile")));
        assert!(WorkspaceProfile::is_code_path(Path::new("Makefile")));
        assert!(WorkspaceProfile::is_ui_path(Path::new("app/page.tsx")));
        assert!(WorkspaceProfile::is_ui_path(Path::new("styles/main.css")));

        assert!(!WorkspaceProfile::is_code_path(Path::new("README.md")));
        assert!(!WorkspaceProfile::is_code_path(Path::new("config.json")));
        assert!(WorkspaceProfile::is_doc_path(Path::new("README.md")));
        assert!(WorkspaceProfile::is_doc_path(Path::new("docs/spec.rst")));
        assert!(WorkspaceProfile::is_doc_path(Path::new("notes.txt")));

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
        assert_eq!(
            WorkspaceProfile::language_label_for_path(Path::new("CMakeLists.txt")),
            "CMake"
        );
        assert_eq!(
            WorkspaceProfile::language_label_for_path(Path::new("notes.txt")),
            "General"
        );
    }

    #[test]
    fn test_tokei_fsm_handles_nested_block_comments_strings_and_mixed_lines() {
        let rust_src = br#"// Leading line comment
/* Outer comment
   /* Nested Rust block comment */
   still inside outer comment!
*/
pub fn fetch() -> &'static str {
    let url = "https://example.com /* not a comment */"; // trailing comment on code line

    url
}
"#;
        let stats = count_bytes_tokei_fsm(rust_src, SYNTAX_NESTED_C);
        assert_eq!(stats.comments, 5, "1 line comment + 4 block comment lines");
        assert_eq!(
            stats.code, 4,
            "fn signature, url literal with // and /*, url expr, closing brace"
        );
        assert_eq!(stats.blanks, 1, "single blank line inside function");
    }
}
