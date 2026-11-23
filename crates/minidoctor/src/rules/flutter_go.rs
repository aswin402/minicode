//! Real native Rust structural scanners for Flutter/Dart (upgrading `flutter_doctor`'s Phase-1
//! rule catalog into live static diagnostics) and Go.

use crate::detector::LoadedFile;
use crate::rules::make_diag;
use crate::types::{Diagnostic, DiagnosticCategory, Severity};
use regex::Regex;
use std::sync::OnceLock;

fn controller_decl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"\b(?:TextEditingController|AnimationController|ScrollController|PageController|TabController|FocusNode)\s+([_a-zA-Z]\w*)"#)
            .unwrap_or_else(|_| Regex::new("a^").unwrap_or_else(|_| unreachable!()))
    })
}

fn flutter_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"\b(SizedBox\((?:height|width)\s*:\s*\d+(?:\.\d+)?\)|EdgeInsets\.all\(\d+(?:\.\d+)?\))"#)
            .unwrap_or_else(|_| Regex::new("a^").unwrap_or_else(|_| unreachable!()))
    })
}

/// Evaluates real Flutter / Dart diagnostics on `.dart` files.
pub fn check_flutter_dart(file: &LoadedFile, out: &mut Vec<Diagnostic>) {
    if file.is_test {
        return;
    }
    let src = &file.content;
    let lines: Vec<&str> = src.lines().collect();

    // 1. `flutter/dispose-controllers`: Controllers declared in a `State<...>` without `.dispose()`
    if src.contains("extends State<") {
        let ctrl_re = controller_decl_re();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if let Some(caps) = ctrl_re.captures(line) {
                let var = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                if !var.is_empty()
                    && !src.contains(&format!("{var}.dispose()"))
                    && !src.contains(&format!("{var}?.dispose()"))
                {
                    out.push(make_diag(
                        "flutter/dispose-controllers",
                        Severity::Critical,
                        DiagnosticCategory::Performance,
                        &file.rel_path,
                        idx + 1,
                        1,
                        format!("Controller `{var}` declared in State without a matching `{var}.dispose()` call"),
                        format!("Call `{var}.dispose();` inside `override void dispose()` before `super.dispose()`."),
                        src,
                    ));
                }
            }
        }

        // 2. `flutter/unclosed-stream`: `.listen(` in State without `.cancel()`
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if !trimmed.starts_with("//")
                && trimmed.contains(".listen(")
                && !src.contains(".cancel()")
            {
                out.push(make_diag(
                    "flutter/unclosed-stream",
                    Severity::Error,
                    DiagnosticCategory::Performance,
                    &file.rel_path,
                    idx + 1,
                    1,
                    "Stream `.listen()` subscription created in State without any `.cancel()` cleanup",
                    "Store the `StreamSubscription` and call `_sub?.cancel()` inside `dispose()`.",
                    src,
                ));
                break;
            }
        }
    }

    // 3. `flutter/no-build-context-async`: Using `context` after `await` inside an `async` block without `mounted` check
    let mut in_async_method = false;
    let mut saw_await = false;
    let mut saw_mounted_guard = false;
    let mut brace_depth: i32 = 0;
    let mut async_start_depth: i32 = 0;

    let const_re = flutter_const_re();
    let mut missing_const_hits = 0usize;

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        if trimmed.contains(") async {") || trimmed.contains(") async =>") {
            in_async_method = true;
            saw_await = false;
            saw_mounted_guard = false;
            async_start_depth = brace_depth;
        }

        if in_async_method {
            if trimmed.contains("await ") {
                saw_await = true;
            }
            if trimmed.contains("mounted") {
                saw_mounted_guard = true;
            }
            if saw_await
                && !saw_mounted_guard
                && (trimmed.contains("Navigator.of(context)")
                    || trimmed.contains("ScaffoldMessenger.of(context)")
                    || trimmed.contains("Theme.of(context)")
                    || trimmed.contains("showDialog(") && trimmed.contains("context:"))
            {
                out.push(make_diag(
                    "flutter/no-build-context-async",
                    Severity::Critical,
                    DiagnosticCategory::Correctness,
                    &file.rel_path,
                    line_no,
                    1,
                    "`BuildContext` used after `await` without verifying `mounted` / `context.mounted`",
                    "Guard with `if (!mounted) return;` (or `if (!context.mounted) return;`) after the `await`.",
                    src,
                ));
                saw_mounted_guard = true; // emit once per async gap
            }
        }

        let opens = line.chars().filter(|&c| c == '{').count() as i32;
        let closes = line.chars().filter(|&c| c == '}').count() as i32;
        brace_depth += opens - closes;
        if in_async_method && brace_depth <= async_start_depth {
            in_async_method = false;
            saw_await = false;
            saw_mounted_guard = false;
        }

        // 4. `flutter/list-view-shrink-wrap`
        if trimmed.contains("shrinkWrap: true") {
            out.push(make_diag(
                "flutter/list-view-shrink-wrap",
                Severity::Warning,
                DiagnosticCategory::Performance,
                &file.rel_path,
                line_no,
                1,
                "`shrinkWrap: true` forces eager O(n) ListView layout",
                "Prefer `CustomScrollView` + `SliverList` or wrap `ListView.builder` inside `Expanded`.",
                src,
            ));
        }

        // 5. `flutter/missing-const` (capped to avoid noisy spam in a single file)
        if missing_const_hits < 3 && !trimmed.contains("const ") && const_re.is_match(trimmed) {
            missing_const_hits += 1;
            out.push(make_diag(
                "flutter/missing-const",
                Severity::Warning,
                DiagnosticCategory::Performance,
                &file.rel_path,
                line_no,
                1,
                "Compile-time constant widget constructor missing `const`",
                "Add `const` prefix so Flutter skips redundant widget allocations on rebuild.",
                src,
            ));
        }
    }
}

/// Evaluates real structural diagnostics on `.go` files.
pub fn check_go(file: &LoadedFile, run_security: bool, out: &mut Vec<Diagnostic>) {
    if file.is_test {
        return;
    }
    let src = &file.content;
    for (idx, line) in src.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }

        if run_security
            && (trimmed.contains(".Query(fmt.Sprintf(")
                || trimmed.contains(".Exec(fmt.Sprintf(")
                || trimmed.contains(".QueryRow(fmt.Sprintf("))
        {
            out.push(make_diag(
                "go/sql-concat-injection",
                Severity::Critical,
                DiagnosticCategory::Security,
                &file.rel_path,
                line_no,
                1,
                "SQL query constructed with `fmt.Sprintf` — SQL injection vulnerability",
                "Pass query parameters via placeholders (`db.Query(\"SELECT ... WHERE id = $1\", id)`).",
                src,
            ));
        }

        if trimmed.starts_with("_ = ")
            && (trimmed.contains(".Close(")
                || trimmed.contains(".Write(")
                || trimmed.contains("json.Unmarshal")
                || trimmed.contains(".Exec("))
        {
            out.push(make_diag(
                "go/ignored-error",
                Severity::Warning,
                DiagnosticCategory::Correctness,
                &file.rel_path,
                line_no,
                1,
                "I/O or DB error explicitly discarded with `_ = ...`",
                "Check and handle or propagate the returned `err` value.",
                src,
            ));
        }
    }
}
