use crate::agent::provider::ToolSchema;
use crate::error::Result;
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "repair_diagnostics".to_string(),
            description: "Autonomously run fast compiler/LSP diagnostics, triage and cluster errors by root cause, and execute a bounded self-healing loop to synthesize and apply surgical fixes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "dry_run": {
                        "type": "boolean",
                        "description": "If true, analyzes and triages compiler errors into primary root causes without modifying files (default: false)"
                    },
                    "max_attempts": {
                        "type": "integer",
                        "description": "Maximum number of self-healing repair attempts (default: 3, max: 5)"
                    },
                    "auto_apply_imports": {
                        "type": "boolean",
                        "description": "If true, automatically detects missing symbols in the workspace and inserts imports (default: true)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "synthesize_reproducer".to_string(),
            description: "Synthesize an isolated TDD bug reproducer test in 'tests/repro_<name>.rs'. Automatically executes Red Phase against unpatched codebase to prove that the bug is real (must fail). Warns if the test is vacuous (passes unexpectedly).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Unique identifier for the reproducer (e.g. 'null_pointer', 'donut_truncation', 'parser_bounds')"
                    },
                    "test_code": {
                        "type": "string",
                        "description": "Complete Rust integration test code for tests/repro_<name>.rs (e.g. '#[test] fn test_repro() { ... }')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Short explanation of what bug or edge case this reproducer isolates"
                    },
                    "run_red_phase": {
                        "type": "boolean",
                        "description": "Whether to immediately compile and run the reproducer to confirm it fails on unpatched code (default: true)"
                    }
                },
                "required": ["name", "test_code", "description"]
            }),
        },
        ToolSchema {
            name: "verify_reproducer".to_string(),
            description: "Execute and verify an active reproducer test target to check if it is still failing (RED) or now passing (GREEN) after source code edits.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name or test target of the reproducer to execute (e.g. 'null_pointer' or 'repro_null_pointer')"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "list_reproducers".to_string(),
            description: "List all active standalone bug reproducers, their Red-phase proof, and Green-phase verification status in the current workspace.".to_string(),
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
        "repair_diagnostics" => Some(
            async {
                let dry_run = param::opt_bool(args, "dry_run", false);
                let max_attempts = param::opt_u64(args, "max_attempts").unwrap_or(3) as usize;
                let auto_apply_imports = param::opt_bool(args, "auto_apply_imports", true);

                let report = crate::agent::self_healing::SelfHealingEngine::heal(
                    workspace_root,
                    max_attempts,
                    auto_apply_imports,
                    dry_run,
                )
                .await?;

                Ok(report.format_summary(workspace_root))
            }
            .await,
        ),
        "synthesize_reproducer" => Some(
            async {
                let name = param::require_str(args, "name", "synthesize_reproducer")?;
                let test_code = param::require_str(args, "test_code", "synthesize_reproducer")?;
                let description =
                    param::opt_str(args, "description").unwrap_or("TDD bug reproducer");
                let run_red_phase = param::opt_bool(args, "run_red_phase", true);

                let report =
                    crate::agent::reproducer_guard::ReproducerGuard::synthesize_rust_reproducer(
                        workspace_root,
                        name,
                        test_code,
                        description,
                        run_red_phase,
                    )?;

                Ok(report.format_message())
            }
            .await,
        ),
        "verify_reproducer" => Some(
            async {
                let name = param::require_str(args, "name", "verify_reproducer")?;

                let report = crate::agent::reproducer_guard::ReproducerGuard::verify_reproducer(
                    workspace_root,
                    name,
                )?;

                Ok(report.format_message())
            }
            .await,
        ),
        "list_reproducers" => Some(
            async {
                let records =
                    crate::agent::reproducer_guard::ReproducerGuard::list_active_reproducers(
                        workspace_root,
                    );
                Ok(
                    crate::agent::reproducer_guard::ReproducerGuard::format_reproducer_list(
                        &records,
                    ),
                )
            }
            .await,
        ),
        _ => None,
    }
}
