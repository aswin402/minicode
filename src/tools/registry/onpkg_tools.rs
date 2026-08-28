use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
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
            name: "onpkg_skill_list".to_string(),
            description: "List all installed and available AI agent skills managed by onpkg.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "onpkg_skill_install".to_string(),
            description: "Install a technology skill package (e.g. 'gsap-core', 'tailwind-patterns', 'mem0', 'rust-skills') into the project.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_name": {
                        "type": "string",
                        "description": "Name of the skill to install"
                    }
                },
                "required": ["skill_name"]
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
                let category = args.get("category").and_then(|v| v.as_str());
                crate::tools::onpkg::OnpkgService::list_stacks(workspace_root, category).await
            }
            .await,
        ),
        "onpkg_stack_show" => Some(
            async {
                let stack_name =
                    args.get("stack_name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "onpkg_stack_show".to_string(),
                            reason: "Missing required argument 'stack_name'".to_string(),
                        })?;
                crate::tools::onpkg::OnpkgService::show_stack(workspace_root, stack_name).await
            }
            .await,
        ),
        "onpkg_stack_add" => Some(
            async {
                let stack_name =
                    args.get("stack_name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "onpkg_stack_add".to_string(),
                            reason: "Missing required argument 'stack_name'".to_string(),
                        })?;
                let target_dir = args.get("target_dir").and_then(|v| v.as_str());
                let no_install = args
                    .get("no_install")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
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
        "onpkg_skill_list" => Some(
            async { crate::tools::onpkg::OnpkgService::list_skills(workspace_root).await }.await,
        ),
        "onpkg_skill_install" => Some(
            async {
                let skill_name =
                    args.get("skill_name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "onpkg_skill_install".to_string(),
                            reason: "Missing required argument 'skill_name'".to_string(),
                        })?;
                crate::tools::onpkg::OnpkgService::install_skill(workspace_root, skill_name).await
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
