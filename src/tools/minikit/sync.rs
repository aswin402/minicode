use crate::error::Result;
use std::collections::HashMap;
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

    /// Ensures that `.minicode/` local runtime/transient memory, `screenshots/`, `minikit.json`,
    /// and generated package/catalog docs (`minikit_docs/packages/`, `index.md`, `log.md`) are
    /// excluded in `.gitignore`, while keeping `minikit_docs/core/*.md` trackable by default.
    pub fn ensure_gitignore(workspace_root: &Path) {
        let gitignore_path = workspace_root.join(".gitignore");
        if !gitignore_path.exists() {
            let initial = "# minicode local runtime & transient memory\n.minicode/\nscreenshots/\nminikit_docs/packages/\nminikit_docs/index.md\nminikit_docs/log.md\nminikit.json\n";
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
            let has_exact_rule = |rule: &str| {
                let trimmed_rule = rule.trim_end_matches('/');
                let leading = format!("/{trimmed_rule}");
                let leading_slash = format!("/{rule}");
                content.lines().any(|l| {
                    let t = l.trim();
                    t == rule || t == trimmed_rule || t == leading || t == leading_slash
                })
            };
            let has_minicode = has_dir_rule(".minicode");
            let has_screenshots = has_dir_rule("screenshots");
            let has_whole_minikit_docs = has_exact_rule("minikit_docs/");
            let has_minikit_pkgs = has_whole_minikit_docs || has_dir_rule("minikit_docs/packages");
            let has_minikit_index =
                has_whole_minikit_docs || has_exact_rule("minikit_docs/index.md");
            let has_minikit_log = has_whole_minikit_docs || has_exact_rule("minikit_docs/log.md");
            let has_minikit_json = has_exact_rule("minikit.json");

            if !has_minicode
                || !has_screenshots
                || !has_minikit_pkgs
                || !has_minikit_index
                || !has_minikit_log
                || !has_minikit_json
            {
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
                    (has_minikit_pkgs, "minikit_docs/packages/\n"),
                    (has_minikit_index, "minikit_docs/index.md\n"),
                    (has_minikit_log, "minikit_docs/log.md\n"),
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
                project: \"{}\"\n\
                type: core\n\
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

        // Deep architecture detection + live dependency manifest sync
        let arch = Self::detect_architecture(workspace_root);
        manifest["architecture"] = arch.clone();
        let (live_pkgs, live_dev_pkgs) =
            super::scaffolder::MiniKitScaffolder::extract_dependencies_from_workspace(
                workspace_root,
            );
        if !live_pkgs.is_empty() || workspace_root.join("package.json").exists() {
            manifest["packages"] = serde_json::json!(live_pkgs);
        }
        if !live_dev_pkgs.is_empty() || workspace_root.join("package.json").exists() {
            manifest["dev_packages"] = serde_json::json!(live_dev_pkgs);
        }

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

    /// Inner content width (`73` chars) inside the 78-column README architecture box (`│  ` + 73 + ` │`).
    const README_ARCH_BOX_CONTENT_WIDTH: usize = 73;

    /// Formats a single full-width row inside the 78-column ASCII architecture box (`┌───...───┐`).
    fn format_arch_box_line(text: &str) -> String {
        let clamped =
            crate::utils::strings::truncate_chars(text, Self::README_ARCH_BOX_CONTENT_WIDTH);
        let pad_len = Self::README_ARCH_BOX_CONTENT_WIDTH.saturating_sub(clamped.chars().count());
        format!("│  {clamped}{} │", " ".repeat(pad_len))
    }

    /// Formats the two-column `Stack: ... Runtime: ...` row to exact 78-column box alignment.
    fn format_arch_box_stack_runtime_line(stack_name: &str, runtime: &str) -> String {
        const LEFT_COL_WIDTH: usize = 37;
        let stack_label = format!("Stack: {stack_name}");
        let left = crate::utils::strings::truncate_chars(&stack_label, LEFT_COL_WIDTH);
        let left_pad = LEFT_COL_WIDTH.saturating_sub(left.chars().count());
        Self::format_arch_box_line(&format!("{left}{}Runtime: {runtime}", " ".repeat(left_pad)))
    }

    /// Normalizes a module/import specifier string (e.g. `"@scope/pkg/sub"`, `"gsap/ScrollTrigger"`,
    /// `"lucide_react"`) into its canonical package root without regexes or hardcoded package names.
    fn extract_specifier_package_root(raw_spec: &str) -> Option<String> {
        let spec = raw_spec
            .trim()
            .strip_prefix("node:")
            .or_else(|| raw_spec.trim().strip_prefix("npm:"))
            .unwrap_or_else(|| raw_spec.trim());
        if spec.is_empty()
            || spec.len() > 80
            || spec.starts_with('.')
            || spec.starts_with('/')
            || spec.starts_with('#')
            || spec.starts_with("@/")
            || spec.starts_with("~/")
            || spec.contains("://")
            || spec.chars().any(char::is_whitespace)
        {
            return None;
        }
        let is_valid_pkg_seg = |seg: &str| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if spec.starts_with('@') {
            let mut parts = spec.split('/');
            let scope = parts.next()?;
            let name = parts.next()?;
            if scope.len() > 1 && is_valid_pkg_seg(&scope[1..]) && is_valid_pkg_seg(name) {
                return Some(format!(
                    "{}/{}",
                    scope.to_ascii_lowercase(),
                    name.to_ascii_lowercase()
                ));
            }
            None
        } else {
            let root = spec.split(['/', ':', '?']).next()?;
            if is_valid_pkg_seg(root) {
                Some(root.to_ascii_lowercase())
            } else {
                None
            }
        }
    }

    /// Scans non-ignored workspace source files lexically to count how many distinct modules
    /// import or reference each package root, enabling `README.md` to rank actually used packages
    /// and drop unused starter dependencies with zero hardcoded package names.
    fn compute_workspace_package_import_counts(
        workspace_root: &Path,
        packages: &[String],
    ) -> std::collections::HashMap<String, usize> {
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        if packages.is_empty() || !workspace_root.is_dir() {
            return counts;
        }

        let declared: std::collections::HashSet<String> =
            packages.iter().map(|p| p.to_ascii_lowercase()).collect();

        const SOURCE_EXTS: &[&str] = &[
            "ts", "tsx", "js", "jsx", "mjs", "cjs", "css", "scss", "html", "vue", "svelte",
            "astro", "rs", "py", "go", "dart", "java", "kt", "swift", "rb", "php",
        ];
        const SKIP_DIRS: &[&str] = &[
            "node_modules",
            "dist",
            "dist-ssr",
            "build",
            ".next",
            "target",
            ".git",
            ".minicode",
            "minikit_docs",
            "onpkg_docs",
            "vendor",
            ".venv",
            "venv",
            "__pycache__",
        ];

        let walker = ignore::WalkBuilder::new(workspace_root)
            .hidden(false)
            .parents(!workspace_root.join(".git").exists())
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .require_git(false)
            .max_depth(Some(8))
            .filter_entry(|entry| {
                if entry.depth() > 0 && entry.file_type().is_some_and(|ft| ft.is_dir()) {
                    if let Some(name) = entry.file_name().to_str() {
                        return !SKIP_DIRS.contains(&name);
                    }
                }
                true
            })
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let ext_ok = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| SOURCE_EXTS.contains(&ext.to_ascii_lowercase().as_str()));
            if !ext_ok {
                continue;
            }
            if fs::metadata(path).map_or(true, |m| m.len() > 262_144) {
                continue;
            }
            let Ok(src) = fs::read_to_string(path) else {
                continue;
            };

            let mut file_hits: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut record_candidate = |cand: String| {
                if declared.contains(&cand) {
                    file_hits.insert(cand.clone());
                }
                if cand.contains('_') {
                    let hyphenated = cand.replace('_', "-");
                    if declared.contains(&hyphenated) {
                        file_hits.insert(hyphenated);
                    }
                }
            };

            // 1. Lexical scan of single- and double-quoted string literals (JS/TS/CSS/HTML/Go/Vite)
            let bytes = src.as_bytes();
            let mut i = 0usize;
            while i < bytes.len() {
                let b = bytes[i];
                if b == b'\'' || b == b'"' {
                    let quote = b;
                    i += 1;
                    let start = i;
                    while i < bytes.len() && bytes[i] != quote && bytes[i] != b'\n' {
                        if bytes[i] == b'\\' {
                            i = (i + 2).min(bytes.len());
                        } else {
                            i += 1;
                        }
                    }
                    if i < bytes.len() && bytes[i] == quote && i > start && (i - start) <= 80 {
                        if let Some(root) = Self::extract_specifier_package_root(&src[start..i]) {
                            record_candidate(root);
                        }
                    }
                }
                i += 1;
            }

            // 2. Lexical scan of unquoted `use` / `import` / `from` module statements (Rust/Python/Dart)
            for line in src.lines() {
                let trimmed = line.trim_start();
                let rest = trimmed
                    .strip_prefix("pub use ")
                    .or_else(|| trimmed.strip_prefix("use "))
                    .or_else(|| trimmed.strip_prefix("import "))
                    .or_else(|| trimmed.strip_prefix("from "));
                if let Some(after_kw) = rest {
                    let ident = after_kw
                        .trim_start()
                        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                        .next()
                        .unwrap_or("");
                    if !ident.is_empty() {
                        record_candidate(ident.to_ascii_lowercase());
                    }
                }
            }

            for hit in file_hits {
                *counts.entry(hit).or_insert(0) += 1;
            }
        }

        // Propagate structural prefix-family peer credit (e.g. `foo-dom` / `foo-router` -> base runtime `foo`)
        if !counts.is_empty() {
            let active_roots: Vec<String> = counts
                .iter()
                .filter(|(_, &c)| c > 0)
                .map(|(k, _)| k.clone())
                .collect();
            for pkg in &declared {
                if counts.get(pkg).copied().unwrap_or(0) == 0 && !pkg.contains('/') {
                    let prefix = format!("{pkg}-");
                    if active_roots.iter().any(|r| r.starts_with(&prefix)) {
                        counts.insert(pkg.clone(), 1);
                    }
                }
            }
        }

        counts
    }

    /// Formats live dependency rows for `README.md`, ranking packages by actual workspace source-import
    /// usage when `workspace_root` has source files (dropping unused starter packages) and falling back
    /// to deterministic ordering when no source imports exist yet.
    fn format_package_table_rows_for_workspace(
        workspace_root: Option<&Path>,
        runtime: &str,
        packages: &[String],
    ) -> String {
        if packages.is_empty() {
            return format!(
                "| `{}` | Core Runtime & Toolchain | Primary execution environment |",
                runtime
            );
        }
        let import_counts = workspace_root
            .map(|ws| Self::compute_workspace_package_import_counts(ws, packages))
            .unwrap_or_default();
        let has_measured_imports = import_counts.values().any(|&c| c > 0);

        let mut ordered: Vec<&String> = packages
            .iter()
            .filter(|p| {
                if has_measured_imports {
                    import_counts
                        .get(&p.to_ascii_lowercase())
                        .copied()
                        .unwrap_or(0)
                        > 0
                } else {
                    true
                }
            })
            .collect();

        ordered.sort_by(|a, b| {
            let a_low = a.to_ascii_lowercase();
            let b_low = b.to_ascii_lowercase();
            let is_types = |s: &str| s.starts_with("@types/");
            let is_font = |s: &str| s.starts_with("@fontsource");
            let count_a = import_counts.get(&a_low).copied().unwrap_or(0);
            let count_b = import_counts.get(&b_low).copied().unwrap_or(0);
            is_types(&a_low)
                .cmp(&is_types(&b_low))
                .then_with(|| is_font(&a_low).cmp(&is_font(&b_low)))
                .then_with(|| count_b.cmp(&count_a))
                .then_with(|| a.cmp(b))
        });

        let row_limit = if has_measured_imports { 64 } else { 12 };
        ordered
            .into_iter()
            .take(row_limit)
            .map(|p| {
                format!(
                    "| `{}` | `{}` Dependency | Core application package |",
                    p, runtime
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_package_table_rows(runtime: &str, packages: &[String]) -> String {
        Self::format_package_table_rows_for_workspace(None, runtime, packages)
    }

    /// Synthesizes accurate Quickstart shell commands for `README.md`, inspecting live `package.json`
    /// `"scripts"` when present so repositories without a `"test"` script emit their real verification
    /// command (e.g. `bun run lint`) rather than a failing `bun test`.
    fn format_quickstart_block(workspace_root: Option<&Path>, runtime: &str) -> String {
        let rt_clean = runtime.trim().to_lowercase();
        let (install_cmd, default_dev, default_test, default_build) = match rt_clean.as_str() {
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

        if let Some(ws) = workspace_root {
            let pkg_json = ws.join("package.json");
            if let Ok(raw) = fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(scripts) = val.get("scripts").and_then(|s| s.as_object()) {
                        let run_prefix = match rt_clean.as_str() {
                            "bun" => "bun run",
                            "pnpm" => "pnpm",
                            "yarn" => "yarn",
                            _ => "npm run",
                        };
                        let dev_cmd = if scripts.contains_key("dev") {
                            format!("{run_prefix} dev")
                        } else if scripts.contains_key("start") {
                            format!("{run_prefix} start")
                        } else {
                            default_dev.to_string()
                        };
                        let build_cmd = if scripts.contains_key("build") {
                            format!("{run_prefix} build")
                        } else {
                            default_build.to_string()
                        };
                        let has_real_test = scripts
                            .get("test")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| !s.contains("no test specified"));
                        let verify_step: Option<(&str, String)> = if has_real_test {
                            Some(("Run verification & tests", default_test.to_string()))
                        } else if scripts.contains_key("lint") {
                            Some((
                                "Run lint & static verification",
                                format!("{run_prefix} lint"),
                            ))
                        } else if scripts.contains_key("typecheck") {
                            Some((
                                "Run typecheck verification",
                                format!("{run_prefix} typecheck"),
                            ))
                        } else if scripts.contains_key("check") {
                            Some(("Run verification checks", format!("{run_prefix} check")))
                        } else {
                            None
                        };

                        return if let Some((verify_label, verify_cmd)) = verify_step {
                            format!(
                                "```bash\n\
                                # 1. Install dependencies\n\
                                {install_cmd}\n\n\
                                # 2. Start development workflow\n\
                                {dev_cmd}\n\n\
                                # 3. {verify_label}\n\
                                {verify_cmd}\n\n\
                                # 4. Build for production\n\
                                {build_cmd}\n\
                                ```"
                            )
                        } else {
                            format!(
                                "```bash\n\
                                # 1. Install dependencies\n\
                                {install_cmd}\n\n\
                                # 2. Start development workflow\n\
                                {dev_cmd}\n\n\
                                # 3. Build for production\n\
                                {build_cmd}\n\
                                ```"
                            )
                        };
                    }
                }
            }
        }

        format!(
            "```bash\n\
            # 1. Install dependencies\n\
            {install_cmd}\n\n\
            # 2. Start development workflow\n\
            {dev_cmd}\n\n\
            # 3. Run verification & tests\n\
            {test_cmd}\n\n\
            # 4. Build for production\n\
            {build_cmd}\n\
            ```",
            dev_cmd = default_dev,
            test_cmd = default_test,
            build_cmd = default_build,
        )
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
        let pkg_rows = Self::format_package_table_rows(runtime, packages);
        let quickstart_block = Self::format_quickstart_block(None, runtime);

        let badge_runtime = runtime.replace('-', "_");
        let badge_stack = stack_name.replace('-', "_");
        let box_title_line = Self::format_arch_box_line(project_name);
        let box_meta_line = Self::format_arch_box_stack_runtime_line(stack_name, runtime);

        format!(
            "<div align=\"center\">\n\n\
            <h1>{project_name}</h1>\n\n\
            <p>\n  <strong>{description}</strong>\n</p>\n\n\
            <p>\n  \
              <a href=\"#quickstart\"><img src=\"https://img.shields.io/badge/Runtime-{badge_runtime}-61ffca?style=flat-square\" alt=\"Runtime\" /></a>\n  \
              <a href=\"#architecture\"><img src=\"https://img.shields.io/badge/Stack-{badge_stack}-82e2ff?style=flat-square\" alt=\"Stack\" /></a>\n  \
              <a href=\"minikit_docs/core/design.md\"><img src=\"https://img.shields.io/badge/Design_System-DESIGN.md-a277ff?style=flat-square\" alt=\"DESIGN.md\" /></a>\n  \
              <a href=\"#overview\"><img src=\"https://img.shields.io/badge/Agent_Ready-minicode-ffca85?style=flat-square\" alt=\"minicode\" /></a>\n\
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
            | **`.minicode/minicode.md`** | Coding agents (`minicode`) | Local workspace build/test commands, module conventions, and engineering invariants |\n\
            | **[`DESIGN.md`](minikit_docs/core/design.md)** | Design & UI agents | 9-section visual design system, color/typography tokens, and component styling rules |\n\n\
            ---\n\n\
            ## Architecture & Specifications (`#architecture`)\n\n\
            ```text\n\
            ┌────────────────────────────────────────────────────────────────────────────┐\n\
            {box_title_line}\n\
            {box_meta_line}\n\
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
            {quickstart_block}\n"
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

    /// Strips optional leading YAML frontmatter (`---\n...\n---\n`) to inspect the markdown body.
    fn strip_yaml_frontmatter(doc: &str) -> &str {
        let trimmed = doc.trim_start();
        if let Some(after_open) = trimmed.strip_prefix("---\n") {
            if let Some(close_pos) = after_open.find("\n---\n") {
                return after_open[(close_pos + 5)..].trim_start();
            }
        }
        trimmed
    }

    /// Converts a human-readable project title into a valid lowercase package slug (`a-z0-9-`).
    fn to_package_slug(name: &str) -> String {
        let mut slug = String::with_capacity(name.len());
        let mut prev_hyphen = false;
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
                prev_hyphen = false;
            } else if !slug.is_empty() && !prev_hyphen {
                slug.push('-');
                prev_hyphen = true;
            }
        }
        slug.trim_matches('-').to_string()
    }

    /// Derives a lighter interactive hover hex (`#rrggbb`) by blending 22% toward `#ffffff`.
    fn derive_hover_hex(hex: &str) -> Option<String> {
        let raw = hex.trim().strip_prefix('#')?;
        let expanded = match raw.len() {
            3 if raw.chars().all(|c| c.is_ascii_hexdigit()) => {
                let mut s = String::with_capacity(6);
                for ch in raw.chars() {
                    s.push(ch);
                    s.push(ch);
                }
                s
            }
            6 if raw.chars().all(|c| c.is_ascii_hexdigit()) => raw.to_string(),
            _ => return None,
        };
        let r = u16::from_str_radix(&expanded[0..2], 16).ok()?;
        let g = u16::from_str_radix(&expanded[2..4], 16).ok()?;
        let b = u16::from_str_radix(&expanded[4..6], 16).ok()?;
        let lift = |c: u16| -> u8 { (c + ((255u16.saturating_sub(c)) * 22) / 100).min(255) as u8 };
        Some(format!("#{:02x}{:02x}{:02x}", lift(r), lift(g), lift(b)))
    }

    /// Resolves `var(--token)` and `var(--token, fallback)` expressions inside `value` against `env`
    /// using balanced parenthesis scanning (zero regexes).
    fn resolve_css_var_expr(value: &str, self_key: &str, env: &HashMap<String, String>) -> String {
        let mut out = String::with_capacity(value.len());
        let mut cursor = 0usize;
        let bytes = value.as_bytes();

        while cursor < bytes.len() {
            let Some(rel_pos) = value[cursor..].find("var(") else {
                out.push_str(&value[cursor..]);
                break;
            };
            let var_start = cursor + rel_pos;
            let valid_boundary = var_start == 0
                || !matches!(
                    bytes[var_start - 1],
                    b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_'
                );
            if !valid_boundary {
                out.push_str(&value[cursor..var_start + 4]);
                cursor = var_start + 4;
                continue;
            }
            out.push_str(&value[cursor..var_start]);
            let inner_start = var_start + 4;
            let mut depth = 1usize;
            let mut close_idx = None;
            let mut top_comma = None;
            for (off, ch) in value[inner_start..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close_idx = Some(inner_start + off);
                            break;
                        }
                    }
                    ',' if depth == 1 && top_comma.is_none() => {
                        top_comma = Some(inner_start + off);
                    }
                    _ => {}
                }
            }

            let Some(end_paren) = close_idx else {
                out.push_str(&value[var_start..]);
                break;
            };

            let (ref_raw, fallback_opt) = match top_comma {
                Some(comma_idx) => (
                    &value[inner_start..comma_idx],
                    Some(value[comma_idx + 1..end_paren].trim()),
                ),
                None => (&value[inner_start..end_paren], None),
            };
            let ref_key = ref_raw.trim().to_ascii_lowercase();
            let resolved_piece = env
                .get(&ref_key)
                .filter(|target| ref_key != self_key && !target.contains(self_key))
                .map(String::as_str)
                .or(fallback_opt);

            match resolved_piece {
                Some(rep) if !rep.is_empty() => out.push_str(rep),
                _ => out.push_str(&value[var_start..=end_paren]),
            }
            cursor = end_paren + 1;
        }

        out
    }

    /// Extracts concrete CSS custom property declarations (`--token: value`) from the workspace's
    /// customized stylesheet, stripping comments and resolving transitive `var(--...)` indirection
    /// edges to fixed point (bounded by 8 hops).
    pub(crate) fn extract_workspace_css_variables(
        workspace_root: &Path,
    ) -> HashMap<String, String> {
        let mut candidates: Vec<String> = Vec::new();
        if let Some(styles_path) = Self::detect_architecture(workspace_root)
            .get("styles")
            .and_then(|v| v.as_str())
        {
            candidates.push(styles_path.to_string());
        }
        for fallback in [
            "src/index.css",
            "src/App.css",
            "app/globals.css",
            "src/app/globals.css",
            "style.css",
            "styles.css",
        ] {
            if !candidates.iter().any(|c| c == fallback) {
                candidates.push(fallback.to_string());
            }
        }
        for entry in ignore::WalkBuilder::new(workspace_root)
            .max_depth(Some(3))
            .build()
            .flatten()
        {
            let p = entry.path();
            if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("css") {
                if let Ok(rel) = p.strip_prefix(workspace_root) {
                    let rel_s = rel.to_string_lossy().replace('\\', "/");
                    if !candidates.iter().any(|c| c == &rel_s) {
                        candidates.push(rel_s);
                    }
                }
            }
        }

        let Some((_rel_css, raw_css)) = candidates.into_iter().find_map(|rel| {
            let full = workspace_root.join(&rel);
            let text = fs::read_to_string(full).ok()?;
            (!super::scaffolder::MiniKitScaffolder::is_scaffold_file_content(
                Some(workspace_root),
                &rel,
                text.trim(),
            ))
            .then_some((rel, text))
        }) else {
            return HashMap::new();
        };

        // Strip `/* ... */` block comments cleanly before line scanning
        let mut stripped = String::with_capacity(raw_css.len());
        let mut rest = raw_css.as_str();
        while let Some(start) = rest.find("/*") {
            stripped.push_str(&rest[..start]);
            match rest[(start + 2)..].find("*/") {
                Some(end) => rest = &rest[(start + 2 + end + 2)..],
                None => {
                    rest = "";
                    break;
                }
            }
        }
        stripped.push_str(rest);

        let html_is_dark = fs::read_to_string(workspace_root.join("index.html"))
            .is_ok_and(|h| h.contains("class=\"dark") || h.contains("class='dark"));

        let mut vars = HashMap::new();
        let mut selector_buf = String::new();
        let mut in_standalone_dark_block = false;
        let mut brace_depth = 0usize;

        for line in stripped.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if brace_depth == 0 {
                if let Some((sel_part, _)) = trimmed.split_once('{') {
                    selector_buf.push(' ');
                    selector_buf.push_str(sel_part.trim());
                    let sel = selector_buf.trim();
                    in_standalone_dark_block = sel.contains(".dark") && !sel.contains(":root");
                    selector_buf.clear();
                } else {
                    selector_buf.push(' ');
                    selector_buf.push_str(trimmed);
                }
            } else if let Some(rest_decl) = trimmed.strip_prefix("--") {
                if let Some((raw_k, raw_v)) = rest_decl.split_once(':') {
                    let prop_name = format!("--{}", raw_k.trim().to_ascii_lowercase());
                    let prop_val = raw_v.split(';').next().unwrap_or(raw_v).trim();
                    if !prop_val.is_empty() {
                        if in_standalone_dark_block && !html_is_dark {
                            vars.entry(prop_name)
                                .or_insert_with(|| prop_val.to_string());
                        } else {
                            vars.insert(prop_name, prop_val.to_string());
                        }
                    }
                }
            }

            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth = brace_depth.saturating_sub(1);
                        if brace_depth == 0 {
                            in_standalone_dark_block = false;
                            selector_buf.clear();
                        }
                    }
                    _ => {}
                }
            }
        }

        // Fixed-point relaxation across `var(--...)` dependency edges (bounded by 8 hops)
        for _ in 0..8 {
            let snapshot = vars.clone();
            let mut any_changed = false;
            for (k, v) in &mut vars {
                if v.contains("var(") {
                    let next = Self::resolve_css_var_expr(v, k, &snapshot);
                    if next != *v {
                        *v = next;
                        any_changed = true;
                    }
                }
            }
            if !any_changed {
                break;
            }
        }

        // Retain concrete values (filtering out dangling unresolved pure `var(--missing)` aliases)
        vars.retain(|_, v| {
            let t = v.trim();
            !(t.starts_with("var(") && t.ends_with(')') && !t.contains(','))
        });

        vars
    }

    /// Cleans a CSS `font-family` stack (removing quotes and `var(--...)` indirections) into
    /// `(yaml_font_stack, markdown_table_cell)`.
    fn clean_css_font_stack(raw: &str, mono_single: bool) -> Option<(String, String)> {
        let tokens: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty() && !t.starts_with("var("))
            .map(|t| t.trim_matches(|c| c == '\'' || c == '"').trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            return None;
        }
        let yaml_stack = tokens.join(", ");
        let md_cell = if mono_single {
            format!("`{}`", tokens[0])
        } else {
            tokens
                .iter()
                .take(2)
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        Some((yaml_stack, md_cell))
    }

    /// Updates a 2-space indented YAML frontmatter scalar `  <key>: "<old>"` -> `  <key>: "<new>"` in place.
    fn replace_yaml_quoted_slot(content: &mut String, key: &str, new_val: &str) -> bool {
        let Some(after_open) = content.strip_prefix("---\n") else {
            return false;
        };
        let Some(fm_end_rel) = after_open.find("\n---\n") else {
            return false;
        };
        let fm_end = 4 + fm_end_rel;
        let frontmatter = &content[..fm_end];
        let line_prefix = format!("\n  {key}: \"");
        let Some(rel_idx) = frontmatter.find(&line_prefix) else {
            return false;
        };
        let val_start = rel_idx + line_prefix.len();
        let Some(quote_end_rel) = frontmatter[val_start..]
            .find("\"\n")
            .or_else(|| frontmatter[val_start..].strip_suffix('"').map(|s| s.len()))
        else {
            return false;
        };
        let val_end = val_start + quote_end_rel;
        if content[val_start..val_end].eq_ignore_ascii_case(new_val) {
            return false;
        }
        content.replace_range(val_start..val_end, new_val);
        true
    }

    /// Updates a Markdown color table row `| \`{colors.<key>}\` | \`<old>\` |` in place.
    fn replace_color_table_slot(content: &mut String, token_key: &str, new_val: &str) -> bool {
        let row_prefix = format!("| `{{colors.{token_key}}}` | `");
        let Some(start_idx) = content.find(&row_prefix) else {
            return false;
        };
        let val_start = start_idx + row_prefix.len();
        let Some(end_rel) = content[val_start..].find("` |") else {
            return false;
        };
        let val_end = val_start + end_rel;
        if content[val_start..val_end].eq_ignore_ascii_case(new_val) {
            return false;
        }
        content.replace_range(val_start..val_end, new_val);
        true
    }

    /// Synchronizes color and typography tokens in `minikit_docs/core/design.md`
    /// from recorded user decisions and live resolved CSS custom properties.
    fn sync_design_doc_tokens(
        content: &mut String,
        decided_primary_hex: Option<&str>,
        css_vars: &HashMap<String, String>,
    ) -> bool {
        const COLOR_TOKEN_MAP: &[(&str, bool, &[&str])] = &[
            (
                "canvas",
                true,
                &[
                    "--canvas",
                    "--bg-canvas",
                    "--background",
                    "--color-background",
                    "--bg",
                ],
            ),
            (
                "surface-1",
                true,
                &[
                    "--surface-1",
                    "--surface",
                    "--bg-surface",
                    "--card",
                    "--color-card",
                ],
            ),
            (
                "surface-2",
                true,
                &[
                    "--surface-2",
                    "--secondary",
                    "--color-secondary",
                    "--color-muted",
                ],
            ),
            (
                "surface-3",
                false,
                &[
                    "--surface-3",
                    "--accent-surface",
                    "--popover",
                    "--color-popover",
                ],
            ),
            (
                "primary",
                true,
                &["--primary", "--color-primary", "--brand", "--accent"],
            ),
            (
                "on-primary",
                false,
                &[
                    "--on-primary",
                    "--primary-foreground",
                    "--color-primary-foreground",
                ],
            ),
            (
                "ink",
                true,
                &[
                    "--ink",
                    "--text-primary",
                    "--foreground",
                    "--color-foreground",
                    "--text",
                ],
            ),
            (
                "ink-muted",
                true,
                &["--ink-muted", "--text-secondary", "--secondary-foreground"],
            ),
            (
                "ink-subtle",
                true,
                &[
                    "--ink-subtle",
                    "--muted-foreground",
                    "--color-muted-foreground",
                    "--muted",
                ],
            ),
            (
                "semantic-success",
                false,
                &["--success", "--color-success", "--ok"],
            ),
            (
                "semantic-warning",
                false,
                &["--warning", "--color-warning", "--warn"],
            ),
            (
                "semantic-error",
                false,
                &[
                    "--error",
                    "--destructive",
                    "--color-destructive",
                    "--danger",
                ],
            ),
        ];

        let mut modified = false;
        let mut resolved_primary: Option<String> = None;

        for &(token_key, has_table_row, css_keys) in COLOR_TOKEN_MAP {
            let live_val = match token_key {
                "primary" => decided_primary_hex
                    .map(str::to_string)
                    .or_else(|| css_keys.iter().find_map(|k| css_vars.get(*k).cloned())),
                _ => css_keys.iter().find_map(|k| css_vars.get(*k).cloned()),
            };
            let Some(val) = live_val else {
                continue;
            };
            if token_key == "primary" {
                resolved_primary = Some(val.clone());
            }
            if Self::replace_yaml_quoted_slot(content, token_key, &val) {
                modified = true;
            }
            if has_table_row && Self::replace_color_table_slot(content, token_key, &val) {
                modified = true;
            }
        }

        if let Some(ref prim) = resolved_primary {
            if let Some(hover_hex) = Self::derive_hover_hex(prim) {
                if Self::replace_yaml_quoted_slot(content, "primary-hover", &hover_hex) {
                    modified = true;
                }
            }
        }

        if let Some(border_val) = ["--border", "--color-border", "--line"]
            .iter()
            .find_map(|k| css_vars.get(*k))
        {
            if Self::replace_yaml_quoted_slot(content, "hairline", border_val) {
                modified = true;
            }
            if Self::replace_color_table_slot(content, "hairline", border_val) {
                modified = true;
            }
        }

        let heading_raw = css_vars
            .get("--font-heading")
            .or_else(|| css_vars.get("--font-display"))
            .or_else(|| css_vars.get("--font-sans"));
        if let Some((yaml_h, md_h)) = heading_raw.and_then(|r| Self::clean_css_font_stack(r, false))
        {
            for yaml_key in ["display-xl", "headline"] {
                let needle =
                    format!("  {yaml_key}: {{ fontFamily: \"Inter, system-ui, sans-serif\",");
                let repl = format!("  {yaml_key}: {{ fontFamily: \"{yaml_h}\",");
                if content.contains(&needle) && yaml_h != "Inter, system-ui, sans-serif" {
                    *content = content.replacen(&needle, &repl, 1);
                    modified = true;
                }
            }
            for role_label in ["Display XL", "Section Headline", "Card Title"] {
                let needle = format!("| **{role_label}** | `Inter`, `system-ui` |");
                let repl = format!("| **{role_label}** | {md_h} |");
                if content.contains(&needle) && md_h != "`Inter`, `system-ui`" {
                    *content = content.replacen(&needle, &repl, 1);
                    modified = true;
                }
            }
        }

        let body_raw = css_vars
            .get("--font-body")
            .or_else(|| css_vars.get("--font-sans"));
        if let Some((yaml_b, md_b)) = body_raw.and_then(|r| Self::clean_css_font_stack(r, false)) {
            let needle = "  body: { fontFamily: \"Inter, system-ui, sans-serif\",";
            let repl = format!("  body: {{ fontFamily: \"{yaml_b}\",");
            if content.contains(needle) && yaml_b != "Inter, system-ui, sans-serif" {
                *content = content.replacen(needle, &repl, 1);
                modified = true;
            }
            let row_needle = "| **Body Copy** | `Inter`, `system-ui` |";
            let row_repl = format!("| **Body Copy** | {md_b} |");
            if content.contains(row_needle) && md_b != "`Inter`, `system-ui`" {
                *content = content.replacen(row_needle, &row_repl, 1);
                modified = true;
            }
        }

        if let Some((yaml_m, md_m)) = css_vars
            .get("--font-mono")
            .and_then(|r| Self::clean_css_font_stack(r, true))
        {
            let needle = "  mono: { fontFamily: \"JetBrains Mono, monospace\",";
            let repl = format!("  mono: {{ fontFamily: \"{yaml_m}\",");
            if content.contains(needle) && yaml_m != "JetBrains Mono, monospace" {
                *content = content.replacen(needle, &repl, 1);
                modified = true;
            }
            let row_needle = "| **Code / Telemetry** | `JetBrains Mono` |";
            let row_repl = format!("| **Code / Telemetry** | {md_m} |");
            if content.contains(row_needle) && md_m != "`JetBrains Mono`" {
                *content = content.replacen(row_needle, &row_repl, 1);
                modified = true;
            }
        }

        modified
    }

    /// Returns the completed roadmap tasks when the workspace has a real (non-stub) plan at 100% completion.
    fn completed_plan_tasks(
        workspace_root: &Path,
    ) -> Option<Vec<crate::context::memory::working_memory::TaskItem>> {
        let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_root);
        if !wm.has_active_plan() && !wm.archive_dir().exists() {
            return None;
        }
        let roadmap_tasks: Vec<crate::context::memory::working_memory::TaskItem> = wm
            .read_roadmap_milestones()
            .into_iter()
            .flat_map(|m| m.tasks)
            .collect();
        let tasks = if !roadmap_tasks.is_empty() {
            roadmap_tasks
        } else {
            wm.read_parsed_tasks()
        };
        let is_unplanned_scaffold_stub =
            tasks.len() == 1 && tasks[0].title == "Initial project setup and architecture sync";
        if tasks.is_empty()
            || is_unplanned_scaffold_stub
            || tasks.iter().any(|t| {
                t.status != crate::context::memory::working_memory::TaskItemStatus::Completed
            })
        {
            return None;
        }
        Some(tasks)
    }

    /// Resolves the canonical project heading (`"<Name> — <Tagline>"` or `"<Name>"`) from `index.html`,
    /// `implementation.md`, or a customized `README.md`.
    fn resolve_completed_project_heading(workspace_root: &Path) -> Option<String> {
        let html_title = fs::read_to_string(workspace_root.join("index.html"))
            .ok()
            .and_then(|html| {
                let (_, rest) = html.split_once("<title>")?;
                let (inner, _) = rest.split_once("</title>")?;
                let trimmed = inner.trim();
                if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("minikit ") {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });
        if html_title.is_some() {
            return html_title;
        }

        let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_root);
        let impl_title = fs::read_to_string(wm.canonical_implementation_path())
            .ok()
            .and_then(|doc| {
                doc.lines().find_map(|l| {
                    let trimmed = l.trim();
                    trimmed
                        .strip_prefix("# Implementation Blueprint:")
                        .or_else(|| trimmed.strip_prefix("# Task Plan:"))
                        .or_else(|| {
                            trimmed.strip_prefix("# Technical Implementation Architecture —")
                        })
                        .or_else(|| {
                            trimmed.strip_prefix("# Technical Implementation Architecture:")
                        })
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                })
            });
        if impl_title.is_some() {
            return impl_title;
        }

        fs::read_to_string(workspace_root.join("README.md"))
            .ok()
            .and_then(|readme| {
                if super::scaffolder::MiniKitScaffolder::is_scaffold_file_content(
                    Some(workspace_root),
                    "README.md",
                    readme.trim(),
                ) {
                    return None;
                }
                if let (Some(s), Some(e)) = (readme.find("<h1>"), readme.find("</h1>")) {
                    if s + 4 < e {
                        let h = readme[(s + 4)..e].trim();
                        if !h.is_empty() {
                            return Some(h.to_string());
                        }
                    }
                }
                readme.lines().find_map(|l| {
                    l.trim()
                        .strip_prefix("# ")
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                })
            })
    }

    /// Synchronizes recorded user decisions and live CSS design tokens into existing `minikit_docs/core/` docs.
    pub fn sync_decisions_into_core_docs(workspace_root: &Path) {
        let core_dir = workspace_root
            .join(crate::constants::MINIKIT_DOCS_DIR)
            .join("core");
        if !core_dir.is_dir() {
            return;
        }
        let bullets = Self::format_decision_bullets(workspace_root);
        let primary_hex = crate::agent::decisions::extract_decided_primary_hex(workspace_root);
        let css_vars = Self::extract_workspace_css_variables(workspace_root);

        for file_name in ["design.md", "prd.md", "coreidea.md"] {
            let path = core_dir.join(file_name);
            let Ok(mut content) = fs::read_to_string(&path) else {
                continue;
            };
            let original = content.clone();
            let mut modified = false;

            if file_name == "design.md"
                && Self::sync_design_doc_tokens(&mut content, primary_hex.as_deref(), &css_vars)
            {
                modified = true;
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

            if modified && content != original {
                let _ = fs::write(&path, content);
            }
        }

        Self::sync_unmodified_scaffold_readme(workspace_root, &bullets);

        if Self::completed_plan_tasks(workspace_root).is_some() {
            let resolved_heading = Self::resolve_completed_project_heading(workspace_root);
            let (live_packages, live_dev_packages) =
                super::scaffolder::MiniKitScaffolder::extract_dependencies_from_workspace(
                    workspace_root,
                );
            Self::sync_completed_manifest_and_arch_docs(
                workspace_root,
                resolved_heading.as_deref(),
                &live_packages,
                &live_dev_packages,
            );
        }
    }

    /// Returns `true` if `content` for a canonical `minikit_docs/core/<doc_name>` file is still an
    /// untouched skeleton/boilerplate stub that can safely be hydrated with real project context.
    pub fn is_core_doc_boilerplate(doc_name: &str, content: &str) -> bool {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return true;
        }
        let non_empty_lines = trimmed.lines().filter(|l| !l.trim().is_empty()).count();
        match doc_name.to_ascii_lowercase().as_str() {
            "coreidea.md" => {
                trimmed
                    .contains("Deterministic, fast, and resilient user and developer experience.")
                    && trimmed
                        .contains("Over-engineered complexity or rigid inflexible abstractions.")
            }
            "prd.md" => {
                trimmed.contains("- [ ] **Objective 1**: Core system functionality.")
                    && trimmed.contains(
                        "- [ ] **Objective 2**: Fast, intuitive developer and user experience.",
                    )
            }
            "architecture.md" => (trimmed.contains(
                "Automatically synthesized and maintained by `minicode` AST & CodeGraph Engine.",
            ) && !trimmed.contains("## Active Architectural Blueprint")
                && !trimmed.contains("Clean Architecture Layer Breakdown"))
                || non_empty_lines <= 6,
            "spec.md" => (non_empty_lines <= 24
                && (trimmed.contains("Core invariants, interfaces, and constraints")
                    || trimmed.contains("_Placeholder")
                    || (trimmed.contains("## Core Engineering Invariants")
                        && trimmed.contains(
                            "Component props, state models, routes, and interface contracts for",
                        ))))
                || non_empty_lines <= 8,
            "design.md" => {
                trimmed.contains("High-craft precision interface with generous negative space")
                    && !trimmed.contains("## Project Visual Direction")
            }
            "implementation.md" => {
                trimmed.contains("Code layout, public APIs")
                    || trimmed.contains("Structured phases with verifiable acceptance criteria.")
                    || trimmed.len() < 300
            }
            "content.md" => {
                trimmed.contains("User-facing sections, routes, commands, and schemas for")
                    && non_empty_lines <= 16
            }
            "todo.md" => {
                trimmed.contains("Initial project setup and architecture sync")
                    || trimmed.contains("Scaffolding complete. Decompose features into milestones")
            }
            _ => false,
        }
    }

    /// Bootstraps the full canonical `minikit_docs/core/` suite (`coreidea.md`, `prd.md`,
    /// `architecture.md`, `spec.md`, `design.md`, `implementation.md`, `content.md`, `todo.md`)
    /// whenever `create_plan` or `power_brainstorm` runs on ANY project (new or existing), and
    /// hydrates untouched boilerplate stubs with real plan/project details without ever overwriting
    /// user-authored specs.
    #[allow(clippy::too_many_arguments)]
    pub fn ensure_core_docs_for_plan(
        workspace_root: &Path,
        title: &str,
        steps: &[String],
        architecture: Option<&str>,
        milestones: Option<&[String]>,
        prd_brief: Option<&str>,
        design_brief: Option<&str>,
        target_files: &[String],
    ) -> Vec<String> {
        let docs_dir = super::resolve_docs_dir(workspace_root);
        let core_dir = docs_dir.join("core");
        let (runtime, _pkg_mgr) = Self::detect_runtime(workspace_root);
        let arch_map = Self::detect_architecture(workspace_root);
        let project_name = workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("project")
            .to_string();

        let existed_before: std::collections::HashSet<&'static str> = super::CORE_DOC_FILES
            .iter()
            .copied()
            .filter(|f| core_dir.join(f).is_file())
            .collect();

        // 1. Ensure all 8 canonical core files exist under minikit_docs/core/ (never clobbers existing)
        Self::ensure_workflow_docs_with_arch(&docs_dir, &project_name, runtime, Some(&arch_map));

        let mut touched: Vec<String> = super::CORE_DOC_FILES
            .iter()
            .filter(|f| !existed_before.contains(**f) && core_dir.join(f).is_file())
            .map(|f| (*f).to_string())
            .collect();
        let mut mark_touched = |name: &str| {
            if !touched.iter().any(|t| t == name) {
                touched.push(name.to_string());
            }
        };

        let clean_title = title.trim();
        let clean_steps: Vec<String> = steps
            .iter()
            .map(|s| crate::context::working_memory::WorkingMemory::sanitize_step_title(s))
            .filter(|s| !s.is_empty())
            .collect();
        let decisions = Self::format_decision_bullets(workspace_root);
        let decisions_block = if decisions.is_empty() {
            String::new()
        } else {
            format!("\n## Recorded User Decisions\n{decisions}\n")
        };

        // 2. Hydrate coreidea.md if untouched boilerplate
        let coreidea_path = core_dir.join("coreidea.md");
        if let Ok(existing) = fs::read_to_string(&coreidea_path) {
            if Self::is_core_doc_boilerplate("coreidea.md", &existing) && !clean_title.is_empty() {
                let arch_summary = architecture
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|a| format!("\n## Architectural Direction\n{}\n", a))
                    .unwrap_or_default();
                let milestone_summary = milestones
                    .filter(|ms| !ms.is_empty())
                    .map(|ms| {
                        let bullets = ms
                            .iter()
                            .map(|m| {
                                format!(
                                    "- {}",
                                    crate::context::working_memory::WorkingMemory::sanitize_step_title(m)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        format!("\n## Key Milestones\n{bullets}\n")
                    })
                    .unwrap_or_default();
                let hydrated = format!(
                    "---\ntype: Vision\ntitle: \"Core Vision — {}\"\ndescription: \"Core project vision, value proposition, and scope\"\nstatus: active\n---\n\n\
                    # Core Vision & Philosophy — {}\n\n\
                    ## Core Purpose\n\
                    *Project: `{}` (`{}` runtime) — Goal: {}*\n{}{}{}\n\
                    ## Engineering Tenets\n\
                    - Modular, verifiable architecture with clear separation of concerns.\n\
                    - Evidence-driven implementation verified by automated checks.\n",
                    clean_title.replace('"', "'"),
                    clean_title,
                    project_name,
                    runtime,
                    clean_title,
                    decisions_block,
                    arch_summary,
                    milestone_summary
                );
                if fs::write(&coreidea_path, hydrated).is_ok() {
                    mark_touched("coreidea.md");
                }
            }
        }

        // 3. Hydrate prd.md if untouched boilerplate (or explicit prd_brief passed)
        let prd_path = core_dir.join("prd.md");
        if let Ok(existing) = fs::read_to_string(&prd_path) {
            let is_stub = Self::is_core_doc_boilerplate("prd.md", &existing);
            let explicit_prd = prd_brief.map(str::trim).filter(|s| !s.is_empty());
            if is_stub && (explicit_prd.is_some() || !clean_steps.is_empty()) {
                let objectives = if clean_steps.is_empty() {
                    "- [ ] **Objective 1**: Deliver core project functionality.".to_string()
                } else {
                    clean_steps
                        .iter()
                        .enumerate()
                        .map(|(i, s)| format!("- [ ] **Objective {}**: {}", i + 1, s))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let brief_section = explicit_prd
                    .map(|b| format!("\n## Product Requirements & User Stories\n{b}\n"))
                    .unwrap_or_default();
                let hydrated = format!(
                    "---\ntype: PRD\ntitle: \"Product Requirements Document — {}\"\ndescription: \"Product objectives, requirements, and acceptance criteria\"\nstatus: active\n---\n\n\
                    # Product Requirements Document (PRD) — {}\n\n\
                    ## Project Overview\n\
                    *Project: `{}` (`{}` runtime) — Active Objective: {}*\n{}{}\n\
                    ## Core Objectives & User Stories\n\
                    {}\n\n\
                    ## Success Metrics & Constraints\n\
                    - Build, lint, and automated verification pass cleanly with zero regressions.\n\
                    - All objectives verified against live workspace code and specifications.\n",
                    clean_title.replace('"', "'"),
                    clean_title,
                    project_name,
                    runtime,
                    clean_title,
                    decisions_block,
                    brief_section,
                    objectives
                );
                if fs::write(&prd_path, hydrated).is_ok() {
                    mark_touched("prd.md");
                }
            }
        }

        // 4. Hydrate architecture.md if untouched boilerplate (AST CodeGraph + Active Plan Blueprint)
        let arch_path = core_dir.join("architecture.md");
        if let Ok(existing) = fs::read_to_string(&arch_path) {
            if Self::is_core_doc_boilerplate("architecture.md", &existing) {
                let mut synthesized_base: Option<String> = None;
                let arch_options =
                    crate::context::governance::doc_synthesizer::ArchitectureDocOptions {
                        include_mermaid: true,
                        include_symbol_catalog: true,
                        write_to_file: false,
                    };
                if let Ok(report) =
                    crate::context::governance::doc_synthesizer::ArchitectureDocSynthesizer::synthesize(
                        workspace_root,
                        arch_options,
                    )
                {
                    if report.total_files > 0 {
                        synthesized_base = Some(report.markdown_content);
                    }
                }

                let mut arch_doc = synthesized_base.unwrap_or(existing);
                let has_plan_arch = architecture.map(str::trim).is_some_and(|a| !a.is_empty());
                if has_plan_arch || !target_files.is_empty() || !clean_title.is_empty() {
                    arch_doc.push_str(&format!(
                        "\n## Active Architectural Blueprint — {}\n",
                        clean_title
                    ));
                    if let Some(a) = architecture.map(str::trim).filter(|a| !a.is_empty()) {
                        arch_doc.push_str(&format!("\n{a}\n"));
                    }
                    if !target_files.is_empty() {
                        arch_doc.push_str("\n### Planned Module & File Ownership\n");
                        for tf in target_files {
                            arch_doc.push_str(&format!("- `{tf}`\n"));
                        }
                    }
                    if !decisions.is_empty() {
                        arch_doc.push_str(&format!(
                            "\n### Settled Architectural Decisions\n{decisions}\n"
                        ));
                    }
                }
                if fs::write(&arch_path, arch_doc).is_ok() {
                    mark_touched("architecture.md");
                }
            }
        }

        // 5. Hydrate design.md with explicit design brief if provided and still boilerplate
        let design_path = core_dir.join("design.md");
        if let Some(design_text) = design_brief.map(str::trim).filter(|s| !s.is_empty()) {
            if let Ok(existing) = fs::read_to_string(&design_path) {
                if Self::is_core_doc_boilerplate("design.md", &existing) {
                    let injected = format!(
                        "\n## Project Visual Direction — {}\n{}\n",
                        clean_title, design_text
                    );
                    let updated = if let Some(pos) = existing.find("\n## 2. Color Palette & Roles")
                    {
                        let mut s = existing;
                        s.insert_str(pos, &injected);
                        s
                    } else {
                        format!("{existing}\n{injected}")
                    };
                    if fs::write(&design_path, updated).is_ok() {
                        mark_touched("design.md");
                    }
                }
            }
        }

        // 6. Hydrate content.md if untouched boilerplate and plan lists deliverables/files
        let content_path = core_dir.join("content.md");
        if let Ok(existing) = fs::read_to_string(&content_path) {
            if Self::is_core_doc_boilerplate("content.md", &existing)
                && (!target_files.is_empty() || !clean_steps.is_empty())
            {
                let mut catalog = format!(
                    "---\ntype: Reference\ntitle: \"Content & Interface Catalog — {}\"\ndescription: \"Planned modules, routes, interfaces, and user-facing deliverables\"\nstatus: active\n---\n\n\
                    # Content & Interface Catalog — {}\n\n\
                    *Project: `{}` (`{}` runtime) — Active Objective: {}*\n\n\
                    ## Planned Deliverables & Surfaces\n",
                    clean_title.replace('"', "'"),
                    clean_title,
                    project_name,
                    runtime,
                    clean_title
                );
                for (i, step) in clean_steps.iter().enumerate() {
                    catalog.push_str(&format!("{}. {}\n", i + 1, step));
                }
                if !target_files.is_empty() {
                    catalog.push_str("\n## Key Target Files & Interfaces\n");
                    for tf in target_files {
                        catalog.push_str(&format!("- `{tf}`\n"));
                    }
                }
                if fs::write(&content_path, catalog).is_ok() {
                    mark_touched("content.md");
                }
            }
        }

        // 7. Sync recorded user decisions & live CSS tokens into design.md / prd.md / coreidea.md
        Self::sync_decisions_into_core_docs(workspace_root);

        touched
    }

    /// When a scaffolded workspace finishes its plan with `README.md` still untouched at the
    /// generic starter boilerplate (`<h1>minitest...</h1>` / `Clean Minimal Starter`), enriches
    /// the header title/subtitle from `index.html` or `implementation.md` and appends the
    /// completed roadmap milestones and recorded user decisions.
    fn sync_unmodified_scaffold_readme(workspace_root: &Path, decision_bullets: &str) {
        let readme_path = workspace_root.join("README.md");
        let Ok(mut readme) = fs::read_to_string(&readme_path) else {
            return;
        };
        if !super::scaffolder::MiniKitScaffolder::is_scaffold_file_content(
            Some(workspace_root),
            "README.md",
            readme.trim(),
        ) {
            return;
        }

        let Some(tasks) = Self::completed_plan_tasks(workspace_root) else {
            return;
        };
        let resolved_heading = Self::resolve_completed_project_heading(workspace_root);
        if resolved_heading.is_none() && tasks.is_empty() && decision_bullets.is_empty() {
            return;
        }

        let original_readme = readme.clone();
        let mut modified = false;
        if let Some(ref heading) = resolved_heading {
            let display_h1 = heading
                .split(['—', '–', '|'])
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(heading.as_str());
            if let (Some(h_start), Some(h_end)) = (readme.find("<h1>"), readme.find("</h1>")) {
                if h_start + 4 <= h_end && &readme[(h_start + 4)..h_end] != display_h1 {
                    readme.replace_range((h_start + 4)..h_end, display_h1);
                    modified = true;
                }
            } else if readme.contains("# MiniKit Vite+React Kinetic Template") {
                readme = readme.replacen(
                    "# MiniKit Vite+React Kinetic Template",
                    &format!("# {heading}"),
                    1,
                );
                modified = true;
            }
            if readme.contains("Starter)</strong>") || readme.contains("Template)</strong>") {
                if let (Some(s_start), Some(s_end)) =
                    (readme.find("<strong>"), readme.find("</strong>"))
                {
                    if s_start + 8 <= s_end && &readme[(s_start + 8)..s_end] != heading.as_str() {
                        readme.replace_range((s_start + 8)..s_end, heading);
                        modified = true;
                    }
                }
            }
            if let Some(ws_name) = workspace_root.file_name().and_then(|n| n.to_str()) {
                if ws_name != display_h1 {
                    let raw_bold = format!("**{ws_name}** is built on");
                    if readme.contains(&raw_bold) {
                        readme =
                            readme.replacen(&raw_bold, &format!("**{display_h1}** is built on"), 1);
                        modified = true;
                    }
                    let aligned_ws_box = Self::format_arch_box_line(ws_name);
                    let legacy_ws_box = format!("│  {ws_name:<72} │");
                    let aligned_h1_box = Self::format_arch_box_line(display_h1);
                    if readme.contains(&aligned_ws_box) {
                        readme = readme.replacen(&aligned_ws_box, &aligned_h1_box, 1);
                        modified = true;
                    } else if readme.contains(&legacy_ws_box) {
                        readme = readme.replacen(&legacy_ws_box, &aligned_h1_box, 1);
                        modified = true;
                    }
                    let raw_tree = format!("```text\n{ws_name}/\n");
                    if readme.contains(&raw_tree) {
                        readme =
                            readme.replacen(&raw_tree, &format!("```text\n{display_h1}/\n"), 1);
                        modified = true;
                    }
                }
            }
        }

        // Upgrade any legacy dead `.minicode/minicode.md` hyperlinks (gitignored directory) and
        // misaligned 74/77-column ASCII box rows in older scaffolded READMEs.
        if readme.contains("<a href=\".minicode/minicode.md\">") {
            readme = readme.replace(
                "<a href=\".minicode/minicode.md\">",
                "<a href=\"#overview\">",
            );
            modified = true;
        }
        if readme.contains("**[`.minicode/minicode.md`](.minicode/minicode.md)**") {
            readme = readme.replace(
                "**[`.minicode/minicode.md`](.minicode/minicode.md)**",
                "**`.minicode/minicode.md`**",
            );
            modified = true;
        }
        let repaired_lines: Vec<String> = readme
            .lines()
            .map(|line| {
                if line.starts_with("│  Stack: ")
                    && line.ends_with('│')
                    && line.chars().count() != 78
                {
                    let inner = line.trim_start_matches('│').trim_end_matches('│').trim();
                    if let Some((stack_part, rt_part)) = inner.split_once("Runtime:") {
                        let st = stack_part.trim_start_matches("Stack:").trim();
                        let rt = rt_part.trim();
                        return Self::format_arch_box_stack_runtime_line(st, rt);
                    }
                }
                line.to_string()
            })
            .collect();
        let rejoined = if readme.ends_with('\n') {
            format!("{}\n", repaired_lines.join("\n"))
        } else {
            repaired_lines.join("\n")
        };
        if rejoined != readme {
            readme = rejoined;
            modified = true;
        }

        // Synchronize ASCII box & `## Directory Structure` against live `.gitignore` rules so
        // gitignored local artifacts (`minikit.json`, `.minicode/`, `minikit_docs/packages/`) are never
        // advertised as committed repository files.
        let gi_matcher = {
            let gi_path = workspace_root.join(".gitignore");
            gi_path
                .is_file()
                .then(|| {
                    let mut builder = ignore::gitignore::GitignoreBuilder::new(workspace_root);
                    let _ = builder.add(&gi_path);
                    builder.build().ok()
                })
                .flatten()
        };
        let is_gitignored = |rel: &str, is_dir: bool| -> bool {
            gi_matcher
                .as_ref()
                .is_some_and(|gi| gi.matched(workspace_root.join(rel), is_dir).is_ignore())
        };
        if is_gitignored(".minicode", true) || is_gitignored(".minicode/minicode.md", false) {
            let old_pillar = "| **`.minicode/minicode.md`** | Coding agents (`minicode`) | Local workspace build/test commands, module conventions, and engineering invariants |";
            let new_pillar = "| **[`SPEC.md`](minikit_docs/core/spec.md)** | Coding agents (`minicode`) | Technical specifications, build/test gates, and engineering invariants |";
            if readme.contains(old_pillar) {
                readme = readme.replacen(old_pillar, new_pillar, 1);
                modified = true;
            }
        }
        if is_gitignored("minikit.json", false) && readme.contains("│  minikit.json         │")
        {
            readme = readme.replacen("│  minikit.json         │", "│  Entrypoints & Assets │", 1);
            modified = true;
        }

        let dir_struct_header = "## Directory Structure (`#directory-structure`)\n\n";
        if gi_matcher.is_some() {
            if let Some(ds_pos) = readme.find(dir_struct_header) {
                let block_start = ds_pos + dir_struct_header.len();
                if readme[block_start..].starts_with("```text\n") {
                    if let Some(fence_close) = readme[(block_start + 8)..].find("\n```") {
                        let block_end = (block_start + 8 + fence_close + 4).min(readme.len());
                        let root_label = resolved_heading
                            .as_deref()
                            .map(|h| {
                                h.split(['—', '–', '|'])
                                    .next()
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or(h)
                            })
                            .or_else(|| workspace_root.file_name().and_then(|n| n.to_str()))
                            .unwrap_or("project");
                        let fresh_tree = Self::render_tracked_directory_tree(
                            workspace_root,
                            root_label,
                            &is_gitignored,
                        );
                        if readme[block_start..block_end].trim() != fresh_tree.trim() {
                            readme.replace_range(block_start..block_end, fresh_tree.trim());
                            modified = true;
                        }
                    }
                }
            }
        }

        // Synchronize `## Tech Stack & Packages` and `## Quickstart` against live workspace manifests & imports
        let (detected_rt, detected_pm) = Self::detect_runtime(workspace_root);
        let effective_rt = if readme.contains("Runtime-bun-")
            || workspace_root.join("bun.lock").exists()
            || workspace_root.join("bun.lockb").exists()
        {
            "bun"
        } else if detected_pm != "custom" {
            detected_pm
        } else {
            detected_rt
        };
        let (live_packages, live_dev_packages) =
            super::scaffolder::MiniKitScaffolder::extract_dependencies_from_workspace(
                workspace_root,
            );
        if !live_packages.is_empty() {
            let table_header =
                "## Tech Stack & Packages (`#tech-stack`)\n\n| Package / Tool | Category | Role |\n| :--- | :--- | :--- |\n";
            if let Some(th_pos) = readme.find(table_header) {
                let rows_start = th_pos + table_header.len();
                if let Some(rel_end) = readme[rows_start..].find("\n\n---\n\n## Quickstart") {
                    let fresh_rows = Self::format_package_table_rows_for_workspace(
                        Some(workspace_root),
                        effective_rt,
                        &live_packages,
                    );
                    if readme[rows_start..(rows_start + rel_end)] != fresh_rows {
                        readme.replace_range(rows_start..(rows_start + rel_end), &fresh_rows);
                        modified = true;
                    }
                }
            }
        }

        // Keep `minikit.json` and `minikit_docs/core/architecture.md` synchronized with live
        // workspace dependencies, architecture, and resolved project identity at plan completion.
        Self::sync_completed_manifest_and_arch_docs(
            workspace_root,
            resolved_heading.as_deref(),
            &live_packages,
            &live_dev_packages,
        );

        let quickstart_header = "## Quickstart (`#quickstart`)\n\n";
        if let Some(qs_pos) = readme.find(quickstart_header) {
            let block_start = qs_pos + quickstart_header.len();
            let fresh_qs = format!(
                "{}\n",
                Self::format_quickstart_block(Some(workspace_root), effective_rt)
            );
            if readme[block_start..].starts_with("```bash\n") && readme[block_start..] != fresh_qs {
                if let Some(fence_close) = readme[(block_start + 8)..].find("\n```") {
                    let block_end = (block_start + 8 + fence_close + 4).min(readme.len());
                    let existing_trimmed = readme[block_start..block_end].trim();
                    if existing_trimmed != fresh_qs.trim() {
                        readme.replace_range(block_start..block_end, fresh_qs.trim());
                        modified = true;
                    }
                }
            }
        }

        let mut extra_sections = String::new();
        if !tasks.is_empty() && !readme.contains("## Implemented Milestones") {
            extra_sections.push_str("\n## Implemented Milestones\n\n");
            for (idx, t) in tasks.iter().enumerate() {
                extra_sections.push_str(&format!("- [x] **{}:** {}\n", idx + 1, t.title));
            }
        }
        if !decision_bullets.is_empty() && !readme.contains("## Recorded User Decisions") {
            extra_sections.push_str(&format!(
                "\n## Recorded User Decisions\n\n{decision_bullets}\n"
            ));
        }
        if !extra_sections.is_empty() {
            if let Some(pos) = readme
                .find("\n## Architecture & Specifications")
                .or_else(|| readme.find("\n---\n\n## 🛠️ Technology Stack"))
            {
                readme.insert_str(pos, &extra_sections);
                modified = true;
            }
        }

        if modified && readme != original_readme {
            let _ = fs::write(&readme_path, readme);
        }
    }

    /// Renders the `## Directory Structure` tree block dynamically filtered by `.gitignore` rules
    /// and on-disk directory presence, computing valid `├──` / `└──` connectors at every tree depth.
    fn render_tracked_directory_tree(
        workspace_root: &Path,
        root_label: &str,
        is_gitignored: &dyn Fn(&str, bool) -> bool,
    ) -> String {
        struct TreeNode<'a> {
            name: &'a str,
            comment: &'a str,
            children: Vec<TreeNode<'a>>,
        }
        impl<'a> TreeNode<'a> {
            fn leaf(name: &'a str, comment: &'a str) -> Self {
                Self {
                    name,
                    comment,
                    children: Vec::new(),
                }
            }
            fn dir(name: &'a str, children: Vec<TreeNode<'a>>) -> Self {
                Self {
                    name,
                    comment: "",
                    children,
                }
            }
        }

        let mut docs_children = vec![TreeNode::dir(
            "core/",
            vec![
                TreeNode::leaf("coreidea.md", "Product vision & value proposition"),
                TreeNode::leaf("prd.md", "Functional requirements & acceptance criteria"),
                TreeNode::leaf("architecture.md", "Clean architecture & module boundaries"),
                TreeNode::leaf(
                    "design.md",
                    "9-section VoltAgent/Stitch DESIGN.md specification",
                ),
                TreeNode::leaf("todo.md", "Phased implementation roadmap"),
            ],
        )];
        if workspace_root.join("minikit_docs/skills").is_dir()
            && !is_gitignored("minikit_docs/skills", true)
        {
            docs_children.push(TreeNode::leaf("skills/", "Project-scoped domain skills"));
        }
        if !is_gitignored("minikit_docs/packages", true) {
            docs_children.push(TreeNode::leaf(
                "packages/",
                "Upstream dependency documentation",
            ));
        }

        let mut top_items = vec![
            TreeNode::leaf(
                "src/",
                "Application entrypoint, domain modules, and components",
            ),
            TreeNode::dir("minikit_docs/", docs_children),
        ];
        if !is_gitignored(".minicode", true) && !is_gitignored(".minicode/minicode.md", false) {
            top_items.push(TreeNode::dir(
                ".minicode/",
                vec![TreeNode::leaf(
                    "minicode.md",
                    "Agent build/test/lint operational guidelines",
                )],
            ));
        }
        if !is_gitignored("minikit.json", false) {
            top_items.push(TreeNode::leaf(
                "minikit.json",
                "Stack manifest & active skills configuration",
            ));
        }
        top_items.push(TreeNode::leaf(
            "README.md",
            "Repository overview & quickstart",
        ));

        fn emit_tree<'a>(nodes: &[TreeNode<'a>], indent: &str, out: &mut Vec<String>) {
            let len = nodes.len();
            for (idx, node) in nodes.iter().enumerate() {
                let is_last = idx + 1 == len;
                let branch = if is_last { "└── " } else { "├── " };
                let label_col = format!("{indent}{branch}{}", node.name);
                if node.comment.is_empty() {
                    out.push(label_col);
                } else {
                    let pad = 32usize.saturating_sub(label_col.chars().count()).max(2);
                    out.push(format!("{label_col}{:pad$}# {}", "", node.comment));
                }
                if !node.children.is_empty() {
                    let child_indent = format!("{indent}{}", if is_last { "    " } else { "│   " });
                    emit_tree(&node.children, &child_indent, out);
                }
            }
        }

        let mut lines = vec![format!("```text\n{root_label}/")];
        emit_tree(&top_items, "", &mut lines);
        lines.push("```".to_string());
        lines.join("\n")
    }

    /// Synchronizes `minikit.json` and `minikit_docs/core/architecture.md` with live workspace
    /// dependencies, architecture topology, and resolved project identity upon plan completion,
    /// writing to disk strictly when content bytes differ.
    fn sync_completed_manifest_and_arch_docs(
        workspace_root: &Path,
        resolved_heading: Option<&str>,
        live_packages: &[String],
        live_dev_packages: &[String],
    ) {
        let ws_name = workspace_root.file_name().and_then(|n| n.to_str());
        let display_h1 = resolved_heading.map(|h| {
            h.split(['—', '–', '|'])
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(h)
        });

        let import_counts =
            Self::compute_workspace_package_import_counts(workspace_root, live_packages);
        let has_measured_imports = import_counts.values().any(|&c| c > 0);
        let active_packages: Vec<String> = if has_measured_imports {
            live_packages
                .iter()
                .filter(|p| {
                    import_counts
                        .get(&p.to_ascii_lowercase())
                        .copied()
                        .unwrap_or(0)
                        > 0
                })
                .cloned()
                .collect()
        } else {
            live_packages.to_vec()
        };

        let manifest_path = workspace_root.join(crate::constants::MINIKIT_MANIFEST_FILE);
        if let Ok(existing_raw) = fs::read_to_string(&manifest_path) {
            if let Ok(mut manifest) = serde_json::from_str::<serde_json::Value>(&existing_raw) {
                let has_pkg_json = workspace_root.join("package.json").exists();
                if !active_packages.is_empty() || has_pkg_json {
                    manifest["packages"] = serde_json::json!(active_packages);
                }
                if !live_dev_packages.is_empty() || has_pkg_json {
                    manifest["dev_packages"] = serde_json::json!(live_dev_packages);
                }
                let arch = Self::detect_architecture(workspace_root);
                if arch.as_object().is_some_and(|o| !o.is_empty()) {
                    manifest["architecture"] = arch;
                }
                if let (Some(ws_n), Some(h1), Some(full_h)) =
                    (ws_name, display_h1, resolved_heading)
                {
                    if manifest.get("name").and_then(|v| v.as_str()) == Some(ws_n) && ws_n != h1 {
                        manifest["name"] = serde_json::json!(h1);
                    }
                    if manifest
                        .get("description")
                        .and_then(|v| v.as_str())
                        .is_some_and(|d| d.ends_with("Starter)") || d.ends_with("Template)"))
                    {
                        manifest["description"] = serde_json::json!(full_h);
                    }
                }
                if let Ok(serialized) = serde_json::to_string_pretty(&manifest) {
                    if serialized.trim() != existing_raw.trim() {
                        let _ = fs::write(&manifest_path, format!("{serialized}\n"));
                    }
                }
            }
        }

        let core_dir = workspace_root
            .join(crate::constants::MINIKIT_DOCS_DIR)
            .join("core");

        // Synchronize starter `package.json` `"name"` slug when it still holds a starter template or directory placeholder
        if let Some(h1) = display_h1 {
            let pkg_json_path = workspace_root.join("package.json");
            let slug = Self::to_package_slug(h1);
            if !slug.is_empty() && pkg_json_path.is_file() {
                if let Ok(raw_pkg) = fs::read_to_string(&pkg_json_path) {
                    if let Ok(parsed_pkg) = serde_json::from_str::<serde_json::Value>(&raw_pkg) {
                        if let Some(curr_name) = parsed_pkg.get("name").and_then(|v| v.as_str()) {
                            let is_placeholder_name = ws_name.is_some_and(|w| curr_name == w)
                                || super::scaffolder::MiniKitScaffolder::is_builtin_starter_package_name(curr_name);
                            if is_placeholder_name && curr_name != slug {
                                let needle = format!("\"name\": \"{curr_name}\"");
                                let repl = format!("\"name\": \"{slug}\"");
                                if raw_pkg.contains(&needle) {
                                    let updated_pkg = raw_pkg.replacen(&needle, &repl, 1);
                                    if updated_pkg != raw_pkg {
                                        let _ = fs::write(&pkg_json_path, updated_pkg);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if !core_dir.is_dir() {
            return;
        }

        // Refresh `architecture.md` from the live AST CodeGraph whenever it is starter text or
        // machine-synthesized by `ArchitectureDocSynthesizer` (preserving hand-authored custom docs).
        let arch_path = core_dir.join("architecture.md");
        if let Ok(existing_arch) = fs::read_to_string(&arch_path) {
            let arch_body = Self::strip_yaml_frontmatter(&existing_arch);
            let is_machine_managed_arch = arch_body
                .starts_with("# Clean Architecture & AST CodeGraph Overview:")
                || arch_body.starts_with("# Architecture & CodeGraph:")
                || arch_body.contains("Starter Architecture")
                || arch_body
                    .contains("Automatically synthesized by `minicode` AST & CodeGraph Engine");
            if is_machine_managed_arch {
                let arch_options =
                    crate::context::governance::doc_synthesizer::ArchitectureDocOptions {
                        include_mermaid: true,
                        include_symbol_catalog: true,
                        write_to_file: false,
                    };
                if let Ok(report) =
                    crate::context::governance::doc_synthesizer::ArchitectureDocSynthesizer::synthesize(
                        workspace_root,
                        arch_options,
                    )
                {
                    let mut next_arch = report.markdown_content;
                    if let (Some(ws_n), Some(h1)) = (ws_name, display_h1) {
                        if ws_n != h1 {
                            next_arch = next_arch.replace(
                                &format!("Architecture Documentation: {ws_n}"),
                                &format!("Architecture Documentation: {h1}"),
                            );
                            next_arch = next_arch.replace(&format!("`{ws_n}`"), &format!("`{h1}`"));
                        }
                    }
                    if next_arch.trim() != existing_arch.trim() {
                        let _ = fs::write(&arch_path, next_arch);
                    }
                }
            }
        }

        // Propagate resolved project identity (`ws_name` -> `display_h1`) across all core spec docs
        if let (Some(ws_n), Some(h1)) = (ws_name, display_h1) {
            if ws_n != h1 {
                let yaml_proj_old = format!("project: \"{ws_n}\"");
                let yaml_proj_new = format!("project: \"{h1}\"");
                let backtick_old = format!("`{ws_n}`");
                let backtick_new = format!("`{h1}`");
                let todo_h1_old = format!("# Tasks & Todo: {ws_n}");
                let todo_h1_new = format!("# Tasks & Todo: {h1}");

                if let Ok(entries) = fs::read_dir(&core_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("md") {
                            continue;
                        }
                        let Ok(existing_doc) = fs::read_to_string(&path) else {
                            continue;
                        };
                        let updated_doc = existing_doc
                            .replace(&yaml_proj_old, &yaml_proj_new)
                            .replace(&backtick_old, &backtick_new)
                            .replace(&todo_h1_old, &todo_h1_new);
                        if updated_doc != existing_doc {
                            let _ = fs::write(&path, updated_doc);
                        }
                    }
                }
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

    #[test]
    fn test_sync_unmodified_scaffold_readme_upgrades_starter_without_overwriting_custom() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let docs = ws.join("minikit_docs");
        MiniKitSyncEngine::ensure_workflow_docs(&docs, "minitest57", "bun");

        let starter_readme = MiniKitSyncEngine::generate_awesome_readme_template(
            "minitest57",
            "bun",
            "react-vite",
            "React 19 + Vite 8 + Tailwind 4 + React Router 7 + Lucide Icons (Clean Minimal Starter)",
            &["react".to_string(), "lucide-react".to_string()],
        );
        assert!(
            super::super::scaffolder::MiniKitScaffolder::is_scaffold_file_content(
                Some(ws),
                "README.md",
                starter_readme.trim()
            )
        );
        fs::write(ws.join("README.md"), &starter_readme).unwrap();
        fs::write(
            ws.join("index.html"),
            "<!doctype html><html><head><title>minicode — Fast, Minimalist Terminal Coding Agent</title></head><body></body></html>",
        )
        .unwrap();

        let wm = crate::context::memory::working_memory::WorkingMemory::new(ws);
        wm.init_two_tier_plan(
            "minicode landing page",
            &[
                "Scaffold stack & theme tokens".to_string(),
                "Build Hero & Swarm DAG sections".to_string(),
            ],
            None,
            None,
        )
        .unwrap();
        wm.update_progress("1", "completed").unwrap();
        wm.update_progress("2", "completed").unwrap();

        let enriched = fs::read_to_string(ws.join("README.md")).unwrap();
        assert!(enriched.contains("<h1>minicode</h1>"), "got:\n{enriched}");
        assert!(!enriched.contains("(Clean Minimal Starter)"));
        assert!(enriched.contains("## Implemented Milestones"));
        assert!(enriched.contains("Build Hero & Swarm DAG sections"));

        // Custom README.md must NEVER be overwritten by sync_decisions_into_core_docs
        let custom_readme = "# Custom Handcrafted README\n\nCustom docs.\n";
        fs::write(ws.join("README.md"), custom_readme).unwrap();
        MiniKitSyncEngine::sync_decisions_into_core_docs(ws);
        assert_eq!(
            fs::read_to_string(ws.join("README.md")).unwrap(),
            custom_readme
        );
    }

    #[test]
    fn test_sync_unmodified_scaffold_readme_defers_mid_plan_and_records_macro_milestones() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("minitest58");
        fs::create_dir_all(&ws).unwrap();
        let docs = ws.join("minikit_docs");
        MiniKitSyncEngine::ensure_workflow_docs(&docs, "minitest58", "bun");

        let kinetic_readme = MiniKitSyncEngine::generate_awesome_readme_template(
            "minitest58",
            "bun",
            "react-vite-gsap",
            "React 19 + Vite 8 + Tailwind 4 + GSAP + Framer Motion + Lenis + shadcn/ui + Lordicon (Kinetic Motion Template)",
            &["react".to_string(), "gsap".to_string()],
        );
        fs::write(
            ws.join("minikit.json"),
            serde_json::json!({
                "name": "minitest58",
                "stack": "react-vite-gsap",
                "package_manager": "bun",
                "packages": ["react", "gsap"]
            })
            .to_string(),
        )
        .unwrap();
        fs::write(ws.join("README.md"), &kinetic_readme).unwrap();

        // Recording decisions BEFORE a plan exists must NOT prematurely mutate README.md
        crate::agent::decisions::upsert_decision(
            &ws,
            "Stack",
            "react-vite-gsap Kinetic Motion stack",
            None,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(ws.join("README.md")).unwrap(),
            kinetic_readme
        );

        fs::write(
            ws.join("index.html"),
            "<!doctype html><html><head><title>minicode — Fast, Minimalist Terminal Coding Agent</title></head><body></body></html>",
        )
        .unwrap();

        let wm = crate::context::memory::working_memory::WorkingMemory::new(&ws);
        wm.init_two_tier_plan_with_substeps(
            "minicode",
            &[
                "Foundation — theme tokens & layout shell".to_string(),
                "Hero & verification".to_string(),
            ],
            &[
                vec![
                    "bun add @fontsource-variable/jetbrains-mono".to_string(),
                    "Rewrite src/index.css".to_string(),
                ],
                Vec::new(),
            ],
            None,
            None,
        )
        .unwrap();

        // Completing Step 1's substeps (1/2 macro steps done) must NOT freeze README.md mid-plan!
        wm.update_progress("1", "completed").unwrap();
        wm.update_progress("2", "completed").unwrap();
        assert_eq!(
            fs::read_to_string(ws.join("README.md")).unwrap(),
            kinetic_readme,
            "README.md must stay untouched while later macro milestones are still incomplete"
        );

        // Completing final Macro Step 2 (2/2) enriches README.md with macro milestones, never Step 1's substeps
        wm.update_progress("active", "completed").unwrap();
        let final_readme = fs::read_to_string(ws.join("README.md")).unwrap();
        assert!(
            final_readme.contains("<h1>minicode</h1>"),
            "got:\n{final_readme}"
        );
        assert!(!final_readme.contains("(Kinetic Motion Template)"));
        assert!(final_readme.contains("**minicode** is built on"));
        assert!(final_readme.contains("Foundation — theme tokens & layout shell"));
        assert!(final_readme.contains("Hero & verification"));
        assert!(!final_readme.contains("bun add @fontsource-variable/jetbrains-mono"));

        // Re-running sync when README content already matches MUST NOT rewrite or bump mtime!
        let mtime_before = fs::metadata(ws.join("README.md"))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        MiniKitSyncEngine::sync_unmodified_scaffold_readme(&ws, "");
        let mtime_after = fs::metadata(ws.join("README.md"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            mtime_before, mtime_after,
            "Idempotent sync_unmodified_scaffold_readme must not rewrite identical README.md bytes"
        );
    }

    #[test]
    fn test_minitest66_readme_sync_refreshes_live_deps_lint_quickstart_and_box_title() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("minitest66");
        fs::create_dir_all(ws.join("minikit_docs/core")).unwrap();

        // Initial scaffold README generated with `minitest66`, `axios`/`gsap`, and `bun test`
        let initial_readme = MiniKitSyncEngine::generate_awesome_readme_template(
            "minitest66",
            "bun",
            "react-vite-gsap",
            "minitest66 (Kinetic Motion Template)",
            &[
                "axios".to_string(),
                "gsap".to_string(),
                "framer-motion".to_string(),
                "react".to_string(),
            ],
        );
        fs::write(ws.join("README.md"), &initial_readme).unwrap();
        fs::write(
            ws.join("index.html"),
            "<!doctype html><html><head><title>minicode — Autonomous coding agent</title></head></html>",
        )
        .unwrap();
        // Agent removed `axios`/`gsap`/`framer-motion` and `package.json` has `lint` (no `test` script)
        fs::write(
            ws.join("package.json"),
            r#"{
  "dependencies": {
    "@fontsource-variable/inter": "^5.2.8",
    "lenis": "^1.3.23",
    "lucide-react": "^1.17.0",
    "react": "^19.2.4",
    "tailwindcss": "^4.2.2",
    "zustand": "^5.0.12"
  },
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "lint": "eslint ."
  }
}"#,
        )
        .unwrap();
        fs::write(ws.join("bun.lock"), "").unwrap();

        let wm = crate::context::memory::working_memory::WorkingMemory::new(&ws);
        wm.init_two_tier_plan_with_substeps(
            "minicode",
            &["Build landing page".to_string()],
            &[Vec::new()],
            None,
            None,
        )
        .unwrap();
        wm.update_progress("1", "completed").unwrap();

        let synced_readme = fs::read_to_string(ws.join("README.md")).unwrap();
        assert!(synced_readme.contains("<h1>minicode</h1>"));
        // ASCII box and directory tree use `minicode`, not `minitest66`
        assert!(!synced_readme.contains("│  minitest66 "));
        assert!(synced_readme.contains("│  minicode "));
        assert!(synced_readme.contains("```text\nminicode/\n"));
        // Removed dependencies (`axios`, `gsap`, `framer-motion`) are purged; live deps appear
        assert!(!synced_readme.contains("`axios`"));
        assert!(!synced_readme.contains("`gsap`"));
        assert!(!synced_readme.contains("`framer-motion`"));
        assert!(synced_readme.contains("`lucide-react`"));
        assert!(synced_readme.contains("`tailwindcss`"));
        // Quickstart uses `bun run lint` instead of non-existent `bun test`
        assert!(!synced_readme.contains("bun test"));
        assert!(synced_readme.contains("bun run lint"));
    }

    #[test]
    fn test_minitest67_readme_box_alignment_import_usage_ranking_no_dead_links_and_manifest_sync() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("minitest67");
        fs::create_dir_all(ws.join("src/components/landing")).unwrap();
        fs::create_dir_all(ws.join("minikit_docs/core")).unwrap();

        let starter_readme = MiniKitSyncEngine::generate_awesome_readme_template(
            "minitest67",
            "bun",
            "react-vite-gsap",
            "React 19 + Vite 8 + Tailwind 4 + GSAP + Lordicon (Kinetic Motion Template)",
            &[
                "@hookform/resolvers".to_string(),
                "@lordicon/react".to_string(),
                "@tanstack/react-query".to_string(),
                "axios".to_string(),
                "gsap".to_string(),
            ],
        );
        // 1. Every ASCII box line in generated README must have exact 78-char width and no dead `.minicode/` links
        for box_line in starter_readme.lines().filter(|l| {
            l.starts_with(['┌', '│', '├', '└'])
                && !l.starts_with("├──")
                && !l.starts_with("└──")
                && !l.starts_with("│   ")
        }) {
            assert_eq!(
                box_line.chars().count(),
                78,
                "Misaligned ASCII box line ({} cols): {box_line}",
                box_line.chars().count()
            );
        }
        assert!(!starter_readme.contains("href=\".minicode/minicode.md\""));
        assert!(!starter_readme.contains("](.minicode/minicode.md)"));

        fs::write(ws.join("README.md"), &starter_readme).unwrap();
        fs::write(
            ws.join("minikit.json"),
            serde_json::json!({
                "name": "minitest67",
                "stack": "react-vite-gsap",
                "description": "React 19 + Vite 8 + Tailwind 4 + GSAP + Lordicon (Kinetic Motion Template)",
                "packages": ["axios", "@lordicon/react"]
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            ws.join("package.json"),
            r#"{
  "dependencies": {
    "@fontsource-variable/inter": "^5.2.8",
    "@fontsource-variable/jetbrains-mono": "^5.3.0",
    "@hookform/resolvers": "^5.2.2",
    "@lordicon/react": "^1.11.0",
    "@tailwindcss/vite": "^4.2.2",
    "@tanstack/react-query": "^5.95.2",
    "axios": "^1.13.6",
    "gsap": "^3.15.0",
    "lenis": "^1.3.23",
    "lucide-react": "^1.17.0",
    "react": "^19.2.4",
    "react-dom": "^19.2.4",
    "react-router-dom": "^7.13.2",
    "tailwindcss": "^4.2.2"
  },
  "scripts": { "dev": "vite", "build": "tsc -b && vite build", "lint": "eslint ." }
}"#,
        )
        .unwrap();
        fs::write(ws.join("bun.lock"), "").unwrap();
        fs::write(
            ws.join("index.html"),
            "<!doctype html><html><head><title>minicode — the autonomous coding agent</title></head></html>",
        )
        .unwrap();
        fs::write(
            ws.join("src/index.css"),
            "@import \"tailwindcss\";\n@import \"@fontsource-variable/inter\";\n@import \"@fontsource-variable/jetbrains-mono\";\n",
        )
        .unwrap();
        fs::write(
            ws.join("src/main.tsx"),
            "import { createRoot } from 'react-dom/client';\nimport { BrowserRouter } from 'react-router-dom';\n",
        )
        .unwrap();
        fs::write(
            ws.join("src/components/landing/Hero.tsx"),
            "import { ArrowRight } from 'lucide-react';\nimport { gsap } from 'gsap';\nimport Lenis from 'lenis';\n",
        )
        .unwrap();
        fs::write(
            ws.join("vite.config.ts"),
            "import tailwindcss from '@tailwindcss/vite';\nexport default {};\n",
        )
        .unwrap();

        let wm = crate::context::memory::working_memory::WorkingMemory::new(&ws);
        wm.init_two_tier_plan_with_substeps(
            "minicode landing page",
            &["Build landing page".to_string()],
            &[Vec::new()],
            None,
            None,
        )
        .unwrap();
        wm.update_progress("1", "completed").unwrap();

        let synced_readme = fs::read_to_string(ws.join("README.md")).unwrap();
        for box_line in synced_readme.lines().filter(|l| {
            l.starts_with(['┌', '│', '├', '└'])
                && !l.starts_with("├──")
                && !l.starts_with("└──")
                && !l.starts_with("│   ")
        }) {
            assert_eq!(
                box_line.chars().count(),
                78,
                "Post-sync ASCII box line must stay 78 cols: {box_line}"
            );
        }
        // Unused starter packages (`@lordicon/react`, `axios`, `@hookform/resolvers`, `@tanstack/react-query`) are excluded
        assert!(
            !synced_readme.contains("`@lordicon/react`"),
            "got:\n{synced_readme}"
        );
        assert!(!synced_readme.contains("`axios`"), "got:\n{synced_readme}");
        assert!(
            !synced_readme.contains("`@hookform/resolvers`"),
            "got:\n{synced_readme}"
        );
        assert!(
            !synced_readme.contains("`@tanstack/react-query`"),
            "got:\n{synced_readme}"
        );
        // Imported packages (`lucide-react`, `react`, `tailwindcss`, `@fontsource-variable/jetbrains-mono`) appear
        assert!(
            synced_readme.contains("`lucide-react`"),
            "got:\n{synced_readme}"
        );
        assert!(synced_readme.contains("`react`"), "got:\n{synced_readme}");
        assert!(
            synced_readme.contains("`tailwindcss`"),
            "got:\n{synced_readme}"
        );
        assert!(
            synced_readme.contains("`@fontsource-variable/jetbrains-mono`"),
            "got:\n{synced_readme}"
        );

        // `minikit.json` is updated with `"name": "minicode"` and `@fontsource-variable/jetbrains-mono`
        let synced_manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(ws.join("minikit.json")).unwrap()).unwrap();
        assert_eq!(synced_manifest["name"], "minicode");
        let pkgs = synced_manifest["packages"].as_array().unwrap();
        assert!(pkgs
            .iter()
            .any(|v| v == "@fontsource-variable/jetbrains-mono"));
    }

    #[test]
    fn test_minitest68_sync_synthesizes_arch_css_tokens_core_identity_and_uncapped_imports() {
        let parent = tempdir().unwrap();
        let ws = parent.path().join("minitest68");
        fs::create_dir_all(ws.join("src/components")).unwrap();

        let all_deps: Vec<String> = vec![
            "react",
            "react-dom",
            "lucide-react",
            "clsx",
            "tailwind-merge",
            "class-variance-authority",
            "@base-ui/react",
            "shadcn",
            "tw-animate-css",
            "tailwindcss",
            "@fontsource-variable/inter",
            "@fontsource-variable/jetbrains-mono",
            "@fontsource-variable/space-grotesk",
            "axios",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();

        MiniKitSyncEngine::ensure_workflow_docs(&ws.join("minikit_docs"), "minitest68", "bun");
        let starter_readme = MiniKitSyncEngine::generate_awesome_readme_template(
            "minitest68",
            "bun",
            "react-vite",
            "Vite + React 19 + TypeScript + Tailwind CSS v4 (Clean Minimal Starter)",
            &all_deps,
        );
        fs::write(ws.join("README.md"), starter_readme).unwrap();
        fs::write(
            ws.join("minikit.json"),
            r#"{
  "name": "minitest68",
  "stack": "react-vite",
  "runtime": "bun",
  "package_manager": "bun",
  "description": "Vite + React 19 + TypeScript + Tailwind CSS v4 (Clean Minimal Starter)",
  "packages": ["react"],
  "dev_packages": ["vite"]
}
"#,
        )
        .unwrap();
        fs::write(
            ws.join("package.json"),
            r#"{
  "name": "minikit-vite-react",
  "private": true,
  "version": "0.1.0",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build"
  },
  "dependencies": {
    "@base-ui/react": "^1.0.0",
    "@fontsource-variable/inter": "^5.0.0",
    "@fontsource-variable/jetbrains-mono": "^5.0.0",
    "@fontsource-variable/space-grotesk": "^5.0.0",
    "axios": "^1.7.0",
    "class-variance-authority": "^0.7.0",
    "clsx": "^2.1.0",
    "lucide-react": "^0.500.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "shadcn": "^4.0.0",
    "tailwind-merge": "^3.0.0",
    "tailwindcss": "^4.0.0",
    "tw-animate-css": "^1.0.0"
  },
  "devDependencies": {
    "typescript": "^5.7.0",
    "vite": "^6.0.0"
  }
}
"#,
        )
        .unwrap();
        fs::write(
            ws.join("index.html"),
            "<!doctype html><html><head><title>minicode — Terminal-Native Rust Coding Agent</title></head><body></body></html>",
        )
        .unwrap();
        fs::write(
            ws.join("src/index.css"),
            r#"@import "tailwindcss";
@import "tw-animate-css";
@import "shadcn/tailwind.css";
@import "@fontsource-variable/inter";
@import "@fontsource-variable/space-grotesk";
@import "@fontsource-variable/jetbrains-mono";

@theme inline {
  --font-heading: 'Space Grotesk Variable', var(--font-sans), sans-serif;
  --font-body: 'Inter Variable', var(--font-sans), sans-serif;
  --font-mono: 'JetBrains Mono Variable', ui-monospace, monospace;
  --color-primary: var(--primary);
}

:root,
.dark {
  --background: #090a0f;
  --foreground: #f8fafc;
  --card: #12151e;
  --primary: #06b6d4;
  --primary-foreground: #04121a;
  --secondary: #171b26;
  --secondary-foreground: #e2e8f0;
  --muted-foreground: #8b95a7;
  --destructive: #f26d6d;
}
"#,
        )
        .unwrap();
        fs::write(
            ws.join("src/App.tsx"),
            r#"import React from 'react';
import { createRoot } from 'react-dom/client';
import { Terminal } from 'lucide-react';
import { clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';
import { cva } from 'class-variance-authority';
import { Dialog } from '@base-ui/react/dialog';

export function App() {
  return <div className={twMerge(clsx('p-4'))}><Terminal />{String(Boolean(cva))}{String(Boolean(Dialog))}{String(Boolean( createRoot ))}</div>;
}
"#,
        )
        .unwrap();

        let wm = crate::context::memory::working_memory::WorkingMemory::new(&ws);
        wm.init_two_tier_plan_with_substeps(
            "Build minicode showcase",
            &["Implement showcase".to_string()],
            &[Vec::new()],
            None,
            None,
        )
        .unwrap();
        wm.update_progress("1", "completed").unwrap();

        // 1. YAML-frontmatter starter `architecture.md` was synthesized into full AST + Mermaid doc with `minicode` identity
        let arch_doc = fs::read_to_string(ws.join("minikit_docs/core/architecture.md")).unwrap();
        assert!(arch_doc.contains("# 🏛️ Architecture Documentation: minicode"));
        assert!(arch_doc.contains("```mermaid"));
        assert!(!arch_doc.contains("`minitest68`"));

        // 2. `design.md` synced live CSS colors, derived cyan hover hex, typography stacks, and `minicode` identity
        let design_doc = fs::read_to_string(ws.join("minikit_docs/core/design.md")).unwrap();
        assert!(design_doc.contains("project: \"minicode\""));
        assert!(design_doc.contains("primary: \"#06b6d4\""));
        assert!(design_doc.contains("primary-hover: \"#3cc6dd\""));
        assert!(!design_doc.contains("#828fff"));
        assert!(design_doc.contains("canvas: \"#090a0f\""));
        assert!(design_doc.contains("surface-1: \"#12151e\""));
        assert!(design_doc.contains("ink: \"#f8fafc\""));
        assert!(design_doc.contains("ink-subtle: \"#8b95a7\""));
        assert!(design_doc.contains("Space Grotesk Variable"));
        assert!(design_doc.contains("Inter Variable"));
        assert!(design_doc.contains("JetBrains Mono Variable"));

        // 3. Remaining core docs (`prd.md`, `coreidea.md`, `spec.md`, `content.md`) replaced `minitest68` -> `minicode`
        for doc_name in ["prd.md", "coreidea.md", "spec.md", "content.md"] {
            let text = fs::read_to_string(ws.join("minikit_docs/core").join(doc_name)).unwrap();
            assert!(
                text.contains("`minicode`") && !text.contains("`minitest68`"),
                "{doc_name} retained stale placeholder:\n{text}"
            );
        }

        // 4. `package.json` updated starter `"name": "minikit-vite-react"` -> `"name": "minicode"`
        let pkg_json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(ws.join("package.json")).unwrap()).unwrap();
        assert_eq!(pkg_json["name"], "minicode");

        // 5. `README.md` retains all 13 actively imported dependencies (including `@fontsource-variable/space-grotesk`) and drops unused `axios`
        let readme = fs::read_to_string(ws.join("README.md")).unwrap();
        assert!(
            readme.contains("`@fontsource-variable/space-grotesk`"),
            "13th imported package truncated:\n{readme}"
        );
        assert!(!readme.contains("`axios`"));
    }

    #[test]
    fn test_minitest70_css_var_graph_design_tokens_gitignore_readme_and_arch_resync() {
        let parent = tempdir().unwrap();
        let ws = parent.path().join("minitest70");
        fs::create_dir_all(ws.join("src/components/ui")).unwrap();
        fs::create_dir_all(ws.join("src/components/landing")).unwrap();

        fs::write(
            ws.join(".gitignore"),
            "node_modules\ndist\n.minicode/\nminikit_docs/packages/\nminikit.json\n",
        )
        .unwrap();
        fs::write(
            ws.join("src/components/ui/button.tsx"),
            "export function StarterButton() { return <button />; }\n",
        )
        .unwrap();
        fs::write(
            ws.join("src/components/landing/Hero.tsx"),
            "import { Terminal } from 'lucide-react';\nexport function Hero() { return <section><Terminal /></section>; }\n",
        )
        .unwrap();
        fs::write(
            ws.join("package.json"),
            r#"{
  "name": "minitest70",
  "dependencies": {
    "axios": "^1.7.0",
    "lucide-react": "^1.0.0",
    "react": "^19.0.0"
  },
  "scripts": { "dev": "vite", "build": "tsc -b && vite build", "lint": "eslint ." }
}"#,
        )
        .unwrap();
        fs::write(
            ws.join("minikit.json"),
            r#"{
  "name": "minitest70",
  "stack": "react-vite-gsap",
  "packages": ["axios", "lucide-react", "react"]
}"#,
        )
        .unwrap();
        fs::write(
            ws.join("index.html"),
            "<!doctype html><html><head><title>minicode — Fast Terminal Coding Agent</title></head></html>",
        )
        .unwrap();
        // Exact minitest70 stylesheet structure: `@theme` uses `var(--accent)` + `--color-primary-foreground: #04121a`
        // and `:root` defines `--bg`, `--surface-2`, `--muted` (foreground text!), `--accent`, `--ok`, `--warn`, `--danger`
        fs::write(
            ws.join("src/index.css"),
            r#"@import "tailwindcss";
@theme {
  --color-background: var(--bg);
  --color-foreground: var(--text);
  --color-card: var(--surface-1);
  --color-primary: var(--accent);
  --color-primary-foreground: #04121a;
  --color-secondary: var(--surface-2);
  --color-muted: var(--surface-2);
  --color-muted-foreground: var(--muted);
  --color-accent: var(--surface-2);
  --color-destructive: var(--danger);
  --color-border: var(--line);
}
:root {
  --bg: #07090c;
  --surface-1: #0c1117;
  --surface-2: #111823;
  --surface-3: #16202c;
  --line: rgba(255, 255, 255, 0.07);
  --text: #e9eff5;
  --muted: #8d9bab;
  --accent: #35d6e8;
  --ok: #5cc98d;
  --warn: #e0a458;
  --danger: #e06a6a;
}
"#,
        )
        .unwrap();

        MiniKitSyncEngine::ensure_workflow_docs(&ws.join("minikit_docs"), "minitest70", "bun");
        let starter_readme = MiniKitSyncEngine::generate_awesome_readme_template(
            "minitest70",
            "bun",
            "react-vite-gsap",
            "React 19 + Vite 8 (Kinetic Motion Template)",
            &[
                "axios".to_string(),
                "lucide-react".to_string(),
                "react".to_string(),
            ],
        );
        fs::write(ws.join("README.md"), starter_readme).unwrap();

        let wm = crate::context::memory::working_memory::WorkingMemory::new(&ws);
        wm.init_two_tier_plan_with_substeps(
            "Build minicode landing page",
            &["Build landing sections".to_string()],
            &[Vec::new()],
            None,
            None,
        )
        .unwrap();
        wm.update_progress("1", "completed").unwrap();

        // Simulate post-verify cleanup deleting starter `src/components/ui/button.tsx` and re-syncing
        fs::remove_file(ws.join("src/components/ui/button.tsx")).unwrap();
        MiniKitSyncEngine::sync_decisions_into_core_docs(&ws);

        // 1. `design.md`: primary MUST be `#35d6e8` (never `#04121a`), surface-2 MUST be `#111823` (never `#8d9bab`)
        let design = fs::read_to_string(ws.join("minikit_docs/core/design.md")).unwrap();
        assert!(design.contains("primary: \"#35d6e8\""), "got:\n{design}");
        assert!(design.contains("on-primary: \"#04121a\""), "got:\n{design}");
        assert!(design.contains("canvas: \"#07090c\""), "got:\n{design}");
        assert!(design.contains("surface-1: \"#0c1117\""), "got:\n{design}");
        assert!(design.contains("surface-2: \"#111823\""), "got:\n{design}");
        assert!(design.contains("surface-3: \"#16202c\""), "got:\n{design}");
        assert!(design.contains("ink: \"#e9eff5\""), "got:\n{design}");
        assert!(design.contains("ink-subtle: \"#8d9bab\""), "got:\n{design}");
        assert!(
            design.contains("semantic-success: \"#5cc98d\""),
            "got:\n{design}"
        );
        assert!(
            design.contains("semantic-warning: \"#e0a458\""),
            "got:\n{design}"
        );
        assert!(
            design.contains("semantic-error: \"#e06a6a\""),
            "got:\n{design}"
        );

        // 2. `architecture.md`: re-synthesized after starter cleanup -> deleted `button.tsx` is gone!
        let arch = fs::read_to_string(ws.join("minikit_docs/core/architecture.md")).unwrap();
        assert!(
            !arch.contains("button.tsx"),
            "Stale deleted file remained:\n{arch}"
        );
        assert!(arch.contains("Hero.tsx"), "Live Hero.tsx missing:\n{arch}");
        assert!(
            !arch.contains("| `File` |"),
            "File node leaked into symbol table:\n{arch}"
        );

        // 3. `README.md`: gitignored `minikit.json`, `.minicode/`, and `packages/` excluded; 78-col box preserved
        let readme = fs::read_to_string(ws.join("README.md")).unwrap();
        assert!(
            !readme.contains("minikit.json"),
            "Gitignored minikit.json leaked:\n{readme}"
        );
        assert!(
            !readme.contains("minicode.md"),
            "Gitignored minicode.md leaked:\n{readme}"
        );
        assert!(
            !readme.contains("packages/"),
            "Gitignored packages/ leaked:\n{readme}"
        );
        for box_line in readme.lines().filter(|l| {
            l.starts_with(['┌', '│', '├', '└'])
                && !l.starts_with("├──")
                && !l.starts_with("└──")
                && !l.starts_with("│   ")
        }) {
            assert_eq!(box_line.chars().count(), 78, "Box width drift: {box_line}");
        }

        // 4. `minikit.json`: `"packages"` filtered to actually imported packages (drops unused `axios`)
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(ws.join("minikit.json")).unwrap()).unwrap();
        let pkgs = manifest["packages"].as_array().unwrap();
        assert!(pkgs.iter().any(|v| v == "lucide-react"));
        assert!(!pkgs.iter().any(|v| v == "axios"));
    }
}
