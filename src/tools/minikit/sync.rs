use crate::error::Result;
use std::fs;
use std::path::Path;

/// Native engine for scanning workspace dependencies, updating `minikit.json`/`onpkg.json`, and refreshing `AGENTS.md`.
pub struct MiniKitSyncEngine;

#[allow(dead_code)]
pub type OnpkgSyncEngine = MiniKitSyncEngine;

impl MiniKitSyncEngine {
    /// Detects project runtime and primary configuration files.
    pub fn detect_runtime(workspace_root: &Path) -> (&'static str, &'static str) {
        if workspace_root.join("Cargo.toml").exists() {
            ("rust", "cargo")
        } else if workspace_root.join("pubspec.yaml").exists() {
            ("flutter", "flutter")
        } else if workspace_root.join("go.mod").exists() {
            ("go", "go")
        } else if workspace_root.join("pyproject.toml").exists()
            || workspace_root.join("requirements.txt").exists()
        {
            ("python", "uv")
        } else if workspace_root.join("bun.lockb").exists()
            || workspace_root.join("bun.lock").exists()
        {
            ("bun", "bun")
        } else if workspace_root.join("pnpm-lock.yaml").exists() {
            ("node", "pnpm")
        } else if workspace_root.join("yarn.lock").exists() {
            ("node", "yarn")
        } else if workspace_root.join("package.json").exists() {
            ("node", "npm")
        } else {
            ("generic", "custom")
        }
    }

    /// Detects deep architecture patterns (entrypoints, routing, components, styles, database, tests).
    pub fn detect_architecture(workspace_root: &Path) -> serde_json::Value {
        let mut arch = serde_json::Map::new();

        let entrypoint_patterns = [
            "src/main.rs",
            "src/main.tsx",
            "src/main.ts",
            "src/index.js",
            "src/index.tsx",
            "src/index.ts",
            "lib/main.dart",
            "app/main.py",
            "src/main.py",
            "main.py",
            "main.go",
            "cmd/main.go",
            "cmd/server/main.go",
            "cmd/api/main.go",
        ];
        for ep in &entrypoint_patterns {
            if workspace_root.join(ep).exists() {
                arch.insert("entrypoint".to_string(), serde_json::json!(ep));
                break;
            }
        }

        let routing_patterns = [
            "src/routes",
            "src/pages",
            "app/routes",
            "app",
            "src/app",
            "routes",
            "pages",
        ];
        for r in &routing_patterns {
            if workspace_root.join(r).exists() {
                arch.insert("routing".to_string(), serde_json::json!(r));
                break;
            }
        }

        let components_patterns = [
            "src/components",
            "components",
            "src/ui",
            "src/components/ui",
            "ui",
        ];
        for c in &components_patterns {
            if workspace_root.join(c).exists() {
                arch.insert("components".to_string(), serde_json::json!(c));
                break;
            }
        }

        let styles_patterns = [
            "src/index.css",
            "src/App.css",
            "src/styles.css",
            "styles",
            "src/global.css",
            "src/styles/global.css",
        ];
        for s in &styles_patterns {
            if workspace_root.join(s).exists() {
                arch.insert("styles".to_string(), serde_json::json!(s));
                break;
            }
        }

        let db_patterns = [
            "prisma/schema.prisma",
            "src/db",
            "db",
            "schema.sql",
            "migrations",
            "src/database",
        ];
        for d in &db_patterns {
            if workspace_root.join(d).exists() {
                arch.insert("database".to_string(), serde_json::json!(d));
                break;
            }
        }

        let tests_patterns = ["src/tests", "tests", "test", "src/test"];
        for t in &tests_patterns {
            if workspace_root.join(t).exists() {
                arch.insert("tests".to_string(), serde_json::json!(t));
                break;
            }
        }

        serde_json::Value::Object(arch)
    }

    /// Ensures the 5 core spec workflow files exist under the docs directory.
    pub fn ensure_workflow_docs(docs_dir: &Path, project_name: &str, runtime: &str) {
        fs::create_dir_all(docs_dir).ok();

        // 1. prd.md
        let prd_path = docs_dir.join("prd.md");
        if !prd_path.exists() {
            let prd = format!(
                "# Product Requirements Document (PRD) 🚀\n\n\
                ## Project Overview\n\
                *Project: `{}` — built with `{}`.*\n\n\
                ## Core Objectives & User Stories\n\
                - [ ] **Objective 1**: Core system functionality.\n\
                - [ ] **Objective 2**: Fast, intuitive developer and user experience.\n\n\
                ## Success Metrics & Constraints\n\
                - Performance: Sub-second response times and minimal latency.\n\
                - Reliability: Strict error handling with zero unhandled crashes.\n",
                project_name, runtime
            );
            fs::write(prd_path, prd).ok();
        }

        // 2. design.md
        let design_path = docs_dir.join("design.md");
        if !design_path.exists() {
            let design = format!(
                "# Architecture & Design Specification 🎨\n\n\
                ## Architectural Principles\n\
                - Modular, decoupled components with single responsibility.\n\
                - Idiomatic `{}` conventions.\n\n\
                ## Design System & Interfaces\n\
                - Visual layout, styles, and command contracts.\n",
                runtime
            );
            fs::write(design_path, design).ok();
        }

        // 3. implementation.md
        let impl_path = docs_dir.join("implementation.md");
        if !impl_path.exists() {
            let imp = format!(
                "# Technical Implementation Plan 🛠️\n\n\
                ## Technology Stack\n\
                - **Runtime**: `{}`\n\n\
                ## Architecture Layout\n\
                - Code layout, public APIs, and database migrations.\n\n\
                ## Milestone Roadmap\n\
                - Structured phases with verifiable acceptance criteria.\n",
                runtime
            );
            fs::write(impl_path, imp).ok();
        }

        // 4. todo.md
        let todo_path = docs_dir.join("todo.md");
        if !todo_path.exists() {
            let todo = "# Task Tracker (todo.md) 📋\n\n\
                ## Active Milestone\n\
                - [x] Initial project setup and architecture sync\n\
                - [ ] Implement core features\n\
                - [ ] Add integration and unit tests\n\
                - [ ] Verification and documentation\n";
            fs::write(todo_path, todo).ok();
        }

        // 5. content.md
        let content_path = docs_dir.join("content.md");
        if !content_path.exists() {
            let content = format!(
                "# Content & Interface Catalog 📝\n\n\
                ## Interfaces & Endpoints\n\
                - User-facing commands, routes, and output schemas for `{}`.\n",
                project_name
            );
            fs::write(content_path, content).ok();
        }
    }

    /// Performs native spec-driven synchronization.
    pub fn sync(workspace_root: &Path) -> Result<String> {
        let (runtime, package_manager) = Self::detect_runtime(workspace_root);
        let project_name = workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");

        let manifest_path = super::default_manifest_path(workspace_root);
        let manifest_filename = manifest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("minikit.json");

        let mut manifest: serde_json::Value = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if manifest.get("name").is_none() {
            manifest["name"] = serde_json::json!(project_name);
        }
        manifest["runtime"] = serde_json::json!(runtime);
        manifest["package_manager"] = serde_json::json!(package_manager);

        if manifest.get("active_skills").is_none() {
            manifest["active_skills"] = serde_json::json!([runtime]);
        }

        // Deep architecture detection
        let arch = Self::detect_architecture(workspace_root);
        manifest["architecture"] = arch;

        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap_or_default();
        fs::write(&manifest_path, &manifest_json).ok();

        // Also sync onpkg.json for seamless backward-compatibility
        let onpkg_json_path = workspace_root.join(crate::constants::ONPKG_MANIFEST_FILE);
        if onpkg_json_path.exists() || manifest_filename == crate::constants::ONPKG_MANIFEST_FILE {
            fs::write(&onpkg_json_path, &manifest_json).ok();
        } else {
            // Also write onpkg.json so tooling expecting either manifest works
            fs::write(&onpkg_json_path, &manifest_json).ok();
        }

        // 2. Refresh AGENTS.md if missing
        let docs_dir = super::resolve_docs_dir(workspace_root);
        let docs_dir_name = docs_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("minikit_docs");

        let agents_md_path = workspace_root.join("AGENTS.md");
        if !agents_md_path.exists() {
            let agents_md = format!(
                "# {} — Agent Guidelines & Repository Instructions 🧠\n\n\
                > Synchronized with `minicode` + `MiniKit`.\n\n\
                ## Project Summary\n\
                - **Name:** `{}`\n\
                - **Runtime:** `{}`\n\
                - **Package Manager:** `{}`\n\n\
                ## Documentation & Guidelines\n\
                - Follow active rules under `{}/`.\n",
                project_name, project_name, runtime, package_manager, docs_dir_name
            );
            fs::write(&agents_md_path, agents_md).ok();
        }

        // 3. Ensure docs directory exists and synchronize the 5 workflow documents
        Self::ensure_workflow_docs(&docs_dir, project_name, runtime);

        // Also ensure onpkg_docs exists for backward compatibility if configured
        let onpkg_docs = workspace_root.join(crate::constants::ONPKG_DOCS_DIR);
        if onpkg_docs.exists() {
            Self::ensure_workflow_docs(&onpkg_docs, project_name, runtime);
        }

        crate::context::okf::OkfManager::generate_index_md(&docs_dir).ok();
        crate::context::okf::OkfManager::append_log_entry(
            &docs_dir,
            &format!("minicode/v{}", env!("CARGO_PKG_VERSION")),
            "SYNC",
            manifest_filename,
            &format!(
                "Synchronized project manifest, architecture, and workflow docs (Runtime: {}, Package Manager: {})",
                runtime, package_manager
            ),
        ).ok();

        Ok(format!(
            "✔ Synchronized `{}` project manifest:\n\
            • Manifest: {} (Runtime: `{}`, Package Manager: `{}`)\n\
            • Architecture: {:?}\n\
            • Instructions: AGENTS.md\n\
            • Workflow Docs: {}/ (prd.md, design.md, implementation.md, todo.md, content.md)\n\
            • OKF Catalog: index.md & log.md updated",
            project_name,
            manifest_filename,
            runtime,
            package_manager,
            manifest["architecture"],
            docs_dir_name
        ))
    }
}
