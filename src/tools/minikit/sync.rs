use crate::error::Result;
use std::fs;
use std::path::Path;

/// Native engine for scanning workspace dependencies, updating `minikit.json`/`onpkg.json`, and refreshing `AGENTS.md`.
pub struct MiniKitSyncEngine;

#[allow(dead_code)]
pub type OnpkgSyncEngine = MiniKitSyncEngine;

impl MiniKitSyncEngine {
    /// Detects project runtime and primary configuration files from the declarative `RUNTIME_DESCRIPTORS` table.
    pub fn detect_runtime(workspace_root: &Path) -> (&'static str, &'static str) {
        super::detect_workspace_runtime_descriptor(workspace_root)
            .map(|d| (d.runtime_id, d.package_manager))
            .unwrap_or(("generic", "custom"))
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

    /// Ensures that `.minicode/` local runtime/transient memory, `screenshots/`, `minikit_docs/`, and `minikit.json` are present in `.gitignore`.
    pub fn ensure_gitignore(workspace_root: &Path) {
        let gitignore_path = workspace_root.join(".gitignore");
        if !gitignore_path.exists() {
            let initial = "# minicode local runtime & transient memory\n.minicode/\nscreenshots/\nminikit_docs/\nminikit.json\n";
            let _ = fs::write(&gitignore_path, initial);
            return;
        }

        if let Ok(content) = fs::read_to_string(&gitignore_path) {
            let has_dir_rule = |dir: &str| {
                let slash = format!("{}/", dir);
                let leading_slash = format!("/{}", dir);
                content.lines().any(|l| {
                    let t = l.trim();
                    t == dir || t == slash || t.starts_with(&slash) || t.contains(&leading_slash)
                })
            };
            let has_minicode = has_dir_rule(".minicode");
            let has_screenshots = has_dir_rule("screenshots");
            let has_minikit_docs = has_dir_rule("minikit_docs");
            let has_minikit_json = content
                .lines()
                .any(|l| matches!(l.trim(), "minikit.json" | "/minikit.json"));

            if !has_minicode || !has_screenshots || !has_minikit_docs || !has_minikit_json {
                let mut updated = content;
                if !updated.ends_with('\n') {
                    updated.push('\n');
                }
                for (present, snippet) in [
                    (
                        has_minicode,
                        "\n# minicode local runtime & transient memory\n.minicode/\n",
                    ),
                    (has_screenshots, "screenshots/\n"),
                    (has_minikit_docs, "minikit_docs/\n"),
                    (has_minikit_json, "minikit.json\n"),
                ] {
                    if !present {
                        updated.push_str(snippet);
                    }
                }
                let _ = fs::write(&gitignore_path, updated);
            }
        }
    }

    /// Collects the canonical `minikit_<slug>.md` filenames for packages explicitly declared
    /// in the workspace's `minikit.json`, `onpkg.json`, or `package.json`.
    fn collect_declared_package_doc_filenames(
        workspace_root: &Path,
    ) -> std::collections::HashSet<String> {
        let mut filenames = std::collections::HashSet::new();
        let mut insert_pkg = |pkg_name: &str| {
            let trimmed = pkg_name.trim().to_lowercase();
            if !trimmed.is_empty() {
                let slug = trimmed.replace(['@', '/'], "__");
                filenames.insert(format!("minikit_{}.md", slug));
                filenames.insert(trimmed);
            }
        };

        for manifest_name in &[
            crate::constants::MINIKIT_MANIFEST_FILE,
            crate::constants::ONPKG_MANIFEST_FILE,
        ] {
            let path = workspace_root.join(manifest_name);
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    for field in &["packages", "dev_packages", "transitive_packages"] {
                        if let Some(arr) = json.get(*field).and_then(|v| v.as_array()) {
                            for item in arr {
                                if let Some(s) = item.as_str() {
                                    insert_pkg(s);
                                }
                            }
                        }
                    }
                }
            }
        }

        let pkg_json_path = workspace_root.join("package.json");
        if let Ok(content) = fs::read_to_string(&pkg_json_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                for field in &["dependencies", "devDependencies", "peerDependencies"] {
                    if let Some(obj) = json.get(*field).and_then(|v| v.as_object()) {
                        for key in obj.keys() {
                            insert_pkg(key);
                        }
                    }
                }
            }
        }

        filenames
    }

    /// Ensures the canonical 8 core spec workflow files exist under `docs_dir/core/`,
    /// categorizes `packages/`, and seamlessly migrates any legacy flat files or legacy skills/ directories.
    pub fn ensure_workflow_docs(docs_dir: &Path, project_name: &str, runtime: &str) {
        Self::ensure_workflow_docs_with_arch(docs_dir, project_name, runtime, None);
    }

    fn ensure_workflow_docs_with_arch(
        docs_dir: &Path,
        project_name: &str,
        runtime: &str,
        precomputed_arch: Option<&serde_json::Value>,
    ) {
        let core_dir = docs_dir.join("core");
        let packages_dir = docs_dir.join("packages");
        let legacy_skills_dir = docs_dir.join("skills");

        fs::create_dir_all(&core_dir).ok();
        fs::create_dir_all(&packages_dir).ok();

        // --- Automated Migration of Legacy skills/ directory into packages/ ---
        if legacy_skills_dir.exists() {
            if let Ok(skill_entries) = fs::read_dir(&legacy_skills_dir) {
                for entry in skill_entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let file_name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let target = packages_dir.join(&file_name);
                        if !target.exists() {
                            let _ = fs::rename(&path, &target);
                        } else {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
            let _ = fs::remove_dir_all(&legacy_skills_dir);
        }

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

                    // Remove redundant legacy uppercase INDEX.md to avoid case collision with index.md
                    if file_name == "INDEX.md" {
                        let _ = fs::remove_file(&path);
                        continue;
                    }

                    // Leave top-level catalog files at the root
                    if file_name == "index.md" || file_name == "log.md" {
                        continue;
                    }

                    if super::CORE_DOC_FILES
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(&file_name))
                    {
                        let target = core_dir.join(&file_name);
                        if !target.exists() {
                            let _ = fs::rename(&path, &target);
                        } else {
                            let _ = fs::remove_file(&path);
                        }
                    } else if file_name.ends_with(".md") {
                        let target = packages_dir.join(&file_name);
                        if !target.exists() {
                            let _ = fs::rename(&path, &target);
                        } else {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        let workspace_root = docs_dir.parent().unwrap_or(docs_dir);

        // --- Populate Package Documentation into packages/ (filtered by workspace dependencies) ---
        let declared_doc_files = Self::collect_declared_package_doc_filenames(workspace_root);
        if !declared_doc_files.is_empty() {
            let package_docs = crate::tools::minikit::stacks::builtin::react_package_docs();
            for doc in &package_docs {
                let file_name = std::path::Path::new(&doc.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let by_npm_url =
                    crate::tools::minikit::stacks::builtin::package_docs::extract_doc_npm_package(
                        &doc.content,
                    )
                    .is_some_and(|pkg| declared_doc_files.contains(&pkg.to_ascii_lowercase()));
                if !declared_doc_files.contains(file_name.as_ref()) && !by_npm_url {
                    continue;
                }
                let target = packages_dir.join(&*file_name);
                if !target.exists() {
                    let _ = fs::write(&target, &doc.content);
                }
            }
        }

        let decision_bullets = Self::format_decision_bullets(workspace_root);
        let decisions_section = if decision_bullets.is_empty() {
            String::new()
        } else {
            format!("\n## Recorded User Decisions\n{decision_bullets}\n")
        };
        let primary_hex = crate::agent::decisions::extract_decided_primary_hex(workspace_root)
            .unwrap_or_else(|| crate::constants::DEFAULT_DESIGN_PRIMARY_HEX.to_string());
        let owned_arch;
        let detected_arch = match precomputed_arch {
            Some(a) => a,
            None => {
                owned_arch = Self::detect_architecture(workspace_root);
                &owned_arch
            }
        };
        let mut arch_structure_lines = Vec::new();
        if let Some(obj) = detected_arch.as_object() {
            for (role, path_val) in obj {
                if let Some(p) = path_val.as_str() {
                    arch_structure_lines.push(format!("- **{}**: `{}`", role, p));
                }
            }
        }
        if arch_structure_lines.is_empty() {
            arch_structure_lines
                .push("- `src/`: Core application source modules and components.".to_string());
        }
        let arch_structure_block = arch_structure_lines.join("\n");

        // 1. coreidea.md (Core Project Philosophy & High-Level Vision)
        let coreidea_path = core_dir.join("coreidea.md");
        if !coreidea_path.exists() {
            let coreidea = format!(
                "---\ntype: Vision\ntitle: \"Core Philosophy & High-Level Vision\"\ndescription: \"Core project vision, value proposition, and engineering tenets\"\nstatus: stable\n---\n\n\
                # Core Philosophy & High-Level Vision 💡\n\n\
                ## Core Purpose\n\
                *Project: `{}` — built with `{}`.*\n{}\n\
                ## Value Proposition & Tenets\n\
                - Deterministic, fast, and resilient user and developer experience.\n\
                - Rigorous modular architecture with strict separation of concerns.\n\
                - Zero unverified assumptions: evidence before assertions.\n\n\
                ## Non-Goals\n\
                - Over-engineered complexity or rigid inflexible abstractions.\n",
                project_name, runtime, decisions_section
            );
            fs::write(coreidea_path, coreidea).ok();
        }

        // 2. prd.md (Product Requirements Document)
        let prd_path = core_dir.join("prd.md");
        if !prd_path.exists() {
            let prd = format!(
                "---\ntype: PRD\ntitle: \"Product Requirements Document\"\ndescription: \"Core product objectives, user stories, and success metrics\"\nstatus: stable\n---\n\n\
                # Product Requirements Document (PRD) 🚀\n\n\
                ## Project Overview\n\
                *Project: `{}` — built with `{}`.*\n{}\n\
                ## Core Objectives & User Stories\n\
                - [ ] **Objective 1**: Core system functionality.\n\
                - [ ] **Objective 2**: Fast, intuitive developer and user experience.\n\n\
                ## Success Metrics & Constraints\n\
                - Performance: Sub-second response times and minimal latency.\n\
                - Reliability: Strict error handling with zero unhandled runtime errors.\n",
                project_name, runtime, decisions_section
            );
            fs::write(&prd_path, &prd).ok();
        }
        let root_prd = docs_dir.join("prd.md");
        if root_prd.exists() {
            let _ = fs::remove_file(root_prd);
        }

        // 3. architecture.md (Architecture & AST CodeGraph)
        let arch_path = core_dir.join("architecture.md");
        if !arch_path.exists() {
            let arch = format!(
                "---\ntype: Architecture\ntitle: \"Clean Architecture & AST CodeGraph Overview\"\ndescription: \"System layer topology, dependency boundaries, and AST symbol structure\"\nstatus: stable\n---\n\n\
                # Clean Architecture & AST CodeGraph Overview: `{}` 🏛️\n\n\
                > *Automatically synthesized and maintained by `minicode` AST & CodeGraph Engine.*\n\n\
                ## Architectural Principles\n\
                - Modular layers with clear unidirectional dependency boundaries.\n\
                - AST symbol indexing and PageRank centrality graph.\n\
                - Idiomatic `{}` runtime conventions and portable dependencies.\n\n\
                ## System Structure\n\
                {}\n",
                project_name, runtime, arch_structure_block
            );
            fs::write(arch_path, arch).ok();
        }

        // 4. spec.md (Technical Specifications & Invariant Rules)
        let spec_path = core_dir.join("spec.md");
        if !spec_path.exists() {
            let spec = format!(
                "---\ntype: Specification\ntitle: \"Technical Specifications & Invariant Rules\"\ndescription: \"Core engineering invariants and interface contracts\"\nstatus: stable\n---\n\n\
                # Technical Specifications & Invariant Rules 📐\n\n\
                ## Core Engineering Invariants\n\
                1. **Zero Crash Invariant**: Strict error handling and type safety in production paths.\n\
                2. **Verification Barrier**: Build checks, tests, and runtime verification must pass before completing work.\n\
                3. **Portability Invariant**: Clean, reproducible dependency management under `{}`.\n\n\
                ## Technical Interface Contracts\n\
                - Component props, state models, routes, and interface contracts for `{}`.\n",
                runtime, project_name
            );
            fs::write(spec_path, spec).ok();
        }

        // 5. design.md (Google Stitch & VoltAgent/awesome-design-md 9-Section Design Specification)
        let design_path = core_dir.join("design.md");
        if !design_path.exists() {
            let design = format!(
                "---\n\
                version: alpha\n\
                name: \"{}\"\n\
                type: Design\n\
                title: \"Architecture & Design System Specification (DESIGN.md)\"\n\
                description: \"9-section Stitch & VoltAgent/awesome-design-md visual system, color/typography tokens, elevation ladder, and component rules\"\n\
                status: stable\n\
                colors:\n\
                \x20\x20primary: \"{}\"\n\
                \x20\x20on-primary: \"#ffffff\"\n\
                \x20\x20primary-hover: \"#828fff\"\n\
                \x20\x20canvas: \"#08090a\"\n\
                \x20\x20surface-1: \"#0f1011\"\n\
                \x20\x20surface-2: \"#141516\"\n\
                \x20\x20surface-3: \"#191a1b\"\n\
                \x20\x20ink: \"#f7f8f8\"\n\
                \x20\x20ink-muted: \"#d0d6e0\"\n\
                \x20\x20ink-subtle: \"#8a8f98\"\n\
                \x20\x20hairline: \"rgba(255, 255, 255, 0.08)\"\n\
                \x20\x20semantic-success: \"#27a644\"\n\
                \x20\x20semantic-warning: \"#f59e0b\"\n\
                \x20\x20semantic-error: \"#ef4444\"\n\
                typography:\n\
                \x20\x20display-xl: {{ fontFamily: \"Inter, system-ui, sans-serif\", fontSize: \"3.5rem\", fontWeight: 600, lineHeight: 1.05, letterSpacing: \"-0.022em\" }}\n\
                \x20\x20headline: {{ fontFamily: \"Inter, system-ui, sans-serif\", fontSize: \"2rem\", fontWeight: 600, lineHeight: 1.15, letterSpacing: \"-0.015em\" }}\n\
                \x20\x20body: {{ fontFamily: \"Inter, system-ui, sans-serif\", fontSize: \"1rem\", fontWeight: 400, lineHeight: 1.6, letterSpacing: \"-0.011em\" }}\n\
                \x20\x20mono: {{ fontFamily: \"JetBrains Mono, monospace\", fontSize: \"0.875rem\", fontWeight: 400, lineHeight: 1.5, letterSpacing: \"0em\" }}\n\
                rounded:\n\
                \x20\x20sm: \"6px\"\n\
                \x20\x20md: \"8px\"\n\
                \x20\x20lg: \"12px\"\n\
                \x20\x20pill: \"9999px\"\n\
                spacing:\n\
                \x20\x20xs: \"4px\"\n\
                \x20\x20sm: \"8px\"\n\
                \x20\x20md: \"16px\"\n\
                \x20\x20lg: \"24px\"\n\
                \x20\x20xl: \"48px\"\n\
                \x20\x20section: \"96px\"\n\
                ---\n\n\
                # Design System Specification (`DESIGN.md`): `{}` 🎨\n\n\
                > Structured following the [VoltAgent/awesome-design-md](https://github.com/VoltAgent/awesome-design-md) & Google Stitch 9-Section `DESIGN.md` standard.\n\
                > Import any of 84+ real brand design systems via `vault_import(url=\"awesome-design-md/<brand>\", scope=\"project\")` (e.g., `linear.app`, `vercel`, `stripe`, `claude`, `raycast`, `supabase`).\n\n\
                ## 1. Visual Theme & Atmosphere\n\
                - **Mood & Density**: High-craft precision interface with generous negative space, crisp typographic hierarchy, and zero visual clutter.\n\
                - **Runtime & Stack Conventions**: Modular, decoupled components following idiomatic `{}` conventions.\n{}\n\
                ## 2. Color Palette & Roles\n\
                | Token | Value | Functional Role |\n\
                | :--- | :--- | :--- |\n\
                | `{{colors.canvas}}` | `#08090a` | Primary page background canvas (Level 0) |\n\
                | `{{colors.surface-1}}` | `#0f1011` | Elevated card & panel surface (Level 1) |\n\
                | `{{colors.surface-2}}` | `#141516` | Interactive hover & nested block surface (Level 2) |\n\
                | `{{colors.primary}}` | `{}` | Primary interactive brand accent & active CTAs |\n\
                | `{{colors.ink}}` | `#f7f8f8` | Primary high-contrast headings & foreground copy |\n\
                | `{{colors.ink-muted}}` | `#d0d6e0` | Secondary body copy & descriptions |\n\
                | `{{colors.ink-subtle}}` | `#8a8f98` | Metadata, timestamps, and keyboard shortcut hints |\n\
                | `{{colors.hairline}}` | `rgba(255,255,255,0.08)` | `1px solid` structural card & section dividers |\n\n\
                ## 3. Typography Rules\n\
                | Role | Font Family | Size | Weight | Line Height | Letter Spacing |\n\
                | :--- | :--- | :--- | :--- | :--- | :--- |\n\
                | **Display XL** | `Inter`, `system-ui` | `3.5rem` (`56px`) | `600` | `1.05` | `-0.022em` |\n\
                | **Section Headline** | `Inter`, `system-ui` | `2.0rem` (`32px`) | `600` | `1.15` | `-0.015em` |\n\
                | **Card Title** | `Inter`, `system-ui` | `1.25rem` (`20px`) | `500` | `1.30` | `-0.012em` |\n\
                | **Body Copy** | `Inter`, `system-ui` | `1.0rem` (`16px`) | `400` | `1.60` | `-0.011em` |\n\
                | **Code / Telemetry** | `JetBrains Mono` | `0.875rem` (`14px`) | `400` | `1.50` | `0em` (`tabular-nums`) |\n\n\
                ## 4. Layout & Spacing Principles\n\
                - **Spacing Scale**: `8px` base spatial grid (`4px`, `8px`, `16px`, `24px`, `48px`, `96px`).\n\
                - **Container Width**: `1200px`–`1280px` max-width centered container with `24px` mobile / `48px` desktop horizontal padding.\n\
                - **Navbar Clearance**: Fixed top navigation bars require `pt-28` (`112px`+) hero top padding so headings never overlap.\n\n\
                ## 5. Elevation & Depth\n\
                | Level | Surface | Border / Shadow Treatment | Use Case |\n\
                | :--- | :--- | :--- | :--- |\n\
                | **Level 0** | `{{colors.canvas}}` | Flat canvas | Root viewport background |\n\
                | **Level 1** | `{{colors.surface-1}}` | `1px solid {{colors.hairline}}` | Feature cards & content panels |\n\
                | **Level 2** | `{{colors.surface-2}}` | `1px solid rgba(255,255,255,0.12)` | Hover states & active controls |\n\
                | **Level 3** | Glass (`backdrop-blur-md`) | `1px solid {{colors.hairline}}` | Sticky navigation & floating modals |\n\n\
                ## 6. Shapes & Border Radius Scale\n\
                - **Controls & Inputs (`sm`)**: `6px` | **Cards (`md`–`lg`)**: `8px`–`12px` | **Badges & Tags (`pill`)**: `9999px`.\n\n\
                ## 7. Component Stylings\n\
                - **Buttons**: Primary (`bg: {{colors.primary}}`, `text: #fff`, `radius: 6px`), Secondary (`bg: {{colors.surface-2}}`, `1px solid {{colors.hairline}}`).\n\
                - **Cards**: `bg: {{colors.surface-1}}`, `padding: 24px`, `radius: 12px`, `border: 1px solid {{colors.hairline}}`.\n\n\
                ## 8. Do's and Don'ts\n\
                - **Do**: Maintain WCAG AA contrast, use `1px solid` hairline borders for containment, and keep component files modular (`<200` lines).\n\
                - **Don't**: Mix conflicting font families, use heavy multi-layer drop shadows on dark surfaces, or crowd sections without breathing room.\n\n\
                ## 9. Responsive Behavior & Agent Prompt Guide\n\
                - **Breakpoints**: Mobile (`390px` single-column), Tablet (`768px` 2-column grid), Desktop (`1280px`–`1440px` full 12-column grid).\n\
                - **Agent Invariant**: Read this file (or root `DESIGN.md`) before generating or styling any UI component or `README.md` showcase.\n",
                project_name, primary_hex, project_name, runtime, decisions_section, primary_hex
            );
            fs::write(design_path, design).ok();
        } else {
            Self::sync_decisions_into_core_docs(workspace_root);
        }

        // 6. implementation.md (Technical Implementation Plan)
        let impl_path = core_dir.join("implementation.md");
        if !impl_path.exists() {
            let imp = format!(
                "---\ntype: Implementation Plan\ntitle: \"Technical Implementation Plan\"\ndescription: \"Technology stack, architecture layout, and milestone roadmap\"\nstatus: stable\n---\n\n\
                # Technical Implementation Plan 🛠️\n\n\
                ## Technology Stack\n\
                - **Runtime**: `{}`\n\n\
                ## Architecture Layout\n\
                {}\n\n\
                ## Milestone Roadmap\n\
                - Structured phases with verifiable acceptance criteria.\n",
                runtime, arch_structure_block
            );
            fs::write(impl_path, imp).ok();
        }

        // 7. content.md (Content & Interface Catalog)
        let content_path = core_dir.join("content.md");
        if !content_path.exists() {
            let content = format!(
                "---\ntype: Reference\ntitle: \"Content & Interface Catalog\"\ndescription: \"User-facing sections, routes, commands, and interface schemas\"\nstatus: stable\n---\n\n\
                # Content & Interface Catalog 📝\n\n\
                ## Interfaces & Endpoints\n\
                - User-facing sections, routes, commands, and schemas for `{}`.\n",
                project_name
            );
            fs::write(content_path, content).ok();
        }

        // 8. todo.md (Task Tracker & Macro Roadmap)
        let todo_path = core_dir.join("todo.md");
        if !todo_path.exists() {
            // Check if an active task_plan.md already exists in the workspace (.minicode/plan/task_plan.md)
            let existing_plan = core_dir
                .parent()
                .and_then(|d| d.parent())
                .map(|ws| ws.join(".minicode").join("plan").join("task_plan.md"))
                .and_then(|p| {
                    if p.exists() {
                        fs::read_to_string(p).ok()
                    } else {
                        None
                    }
                });

            let todo = if let Some(plan_text) = existing_plan {
                format!(
                    "---\ntype: Roadmap\ntitle: \"Task Tracker & Macro Roadmap\"\ndescription: \"Active milestone checklist and task tracker\"\nstatus: stable\n---\n\n# Tasks & Todo: {}\n\n{}\n",
                    project_name,
                    plan_text.trim()
                )
            } else {
                "---\ntype: Roadmap\ntitle: \"Task Tracker & Macro Roadmap\"\ndescription: \"Active milestone checklist and task tracker\"\nstatus: stable\n---\n\n\
                # Task Tracker (todo.md) 📋\n\n\
                ## Active Milestone\n\
                - [x] Initial project setup and architecture sync\n\n\
                > Scaffolding complete. Decompose features into milestones using `create_plan`.\n"
                    .to_string()
            };
            fs::write(&todo_path, &todo).ok();
        }
        let root_todo = docs_dir.join("todo.md");
        if root_todo.exists() {
            let _ = fs::remove_file(root_todo);
        }

        // 9. packages/miniblocks.md (MiniBlocks Native UI Component & Design Warehouse)
        let miniblocks_path = packages_dir.join("miniblocks.md");
        if !miniblocks_path.exists() {
            let miniblocks_content = "---\ntype: Package\ntitle: \"MiniBlocks Native UI Component & Design Warehouse\"\ndescription: \"Local-first UI component and design token warehouse reference\"\nstatus: stable\n---\n\n\
                # MiniBlocks Native UI Component & Design Warehouse 🎨\n\n\
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

        // 10. index.md (Canonical 4-domain knowledge catalog index)
        let index_path = docs_dir.join("index.md");
        if !index_path.exists() {
            let index_content = format!(
                "---\ntype: Directory Index\ntitle: Knowledge & Engineering Catalog Index\n---\n\n\
                # Project Knowledge & Engineering Catalog 📚\n\n\
                > Organized into 4 distinct knowledge domains: **Core Specifications**, **Runtime Workflows**, **Domain Skills**, and **Package References**.\n\n\
                ## 1. Core Engineering Specifications (`core/`) 🏛️\n\
                *Inviolable project contract and single source of truth for minicode.*\n\
                - [Core Philosophy & Vision](./core/coreidea.md)\n\
                - [Product Requirements Document (PRD)](./core/prd.md)\n\
                - [Clean Architecture & CodeGraph](./core/architecture.md)\n\
                - [Technical Invariants & Spec](./core/spec.md)\n\
                - [Design Specification](./core/design.md)\n\
                - [Technical Implementation Plan](./core/implementation.md)\n\
                - [Task Tracker (todo.md)](./core/todo.md)\n\
                - [Content Reference](./core/content.md)\n\n\
                ## 2. Runtime & Stack Workflows (`runtimes/` or `{}/`) ⚙️\n\
                *Language conventions, compilation safety rules, and architecture patterns.*\n\n\
                ## 3. Domain & Procedural Skills (`skills/`) 🧠\n\
                *Autonomous behavioral capabilities, design methodologies, and testing protocols.*\n\n\
                ## 4. Package References & Continuous Learning (`packages/`) 📦\n\
                *Local documentation cheat sheets, API gotchas, and self-improved solutions.*\n",
                runtime
            );
            fs::write(index_path, index_content).ok();
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

        let (_, mut workspace_files, doc_files) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(workspace_root);
        workspace_files.extend(doc_files);
        let synced_skills = crate::vault::store::VaultStore::new(workspace_root)
            .sync_project_active_skills(&workspace_files);
        manifest["active_skills"] = serde_json::json!(synced_skills);

        // Deep architecture detection
        let arch = Self::detect_architecture(workspace_root);
        manifest["architecture"] = arch.clone();

        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap_or_default();
        fs::write(&manifest_path, &manifest_json).ok();

        // Also sync onpkg.json if it already exists for backward compatibility
        let onpkg_json_path = workspace_root.join(crate::constants::ONPKG_MANIFEST_FILE);
        if onpkg_json_path.exists() {
            fs::write(&onpkg_json_path, &manifest_json).ok();
        }

        // 2. Ensure .gitignore protects .minicode/ local runtime & transient memory
        Self::ensure_gitignore(workspace_root);

        // 3. Refresh .minicode/minicode.md if missing
        let docs_dir = super::resolve_docs_dir(workspace_root);
        let docs_dir_name = docs_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("minikit_docs");

        let minicode_md_path = workspace_root.join(crate::constants::MINICODE_MD_FILE);
        if !minicode_md_path.exists() {
            let minicode_md = format!(
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
            let _ = fs::create_dir_all(workspace_root.join(crate::constants::WORKSPACE_DIR_NAME));
            fs::write(&minicode_md_path, &minicode_md).ok();
        }

        // 4. Ensure docs directory exists and synchronize the 8 core workflow documents
        Self::ensure_workflow_docs_with_arch(&docs_dir, project_name, runtime, Some(&arch));

        // Also ensure onpkg_docs exists for backward compatibility if configured
        let onpkg_docs = workspace_root.join(crate::constants::ONPKG_DOCS_DIR);
        if onpkg_docs.exists() {
            Self::ensure_workflow_docs_with_arch(&onpkg_docs, project_name, runtime, Some(&arch));
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
            • Instructions: .minicode/minicode.md\n\
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

    /// Generates an awesome, high-conversion repository `README.md` modeled on `VoltAgent/awesome-design-md`
    /// with a centered hero header, dynamic shield badges, Three-Pillar Repository Contract table,
    /// tech stack breakdown, and runtime-specific quickstart commands.
    pub fn generate_awesome_readme_template(
        project_name: &str,
        runtime: &str,
        stack_name: &str,
        description: &str,
        packages: &[String],
    ) -> String {
        let rt_clean = runtime.trim().to_lowercase();
        let (install_cmd, dev_cmd, test_cmd, build_cmd) = match rt_clean.as_str() {
            "cargo" | "rust" => (
                "cargo fetch",
                "cargo run",
                "cargo test",
                "cargo build --release",
            ),
            "bun" => ("bun install", "bun run dev", "bun test", "bun run build"),
            "pnpm" => ("pnpm install", "pnpm dev", "pnpm test", "pnpm build"),
            "yarn" => ("yarn install", "yarn dev", "yarn test", "yarn build"),
            "uv" | "python" => (
                "uv sync",
                "uv run python main.py",
                "uv run pytest",
                "uv build",
            ),
            "flutter" | "dart" => (
                "flutter pub get",
                "flutter run",
                "flutter test",
                "flutter build",
            ),
            "go" | "golang" => ("go mod download", "go run .", "go test ./...", "go build ."),
            _ => ("npm install", "npm run dev", "npm test", "npm run build"),
        };

        let pkg_rows = if packages.is_empty() {
            format!(
                "| `{}` | Core Runtime & Toolchain | Primary execution environment |",
                runtime
            )
        } else {
            packages
                .iter()
                .take(10)
                .map(|p| {
                    format!(
                        "| `{}` | `{}` Dependency | Core application package |",
                        p, runtime
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let badge_runtime = runtime.replace('-', "_");
        let badge_stack = stack_name.replace('-', "_");

        format!(
            "<div align=\"center\">\n\n\
            <h1>{project_name}</h1>\n\n\
            <p>\n  <strong>{description}</strong>\n</p>\n\n\
            <p>\n  \
              <a href=\"#quickstart\"><img src=\"https://img.shields.io/badge/Runtime-{badge_runtime}-61ffca?style=flat-square\" alt=\"Runtime\" /></a>\n  \
              <a href=\"#architecture\"><img src=\"https://img.shields.io/badge/Stack-{badge_stack}-82e2ff?style=flat-square\" alt=\"Stack\" /></a>\n  \
              <a href=\"minikit_docs/core/design.md\"><img src=\"https://img.shields.io/badge/Design_System-DESIGN.md-a277ff?style=flat-square\" alt=\"DESIGN.md\" /></a>\n  \
              <a href=\".minicode/minicode.md\"><img src=\"https://img.shields.io/badge/Agent_Ready-minicode-ffca85?style=flat-square\" alt=\"minicode\" /></a>\n\
            </p>\n\n\
            <p>\n  \
              <a href=\"#overview\">Overview</a> •\n  \
              <a href=\"#architecture\">Architecture</a> •\n  \
              <a href=\"#directory-structure\">Directory Structure</a> •\n  \
              <a href=\"#tech-stack\">Tech Stack</a> •\n  \
              <a href=\"#quickstart\">Quickstart</a>\n\
            </p>\n\n\
            </div>\n\n\
            ---\n\n\
            ## Overview & Repository Contract (`#overview`)\n\n\
            **{project_name}** is built on the `{stack_name}` architecture (`{runtime}`). This repository follows the Three-Pillar Agent-First Documentation Standard inspired by [VoltAgent/awesome-design-md](https://github.com/VoltAgent/awesome-design-md):\n\n\
            | File | Who Reads It | What It Defines |\n\
            | :--- | :--- | :--- |\n\
            | **[`README.md`](README.md)** | Humans & contributors | Project overview, architecture anatomy, dependency matrix, and quickstart |\n\
            | **[`.minicode/minicode.md`](.minicode/minicode.md)** | Coding agents (`minicode`) | Build/test commands, module conventions, and engineering invariants |\n\
            | **[`DESIGN.md`](minikit_docs/core/design.md)** | Design & UI agents | 9-section visual design system, color/typography tokens, and component styling rules |\n\n\
            ---\n\n\
            ## Architecture & Specifications (`#architecture`)\n\n\
            ```text\n\
            ┌────────────────────────────────────────────────────────────────────────────┐\n\
            │  {project_name:<72} │\n\
            │  Stack: {stack_name:<28} Runtime: {runtime:<24} │\n\
            ├───────────────────────┬──────────────────────────┬─────────────────────────┤\n\
            │  Interface / Entry    │  Domain & Core Logic     │  Specs & Design Tokens  │\n\
            │  src/                 │  Routes / Services       │  minikit_docs/core/     │\n\
            │  minikit.json         │  Typed Data Contracts    │  DESIGN.md (9-Section)  │\n\
            └───────────────────────┴──────────────────────────┴─────────────────────────┘\n\
            ```\n\n\
            | # | Specification | Path | Responsibility |\n\
            | :--- | :--- | :--- | :--- |\n\
            | **1** | **Core Idea** | [`minikit_docs/core/coreidea.md`](minikit_docs/core/coreidea.md) | Vision, problem statement, and core value proposition |\n\
            | **2** | **Product Requirements** | [`minikit_docs/core/prd.md`](minikit_docs/core/prd.md) | User flows, functional requirements, and acceptance criteria |\n\
            | **3** | **System Architecture** | [`minikit_docs/core/architecture.md`](minikit_docs/core/architecture.md) | Clean architecture topology and AST symbol boundaries |\n\
            | **4** | **Design System (`DESIGN.md`)** | [`minikit_docs/core/design.md`](minikit_docs/core/design.md) | 9-section Stitch/VoltAgent color, typography, elevation, and UI tokens |\n\
            | **5** | **Task Roadmap** | [`minikit_docs/core/todo.md`](minikit_docs/core/todo.md) | Phased engineering milestones and verification checklist |\n\n\
            ## Directory Structure (`#directory-structure`)\n\n\
            ```text\n\
            {project_name}/\n\
            ├── src/                        # Application entrypoint, domain modules, and components\n\
            ├── minikit_docs/\n\
            │   ├── core/\n\
            │   │   ├── coreidea.md         # Product vision & value proposition\n\
            │   │   ├── prd.md              # Functional requirements & acceptance criteria\n\
            │   │   ├── architecture.md     # Clean architecture & module boundaries\n\
            │   │   ├── design.md           # 9-section VoltAgent/Stitch DESIGN.md specification\n\
            │   │   └── todo.md             # Phased implementation roadmap\n\
            │   ├── skills/                 # Project-scoped domain skills\n\
            │   └── packages/               # Upstream dependency documentation\n\
            ├── .minicode/\n\
            │   └── minicode.md             # Agent build/test/lint operational guidelines\n\
            ├── minikit.json                # Stack manifest & active skills configuration\n\
            └── README.md                   # Repository overview & quickstart\n\
            ```\n\n\
            ## Tech Stack & Packages (`#tech-stack`)\n\n\
            | Package / Tool | Category | Role |\n\
            | :--- | :--- | :--- |\n\
            {pkg_rows}\n\n\
            ---\n\n\
            ## Quickstart (`#quickstart`)\n\n\
            ```bash\n\
            # 1. Install dependencies\n\
            {install_cmd}\n\n\
            # 2. Start development workflow\n\
            {dev_cmd}\n\n\
            # 3. Run verification & tests\n\
            {test_cmd}\n\n\
            # 4. Build for production\n\
            {build_cmd}\n\
            ```\n"
        )
    }

    /// Formats `.minicode/decisions.md` entries as compact markdown bullets (`- [D1] ...`) without any H1 header.
    #[must_use]
    pub fn format_decision_bullets(workspace_root: &Path) -> String {
        crate::agent::decisions::list_decision_summaries(workspace_root)
            .into_iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Synchronizes recorded user decisions and primary accent hex tokens into existing `minikit_docs/core/` docs.
    pub fn sync_decisions_into_core_docs(workspace_root: &Path) {
        let core_dir = workspace_root
            .join(crate::constants::MINIKIT_DOCS_DIR)
            .join("core");
        if !core_dir.is_dir() {
            return;
        }
        let bullets = Self::format_decision_bullets(workspace_root);
        let primary_hex = crate::agent::decisions::extract_decided_primary_hex(workspace_root);

        for file_name in ["design.md", "prd.md", "coreidea.md"] {
            let path = core_dir.join(file_name);
            let Ok(mut content) = fs::read_to_string(&path) else {
                continue;
            };
            let mut modified = false;

            if file_name == "design.md" {
                if let Some(ref hex) = primary_hex {
                    let default_yaml = format!(
                        "primary: \"{}\"",
                        crate::constants::DEFAULT_DESIGN_PRIMARY_HEX
                    );
                    let updated_yaml = format!("primary: \"{hex}\"");
                    if content.contains(&default_yaml)
                        && hex != crate::constants::DEFAULT_DESIGN_PRIMARY_HEX
                    {
                        content = content.replacen(&default_yaml, &updated_yaml, 1);
                        modified = true;
                    }
                    let default_row = format!(
                        "| `{{colors.primary}}` | `{}` |",
                        crate::constants::DEFAULT_DESIGN_PRIMARY_HEX
                    );
                    let updated_row = format!("| `{{colors.primary}}` | `{hex}` |");
                    if content.contains(&default_row)
                        && hex != crate::constants::DEFAULT_DESIGN_PRIMARY_HEX
                    {
                        content = content.replacen(&default_row, &updated_row, 1);
                        modified = true;
                    }
                }
            }

            if !bullets.is_empty() && !content.contains("## Recorded User Decisions") {
                let anchor = match file_name {
                    "design.md" => Some("\n## 2. Color Palette & Roles"),
                    "prd.md" => Some("\n## Core Objectives & User Stories"),
                    "coreidea.md" => Some("\n## Value Proposition & Tenets"),
                    _ => None,
                };
                if let Some(target_anchor) = anchor {
                    if let Some(pos) = content.find(target_anchor) {
                        let injected = format!("\n## Recorded User Decisions\n{bullets}\n");
                        content.insert_str(pos, &injected);
                        modified = true;
                    }
                }
            }

            if modified {
                let _ = fs::write(&path, content);
            }
        }
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

        // Verify categories exist: core/ and packages/ exist, skills/ is purged
        assert!(docs.join("core").is_dir());
        assert!(!docs.join("skills").exists());
        assert!(docs.join("packages").is_dir());

        // Verify index.md was preserved at root
        assert!(docs.join("index.md").is_file());

        // Verify legacy flat core files were migrated to core/ without data loss
        let migrated_prd = fs::read_to_string(docs.join("core").join("prd.md")).unwrap();
        assert_eq!(migrated_prd, "# Custom PRD");
        assert!(!docs.join("prd.md").exists());

        let migrated_todo = fs::read_to_string(docs.join("core").join("todo.md")).unwrap();
        assert_eq!(migrated_todo, "# Custom Todo");
        assert!(!docs.join("todo.md").exists());

        // Verify non-React projects do NOT get unrequested React package docs polluted into packages/
        assert!(!docs.join("packages").join("minikit_react.md").exists());

        // Verify non-core .md was migrated to packages/
        let migrated_pkg = fs::read_to_string(docs.join("packages").join("rust.md")).unwrap();
        assert_eq!(migrated_pkg, "# Rust Skill");

        // Verify all 8 canonical core files exist
        for core_file in crate::tools::minikit::CORE_DOC_FILES {
            assert!(
                docs.join("core").join(core_file).exists(),
                "Missing core file {}",
                core_file
            );
        }

        // Verify packages/miniblocks.md was generated and skills/ directory does not exist
        assert!(docs.join("packages").join("miniblocks.md").exists());
        assert!(!docs.join("skills").exists());
    }

    #[test]
    fn test_ensure_workflow_docs_populates_only_declared_package_docs() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let docs = ws.join("minikit_docs");
        fs::create_dir_all(&docs).unwrap();

        let manifest = serde_json::json!({
            "name": "web_app",
            "packages": ["react", "lucide-react"],
            "dev_packages": ["@tailwindcss/vite"]
        });
        fs::write(ws.join("minikit.json"), manifest.to_string()).unwrap();

        MiniKitSyncEngine::ensure_workflow_docs(&docs, "web_app", "bun");

        assert!(docs.join("packages").join("minikit_react.md").exists());
        assert!(docs
            .join("packages")
            .join("minikit_lucide-react.md")
            .exists());
        assert!(docs
            .join("packages")
            .join("minikit___tailwindcss__vite.md")
            .exists());
        assert!(!docs.join("packages").join("minikit_zustand.md").exists());
    }

    #[test]
    fn test_design_yaml_indentation_and_decision_sync() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let docs = ws.join("minikit_docs");

        crate::agent::decisions::upsert_decision(
            ws,
            "Brand Accent",
            "Primary accent color is Emerald #10b981",
            None,
        )
        .unwrap();

        MiniKitSyncEngine::ensure_workflow_docs(&docs, "demo_app", "bun");

        let design = fs::read_to_string(docs.join("core").join("design.md")).unwrap();
        assert!(
            design.contains("colors:\n  primary: \"#10b981\""),
            "Expected 2-space indented primary hex in YAML frontmatter, got:\n{design}"
        );
        assert!(
            design.contains("rounded:\n  sm: \"6px\""),
            "Expected 2-space indented rounded keys in YAML frontmatter"
        );
        assert!(
            design.contains("spacing:\n  xs: \"4px\""),
            "Expected 2-space indented spacing keys in YAML frontmatter"
        );
        assert!(
            !design.contains("# Project Decisions"),
            "design.md must not inject duplicate H1 '# Project Decisions' header"
        );
        assert!(design.contains("| `{colors.primary}` | `#10b981` |"));
    }
}
