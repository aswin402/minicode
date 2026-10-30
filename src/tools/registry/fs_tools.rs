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
            description: "Read the contents of a file in the workspace within an optional 1-indexed line range. Output format is 'line | code'. For files exceeding 250 lines without specified bounds, the first 200 lines are returned with a pagination notice.".to_string(),
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
            name: "batch_read".to_string(),
            description: "Read multiple workspace files in parallel in a single tool call. Much faster and more token-efficient than sequential read_file calls when inspecting several related files.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of relative file paths within the workspace to read in parallel (up to 20 files)"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Optional maximum lines to return per file (default: 200)"
                    }
                },
                "required": ["paths"]
            }),
        },
        ToolSchema {
            name: "look_at".to_string(),
            description: "Inspect a file's structure, imports, exports, and Tree-sitter AST symbol outline with exact 1-indexed line spans without loading the entire file into context. Returns full content automatically for small files (<= 150 lines). Use this first on unfamiliar or large files before calling read_file with start_line/end_line.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file within the workspace"
                    },
                    "extract": {
                        "type": "string",
                        "enum": ["all", "structure", "imports", "exports"],
                        "description": "Optional focus filter: 'all' (default), 'structure' (AST symbols/headings), 'imports', or 'exports'"
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
            name: "replace_in_files".to_string(),
            description: "Batch refactoring: Atomically find and replace text across multiple workspace files matching a glob pattern (e.g. 'src/components/*.tsx'). Use ONLY when the EXACT same search_block exists across 2 or more files (e.g. renaming a shared import or token). For single-file edits, always use patch_file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "glob": {
                        "type": "string",
                        "description": "File pattern to match (e.g. 'src/components/sections/*.tsx', '**/*.ts', 'src/**/*.rs')"
                    },
                    "search_block": {
                        "type": "string",
                        "description": "The exact unique code block or pattern to find in matching files"
                    },
                    "replace_block": {
                        "type": "string",
                        "description": "The replacement code block"
                    }
                },
                "required": ["glob", "search_block", "replace_block"]
            }),
        },
        ToolSchema {
            name: "write_file".to_string(),
            description: "Create a file, append to it, or overwrite it. Use for project-specific code and content. Not for project boilerplate: when starting a new project, scaffold with kit_stack_add first. Existing non-empty files are protected: pass overwrite=true to replace or append=true to add; prefer patch_file for targeted edits. Keep each call under ~200 lines / 8KB; larger payloads get truncated. Split work into modular files, or write a skeleton and then append sections.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "The text content to write or append"
                    },
                    "append": {
                        "type": "boolean",
                        "description": "If true, appends content to the existing file rather than overwriting it (essential for large files generated in sequential section chunks)"
                    },
                    "overwrite": {
                        "type": "boolean",
                        "description": "If true, explicitly allows replacing the entire contents of an existing file. Required when modifying an existing non-empty file without appending."
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

fn prepare_file_mutation(
    workspace_root: &Path,
    path: &str,
    backup_manager: Option<&BackupManager>,
    turn_id: usize,
    tool_name: &str,
) -> Result<std::path::PathBuf> {
    let validated_path =
        crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(path))?;

    if let Some(mgr) = backup_manager {
        if let Err(e) = mgr.create_checkpoint(workspace_root, &validated_path, turn_id) {
            tracing::warn!(
                path = %validated_path.display(),
                tool = tool_name,
                error = %e,
                "Failed to create safety checkpoint before file mutation"
            );
        }
    }

    if let Err(e) = crate::session::transaction::TransactionManager::record_mutation_pre(
        workspace_root,
        &validated_path,
    ) {
        tracing::warn!(
            path = %validated_path.display(),
            tool = tool_name,
            error = %e,
            "Failed to record transaction pre-mutation hook"
        );
    }

    Ok(validated_path)
}

fn finish_file_mutation(workspace_root: &Path, validated_path: &Path) {
    let _ = crate::session::transaction::TransactionManager::record_mutation_post(
        workspace_root,
        validated_path,
    );
}

fn finalize_file_mutation_with_hygiene(
    workspace_root: &Path,
    validated_path: &Path,
    relative_path: &str,
    added_content: &str,
    out: &mut String,
) {
    finish_file_mutation(workspace_root, validated_path);
    if let Some(warning) = detect_code_elision_or_slop(relative_path, added_content) {
        out.push_str(&warning);
    }
    if let Some(violation) =
        check_workspace_constraints(workspace_root, relative_path, added_content)
    {
        out.push_str(&violation);
    }
    if let Some(taste_notice) =
        crate::blocks::taste::format_post_write_taste_notice(relative_path, added_content)
    {
        out.push_str(&taste_notice);
    }
    if let Some(subdir_rules) =
        crate::context::governance::dox::DoxEngine::inject_unseen_subdirectory_rules(
            workspace_root,
            relative_path,
        )
    {
        out.push_str(&subdir_rules);
    }
}

/// Scans newly written or patched source code for lazy AI comment elision placeholders
/// (e.g. `// ... rest of code ...`, `// ... existing code ...`, `/* ... existing ... */`)
/// so the agent immediately restores complete code before concluding the turn.
pub(crate) fn detect_code_elision_or_slop(path: &str, added_content: &str) -> Option<String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "c" | "cpp" | "java" | "svelte" | "vue"
    ) {
        return None;
    }

    let mut flagged = Vec::new();
    for (idx, line) in added_content.lines().enumerate() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        let is_comment =
            trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('#');
        if !is_comment {
            continue;
        }
        let is_elision = lower.contains("... rest of")
            || lower.contains("... existing")
            || lower.contains("...remaining")
            || lower.contains("... remaining")
            || lower.contains("... keep existing")
            || lower.contains("... unchanged")
            || lower.contains("rest of code")
            || lower.contains("existing code here")
            || lower.contains("implementation goes here");
        if is_elision {
            flagged.push(format!("L{}: `{}`", idx + 1, trimmed));
            if flagged.len() >= 3 {
                break;
            }
        }
    }

    if flagged.is_empty() {
        None
    } else {
        Some(format!(
            "\n\n⚠️ [Code Hygiene Warning — Elision & Slop Guard]: Detected lazy placeholder/elision comment(s) in `{}` ({}). Never leave `// ... rest of code ...` elisions in source files — patch them with the complete implementation immediately.",
            path,
            flagged.join(", ")
        ))
    }
}

/// Post-write constraint reviewer inspired by `micode`'s Mindmodel Constraint Reviewer.
/// Checks newly written/patched code against deterministic repository rules and `.minicode/RULES.md`
/// forbidden patterns, returning at most 2 actionable `<constraint-violations>` warnings.
pub(crate) fn check_workspace_constraints(
    workspace_root: &Path,
    path: &str,
    added_content: &str,
) -> Option<String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "c" | "cpp" | "java" | "svelte" | "vue"
    ) {
        return None;
    }

    let mut violations: Vec<String> = Vec::new();
    let max_violations = 2;

    // 1. Check for unresolved git merge conflict markers
    for (idx, line) in added_content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("<<<<<<<")
            || trimmed.starts_with("=======")
            || trimmed.starts_with(">>>>>>>")
        {
            violations.push(format!(
                "- [Rule: No unresolved merge conflict markers] Found `{}` at L{}",
                trimmed.chars().take(15).collect::<String>(),
                idx + 1
            ));
            break;
        }
    }

    // Load workspace rules from .minicode/minicode.md (or legacy .minicode/RULES.md / AGENTS.md)
    let rules_text = std::fs::read_to_string(workspace_root.join(crate::constants::RULES_MD_FILE))
        .or_else(|_| {
            std::fs::read_to_string(workspace_root.join(crate::constants::LEGACY_RULES_MD_FILE))
        })
        .or_else(|_| std::fs::read_to_string(workspace_root.join(crate::constants::AGENTS_MD_FILE)))
        .unwrap_or_default();
    let rules_lower = rules_text.to_ascii_lowercase();

    // 2. Rust library hygiene checks when workspace rules forbid .unwrap() or println!
    let norm_path = path.trim_start_matches("./").replace('\\', "/");
    let is_rust_lib_file = ext == "rs"
        && norm_path.starts_with("src/")
        && norm_path != "src/main.rs"
        && !norm_path.starts_with("src/bin/")
        && !norm_path.contains("/tests/");

    if is_rust_lib_file && violations.len() < max_violations {
        let forbids_unwrap = rules_lower.contains("never use `.unwrap()`")
            || rules_lower.contains("never use .unwrap()");
        let forbids_println = rules_lower.contains("never use `println!`")
            || rules_lower.contains("never use println!");

        let mut in_test_module = false;
        for (idx, line) in added_content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("mod tests") {
                in_test_module = true;
            }
            if in_test_module
                || trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
            {
                continue;
            }

            if forbids_unwrap
                && violations.len() < max_violations
                && trimmed.contains(".unwrap()")
                && !trimmed.contains("// allow")
            {
                violations.push(format!(
                    "- [Rule: Never use `.unwrap()` in non-test library code] Found at L{}: `{}`",
                    idx + 1,
                    trimmed.chars().take(80).collect::<String>()
                ));
            }
            if forbids_println
                && violations.len() < max_violations
                && (trimmed.contains("println!(") || trimmed.contains("eprintln!("))
                && !trimmed.contains("macro_rules!")
            {
                violations.push(format!(
                    "- [Rule: Never use `println!`/`eprintln!` in library code (use `tracing`)] Found at L{}: `{}`",
                    idx + 1,
                    trimmed.chars().take(80).collect::<String>()
                ));
            }
        }
    }

    // 3. Explicit Forbidden backticked literals in .minicode/RULES.md
    if !rules_text.is_empty() && violations.len() < max_violations {
        for rule_line in rules_text.lines() {
            if violations.len() >= max_violations {
                break;
            }
            let rl_trim = rule_line.trim();
            let rl_lower = rl_trim.to_ascii_lowercase();
            if !(rl_lower.starts_with("- forbidden:")
                || rl_lower.starts_with("forbidden:")
                || rl_lower.starts_with("- never use "))
            {
                continue;
            }
            // Extract backticked token
            let mut rest = rl_trim;
            while let Some(start) = rest.find('`') {
                let after = &rest[start + 1..];
                if let Some(end) = after.find('`') {
                    let token = &after[..end];
                    if token.len() >= 4
                        && token != ".unwrap()"
                        && token != "println!"
                        && token != "eprintln!"
                    {
                        let token_upper = token.to_ascii_uppercase();
                        let check_comments = token_upper.contains("TODO")
                            || token_upper.contains("FIXME")
                            || token_upper.contains("HACK");
                        for (idx, code_line) in added_content.lines().enumerate() {
                            let ct = code_line.trim();
                            if !check_comments
                                && (ct.starts_with("//")
                                    || ct.starts_with('#')
                                    || ct.starts_with("/*"))
                            {
                                continue;
                            }
                            if ct.contains(token) {
                                violations.push(format!(
                                    "- [Workspace Rule: {}] Found `{}` at L{}",
                                    rl_trim.trim_start_matches('-').trim(),
                                    token,
                                    idx + 1
                                ));
                                break;
                            }
                        }
                    }
                    rest = &after[end + 1..];
                } else {
                    break;
                }
            }
        }
    }

    if violations.is_empty() {
        None
    } else {
        Some(format!(
            "\n\n<constraint-violations file=\"{}\">\n{}\n</constraint-violations>\nPlease fix these workspace rule violations in your next step.",
            path,
            violations.join("\n")
        ))
    }
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
            let path = param::require_path(args, "read_file")?;
            let start_line = param::get_usize(args, "start_line");
            let end_line = param::get_usize(args, "end_line");
            fs::read_file(workspace_root, path, start_line, end_line)
        })()),
        "batch_read" => Some((|| {
            let paths_val = args
                .get("paths")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    crate::error::ToolError::invalid_args(
                        "batch_read",
                        "Missing required 'paths' array of file paths",
                    )
                })?;
            let paths: Vec<String> = paths_val
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            let max_lines = param::get_usize(args, "max_lines");
            fs::batch_read(workspace_root, &paths, max_lines)
        })()),
        "look_at" => Some((|| {
            let path = param::require_path(args, "look_at")?;
            let extract = param::opt_str(args, "extract");
            fs::look_at(workspace_root, path, extract)
        })()),
        "write_file" => Some(
            async {
                let path = param::require_path(args, "write_file")?;
                let content = param::require_str(args, "content", "write_file")?;
                let _file_lock =
                    crate::tools::concurrency::acquire_file_mutation_lock(workspace_root, path)
                        .await;
                let validated_path = prepare_file_mutation(
                    workspace_root,
                    path,
                    backup_manager,
                    turn_id,
                    "write_file",
                )?;

                let append = param::opt_bool(args, "append", false);
                let overwrite = param::opt_bool(args, "overwrite", false);
                let root_buf = workspace_root.to_path_buf();
                let path_str = path.to_string();
                let content_str = content.to_string();
                let res = tokio::task::spawn_blocking(move || {
                    fs::write_file_with_options(
                        &root_buf,
                        &path_str,
                        &content_str,
                        append,
                        overwrite,
                    )
                })
                .await
                .map_err(|e| crate::error::ToolError::invalid_args("write_file", e.to_string()))?;

                let mut res = res;
                if let Ok(ref mut out) = res {
                    finalize_file_mutation_with_hygiene(
                        workspace_root,
                        &validated_path,
                        path,
                        content,
                        out,
                    );
                }
                res
            }
            .await,
        ),
        "patch_file" => Some(
            async {
                let path = param::require_path(args, "patch_file")?;
                let search = param::require_search_block(args, "patch_file")?;
                let replace = param::require_replace_block(args, "patch_file")?;
                let _file_lock =
                    crate::tools::concurrency::acquire_file_mutation_lock(workspace_root, path)
                        .await;

                if search == replace {
                    crate::sandbox::path::validate_path_in_workspace(
                        workspace_root,
                        Path::new(path),
                    )?;
                    return fs::patch_file(workspace_root, path, search, replace);
                }

                let validated_path = prepare_file_mutation(
                    workspace_root,
                    path,
                    backup_manager,
                    turn_id,
                    "patch_file",
                )?;

                let root_buf = workspace_root.to_path_buf();
                let path_str = path.to_string();
                let search_str = search.to_string();
                let replace_str = replace.to_string();
                let res = tokio::task::spawn_blocking(move || {
                    fs::patch_file(&root_buf, &path_str, &search_str, &replace_str)
                })
                .await
                .map_err(|e| crate::error::ToolError::invalid_args("patch_file", e.to_string()))?;

                let mut res = res;
                if let Ok(ref mut out) = res {
                    finalize_file_mutation_with_hygiene(
                        workspace_root,
                        &validated_path,
                        path,
                        replace,
                        out,
                    );
                }
                res
            }
            .await,
        ),
        "replace_in_files" => Some(
            async {
                let glob_pattern = param::require_str_with_aliases(
                    args,
                    &["glob", "pattern", "glob_pattern", "path"],
                    "replace_in_files",
                )?;
                let search = param::require_search_block(args, "replace_in_files")?;
                let replace = param::require_replace_block(args, "replace_in_files")?;
                let root_buf = workspace_root.to_path_buf();
                let glob_str = glob_pattern.to_string();
                let search_str = search.to_string();
                let replace_str = replace.to_string();
                let has_backup = backup_manager.is_some();
                tokio::task::spawn_blocking(move || {
                    let local_bm = has_backup.then(|| BackupManager::new(&root_buf));
                    fs::replace_in_files(
                        &root_buf,
                        &glob_str,
                        &search_str,
                        &replace_str,
                        local_bm.as_ref(),
                        turn_id,
                    )
                })
                .await
                .map_err(|e| {
                    crate::error::ToolError::invalid_args("replace_in_files", e.to_string())
                })?
            }
            .await,
        ),
        "repair_patch" => Some(
            async {
                let path = param::require_path(args, "repair_patch")?;
                let search = param::require_search_block(args, "repair_patch")?;
                let replace = param::require_replace_block(args, "repair_patch")?;
                let verification_cmd =
                    param::opt_command(args).or_else(|| param::opt_str(args, "verification_cmd"));
                let _file_lock =
                    crate::tools::concurrency::acquire_file_mutation_lock(workspace_root, path)
                        .await;

                let validated_path = prepare_file_mutation(
                    workspace_root,
                    path,
                    backup_manager,
                    turn_id,
                    "repair_patch",
                )?;

                let result = crate::tools::repair::SurgicalRepairEngine::execute_surgical_repair(
                    workspace_root,
                    path,
                    search,
                    replace,
                    verification_cmd,
                )
                .await?;

                if result.success {
                    finish_file_mutation(workspace_root, &validated_path);
                    Ok(result.format_markdown())
                } else {
                    Err(crate::error::ToolError::PatchFailed {
                        path: path.to_string(),
                        reason: format!(
                            "Verification failed; changes automatically rolled back.\n\n{}",
                            result.format_markdown()
                        ),
                    }
                    .into())
                }
            }
            .await,
        ),
        "ast_replace_node" => Some(
            async {
                let path = param::require_path(args, "ast_replace_node")?;
                let symbol = param::require_str_with_aliases(
                    args,
                    &["symbol", "symbol_name", "name", "target_symbol"],
                    "ast_replace_node",
                )?;
                let replacement = param::require_str(args, "replacement_code", "ast_replace_node")?;
                let kind = param::opt_str(args, "kind");
                let _file_lock =
                    crate::tools::concurrency::acquire_file_mutation_lock(workspace_root, path)
                        .await;

                let validated_path = prepare_file_mutation(
                    workspace_root,
                    path,
                    backup_manager,
                    turn_id,
                    "ast_replace_node",
                )?;

                let root_buf = workspace_root.to_path_buf();
                let path_str = path.to_string();
                let symbol_str = symbol.to_string();
                let replacement_str = replacement.to_string();
                let kind_opt = kind.map(|s| s.to_string());
                let result = tokio::task::spawn_blocking(move || {
                    crate::context::ast_transform::AstTransformer::replace_node(
                        &root_buf,
                        &path_str,
                        &symbol_str,
                        &replacement_str,
                        kind_opt.as_deref(),
                    )
                })
                .await
                .map_err(|e| {
                    crate::error::ToolError::invalid_args("ast_replace_node", e.to_string())
                })??;
                let mut out = result.format_receipt();
                finalize_file_mutation_with_hygiene(
                    workspace_root,
                    &validated_path,
                    path,
                    replacement,
                    &mut out,
                );
                Ok(out)
            }
            .await,
        ),
        "begin_transaction" => Some((|| {
            let desc = param::require_str(args, "description", "begin_transaction")?;
            if let Ok(Some(active)) =
                crate::session::transaction::TransactionManager::get_active(workspace_root)
            {
                return Ok(format!(
                    "✔ Active workspace transaction '{}' already protecting workspace: {}\nAll subsequent file modifications are journaled in the WAL.",
                    active.tx_id, active.description
                ));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_workspace_constraints_detects_violations() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_constraint_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join(".minicode")).unwrap();
        std::fs::write(
            temp_dir.join(".minicode/RULES.md"),
            "# Workspace Rules\n- Never use `.unwrap()` in non-test code.\n- Never use `println!` in library code.\n- Forbidden: `std::mem::transmute`\n",
        )
        .unwrap();

        let bad_lib_code = "pub fn compute(x: Option<i32>) -> i32 {\n    println!(\"debug\");\n    x.unwrap()\n}\n";
        let warning = check_workspace_constraints(&temp_dir, "src/lib_mod.rs", bad_lib_code);
        assert!(warning.is_some());
        let w = warning.unwrap();
        assert!(w.contains("<constraint-violations file=\"src/lib_mod.rs\">"));
        assert!(w.contains("Never use `.unwrap()`"));
        assert!(w.contains("Never use `println!`"));

        // Inside #[cfg(test)] block, .unwrap() is allowed
        let test_code = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn it_works() {\n        let x = Some(1).unwrap();\n        assert_eq!(x, 1);\n    }\n}\n";
        assert!(check_workspace_constraints(&temp_dir, "src/lib_mod.rs", test_code).is_none());

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
