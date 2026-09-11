use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::session::backup::BackupManager;
use crate::tools::fs;
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "read_file".to_string(),
            description: "Read the contents of a file in the workspace within an optional 1-indexed line range.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file within the workspace"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Optional 1-indexed starting line number"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Optional 1-indexed ending line number (inclusive)"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "patch_file".to_string(),
            description: "Apply a precise search-and-replace block edit to a file. Provide the exact text to replace and the new content.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file to modify"
                    },
                    "search_block": {
                        "type": "string",
                        "description": "The exact unique code block in the file to replace"
                    },
                    "replace_block": {
                        "type": "string",
                        "description": "The new replacement code block"
                    }
                },
                "required": ["path", "search_block", "replace_block"]
            }),
        },
        ToolSchema {
            name: "repair_patch".to_string(),
            description: "Surgical fault repair: Atomically applies a search-and-replace block patch to a file with 5-tier resilient matching, executes an optional pre-flight verification gate (e.g. 'cargo test -j 3' or 'npm test'), and automatically rolls back changes to pristine state if tests or compilation break.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative workspace path to target file (e.g. 'src/calc.rs', 'app.py')"
                    },
                    "search_block": {
                        "type": "string",
                        "description": "The exact unique code block in the file to replace (include surrounding context if needed)"
                    },
                    "replace_block": {
                        "type": "string",
                        "description": "The replacement code block"
                    },
                    "verification_cmd": {
                        "type": "string",
                        "description": "Optional test or build command to run before finalizing patch (e.g. 'cargo test -j 3 --test foo', 'npm test'). If omitted, runs scoped compiler check."
                    }
                },
                "required": ["path", "search_block", "replace_block"]
            }),
        },
        ToolSchema {
            name: "write_file".to_string(),
            description: "Create a new file or completely overwrite an existing file with the provided content.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "The complete text content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolSchema {
            name: "ast_replace_node".to_string(),
            description: "Surgically replace an entire function, method, struct, class, enum, or interface AST node with pre-disk Tree-sitter syntax validation, scope-aligned indentation, and semantic AST diff generation.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the source file (.rs, .py, .ts, .tsx, .js, .jsx)"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Exact symbol or node name to replace (e.g. 'my_func', 'UserConfig')"
                    },
                    "replacement_code": {
                        "type": "string",
                        "description": "The complete replacement code for this AST node"
                    },
                    "kind": {
                        "type": "string",
                        "description": "Optional node kind filter (e.g. 'function_item', 'struct_item', 'impl_item', 'class_definition')"
                    }
                },
                "required": ["path", "symbol", "replacement_code"]
            }),
        },
        ToolSchema {
            name: "begin_transaction".to_string(),
            description: "Begin an atomic workspace transaction with pre-mutation Write-Ahead Logging (WAL). All subsequent file mutations across the workspace are journaled for atomic commit or rollback.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "Descriptive goal or reason for the transaction (e.g. 'Refactor error types and update call sites')"
                    }
                },
                "required": ["description"]
            }),
        },
        ToolSchema {
            name: "commit_transaction".to_string(),
            description: "Commit an active workspace transaction, sealing the Write-Ahead Log journal and finalizing all file modifications across the workspace.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tx_id": {
                        "type": "string",
                        "description": "Optional transaction ID to commit (defaults to current active transaction)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "rollback_transaction".to_string(),
            description: "Atomically roll back all file modifications, creations, and deletions performed in an active workspace transaction, restoring the workspace to its pre-transaction baseline.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tx_id": {
                        "type": "string",
                        "description": "Optional transaction ID to rollback (defaults to current active transaction)"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Optional explanation of why the transaction was rolled back (e.g. 'Compiler verification failed on Gate 1')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "get_transaction_status".to_string(),
            description: "Inspect the status of the current active workspace transaction or a historical transaction, detailing all affected files, hashes, and durations.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tx_id": {
                        "type": "string",
                        "description": "Optional transaction ID to query (defaults to current active transaction)"
                    }
                }
            }),
        },
    ]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
    backup_manager: Option<&BackupManager>,
    turn_id: usize,
) -> Option<Result<String>> {
    match tool_name {
        "read_file" => Some((|| {
            let path = param::require_str(args, "path", "read_file")?;
            let start_line =
                param::opt_u64(args, "start_line").and_then(|v| usize::try_from(v).ok());
            let end_line = param::opt_u64(args, "end_line").and_then(|v| usize::try_from(v).ok());
            fs::read_file(workspace_root, path, start_line, end_line)
        })()),
        "write_file" => Some((|| {
            let path = param::require_str(args, "path", "write_file")?;
            let content = param::require_str(args, "content", "write_file")?;

            let validated_path =
                crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(path))?;

            // Safety checkpoint before modifying
            if let Some(mgr) = backup_manager {
                if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id) {
                    tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before write_file");
                }
            }

            // Transaction WAL pre-mutation hook
            if let Err(e) = crate::session::transaction::TransactionManager::record_mutation_pre(
                workspace_root,
                &validated_path,
            ) {
                tracing::warn!(path = %validated_path.display(), error = %e, "Failed to record transaction pre-mutation hook for write_file");
            }

            let res = fs::write_file(workspace_root, path, content);
            if res.is_ok() {
                let _ = crate::session::transaction::TransactionManager::record_mutation_post(
                    workspace_root,
                    &validated_path,
                );
            }
            res
        })()),
        "patch_file" => Some((|| {
            let path = param::require_str(args, "path", "patch_file")?;
            let search = param::require_str(args, "search_block", "patch_file")?;
            let replace = param::require_str(args, "replace_block", "patch_file")?;

            let validated_path =
                crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(path))?;

            // Safety checkpoint before patching
            if let Some(mgr) = backup_manager {
                if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id) {
                    tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before patch_file");
                }
            }

            // Transaction WAL pre-mutation hook
            if let Err(e) = crate::session::transaction::TransactionManager::record_mutation_pre(
                workspace_root,
                &validated_path,
            ) {
                tracing::warn!(path = %validated_path.display(), error = %e, "Failed to record transaction pre-mutation hook for patch_file");
            }

            let res = fs::patch_file(workspace_root, path, search, replace);
            if res.is_ok() {
                let _ = crate::session::transaction::TransactionManager::record_mutation_post(
                    workspace_root,
                    &validated_path,
                );
            }
            res
        })()),
        "repair_patch" => Some(async {
            let path = param::require_str(args, "path", "repair_patch")?;
            let search = param::require_str(args, "search_block", "repair_patch")?;
            let replace = param::require_str(args, "replace_block", "repair_patch")?;
            let verification_cmd = param::opt_str(args, "verification_cmd");

            let validated_path =
                crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(path))?;

            // Safety checkpoint before repair
            if let Some(mgr) = backup_manager {
                if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id) {
                    tracing::warn!(path = %validated_path.display(), error = %e, "Failed to create safety checkpoint before repair_patch");
                }
            }

            // Transaction WAL pre-mutation hook
            if let Err(e) = crate::session::transaction::TransactionManager::record_mutation_pre(
                workspace_root,
                &validated_path,
            ) {
                tracing::warn!(path = %validated_path.display(), error = %e, "Failed to record transaction pre-mutation hook for repair_patch");
            }

            let result = crate::tools::repair::SurgicalRepairEngine::execute_surgical_repair(
                workspace_root,
                path,
                search,
                replace,
                verification_cmd,
            )
            .await?;

            if result.success {
                let _ = crate::session::transaction::TransactionManager::record_mutation_post(
                    workspace_root,
                    &validated_path,
                );
            }

            Ok(result.format_markdown())
        }.await),
        "ast_replace_node" => Some((|| {
            let path = param::require_str(args, "path", "ast_replace_node")?;
            let symbol = param::require_str(args, "symbol", "ast_replace_node")?;
            let replacement = param::require_str(args, "replacement_code", "ast_replace_node")?;
            let kind = param::opt_str(args, "kind");

            let validated_path =
                crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(path))?;

            // Safety checkpoint before AST replacement
            if let Some(mgr) = backup_manager {
                if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id) {
                    tracing::warn!(
                        path = %validated_path.display(),
                        error = %e,
                        "Failed to create safety checkpoint before ast_replace_node"
                    );
                }
            }

            // Transaction WAL pre-mutation hook
            if let Err(e) = crate::session::transaction::TransactionManager::record_mutation_pre(
                workspace_root,
                &validated_path,
            ) {
                tracing::warn!(
                    path = %validated_path.display(),
                    error = %e,
                    "Failed to record transaction pre-mutation hook for ast_replace_node"
                );
            }

            let result = crate::context::ast_transform::AstTransformer::replace_node(
                workspace_root,
                path,
                symbol,
                replacement,
                kind,
            )?;
            let _ = crate::session::transaction::TransactionManager::record_mutation_post(
                workspace_root,
                &validated_path,
            );
            Ok(result.format_receipt())
        })()),
        "begin_transaction" => Some((|| {
            let desc = param::require_str(args, "description", "begin_transaction")?;
            let manifest =
                crate::session::transaction::TransactionManager::begin(workspace_root, desc)?;
            Ok(format!(
                "✔ Began atomic workspace transaction '{}' for: {}\nAll subsequent file modifications will be journaled in the WAL and can be atomically rolled back.",
                manifest.tx_id, manifest.description
            ))
        })()),
        "commit_transaction" => Some((|| {
            let tx_id = param::opt_str(args, "tx_id");
            let receipt =
                crate::session::transaction::TransactionManager::commit(workspace_root, tx_id)?;
            Ok(receipt.format_receipt())
        })()),
        "rollback_transaction" => Some((|| {
            let tx_id = param::opt_str(args, "tx_id");
            let reason = param::opt_str(args, "reason");
            let receipt = crate::session::transaction::TransactionManager::rollback(
                workspace_root,
                tx_id,
                reason,
            )?;
            Ok(receipt.format_receipt())
        })()),
        "get_transaction_status" => {
            Some((|| {
                let tx_id = param::opt_str(args, "tx_id");
                match crate::session::transaction::TransactionManager::status(workspace_root, tx_id)? {
                Some(receipt) => Ok(receipt.format_receipt()),
                None => Ok("No active workspace transaction. Workspace is in direct modification mode.".to_string()),
            }
            })())
        }
        _ => None,
    }
}
