use crate::error::{Result, ToolError};
use crate::sandbox::path::{validate_path_in_workspace, ConfinedRoot};
use crate::session::backup::BackupManager;
use similar::TextDiff;
use std::path::{Path, PathBuf};

/// Reads a file from the workspace within the specified optional 1-indexed line range.
pub fn read_file(
    workspace_root: &Path,
    relative_path: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<String> {
    read_file_with_options(workspace_root, relative_path, start_line, end_line, false)
}

/// Reads a file from the workspace within the specified optional 1-indexed line range,
/// optionally formatting lines with content-addressed `Hashline` anchors (`11#VK| code`).
pub fn read_file_with_options(
    workspace_root: &Path,
    relative_path: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
    hashline: bool,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
    let confined = ConfinedRoot::open(workspace_root)?;
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
            let bytes = confined.read_bytes(Path::new(relative_path))?;
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

    let content = confined.read_to_string(Path::new(relative_path))?;

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
            if hashline {
                output.push_str(&crate::tools::hashline::format_hashline(
                    line_idx,
                    lines[line_idx - 1],
                ));
            } else {
                output.push_str(&format!("{:>4} | {}\n", line_idx, lines[line_idx - 1]));
            }
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
                        let content = match ConfinedRoot::open(workspace_root)
                            .and_then(|c| c.read_to_string(Path::new(&rel)))
                        {
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

// ============================================================================
// Algorithmic AST-Segmented Global Okapi BM25 Bulk-Reader (`read_file`/`batch_read` `query` mode)
// ============================================================================

/// A semantic code or documentation block anchored to exact 1-indexed source lines.
#[derive(Debug, Clone)]
struct StructuralBlock {
    file_path: String,
    start_line: usize,
    end_line: usize,
    heading: String,
    text: String,
}

/// Tokenizes code or natural-language queries via the shared camelCase/snake_case stemming tokenizer.
fn tokenize_bulk_query_terms(text: &str) -> Vec<String> {
    crate::utils::strings::tokenize_identifier_content_terms(text, 2, true, false)
}

/// Partitions a source or Markdown file along structural boundaries (Tree-sitter AST declarations,
/// Markdown ATX headings, or blank-line paragraph blocks) so BM25 ranks complete semantic units.
fn extract_structural_blocks(
    target_path: &Path,
    relative_path: &str,
    content: &str,
    lines: &[&str],
) -> Vec<StructuralBlock> {
    if lines.is_empty() {
        return Vec::new();
    }

    let ext_lower = target_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // 1. Markdown documents: partition along ATX heading spans
    if matches!(ext_lower.as_str(), "md" | "mdx" | "markdown") {
        let mut heading_rows: Vec<(usize, String)> = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if line.starts_with('#') {
                heading_rows.push((idx + 1, line.trim().to_string()));
            }
        }
        if !heading_rows.is_empty() {
            let mut blocks = Vec::new();
            if heading_rows[0].0 > 1 {
                let end = heading_rows[0].0 - 1;
                blocks.push(StructuralBlock {
                    file_path: relative_path.to_string(),
                    start_line: 1,
                    end_line: end,
                    heading: "preamble".to_string(),
                    text: lines[..end].join("\n"),
                });
            }
            for (i, (start, heading)) in heading_rows.iter().enumerate() {
                let end = heading_rows
                    .get(i + 1)
                    .map(|(next_s, _)| next_s.saturating_sub(1))
                    .unwrap_or(lines.len());
                if *start <= end && end <= lines.len() {
                    blocks.push(StructuralBlock {
                        file_path: relative_path.to_string(),
                        start_line: *start,
                        end_line: end,
                        heading: heading.clone(),
                        text: lines[*start - 1..end].join("\n"),
                    });
                }
            }
            return blocks;
        }
    }

    // 2. Source code: partition along Tree-sitter AST symbol boundaries
    let extractor = crate::context::ast::repomap::RepoMapExtractor::new();
    let ast_symbols = extractor
        .extract_symbols_from_str(&ext_lower, content)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.kind != "import" && s.line_number >= 1 && s.end_line >= s.line_number)
        .collect::<Vec<_>>();

    if !ast_symbols.is_empty() {
        // Prefer innermost (leaf) symbols so an enclosing `impl` block does not shadow individual
        // methods during BM25 length normalization.
        let mut selected: Vec<(usize, usize, String)> = Vec::new();
        for (i, sym) in ast_symbols.iter().enumerate() {
            let s_start = sym.line_number.min(lines.len());
            let s_end = sym.end_line.min(lines.len());
            let contains_child = ast_symbols.iter().enumerate().any(|(j, other)| {
                j != i
                    && other.line_number >= s_start
                    && other.end_line <= s_end
                    && (other.line_number > s_start || other.end_line < s_end)
            });
            if !contains_child {
                selected.push((
                    s_start,
                    s_end,
                    format!("[{}] {}", sym.kind, sym.signature.trim()),
                ));
            }
        }
        selected.sort_by_key(|(s, e, _)| (*s, *e));
        selected.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

        let mut blocks = Vec::new();
        if let Some(&(first_start, _, _)) = selected.first() {
            if first_start > 1 {
                let end = (first_start - 1).min(80);
                blocks.push(StructuralBlock {
                    file_path: relative_path.to_string(),
                    start_line: 1,
                    end_line: end,
                    heading: "imports / module header".to_string(),
                    text: lines[..end].join("\n"),
                });
            }
        }
        for (s_start, s_end, heading) in selected {
            blocks.push(StructuralBlock {
                file_path: relative_path.to_string(),
                start_line: s_start,
                end_line: s_end,
                heading,
                text: lines[s_start - 1..s_end].join("\n"),
            });
        }
        if !blocks.is_empty() {
            return blocks;
        }
    }

    // 3. Fallback for configs/logs/unsupported languages: blank-line segmented windows (<= 60 lines)
    const MAX_WINDOW: usize = 60;
    let mut blocks = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let mut end = (start + MAX_WINDOW).min(lines.len());
        if end < lines.len() {
            // Prefer breaking on a blank line in the back half of the window
            if let Some(rel_blank) = lines[start + MAX_WINDOW / 2..end]
                .iter()
                .rposition(|l| l.trim().is_empty())
            {
                end = start + MAX_WINDOW / 2 + rel_blank + 1;
            }
        }
        let first_non_empty = lines[start..end]
            .iter()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().chars().take(60).collect::<String>())
            .unwrap_or_else(|| format!("lines {}-{}", start + 1, end));
        blocks.push(StructuralBlock {
            file_path: relative_path.to_string(),
            start_line: start + 1,
            end_line: end,
            heading: first_non_empty,
            text: lines[start..end].join("\n"),
        });
        start = end;
    }
    blocks
}

/// Scores a corpus of structural blocks against `query` using Okapi BM25 ($k_1 = 1.5, b = 0.75$)
/// plus a heading/signature relevance boost, returning indices sorted by descending score.
fn rank_structural_blocks_bm25(blocks: &[StructuralBlock], query: &str) -> Vec<(usize, f64)> {
    let query_terms: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        tokenize_bulk_query_terms(query)
            .into_iter()
            .filter(|t| seen.insert(t.clone()))
            .collect()
    };
    if blocks.is_empty() || query_terms.is_empty() {
        return Vec::new();
    }

    let n_docs = blocks.len() as f64;
    let mut doc_tf: Vec<std::collections::HashMap<String, usize>> =
        Vec::with_capacity(blocks.len());
    let mut doc_lens: Vec<usize> = Vec::with_capacity(blocks.len());
    let mut heading_sets: Vec<std::collections::HashSet<String>> = Vec::with_capacity(blocks.len());
    let mut df: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for block in blocks {
        let terms = tokenize_bulk_query_terms(&block.text);
        doc_lens.push(terms.len().max(1));
        let mut tf_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for term in terms {
            *tf_map.entry(term).or_insert(0) += 1;
        }
        for k in tf_map.keys() {
            *df.entry(k.clone()).or_insert(0) += 1;
        }
        let h_set: std::collections::HashSet<String> = tokenize_bulk_query_terms(&block.heading)
            .into_iter()
            .collect();
        doc_tf.push(tf_map);
        heading_sets.push(h_set);
    }

    let avg_dl = (doc_lens.iter().sum::<usize>() as f64 / n_docs).max(1.0);
    const K1: f64 = 1.5;
    const B: f64 = 0.75;

    let mut scored: Vec<(usize, f64)> = Vec::new();
    for (idx, tf_map) in doc_tf.iter().enumerate() {
        let dl = doc_lens[idx] as f64;
        let mut score = 0.0f64;
        let mut heading_hits = 0usize;

        for q in &query_terms {
            if heading_sets[idx].contains(q) {
                heading_hits += 1;
            }
            let tf = *tf_map.get(q).unwrap_or(&0) as f64;
            if tf <= 0.0 {
                continue;
            }
            let df_t = *df.get(q).unwrap_or(&0) as f64;
            let idf = ((n_docs - df_t + 0.5) / (df_t + 0.5) + 1.0).ln();
            let numer = tf * (K1 + 1.0);
            let denom = tf + K1 * (1.0 - B + B * (dl / avg_dl));
            score += idf * (numer / denom);
        }

        if heading_hits > 0 {
            // Boost blocks whose symbol signature or heading directly matches query terms
            score = (score + 0.5 * heading_hits as f64) * 1.6;
        }
        if score > 0.0 {
            scored.push((idx, score));
        }
    }

    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| blocks[a.0].start_line.cmp(&blocks[b.0].start_line))
    });
    scored
}

/// Query-focused AST-segmented BM25 bulk-reader for a single workspace file.
/// Returns only the highest-relevance AST declarations/sections with exact 1-indexed line spans.
pub fn read_file_with_query(
    workspace_root: &Path,
    relative_path: &str,
    query: &str,
    hashline: bool,
    max_lines: Option<usize>,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
    let confined = ConfinedRoot::open(workspace_root)?;
    crate::context::governance::dox::DoxEngine::sync_and_harvest_rules(
        workspace_root,
        &[relative_path],
    );
    crate::context::governance::dox::DoxEngine::on_file_mutated(workspace_root, relative_path);

    let content = confined.read_to_string(Path::new(relative_path))?;
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    if total_lines == 0 {
        return Ok(String::new());
    }

    let line_budget = max_lines
        .unwrap_or(crate::constants::READ_FILE_DEFAULT_WINDOW)
        .clamp(40, 600);

    // Small file already fits comfortably inside the budget — return directly
    if total_lines <= line_budget / 2 {
        return read_file_with_options(workspace_root, relative_path, None, None, hashline);
    }

    let blocks = extract_structural_blocks(&target_path, relative_path, &content, &lines);
    let ranked = rank_structural_blocks_bm25(&blocks, query);

    if ranked.is_empty() {
        let mut fallback = format!(
            "[Bulk-Reader Query: `{}` — 0 lexical AST matches in `{}` ({} lines). Showing file outline + head:]\n\n",
            query.trim(),
            relative_path,
            total_lines
        );
        fallback.push_str(&read_file_with_options(
            workspace_root,
            relative_path,
            Some(1),
            Some(total_lines.min(80)),
            hashline,
        )?);
        if let Some(outline) = build_unbounded_read_ast_outline_hint(
            &target_path,
            relative_path,
            &content,
            &lines,
            total_lines.min(80),
            total_lines,
        ) {
            fallback.push('\n');
            fallback.push_str(&outline);
        }
        return Ok(fallback);
    }

    // Greedily pack highest-scoring AST blocks up to `line_budget`
    let mut chosen: Vec<(usize, f64)> = Vec::new();
    let mut used_lines = 0usize;
    for (block_idx, score) in ranked {
        let blk = &blocks[block_idx];
        let span = blk.end_line.saturating_sub(blk.start_line) + 1;
        if used_lines > 0 && used_lines + span > line_budget {
            continue;
        }
        chosen.push((block_idx, score));
        used_lines += span.min(line_budget);
        if used_lines >= line_budget || chosen.len() >= 8 {
            break;
        }
    }

    // Re-sort chosen blocks in natural top-to-bottom source order
    chosen.sort_by_key(|(idx, _)| blocks[*idx].start_line);

    let saved_pct = if total_lines > 0 {
        100.0 * (1.0 - (used_lines.min(total_lines) as f64 / total_lines as f64))
    } else {
        0.0
    };

    let mut out = format!(
        "# Bulk-Reader (`{}` | query: `{}` | {} block(s), {}/{} lines — {:.0}% context saved)\n",
        relative_path,
        query.trim(),
        chosen.len(),
        used_lines.min(total_lines),
        total_lines,
        saved_pct.max(0.0)
    );

    for (block_idx, score) in chosen {
        let blk = &blocks[block_idx];
        let clamped_end = blk.end_line.min(blk.start_line + line_budget - 1);
        out.push_str(&format!(
            "\n## L{}-L{} {} (bm25={:.2})\n",
            blk.start_line, clamped_end, blk.heading, score
        ));
        for line_no in blk.start_line..=clamped_end {
            if let Some(line_str) = lines.get(line_no - 1) {
                if hashline {
                    out.push_str(&crate::tools::hashline::format_hashline(line_no, line_str));
                } else {
                    out.push_str(&format!("{:>4} | {}\n", line_no, line_str));
                }
            }
        }
    }

    if let Some(subdir_rules) =
        crate::context::governance::dox::DoxEngine::inject_unseen_subdirectory_rules(
            workspace_root,
            relative_path,
        )
    {
        out.push_str(&subdir_rules);
    }

    Ok(out)
}

/// Multi-file query-focused AST-segmented Global Okapi BM25 bulk-reader.
/// Indexes structural blocks across all requested files concurrently and returns only the top
/// globally ranked spans within the line budget.
pub fn batch_read_with_query(
    workspace_root: &Path,
    paths: &[String],
    query: &str,
    max_lines: Option<usize>,
) -> Result<String> {
    if paths.is_empty() {
        return Err(ToolError::InvalidArguments {
            name: "batch_read".to_string(),
            reason: "paths array cannot be empty".to_string(),
        }
        .into());
    }

    let capped_paths = if paths.len() > MAX_BATCH_READ_FILES {
        &paths[..MAX_BATCH_READ_FILES]
    } else {
        paths
    };
    let rel_refs: Vec<&str> = capped_paths.iter().map(|s| s.as_str()).collect();
    crate::context::governance::dox::DoxEngine::sync_and_harvest_rules(workspace_root, &rel_refs);

    let per_file_blocks: Vec<_> = std::thread::scope(|s| {
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
                    let content = match ConfinedRoot::open(workspace_root)
                        .and_then(|c| c.read_to_string(Path::new(&rel)))
                    {
                        Ok(c) => c,
                        Err(e) => return (rel, Err(e.to_string())),
                    };
                    let lines: Vec<&str> = content.lines().collect();
                    let total_lines = lines.len();
                    let blocks = extract_structural_blocks(&target_path, &rel, &content, &lines);
                    (rel, Ok((total_lines, blocks)))
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

    let mut all_blocks: Vec<StructuralBlock> = Vec::new();
    let mut total_corpus_lines = 0usize;
    let mut errors: Vec<(String, String)> = Vec::new();

    for (rel, outcome) in per_file_blocks {
        match outcome {
            Ok((line_cnt, blocks)) => {
                total_corpus_lines += line_cnt;
                all_blocks.extend(blocks);
            }
            Err(e) => errors.push((rel, e)),
        }
    }

    let ranked = rank_structural_blocks_bm25(&all_blocks, query);
    let total_budget = max_lines.unwrap_or(240).clamp(60, 800);

    if ranked.is_empty() {
        let mut out = format!(
            "# Batch Bulk-Reader (query: `{}` | 0 AST matches across {} files, {} total lines)\n",
            query.trim(),
            capped_paths.len(),
            total_corpus_lines
        );
        for (rel, err) in errors {
            out.push_str(&format!("- `{rel}`: error: {err}\n"));
        }
        return Ok(out);
    }

    let mut chosen: Vec<(usize, f64)> = Vec::new();
    let mut used_lines = 0usize;
    for (idx, score) in ranked {
        let blk = &all_blocks[idx];
        let span = blk.end_line.saturating_sub(blk.start_line) + 1;
        if used_lines > 0 && used_lines + span > total_budget {
            continue;
        }
        chosen.push((idx, score));
        used_lines += span.min(total_budget);
        if used_lines >= total_budget || chosen.len() >= 12 {
            break;
        }
    }

    let saved_pct = if total_corpus_lines > 0 {
        100.0 * (1.0 - (used_lines.min(total_corpus_lines) as f64 / total_corpus_lines as f64))
    } else {
        0.0
    };

    let mut out = format!(
        "# Batch Bulk-Reader ({} files | query: `{}` | {} top AST block(s), {}/{} lines — {:.0}% context saved)\n",
        capped_paths.len(),
        query.trim(),
        chosen.len(),
        used_lines.min(total_corpus_lines),
        total_corpus_lines,
        saved_pct.max(0.0)
    );

    for (idx, score) in chosen {
        let blk = &all_blocks[idx];
        out.push_str(&format!(
            "\n## `{}` L{}-L{} {} (bm25={:.2})\n",
            blk.file_path, blk.start_line, blk.end_line, blk.heading, score
        ));
        for (offset, line_str) in blk.text.lines().take(total_budget).enumerate() {
            out.push_str(&format!("{:>4} | {}\n", blk.start_line + offset, line_str));
        }
    }

    if !errors.is_empty() {
        out.push_str("\n### Unreadable Files\n");
        for (rel, err) in errors {
            out.push_str(&format!("- `{rel}`: {err}\n"));
        }
    }

    Ok(out)
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
    look_at_with_options(workspace_root, relative_path, extract, None)
}

/// Inspects a file or Markdown directory structure, supporting vectorless `PageIndex` multi-doc
/// navigation and exact `[Lstart-Lend]` section drilldowns (`section` query).
pub fn look_at_with_options(
    workspace_root: &Path,
    relative_path: &str,
    extract: Option<&str>,
    section: Option<&str>,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;

    if let Some(sec_query) = section.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(drilldown) = crate::context::doc_tree::drilldown_markdown_section(
            &target_path,
            relative_path,
            sec_query,
        ) {
            return Ok(drilldown);
        }
        return Err(ToolError::invalid_args(
            "look_at",
            format!(
                "No Markdown heading matching section '{}' found under '{}'. Call look_at(path: \"{}\") without `section` to view the PageIndex outline.",
                sec_query, relative_path, relative_path
            ),
        )
        .into());
    }

    if target_path.is_dir() {
        if let Some(forest) =
            crate::context::doc_tree::render_directory_page_index(&target_path, relative_path)
        {
            return Ok(forest);
        }
        return Err(ToolError::invalid_args(
            "look_at",
            format!(
                "Directory '{}' contains no Markdown/OKF documents for PageIndex navigation. Use list_dir(path: \"{}\") to inspect source directories.",
                relative_path, relative_path
            ),
        )
        .into());
    }

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

    let content = ConfinedRoot::open(workspace_root)?.read_to_string(Path::new(relative_path))?;

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

    if matches!(ext_lower.as_str(), "md" | "mdx" | "markdown") {
        return Ok(crate::context::doc_tree::render_markdown_page_index(
            relative_path,
            &content,
        ));
    }

    let mut out = format!(
        "# `{}` ({} lines, {:.1} KB — structural outline)\n\n",
        relative_path, total_lines, size_kb
    );

    let want_imports = mode == "all" || mode == "imports";
    let want_exports = mode == "all" || mode == "exports";
    let want_structure = mode == "all" || mode == "structure";

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
            // Protect substantial existing files from accidental clobbering when the caller omits
            // both `append: true` and `overwrite: true` (even inside an active turn transaction,
            // since `prepare_file_mutation` journals files before `write_file_inner` runs):
            // 1. Suspicious truncation (> 40 lines replaced by < 25% of original lines), OR
            // 2. Replacing an existing full HTML document (`<!doctype`/`<html`) with a bare HTML
            //    section fragment (a classic multi-chunk write where `append: true` was omitted).
            let orig_lines = orig_content.lines().count();
            let new_lines = content.lines().count();
            let is_suspicious_truncation =
                orig_lines > 40 && new_lines.saturating_mul(4) < orig_lines;
            let is_html_file = clean_rel.ends_with(".html") || clean_rel.ends_with(".htm");
            let is_html_document_clobber = if is_html_file {
                let orig_lower = orig_content.to_ascii_lowercase();
                let new_lower = content.to_ascii_lowercase();
                let orig_is_full_doc =
                    orig_lower.contains("<!doctype") || orig_lower.contains("<html");
                let new_is_full_doc =
                    new_lower.contains("<!doctype") || new_lower.contains("<html");
                orig_is_full_doc && !new_is_full_doc
            } else {
                false
            };

            if is_suspicious_truncation || is_html_document_clobber {
                let detail = if is_html_document_clobber {
                    "New content looks like an HTML fragment (missing `<!DOCTYPE html>` / `<html>`) that would wipe the existing HTML document."
                } else {
                    "New content would drop over 75% of the existing file."
                };
                return Err(ToolError::InvalidArguments {
                    name: "write_file".to_string(),
                    reason: format!(
                        "Target file '{}' already exists ({} lines, {} bytes). {} To prevent accidental file destruction, write_file requires explicit intent:\n\
                        • To append new sections to the end of the file: pass `append: true`.\n\
                        • To completely replace and overwrite the file: pass `overwrite: true`.\n\
                        • To surgically edit specific lines: use `patch_file`.",
                        relative_path,
                        orig_lines,
                        orig_content.len(),
                        detail
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
    let file_existed = target_path.exists();
    crate::context::syntax_guard::SyntaxGuard::check_syntax_barrier(
        &target_path,
        &orig_content,
        &effective_content,
    )
    .map_err(|mut reason| {
        if !file_existed {
            reason.push_str(&format!(
                "\n(Note: Because this pre-write barrier rejected the write before touching disk, '{}' was NOT created on disk — no cleanup or `rm` is needed.)",
                relative_path
            ));
        }
        ToolError::PatchFailed {
            path: relative_path.to_string(),
            reason,
        }
    })?;

    // Kernel-enforced atomic write (`openat` + `O_NOFOLLOW` + `renameat`) eliminating TOCTOU races
    ConfinedRoot::open(workspace_root)?
        .atomic_write_bytes(Path::new(relative_path), effective_content.as_bytes())?;

    let mut_op = if file_existed {
        crate::context::search::trigram::FileMutationOp::Modify
    } else {
        crate::context::search::trigram::FileMutationOp::Create
    };
    crate::context::index::SymbolIndex::on_file_mutated(workspace_root, relative_path, mut_op);
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

    if relative_path.ends_with(".css") {
        if let Err(css_advisory) =
            crate::context::syntax_guard::SyntaxGuard::check_css_semantic_barrier(
                &target_path,
                &effective_content,
            )
        {
            msg.push_str("\n\n");
            msg.push_str(&css_advisory);
        }
    }

    if run_compiler_check {
        let diag_block = crate::tools::compiler::ScopedCompiler::run_post_edit_diagnostics(
            workspace_root,
            relative_path,
            &effective_content,
        );
        if !diag_block.is_empty() {
            msg.push_str(&diag_block);
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

/// Formats patch success message, preserving all post-edit diagnostic blocks (CSS advisories,
/// scoped compiler feedback, linter feedback, and MiniDoctor Auto-Guard warnings).
fn format_patch_success(tier_desc: &str, relative_path: &str, write_res: &str) -> String {
    let mut msg = format!("Successfully patched {}", relative_path);
    if !tier_desc.is_empty() {
        msg.push_str(&format!(" ({})", tier_desc));
    }
    const FEEDBACK_MARKERS: &[&str] =
        &["\n\n⚠️", "\n\n[Compiler", "\n\n[Linter", "\n\n[MiniDoctor"];
    if let Some(earliest) = FEEDBACK_MARKERS
        .iter()
        .filter_map(|marker| write_res.find(marker))
        .min()
    {
        msg.push_str(&write_res[earliest..]);
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
/// Applies a single search-and-replace hunk against `original` in memory using the 5-tier
/// cascade (Exact -> CRLF/trailing-ws -> Auto-reindented -> Blank-line relaxed -> Fuzzy diff).
/// Returns `(patched_content, match_tier_label)`.
pub fn apply_patch_hunk_in_memory(
    original: &str,
    relative_path: &str,
    search_block: &str,
    replace_block: &str,
) -> Result<(String, &'static str)> {
    let stripped_search = crate::tools::hashline::strip_line_prefixes(search_block);
    let stripped_replace = crate::tools::hashline::strip_line_prefixes(replace_block);
    let search_block = stripped_search.as_str();
    let replace_block = stripped_replace.as_str();

    // 1. Tier 1: Exact Substring Match
    if original.contains(search_block) {
        let occurrences = original.matches(search_block).count();
        if occurrences > 1 {
            let match_lines = find_all_match_lines(original, search_block);
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
        return Ok((new_content, ""));
    }

    // 2. Tier 2: CRLF & Trailing Whitespace Normalization
    if let Some(new_content) =
        try_crlf_and_trailing_whitespace_replace(original, search_block, replace_block)
    {
        return Ok((new_content, "whitespace-normalized match"));
    }

    // 3. Tier 3: Whitespace/Indentation-Insensitive Match with Auto-Re-Indentation
    if let Some(new_content) =
        try_whitespace_normalized_replace(original, search_block, replace_block)
    {
        return Ok((new_content, "auto-reindented match"));
    }

    // 4. Tier 4: Blank-Line Relaxed Match with Auto-Re-Indentation
    if let Some(new_content) = try_blank_line_relaxed_replace(original, search_block, replace_block)
    {
        return Ok((new_content, "blank-line relaxed match"));
    }

    // 5. Tier 5: High-Threshold Fuzzy Diff with Strict Uniqueness Gap & Auto-Re-Indentation
    let diag = match try_fuzzy_replace_with_diagnostic(original, search_block, replace_block) {
        Ok(new_content) => {
            return Ok((new_content, "fuzzy matched with auto-reindentation"));
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

fn build_identical_block_diagnostic(
    original: &str,
    relative_path: &str,
    search_block: &str,
) -> String {
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
        return format!(
            "No changes applied to {}: search_block and replace_block are identical (block is already in the file).\n\n[Surrounding Context in '{}' (lines {}-{})]:\n{}💡 Do NOT repeat this identical `patch_file` call. Pass the existing lines above as `search_block` and your modified code as `replace_block`.",
            relative_path, relative_path, ctx_start, ctx_end, snippet
        );
    }

    let diag = find_nearest_match_diagnostic(original, search_block);
    let mut msg = format!(
        "No changes applied to {}: search_block and replace_block are identical, and this block is not currently present in the file.",
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
    msg
}

/// Applies multiple search-and-replace hunks atomically in memory before running the AST
/// syntax barrier and scoped compiler check once on the final patched content.
pub fn patch_file_hunks(
    workspace_root: &Path,
    relative_path: &str,
    hunks: &[(String, String)],
) -> Result<String> {
    if hunks.len() == 1 {
        return patch_file(workspace_root, relative_path, &hunks[0].0, &hunks[0].1);
    }

    let _ = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
    let original = ConfinedRoot::open(workspace_root)?.read_to_string(Path::new(relative_path))?;
    let mut current_content = original.clone();
    let mut applied_count = 0usize;
    let mut first_identical_block: Option<String> = None;

    for (idx, (search_block, replace_block)) in hunks.iter().enumerate() {
        let stripped_search = crate::tools::hashline::strip_line_prefixes(search_block);
        let stripped_replace = crate::tools::hashline::strip_line_prefixes(replace_block);
        if stripped_search == stripped_replace {
            if !current_content.contains(&stripped_search) {
                return Err(ToolError::PatchFailed {
                    path: relative_path.to_string(),
                    reason: format!(
                        "Multi-hunk patch aborted at hunk {}/{} (previous hunks rolled back): {}",
                        idx + 1,
                        hunks.len(),
                        build_identical_block_diagnostic(
                            &current_content,
                            relative_path,
                            &stripped_search
                        )
                    ),
                }
                .into());
            }
            if first_identical_block.is_none() {
                first_identical_block = Some(stripped_search);
            }
            continue;
        }
        match apply_patch_hunk_in_memory(
            &current_content,
            relative_path,
            &stripped_search,
            &stripped_replace,
        ) {
            Ok((next_content, _tier)) => {
                if next_content != current_content {
                    applied_count += 1;
                }
                current_content = next_content;
            }
            Err(err) => {
                return Err(ToolError::PatchFailed {
                    path: relative_path.to_string(),
                    reason: format!(
                        "Multi-hunk patch aborted at hunk {}/{} (previous hunks rolled back): {}",
                        idx + 1,
                        hunks.len(),
                        err
                    ),
                }
                .into());
            }
        }
    }

    if applied_count == 0 || current_content == original {
        let sample = first_identical_block.as_deref().unwrap_or("");
        return Err(ToolError::PatchFailed {
            path: relative_path.to_string(),
            reason: build_identical_block_diagnostic(&original, relative_path, sample),
        }
        .into());
    }

    let write_res = write_file(workspace_root, relative_path, &current_content)?;
    Ok(format!(
        "Successfully applied {} hunks to {} ({})",
        applied_count,
        relative_path,
        format_patch_success("", relative_path, &write_res)
    ))
}

pub fn patch_file(
    workspace_root: &Path,
    relative_path: &str,
    search_block: &str,
    replace_block: &str,
) -> Result<String> {
    let stripped_search = crate::tools::hashline::strip_line_prefixes(search_block);
    let stripped_replace = crate::tools::hashline::strip_line_prefixes(replace_block);
    let search_block = stripped_search.as_str();
    let replace_block = stripped_replace.as_str();

    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
    let _ = &target_path;

    let original = ConfinedRoot::open(workspace_root)?.read_to_string(Path::new(relative_path))?;

    if search_block == replace_block {
        return Err(ToolError::PatchFailed {
            path: relative_path.to_string(),
            reason: build_identical_block_diagnostic(&original, relative_path, search_block),
        }
        .into());
    }

    let (new_content, tier_label) =
        apply_patch_hunk_in_memory(&original, relative_path, search_block, replace_block)?;
    let write_res = write_file(workspace_root, relative_path, &new_content)?;
    Ok(format_patch_success(tier_label, relative_path, &write_res))
}

/// Applies a standard unified diff (`@@ -old,len +new,len @@`) atomically to `relative_path`
/// under kernel-confined `ConfinedRoot` reads/writes, running the full AST syntax barrier,
/// scoped compiler check, and MiniDoctor auto-guard.
pub fn patch_file_unified_diff(
    workspace_root: &Path,
    relative_path: &str,
    unified_diff: &str,
) -> Result<String> {
    let _ = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
    let original = ConfinedRoot::open(workspace_root)?.read_to_string(Path::new(relative_path))?;

    let (new_content, hunk_count, tier_label) =
        crate::tools::diff::apply_unified_diff_in_memory(&original, unified_diff).map_err(
            |reason| ToolError::PatchFailed {
                path: relative_path.to_string(),
                reason,
            },
        )?;

    if new_content == original {
        return Err(ToolError::PatchFailed {
            path: relative_path.to_string(),
            reason: "Unified diff produced no net change to the file.".to_string(),
        }
        .into());
    }

    let write_res = write_file(workspace_root, relative_path, &new_content)?;
    Ok(format!(
        "✔ Applied {} unified-diff hunk(s) ({}) to {}\n{}",
        hunk_count, tier_label, relative_path, write_res
    ))
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

/// Options for `replace_in_files_with_options` (`chmln/sd`-style literal-first / word-boundary /
/// capture-group regex replacement + `dry_run` unified-diff preview).
#[derive(Debug, Clone, Copy, Default)]
pub struct ReplaceInFilesOptions {
    pub dry_run: bool,
    pub word_boundary: bool,
    pub regex_mode: bool,
}

/// Replaces occurrences of literal `needle` with `replacement` only when surrounded by non-identifier
/// boundaries (pure ASCII/Unicode identifier boundary check — zero regexes).
fn replace_literal_word_boundary(
    content: &str,
    needle: &str,
    replacement: &str,
) -> (String, usize) {
    if needle.is_empty() {
        return (content.to_string(), 0);
    }
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(content.len());
    let mut count = 0usize;
    let mut cursor = 0usize;

    while let Some(rel_idx) = content[cursor..].find(needle) {
        let start = cursor + rel_idx;
        let end = start + needle.len();
        let left_ok = content[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident(c));
        let right_ok = content[end..].chars().next().is_none_or(|c| !is_ident(c));

        if left_ok && right_ok {
            out.push_str(&content[cursor..start]);
            out.push_str(replacement);
            count += 1;
            cursor = end;
        } else {
            // Advance past one character so overlapping candidates aren't skipped
            let next_step = content[start..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
            out.push_str(&content[cursor..start + next_step]);
            cursor = start + next_step;
        }
    }
    out.push_str(&content[cursor..]);
    (out, count)
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
    replace_in_files_with_options(
        workspace_root,
        glob_pattern,
        search_block,
        replace_block,
        ReplaceInFilesOptions::default(),
        backup_manager,
        turn_id,
    )
}

/// Trigram-accelerated batch find-and-replace supporting `dry_run` preview, pure-Rust `word_boundary`
/// matching, and opt-in `$1`/`$name` capture-group `regex_mode` (literal-first default like `sd -F`),
/// validated atomically by `SyntaxGuard` and committed via `ConfinedRoot`.
pub fn replace_in_files_with_options(
    workspace_root: &Path,
    glob_pattern: &str,
    search_block: &str,
    replace_block: &str,
    options: ReplaceInFilesOptions,
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

    let compiled_regex = if options.regex_mode {
        let pat = if options.word_boundary {
            format!(r"\b(?:{search_block})\b")
        } else {
            search_block.to_string()
        };
        Some(
            regex::Regex::new(&pat).map_err(|e| ToolError::InvalidArguments {
                name: "replace_in_files".to_string(),
                reason: format!("Invalid regex pattern '{}': {}", search_block, e),
            })?,
        )
    } else {
        None
    };

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

    // Trigram candidate pre-filter (`microsoft/tgrep` acceleration): skip files that cannot
    // contain the required trigrams before reading them from disk.
    let trigram_candidates: Option<std::collections::HashSet<String>> = {
        let plan =
            crate::context::search::trigram::build_query_plan(search_block, options.regex_mode);
        if plan.is_match_all() {
            None
        } else {
            crate::context::search::trigram::TrigramIndex::get_or_build_cached(workspace_root)
                .ok()
                .map(|idx| {
                    idx.query_candidate_files(&plan, None)
                        .into_iter()
                        .map(|p| p.replace('\\', "/"))
                        .collect()
                })
        }
    };

    let walker = ignore::WalkBuilder::new(workspace_root)
        .overrides(overrides)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    let confined = ConfinedRoot::open(workspace_root)?;
    let mut matched_files: Vec<(PathBuf, String, String, usize)> = Vec::new();
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
        let rel_path = file_path.strip_prefix(workspace_root).unwrap_or(&file_path);
        let rel_norm = rel_path.to_string_lossy().replace('\\', "/");

        if let Some(ref candidates) = trigram_candidates {
            if !candidates.contains(&rel_norm) {
                skipped_files.push(format!("  • {}", rel_norm));
                continue;
            }
        }

        let content = match confined.read_to_string(Path::new(&rel_norm)) {
            Ok(c) => c,
            Err(_) => continue, // Skip binary or unreadable files
        };

        let (new_content, match_count) = if let Some(ref re) = compiled_regex {
            let cnt = re.find_iter(&content).count();
            if cnt == 0 {
                (String::new(), 0)
            } else {
                (re.replace_all(&content, replace_block).into_owned(), cnt)
            }
        } else if options.word_boundary {
            replace_literal_word_boundary(&content, search_block, replace_block)
        } else if content.contains(search_block) {
            let cnt = content.matches(search_block).count();
            (content.replace(search_block, replace_block), cnt)
        } else {
            (String::new(), 0)
        };

        if match_count > 0 && new_content != content {
            // Pre-Write AST Syntax Barrier: verify that tentative content does not introduce syntax errors
            crate::context::syntax_guard::SyntaxGuard::check_syntax_barrier(
                &file_path,
                &content,
                &new_content,
            )
            .map_err(|reason| ToolError::PatchFailed {
                path: rel_norm.clone(),
                reason: format!("Batch replacement rejected: {}", reason),
            })?;

            matched_files.push((file_path, content, new_content, match_count));
        } else {
            skipped_files.push(format!("  • {}", rel_norm));
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

    if options.dry_run {
        let total_replacements: usize = matched_files.iter().map(|(_, _, _, c)| *c).sum();
        let mut preview = format!(
            "[DRY RUN — no files modified on disk] Would apply {} replacement(s) across {} of {} file(s) matching '{}':\n\n```diff\n",
            total_replacements,
            matched_files.len(),
            files_scanned,
            norm_pattern
        );
        for (file_path, orig, updated, _) in matched_files.iter().take(12) {
            let rel = file_path
                .strip_prefix(workspace_root)
                .unwrap_or(file_path)
                .to_string_lossy();
            let diff = TextDiff::from_lines(orig.as_str(), updated.as_str());
            preview.push_str(
                &diff
                    .unified_diff()
                    .context_radius(2)
                    .header(&format!("a/{rel}"), &format!("b/{rel}"))
                    .to_string(),
            );
        }
        preview.push_str("```");
        return Ok(preview);
    }

    // Apply all edits atomically with safety backups
    let mut modified_summaries = Vec::new();
    let first_modified_rel = matched_files[0]
        .0
        .strip_prefix(workspace_root)
        .unwrap_or(&matched_files[0].0)
        .display()
        .to_string();

    for (file_path, _orig_content, new_content, _cnt) in &matched_files {
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

    let first_modified_content = &matched_files[0].2;
    let diag_block = crate::tools::compiler::ScopedCompiler::run_post_edit_diagnostics(
        workspace_root,
        &first_modified_rel,
        first_modified_content,
    );
    if !diag_block.is_empty() {
        result_msg.push_str(&diag_block);
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
        let chunk1: String = (0..45).map(|i| format!("<div>Line {i}</div>\n")).collect();
        // First write creates a >40-line file cleanly
        let res1 = write_file_with_options(&temp_dir, rel_path, &chunk1, false, false);
        assert!(res1.is_ok());

        let chunk2 = "<section>Hero</section>\n";
        // Second write replacing >40 lines with 1 line without append or overwrite is rejected by the truncation guard
        let res2 = write_file_with_options(&temp_dir, rel_path, chunk2, false, false);
        assert!(res2.is_err());
        let err_str = res2.unwrap_err().to_string();
        assert!(err_str.contains("already exists"));
        assert!(err_str.contains("append: true"));

        // Write with append: true succeeds
        let res3 = write_file_with_options(&temp_dir, rel_path, chunk2, true, false);
        assert!(res3.is_ok());
        let content = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert!(content.contains(&chunk1));
        assert!(content.contains(chunk2));

        // Write with overwrite: true replaces
        let chunk3 = "<!DOCTYPE html><html><body>New Only</body></html>";
        let res4 = write_file_with_options(&temp_dir, rel_path, chunk3, false, true);
        assert!(res4.is_ok());
        let content2 = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert_eq!(content2, chunk3);

        // minitest77 regression: replacing an existing full `<!DOCTYPE html>` document with a
        // same-sized bare `<section>` fragment without `append: true` or `overwrite: true` MUST
        // be rejected even when an active workspace transaction already tracks the file!
        let _tx =
            crate::session::transaction::TransactionManager::begin(&temp_dir, "test-tx").unwrap();
        let _ = crate::session::transaction::TransactionManager::record_mutation_pre(
            &temp_dir,
            &temp_dir.join(rel_path),
        );
        let html_fragment =
            "<!-- === SECTION 2 === -->\n<section id=\"swarm\"><h2>Swarm</h2></section>\n";
        let res5 = write_file_with_options(&temp_dir, rel_path, html_fragment, false, false);
        assert!(
            res5.is_err(),
            "Bare HTML fragment must not clobber full HTML document without append/overwrite"
        );
        assert!(res5.unwrap_err().to_string().contains("append: true"));

        // Appending the fragment cleanly inserts before </body>
        let res6 = write_file_with_options(&temp_dir, rel_path, html_fragment, true, false);
        assert!(res6.is_ok());
        let merged = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert!(merged.contains("<!DOCTYPE html>"));
        assert!(merged.contains("<section id=\"swarm\">"));
        assert!(merged.ends_with("</body></html>"));

        let _ = crate::session::transaction::TransactionManager::commit(&temp_dir, None);
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
        assert_eq!(&props[..3], &["path", "search_block", "updated_code"]);

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
    fn test_patch_file_identical_blocks_fails_with_diagnostic() {
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
        let err = patch_file(&temp_dir, "src/Section.tsx", absent_block, absent_block)
            .unwrap_err()
            .to_string();
        assert!(err.contains("No changes applied to src/Section.tsx:"));
        assert!(err.contains("not currently present in the file"));
        assert!(err.contains("max-w-2xl"));

        let present_block = "  return <div className=\"max-w-2xl\">Hello</div>;";
        let err2 = patch_file(&temp_dir, "src/Section.tsx", present_block, present_block)
            .unwrap_err()
            .to_string();
        assert!(err2.contains("No changes applied to src/Section.tsx:"));
        assert!(err2.contains("already in the file"));

        // Multi-hunk where all hunks are identical must also fail rather than claiming 2 hunks applied
        let multi_noop = vec![
            (
                "export function Section() {".to_string(),
                "export function Section() {".to_string(),
            ),
            (present_block.to_string(), present_block.to_string()),
        ];
        let err3 = patch_file_hunks(&temp_dir, "src/Section.tsx", &multi_noop)
            .unwrap_err()
            .to_string();
        assert!(err3.contains("No changes applied to src/Section.tsx:"));

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

    #[test]
    fn test_patch_file_hunks_applies_jsx_wrapper_atomically() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_multi_hunk_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        write_file(
            &temp_dir,
            "src/Swarm.tsx",
            "export function Swarm() {\n  return (\n    <section>\n      <svg viewBox=\"0 0 100 100\">\n        <circle cx=\"50\" cy=\"50\" r=\"40\" />\n      </svg>\n    </section>\n  );\n}\n",
        )
        .unwrap();

        // Hunk 1 opens a wrapper <div> around <svg>, Hunk 2 closes </div> after </svg>.
        // Evaluating SyntaxGuard after Hunk 1 alone would fail with unclosed <div>;
        // patch_file_hunks applies both in memory before checking AST syntax.
        let hunks = vec![
            (
                "      <svg viewBox=\"0 0 100 100\">".to_string(),
                "      <div className=\"-mx-1 overflow-x-auto px-1\">\n        <svg viewBox=\"0 0 100 100\">".to_string(),
            ),
            (
                "      </svg>\n    </section>".to_string(),
                "        </svg>\n      </div>\n    </section>".to_string(),
            ),
        ];

        let out = patch_file_hunks(&temp_dir, "src/Swarm.tsx", &hunks).unwrap();
        assert!(out.contains("Successfully applied 2 hunks to src/Swarm.tsx"));

        let updated = std::fs::read_to_string(temp_dir.join("src/Swarm.tsx")).unwrap();
        assert!(updated.contains("<div className=\"-mx-1 overflow-x-auto px-1\">"));
        assert!(updated.contains("</div>\n    </section>"));

        // Multi-hunk call where all hunks are identical no-ops MUST return Err, not false-positive Ok
        let noop_hunks = vec![
            ("  return (".to_string(), "  return (".to_string()),
            ("    </section>".to_string(), "    </section>".to_string()),
        ];
        let noop_err = patch_file_hunks(&temp_dir, "src/Swarm.tsx", &noop_hunks).unwrap_err();
        assert!(noop_err
            .to_string()
            .contains("search_block and replace_block are identical"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[tokio::test]
    async fn test_patch_file_schema_is_alphabetical_order_safe_and_updates_work() {
        let schemas = crate::tools::registry::fs_tools::get_schemas();
        let patch_schema = schemas
            .iter()
            .find(|s| s.name == "patch_file")
            .expect("patch_file schema");

        // Even under server-side alphabetical (`a..z`) JSON key canonicalization (e.g. MiniMax),
        // `"path"` ('p') MUST precede `"search_block"` ('s'), which MUST precede `"updated_code"` ('u')
        // and `"updates"` ('u'), and `"old_code"` ('o') MUST precede `"replace_block"` ('r').
        let mut top_keys: Vec<String> = patch_schema.parameters["properties"]
            .as_object()
            .expect("properties")
            .keys()
            .cloned()
            .collect();
        top_keys.sort();
        let path_pos = top_keys.iter().position(|k| k == "path").expect("path");
        let search_pos = top_keys
            .iter()
            .position(|k| k == "search_block")
            .expect("search_block");
        let updated_pos = top_keys
            .iter()
            .position(|k| k == "updated_code")
            .expect("updated_code");
        let updates_pos = top_keys
            .iter()
            .position(|k| k == "updates")
            .expect("updates");
        assert!(path_pos < search_pos);
        assert!(search_pos < updated_pos);
        assert!(path_pos < updates_pos);

        let mut item_keys: Vec<String> = patch_schema.parameters["properties"]["updates"]["items"]
            ["properties"]
            .as_object()
            .expect("updates.items.properties")
            .keys()
            .cloned()
            .collect();
        item_keys.sort();
        assert_eq!(item_keys, vec!["old_code", "replace_block"]);

        let temp_dir =
            std::env::temp_dir().join(format!("minicode_patch_updates_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();
        write_file(
            &temp_dir,
            "src/Hero.tsx",
            "export const a = 1;\nexport const b = 2;\n",
        )
        .unwrap();

        let args = serde_json::json!({
            "path": "src/Hero.tsx",
            "updates": [
                { "old_code": "export const a = 1;", "replace_block": "export const a = 10;" },
                { "old_code": "export const b = 2;", "replace_block": "export const b = 20;" }
            ]
        });
        let res =
            crate::tools::registry::fs_tools::dispatch("patch_file", &args, &temp_dir, None, 1)
                .await
                .expect("dispatched")
                .expect("patch_file updates succeeded");
        assert!(res.contains("Successfully applied 2 hunks"));
        let content = std::fs::read_to_string(temp_dir.join("src/Hero.tsx")).unwrap();
        assert!(content.contains("export const a = 10;"));
        assert!(content.contains("export const b = 20;"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[tokio::test]
    async fn test_look_at_directory_page_index_and_section_drilldown() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_look_at_pageindex_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(temp_dir.join("docs")).unwrap();
        write_file(
            &temp_dir,
            "docs/architecture.md",
            "# System Architecture\nOverview.\n\n## Binary IVF\nRoutes queries across 128-bit centroids.\n\n## QJL Residual\nProjects orthogonal error into 1-bit sign planes.\n",
        )
        .unwrap();

        let dir_args = serde_json::json!({ "path": "docs" });
        let dir_tree =
            crate::tools::registry::fs_tools::dispatch("look_at", &dir_args, &temp_dir, None, 1)
                .await
                .expect("dispatched")
                .expect("dir look_at succeeded");
        assert!(dir_tree.contains("Multi-Doc PageIndex Navigator"));
        assert!(dir_tree.contains("docs/architecture.md"));
        assert!(dir_tree.contains("QJL Residual"));

        let sec_args = serde_json::json!({
            "path": "docs",
            "section": "architecture#QJL Residual"
        });
        let drill =
            crate::tools::registry::fs_tools::dispatch("look_at", &sec_args, &temp_dir, None, 1)
                .await
                .expect("dispatched")
                .expect("section drilldown succeeded");
        assert!(drill.contains("PageIndex Drilldown: `docs/architecture.md`"));
        assert!(drill.contains("Projects orthogonal error into 1-bit sign planes."));
        assert!(!drill.contains("Routes queries across 128-bit centroids."));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[tokio::test]
    async fn test_patch_file_unified_diff_end_to_end() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_unified_diff_e2e_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();
        write_file(
            &temp_dir,
            "src/math.rs",
            "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn mul(a: i32, b: i32) -> i32 {\n    a * b\n}\n",
        )
        .unwrap();

        // Dispatch through the registry `patch_file` tool using `unified_diff` (with shifted line numbers)
        let args = serde_json::json!({
            "path": "src/math.rs",
            "unified_diff": "@@ -1,3 +1,3 @@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a + b\n+    a.saturating_add(b)\n }\n@@ -5,3 +5,3 @@\n pub fn mul(a: i32, b: i32) -> i32 {\n-    a * b\n+    a.saturating_mul(b)\n }\n"
        });
        let res =
            crate::tools::registry::fs_tools::dispatch("patch_file", &args, &temp_dir, None, 1)
                .await
                .expect("dispatched")
                .expect("unified_diff patch succeeded");
        assert!(res.contains("Applied 2 unified-diff hunk(s)"));

        let updated = read_file(&temp_dir, "src/math.rs", None, None).unwrap();
        assert!(updated.contains("a.saturating_add(b)"));
        assert!(updated.contains("a.saturating_mul(b)"));

        // Invalid syntax introduced via unified_diff must be rejected by the AST syntax barrier
        let bad_args = serde_json::json!({
            "path": "src/math.rs",
            "unified_diff": "@@ -1,3 +1,3 @@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a.saturating_add(b)\n+    a.saturating_add(b\n }\n"
        });
        let bad_res =
            crate::tools::registry::fs_tools::dispatch("patch_file", &bad_args, &temp_dir, None, 2)
                .await
                .expect("dispatched");
        assert!(
            bad_res.is_err(),
            "Broken Rust syntax via unified_diff must be blocked by AST syntax barrier"
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_read_file_query_ast_bm25_extracts_relevant_function_only() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_bulk_query_single_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        let mut big_rs = String::new();
        for i in 0..40 {
            big_rs.push_str(&format!(
                "pub fn filler_helper_{i}(x: i32) -> i32 {{\n    let a = x + {i};\n    let b = a * 2;\n    let c = b - 1;\n    c\n}}\n\n"
            ));
        }
        big_rs.push_str(
            "pub fn reconcile_kv_cache_watermark(prompt_tokens: usize, limit: usize) -> usize {\n    let ratio = prompt_tokens.saturating_mul(100) / limit.max(1);\n    if ratio > 80 { 0 } else { prompt_tokens }\n}\n",
        );
        write_file(&temp_dir, "src/big.rs", &big_rs).unwrap();

        let out = read_file_with_query(
            &temp_dir,
            "src/big.rs",
            "reconcile kv cache watermark",
            false,
            Some(80),
        )
        .expect("query read succeeds");
        assert!(out.contains("Bulk-Reader"));
        assert!(out.contains("reconcile_kv_cache_watermark"));
        assert!(out.contains("context saved"));
        assert!(
            !out.contains("filler_helper_0"),
            "Irrelevant filler functions should be excluded from query extraction"
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_batch_read_query_ranks_ast_blocks_across_multiple_files() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_bulk_query_batch_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        write_file(
            &temp_dir,
            "src/alpha.rs",
            "pub fn render_button() -> &'static str {\n    \"ok\"\n}\n\npub fn compute_tax(v: u64) -> u64 {\n    v / 10\n}\n",
        )
        .unwrap();
        write_file(
            &temp_dir,
            "src/beta.rs",
            "pub fn verify_oauth_jwt_signature(token: &str, secret: &[u8]) -> bool {\n    !token.is_empty() && !secret.is_empty()\n}\n",
        )
        .unwrap();

        let out = batch_read_with_query(
            &temp_dir,
            &["src/alpha.rs".to_string(), "src/beta.rs".to_string()],
            "oauth jwt signature verification",
            Some(80),
        )
        .expect("batch query read succeeds");
        assert!(out.contains("src/beta.rs"));
        assert!(out.contains("verify_oauth_jwt_signature"));
        assert!(!out.contains("compute_tax"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_replace_in_files_word_boundary_regex_and_dry_run() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_sd_replace_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join("src")).unwrap();

        write_file(
            &temp_dir,
            "src/a.rs",
            "pub fn run() {\n    let foo = 1;\n    let foobar = foo + 2;\n    let _ = foobar;\n}\n",
        )
        .unwrap();
        write_file(
            &temp_dir,
            "src/b.rs",
            "pub fn other() -> &'static str {\n    \"v1_alpha\"\n}\n",
        )
        .unwrap();

        // 1. Word-boundary dry_run: matches `foo` only, leaves `foobar` untouched, does NOT modify disk
        let dry_out = replace_in_files_with_options(
            &temp_dir,
            "src/*.rs",
            "foo",
            "bar",
            ReplaceInFilesOptions {
                dry_run: true,
                word_boundary: true,
                regex_mode: false,
            },
            None,
            1,
        )
        .expect("dry_run word_boundary succeeds");
        assert!(dry_out.contains("DRY RUN"));
        assert!(dry_out.contains("let bar = 1;"));
        assert!(dry_out.contains("let foobar = bar + 2;"));
        // Disk remains untouched
        let on_disk = std::fs::read_to_string(temp_dir.join("src/a.rs")).unwrap();
        assert!(on_disk.contains("let foo = 1;"));

        // 2. Apply word_boundary replacement to disk
        replace_in_files_with_options(
            &temp_dir,
            "src/*.rs",
            "foo",
            "bar",
            ReplaceInFilesOptions {
                dry_run: false,
                word_boundary: true,
                regex_mode: false,
            },
            None,
            2,
        )
        .expect("apply word_boundary succeeds");
        let after_wb = std::fs::read_to_string(temp_dir.join("src/a.rs")).unwrap();
        assert!(after_wb.contains("let bar = 1;"));
        assert!(after_wb.contains("let foobar = bar + 2;"));

        // 3. Opt-in regex_mode with `$1` capture expansion (sd-style)
        replace_in_files_with_options(
            &temp_dir,
            "src/*.rs",
            r"v1_([a-z]+)",
            "v2_$1",
            ReplaceInFilesOptions {
                dry_run: false,
                word_boundary: false,
                regex_mode: true,
            },
            None,
            3,
        )
        .expect("regex capture replace succeeds");
        let after_re = std::fs::read_to_string(temp_dir.join("src/b.rs")).unwrap();
        assert!(after_re.contains("\"v2_alpha\""));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
