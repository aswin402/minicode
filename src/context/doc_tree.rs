//! Vectorless Hierarchical Document Tree (`PageIndex` / `OpenKB` pattern) in pure Rust.
//!
//! Parses Markdown, MDX, and OKF v0.2 concept documents via `pulldown-cmark` AST offset streams
//! into an exact `[Lstart-Lend]` hierarchical Table-of-Contents tree with per-section token
//! estimates, lead-sentence summaries, OKF trust/freshness metadata, and `[[wikilink]]` / `.md`
//! cross-reference extraction — requiring zero embedding models and zero vector RAM.

use crate::context::okf::{is_stale, trust_tier, OkfDocument};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Single section node in a hierarchical Markdown `PageIndex` outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSectionNode {
    pub level: u8,
    pub title: String,
    pub start_line: usize,
    pub end_line: usize,
    pub own_end_line: usize,
    pub estimated_tokens: usize,
    pub summary: String,
}

/// Complete vectorless structural index of a Markdown / OKF document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownDocTree {
    pub total_lines: usize,
    pub estimated_total_tokens: usize,
    pub okf_header: Option<String>,
    pub sections: Vec<DocSectionNode>,
    pub wikilinks: Vec<String>,
    pub markdown_links: Vec<String>,
}

#[inline]
fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[inline]
fn byte_offset_to_line(newline_offsets: &[usize], offset: usize) -> usize {
    match newline_offsets.binary_search(&offset) {
        Ok(idx) => idx + 1,
        Err(idx) => idx + 1,
    }
}

#[inline]
fn estimate_tokens_fast(text: &str) -> usize {
    text.len().div_ceil(4).max(1)
}

/// Extracts `[[wikilink]]` targets (`[[concept]]`, `[[folder/concept|Label]]`, `[[concept#heading]]`)
/// from plain prose text (outside fenced code blocks and inline code spans).
pub fn extract_wikilinks_from_prose(text: &str, out: &mut BTreeSet<String>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        // Ignore image embeds `![[...]]`
        if bytes[i] == b'[' && bytes[i + 1] == b'[' && (i == 0 || bytes[i - 1] != b'!') {
            let start = i + 2;
            let mut end = None;
            let mut cursor = start;
            while cursor + 1 < bytes.len() {
                if bytes[cursor] == b'\n' || bytes[cursor] == b'[' {
                    break;
                }
                if bytes[cursor] == b']' && bytes[cursor + 1] == b']' {
                    end = Some(cursor);
                    break;
                }
                cursor += 1;
            }
            if let Some(close_pos) = end {
                if let Ok(raw_inner) = std::str::from_utf8(&bytes[start..close_pos]) {
                    let before_alias = raw_inner.split('|').next().unwrap_or("").trim();
                    let before_anchor = before_alias.split('#').next().unwrap_or("").trim();
                    let cleaned = before_anchor.trim_end_matches(".md").trim();
                    if !cleaned.is_empty() && !cleaned.contains("://") {
                        out.insert(cleaned.replace('\\', "/"));
                    }
                }
                i = close_pos + 2;
                continue;
            }
        }
        i += 1;
    }
}

fn truncate_summary_words(raw: &str, max_chars: usize) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut acc = String::new();
    for word in collapsed.split_whitespace() {
        let next_len = if acc.is_empty() {
            word.chars().count()
        } else {
            acc.chars().count() + 1 + word.chars().count()
        };
        if next_len > max_chars {
            break;
        }
        if !acc.is_empty() {
            acc.push(' ');
        }
        acc.push_str(word);
    }
    if acc.is_empty() {
        collapsed.chars().take(max_chars).collect()
    } else {
        format!("{acc}...")
    }
}

/// Parses a Markdown/MDX/OKF document into a vectorless `PageIndex` hierarchical outline.
pub fn parse_markdown_doc_tree(content: &str) -> MarkdownDocTree {
    let total_lines = content.lines().count().max(1);
    let estimated_total_tokens = estimate_tokens_fast(content);

    let mut okf_header = None;
    let mut body_slice = content;
    let mut body_byte_offset = 0usize;

    if let Ok((Some(fm), parsed_body)) = OkfDocument::parse(content) {
        let tier = trust_tier(&fm);
        let stale_flag = if is_stale(&fm, None) {
            "STALE"
        } else {
            "fresh"
        };
        let status = fm.status.as_deref().unwrap_or("stable");
        okf_header = Some(format!(
            "type=`{}` • status=`{}` • trust=`{}` • freshness=`{}`",
            fm.concept_type, status, tier, stale_flag
        ));
        body_slice = parsed_body;
        body_byte_offset = content.len().saturating_sub(parsed_body.len());
    }

    let newline_offsets: Vec<usize> = content
        .bytes()
        .enumerate()
        .filter_map(|(idx, b)| (b == b'\n').then_some(idx))
        .collect();

    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(body_slice, opts).into_offset_iter();

    struct RawHeading {
        level: u8,
        title: String,
        start_byte: usize,
        start_line: usize,
        lead_prose: String,
    }

    let mut raw_headings: Vec<RawHeading> = Vec::new();
    let mut wikilinks_set = BTreeSet::new();
    let mut md_links_set = BTreeSet::new();
    let mut prose_buf = String::with_capacity(body_slice.len().min(8192));

    let mut in_heading: Option<(u8, usize, String)> = None;
    let mut in_code_block = false;

    for (event, range) in parser {
        let abs_start = body_byte_offset + range.start;
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                prose_buf.push('\n');
                in_heading = Some((heading_level_to_u8(level), abs_start, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                prose_buf.push('\n');
                if let Some((lvl, start_b, title_buf)) = in_heading.take() {
                    let trimmed = title_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !trimmed.is_empty() {
                        let start_line = byte_offset_to_line(&newline_offsets, start_b);
                        raw_headings.push(RawHeading {
                            level: lvl,
                            title: trimmed,
                            start_byte: start_b,
                            start_line,
                            lead_prose: String::new(),
                        });
                    }
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                prose_buf.push('\n');
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                prose_buf.push('\n');
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let target = dest_url.split('#').next().unwrap_or("").trim();
                if !target.is_empty() && !target.contains("://") && !target.starts_with("mailto:") {
                    md_links_set.insert(target.to_string());
                }
            }
            Event::Text(text) => {
                if let Some((_, _, ref mut h_buf)) = in_heading {
                    h_buf.push_str(&text);
                    prose_buf.push_str(&text);
                } else if !in_code_block {
                    prose_buf.push_str(&text);
                    if let Some(last_h) = raw_headings.last_mut() {
                        if last_h.lead_prose.len() < 180 {
                            last_h.lead_prose.push_str(&text);
                        }
                    }
                }
            }
            Event::Code(inline_code) => {
                prose_buf.push(' ');
                if let Some((_, _, ref mut h_buf)) = in_heading {
                    h_buf.push('`');
                    h_buf.push_str(&inline_code);
                    h_buf.push('`');
                } else if !in_code_block {
                    if let Some(last_h) = raw_headings.last_mut() {
                        if last_h.lead_prose.len() < 160 {
                            last_h.lead_prose.push('`');
                            last_h.lead_prose.push_str(&inline_code);
                            last_h.lead_prose.push('`');
                        }
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                prose_buf.push(' ');
                if let Some((_, _, ref mut h_buf)) = in_heading {
                    h_buf.push(' ');
                } else if !in_code_block {
                    if let Some(last_h) = raw_headings.last_mut() {
                        if !last_h.lead_prose.is_empty() && last_h.lead_prose.len() < 180 {
                            last_h.lead_prose.push(' ');
                        }
                    }
                }
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Item | TagEnd::BlockQuote(_) | TagEnd::TableCell,
            ) => {
                prose_buf.push('\n');
                if !in_code_block {
                    if let Some(last_h) = raw_headings.last_mut() {
                        if !last_h.lead_prose.is_empty()
                            && !last_h.lead_prose.ends_with(' ')
                            && last_h.lead_prose.len() < 180
                        {
                            last_h.lead_prose.push(' ');
                        }
                    }
                }
            }
            _ => {}
        }
    }

    extract_wikilinks_from_prose(&prose_buf, &mut wikilinks_set);

    let mut sections = Vec::with_capacity(raw_headings.len());
    for (idx, h) in raw_headings.iter().enumerate() {
        let own_end_byte = raw_headings
            .get(idx + 1)
            .map(|next| next.start_byte)
            .unwrap_or(content.len());
        let own_end_line = if idx + 1 < raw_headings.len() {
            raw_headings[idx + 1]
                .start_line
                .saturating_sub(1)
                .max(h.start_line)
        } else {
            total_lines
        };

        // Hierarchical subtree span: extends until next heading with `level <= h.level`
        let subtree_next_idx = raw_headings[idx + 1..]
            .iter()
            .position(|later| later.level <= h.level)
            .map(|rel| idx + 1 + rel);

        let (end_byte, end_line) = match subtree_next_idx {
            Some(next_i) => (
                raw_headings[next_i].start_byte,
                raw_headings[next_i]
                    .start_line
                    .saturating_sub(1)
                    .max(h.start_line),
            ),
            None => (content.len(), total_lines),
        };

        let slice_end = end_byte.min(content.len()).max(h.start_byte);
        let section_bytes = content.get(h.start_byte..slice_end).unwrap_or("");
        let estimated_tokens = estimate_tokens_fast(section_bytes);
        let _ = own_end_byte;

        sections.push(DocSectionNode {
            level: h.level,
            title: h.title.clone(),
            start_line: h.start_line,
            end_line,
            own_end_line,
            estimated_tokens,
            summary: truncate_summary_words(&h.lead_prose, 96),
        });
    }

    MarkdownDocTree {
        total_lines,
        estimated_total_tokens,
        okf_header,
        sections,
        wikilinks: wikilinks_set.into_iter().collect(),
        markdown_links: md_links_set.into_iter().collect(),
    }
}

/// Formats a `MarkdownDocTree` into an LLM-optimized `PageIndex` outline for `look_at` / `view_outline`.
pub fn render_markdown_page_index(relative_path: &str, content: &str) -> String {
    let size_kb = content.len() as f64 / 1024.0;
    let tree = parse_markdown_doc_tree(content);

    let mut out = format!(
        "# `{}` ({} lines, {:.1} KB, ~{} tok — PageIndex document tree)\n",
        relative_path, tree.total_lines, size_kb, tree.estimated_total_tokens
    );

    if let Some(ref okf) = tree.okf_header {
        out.push_str(&format!("• OKF Metadata: {}\n", okf));
    }

    out.push('\n');
    out.push_str(&format!(
        "## Hierarchical Section Tree ({})\n",
        tree.sections.len()
    ));

    if tree.sections.is_empty() {
        out.push_str("  *(no markdown headings found)*\n");
    } else {
        let min_level = tree.sections.iter().map(|s| s.level).min().unwrap_or(1);
        for sec in tree.sections.iter().take(100) {
            let depth = sec.level.saturating_sub(min_level) as usize;
            let indent = "  ".repeat(depth + 1);
            let hashes = "#".repeat(sec.level as usize);
            if sec.summary.is_empty() {
                out.push_str(&format!(
                    "{}[L{:>4}-L{:<4} | ~{:>4} tok] {} {}\n",
                    indent, sec.start_line, sec.end_line, sec.estimated_tokens, hashes, sec.title
                ));
            } else {
                out.push_str(&format!(
                    "{}[L{:>4}-L{:<4} | ~{:>4} tok] {} {} — {}\n",
                    indent,
                    sec.start_line,
                    sec.end_line,
                    sec.estimated_tokens,
                    hashes,
                    sec.title,
                    sec.summary
                ));
            }
        }
        if tree.sections.len() > 100 {
            out.push_str(&format!(
                "  *(... {} additional sections omitted)*\n",
                tree.sections.len() - 100
            ));
        }
    }

    if !tree.wikilinks.is_empty() || !tree.markdown_links.is_empty() {
        out.push_str("\n## Cross-References & Backlink Targets\n");
        if !tree.wikilinks.is_empty() {
            let preview: Vec<String> = tree
                .wikilinks
                .iter()
                .take(25)
                .map(|w| format!("`[[{}]]`", w))
                .collect();
            out.push_str(&format!("  • Wikilinks: {}\n", preview.join(", ")));
        }
        if !tree.markdown_links.is_empty() {
            let preview: Vec<String> = tree
                .markdown_links
                .iter()
                .take(25)
                .map(|m| format!("`{}`", m))
                .collect();
            out.push_str(&format!("  • Document Links: {}\n", preview.join(", ")));
        }
    }

    out.push_str(&format!(
        "\n*Use `look_at(path: \"{}\", section: \"Heading\")` or `read_file(path: \"{}\", start_line: X, end_line: Y)` to drill into exact sections.*",
        relative_path, relative_path
    ));
    out
}

fn collect_markdown_files(target_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let walker = ignore::WalkBuilder::new(target_dir)
        .hidden(false)
        .git_ignore(true)
        .max_depth(Some(6))
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some(".git" | "target" | "node_modules")
            )
        }) {
            continue;
        }
        let is_md = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                matches!(ext.to_ascii_lowercase().as_str(), "md" | "mdx" | "markdown")
            });
        if is_md {
            files.push(path.to_path_buf());
        }
    }
    files.sort();
    files
}

fn build_section_breadcrumbs(sections: &[DocSectionNode]) -> Vec<String> {
    let mut stack: Vec<(u8, &str)> = Vec::new();
    let mut crumbs = Vec::with_capacity(sections.len());
    for sec in sections {
        while stack.last().is_some_and(|&(lvl, _)| lvl >= sec.level) {
            stack.pop();
        }
        stack.push((sec.level, sec.title.as_str()));
        let joined = stack
            .iter()
            .map(|&(_, t)| t)
            .collect::<Vec<_>>()
            .join(" > ");
        crumbs.push(joined);
    }
    crumbs
}

/// Renders a workspace/bundle-wide multi-document `PageIndex` forest across all Markdown/OKF files in `target_dir`.
pub fn render_directory_page_index(
    target_dir: &std::path::Path,
    display_dir: &str,
) -> Option<String> {
    if !target_dir.is_dir() {
        return None;
    }
    let md_files = collect_markdown_files(target_dir);
    if md_files.is_empty() {
        return None;
    }

    let mut total_tokens = 0usize;
    let mut parsed_docs = Vec::new();
    for file_path in md_files.iter().take(80) {
        let Ok(content) = std::fs::read_to_string(file_path) else {
            continue;
        };
        let rel = file_path
            .strip_prefix(target_dir)
            .unwrap_or(file_path)
            .to_string_lossy()
            .replace('\\', "/");
        let tree = parse_markdown_doc_tree(&content);
        total_tokens += tree.estimated_total_tokens;
        parsed_docs.push((rel, tree));
    }

    if parsed_docs.is_empty() {
        return None;
    }

    let label = if display_dir.trim().is_empty() {
        "."
    } else {
        display_dir.trim_end_matches('/')
    };
    let mut out = format!(
        "# `{}` ({} Markdown docs, ~{} tok total — Multi-Doc PageIndex Navigator)\n\n",
        label,
        parsed_docs.len(),
        total_tokens
    );

    for (rel, tree) in &parsed_docs {
        let full_rel = if label == "." {
            rel.clone()
        } else {
            format!("{label}/{rel}")
        };
        out.push_str(&format!(
            "## `{}` ({} lines, ~{} tok)\n",
            full_rel, tree.total_lines, tree.estimated_total_tokens
        ));
        if let Some(ref okf) = tree.okf_header {
            out.push_str(&format!("  • OKF: {okf}\n"));
        }
        if tree.sections.is_empty() {
            out.push_str("  *(no headings)*\n");
        } else {
            let min_lvl = tree.sections.iter().map(|s| s.level).min().unwrap_or(1);
            for sec in tree.sections.iter().take(12) {
                let depth = sec.level.saturating_sub(min_lvl) as usize;
                let indent = "  ".repeat(depth + 1);
                let hashes = "#".repeat(sec.level as usize);
                if sec.summary.is_empty() {
                    out.push_str(&format!(
                        "{}[L{:>4}-L{:<4} | ~{:>4} tok] {} {}\n",
                        indent,
                        sec.start_line,
                        sec.end_line,
                        sec.estimated_tokens,
                        hashes,
                        sec.title
                    ));
                } else {
                    out.push_str(&format!(
                        "{}[L{:>4}-L{:<4} | ~{:>4} tok] {} {} — {}\n",
                        indent,
                        sec.start_line,
                        sec.end_line,
                        sec.estimated_tokens,
                        hashes,
                        sec.title,
                        sec.summary
                    ));
                }
            }
            if tree.sections.len() > 12 {
                out.push_str(&format!(
                    "  *(... {} more sections)*\n",
                    tree.sections.len() - 12
                ));
            }
        }
        if !tree.wikilinks.is_empty() {
            let links: Vec<String> = tree
                .wikilinks
                .iter()
                .take(8)
                .map(|w| format!("`[[{w}]]`"))
                .collect();
            out.push_str(&format!("  • Wikilinks: {}\n", links.join(", ")));
        }
        out.push('\n');
    }

    if md_files.len() > 80 {
        out.push_str(&format!(
            "*(... {} additional Markdown files omitted)*\n\n",
            md_files.len() - 80
        ));
    }

    out.push_str(&format!(
        "*Use `look_at(path: \"{label}\", section: \"file#Heading\")` to drill directly into any section verbatim.*"
    ));
    Some(out)
}

/// Vectorless `PageIndex` section drilldown (`OpenKB` pattern): resolves `section_query`
/// (e.g. `"Stage 1"`, `"Bounded Heap"`, or `"search#FastScan"`) against a single Markdown file
/// or an entire Markdown directory and extracts the exact `[Lstart-Lend]` lines verbatim.
pub fn drilldown_markdown_section(
    target_path: &std::path::Path,
    display_path: &str,
    section_query: &str,
) -> Option<String> {
    use std::path::Path;
    let query_trimmed = section_query.trim();
    if query_trimmed.is_empty() {
        return None;
    }

    let (file_hint, heading_query) = match query_trimmed.split_once('#') {
        Some((f, h)) if !h.trim().is_empty() => (
            Some(f.trim().trim_end_matches(".md").to_ascii_lowercase()),
            h.trim(),
        ),
        _ => (None, query_trimmed),
    };

    let candidate_files: Vec<(String, PathBuf)> =
        match (target_path.is_file(), target_path.is_dir()) {
            (true, _) => vec![(display_path.to_string(), target_path.to_path_buf())],
            loaded if loaded.1 => {
                let base_label = display_path.trim_end_matches('/');
                collect_markdown_files(target_path)
                    .into_iter()
                    .map(|p| {
                        let rel = p
                            .strip_prefix(target_path)
                            .unwrap_or(Path::new(""))
                            .to_string_lossy()
                            .replace('\\', "/");
                        let disp = if base_label.is_empty() || base_label == "." {
                            rel
                        } else {
                            format!("{base_label}/{rel}")
                        };
                        (disp, p)
                    })
                    .collect()
            }
            _ => return None,
        };

    struct CandidateSection {
        file_idx: usize,
        title: String,
        breadcrumb: String,
        summary: String,
        start_line: usize,
        end_line: usize,
        estimated_tokens: usize,
    }

    let mut loaded_files: Vec<(String, String)> = Vec::new();
    let mut candidates: Vec<CandidateSection> = Vec::new();

    for (disp_file, path_buf) in candidate_files {
        if let Some(ref f_hint) = file_hint {
            let disp_no_ext = disp_file.trim_end_matches(".md").to_ascii_lowercase();
            let file_dice =
                crate::context::search::lexical::sorensen_dice_bigrams(f_hint, &disp_no_ext);
            if !disp_no_ext.contains(f_hint.as_str()) && file_dice < 0.6 {
                continue;
            }
        }
        let content = match std::fs::read(&path_buf) {
            Ok(raw_bytes) => match crate::context::doc_convert::convert_workspace_file_cached(
                &path_buf,
                &disp_file,
                &raw_bytes,
                crate::context::doc_convert::ConversionPolicy::StructuralOutline,
            ) {
                Ok(Some(doc)) => doc.markdown,
                _ => match String::from_utf8(raw_bytes) {
                    Ok(text) => text,
                    Err(_) => continue,
                },
            },
            Err(_) => continue,
        };
        let tree = parse_markdown_doc_tree(&content);
        if tree.sections.is_empty() {
            continue;
        }
        let file_idx = loaded_files.len();
        loaded_files.push((disp_file, content));
        let breadcrumbs = build_section_breadcrumbs(&tree.sections);
        for (sec, crumb) in tree.sections.into_iter().zip(breadcrumbs) {
            candidates.push(CandidateSection {
                file_idx,
                title: sec.title,
                breadcrumb: crumb,
                summary: sec.summary,
                start_line: sec.start_line,
                end_line: sec.end_line,
                estimated_tokens: sec.estimated_tokens,
            });
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Multi-field BM25F + Sørensen–Dice character-bigram scoring (zero arbitrary if/else score tiers)
    let field_storage: Vec<[(&str, f32); 3]> = candidates
        .iter()
        .map(|c| {
            [
                (c.title.as_str(), 3.0),
                (c.breadcrumb.as_str(), 1.8),
                (c.summary.as_str(), 1.0),
            ]
        })
        .collect();
    let bm25_docs: Vec<crate::context::search::lexical::Bm25FieldDoc<'_>> = field_storage
        .iter()
        .map(|fields| crate::context::search::lexical::Bm25FieldDoc { fields })
        .collect();
    let bm25_scores = crate::context::search::lexical::score_bm25f_batch(heading_query, &bm25_docs);

    let best_idx = candidates
        .iter()
        .enumerate()
        .filter_map(|(idx, cand)| {
            let bm25 = bm25_scores[idx];
            let title_dice =
                crate::context::search::lexical::sorensen_dice_bigrams(heading_query, &cand.title);
            let crumb_dice = crate::context::search::lexical::sorensen_dice_bigrams(
                heading_query,
                &cand.breadcrumb,
            );
            let dice = title_dice.max(crumb_dice);
            (bm25 > 0.0 || dice >= 0.45).then_some((idx, bm25 + 2.0 * dice))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(idx, _)| idx)?;

    let hit = &candidates[best_idx];
    let (ref hit_display_file, ref hit_content) = loaded_files[hit.file_idx];
    let all_lines: Vec<&str> = hit_content.lines().collect();
    let start_idx = hit.start_line.saturating_sub(1).min(all_lines.len());
    let end_idx = hit.end_line.min(all_lines.len()).max(start_idx);
    let max_drilldown_lines = 250usize;
    let shown_end_idx = end_idx.min(start_idx + max_drilldown_lines);

    let mut out = format!(
        "# PageIndex Drilldown: `{}` [L{}-L{} | ~{} tok]\n• Breadcrumb: `{}`\n\n",
        hit_display_file, hit.start_line, hit.end_line, hit.estimated_tokens, hit.breadcrumb
    );
    for (offset, line) in all_lines[start_idx..shown_end_idx].iter().enumerate() {
        out.push_str(&format!("{:>4} | {}\n", start_idx + offset + 1, line));
    }
    if end_idx > shown_end_idx {
        out.push_str(&format!(
            "\n[... Showing L{}-L{} of section L{}-L{}. Use `read_file(path: \"{}\", start_line: {}, end_line: {})` for remaining lines ...]",
            hit.start_line,
            shown_end_idx,
            hit.start_line,
            hit.end_line,
            hit_display_file,
            shown_end_idx + 1,
            hit.end_line
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_index_doc_tree_ignores_code_fences_and_computes_hierarchical_spans() {
        let doc = "\
---
type: Architecture
title: Search Pipeline
status: stable
---
# Search Architecture
High-level overview of the retrieval stack linking [[concepts/quantization|2-Bit Quantization]] and [OKF](./okf.md).

```bash
# Not a markdown heading!
cargo test --release
[[ignored/inside_code_fence]]
```

## Stage 1: Binary Screening
Fast hardware popcount screening over sign bit-planes.

### Bounded Heap
Maintains top-256 candidates with zero heap churn.

## Stage 2: FastScan LUT
32 KiB L1 lookup table scoring.
";

        let tree = parse_markdown_doc_tree(doc);
        assert!(tree
            .okf_header
            .as_deref()
            .unwrap_or("")
            .contains("Architecture"));
        assert_eq!(tree.sections.len(), 4);

        assert_eq!(tree.sections[0].title, "Search Architecture");
        assert_eq!(tree.sections[0].level, 1);
        assert_eq!(tree.sections[0].start_line, 6);
        assert_eq!(tree.sections[0].end_line, tree.total_lines);

        assert_eq!(tree.sections[1].title, "Stage 1: Binary Screening");
        assert_eq!(tree.sections[1].level, 2);
        // Stage 1 H2 subtree includes its H3 child ("Bounded Heap") up to right before Stage 2 H2!
        assert!(tree.sections[1].end_line >= tree.sections[2].end_line);
        assert!(tree.sections[1].end_line < tree.sections[3].start_line);

        assert_eq!(tree.sections[2].title, "Bounded Heap");
        assert_eq!(tree.sections[2].level, 3);

        assert_eq!(tree.sections[3].title, "Stage 2: FastScan LUT");
        assert_eq!(tree.sections[3].level, 2);

        // Code-fence `# Not a markdown heading!` and `[[ignored/inside_code_fence]]` must NOT appear
        assert_eq!(tree.wikilinks, vec!["concepts/quantization".to_string()]);
        assert_eq!(tree.markdown_links, vec!["./okf.md".to_string()]);

        let rendered = render_markdown_page_index("docs/search.md", doc);
        assert!(rendered.contains("PageIndex document tree"));
        assert!(rendered.contains("`[[concepts/quantization]]`"));
    }

    #[test]
    fn test_multi_doc_directory_page_index_and_section_drilldown() {
        let dir = std::env::temp_dir().join(format!(
            "minicode_page_index_dir_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::create_dir_all(dir.join("concepts"));

        std::fs::write(
            dir.join("concepts/storage.md"),
            "# Storage Engine\nIntro.\n\n## Tombstone Persistence\nPersists `.tomb` bitmaps in O(1).\n\n## Binary IVF\nPartitions 128-bit centroids.\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("concepts/cache.md"),
            "# KV Cache\nOverview.\n\n## Slot Affinity\nRoutes session prefix to warm worker.\n",
        )
        .unwrap();

        let forest = render_directory_page_index(&dir, "wiki").expect("forest missing");
        assert!(forest.contains("Multi-Doc PageIndex Navigator"));
        assert!(forest.contains("wiki/concepts/storage.md"));
        assert!(forest.contains("Tombstone Persistence"));

        let drill = drilldown_markdown_section(&dir, "wiki", "storage#Tombstone Persistence")
            .expect("drilldown missing");
        assert!(drill.contains("PageIndex Drilldown: `wiki/concepts/storage.md`"));
        assert!(drill.contains("Storage Engine > Tombstone Persistence"));
        assert!(drill.contains("Persists `.tomb` bitmaps in O(1)."));
        assert!(!drill.contains("Partitions 128-bit centroids."));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
