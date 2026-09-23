use crate::error::{Result, ToolError};
use crate::tools::minikit::stacks::StackFile;
use crate::tools::minikit::sync::MiniKitSyncEngine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};
use std::time::SystemTime;

/// Represents a modular, composable architecture recipe or component block.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ArchitectureBlock {
    pub name: String,
    pub description: String,
    pub category: String,
    pub supported_runtimes: Vec<String>,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub dev_packages: Vec<String>,
    pub files: Vec<StackFile>,
}

#[derive(Clone, Debug)]
struct DirectoryBlockCache {
    last_mtime: SystemTime,
    blocks: Vec<ArchitectureBlock>,
}

static BLOCK_DIR_CACHE: LazyLock<RwLock<HashMap<PathBuf, DirectoryBlockCache>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Native manager for modular architecture blocks (Docker, CI, Tailwind, Healthcheck, Gitignore).
pub struct MiniKitBlocksManager;

#[allow(dead_code)]
pub type OnpkgBlocksManager = MiniKitBlocksManager;

impl MiniKitBlocksManager {
    /// Clears the directory block cache.
    #[allow(dead_code)]
    pub fn clear_cache() {
        if let Ok(mut cache) = BLOCK_DIR_CACHE.write() {
            cache.clear();
        }
    }

    /// Generates built-in dynamic blocks customized to the project's detected runtime.
    pub fn get_builtin_blocks(workspace_root: &Path) -> Vec<ArchitectureBlock> {
        let (runtime, _) = MiniKitSyncEngine::detect_runtime(workspace_root);
        let rt = runtime;

        vec![
            Self::generate_docker_block(rt),
            Self::generate_github_ci_block(rt),
            Self::generate_gitignore_block(rt),
            Self::generate_editorconfig_block(),
            Self::generate_healthcheck_block(rt),
            Self::generate_tailwind_block(rt),
        ]
    }

    /// Returns all available architecture blocks (built-in + workspace custom + user global).
    pub fn get_all_blocks(workspace_root: &Path) -> Vec<ArchitectureBlock> {
        let mut blocks = Self::get_builtin_blocks(workspace_root);

        let mut search_dirs = Vec::new();
        search_dirs.push(workspace_root.join(".minicode").join("blocks"));
        search_dirs.push(workspace_root.join(".minikit").join("blocks"));

        if let Ok(cwd) = std::env::current_dir() {
            let s1 = cwd.join(".minicode").join("blocks");
            let s2 = cwd.join(".minikit").join("blocks");
            if !search_dirs.contains(&s1) {
                search_dirs.push(s1);
            }
            if !search_dirs.contains(&s2) {
                search_dirs.push(s2);
            }
        }

        if let Some(home) = dirs::home_dir() {
            let s1 = home.join(".config").join("minicode").join("blocks");
            let s2 = home.join(".minikit").join("blocks");
            let s3 = home.join(".onpkg").join("blocks");
            if !search_dirs.contains(&s1) {
                search_dirs.push(s1);
            }
            if !search_dirs.contains(&s2) {
                search_dirs.push(s2);
            }
            if !search_dirs.contains(&s3) {
                search_dirs.push(s3);
            }
        }

        for dir in search_dirs {
            let meta = match fs::metadata(&dir) {
                Ok(m) if m.is_dir() => m,
                _ => {
                    if let Ok(mut cache) = BLOCK_DIR_CACHE.write() {
                        cache.remove(&dir);
                    }
                    continue;
                }
            };

            let current_mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

            let cached_blocks = {
                let cache = BLOCK_DIR_CACHE.read().ok();
                cache.and_then(|c| {
                    c.get(&dir).and_then(|entry| {
                        if entry.last_mtime == current_mtime {
                            Some(entry.blocks.clone())
                        } else {
                            None
                        }
                    })
                })
            };

            let dir_blocks = if let Some(hit) = cached_blocks {
                hit
            } else {
                let mut loaded = Vec::new();
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(block) =
                                    serde_json::from_str::<ArchitectureBlock>(&content)
                                {
                                    loaded.push(block);
                                }
                            }
                        }
                    }
                }
                if let Ok(mut cache) = BLOCK_DIR_CACHE.write() {
                    cache.insert(
                        dir.clone(),
                        DirectoryBlockCache {
                            last_mtime: current_mtime,
                            blocks: loaded.clone(),
                        },
                    );
                }
                loaded
            };

            for block in dir_blocks {
                if !blocks
                    .iter()
                    .any(|b| b.name.eq_ignore_ascii_case(&block.name))
                {
                    blocks.push(block);
                }
            }
        }

        blocks
    }

    /// Finds a specific block by name.
    pub fn find_block(workspace_root: &Path, name: &str) -> Option<ArchitectureBlock> {
        let norm = name.trim().to_lowercase();
        Self::get_all_blocks(workspace_root)
            .into_iter()
            .find(|b| b.name.eq_ignore_ascii_case(&norm))
    }

    /// Injects an architecture block into the workspace.
    pub fn add_block(workspace_root: &Path, name: &str, force: bool) -> Result<String> {
        let block = Self::find_block(workspace_root, name).ok_or_else(|| {
            let available: Vec<String> = Self::get_all_blocks(workspace_root)
                .into_iter()
                .map(|b| b.name)
                .collect();
            ToolError::InvalidArguments {
                name: "kit_block_add".to_string(),
                reason: format!(
                    "Architecture block `{}` not found. Available blocks: {}",
                    name,
                    available.join(", ")
                ),
            }
        })?;

        // 1. Conflict detection if force is not set
        if !force {
            let mut existing = Vec::new();
            for f in &block.files {
                let target = workspace_root.join(&f.path);
                if target.exists() {
                    existing.push(f.path.clone());
                }
            }
            if !existing.is_empty() {
                return Err(ToolError::InvalidArguments {
                    name: "kit_block_add".to_string(),
                    reason: format!(
                        "Block `{}` conflicts with existing files in workspace: {}. Use --force to overwrite.",
                        block.name,
                        existing.join(", ")
                    ),
                }
                .into());
            }
        }

        // 2. Write block files
        let mut written_files = Vec::new();
        for f in &block.files {
            let target = workspace_root.join(&f.path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
                    path: parent.display().to_string(),
                    source: e,
                })?;
            }
            if let Some(bin) = &f.binary_content {
                fs::write(&target, bin).map_err(|e| ToolError::FileOp {
                    path: target.display().to_string(),
                    source: e,
                })?;
            } else {
                fs::write(&target, &f.content).map_err(|e| ToolError::FileOp {
                    path: target.display().to_string(),
                    source: e,
                })?;
            }
            written_files.push(f.path.clone());
        }

        let mut out = format!(
            "✔ Successfully added architecture block `{}` ({})\n",
            block.name, block.category
        );
        out.push_str("📁 Files generated:\n");
        for wf in &written_files {
            out.push_str(&format!("  ├── {}\n", wf));
        }

        if !block.packages.is_empty() || !block.dev_packages.is_empty() {
            out.push_str("\n💡 Suggested dependencies to install:\n");
            if !block.packages.is_empty() {
                out.push_str(&format!("  • Packages: {}\n", block.packages.join(", ")));
            }
            if !block.dev_packages.is_empty() {
                out.push_str(&format!(
                    "  • Dev Packages: {}\n",
                    block.dev_packages.join(", ")
                ));
            }
        }

        Ok(out)
    }

    /// Formats a list of all available blocks for CLI / TUI display.
    pub fn format_list(workspace_root: &Path) -> String {
        let blocks = Self::get_all_blocks(workspace_root);
        let mut out = format!(
            "\n🧱 **Available Architecture Blocks ({})**\n\n",
            blocks.len()
        );

        for b in &blocks {
            out.push_str(&format!(
                "  • \x1b[1m\x1b[38;2;162;119;255m{:<16}\x1b[0m [{}] ({} files) — {}\n",
                b.name,
                b.category,
                b.files.len(),
                b.description
            ));
        }
        out.push_str("\n💡 Run `minicode block add <name>` or `/kit block add <name>` to inject into project.\n");
        out
    }

    // ── Built-in Block Generators ───────────────────────────────────────────

    fn generate_docker_block(runtime: &str) -> ArchitectureBlock {
        let (dockerfile, dockerignore, compose) = match runtime {
            "rust" | "cargo" => (
                r#"# Multi-stage Rust build with cargo-chef
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/* /usr/local/bin/
USER 10001:10001
CMD ["app"]
"#,
                "target/\n.git/\n.minicode/\n.minikit/\n*.log\n",
                r#"services:
  app:
    build: .
    restart: unless-stopped
    ports:
      - "8080:8080"
"#,
            ),
            "python" | "uv" | "pip" => (
                r#"FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim AS builder
ENV UV_COMPILE_BYTECODE=1 UV_LINK_MODE=copy
WORKDIR /app
COPY pyproject.toml requirements.txt* ./
RUN uv pip install -r requirements.txt --system 2>/dev/null || true
COPY . /app

FROM python:3.12-slim-bookworm
WORKDIR /app
COPY --from=builder /app /app
USER 10001:10001
EXPOSE 8000
CMD ["python", "-m", "app.main"]
"#,
                "__pycache__/\n*.pyc\n.venv/\n.git/\n.minicode/\n.pytest_cache/\n",
                r#"services:
  api:
    build: .
    restart: unless-stopped
    ports:
      - "8000:8000"
"#,
            ),
            "go" | "golang" => (
                r#"FROM golang:1.23-alpine AS builder
WORKDIR /app
COPY go.mod go.sum* ./
RUN go mod download 2>/dev/null || true
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -o /app/server .

FROM alpine:latest
WORKDIR /app
RUN apk --no-cache add ca-certificates
COPY --from=builder /app/server .
EXPOSE 8080
CMD ["./server"]
"#,
                ".git/\n.minicode/\nbin/\n*.exe\n",
                r#"services:
  app:
    build: .
    restart: unless-stopped
    ports:
      - "8080:8080"
"#,
            ),
            _ => (
                r#"FROM oven/bun:1-alpine AS base
WORKDIR /app

FROM base AS install
COPY package.json bun.lockb* bun.lock* package-lock.json* ./
RUN bun install 2>/dev/null || npm install

FROM base AS release
COPY --from=install /app/node_modules node_modules
COPY . .
EXPOSE 3000
CMD ["bun", "run", "start"]
"#,
                "node_modules/\n.git/\n.minicode/\ndist/\n.next/\n*.log\n",
                r#"services:
  web:
    build: .
    restart: unless-stopped
    ports:
      - "3000:3000"
    environment:
      - NODE_ENV=production
"#,
            ),
        };

        ArchitectureBlock {
            name: "docker".to_string(),
            description: format!(
                "Multi-stage containerization setup tailored for {}",
                runtime
            ),
            category: "devops".to_string(),
            supported_runtimes: vec!["any".to_string()],
            packages: vec![],
            dev_packages: vec![],
            files: vec![
                StackFile {
                    path: "Dockerfile".to_string(),
                    content: dockerfile.to_string(),
                    binary_content: None,
                },
                StackFile {
                    path: ".dockerignore".to_string(),
                    content: dockerignore.to_string(),
                    binary_content: None,
                },
                StackFile {
                    path: "docker-compose.yml".to_string(),
                    content: compose.to_string(),
                    binary_content: None,
                },
            ],
        }
    }

    fn generate_github_ci_block(runtime: &str) -> ArchitectureBlock {
        let ci_yaml = match runtime {
            "rust" | "cargo" => {
                r#"name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    name: Build & Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - name: Check Formatting
        run: cargo fmt --all -- --check
      - name: Lint with Clippy
        run: cargo clippy -- -D warnings
      - name: Run Tests
        run: cargo test --all-targets
"#
            }
            "python" | "uv" | "pip" => {
                r#"name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    name: Test & Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v3
        with:
          enable-cache: true
      - name: Set up Python
        run: uv python install 3.12
      - name: Run Tests
        run: uv run pytest || pytest
"#
            }
            "go" | "golang" => {
                r#"name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    name: Test & Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version: '1.23'
          cache: true
      - name: Build
        run: go build -v ./...
      - name: Test
        run: go test -v ./...
"#
            }
            _ => {
                r#"name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    name: Test & Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
        with:
          bun-version: latest
      - name: Install Dependencies
        run: bun install
      - name: Run Tests
        run: bun test || npm test --if-present
"#
            }
        };

        ArchitectureBlock {
            name: "github-ci".to_string(),
            description: format!(
                "GitHub Actions automated CI pipeline tailored for {}",
                runtime
            ),
            category: "ci".to_string(),
            supported_runtimes: vec!["any".to_string()],
            packages: vec![],
            dev_packages: vec![],
            files: vec![StackFile {
                path: ".github/workflows/ci.yml".to_string(),
                content: ci_yaml.to_string(),
                binary_content: None,
            }],
        }
    }

    fn generate_gitignore_block(runtime: &str) -> ArchitectureBlock {
        let mut content = String::from(
            r#"# Operating System
.DS_Store
Thumbs.db

# IDE & Editor
.idea/
.vscode/
*.swp
*.swo

# minicode & Local Agents
.minicode/backups/
.minicode/runtime/
.minicode/sessions/
*.log
"#,
        );

        match runtime {
            "rust" | "cargo" => {
                content.push_str("\n# Rust / Cargo\ntarget/\nCargo.lock\n");
            }
            "python" | "uv" | "pip" => {
                content.push_str("\n# Python\n__pycache__/\n*.py[cod]\n.venv/\nvenv/\n.pytest_cache/\n.ruff_cache/\n");
            }
            "go" | "golang" => {
                content.push_str("\n# Go\nbin/\n*.exe\n*.test\nvendor/\n");
            }
            _ => {
                content.push_str("\n# Node / Web\nnode_modules/\ndist/\nbuild/\n.next/\n.turbo/\n");
            }
        }

        ArchitectureBlock {
            name: "gitignore".to_string(),
            description: format!("Comprehensive .gitignore tailored for {}", runtime),
            category: "config".to_string(),
            supported_runtimes: vec!["any".to_string()],
            packages: vec![],
            dev_packages: vec![],
            files: vec![StackFile {
                path: ".gitignore".to_string(),
                content,
                binary_content: None,
            }],
        }
    }

    fn generate_editorconfig_block() -> ArchitectureBlock {
        let content = r#"root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
indent_style = space
indent_size = 2

[*.rs]
indent_size = 4

[*.py]
indent_size = 4

[*.go]
indent_style = tab

[*.md]
trim_trailing_whitespace = false
"#;

        ArchitectureBlock {
            name: "editorconfig".to_string(),
            description: "Cross-editor code formatting guidelines (.editorconfig)".to_string(),
            category: "config".to_string(),
            supported_runtimes: vec!["any".to_string()],
            packages: vec![],
            dev_packages: vec![],
            files: vec![StackFile {
                path: ".editorconfig".to_string(),
                content: content.to_string(),
                binary_content: None,
            }],
        }
    }

    fn generate_healthcheck_block(runtime: &str) -> ArchitectureBlock {
        let (file_path, content) = match runtime {
            "rust" | "cargo" => (
                "src/health.rs",
                r##"use std::time::Instant;

pub struct HealthProbe {
    started_at: Instant,
}

impl Default for HealthProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthProbe {
    pub fn new() -> Self {
        Self { started_at: Instant::now() }
    }

    pub fn status_json(&self) -> String {
        format!(
            r#"{{"status":"ok","uptime_secs":{}}}"#,
            self.started_at.elapsed().as_secs()
        )
    }
}
"##,
            ),
            "python" | "uv" | "pip" => (
                "app/health.py",
                r#"from time import time

_start_time = time()

def get_health_status():
    return {
        "status": "ok",
        "uptime_secs": round(time() - _start_time, 2)
    }
"#,
            ),
            _ => (
                "src/health.ts",
                r#"const startTime = Date.now();

export function getHealthStatus() {
  return {
    status: "ok",
    uptime_secs: Math.floor((Date.now() - startTime) / 1000),
    timestamp: new Date().toISOString(),
  };
}
"#,
            ),
        };

        ArchitectureBlock {
            name: "healthcheck".to_string(),
            description: "Standard liveness and uptime health probe endpoint".to_string(),
            category: "observability".to_string(),
            supported_runtimes: vec!["any".to_string()],
            packages: vec![],
            dev_packages: vec![],
            files: vec![StackFile {
                path: file_path.to_string(),
                content: content.to_string(),
                binary_content: None,
            }],
        }
    }

    fn generate_tailwind_block(_runtime: &str) -> ArchitectureBlock {
        ArchitectureBlock {
            name: "tailwind".to_string(),
            description: "Tailwind CSS v4 modern configuration and stylesheet entrypoint"
                .to_string(),
            category: "ui".to_string(),
            supported_runtimes: vec!["bun".to_string(), "node".to_string(), "npm".to_string()],
            packages: vec!["tailwindcss".to_string()],
            dev_packages: vec!["postcss".to_string(), "autoprefixer".to_string()],
            files: vec![
                StackFile {
                    path: "src/styles/tailwind.css".to_string(),
                    content: "@import \"tailwindcss\";\n".to_string(),
                    binary_content: None,
                },
                StackFile {
                    path: "postcss.config.js".to_string(),
                    content: r#"module.exports = {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
};
"#
                    .to_string(),
                    binary_content: None,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_builtin_blocks_catalogue() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        let blocks = MiniKitBlocksManager::get_builtin_blocks(ws);
        let names: Vec<String> = blocks.iter().map(|b| b.name.clone()).collect();

        assert!(names.contains(&"docker".to_string()));
        assert!(names.contains(&"github-ci".to_string()));
        assert!(names.contains(&"gitignore".to_string()));
        assert!(names.contains(&"editorconfig".to_string()));
        assert!(names.contains(&"healthcheck".to_string()));
        assert!(names.contains(&"tailwind".to_string()));
    }

    #[test]
    fn test_add_block_lifecycle_and_conflict_safety() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        // 1. Add editorconfig block
        let res = MiniKitBlocksManager::add_block(ws, "editorconfig", false).unwrap();
        assert!(res.contains("Successfully added architecture block `editorconfig`"));
        assert!(ws.join(".editorconfig").exists());

        // 2. Re-adding without force fails due to existing file
        let res_err = MiniKitBlocksManager::add_block(ws, "editorconfig", false);
        assert!(res_err.is_err());
        assert!(res_err
            .unwrap_err()
            .to_string()
            .contains("conflicts with existing files"));

        // 3. Re-adding with force = true succeeds
        let res_force = MiniKitBlocksManager::add_block(ws, "editorconfig", true).unwrap();
        assert!(res_force.contains("Successfully added architecture block `editorconfig`"));
    }

    #[test]
    fn test_docker_block_adapts_to_runtime() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        // Rust workspace
        fs::write(ws.join("Cargo.toml"), "[package]\nname = \"foo\"\n").unwrap();
        let docker_rust = MiniKitBlocksManager::generate_docker_block("rust");
        assert!(docker_rust.files[0].content.contains("cargo-chef"));

        // Python workspace
        let docker_py = MiniKitBlocksManager::generate_docker_block("python");
        assert!(docker_py.files[0].content.contains("uv"));
    }
}
