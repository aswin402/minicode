use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::param::*;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "kit_stack_list".to_string(),
            description: "List all available MiniKit project templates (React Vite, Next.js 16, FastAPI, Flutter, Hono, PERN, MERN, etc.) with file counts and technology tags.".to_string(),
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
            name: "kit_stack_show".to_string(),
            description: "Inspect the exact structure, package dependencies, and files of a specific MiniKit stack template.".to_string(),
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
            name: "kit_stack_add".to_string(),
            description: "Scaffold a complete, production-grade application stack into the target folder with automatic dependency installation and AGENTS.md / minikit_docs generation.".to_string(),
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
                        "description": "If true, skips running automatic package installation (bun install, uv sync, cargo check, etc.)"
                    }
                },
                "required": ["stack_name"]
            }),
        },
        ToolSchema {
            name: "kit_stack_diff".to_string(),
            description: "Inspect architectural drift between workspace files and the canonical stack template. Identifies missing or modified files, with optional self-healing via apply: true.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "stack_name": {
                        "type": "string",
                        "description": "Optional stack template name (defaults to stack configured in minikit.json / onpkg.json)"
                    },
                    "apply": {
                        "type": "boolean",
                        "description": "If true, automatically re-scaffolds and restores any missing architecture template files"
                    }
                }
            }),
        },
        ToolSchema {
            name: "kit_skill_list".to_string(),
            description: "List all active workspace skills and the 14 built-in domain skills available for installation (React, Next.js, FastAPI, Flutter, Hono, Rust, Tailwind, MongoDB, Postgres, Prisma, Vite, Express, Frontend Design, UI/UX Pro Max).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "kit_skill_show".to_string(),
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
            name: "kit_skill_install".to_string(),
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
            name: "kit_info".to_string(),
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
            name: "kit_add".to_string(),
            description: "Add a verified package to the workspace dependencies manifest (package.json, Cargo.toml, requirements.txt, pubspec.yaml) and sync minikit.json.".to_string(),
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
            name: "kit_sync".to_string(),
            description: "Scan project files and packages to update minikit.json, synchronize AGENTS.md, and update spec-driven workflow docs in minikit_docs/.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "kit_doctor".to_string(),
            description: "Run environment diagnostics to verify installed runtimes (Bun, Node.js, UV/Python, Cargo, Flutter) and template database health.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "kit_stack_new".to_string(),
            description: "Create a new custom stack template specification in `.minicode/stacks/<name>.json` (or globally in `~/.config/minicode/stacks/`).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name for the new custom stack template"
                    },
                    "runtime": {
                        "type": "string",
                        "description": "Optional runtime ecosystem ('bun', 'node', 'uv', 'cargo', 'flutter'). Defaults to 'bun'."
                    },
                    "global": {
                        "type": "boolean",
                        "description": "If true, saves globally in ~/.config/minicode/stacks/ instead of workspace .minicode/stacks/"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "kit_stack_remove".to_string(),
            description: "Delete a custom stack template specification from `.minicode/stacks/<name>.json` (or globally).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the custom stack template to delete"
                    },
                    "global": {
                        "type": "boolean",
                        "description": "If true, deletes from global ~/.config/minicode/stacks/ instead of workspace"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "kit_skill_remove".to_string(),
            description: "Remove and uninstall a domain skill from the project workspace (.minicode/skills/<name> and manifest active_skills).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_name": {
                        "type": "string",
                        "description": "Name of the skill to remove (e.g. 'react', 'next', 'tailwind', 'rust')"
                    }
                },
                "required": ["skill_name"]
            }),
        },
        ToolSchema {
            name: "kit_remove".to_string(),
            description: "Remove a package dependency from the project manifest (package.json, Cargo.toml, requirements.txt, pubspec.yaml) and update minikit.json.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the package to remove"
                    },
                    "runtime": {
                        "type": "string",
                        "description": "Optional ecosystem ('npm', 'pypi', 'cargo', 'pub'). Auto-detected if omitted."
                    }
                },
                "required": ["name"]
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
        "kit_stack_list" | "onpkg_stack_list" => Some(
            async {
                let category = opt_str(args, "category");
                crate::tools::onpkg::OnpkgService::list_stacks(workspace_root, category).await
            }
            .await,
        ),
        "kit_stack_show" | "onpkg_stack_show" => Some(
            async {
                let stack_name =
                    get_str_with_aliases(args, &["stack_name", "name", "stack", "template"])
                        .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                            name: "kit_stack_show".to_string(),
                            reason: "Missing required argument 'stack_name'".to_string(),
                        })?;
                crate::tools::onpkg::OnpkgService::show_stack(workspace_root, stack_name).await
            }
            .await,
        ),
        "kit_stack_add" | "onpkg_stack_add" => Some(
            async {
                let stack_name =
                    get_str_with_aliases(args, &["stack_name", "name", "stack", "template"])
                        .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                            name: "kit_stack_add".to_string(),
                            reason: "Missing required argument 'stack_name'".to_string(),
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
        "kit_stack_new" | "onpkg_stack_new" => Some(
            async {
                let name = get_str_with_aliases(args, &["name", "stack_name", "stack"])
                    .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                        name: "kit_stack_new".to_string(),
                        reason: "Missing required argument 'name'".to_string(),
                    })?;
                let runtime = opt_str(args, "runtime").unwrap_or("bun");
                let global = opt_bool(args, "global", false);
                crate::tools::onpkg::OnpkgService::create_custom_stack(
                    workspace_root,
                    name,
                    runtime,
                    global,
                )
                .await
            }
            .await,
        ),
        "kit_stack_remove" | "onpkg_stack_remove" => Some(
            async {
                let name = get_str_with_aliases(args, &["name", "stack_name", "stack"])
                    .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                        name: "kit_stack_remove".to_string(),
                        reason: "Missing required argument 'name'".to_string(),
                    })?;
                let global = opt_bool(args, "global", false);
                crate::tools::onpkg::OnpkgService::delete_custom_stack(workspace_root, name, global)
                    .await
            }
            .await,
        ),
        "kit_stack_diff" | "onpkg_stack_diff" => Some(
            async {
                let stack_name = opt_str(args, "stack_name").or_else(|| opt_str(args, "name"));
                let apply = opt_bool(args, "apply", false);
                crate::tools::onpkg::OnpkgService::diff_stack(workspace_root, stack_name, apply)
                    .await
            }
            .await,
        ),
        "kit_skill_list" | "onpkg_skill_list" => Some(
            async { crate::tools::onpkg::OnpkgService::list_skills(workspace_root).await }.await,
        ),
        "kit_skill_show" | "onpkg_skill_show" => Some(
            async {
                let skill_name = get_str_with_aliases(args, &["skill_name", "name", "skill"])
                    .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                        name: "kit_skill_show".to_string(),
                        reason: "Missing required argument 'skill_name'".to_string(),
                    })?;
                crate::tools::onpkg::OnpkgService::show_skill(workspace_root, skill_name).await
            }
            .await,
        ),
        "kit_skill_install" | "onpkg_skill_install" => Some(
            async {
                let skill_name = get_str_with_aliases(args, &["skill_name", "name", "skill"])
                    .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                        name: "kit_skill_install".to_string(),
                        reason: "Missing required argument 'skill_name'".to_string(),
                    })?;
                crate::tools::onpkg::OnpkgService::install_skill(workspace_root, skill_name).await
            }
            .await,
        ),
        "kit_skill_remove" | "onpkg_skill_remove" => Some(
            async {
                let skill_name = get_str_with_aliases(args, &["skill_name", "name", "skill"])
                    .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                        name: "kit_skill_remove".to_string(),
                        reason: "Missing required argument 'skill_name'".to_string(),
                    })?;
                crate::tools::onpkg::OnpkgService::remove_skill(workspace_root, skill_name).await
            }
            .await,
        ),
        "kit_info" | "kit_pkg_info" | "onpkg_pkg_info" => Some(
            async {
                let name =
                    get_str_with_aliases(args, &["name", "pkg", "package"]).ok_or_else(|| {
                        crate::error::ToolError::InvalidArguments {
                            name: "kit_info".to_string(),
                            reason: "Missing required argument 'name'".to_string(),
                        }
                    })?;
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
        "kit_add" | "kit_pkg_add" | "onpkg_pkg_add" => Some(
            async {
                let name =
                    get_str_with_aliases(args, &["name", "pkg", "package"]).ok_or_else(|| {
                        crate::error::ToolError::InvalidArguments {
                            name: "kit_add".to_string(),
                            reason: "Missing required argument 'name'".to_string(),
                        }
                    })?;
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
        "kit_remove" | "kit_pkg_remove" | "onpkg_pkg_remove" | "onpkg_pkg_rm" => Some(
            async {
                let name =
                    get_str_with_aliases(args, &["name", "pkg", "package"]).ok_or_else(|| {
                        crate::error::ToolError::InvalidArguments {
                            name: "kit_remove".to_string(),
                            reason: "Missing required argument 'name'".to_string(),
                        }
                    })?;
                let runtime = opt_str(args, "runtime");
                crate::tools::onpkg::OnpkgService::remove_package(workspace_root, name, runtime)
                    .await
            }
            .await,
        ),
        "kit_sync" | "onpkg_sync" => Some(
            async { crate::tools::onpkg::OnpkgService::sync_project(workspace_root).await }.await,
        ),
        "kit_doctor" | "onpkg_doctor" => Some(
            async { crate::tools::onpkg::OnpkgService::run_doctor(workspace_root).await }.await,
        ),
        _ => None,
    }
}
