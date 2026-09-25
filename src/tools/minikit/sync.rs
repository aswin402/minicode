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

    /// Ensures that `.minicode/` local runtime and transient memory directory is present in `.gitignore`.
    pub fn ensure_gitignore(workspace_root: &Path) {
        let gitignore_path = workspace_root.join(".gitignore");
        if !gitignore_path.exists() {
            let initial = "# minicode local runtime & transient memory\n.minicode/\n";
            let _ = fs::write(&gitignore_path, initial);
            return;
        }

        if let Ok(content) = fs::read_to_string(&gitignore_path) {
            let has_minicode = content.lines().any(|l| {
                let t = l.trim();
                t == ".minicode"
                    || t == ".minicode/"
                    || t.starts_with(".minicode/")
                    || t.contains("/.minicode")
            });
            if !has_minicode {
                let mut updated = content;
                if !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str("\n# minicode local runtime & transient memory\n.minicode/\n");
                let _ = fs::write(&gitignore_path, updated);
            }
        }
    }

    /// Ensures the canonical 8 core spec workflow files exist under `docs_dir/core/`,
    /// categorizes `skills/` and `packages/`, and seamlessly migrates any legacy flat files.
    pub fn ensure_workflow_docs(docs_dir: &Path, project_name: &str, runtime: &str) {
        let core_dir = docs_dir.join("core");
        let skills_dir = docs_dir.join("skills");
        let packages_dir = docs_dir.join("packages");

        fs::create_dir_all(&core_dir).ok();
        fs::create_dir_all(&skills_dir).ok();
        fs::create_dir_all(&packages_dir).ok();

        // --- Automated Migration of Legacy Flat Docs without Data Loss ---
        if let Ok(entries) = fs::read_dir(docs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

                    // Leave top-level catalog files at the root
                    if file_name.eq_ignore_ascii_case("index.md")
                        || file_name.eq_ignore_ascii_case("log.md")
                    {
                        continue;
                    }

                    if super::CORE_DOC_FILES
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(&file_name))
                    {
                        let target = core_dir.join(&file_name);
                        if !target.exists() {
                            let _ = fs::rename(&path, &target);
                        }
                    } else if file_name.ends_with(".md") {
                        // Migrate skill/domain markdown files into skills/
                        let target = skills_dir.join(&file_name);
                        if !target.exists() {
                            let _ = fs::rename(&path, &target);
                        }
                    }
                }
            }
        }

        // 1. coreidea.md (Core Project Philosophy & High-Level Vision)
        let coreidea_path = core_dir.join("coreidea.md");
        if !coreidea_path.exists() {
            let coreidea = format!(
                "# Core Philosophy & High-Level Vision 💡\n\n\
                ## Core Purpose\n\
                *Project: `{}` — built with `{}`.*\n\n\
                ## Value Proposition & Tenets\n\
                - Deterministic, fast, and resilient developer experience.\n\
                - Rigorous modular architecture with strict separation of concerns.\n\
                - Zero unverified assumptions: evidence before assertions.\n\n\
                ## Non-Goals\n\
                - Over-engineered complexity or rigid inflexible abstractions.\n",
                project_name, runtime
            );
            fs::write(coreidea_path, coreidea).ok();
        }

        // 2. prd.md (Product Requirements Document)
        let prd_path = core_dir.join("prd.md");
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

        // 3. architecture.md (Architecture & AST CodeGraph)
        let arch_path = core_dir.join("architecture.md");
        if !arch_path.exists() {
            let arch = format!(
                "# Clean Architecture & AST CodeGraph Overview: `{}` 🏛️\n\n\
                > *Automatically synthesized and maintained by `minicode` AST & CodeGraph Engine.*\n\n\
                ## Architectural Principles\n\
                - Modular layers with clear unidirectional dependency boundaries.\n\
                - AST symbol indexing and PageRank centrality graph.\n\
                - Pure-Rust runtime portability.\n\n\
                ## System Structure\n\
                - `src/`: Core application logic and modules.\n",
                project_name
            );
            fs::write(arch_path, arch).ok();
        }

        // 4. spec.md (Technical Specifications & Invariant Rules)
        let spec_path = core_dir.join("spec.md");
        if !spec_path.exists() {
            let spec = format!(
                "# Technical Specifications & Invariant Rules 📐\n\n\
                ## Core Engineering Invariants\n\
                1. **Zero Crash Invariant**: Zero unhandled panics or `.unwrap()` in production paths.\n\
                2. **Verification Barrier**: Compile checks and tests must pass before completing work.\n\
                3. **Portability Invariant**: Pure-system dependencies without native build friction.\n\n\
                ## Technical Interface Contracts\n\
                - Protocol schemas, CLI arguments, and error representations for `{}`.\n",
                project_name
            );
            fs::write(spec_path, spec).ok();
        }

        // 5. design.md (Architecture & Design Specification)
        let design_path = core_dir.join("design.md");
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

        // 6. implementation.md (Technical Implementation Plan)
        let impl_path = core_dir.join("implementation.md");
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

        // 7. content.md (Content & Interface Catalog)
        let content_path = core_dir.join("content.md");
        if !content_path.exists() {
            let content = format!(
                "# Content & Interface Catalog 📝\n\n\
                ## Interfaces & Endpoints\n\
                - User-facing commands, routes, and output schemas for `{}`.\n",
                project_name
            );
            fs::write(content_path, content).ok();
        }

        // 8. todo.md (Task Tracker & Macro Roadmap)
        let todo_path = core_dir.join("todo.md");
        if !todo_path.exists() {
            let todo = "# Task Tracker (todo.md) 📋\n\n\
                ## Active Milestone\n\
                - [x] Initial project setup and architecture sync\n\
                - [ ] Implement core features\n\
                - [ ] Add integration and unit tests\n\
                - [ ] Verification and documentation\n";
            fs::write(todo_path, todo).ok();
        }

        // 9. skills/miniblocks.md (MiniBlocks Native UI Component & Design Warehouse)
        let miniblocks_path = skills_dir.join("miniblocks.md");
        if !miniblocks_path.exists() {
            let miniblocks_content = "# MiniBlocks Native UI Component & Design Warehouse 🎨\n\n\
                ## Overview\n\
                MiniBlocks is a native, local-first UI component and design token warehouse embedded directly inside minicode. It provides instant access to **1,082+ verified components**, **105 curated palettes**, **212 gradients**, and **3 layout templates** without external network dependencies.\n\n\
                ## Available Component Categories\n\
                - `navbar`: Navigation headers, responsive menus, brand bars.\n\
                - `hero`: Above-the-fold banners, CTAs, conversion hero sections.\n\
                - `footer`: Site footers, link matrices, copyright bars.\n\
                - `card`: Feature cards, pricing cards, profile displays, interactive cards.\n\
                - `modal`: Dialogs, overlays, confirmation sheets, alerts.\n\
                - `pricing`: Subscription tiers, comparison tables, billing switches.\n\
                - `form`: Inputs, login/signup forms, contact sheets, validation states.\n\
                - `table`: Data tables, sortable grids, record lists.\n\
                - `sidebar`, `banner`, `badge`, `button`, and more.\n\n\
                ## Warehouse Tools Reference (11 Tools)\n\
                1. `block_search`: Search UI components by keyword, category, framework, or tags with relevance scoring.\n\
                2. `block_get`: Retrieve complete source code, dependencies, and metadata by component ID or name.\n\
                3. `block_insert`: Inject a component or raw code into a workspace file (`append`, `prepend`, `create`, `replace`).\n\
                4. `block_save`: Save a new custom UI component into the warehouse.\n\
                5. `block_update`: Update component code, description, or tags with automatic version incrementing.\n\
                6. `block_delete`: Delete a component from the warehouse by UUID.\n\
                7. `block_palettes`: Query 4-hex curated color palettes with multi-format token export (CSS, Tailwind, SCSS, JSON).\n\
                8. `block_gradients`: Search curated modern CSS gradients with color stops.\n\
                9. `block_scaffold`: Scaffold complete layout templates (`landing`, `portfolio`, `dashboard`) or project-tailored custom components with palette token inheritance (`palette`) and auto-wiring (`wire_to`).\n\
                10. `block_import`: Generate exact path-aliased import statements and JSX tags, with optional auto-wiring into consumer files.\n\
                11. `block_stats`: Inspect warehouse catalog statistics and category/framework breakdowns.\n\n\
                ## AI Agent Guidelines\n\
                - **Query First**: Always query `block_search` or `block_palettes` before creating UI components or color palettes from scratch.\n\
                - **Themed Scaffolding & Wiring**: Pass `palette=\"<palette_name>\"` to `block_scaffold` to automatically apply background, surface, and accent tokens, and `wire_to=\"src/App.tsx\"` to auto-wire the component.\n\
                - **Surgical Insertion & Imports**: Use `block_insert` to safely write components, and `block_import` with `wire=true` to inject imports into existing files without rewriting them.\n\
                - **Vite & TypeScript Setup**: When creating a Vite + React TypeScript project, always include `src/vite-env.d.ts` (`/// <reference types=\"vite/client\" />`) or add `\"types\": [\"vite/client\"]` to `tsconfig.json` so CSS imports (`import './index.css'`) and assets resolve cleanly without type errors.\n\
                - **Path Sandboxing**: All file insertions are confined within the workspace.\n\n\
                ## User Shortcuts & Commands\n\
                - **Keyboard Shortcut**: Press `F6` in the TUI to open the interactive MiniBlocks browser modal.\n\
                - **Slash Command**: Type `/blocks` in chat to browse components and palettes.\n";
            fs::write(miniblocks_path, miniblocks_content).ok();
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

        // 2. Ensure .gitignore protects .minicode/ local runtime & transient memory
        Self::ensure_gitignore(workspace_root);

        // 3. Refresh AGENTS.md if missing
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
                ## Active Documentation & Specifications\n\
                - [Core Philosophy & Vision](file://./{}/core/coreidea.md)\n\
                - [Product Requirements Document (PRD)](file://./{}/core/prd.md)\n\
                - [Architecture & AST CodeGraph](file://./{}/core/architecture.md)\n\
                - [Technical Invariants & Spec](file://./{}/core/spec.md)\n\
                - [Design Specification](file://./{}/core/design.md)\n\
                - [Technical Implementation Plan](file://./{}/core/implementation.md)\n\
                - [Task Tracker (todo.md)](file://./{}/core/todo.md)\n\
                - [Content Reference](file://./{}/core/content.md)\n",
                project_name,
                project_name,
                runtime,
                package_manager,
                docs_dir_name,
                docs_dir_name,
                docs_dir_name,
                docs_dir_name,
                docs_dir_name,
                docs_dir_name,
                docs_dir_name,
                docs_dir_name
            );
            fs::write(&agents_md_path, agents_md).ok();
        }

        // 4. Ensure docs directory exists and synchronize the 8 core workflow documents
        Self::ensure_workflow_docs(&docs_dir, project_name, runtime);

        // Also ensure onpkg_docs exists for backward compatibility if configured
        let onpkg_docs = workspace_root.join(crate::constants::ONPKG_DOCS_DIR);
        if onpkg_docs.exists() {
            Self::ensure_workflow_docs(&onpkg_docs, project_name, runtime);
        }

        // 5. Wire AST CodeGraph & synthesize clean architecture documentation into core/architecture.md
        let arch_options = crate::context::governance::doc_synthesizer::ArchitectureDocOptions {
            include_mermaid: true,
            include_symbol_catalog: true,
            write_to_file: false,
        };
        if let Ok(arch_report) =
            crate::context::governance::doc_synthesizer::ArchitectureDocSynthesizer::synthesize(
                workspace_root,
                arch_options,
            )
        {
            let arch_file = docs_dir.join("core").join("architecture.md");
            fs::write(&arch_file, arch_report.markdown_content).ok();
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
            • Core Specs: {}/core/ (coreidea.md, prd.md, architecture.md, spec.md, design.md, implementation.md, content.md, todo.md)\n\
            • Domain Skills: {}/skills/ | Package Docs: {}/packages/\n\
            • Memory Isolation: .minicode/ (.gitignore verified)\n\
            • OKF Catalog: index.md & log.md updated",
            project_name,
            manifest_filename,
            runtime,
            package_manager,
            manifest["architecture"],
            docs_dir_name,
            docs_dir_name,
            docs_dir_name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ensure_gitignore() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        // Case 1: .gitignore doesn't exist -> created with .minicode/
        MiniKitSyncEngine::ensure_gitignore(ws);
        let content = fs::read_to_string(ws.join(".gitignore")).unwrap();
        assert!(content.contains(".minicode/"));

        // Case 2: existing .gitignore without .minicode/ -> appended
        fs::write(ws.join(".gitignore"), "target/\n.env\n").unwrap();
        MiniKitSyncEngine::ensure_gitignore(ws);
        let content2 = fs::read_to_string(ws.join(".gitignore")).unwrap();
        assert!(content2.starts_with("target/\n.env\n"));
        assert!(content2.contains(".minicode/"));

        // Case 3: already contains .minicode/ -> unchanged
        MiniKitSyncEngine::ensure_gitignore(ws);
        let content3 = fs::read_to_string(ws.join(".gitignore")).unwrap();
        assert_eq!(content2, content3);
    }

    #[test]
    fn test_ensure_workflow_docs_scaffolding_and_migration() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("minikit_docs");
        fs::create_dir_all(&docs).unwrap();

        // Simulate legacy flat docs directory
        fs::write(docs.join("prd.md"), "# Custom PRD").unwrap();
        fs::write(docs.join("todo.md"), "# Custom Todo").unwrap();
        fs::write(docs.join("rust.md"), "# Rust Skill").unwrap();
        fs::write(docs.join("index.md"), "# Catalog Index").unwrap();

        MiniKitSyncEngine::ensure_workflow_docs(&docs, "my_app", "rust");

        // Verify categories exist
        assert!(docs.join("core").is_dir());
        assert!(docs.join("skills").is_dir());
        assert!(docs.join("packages").is_dir());

        // Verify index.md was preserved at root
        assert!(docs.join("index.md").is_file());

        // Verify legacy flat core files were migrated to core/ without data loss
        let migrated_prd = fs::read_to_string(docs.join("core").join("prd.md")).unwrap();
        assert_eq!(migrated_prd, "# Custom PRD");

        let migrated_todo = fs::read_to_string(docs.join("core").join("todo.md")).unwrap();
        assert_eq!(migrated_todo, "# Custom Todo");

        // Verify non-core .md was migrated to skills/
        let migrated_skill = fs::read_to_string(docs.join("skills").join("rust.md")).unwrap();
        assert_eq!(migrated_skill, "# Rust Skill");

        // Verify all 8 canonical core files exist
        for core_file in crate::tools::minikit::CORE_DOC_FILES {
            assert!(
                docs.join("core").join(core_file).exists(),
                "Missing core file {}",
                core_file
            );
        }

        // Verify skills/miniblocks.md was generated
        assert!(docs.join("skills").join("miniblocks.md").exists());
    }
}
