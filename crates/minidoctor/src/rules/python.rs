//! Native Tree-sitter AST rules for Python, FastAPI, SQLAlchemy, and Pydantic
//! (porting the highest-signal semantic rules from `aswin402/py_doctor` directly into native Rust).

use crate::detector::LoadedFile;
use crate::rules::make_diag;
use crate::types::{DetectedStack, Diagnostic, DiagnosticCategory, Severity};
use tree_sitter::{Node, Parser};

const ROUTE_METHODS: &[&str] = &[
    "get",
    "post",
    "put",
    "delete",
    "patch",
    "options",
    "head",
    "api_route",
];
const ASYNC_DB_CALLS: &[&str] = &[
    "db.execute",
    "session.execute",
    "db.commit",
    "session.commit",
    "db.flush",
    "session.flush",
    "db.refresh",
    "session.refresh",
    "db.scalar",
    "session.scalar",
    "db.scalars",
    "session.scalars",
    "client.get",
    "client.post",
    "client.put",
    "client.delete",
    "client.patch",
];

pub fn check_python(
    file: &LoadedFile,
    stacks: &[DetectedStack],
    run_security: bool,
    out: &mut Vec<Diagnostic>,
) {
    let mut parser = Parser::new();
    let lang = tree_sitter_python::LANGUAGE.into();
    if parser.set_language(&lang).is_err() {
        return;
    }
    let Some(tree) = parser.parse(&file.content, None) else {
        return;
    };

    let is_fastapi = stacks.contains(&DetectedStack::FastApi)
        || file.content.contains("FastAPI(")
        || file.content.contains("APIRouter(");
    let is_async_sqlalchemy = file.content.contains("AsyncSession")
        || file.content.contains("async_session")
        || file.content.contains("create_async_engine");

    let mut ctx = PyWalkCtx {
        file,
        bytes: file.content.as_bytes(),
        is_fastapi,
        is_async_sqlalchemy,
        run_security,
        out,
    };

    walk_py(tree.root_node(), false, 0, &mut ctx);
}

struct PyWalkCtx<'a> {
    file: &'a LoadedFile,
    bytes: &'a [u8],
    is_fastapi: bool,
    is_async_sqlalchemy: bool,
    run_security: bool,
    out: &'a mut Vec<Diagnostic>,
}

fn txt<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

fn is_async_function(fn_node: Node, bytes: &[u8]) -> bool {
    txt(fn_node, bytes).trim_start().starts_with("async def ")
}

fn walk_py(node: Node, in_async_fn: bool, nesting: usize, ctx: &mut PyWalkCtx<'_>) {
    let kind = node.kind();
    let line = node.start_position().row + 1;
    let col = node.start_position().column + 1;

    // 1. Decorated FastAPI routes (`missing-response-model`, `status-204-with-body`)
    if kind == "decorated_definition" && ctx.is_fastapi && !ctx.file.is_test {
        check_fastapi_decorated_route(node, ctx);
    }

    // 2. Function definitions: mutable defaults, god-function, async scope tracking
    let mut next_async = in_async_fn;
    if kind == "function_definition" {
        next_async = is_async_function(node, ctx.bytes);
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
                format!("Python function spans {span_lines} lines (> 180 line ceiling)"),
                "Split business logic into focused service helpers.",
                &ctx.file.content,
            ));
        }

        if let Some(params) = node.child_by_field_name("parameters") {
            let mut p_cur = params.walk();
            for param in params.children(&mut p_cur) {
                if matches!(
                    param.kind(),
                    "default_parameter" | "typed_default_parameter"
                ) {
                    if let Some(val) = param.child_by_field_name("value") {
                        let val_k = val.kind();
                        let val_t = txt(val, ctx.bytes).trim();
                        if matches!(val_k, "list" | "dictionary" | "set")
                            || val_t == "set()"
                            || val_t == "dict()"
                            || val_t == "list()"
                        {
                            ctx.out.push(make_diag(
                                "py/mutable-default-arg",
                                Severity::Error,
                                DiagnosticCategory::Correctness,
                                &ctx.file.rel_path,
                                val.start_position().row + 1,
                                val.start_position().column + 1,
                                format!("Mutable default argument `{val_t}` is shared across all calls"),
                                "Default the parameter to `None` and initialize inside the function body.",
                                &ctx.file.content,
                            ));
                        }
                    }
                }
            }
        }
    }

    // 3. Deep nesting check
    let next_nesting = if matches!(
        kind,
        "if_statement" | "for_statement" | "while_statement" | "try_statement" | "with_statement"
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
                "Use early returns / guard clauses to flatten nested branches.",
                &ctx.file.content,
            ));
        }
        d
    } else {
        nesting
    };

    // 4. Bare `except:` clause
    if kind == "except_clause" && !ctx.file.is_test {
        let header = txt(node, ctx.bytes).lines().next().unwrap_or("").trim();
        if header == "except:" {
            ctx.out.push(make_diag(
                "py/bare-except",
                Severity::Warning,
                DiagnosticCategory::Correctness,
                &ctx.file.rel_path,
                line,
                col,
                "Bare `except:` catches `KeyboardInterrupt` and `SystemExit`",
                "Use `except Exception as exc:` or catch a specific exception type.",
                &ctx.file.content,
            ));
        }
    }

    // 5. Call checks
    if kind == "call" {
        let fn_txt = node
            .child_by_field_name("function")
            .map(|n| txt(n, ctx.bytes))
            .unwrap_or("");
        let full_call = txt(node, ctx.bytes);

        // 5a. Blocking sync I/O inside `async def`
        if in_async_fn && !ctx.file.is_test {
            let blocking_hint = if fn_txt == "time.sleep" {
                Some("Replace `time.sleep()` with `await asyncio.sleep()`")
            } else if matches!(
                fn_txt,
                "requests.get"
                    | "requests.post"
                    | "requests.put"
                    | "requests.delete"
                    | "requests.patch"
                    | "requests.request"
            ) {
                Some("Replace synchronous `requests.*` with `await httpx.AsyncClient().*` or `asyncio.to_thread()`")
            } else if matches!(
                fn_txt,
                "subprocess.run"
                    | "subprocess.call"
                    | "subprocess.check_output"
                    | "subprocess.check_call"
            ) {
                Some("Replace `subprocess.*` with `await asyncio.create_subprocess_exec()` or `asyncio.to_thread()`")
            } else {
                None
            };

            if let Some(hint) = blocking_hint {
                ctx.out.push(make_diag(
                    "fastapi/sync-in-async-route",
                    Severity::Critical,
                    DiagnosticCategory::Performance,
                    &ctx.file.rel_path,
                    line,
                    col,
                    format!("Blocking call `{fn_txt}()` inside `async def` freezes the event loop"),
                    hint,
                    &ctx.file.content,
                ));
            }

            // 5b. Unawaited async DB / httpx call inside `async def`
            if ctx.is_async_sqlalchemy
                && ASYNC_DB_CALLS.contains(&fn_txt)
                && node.parent().is_none_or(|p| p.kind() != "await")
            {
                ctx.out.push(make_diag(
                    "fastapi/missing-await",
                    Severity::Critical,
                    DiagnosticCategory::Correctness,
                    &ctx.file.rel_path,
                    line,
                    col,
                    format!("`{fn_txt}()` called inside `async def` without `await` — returns unexecuted coroutine"),
                    format!("Add `await` before `{fn_txt}(...)`."),
                    &ctx.file.content,
                ));
            }
        }

        // 5c. FastAPI `CORSMiddleware` with wildcard origin + `allow_credentials=True`
        if ctx.run_security
            && !ctx.file.is_test
            && full_call.contains("CORSMiddleware")
            && (full_call.contains("[\"*\"]") || full_call.contains("['*']"))
            && full_call.contains("allow_credentials=True")
        {
            ctx.out.push(make_diag(
                "fastapi/insecure-cors",
                Severity::Critical,
                DiagnosticCategory::Security,
                &ctx.file.rel_path,
                line,
                col,
                "CORSMiddleware combines `allow_origins=[\"*\"]` with `allow_credentials=True`",
                "Restrict `allow_origins` to explicit trusted origins or set `allow_credentials=False`.",
                &ctx.file.content,
            ));
        }

        // 5d. FastAPI `background_tasks.add_task(..., db)` request-scoped DB session leak
        if ctx.is_fastapi && !ctx.file.is_test && fn_txt.ends_with(".add_task") {
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut ac = args.walk();
                for arg in args.named_children(&mut ac) {
                    let arg_t = txt(arg, ctx.bytes).trim();
                    let arg_l = arg_t.to_ascii_lowercase();
                    if matches!(
                        arg_l.as_str(),
                        "db" | "session" | "db_session" | "async_session"
                    ) || arg_l.ends_with("=db")
                        || arg_l.ends_with("=session")
                    {
                        ctx.out.push(make_diag(
                            "fastapi/background-task-db-session",
                            Severity::Critical,
                            DiagnosticCategory::Correctness,
                            &ctx.file.rel_path,
                            line,
                            col,
                            format!("Request-scoped DB session `{arg_t}` passed to `background_tasks.add_task()`"),
                            "Pass entity IDs to the background task and open an independent DB session inside it.",
                            &ctx.file.content,
                        ));
                    }
                }
            }
        }

        // 5e. SQL f-string injection inside `.execute(f"SELECT ...")`
        if ctx.run_security
            && !ctx.file.is_test
            && (fn_txt.ends_with(".execute") || fn_txt == "text")
            && (full_call.contains("f\"SELECT")
                || full_call.contains("f'SELECT")
                || full_call.contains("f\"DELETE")
                || full_call.contains("f'DELETE")
                || full_call.contains("f\"UPDATE")
                || full_call.contains("f'UPDATE")
                || full_call.contains("f\"INSERT")
                || full_call.contains("f'INSERT"))
        {
            ctx.out.push(make_diag(
                "py/sql-fstring-injection",
                Severity::Critical,
                DiagnosticCategory::Security,
                &ctx.file.rel_path,
                line,
                col,
                "SQL query constructed with Python f-string interpolation — SQL injection risk",
                "Use parameterized queries (`text(\"SELECT ... WHERE id = :id\"), {\"id\": val}`).",
                &ctx.file.content,
            ));
        }

        // 5f. `subprocess.*(..., shell=True)` command injection hazard (sync or async)
        if ctx.run_security
            && !ctx.file.is_test
            && matches!(
                fn_txt,
                "subprocess.run"
                    | "subprocess.call"
                    | "subprocess.Popen"
                    | "subprocess.check_output"
                    | "subprocess.check_call"
            )
            && (full_call.contains("shell=True") || full_call.contains("shell = True"))
        {
            ctx.out.push(make_diag(
                "py/subprocess-shell-true",
                Severity::Critical,
                DiagnosticCategory::Security,
                &ctx.file.rel_path,
                line,
                col,
                format!("`{fn_txt}(..., shell=True)` enables shell command injection"),
                "Pass an argument list (`[\"cmd\", arg]`) with `shell=False` instead of invoking a shell.",
                &ctx.file.content,
            ));
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_py(child, next_async, next_nesting, ctx);
    }
}

fn check_fastapi_decorated_route(decorated: Node, ctx: &mut PyWalkCtx<'_>) {
    let mut route_decorators = Vec::new();
    let mut fn_node: Option<Node> = None;

    let mut cursor = decorated.walk();
    for child in decorated.children(&mut cursor) {
        if child.kind() == "decorator" {
            let dec_txt = txt(child, ctx.bytes);
            let is_route = ROUTE_METHODS
                .iter()
                .any(|m| dec_txt.contains(&format!(".{m}(")));
            if is_route {
                route_decorators.push(child);
            }
        } else if child.kind() == "function_definition" {
            fn_node = Some(child);
        }
    }

    let Some(func) = fn_node else {
        return;
    };
    let has_return_annotation = func.child_by_field_name("return_type").is_some();

    for dec in route_decorators {
        let dec_txt = txt(dec, ctx.bytes);
        let dec_line = dec.start_position().row + 1;
        let has_response_model =
            dec_txt.contains("response_model=") && !dec_txt.contains("response_model=None");
        let is_204 = dec_txt.contains("status_code=204")
            || dec_txt.contains("HTTP_204_NO_CONTENT")
            || dec_txt.contains("NO_CONTENT");

        if is_204 {
            if has_response_model {
                ctx.out.push(make_diag(
                    "fastapi/status-204-with-body",
                    Severity::Warning,
                    DiagnosticCategory::Correctness,
                    &ctx.file.rel_path,
                    dec_line,
                    1,
                    "HTTP 204 route declares `response_model` — RFC 9110 forbids a 204 response body",
                    "Remove `response_model=...` from the decorator or use status code 200/202.",
                    &ctx.file.content,
                ));
            }
            check_204_returns(func, ctx);
        } else if !has_response_model && !has_return_annotation {
            ctx.out.push(make_diag(
                "fastapi/missing-response-model",
                Severity::Warning,
                DiagnosticCategory::Architecture,
                &ctx.file.rel_path,
                func.start_position().row + 1,
                1,
                "FastAPI route missing `response_model` or return type annotation",
                "Add `response_model=ResponseSchema` or `-> ResponseSchema` to enforce the API contract.",
                &ctx.file.content,
            ));
        }
    }
}

fn check_204_returns(fn_node: Node, ctx: &mut PyWalkCtx<'_>) {
    let mut stack = vec![fn_node];
    while let Some(curr) = stack.pop() {
        let mut cursor = curr.walk();
        for child in curr.children(&mut cursor) {
            if child.kind() == "return_statement" {
                let r_txt = txt(child, ctx.bytes).trim();
                let payload = r_txt.strip_prefix("return").unwrap_or("").trim();
                if !payload.is_empty() && payload != "None" && !payload.starts_with("Response(") {
                    ctx.out.push(make_diag(
                        "fastapi/status-204-with-body",
                        Severity::Warning,
                        DiagnosticCategory::Correctness,
                        &ctx.file.rel_path,
                        child.start_position().row + 1,
                        1,
                        "HTTP 204 (No Content) route returns a body payload",
                        "Return `None` or `Response(status_code=204)`.",
                        &ctx.file.content,
                    ));
                }
            } else if !matches!(child.kind(), "function_definition" | "class_definition") {
                stack.push(child);
            }
        }
    }
}
