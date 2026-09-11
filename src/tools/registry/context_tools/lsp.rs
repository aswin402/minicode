use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "lsp_diagnostics".to_string(),
            description: "Fetch compiler and linter diagnostics across the workspace (or specific files) to check for compile errors and warnings.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "max_items": {
                        "type": "integer",
                        "description": "Maximum number of error items to display in detail (default: 8)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "lsp_goto_definition".to_string(),
            description: "Resolve the exact file path and line location where a code symbol (function, struct, type) is defined using LSP.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative file path containing the symbol usage"
                    },
                    "line": {
                        "type": "integer",
                        "description": "Line number (1-indexed) where the symbol appears"
                    },
                    "character": {
                        "type": "integer",
                        "description": "Column character offset (1-indexed) of the symbol"
                    }
                },
                "required": ["path", "line", "character"]
            }),
        },
        ToolSchema {
            name: "lsp_find_references".to_string(),
            description: "Locate all reference usages and call sites of a symbol across the workspace using LSP.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative file path containing the symbol"
                    },
                    "line": {
                        "type": "integer",
                        "description": "Line number (1-indexed)"
                    },
                    "character": {
                        "type": "integer",
                        "description": "Column character offset (1-indexed)"
                    }
                },
                "required": ["path", "line", "character"]
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
        "lsp_diagnostics" => Some({
            let max_items = param::opt_usize(args, "max_items", 8);
            match crate::lsp::LspEngine::run_diagnostics(workspace_root).await {
                Ok(report) => Ok(report.format_for_agent(workspace_root, max_items)),
                Err(e) => Err(e),
            }
        }),
        "lsp_goto_definition" => Some({
            let path = match param::require_str(args, "path", "lsp_goto_definition") {
                Ok(p) => p,
                Err(e) => return Some(Err(e.into())),
            };
            let line = param::opt_usize(args, "line", 1) as u32;
            let character = param::opt_usize(args, "character", 1) as u32;

            let lsp_line = line.saturating_sub(1);
            let lsp_col = character.saturating_sub(1);

            match crate::lsp::LspEngine::goto_definition(
                workspace_root,
                Path::new(path),
                lsp_line,
                lsp_col,
            )
            .await
            {
                Ok(locations) => {
                    if locations.is_empty() {
                        Ok(format!(
                            "ℹ No definition found for '{}:{}:{}' via LSP",
                            path, line, character
                        ))
                    } else {
                        let mut out =
                            format!("✔ Found {} definition location(s):\n", locations.len());
                        for loc in locations {
                            let rel = loc
                                .file_path
                                .strip_prefix(workspace_root)
                                .unwrap_or(&loc.file_path);
                            out.push_str(&format!(
                                "  • {}:{}:{}\n",
                                rel.display(),
                                loc.line + 1,
                                loc.character + 1
                            ));
                        }
                        Ok(out)
                    }
                }
                Err(e) => Err(e),
            }
        }),
        "lsp_find_references" => Some({
            let path = match param::require_str(args, "path", "lsp_find_references") {
                Ok(p) => p,
                Err(e) => return Some(Err(e.into())),
            };
            let line = param::opt_usize(args, "line", 1) as u32;
            let character = param::opt_usize(args, "character", 1) as u32;

            let lsp_line = line.saturating_sub(1);
            let lsp_col = character.saturating_sub(1);

            match crate::lsp::LspEngine::find_references(
                workspace_root,
                Path::new(path),
                lsp_line,
                lsp_col,
            )
            .await
            {
                Ok(locations) => {
                    if locations.is_empty() {
                        Ok(format!(
                            "ℹ No references found for '{}:{}:{}' via LSP",
                            path, line, character
                        ))
                    } else {
                        let mut out = format!("✔ Found {} reference usage(s):\n", locations.len());
                        for loc in locations {
                            let rel = loc
                                .file_path
                                .strip_prefix(workspace_root)
                                .unwrap_or(&loc.file_path);
                            out.push_str(&format!(
                                "  • {}:{}:{}\n",
                                rel.display(),
                                loc.line + 1,
                                loc.character + 1
                            ));
                        }
                        Ok(out)
                    }
                }
                Err(e) => Err(e),
            }
        }),
        _ => None,
    }
}
