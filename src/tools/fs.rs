use crate::error::{Result, ToolError};
use crate::sandbox::path::validate_path_in_workspace;
use crate::session::backup::BackupManager;
use similar::TextDiff;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Reads a file from the workspace within the specified optional 1-indexed line range.
pub fn read_file(
    workspace_root: &Path,
    relative_path: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
    crate::context::governance::dox::DoxEngine::sync_and_harvest_rules(
        workspace_root,
        &[relative_path],
    );
    crate::context::governance::dox::DoxEngine::on_file_mutated(workspace_root, relative_path);

    // Handle visual image assets (.png, .jpg, .jpeg, .gif, .webp) with header inspection & multimodal attachment support
    if let Some(ext) = target_path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_ascii_lowercase();
        const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];
        if IMAGE_EXTENSIONS.contains(&ext_lower.as_str()) {
            let bytes = std::fs::read(&target_path).map_err(|e| ToolError::FileOp {
                path: relative_path.to_string(),
                source: e,
            })?;
            let dims = parse_image_dimensions(&bytes, &ext_lower)
                .map(|(w, h)| format!("{}x{} px", w, h))
                .unwrap_or_else(|| "unknown dimensions".to_string());
            let media_type = match ext_lower.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => "application/octet-stream",
            };
            return Ok(format!(
                "✔ [Visual Image Asset Loaded: `{}`]\n\
                 • **Format**: `{}` ({})\n\
                 • **Size**: {} bytes ({:.1} KB)\n\
                 • **Multimodal Vision**: Attached directly to your active multimodal context for pixel-level inspection (and supported in `Vision` MiniPub).",
                relative_path,
                media_type,
                dims,
                bytes.len(),
                bytes.len() as f64 / 1024.0
            ));
        }

        const BINARY_EXTENSIONS: &[&str] = &[
            "bmp", "ico", "tiff", "svgz", "pdf", "wasm", "zip", "tar", "gz", "tgz", "7z", "rar",
            "bin", "exe", "so", "dylib", "dll", "mp4", "webm", "mp3", "wav", "ogg", "woff",
            "woff2", "ttf", "eot", "otf",
        ];
        if BINARY_EXTENSIONS.contains(&ext_lower.as_str()) {
            return Err(ToolError::InvalidArguments {
                name: "read_file".to_string(),
                reason: format!(
                    "Cannot read binary asset file '{}' (.{}). Binary media/compiled files cannot be read as UTF-8 text.",
                    relative_path, ext_lower
                ),
            }
            .into());
        }
    }

    let content = std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    if content.is_empty() {
        return Ok(String::new());
    }

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    if total_lines == 0 {
        return Ok(String::new());
    }

    // Default window clamping: if no bounds provided and total lines exceed threshold, clamp window
    let is_unbounded = start_line.is_none() && end_line.is_none();
    let clamped_window =
        is_unbounded && total_lines > crate::constants::READ_FILE_UNBOUNDED_THRESHOLD;

    let start = start_line.unwrap_or(1).max(1);
    let end = if clamped_window {
        crate::constants::READ_FILE_DEFAULT_WINDOW
    } else {
        end_line.unwrap_or(total_lines).min(total_lines)
    };

    if start > total_lines {
        return Err(ToolError::InvalidArguments {
            name: "read_file".to_string(),
            reason: format!(
                "start_line ({}) exceeds total line count ({})",
                start, total_lines
            ),
        }
        .into());
    }

    if start > end {
        return Err(ToolError::InvalidArguments {
            name: "read_file".to_string(),
            reason: format!(
                "start_line ({}) cannot be greater than end_line ({})",
                start, end
            ),
        }
        .into());
    }

    let mut output = String::new();
    for line_idx in start..=end {
        if line_idx - 1 < total_lines {
            output.push_str(&format!("{:>4} | {}\n", line_idx, lines[line_idx - 1]));
        }
    }

    if clamped_window {
        output.push_str(&format!(
            "\n[... File has {} lines. Showing lines 1-{}. Call read_file with start_line={} to view next chunk ...]",
            total_lines,
            crate::constants::READ_FILE_DEFAULT_WINDOW,
            crate::constants::READ_FILE_DEFAULT_WINDOW + 1
        ));
        if let Some(outline_hint) = build_unbounded_read_ast_outline_hint(
            &target_path,
            relative_path,
            &content,
            &lines,
            crate::constants::READ_FILE_DEFAULT_WINDOW,
            total_lines,
        ) {
            output.push('\n');
            output.push_str(&outline_hint);
        }
    }

    if let Some(subdir_rules) =
        crate::context::governance::dox::DoxEngine::inject_unseen_subdirectory_rules(
            workspace_root,
            relative_path,
        )
    {
        output.push_str(&subdir_rules);
    }

    Ok(output)
}

/// Extracts a compact Tree-sitter AST symbol outline for the unread portion of a large file
/// (`line_number > shown_end_line`) when `read_file` clamps an unbounded read, allowing the
/// agent to jump directly to the needed function/struct line span without sequential paging.
fn build_unbounded_read_ast_outline_hint(
    target_path: &Path,
    relative_path: &str,
    content: &str,
    lines: &[&str],
    shown_end_line: usize,
    total_lines: usize,
) -> Option<String> {
    let ext_lower = target_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut entries = Vec::new();

    if matches!(ext_lower.as_str(), "md" | "mdx" | "markdown") {
        for (idx, line) in lines.iter().enumerate().skip(shown_end_line) {
            if line.starts_with('#') {
                entries.push(format!("  L{:>4}: {}", idx + 1, line.trim()));
            }
        }
    } else {
        let extractor = crate::context::ast::repomap::RepoMapExtractor::new();
        let ast_symbols = extractor
            .extract_symbols_from_str(&ext_lower, content)
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.kind != "import" && s.end_line > shown_end_line)
            .collect::<Vec<_>>();

        if !ast_symbols.is_empty() {
            for sym in ast_symbols.iter().take(35) {
                entries.push(format!(
                    "  L{:>4}-L{:<4} [{}] `{}`",
                    sym.line_number, sym.end_line, sym.kind, sym.signature
                ));
            }
            if ast_symbols.len() > 35 {
                entries.push(format!(
                    "  ... and {} more symbols in L{}-L{}",
                    ast_symbols.len() - 35,
                    shown_end_line + 1,
                    total_lines
                ));
            }
        } else {
            for (idx, line) in lines.iter().enumerate().skip(shown_end_line) {
                let trimmed = line.trim();
                if trimmed.starts_with("fn ")
                    || trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("async fn ")
                    || trimmed.starts_with("pub async fn ")
                    || trimmed.starts_with("struct ")
                    || trimmed.starts_with("pub struct ")
                    || trimmed.starts_with("enum ")
                    || trimmed.starts_with("trait ")
                    || trimmed.starts_with("impl")
                    || trimmed.starts_with("def ")
                    || trimmed.starts_with("async def ")
                    || trimmed.starts_with("class ")
                    || trimmed.starts_with("function ")
                    || trimmed.starts_with("export function ")
                    || trimmed.starts_with("export const ")
                {
                    entries.push(format!(
                        "  L{:>4}: {}",
                        idx + 1,
                        trimmed.trim_end_matches('{').trim()
                    ));
                    if entries.len() >= 35 {
                        break;
                    }
                }
            }
        }
    }

    if entries.is_empty() {
        return None;
    }

    Some(format!(
        "[AST Outline for unread lines L{}-L{} — jump directly via `read_file(path: \"{}\", start_line: X, end_line: Y)`:\n{}\n]",
        shown_end_line + 1,
        total_lines,
        relative_path,
        entries.join("\n")
    ))
}

/// Maximum number of files processed in a single `batch_read` call.
const MAX_BATCH_READ_FILES: usize = 20;

/// Default maximum lines per file in `batch_read`.
const DEFAULT_BATCH_READ_MAX_LINES: usize = 200;

/// Reads multiple workspace files in parallel in a single call, returning a structured
/// Markdown report with 1-indexed line numbers or per-file error diagnostics.
pub fn batch_read(
    workspace_root: &Path,
    paths: &[String],
    max_lines: Option<usize>,
) -> Result<String> {
    if paths.is_empty() {
        return Err(ToolError::InvalidArguments {
            name: "batch_read".to_string(),
            reason: "paths array cannot be empty".to_string(),
        }
        .into());
    }

    let max_per_file = max_lines
        .unwrap_or(DEFAULT_BATCH_READ_MAX_LINES)
        .clamp(1, 1000);
    let capped_paths = if paths.len() > MAX_BATCH_READ_FILES {
        &paths[..MAX_BATCH_READ_FILES]
    } else {
        paths
    };

    let rel_refs: Vec<&str> = capped_paths.iter().map(|s| s.as_str()).collect();
    crate::context::governance::dox::DoxEngine::sync_and_harvest_rules(workspace_root, &rel_refs);

    let results: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = capped_paths
                .iter()
                .map(|rel_path| {
                    let rel = rel_path.clone();
                    s.spawn(move || {
                        let target_path =
                            match validate_path_in_workspace(workspace_root, Path::new(&rel)) {
                                Ok(p) => p,
                                Err(e) => return (rel, Err(e.to_string())),
                            };
                        if let Some(ext) = target_path.extension().and_then(|e| e.to_str()) {
                            let ext_lower = ext.to_ascii_lowercase();
                            if matches!(ext_lower.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
                                return match read_file(workspace_root, &rel, None, None) {
                                    Ok(info) => (rel, Ok((1, format!("{}\n", info)))),
                                    Err(e) => (rel, Err(e.to_string())),
                                };
                            }
                        }
                        let content = match std::fs::read_to_string(&target_path) {
                            Ok(c) => c,
                            Err(e) => return (rel, Err(e.to_string())),
                        };
                        let lines: Vec<&str> = content.lines().collect();
                        let total_lines = lines.len();
                        let show_count = total_lines.min(max_per_file);
                        let mut snippet = String::new();
                        for (idx, line) in lines.iter().take(show_count).enumerate() {
                            snippet.push_str(&format!("{:>4} | {}\n", idx + 1, line));
                        }
                        if total_lines > show_count {
                            snippet.push_str(&format!(
                                "\n[... Showing 1-{} of {} lines ({} more lines). Use read_file(path: \"{}\", start_line: {}) for more ...]\n",
                                show_count,
                                total_lines,
                                total_lines - show_count,
                                rel,
                                show_count + 1
                            ));
                        }
                        (rel, Ok((total_lines, snippet)))
                    })
                })
                .collect();

        handles
            .into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| {
                    (
                        "<unknown>".to_string(),
                        Err("Worker thread panicked during batch_read".to_string()),
                    )
                })
            })
            .collect()
    });

    let succeeded = results.iter().filter(|(_, r)| r.is_ok()).count();
    let failed = results.len().saturating_sub(succeeded);

    let mut report = format!(
        "# Batch Read ({} files: {} succeeded, {} failed)\n\n",
        results.len(),
        succeeded,
        failed
    );
    if paths.len() > MAX_BATCH_READ_FILES {
        report.push_str(&format!(
            "> Note: Request contained {} paths; capped to the first {} files.\n\n",
            paths.len(),
            MAX_BATCH_READ_FILES
        ));
    }

    for (idx, (rel, outcome)) in results.into_iter().enumerate() {
        if idx > 0 {
            report.push_str("\n---\n\n");
        }
        match outcome {
            Ok((total_lines, body)) => {
                report.push_str(&format!("## `{}` ({} lines)\n", rel, total_lines));
                if body.is_empty() {
                    report.push_str("*(empty file)*\n");
                } else {
                    report.push_str(&body);
                }
            }
            Err(err_msg) => {
                report.push_str(&format!("## `{}` — ERROR\n✗ {}\n", rel, err_msg));
            }
        }
    }

    Ok(report)
}

/// Threshold lines below which `look_at` returns full line-numbered content when `extract == "all"`.
const LOOK_AT_SMALL_FILE_LINES: usize = 150;
/// Threshold bytes below which `look_at` returns full line-numbered content when `extract == "all"`.
const LOOK_AT_SMALL_FILE_BYTES: usize = 8 * 1024;

/// Inspects a file's structure, imports, exports, and AST symbol outline with exact line numbers
/// without consuming full-file context tokens on large files.
/// For small files (<= 150 lines and <= 8 KB), returns the full line-numbered content.
pub fn look_at(
    workspace_root: &Path,
    relative_path: &str,
    extract: Option<&str>,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
    crate::context::governance::dox::DoxEngine::sync_and_harvest_rules(
        workspace_root,
        &[relative_path],
    );
    crate::context::governance::dox::DoxEngine::on_file_mutated(workspace_root, relative_path);

    let ext_lower = target_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    const IMAGE_OR_BINARY: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tiff", "svgz", "pdf", "wasm", "zip",
        "tar", "gz", "tgz", "7z", "rar", "bin", "exe", "so", "dylib", "dll",
    ];
    if IMAGE_OR_BINARY.contains(&ext_lower.as_str()) {
        return read_file(workspace_root, relative_path, None, None);
    }

    let content = std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let byte_len = content.len();
    let size_kb = byte_len as f64 / 1024.0;
    let mode = extract.unwrap_or("all").trim().to_ascii_lowercase();

    // Small file fast-path when full overview requested
    if mode == "all"
        && total_lines <= LOOK_AT_SMALL_FILE_LINES
        && byte_len <= LOOK_AT_SMALL_FILE_BYTES
    {
        let mut out = format!(
            "# `{}` ({} lines, {:.1} KB — complete file)\n\n",
            relative_path, total_lines, size_kb
        );
        for (idx, line) in lines.iter().enumerate() {
            out.push_str(&format!("{:>4} | {}\n", idx + 1, line));
        }
        return Ok(out);
    }

    let mut out = format!(
        "# `{}` ({} lines, {:.1} KB — structural outline)\n\n",
        relative_path, total_lines, size_kb
    );

    let want_imports = mode == "all" || mode == "imports";
    let want_exports = mode == "all" || mode == "exports";
    let want_structure = mode == "all" || mode == "structure";

    if matches!(ext_lower.as_str(), "md" | "mdx" | "markdown") {
        let mut headings = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if line.starts_with('#') {
                headings.push(format!("  L{:>4}: {}", idx + 1, line.trim()));
            }
        }
        out.push_str(&format!("## Document Headings ({})\n", headings.len()));
        if headings.is_empty() {
            out.push_str("  *(no markdown headings found)*\n");
        } else {
            for h in headings.iter().take(80) {
                out.push_str(h);
                out.push('\n');
            }
        }
        out.push_str(&format!(
            "\n*Use `read_file(path: \"{}\", start_line: X, end_line: Y)` to inspect specific sections.*",
            relative_path
        ));
        return Ok(out);
    }

    if matches!(ext_lower.as_str(), "json" | "toml" | "yaml" | "yml") {
        let mut keys = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                continue;
            }
            let indent = line.len().saturating_sub(line.trim_start().len());
            if indent <= 2
                && (trimmed.starts_with('[') || trimmed.contains(':') || trimmed.contains('='))
            {
                keys.push(format!("  L{:>4}: {}", idx + 1, trimmed));
            }
        }
        out.push_str(&format!("## Top-Level Sections / Keys ({})\n", keys.len()));
        for k in keys.iter().take(60) {
            out.push_str(k);
            out.push('\n');
        }
        out.push_str(&format!(
            "\n*Use `read_file(path: \"{}\", start_line: X, end_line: Y)` to inspect specific sections.*",
            relative_path
        ));
        return Ok(out);
    }

    if want_imports {
        let mut imports = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ")
                || trimmed.starts_with("pub use ")
                || trimmed.starts_with("import ")
                || (trimmed.starts_with("from ") && trimmed.contains(" import "))
                || trimmed.starts_with("#include ")
                || (trimmed.contains("require(")
                    && (trimmed.starts_with("const ") || trimmed.starts_with("let ")))
            {
                imports.push(format!("  L{:>4}: {}", idx + 1, trimmed));
            }
        }
        out.push_str(&format!("## Imports & Dependencies ({})\n", imports.len()));
        if imports.is_empty() {
            out.push_str("  *(none detected)*\n");
        } else {
            for imp in imports.iter().take(35) {
                out.push_str(imp);
                out.push('\n');
            }
            if imports.len() > 35 {
                out.push_str(&format!("  ... and {} more imports\n", imports.len() - 35));
            }
        }
        out.push('\n');
    }

    if want_exports {
        let mut exports = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub ")
                || trimmed.starts_with("pub(crate) ")
                || trimmed.starts_with("export ")
                || trimmed.starts_with("__all__")
            {
                let clean = trimmed.trim_end_matches('{').trim_end_matches(';').trim();
                exports.push(format!("  L{:>4}: {}", idx + 1, clean));
            }
        }
        out.push_str(&format!(
            "## Exports & Public Surface ({})\n",
            exports.len()
        ));
        if exports.is_empty() {
            out.push_str("  *(none detected)*\n");
        } else {
            for exp in exports.iter().take(50) {
                out.push_str(exp);
                out.push('\n');
            }
            if exports.len() > 50 {
                out.push_str(&format!("  ... and {} more exports\n", exports.len() - 50));
            }
        }
        out.push('\n');
    }

    if want_structure {
        let extractor = crate::context::ast::repomap::RepoMapExtractor::new();
        let ast_symbols = extractor
            .extract_symbols_from_str(&ext_lower, &content)
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.kind != "import")
            .collect::<Vec<_>>();

        if !ast_symbols.is_empty() {
            out.push_str(&format!(
                "## AST Structure ({} symbols)\n",
                ast_symbols.len()
            ));
            for sym in ast_symbols.iter().take(80) {
                if let Some(ref doc) = sym.doc_comment {
                    let short_doc: String = doc.chars().take(90).collect();
                    out.push_str(&format!(
                        "  L{:>4}-L{:<4} [{}] `{}` — {}\n",
                        sym.line_number, sym.end_line, sym.kind, sym.signature, short_doc
                    ));
                } else {
                    out.push_str(&format!(
                        "  L{:>4}-L{:<4} [{}] `{}`\n",
                        sym.line_number, sym.end_line, sym.kind, sym.signature
                    ));
                }
            }
            if ast_symbols.len() > 80 {
                out.push_str(&format!(
                    "  ... and {} more symbols\n",
                    ast_symbols.len() - 80
                ));
            }
        } else {
            // Fallback structural scan for languages without active Tree-sitter grammar
            let mut defs = Vec::new();
            for (idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("fn ")
                    || trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("async fn ")
                    || trimmed.starts_with("pub async fn ")
                    || trimmed.starts_with("struct ")
                    || trimmed.starts_with("pub struct ")
                    || trimmed.starts_with("enum ")
                    || trimmed.starts_with("trait ")
                    || trimmed.starts_with("impl")
                    || trimmed.starts_with("def ")
                    || trimmed.starts_with("async def ")
                    || trimmed.starts_with("class ")
                    || trimmed.starts_with("function ")
                    || trimmed.starts_with("interface ")
                    || trimmed.starts_with("type ")
                    || trimmed.starts_with("func ")
                {
                    defs.push(format!(
                        "  L{:>4}: {}",
                        idx + 1,
                        trimmed.trim_end_matches('{').trim()
                    ));
                }
            }
            out.push_str(&format!("## Code Structure ({} definitions)\n", defs.len()));
            for d in defs.iter().take(80) {
                out.push_str(d);
                out.push('\n');
            }
        }
    }

    out.push_str(&format!(
        "\n*Use `read_file(path: \"{}\", start_line: X, end_line: Y)` to read specific sections in full.*",
        relative_path
    ));

    Ok(out)
}

/// Parses pixel dimensions `(width, height)` from PNG, GIF, JPEG, or WebP headers without external crates.
fn parse_image_dimensions(bytes: &[u8], ext: &str) -> Option<(u32, u32)> {
    match ext {
        "png" if bytes.len() >= 24 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" => {
            let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
            let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
            Some((w, h))
        }
        "gif" if bytes.len() >= 10 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") => {
            let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
            let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
            Some((w, h))
        }
        "jpg" | "jpeg" if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 => {
            let mut idx = 2;
            while idx + 8 < bytes.len() {
                if bytes[idx] != 0xFF {
                    idx += 1;
                    continue;
                }
                let marker = bytes[idx + 1];
                if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                    let h = u16::from_be_bytes([bytes[idx + 5], bytes[idx + 6]]) as u32;
                    let w = u16::from_be_bytes([bytes[idx + 7], bytes[idx + 8]]) as u32;
                    return Some((w, h));
                }
                if marker == 0xD9 || marker == 0xDA {
                    break;
                }
                let seg_len = u16::from_be_bytes([bytes[idx + 2], bytes[idx + 3]]) as usize;
                if seg_len < 2 {
                    break;
                }
                idx += 2 + seg_len;
            }
            None
        }
        _ => None,
    }
}

/// Atomically writes content to a file via a temporary file, supporting overwrite or append mode.
/// By default, to protect against accidental file clobbering when building large files across turns,
/// modifying an existing non-empty file requires explicit intent: `overwrite: true` or `append: true`.
pub fn write_file_with_options(
    workspace_root: &Path,
    relative_path: &str,
    content: &str,
    append: bool,
    overwrite: bool,
) -> Result<String> {
    write_file_inner(
        workspace_root,
        relative_path,
        content,
        append,
        overwrite,
        true,
    )
}

fn write_file_inner(
    workspace_root: &Path,
    relative_path: &str,
    content: &str,
    append: bool,
    overwrite: bool,
    run_compiler_check: bool,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;

    let orig_content = if target_path.exists() {
        std::fs::read_to_string(&target_path).unwrap_or_default()
    } else {
        String::new()
    };

    // Safe-Overwrite & Chunk Guard:
    // If the file exists and is not empty, and neither append nor overwrite was explicitly granted:
    if target_path.exists() && !orig_content.trim().is_empty() && !append && !overwrite {
        // If content is identical, succeed idempotently without warning
        if orig_content == content {
            return Ok(format!(
                "File '{}' is already up to date with identical content (no changes needed).",
                relative_path
            ));
        }

        // Template Placeholder & Starter Scaffold Exemption:
        // When a stack is scaffolded (e.g. with onpkg/minikit), it generates starter template files
        // intended to be replaced. Overwriting an unmodified boilerplate starter does not destroy user work.
        let clean_rel = relative_path.trim_start_matches("./");
        let is_builtin_scaffold_file =
            crate::tools::minikit::MiniKitScaffolder::is_scaffold_file_content(
                Some(workspace_root),
                clean_rel,
                orig_content.trim(),
            );
        let is_template_placeholder = is_builtin_scaffold_file
            || ((clean_rel.starts_with("minikit_docs/") || clean_rel.starts_with("onpkg_docs/"))
                && (orig_content.lines().count() <= 35
                    || orig_content.contains("> Template")
                    || orig_content.contains("<!-- template -->")
                    || orig_content.contains("TODO:")));

        if !is_template_placeholder {
            let mut hint = String::new();
            let trimmed_content = content.trim();
            let trimmed_lower = trimmed_content.to_ascii_lowercase();

            // Heuristic 1: Check if content appears to be an HTML fragment or section
            let is_html_chunk = (relative_path.ends_with(".html")
                || relative_path.ends_with(".htm"))
                && !trimmed_lower.contains("<!doctype")
                && !trimmed_lower.contains("<html")
                && (trimmed_lower.starts_with("<section")
                    || trimmed_lower.starts_with("<!--")
                    || trimmed_lower.starts_with("<div")
                    || trimmed_lower.starts_with("<main")
                    || trimmed_lower.starts_with("<footer")
                    || trimmed_lower.starts_with("<header"));

            // Heuristic 2: Check if content appears to be a CSS rule chunk rather than full stylesheet
            let is_css_chunk = relative_path.ends_with(".css")
                && !trimmed_lower.contains(":root")
                && !trimmed_lower.contains("@import")
                && !trimmed_lower.contains("@tailwind")
                && !trimmed_lower.contains("@layer")
                && (trimmed_lower.starts_with("@media") || trimmed_lower.starts_with('.'));

            // Heuristic 3: Check if a substantial file (> 40 lines) is being replaced by a tiny snippet (< 25% of original lines)
            // without an active transaction or turn checkpoint backup
            let orig_lines = orig_content.lines().count();
            let new_lines = content.lines().count();
            let already_backed_up =
                crate::session::transaction::TransactionManager::is_file_tracked_in_active_tx(
                    workspace_root,
                    &target_path,
                ) || crate::session::backup::BackupManager::new(workspace_root)
                    .is_file_backed_up_in_latest_turn(&target_path);
            let is_suspicious_truncation =
                !already_backed_up && orig_lines > 40 && new_lines.saturating_mul(4) < orig_lines;

            if is_html_chunk {
                hint.push_str("\n\n💡 Chunk Detected: Your content looks like an HTML fragment/section rather than a full document. If you are building this page in sequential steps, pass `append: true` to append to the existing document!");
            } else if is_css_chunk {
                hint.push_str("\n\n💡 Chunk Detected: Your content looks like additional CSS rules/media queries. If you are adding styles in steps, pass `append: true` to avoid erasing previously written styles!");
            }

            if is_html_chunk || is_css_chunk || is_suspicious_truncation {
                return Err(ToolError::InvalidArguments {
                    name: "write_file".to_string(),
                    reason: format!(
                        "Target file '{}' already exists ({} lines, {} bytes). To prevent accidental file destruction, write_file requires explicit intent:\n\
                        • To append new sections to the end of the file: pass `append: true`.\n\
                        • To completely replace and overwrite the file: pass `overwrite: true`.\n\
                        • To surgically edit specific lines: use `patch_file`.{}",
                        relative_path,
                        orig_lines,
                        orig_content.len(),
                        hint
                    ),
                }
                .into());
            }
        }
    }

    let effective_content = if append && !orig_content.is_empty() {
        let is_html = relative_path.ends_with(".html") || relative_path.ends_with(".htm");
        if is_html {
            let orig_lower = orig_content.to_ascii_lowercase();
            let new_lower = content.to_ascii_lowercase();
            if let Some(body_pos) = orig_lower.rfind("</body>") {
                if new_lower.contains("</html>") || new_lower.contains("</body>") {
                    format!("{}{}", &orig_content[..body_pos], content)
                } else {
                    format!(
                        "{}{}\n{}",
                        &orig_content[..body_pos],
                        content,
                        &orig_content[body_pos..]
                    )
                }
            } else {
                format!("{}{}", orig_content, content)
            }
        } else {
            format!("{}{}", orig_content, content)
        }
    } else {
        content.to_string()
    };

    // Pre-Write AST Syntax Barrier: verify that tentative content does not introduce syntax errors
    crate::context::syntax_guard::SyntaxGuard::check_syntax_barrier(
        &target_path,
        &orig_content,
        &effective_content,
    )
    .map_err(|reason| ToolError::PatchFailed {
        path: relative_path.to_string(),
        reason,
    })?;

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        })?;
    }

    let parent_dir = target_path.parent().unwrap_or(workspace_root);
    let tmp_file_name = format!(
        ".tmp_{}_{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    );
    let tmp_path = parent_dir.join(tmp_file_name);

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp_path)
        .map_err(|e| ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        })?;

    file.write_all(effective_content.as_bytes())
        .map_err(|e| ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        })?;

    file.flush().map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    drop(file);

    std::fs::rename(&tmp_path, &target_path).map_err(|e| {
        if let Err(cleanup_err) = std::fs::remove_file(&tmp_path) {
            tracing::warn!(
                path = %tmp_path.display(),
                error = %cleanup_err,
                "Failed to clean up temporary file after rename failure"
            );
        }
        ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        }
    })?;

    crate::context::index::SymbolIndex::invalidate_cache();
    crate::context::graph::CodeGraph::invalidate_cache();
    crate::context::governance::dox::DoxEngine::sync_and_harvest_rules(
        workspace_root,
        &[relative_path],
    );
    crate::context::governance::dox::DoxEngine::on_file_mutated(workspace_root, relative_path);

    let total_lines = effective_content.lines().count();
    let mut msg = if append {
        format!(
            "Successfully appended {} bytes (total {} lines) to {}",
            content.len(),
            total_lines,
            relative_path
        )
    } else {
        format!(
            "Successfully wrote {} lines to {}",
            total_lines, relative_path
        )
    };

    if run_compiler_check {
        if let Some(feedback) =
            crate::tools::compiler::ScopedCompiler::run_scoped_check(workspace_root, relative_path)
        {
            msg.push_str("\n\n");
            msg.push_str(&feedback);
        }
    }

    Ok(msg)
}

/// Atomically writes full content to a file via a temporary file, creating any missing parent directories.
pub fn write_file(workspace_root: &Path, relative_path: &str, content: &str) -> Result<String> {
    write_file_with_options(workspace_root, relative_path, content, false, true)
}

/// Diagnostic structure for nearest match during failed search-and-replace patching.
#[derive(Debug, Clone)]
pub struct NearestMatchDiagnostic {
    pub start_line: usize,
    pub end_line: usize,
    pub similarity: f64,
    pub actual_snippet: String,
}

/// Detects leading indentation of a block of lines (spaces or tabs).
fn detect_leading_indent(lines: &[&str]) -> String {
    for line in lines {
        if !line.trim().is_empty() {
            return line.chars().take_while(|c| c.is_whitespace()).collect();
        }
    }
    String::new()
}

/// Re-indents replacement lines to preserve target file's indentation level.
fn align_indentation(replace: &str, orig_indent: &str, search_indent: &str) -> String {
    if orig_indent == search_indent {
        return replace.to_string();
    }

    let replace_lines: Vec<&str> = replace.lines().collect();
    let mut aligned = Vec::new();

    for line in replace_lines {
        if line.trim().is_empty() {
            aligned.push(String::new());
            continue;
        }
        if !search_indent.is_empty() && line.starts_with(search_indent) {
            let suffix = &line[search_indent.len()..];
            aligned.push(format!("{}{}", orig_indent, suffix));
        } else if search_indent.is_empty() {
            aligned.push(format!("{}{}", orig_indent, line));
        } else {
            let trimmed = line.trim_start();
            aligned.push(format!("{}{}", orig_indent, trimmed));
        }
    }

    aligned.join("\n")
}

/// Locates 1-indexed line numbers of all substring matches in content.
fn find_all_match_lines(content: &str, needle: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut offset = 0;
    while let Some(pos) = content[offset..].find(needle) {
        let abs_pos = offset + pos;
        let line_num = content[..abs_pos].lines().count().max(1);
        lines.push(line_num);
        offset = abs_pos + needle.len();
    }
    lines
}

/// Formats patch success message, preserving optional compiler or linter diagnostic feedback.
fn format_patch_success(tier_desc: &str, relative_path: &str, write_res: &str) -> String {
    let mut msg = format!("Successfully patched {}", relative_path);
    if !tier_desc.is_empty() {
        msg.push_str(&format!(" ({})", tier_desc));
    }
    if let Some(feedback_start) = write_res.find("\n\n[Compiler") {
        msg.push_str(&write_res[feedback_start..]);
    } else if let Some(feedback_start) = write_res.find("\n\n[Linter") {
        msg.push_str(&write_res[feedback_start..]);
    }
    msg
}

/// Applies a search-and-replace block patch to a file with a 5-tier resilient matching pipeline.
///
/// 5-Tier Resilient Hierarchy:
/// 1. Exact string match (0ms)
/// 2. CRLF & trailing whitespace normalized match
/// 3. Whitespace/indentation-insensitive match with automatic re-indentation
/// 4. Blank-line relaxed match with automatic re-indentation
/// 5. High-threshold sliding-window fuzzy diff matching with uniqueness gap
///
/// If all 5 tiers fail to match, generates an actionable 4-part "What-Where-Why-Next" diagnostic.
pub fn patch_file(
    workspace_root: &Path,
    relative_path: &str,
    search_block: &str,
    replace_block: &str,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;

    let original = std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    if search_block == replace_block {
        if let Some(byte_idx) = original.find(search_block) {
            let start_line = original[..byte_idx].lines().count().max(1);
            let block_line_count = search_block.lines().count().max(1);
            let end_line = start_line + block_line_count - 1;
            let all_lines: Vec<&str> = original.lines().collect();
            let ctx_start = start_line.saturating_sub(4).max(1);
            let ctx_end = (end_line + 8).min(all_lines.len().max(1));
            let mut snippet = String::new();
            for line_no in ctx_start..=ctx_end {
                if let Some(line_text) = all_lines.get(line_no - 1) {
                    snippet.push_str(&format!("{:>4} | {}\n", line_no, line_text));
                }
            }
            return Ok(format!(
                "No changes needed in {}: search_block and replace_block are identical (block is already in the file).\n\n[Surrounding Context in '{}' (lines {}-{})]:\n{}💡 Do NOT repeat this identical `patch_file` call. If you intended to update this section, pass the existing lines above as `search_block` and your modified code as `replace_block`.",
                relative_path,
                relative_path,
                ctx_start,
                ctx_end,
                snippet
            ));
        }
        let diag = find_nearest_match_diagnostic(&original, search_block);
        let mut msg = format!(
            "No changes needed in {}: search_block and replace_block are identical, and this block is not currently present in the file.",
            relative_path
        );
        if !diag.actual_snippet.is_empty() {
            msg.push_str(&format!(
                "\n\n[Nearest Match in '{}' at lines {}-{} ({:.1}% similarity)]:\n------------------------------------------------------------\n{}\n------------------------------------------------------------\nPass the existing lines above as `search_block` and your updated code as `replace_block`.",
                relative_path,
                diag.start_line,
                diag.end_line,
                diag.similarity * 100.0,
                diag.actual_snippet
            ));
        }
        return Ok(msg);
    }

    // 1. Tier 1: Exact Substring Match
    if original.contains(search_block) {
        let occurrences = original.matches(search_block).count();
        if occurrences > 1 {
            let match_lines = find_all_match_lines(&original, search_block);
            return Err(ToolError::PatchFailed {
                path: relative_path.to_string(),
                reason: format!(
                    "Search block matches multiple ({} times) locations at lines: {:?}. Provide 2-3 lines of unique surrounding context.",
                    occurrences, match_lines
                ),
            }
            .into());
        }

        let mut new_content = original.replacen(search_block, replace_block, 1);
        if original.ends_with('\n') && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success("", relative_path, &write_res));
    }

    // 2. Tier 2: CRLF & Trailing Whitespace Normalization
    if let Some(new_content) =
        try_crlf_and_trailing_whitespace_replace(&original, search_block, replace_block)
    {
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success(
            "whitespace-normalized match",
            relative_path,
            &write_res,
        ));
    }

    // 3. Tier 3: Whitespace/Indentation-Insensitive Match with Auto-Re-Indentation
    if let Some(new_content) =
        try_whitespace_normalized_replace(&original, search_block, replace_block)
    {
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success(
            "auto-reindented match",
            relative_path,
            &write_res,
        ));
    }

    // 4. Tier 4: Blank-Line Relaxed Match with Auto-Re-Indentation
    if let Some(new_content) =
        try_blank_line_relaxed_replace(&original, search_block, replace_block)
    {
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success(
            "blank-line relaxed match",
            relative_path,
            &write_res,
        ));
    }

    // 5. Tier 5: High-Threshold Fuzzy Diff with Strict Uniqueness Gap & Auto-Re-Indentation
    // Single-pass evaluation returns either the patched file or the nearest diagnostic window.
    let diag = match try_fuzzy_replace_with_diagnostic(&original, search_block, replace_block) {
        Ok(new_content) => {
            let write_res = write_file(workspace_root, relative_path, &new_content)?;
            return Ok(format_patch_success(
                "fuzzy matched with auto-reindentation",
                relative_path,
                &write_res,
            ));
        }
        Err(diag) => diag,
    };

    // Failure: Generate Actionable 4-Part "What-Where-Why-Next" Diagnostic
    let inspect_start = diag.start_line.saturating_sub(5).max(1);
    let inspect_end = diag.end_line + 5;

    let reason = format!(
        "Search block could not be found in '{relative_path}'.\n\n\
         [Where] Nearest match found at lines {}-{} ({:.1}% similarity):\n\
         ------------------------------------------------------------\n\
         {}\n\
         ------------------------------------------------------------\n\n\
         [Expected Search Block]\n\
         {}\n\n\
         [Suggested Next Action]\n\
         Your search block diverged from disk contents around lines {}-{}.\n\
         Update your search_block to match the actual file lines above, or call `read_file(path: \"{}\", start_line: {}, end_line: {})` to inspect current file contents.",
        diag.start_line,
        diag.end_line,
        diag.similarity * 100.0,
        diag.actual_snippet,
        search_block.trim(),
        diag.start_line,
        diag.end_line,
        relative_path,
        inspect_start,
        inspect_end
    );

    Err(ToolError::PatchFailed {
        path: relative_path.to_string(),
        reason,
    }
    .into())
}

fn splice_lines_with_replace(
    original: &str,
    orig_lines: &[&str],
    start: usize,
    end: usize,
    replace_str: &str,
) -> String {
    let mut result_lines = Vec::with_capacity(orig_lines.len() - (end - start) + 1);
    result_lines.extend_from_slice(&orig_lines[..start]);
    result_lines.push(replace_str);
    result_lines.extend_from_slice(&orig_lines[end..]);
    let mut res = result_lines.join("\n");
    if original.ends_with('\n') && !res.ends_with('\n') {
        res.push('\n');
    }
    res
}

fn splice_aligned_replacement(
    original: &str,
    orig_lines: &[&str],
    start: usize,
    end: usize,
    search_lines: &[&str],
    replace: &str,
) -> String {
    let orig_indent = detect_leading_indent(&orig_lines[start..end]);
    let search_indent = detect_leading_indent(search_lines);
    let aligned_replace = align_indentation(replace, &orig_indent, &search_indent);
    splice_lines_with_replace(original, orig_lines, start, end, &aligned_replace)
}

/// Tier 2: Normalizes line endings (CRLF -> LF) and trims trailing whitespace per line.
fn try_crlf_and_trailing_whitespace_replace(
    original: &str,
    search: &str,
    replace: &str,
) -> Option<String> {
    let orig_clean = original.replace("\r\n", "\n");
    let search_clean = search.replace("\r\n", "\n");

    let orig_trimmed_trailing: String = orig_clean
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let search_trimmed_trailing: String = search_clean
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    if orig_trimmed_trailing.contains(&search_trimmed_trailing) {
        let count = orig_trimmed_trailing
            .matches(&search_trimmed_trailing)
            .count();
        if count == 1 {
            let before_match = orig_trimmed_trailing
                .split(&search_trimmed_trailing)
                .next()?;
            let start_line = before_match.lines().count().saturating_sub(1);
            let search_line_count = search_trimmed_trailing.lines().count();

            let orig_lines: Vec<&str> = original.lines().collect();
            if start_line + search_line_count <= orig_lines.len() {
                return Some(splice_lines_with_replace(
                    original,
                    &orig_lines,
                    start_line,
                    start_line + search_line_count,
                    replace,
                ));
            }
        }
    }
    None
}

/// Tier 3: Whitespace/indentation-insensitive match with automatic base re-indentation.
fn try_whitespace_normalized_replace(
    original: &str,
    search: &str,
    replace: &str,
) -> Option<String> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let search_lines: Vec<&str> = search.lines().collect();

    if search_lines.is_empty() || search_lines.len() > orig_lines.len() {
        return None;
    }

    let search_trimmed: Vec<&str> = search_lines.iter().map(|l| l.trim()).collect();
    let mut matches = Vec::new();

    for i in 0..=(orig_lines.len() - search_lines.len()) {
        if orig_lines[i..i + search_lines.len()]
            .iter()
            .map(|l| l.trim())
            .eq(search_trimmed.iter().copied())
        {
            matches.push(i);
        }
    }

    // Only apply if uniquely matched to prevent ambiguous edits
    if matches.len() == 1 {
        let i = matches[0];
        return Some(splice_aligned_replacement(
            original,
            &orig_lines,
            i,
            i + search_lines.len(),
            &search_lines,
            replace,
        ));
    }

    None
}

/// Tier 4: Blank-line relaxed match ignoring empty line differences.
fn try_blank_line_relaxed_replace(original: &str, search: &str, replace: &str) -> Option<String> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let search_lines: Vec<&str> = search.lines().collect();

    let search_non_blank: Vec<&str> = search_lines
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty())
        .collect();
    if search_non_blank.is_empty() || orig_lines.len() < search_non_blank.len() {
        return None;
    }

    let search_trimmed: Vec<&str> = search_non_blank.iter().map(|l| l.trim()).collect();
    let mut matches = Vec::new();
    let window_size = search_lines.len();

    for i in 0..orig_lines.len() {
        let max_end = (i + window_size + 3).min(orig_lines.len());
        for end in (i + search_non_blank.len())..=max_end {
            let non_blank_iter = orig_lines[i..end]
                .iter()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty());
            if non_blank_iter.eq(search_trimmed.iter().copied()) {
                matches.push((i, end));
                break;
            }
        }
    }

    if matches.len() == 1 {
        let (start, end) = matches[0];
        return Some(splice_aligned_replacement(
            original,
            &orig_lines,
            start,
            end,
            &search_lines,
            replace,
        ));
    }

    None
}

/// Computes the closest line window in `original` to `search` without mutating.
fn find_nearest_match_diagnostic(original: &str, search: &str) -> NearestMatchDiagnostic {
    match try_fuzzy_replace_with_diagnostic(original, search, search) {
        Ok(_) => {
            let orig_lines: Vec<&str> = original.lines().collect();
            let search_lines: Vec<&str> = search.lines().collect();
            let window_size = search_lines.len().max(1);
            let search_str = search_lines.join("\n");
            let mut best_ratio = 0.0;
            let mut best_idx = 0;
            let max_start = orig_lines.len().saturating_sub(window_size);
            for i in 0..=max_start {
                let actual_window_size = window_size.min(orig_lines.len() - i);
                let window_str = orig_lines[i..i + actual_window_size].join("\n");
                let ratio = TextDiff::from_chars(&window_str, &search_str).ratio() as f64;
                if ratio > best_ratio {
                    best_ratio = ratio;
                    best_idx = i;
                }
            }
            build_nearest_match_diagnostic(&orig_lines, best_idx, window_size, best_ratio)
        }
        Err(diag) => diag,
    }
}

fn build_nearest_match_diagnostic(
    orig_lines: &[&str],
    best_idx: usize,
    window_size: usize,
    best_ratio: f64,
) -> NearestMatchDiagnostic {
    let actual_len = window_size.min(orig_lines.len() - best_idx);
    let end_idx = best_idx + actual_len;
    let snippet = orig_lines[best_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(offset, line)| format!("{:>4} | {}", best_idx + offset + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    NearestMatchDiagnostic {
        start_line: best_idx + 1,
        end_line: end_idx,
        similarity: best_ratio,
        actual_snippet: snippet,
    }
}

/// Single-pass Tier 5 fuzzy matcher that returns either the uniquely patched content
/// or the closest diagnostic match window without scanning the file twice.
fn try_fuzzy_replace_with_diagnostic(
    original: &str,
    search: &str,
    replace: &str,
) -> std::result::Result<String, NearestMatchDiagnostic> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let search_lines: Vec<&str> = search.lines().collect();

    if orig_lines.is_empty() {
        return Err(NearestMatchDiagnostic {
            start_line: 1,
            end_line: 1,
            similarity: 0.0,
            actual_snippet: String::new(),
        });
    }

    let window_size = search_lines.len().max(1);
    let search_str = search_lines.join("\n");
    let mut best_ratio = 0.0;
    let mut second_best_ratio = 0.0;
    let mut best_idx = 0;
    let mut match_count = 0;

    let max_start = orig_lines.len().saturating_sub(window_size);

    for i in 0..=max_start {
        let actual_window_size = window_size.min(orig_lines.len() - i);
        let window_str = orig_lines[i..i + actual_window_size].join("\n");
        let diff = TextDiff::from_chars(&window_str, &search_str);
        let ratio = diff.ratio() as f64;

        if ratio > best_ratio {
            second_best_ratio = best_ratio;
            best_ratio = ratio;
            best_idx = i;
            match_count = 1;
        } else if (ratio - best_ratio).abs() < 0.001 {
            match_count += 1;
        } else if ratio > second_best_ratio {
            second_best_ratio = ratio;
        }
    }

    if !search_lines.is_empty()
        && search_lines.len() <= orig_lines.len()
        && best_ratio >= crate::constants::FUZZY_MATCH_THRESHOLD
        && match_count == 1
        && (best_ratio - second_best_ratio) >= crate::constants::FUZZY_UNIQUENESS_GAP
    {
        return Ok(splice_aligned_replacement(
            original,
            &orig_lines,
            best_idx,
            best_idx + window_size,
            &search_lines,
            replace,
        ));
    }

    Err(build_nearest_match_diagnostic(
        &orig_lines,
        best_idx,
        window_size,
        best_ratio,
    ))
}

/// Batch refactoring: Atomically find and replace a pattern across multiple workspace files matching a glob pattern.
pub fn replace_in_files(
    workspace_root: &Path,
    glob_pattern: &str,
    search_block: &str,
    replace_block: &str,
    backup_manager: Option<&BackupManager>,
    turn_id: usize,
) -> Result<String> {
    if search_block.is_empty() {
        return Err(ToolError::InvalidArguments {
            name: "replace_in_files".to_string(),
            reason: "search_block cannot be empty".to_string(),
        }
        .into());
    }

    let norm_pattern = glob_pattern.trim();
    let mut override_builder = ignore::overrides::OverrideBuilder::new(workspace_root);
    if let Err(e) = override_builder.add(norm_pattern) {
        return Err(ToolError::InvalidArguments {
            name: "replace_in_files".to_string(),
            reason: format!("Invalid glob pattern '{}': {}", norm_pattern, e),
        }
        .into());
    }

    let overrides = override_builder
        .build()
        .map_err(|e| ToolError::InvalidArguments {
            name: "replace_in_files".to_string(),
            reason: format!(
                "Failed to build glob overrides for '{}': {}",
                norm_pattern, e
            ),
        })?;

    let walker = ignore::WalkBuilder::new(workspace_root)
        .overrides(overrides)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    let mut matched_files: Vec<(PathBuf, String, String)> = Vec::new();
    let mut skipped_files: Vec<String> = Vec::new();
    let mut files_scanned = 0;

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        files_scanned += 1;
        let file_path = entry.path().to_path_buf();
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue, // Skip binary or unreadable files
        };

        let rel_path = file_path.strip_prefix(workspace_root).unwrap_or(&file_path);
        if content.contains(search_block) {
            let new_content = content.replace(search_block, replace_block);

            // Pre-Write AST Syntax Barrier: verify that tentative content does not introduce syntax errors
            crate::context::syntax_guard::SyntaxGuard::check_syntax_barrier(
                &file_path,
                &content,
                &new_content,
            )
            .map_err(|reason| ToolError::PatchFailed {
                path: rel_path.display().to_string(),
                reason: format!("Batch replacement rejected: {}", reason),
            })?;

            matched_files.push((file_path, content, new_content));
        } else {
            skipped_files.push(format!("  • {}", rel_path.display()));
        }
    }

    if matched_files.is_empty() {
        let skipped_detail = if !skipped_files.is_empty() && skipped_files.len() <= 10 {
            format!(
                "\nScanned files without match:\n{}",
                skipped_files.join("\n")
            )
        } else {
            String::new()
        };
        return Ok(format!(
            "No files modified. Scanned {} files matching glob '{}', but found 0 occurrences of search_block.{}",
            files_scanned, norm_pattern, skipped_detail
        ));
    }

    // Apply all edits atomically with safety backups
    let mut modified_summaries = Vec::new();
    let first_modified_rel = matched_files[0]
        .0
        .strip_prefix(workspace_root)
        .unwrap_or(&matched_files[0].0)
        .display()
        .to_string();

    for (file_path, _orig_content, new_content) in &matched_files {
        let rel_path = file_path.strip_prefix(workspace_root).unwrap_or(file_path);
        let rel_str = rel_path.display().to_string();

        if let Some(bm) = backup_manager {
            if let Err(e) = bm.create_checkpoint(workspace_root, file_path, turn_id) {
                tracing::warn!(path = %file_path.display(), error = %e, "Failed to create checkpoint before replace_in_files");
            }
        }

        let _ = crate::session::transaction::TransactionManager::record_mutation_pre(
            workspace_root,
            file_path,
        );

        write_file_inner(workspace_root, &rel_str, new_content, false, true, false)?;

        let _ = crate::session::transaction::TransactionManager::record_mutation_post(
            workspace_root,
            file_path,
        );

        modified_summaries.push(format!("  • {}", rel_str));
    }

    let mut result_msg = format!(
        "Successfully replaced pattern across {} files ({} files scanned matching '{}'):\n{}",
        matched_files.len(),
        files_scanned,
        norm_pattern,
        modified_summaries.join("\n")
    );
    if !skipped_files.is_empty() && files_scanned <= 10 {
        result_msg.push_str(&format!(
            "\nSkipped ({} matched glob but did not contain search_block):\n{}",
            skipped_files.len(),
            skipped_files.join("\n")
        ));
    }

    if let Some(feedback) = crate::tools::compiler::ScopedCompiler::run_scoped_check(
        workspace_root,
        &first_modified_rel,
    ) {
        result_msg.push_str("\n\n");
        result_msg.push_str(&feedback);
    }

    Ok(result_msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_write_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_fs_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "sub/hello.txt";
        let content = "line 1\nline 2\nline 3";
        write_file(&temp_dir, rel_path, content).unwrap();

        let read_back = read_file(&temp_dir, rel_path, Some(2), Some(3)).unwrap();
        assert!(read_back.contains("   2 | line 2"));
        assert!(read_back.contains("   3 | line 3"));
        assert!(!read_back.contains("line 1"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_exact() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_patch_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "code.rs";
        let content = "fn hello() {\n    println!(\"old\");\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        patch_file(
            &temp_dir,
            rel_path,
            "println!(\"old\");",
            "println!(\"new\");",
        )
        .unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("println!(\"new\");"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_read_empty_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_empty_fs_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "empty.txt";
        write_file(&temp_dir, rel_path, "").unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert_eq!(read, "");

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_fuzzy() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_fuzzy_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "fuzzy.rs";
        let content =
            "fn add(a: i32, b: i32) -> i32 {\n    let result = a + b;\n    return result;\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        patch_file(
            &temp_dir,
            rel_path,
            "let result = a + b;\n    return result;",
            "a + b",
        )
        .unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("a + b"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_ambiguous_matches_rejected() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_ambig_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "dup.rs";
        let content = "fn one() {\n    let x = 1;\n}\nfn two() {\n    let x = 1;\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        let res = patch_file(&temp_dir, rel_path, "let x = 1;", "let x = 2;");
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("matches multiple (2 times) locations at lines: [2, 5]"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_indentation_auto_alignment() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_indent_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "nested.rs";
        let content =
            "fn outer() {\n    if true {\n        let a = 10;\n        let b = 20;\n    }\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        // Search has 8 spaces, replacement has 0 spaces base indentation
        let search = "let a = 10;\nlet b = 20;";
        let replace = "let a = 100;\nlet b = 200;\nlet c = 300;";
        patch_file(&temp_dir, rel_path, search, replace).unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("        let a = 100;"));
        assert!(read.contains("        let b = 200;"));
        assert!(read.contains("        let c = 300;"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_crlf_normalization() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_crlf_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "crlf.txt";
        let content = "first line\r\nsecond line\r\nthird line\r\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        // Patch using LF search and replace
        patch_file(&temp_dir, rel_path, "second line", "modified line").unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("modified line"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_blank_line_relaxation() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_blank_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "blank.rs";
        // Original has blank lines
        let content = "fn calculate() {\n    let x = 1;\n\n    let y = 2;\n    x + y\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        // Search has no blank line between x and y
        let search = "    let x = 1;\n    let y = 2;\n    x + y";
        let replace = "    let sum = 1 + 2;\n    sum";
        patch_file(&temp_dir, rel_path, search, replace).unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("let sum = 1 + 2;"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_nearest_match_diagnostic() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_diag_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "diag.rs";
        let content = "fn compute_total(items: &[Item]) -> u32 {\n    let mut total = 0;\n    for item in items {\n        total += item.price;\n    }\n    total\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        // Search block is substantially divergent (below 85% similarity threshold)
        let search =
            "fn process_transactions(records: &[Record]) -> bool {\n    validate(records)\n}";
        let replace = "fn process_transactions(records: &[Record]) -> bool {\n    true\n}";

        let res = patch_file(&temp_dir, rel_path, search, replace);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(msg.contains("[Where] Nearest match found at lines"));
        assert!(msg.contains("[Expected Search Block]"));
        assert!(msg.contains("[Suggested Next Action]"));
        assert!(msg.contains("read_file(path: \"diag.rs\""));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_write_file_append_mode() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_append_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "document.txt";
        let part1 = "Section 1\nHeader info\n";
        write_file(&temp_dir, rel_path, part1).unwrap();

        let part2 = "Section 2\nAdditional details\n";
        let res = write_file_with_options(&temp_dir, rel_path, part2, true, false);
        assert!(res.is_ok());

        let raw_content = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert_eq!(raw_content, format!("{}{}", part1, part2));

        let formatted = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(formatted.contains("Section 1"));
        assert!(formatted.contains("Additional details"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_write_file_safe_overwrite_guard() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_guard_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "index.html";
        let chunk1 = "<!DOCTYPE html><html><head><title>Test</title></head><body>\n";
        // First write creates file cleanly
        let res1 = write_file_with_options(&temp_dir, rel_path, chunk1, false, false);
        assert!(res1.is_ok());

        let chunk2 = "<section>Hero</section>\n";
        // Second write without append or overwrite is rejected by the safe-overwrite guard
        let res2 = write_file_with_options(&temp_dir, rel_path, chunk2, false, false);
        assert!(res2.is_err());
        let err_str = res2.unwrap_err().to_string();
        assert!(err_str.contains("already exists"));
        assert!(err_str.contains("append: true"));

        // Write with append: true succeeds
        let res3 = write_file_with_options(&temp_dir, rel_path, chunk2, true, false);
        assert!(res3.is_ok());
        let content = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert!(content.contains(chunk1));
        assert!(content.contains(chunk2));

        // Write with overwrite: true replaces
        let chunk3 = "<!DOCTYPE html><html><body>New Only</body></html>";
        let res4 = write_file_with_options(&temp_dir, rel_path, chunk3, false, true);
        assert!(res4.is_ok());
        let content2 = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert_eq!(content2, chunk3);

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_replace_in_files_batch() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_replace_in_files_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();

        let file1 = "src/components/A.tsx";
        let file2 = "src/components/B.tsx";
        let file3 = "src/components/C.tsx";

        write_file(&temp_dir, file1, "import { gsap } from 'gsap';\nimport { ScrollTrigger } from 'gsap/ScrollTrigger';\nexport const A = 1;\n").unwrap();
        write_file(&temp_dir, file2, "import { gsap } from 'gsap';\nimport { ScrollTrigger } from 'gsap/ScrollTrigger';\nexport const B = 2;\n").unwrap();
        write_file(
            &temp_dir,
            file3,
            "import { gsap } from 'gsap';\nexport const C = 3;\n",
        )
        .unwrap();

        let search = "import { ScrollTrigger } from 'gsap/ScrollTrigger';\n";
        let replace = "";

        let res =
            replace_in_files(&temp_dir, "src/components/*.tsx", search, replace, None, 0).unwrap();
        assert!(res.contains("Successfully replaced pattern across 2 files"));
        assert!(res.contains("A.tsx"));
        assert!(res.contains("B.tsx"));

        let content1 = std::fs::read_to_string(temp_dir.join(file1)).unwrap();
        assert!(!content1.contains("ScrollTrigger"));
        assert!(content1.contains("export const A = 1;"));

        let content2 = std::fs::read_to_string(temp_dir.join(file2)).unwrap();
        assert!(!content2.contains("ScrollTrigger"));
        assert!(content2.contains("export const B = 2;"));

        let content3 = std::fs::read_to_string(temp_dir.join(file3)).unwrap();
        assert!(content3.contains("export const C = 3;"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_batch_read_parallel_and_missing_resilience() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_batch_read_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        write_file(&temp_dir, "src/one.rs", "pub fn one() -> i32 { 1 }\n").unwrap();
        write_file(&temp_dir, "src/two.rs", "pub fn two() -> i32 { 2 }\n").unwrap();

        let paths = vec![
            "src/one.rs".to_string(),
            "src/missing.rs".to_string(),
            "src/two.rs".to_string(),
        ];
        let report = batch_read(&temp_dir, &paths, Some(50)).unwrap();
        assert!(report.contains("# Batch Read (3 files: 2 succeeded, 1 failed)"));
        assert!(report.contains("## `src/one.rs` (1 lines)"));
        assert!(report.contains("pub fn one() -> i32 { 1 }"));
        assert!(report.contains("## `src/missing.rs` — ERROR"));
        assert!(report.contains("## `src/two.rs` (1 lines)"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_look_at_small_vs_large_structural_outline() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_look_at_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        write_file(
            &temp_dir,
            "src/small.rs",
            "pub fn greet() -> &'static str {\n    \"hi\"\n}\n",
        )
        .unwrap();
        let small_out = look_at(&temp_dir, "src/small.rs", None).unwrap();
        assert!(small_out.contains("complete file"));
        assert!(small_out.contains("pub fn greet()"));

        let mut large_rs =
            String::from("use std::collections::HashMap;\npub use std::path::PathBuf;\n\n");
        for i in 0..45 {
            large_rs.push_str(&format!(
                "/// Computes value {}\npub fn compute_{}(x: usize) -> usize {{\n    let a = x + {};\n    a * 2\n}}\n\n",
                i, i, i
            ));
        }
        write_file(&temp_dir, "src/large.rs", &large_rs).unwrap();
        let large_out = look_at(&temp_dir, "src/large.rs", Some("all")).unwrap();
        assert!(large_out.contains("structural outline"));
        assert!(large_out.contains("## Imports & Dependencies"));
        assert!(large_out.contains("## Exports & Public Surface"));
        assert!(large_out.contains("## AST Structure"));
        assert!(large_out.contains("pub fn compute_0(x: usize) -> usize"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_schema_preserves_parameter_order() {
        let schemas = crate::tools::registry::fs_tools::get_schemas();
        let patch_schema = schemas
            .iter()
            .find(|s| s.name == "patch_file")
            .expect("patch_file schema");
        let props: Vec<&String> = patch_schema.parameters["properties"]
            .as_object()
            .expect("properties object")
            .keys()
            .collect();
        assert_eq!(props, vec!["path", "search_block", "replace_block"]);

        let write_schema = schemas
            .iter()
            .find(|s| s.name == "write_file")
            .expect("write_file schema");
        let write_props: Vec<&String> = write_schema.parameters["properties"]
            .as_object()
            .expect("properties object")
            .keys()
            .collect();
        assert_eq!(write_props[0], "path");
        assert_eq!(write_props[1], "content");
    }

    #[test]
    fn test_patch_file_identical_blocks_shows_nearest_match_when_absent() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_patch_noop_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        write_file(
            &temp_dir,
            "src/Section.tsx",
            "export function Section() {\n  return <div className=\"max-w-2xl\">Hello</div>;\n}\n",
        )
        .unwrap();

        let absent_block = "  return <div className=\"max-w-4xl\">Hello</div>;";
        let out = patch_file(&temp_dir, "src/Section.tsx", absent_block, absent_block).unwrap();
        assert!(out.starts_with("No changes needed in src/Section.tsx:"));
        assert!(out.contains("not currently present in the file"));
        assert!(out.contains("max-w-2xl"));

        let present_block = "  return <div className=\"max-w-2xl\">Hello</div>;";
        let out2 = patch_file(&temp_dir, "src/Section.tsx", present_block, present_block).unwrap();
        assert!(out2.starts_with("No changes needed in src/Section.tsx:"));
        assert!(out2.contains("already in the file"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_replace_in_files_reports_skipped_glob_files() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_replace_skip_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        write_file(
            &temp_dir,
            "src/Performance.tsx",
            "export const P = () => <div className=\"grid-cols-2\">A</div>;\n",
        )
        .unwrap();
        write_file(
            &temp_dir,
            "src/Philosophy.tsx",
            "export const Q = () => <div className=\"grid-cols-4\">B</div>;\n",
        )
        .unwrap();

        let out = replace_in_files(
            &temp_dir,
            "src/{Performance,Philosophy}.tsx",
            "grid-cols-2",
            "sm:grid-cols-2",
            None,
            0,
        )
        .unwrap();
        assert!(out.contains("src/Performance.tsx"));
        assert!(out.contains("Skipped (1 matched glob but did not contain search_block):"));
        assert!(out.contains("src/Philosophy.tsx"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
