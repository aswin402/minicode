//! Real native Rust structural scanners for Flutter/Dart (zero-regex token lexer & brace-balanced
//! state machine) and Go.

use crate::detector::LoadedFile;
use crate::rules::make_diag;
use crate::types::{Diagnostic, DiagnosticCategory, Severity};

const FLUTTER_CONTROLLER_TYPES: &[&str] = &[
    "TextEditingController",
    "AnimationController",
    "ScrollController",
    "PageController",
    "TabController",
    "FocusNode",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum DartTok<'a> {
    Ident(&'a str),
    Number,
    Punct(u8),
}

/// Deterministic zero-regex Dart line lexer (skips inline comments and string literals).
fn lex_dart_line(line: &str) -> Vec<DartTok<'_>> {
    let bytes = line.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            break;
        }
        if b == b'\'' || b == b'"' {
            let q = b;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == q {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            toks.push(DartTok::Ident(&line[start..i]));
            continue;
        }
        if b.is_ascii_digit() {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            toks.push(DartTok::Number);
            continue;
        }
        toks.push(DartTok::Punct(b));
        i += 1;
    }
    toks
}

/// Extracts `<ControllerType> <varName>` declarations from a tokenized Dart line.
fn extract_controller_var<'a>(toks: &[DartTok<'a>]) -> Option<&'a str> {
    for w in toks.windows(2) {
        if let [DartTok::Ident(ty), DartTok::Ident(var)] = w {
            if FLUTTER_CONTROLLER_TYPES.contains(ty) {
                return Some(var);
            }
        }
    }
    None
}

/// Detects non-const `SizedBox(height|width: <num>)` or `EdgeInsets.all(<num>)` via token grammar.
fn has_missing_const_widget(toks: &[DartTok<'_>]) -> bool {
    for i in 0..toks.len() {
        let preceded_by_const = i > 0 && toks[i - 1] == DartTok::Ident("const");
        if preceded_by_const {
            continue;
        }
        // Pattern A: `SizedBox` `(` (`height` | `width`) `:` `<num>` `)`
        if let Some(
            [DartTok::Ident("SizedBox"), DartTok::Punct(b'('), DartTok::Ident(dim), DartTok::Punct(b':'), DartTok::Number, DartTok::Punct(b')')],
        ) = toks.get(i..i + 6)
        {
            if matches!(*dim, "height" | "width") {
                return true;
            }
        }
        // Pattern B: `EdgeInsets` `.` `all` `(` `<num>` `)`
        if let Some(
            [DartTok::Ident("EdgeInsets"), DartTok::Punct(b'.'), DartTok::Ident("all"), DartTok::Punct(b'('), DartTok::Number, DartTok::Punct(b')')],
        ) = toks.get(i..i + 6)
        {
            return true;
        }
    }
    false
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
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            let toks = lex_dart_line(line);
            if let Some(var) = extract_controller_var(&toks) {
                if !src.contains(&format!("{var}.dispose()"))
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

        let toks = lex_dart_line(line);
        for tok in &toks {
            match tok {
                DartTok::Punct(b'{') => brace_depth += 1,
                DartTok::Punct(b'}') => brace_depth -= 1,
                _ => {}
            }
        }
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
        if missing_const_hits < 3 && has_missing_const_widget(&toks) {
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
