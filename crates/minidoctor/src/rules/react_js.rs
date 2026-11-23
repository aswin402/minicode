//! Native Tree-sitter AST rules for React, Next.js, Vite, Hono, TypeScript, JavaScript,
//! Prisma, Zod, and Pino (combining the best rules from `react-doctor` and `hono_doctor`).

use crate::detector::LoadedFile;
use crate::rules::make_diag;
use crate::types::{DetectedStack, Diagnostic, DiagnosticCategory, Severity};
use tree_sitter::{Node, Parser};

pub fn check_react_js(
    file: &LoadedFile,
    stacks: &[DetectedStack],
    run_security: bool,
    out: &mut Vec<Diagnostic>,
) {
    let mut parser = Parser::new();
    let lang = match file.ext.as_str() {
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "ts" | "mts" | "cts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        _ => tree_sitter_javascript::LANGUAGE.into(),
    };
    if parser.set_language(&lang).is_err() {
        return;
    }
    let Some(tree) = parser.parse(&file.content, None) else {
        return;
    };

    let bytes = file.content.as_bytes();
    let is_nextjs = stacks.contains(&DetectedStack::NextJs);
    let is_hono = stacks.contains(&DetectedStack::Hono) || file.content.contains("new Hono(");
    let has_pino = stacks.contains(&DetectedStack::Pino);
    let has_zod = stacks.contains(&DetectedStack::Zod);
    let is_use_client = file.content.lines().take(5).any(|l| {
        l.trim() == "'use client';"
            || l.trim() == "\"use client\";"
            || l.trim() == "'use client'"
            || l.trim() == "\"use client\""
    });

    let mut ctx = JsWalkCtx {
        file,
        bytes,
        is_nextjs,
        is_hono,
        has_pino,
        has_zod,
        is_use_client,
        run_security,
        component_depth: 0,
        out,
    };

    walk_js(tree.root_node(), 0, &mut ctx);
}

struct JsWalkCtx<'a> {
    file: &'a LoadedFile,
    bytes: &'a [u8],
    is_nextjs: bool,
    is_hono: bool,
    has_pino: bool,
    has_zod: bool,
    is_use_client: bool,
    run_security: bool,
    component_depth: usize,
    out: &'a mut Vec<Diagnostic>,
}

fn node_text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

fn looks_like_component_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name.len() >= 2
        && !name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
}

fn contains_jsx(node: Node) -> bool {
    if matches!(
        node.kind(),
        "jsx_element" | "jsx_self_closing_element" | "jsx_fragment"
    ) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_jsx(child) {
            return true;
        }
    }
    false
}

fn walk_js(node: Node, nesting: usize, ctx: &mut JsWalkCtx<'_>) {
    let kind = node.kind();
    let line = node.start_position().row + 1;
    let col = node.start_position().column + 1;

    // 1. Oversized function & nested React component definition detection
    let mut entered_component = false;
    if matches!(
        kind,
        "function_declaration" | "method_definition" | "arrow_function"
    ) {
        let span_lines = node
            .end_position()
            .row
            .saturating_sub(node.start_position().row)
            + 1;
        if span_lines > 180 && !ctx.file.is_test {
            ctx.out.push(make_diag(
                "core/god-function",
                Severity::Warning,
                DiagnosticCategory::Maintainability,
                &ctx.file.rel_path,
                line,
                col,
                format!("Function spans {span_lines} lines (> 180 line maintainability ceiling)"),
                "Extract cohesive sub-helpers or custom hooks to keep this function focused.",
                &ctx.file.content,
            ));
        }

        if kind == "function_declaration" {
            let fn_name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, ctx.bytes))
                .unwrap_or("");
            if looks_like_component_name(fn_name) && contains_jsx(node) {
                if ctx.component_depth > 0 && !ctx.file.is_test {
                    ctx.out.push(make_diag(
                        "react/nested-component-def",
                        Severity::Warning,
                        DiagnosticCategory::Performance,
                        &ctx.file.rel_path,
                        line,
                        col,
                        format!("Component `{fn_name}` defined inside another component's render body"),
                        format!("Move `function {fn_name}` to module top-level scope and pass data via props."),
                        &ctx.file.content,
                    ));
                }
                ctx.component_depth += 1;
                entered_component = true;
            }
        }
    }

    // 2. Deep nesting check
    let next_nesting = if matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "for_in_statement"
            | "while_statement"
            | "switch_statement"
            | "try_statement"
    ) {
        let d = nesting + 1;
        if d == 7 && !ctx.file.is_test {
            ctx.out.push(make_diag(
                "core/deep-nesting",
                Severity::Warning,
                DiagnosticCategory::Maintainability,
                &ctx.file.rel_path,
                line,
                col,
                "Control flow nested more than 6 levels deep",
                "Invert conditions with early `return`/`continue` guard clauses or extract a helper.",
                &ctx.file.content,
            ));
        }
        d
    } else {
        nesting
    };

    // 3. JSX attribute checks (`dangerouslySetInnerHTML`, `key={index}`, raw `<img>` in Next.js)
    if kind == "jsx_attribute" {
        let txt = node_text(node, ctx.bytes);
        if ctx.run_security
            && txt.starts_with("dangerouslySetInnerHTML")
            && !txt.contains("DOMPurify")
            && !txt.contains("sanitize")
        {
            ctx.out.push(make_diag(
                "react/dangerously-set-inner-html",
                Severity::Error,
                DiagnosticCategory::Security,
                &ctx.file.rel_path,
                line,
                col,
                "`dangerouslySetInnerHTML` used without explicit sanitization",
                "Render standard JSX or sanitize the HTML string with DOMPurify before injecting.",
                &ctx.file.content,
            ));
        } else if txt == "key={index}" || txt == "key={idx}" || txt == "key={i}" {
            ctx.out.push(make_diag(
                "react/index-as-key",
                Severity::Warning,
                DiagnosticCategory::Performance,
                &ctx.file.rel_path,
                line,
                col,
                format!("Array index (`{txt}`) used as React JSX key"),
                "Use a stable unique ID from the item (e.g. `key={item.id}`) instead of the loop index.",
                &ctx.file.content,
            ));
        }
    }

    if ctx.is_nextjs
        && !ctx.file.is_test
        && matches!(kind, "jsx_self_closing_element" | "jsx_opening_element")
    {
        if let Some(name_node) = node.child_by_field_name("name") {
            if node_text(name_node, ctx.bytes) == "img" {
                ctx.out.push(make_diag(
                    "next/img-element",
                    Severity::Warning,
                    DiagnosticCategory::Performance,
                    &ctx.file.rel_path,
                    line,
                    col,
                    "Raw `<img>` element used in Next.js component",
                    "Use `import Image from 'next/image'` for automatic optimization and CLS protection.",
                    &ctx.file.content,
                ));
            }
        }
    }

    // 4. Next.js `'use client'` server-only env leak
    if ctx.run_security && ctx.is_use_client && !ctx.file.is_test && kind == "member_expression" {
        let txt = node_text(node, ctx.bytes);
        if let Some(var_name) = txt.strip_prefix("process.env.") {
            let clean = var_name
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            if !clean.is_empty() && !clean.starts_with("NEXT_PUBLIC_") && clean != "NODE_ENV" {
                ctx.out.push(make_diag(
                    "next/client-secret-env",
                    Severity::Critical,
                    DiagnosticCategory::Security,
                    &ctx.file.rel_path,
                    line,
                    col,
                    format!("Non-public `process.env.{clean}` referenced inside `'use client'` module"),
                    "Move server secret access into a Server Component / API Route, or prefix with `NEXT_PUBLIC_` if safe for browsers.",
                    &ctx.file.content,
                ));
            }
        }
    }

    // 5. Call expression checks (`useEffect(async ...)`, `eval(...)`, `$queryRaw`, `console.log`, `c.req.json()`)
    if kind == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            let fn_txt = node_text(func, ctx.bytes);

            // 5a. `useEffect(async () => ...)`
            if fn_txt == "useEffect" || fn_txt == "React.useEffect" {
                if let Some(args) = node.child_by_field_name("arguments") {
                    if let Some(first_arg) = args.named_child(0) {
                        let arg_txt = node_text(first_arg, ctx.bytes).trim_start();
                        if arg_txt.starts_with("async ") || arg_txt.starts_with("async(") {
                            ctx.out.push(make_diag(
                                "react/async-use-effect",
                                Severity::Error,
                                DiagnosticCategory::Correctness,
                                &ctx.file.rel_path,
                                line,
                                col,
                                "`async` callback passed directly to `useEffect` — returns a Promise instead of cleanup",
                                "Declare an inner `async function run()` inside `useEffect` and handle cancellation in the cleanup return.",
                                &ctx.file.content,
                            ));
                        }
                    }
                }
            }

            // 5b. `eval(...)`
            if ctx.run_security && !ctx.file.is_test && fn_txt == "eval" {
                ctx.out.push(make_diag(
                    "ts/eval-usage",
                    Severity::Critical,
                    DiagnosticCategory::Security,
                    &ctx.file.rel_path,
                    line,
                    col,
                    "Unsafe dynamic execution via `eval()`",
                    "Eliminate `eval()` and use `JSON.parse()` or an explicit safe evaluator.",
                    &ctx.file.content,
                ));
            }

            // 5c. Prisma / Hono raw SQL template interpolation (`$queryRawUnsafe` or `$queryRaw` with `${...}`)
            if ctx.run_security
                && !ctx.file.is_test
                && (fn_txt.ends_with("$queryRawUnsafe") || fn_txt.ends_with("$executeRawUnsafe"))
            {
                let full_call = node_text(node, ctx.bytes);
                if full_call.contains("${") {
                    ctx.out.push(make_diag(
                        "hono/sql-injection",
                        Severity::Critical,
                        DiagnosticCategory::Security,
                        &ctx.file.rel_path,
                        line,
                        col,
                        format!("`{fn_txt}` called with interpolated template literal — SQL injection risk"),
                        "Use parameterized `$queryRaw` tagged templates or explicit `$1`/`?` query parameters.",
                        &ctx.file.content,
                    ));
                }
            }

            // 5d. `console.log` / `console.error` when Pino is configured
            if ctx.has_pino
                && !ctx.file.is_test
                && matches!(fn_txt, "console.log" | "console.error" | "console.warn")
            {
                ctx.out.push(make_diag(
                    "pino/console-instead-of-logger",
                    Severity::Warning,
                    DiagnosticCategory::Maintainability,
                    &ctx.file.rel_path,
                    line,
                    col,
                    format!("`{fn_txt}()` bypasses structured Pino logging"),
                    "Use your Pino `logger.info(...)` / `logger.error(...)` instance instead.",
                    &ctx.file.content,
                ));
            }

            // 5e. `c.req.json()` without Zod validation in Hono + Zod project
            if ctx.is_hono
                && ctx.has_zod
                && !ctx.file.is_test
                && fn_txt.ends_with(".req.json")
                && !ctx.file.content.contains("zValidator")
                && !ctx.file.content.contains(".safeParse(")
                && !ctx.file.content.contains(".parse(")
            {
                ctx.out.push(make_diag(
                    "hono/unvalidated-json-body",
                    Severity::Warning,
                    DiagnosticCategory::Correctness,
                    &ctx.file.rel_path,
                    line,
                    col,
                    "`c.req.json()` read without Zod schema validation in this file",
                    "Validate input with `zValidator('json', schema)` or `schema.safeParse(body)`.",
                    &ctx.file.content,
                ));
            }
        }
    }

    // 6. Tagged template `$queryRaw` with interpolated `${...}` where SQL keywords appear
    if ctx.run_security && !ctx.file.is_test && kind == "call_expression" {
        let full = node_text(node, ctx.bytes);
        if (full.contains(".$queryRaw`") || full.contains(".$executeRaw`"))
            && full.contains("${")
            && !full.contains("Prisma.sql")
            && full.contains("Prisma.raw(")
        {
            ctx.out.push(make_diag(
                "hono/sql-injection",
                Severity::Critical,
                DiagnosticCategory::Security,
                &ctx.file.rel_path,
                line,
                col,
                "Raw SQL query uses `Prisma.raw()` interpolation inside template literal",
                "Pass dynamic values directly as parameterized tagged-template placeholders without `Prisma.raw()`.",
                &ctx.file.content,
            ));
        }
    }

    // 7. `new Function(...)` & `new Hono()` missing rate limiting
    if kind == "new_expression" && !ctx.file.is_test {
        if let Some(ctor) = node.child_by_field_name("constructor") {
            let ctor_txt = node_text(ctor, ctx.bytes);
            if ctx.run_security && ctor_txt == "Function" {
                ctx.out.push(make_diag(
                    "ts/eval-usage",
                    Severity::Critical,
                    DiagnosticCategory::Security,
                    &ctx.file.rel_path,
                    line,
                    col,
                    "Unsafe dynamic code compilation via `new Function()`",
                    "Replace `new Function()` with static functions or safe data-driven logic.",
                    &ctx.file.content,
                ));
            } else if ctx.run_security
                && ctor_txt == "Hono"
                && (ctx.file.rel_path.contains("index.")
                    || ctx.file.rel_path.contains("app.")
                    || ctx.file.rel_path.contains("server."))
                && !ctx.file.content.contains("rateLimit")
                && !ctx.file.content.contains("rateLimiter")
            {
                ctx.out.push(make_diag(
                    "hono/missing-rate-limit",
                    Severity::Warning,
                    DiagnosticCategory::Security,
                    &ctx.file.rel_path,
                    line,
                    col,
                    "Hono app entrypoint has no rate-limiting middleware configured",
                    "Add `hono-rate-limiter` (or edge throttling) on public API routes.",
                    &ctx.file.content,
                ));
            }
        }
    }

    // 8. Empty `catch` block silently swallowing exceptions
    if kind == "catch_clause" && !ctx.file.is_test {
        if let Some(body) = node.child_by_field_name("body") {
            if body.named_child_count() == 0 {
                ctx.out.push(make_diag(
                    "js/empty-catch",
                    Severity::Warning,
                    DiagnosticCategory::Correctness,
                    &ctx.file.rel_path,
                    line,
                    col,
                    "Empty `catch` block silently swallows errors",
                    "Log, handle, or rethrow the caught error (or add a clarifying comment).",
                    &ctx.file.content,
                ));
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_js(child, next_nesting, ctx);
    }

    if entered_component {
        ctx.component_depth = ctx.component_depth.saturating_sub(1);
    }
}
