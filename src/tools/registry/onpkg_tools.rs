use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::param::*;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "onpkg_stack_list".to_string(),
            description: "List all available onpkg project templates (React Vite, Next.js 16, FastAPI, Flutter, Hono, PERN, MERN, etc.) with file counts and technology tags.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Optional category filter (e.g. 'frontend', 'backend', 'app')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "onpkg_stack_show".to_string(),
            description: "Inspect the exact structure, package dependencies, and files of a specific onpkg stack template.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "stack_name": {
                        "type": "string",
                        "description": "Name of the stack template (e.g. 'next-template', 'react-vite-gsap', 'fastapi')"
                    }
                },
                "required": ["stack_name"]
            }),
        },
        ToolSchema {
            name: "onpkg_stack_add".to_string(),
            description: "Scaffold a complete, production-grade application stack into the target folder with automatic online dependency installation and AGENTS.md / onpkg_docs generation.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "stack_name": {
                        "type": "string",
                        "description": "Name of the stack template to scaffold (e.g. 'react-vite-gsap', 'next-template', 'fastapi', 'flutter-riverpod-my_app')"
                    },
                    "target_dir": {
                        "type": "string",
                        "description": "Optional target directory path relative to workspace or absolute. Defaults to current workspace."
                    },
                    "no_install": {
                        "type": "boolean",
                        "description": "If true, skips running automatic online package installation (bun install, uv sync, cargo check, etc.)"
                    }
                },
                "required": ["stack_name"]
            }),
        },
        ToolSchema {
            name: "onpkg_stack_diff".to_string(),
            description: "Inspect architectural drift between workspace files and the canonical stack template. Identifies missing or modified files, with optional self-healing via apply: true.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "stack_name": {
                        "type": "string",
                        "description": "Optional stack template name (defaults to stack configured in onpkg.json)"
                    },
                    "apply": {
                        "type": "boolean",
                        "description": "If true, automatically re-scaffolds and restores any missing architecture template files"
                    }
                }
            }),
        },
        ToolSchema {
            name: "onpkg_skill_list".to_string(),
            description: "List all active workspace skills and the 14 built-in domain skills available for installation (React, Next.js, FastAPI, Flutter, Hono, Rust, Tailwind, MongoDB, Postgres, Prisma, Vite, Express, Frontend Design, UI/UX Pro Max).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "onpkg_skill_show".to_string(),
            description: "Read the complete guidelines, instructions, and coding standards of a specific domain skill.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_name": {
                        "type": "string",
                        "description": "Name of the skill to inspect (e.g. 'react', 'next', 'tailwind', 'rust', 'frontend-design')"
                    }
                },
                "required": ["skill_name"]
            }),
        },
        ToolSchema {
            name: "onpkg_skill_install".to_string(),
            description: "Install a battle-tested technology skill package from the built-in catalog into the project (.minicode/skills/<name>/SKILL.md) and update manifest.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_name": {
                        "type": "string",
                        "description": "Name of the skill to install (e.g. 'react', 'next', 'tailwind', 'rust', 'frontend-design', 'ui-ux-pro-max')"
                    }
                },
                "required": ["skill_name"]
            }),
        },
        ToolSchema {
            name: "onpkg_pkg_info".to_string(),
            description: "Query upstream registries (npm, PyPI, crates.io, pub.dev) for real-time package metadata, latest version, description, and repository URL.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the package or library (e.g. 'hono', 'zod', 'fastapi', 'tokio', 'riverpod')"
                    },
                    "runtime": {
                        "type": "string",
                        "description": "Optional ecosystem ('npm', 'pypi', 'cargo', 'pub'). Auto-detected from workspace if omitted."
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "onpkg_pkg_add".to_string(),
            description: "Add a verified package to the workspace dependencies manifest (package.json, Cargo.toml, requirements.txt, pubspec.yaml) and sync onpkg.json.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the package to add"
                    },
                    "version": {
                        "type": "string",
                        "description": "Optional version specifier (defaults to latest upstream version)"
                    },
                    "runtime": {
                        "type": "string",
                        "description": "Optional ecosystem ('npm', 'pypi', 'cargo', 'pub'). Auto-detected if omitted."
                    },
                    "is_dev": {
                        "type": "boolean",
                        "description": "If true, add as a development dependency"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "onpkg_sync".to_string(),
            description: "Scan project files and packages to update onpkg.json, synchronize AGENTS.md, and update spec-driven workflow docs (prd.md, design.md, todo.md) in onpkg_docs/.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "onpkg_doctor".to_string(),
            description: "Run environment diagnostics to verify installed runtimes (Bun, Node.js, UV/Python, Cargo, Flutter) and template database health.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "onpkg_stack_list" => Some(
            async {
                let category = opt_str(args, "category");
                crate::tools::onpkg::OnpkgService::list_stacks(workspace_root, category).await
            }
            .await,
        ),
        "onpkg_stack_show" => Some(
            async {
                let stack_name =
                    get_str_with_aliases(args, &["stack_name", "name", "stack", "template"])
                        .ok_or_else(|| {
                            require_str(args, "stack_name", "onpkg_stack_show").unwrap_err()
                        })?;
                crate::tools::onpkg::OnpkgService::show_stack(workspace_root, stack_name).await
            }
            .await,
        ),
        "onpkg_stack_add" => Some(
            async {
                let stack_name =
                    get_str_with_aliases(args, &["stack_name", "name", "stack", "template"])
                        .ok_or_else(|| {
                            require_str(args, "stack_name", "onpkg_stack_add").unwrap_err()
                        })?;
                let target_dir = opt_path(args).or_else(|| opt_str(args, "target_dir"));
                let no_install = opt_bool(args, "no_install", false);
                crate::tools::onpkg::OnpkgService::add_stack(
                    workspace_root,
                    stack_name,
                    target_dir,
                    no_install,
                )
                .await
            }
            .await,
        ),
        "onpkg_stack_diff" => Some(
            async {
                let stack_name = opt_str(args, "stack_name").or_else(|| opt_str(args, "name"));
                let apply = opt_bool(args, "apply", false);
                crate::tools::onpkg::OnpkgService::diff_stack(workspace_root, stack_name, apply)
                    .await
            }
            .await,
        ),
        "onpkg_skill_list" => Some(
            async { crate::tools::onpkg::OnpkgService::list_skills(workspace_root).await }.await,
        ),
        "onpkg_skill_show" => Some(
            async {
                let skill_name = get_str_with_aliases(args, &["skill_name", "name", "skill"])
                    .ok_or_else(|| {
                        require_str(args, "skill_name", "onpkg_skill_show").unwrap_err()
                    })?;
                crate::tools::onpkg::OnpkgService::show_skill(workspace_root, skill_name).await
            }
            .await,
        ),
        "onpkg_skill_install" => Some(
            async {
                let skill_name = get_str_with_aliases(args, &["skill_name", "name", "skill"])
                    .ok_or_else(|| {
                        require_str(args, "skill_name", "onpkg_skill_install").unwrap_err()
                    })?;
                crate::tools::onpkg::OnpkgService::install_skill(workspace_root, skill_name).await
            }
            .await,
        ),
        "onpkg_pkg_info" => Some(
            async {
                let name = get_str_with_aliases(args, &["name", "pkg", "package"])
                    .ok_or_else(|| require_str(args, "name", "onpkg_pkg_info").unwrap_err())?;
                let runtime = opt_str(args, "runtime");
                let registry = crate::tools::onpkg::pkg::PkgRegistry::new();
                let info = registry.fetch_info(name, runtime, workspace_root).await?;
                let out = serde_json::to_string_pretty(&info).map_err(|e| {
                    crate::error::ToolError::CommandExec(format!(
                        "Failed to serialize package info: {}",
                        e
                    ))
                })?;
                Ok(out)
            }
            .await,
        ),
        "onpkg_pkg_add" => Some(
            async {
                let name = get_str_with_aliases(args, &["name", "pkg", "package"])
                    .ok_or_else(|| require_str(args, "name", "onpkg_pkg_add").unwrap_err())?;
                let version = opt_str(args, "version");
                let runtime = opt_str(args, "runtime");
                let is_dev = opt_bool(args, "is_dev", false);
                let registry = crate::tools::onpkg::pkg::PkgRegistry::new();
                registry
                    .add_to_project(workspace_root, name, version, runtime, is_dev)
                    .await
            }
            .await,
        ),
        "onpkg_sync" => Some(
            async { crate::tools::onpkg::OnpkgService::sync_project(workspace_root).await }.await,
        ),
        "onpkg_doctor" => Some(
            async { crate::tools::onpkg::OnpkgService::run_doctor(workspace_root).await }.await,
        ),
        _ => None,
    }
}
