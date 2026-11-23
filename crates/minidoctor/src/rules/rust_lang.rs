//! Native Tree-sitter AST rules for Rust, Tokio, Axum, and Ratatui codebases.

use crate::detector::LoadedFile;
use crate::rules::make_diag;
use crate::types::{DetectedStack, Diagnostic, DiagnosticCategory, Severity};
use tree_sitter::{Node, Parser};

pub fn check_rust(file: &LoadedFile, stacks: &[DetectedStack], out: &mut Vec<Diagnostic>) {
    let mut parser = Parser::new();
    let lang = tree_sitter_rust::LANGUAGE.into();
    if parser.set_language(&lang).is_err() {
        return;
    }
    let Some(tree) = parser.parse(&file.content, None) else {
        return;
    };

    let is_tui_or_lib = stacks.contains(&DetectedStack::Ratatui)
        && !file.rel_path.ends_with("main.rs")
        && !file.rel_path.contains("/bin/");

    let mut ctx = RustWalkCtx {
        file,
        bytes: file.content.as_bytes(),
        is_tui_or_lib,
        out,
    };

    walk_rust(tree.root_node(), file.is_test, false, 0, &mut ctx);
}

struct RustWalkCtx<'a> {
    file: &'a LoadedFile,
    bytes: &'a [u8],
    is_tui_or_lib: bool,
    out: &'a mut Vec<Diagnostic>,
}

fn txt<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Checks if a Rust item (`mod_item`, `function_item`) is preceded by a `#[cfg(test)]` or `#[test]` attribute.
fn has_test_attribute(node: Node, bytes: &[u8]) -> bool {
    let mut sib = node.prev_named_sibling();
    while let Some(s) = sib {
        if s.kind() == "attribute_item" {
            let t = txt(s, bytes);
            if t.contains("cfg(test)") || t.contains("#[test]") || t.contains("tokio::test") {
                return true;
            }
            sib = s.prev_named_sibling();
        } else {
            break;
        }
    }
    false
}

fn walk_rust(
    node: Node,
    in_test: bool,
    in_async_fn: bool,
    nesting: usize,
    ctx: &mut RustWalkCtx<'_>,
) {
    let kind = node.kind();

    // Skip `#[cfg(test)] mod ...` and `#[test] fn ...` subtrees for production rules
    let now_in_test = in_test
        || (matches!(kind, "mod_item" | "function_item") && has_test_attribute(node, ctx.bytes));
    if now_in_test && matches!(kind, "mod_item" | "function_item") {
        return;
    }

    let line = node.start_position().row + 1;
    let col = node.start_position().column + 1;

    let mut next_async = in_async_fn;
    if kind == "function_item" {
        let fn_txt = txt(node, ctx.bytes);
        let sig_head = fn_txt.split('{').next().unwrap_or("");
        next_async = sig_head.contains("async fn");

        let span_lines = node
            .end_position()
            .row
            .saturating_sub(node.start_position().row)
            + 1;
        if span_lines > 220 && !now_in_test {
            ctx.out.push(make_diag(
                "core/god-function",
                Severity::Warning,
                DiagnosticCategory::Maintainability,
                &ctx.file.rel_path,
                line,
                col,
                format!("Rust function spans {span_lines} lines (> 220 line ceiling)"),
                "Decompose into focused helper functions or cohesive submodules.",
                &ctx.file.content,
            ));
        }
    }

    let next_nesting = if matches!(
        kind,
        "if_expression"
            | "for_expression"
            | "while_expression"
            | "loop_expression"
            | "match_expression"
    ) {
        let d = nesting + 1;
        if d == 7 && !now_in_test {
            ctx.out.push(make_diag(
                "core/deep-nesting",
                Severity::Warning,
                DiagnosticCategory::Maintainability,
                &ctx.file.rel_path,
                line,
                col,
                "Control flow nested more than 6 levels deep",
                "Use early `return`/`continue`, `?`, or `let ... else` guard clauses.",
                &ctx.file.content,
            ));
        }
        d
    } else {
        nesting
    };

    // 1. `.unwrap()` / `.expect()` in non-test production code
    if !now_in_test && kind == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            let fn_t = txt(func, ctx.bytes);
            if func.kind() == "field_expression" {
                if let Some(field) = func.child_by_field_name("field") {
                    let method = txt(field, ctx.bytes);
                    if method == "unwrap" || method == "expect" {
                        ctx.out.push(make_diag(
                            "rust/unwrap-in-non-test",
                            Severity::Error,
                            DiagnosticCategory::Correctness,
                            &ctx.file.rel_path,
                            line,
                            col,
                            format!("`.{method}()` in non-test code can panic at runtime"),
                            "Propagate with `?`, `ok_or_else(...)`, `unwrap_or_default()`, or match explicitly.",
                            &ctx.file.content,
                        ));
                    }
                }
            }

            // 2. Blocking `std::thread::sleep` (Critical) or sync `std::fs::*` (Warning) directly inside `async fn`.
            // Only match the leaf `scoped_identifier` call so chained `.ok().and_then()` never double-reports.
            if in_async_fn && func.kind() == "scoped_identifier" {
                let exact = fn_t.trim();
                if exact == "std::thread::sleep" || exact == "thread::sleep" {
                    ctx.out.push(make_diag(
                        "rust/blocking-in-async",
                        Severity::Critical,
                        DiagnosticCategory::Performance,
                        &ctx.file.rel_path,
                        line,
                        col,
                        format!(
                            "Blocking call `{exact}()` inside `async fn` stalls Tokio worker threads"
                        ),
                        "Replace with `tokio::time::sleep(...).await`.",
                        &ctx.file.content,
                    ));
                } else if matches!(
                    exact,
                    "std::fs::read"
                        | "std::fs::read_to_string"
                        | "std::fs::write"
                        | "std::fs::read_dir"
                ) {
                    ctx.out.push(make_diag(
                        "rust/blocking-in-async",
                        Severity::Warning,
                        DiagnosticCategory::Performance,
                        &ctx.file.rel_path,
                        line,
                        col,
                        format!(
                            "Synchronous `{exact}()` inside `async fn` may block a Tokio worker thread"
                        ),
                        "Consider `tokio::fs` or `tokio::task::spawn_blocking` for non-trivial file I/O.",
                        &ctx.file.content,
                    ));
                }
            }
        }
    }

    // 3. `unsafe { ... }` without a preceding `// SAFETY:` comment
    if !now_in_test && kind == "unsafe_block" {
        let lines: Vec<&str> = ctx.file.content.lines().collect();
        let row = node.start_position().row;
        let check_start = row.saturating_sub(3);
        let has_safety_comment = (check_start..=row)
            .filter_map(|r| lines.get(r))
            .any(|l| l.to_ascii_uppercase().contains("SAFETY:"));
        if !has_safety_comment {
            ctx.out.push(make_diag(
                "rust/unsafe-without-safety-comment",
                Severity::Warning,
                DiagnosticCategory::Security,
                &ctx.file.rel_path,
                line,
                col,
                "`unsafe` block is missing a `// SAFETY:` invariant justification comment",
                "Document the upheld safety invariants with a `// SAFETY: ...` comment right above the block.",
                &ctx.file.content,
            ));
        }
    }

    // 4. `println!` / `eprintln!` in TUI/library code and leftover `todo!()` / `unimplemented!()`
    if !now_in_test && kind == "macro_invocation" {
        if let Some(mac) = node.child_by_field_name("macro") {
            let mac_name = txt(mac, ctx.bytes);
            if matches!(mac_name, "todo" | "unimplemented") {
                ctx.out.push(make_diag(
                    "rust/todo-macro-leftover",
                    Severity::Error,
                    DiagnosticCategory::Correctness,
                    &ctx.file.rel_path,
                    line,
                    col,
                    format!(
                        "`{mac_name}!()` macro left in production code — will panic if reached"
                    ),
                    "Implement the missing branch or return a typed error.",
                    &ctx.file.content,
                ));
            } else if ctx.is_tui_or_lib && matches!(mac_name, "println" | "eprintln") {
                ctx.out.push(make_diag(
                    "rust/println-in-tui-or-lib",
                    Severity::Warning,
                    DiagnosticCategory::Correctness,
                    &ctx.file.rel_path,
                    line,
                    col,
                    format!("`{mac_name}!` in Ratatui/library code can corrupt the alternate terminal screen"),
                    "Replace with `tracing::info!` / `tracing::debug!`.",
                    &ctx.file.content,
                ));
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_rust(child, now_in_test, next_async, next_nesting, ctx);
    }
}
