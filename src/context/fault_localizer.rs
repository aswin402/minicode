use crate::constants::{
    DEFAULT_FAULT_LOCALIZE_MAX_FILES, FAULT_LOCALIZATION_CONTEXT_LINES,
    FAULT_LOCALIZE_ENVELOPE_MARGIN, FAULT_LOCALIZE_MAX_SLICE_LINES,
    FAULT_LOCALIZE_MAX_SYMBOLS_PER_FILE, MAX_FAULT_LOCALIZE_FILES,
};
use crate::context::graph::CodeGraph;
use crate::context::hybrid::HybridIndex;
use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::error::Result;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single parsed frame from an error stack trace or compiler diagnostic
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackTraceFrame {
    pub file_path: String,
    pub line_number: usize,
    pub column: Option<usize>,
    pub function_hint: Option<String>,
}

/// A single suspicious AST symbol or code region localized within a candidate file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalizedSymbol {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub start_line: usize,
    pub end_line: usize,
    pub relevance_score: f64,
    pub doc_comment: Option<String>,
    pub direct_callers: Vec<String>,
    pub relevant_tests: Vec<String>,
    pub code_slice: String,
}

/// A candidate file identified during Tier 1 repository search with its localized symbols
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalizedFileHit {
    pub file_path: String,
    pub score: f64,
    pub pagerank: f64,
    pub symbols: Vec<LocalizedSymbol>,
}

/// Complete hierarchical fault localization report emitted by the engine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaultLocalizationReport {
    pub query: String,
    pub stack_frames: Vec<StackTraceFrame>,
    pub hits: Vec<LocalizedFileHit>,
    pub total_files_scanned: usize,
}

impl FaultLocalizationReport {
    /// Formats the report into a clean, token-efficient markdown representation for LLMs
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("### 🎯 Hierarchical Fault Localization Report\n\n");
        out.push_str(&format!("**Query:** \"{}\"\n\n", self.query));

        if !self.stack_frames.is_empty() {
            out.push_str("#### 🚨 Detected Stack Trace / Diagnostic Frames:\n");
            for frame in &self.stack_frames {
                let loc = match frame.column {
                    Some(col) => format!("{}:{}:{}", frame.file_path, frame.line_number, col),
                    None => format!("{}:{}", frame.file_path, frame.line_number),
                };
                if let Some(ref func) = frame.function_hint {
                    out.push_str(&format!("- `{}` (in `{}`)\n", loc, func));
                } else {
                    out.push_str(&format!("- `{}`\n", loc));
                }
            }
            out.push('\n');
        }

        let status = if self.hits.is_empty() {
            "No strong candidate files found"
        } else {
            "High-confidence fault candidate(s) localized"
        };
        out.push_str(&format!(
            "**Candidate Files Analyzed:** {} | **Status:** {}\n\n",
            self.hits.len(),
            status
        ));

        if self.hits.is_empty() {
            out.push_str("No suspicious symbols or candidate files identified. Try broadening the search query or using `grep_search`.\n");
            return out;
        }

        for (idx, hit) in self.hits.iter().enumerate() {
            out.push_str(&format!(
                "#### {}. `{}` (Relevance: {:.2} | PageRank: {:.4})\n\n",
                idx + 1,
                hit.file_path,
                hit.score,
                hit.pagerank
            ));

            if hit.symbols.is_empty() {
                out.push_str("*(No top-level AST symbols detected in candidate region)*\n\n");
            } else {
                for sym in &hit.symbols {
                    out.push_str(&format!(
                        "- **Suspicious Symbol:** `{}` [`{}`] (Lines {}-{})\n",
                        sym.name, sym.kind, sym.start_line, sym.end_line
                    ));
                    if !sym.signature.is_empty() {
                        out.push_str(&format!("  - **Signature:** `{}`\n", sym.signature));
                    }
                    if let Some(ref doc) = sym.doc_comment {
                        let short_doc: String = doc.chars().take(90).collect();
                        out.push_str(&format!("  - **Doc:** *\"{}\"*\n", short_doc));
                    }
                    if !sym.direct_callers.is_empty() {
                        out.push_str(&format!(
                            "  - **Direct Callers:** {}\n",
                            sym.direct_callers.join(", ")
                        ));
                    }
                    if !sym.relevant_tests.is_empty() {
                        out.push_str(&format!(
                            "  - **Relevant Tests:** {}\n",
                            sym.relevant_tests.join(", ")
                        ));
                    }

                    if !sym.code_slice.is_empty() {
                        let lang = match Path::new(&hit.file_path)
                            .extension()
                            .and_then(|e| e.to_str())
                        {
                            Some("rs") => "rust",
                            Some("py") => "python",
                            Some("js") | Some("jsx") => "javascript",
                            Some("ts") | Some("tsx") => "typescript",
                            Some("json") => "json",
                            Some("toml") => "toml",
                            _ => "text",
                        };
                        out.push_str(&format!(
                            "  - **1-Indexed Code Envelope (Lines {}-{}):**\n```{}\n{}```\n\n",
                            sym.start_line, sym.end_line, lang, sym.code_slice
                        ));
                    }
                }
            }
        }

        out.push_str("#### 💡 Prescriptive Next Actions for Agent:\n");
        out.push_str("1. Inspect the 1-indexed code envelopes above to confirm root cause.\n");
        out.push_str("2. Run `synthesize_reproducer` to lock down a failing test before touching source code.\n");
        out.push_str("3. Use `patch_file` targeting the indicated lines with surgical search-and-replace blocks.\n");

        out
    }
}

#[derive(Debug, Default)]
struct FileHitAccumulator {
    total_score: f64,
    max_pagerank: f64,
    hit_lines: Vec<(usize, usize)>,
}

/// 3-Tier Hierarchical Fault Localization Engine
pub struct FaultLocalizer {
    workspace_root: PathBuf,
}

impl FaultLocalizer {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    /// Extracts stack trace and compiler diagnostic frames across Rust, Python, JavaScript, TypeScript, and Go.
    pub fn extract_trace_frames(text: &str) -> Vec<StackTraceFrame> {
        let mut frames = Vec::new();

        // 1. Rust/Cargo compiler diagnostic: "--> src/foo.rs:42:15" or "--> foo.rs:42"
        static COMPILER_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
        let compiler_re = COMPILER_RE.get_or_init(|| {
            regex::Regex::new(r"-->\s+([a-zA-Z0-9_\-\./\\]+\.[a-zA-Z0-9]+):(\d+)(?::(\d+))?").ok()
        });

        // 2. Rust panic: "panicked at '...', src/foo.rs:42:15" or "panicked at src/foo.rs:42" or "at src/foo.rs:42"
        static RUST_PANIC_RE: std::sync::OnceLock<Option<regex::Regex>> =
            std::sync::OnceLock::new();
        let rust_panic_re = RUST_PANIC_RE.get_or_init(|| {
            regex::Regex::new(r"(?:panicked at.*?(?:,\s*|\s+)|(?:^|\s)at\s+)['`\x22]?([a-zA-Z0-9_\-\./\\]+\.rs):(\d+)(?::(\d+))?").ok()
        });

        // 3. Python traceback: File "foo/bar.py", line 42, in my_func
        static PY_TRACE_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
        let py_trace_re = PY_TRACE_RE.get_or_init(|| {
            regex::Regex::new(
                r#"File\s+["']([^"']+\.py)["'],\s+line\s+(\d+)(?:,\s+in\s+([a-zA-Z0-9_<>\.]+))?"#,
            )
            .ok()
        });

        // 4. JS/TS stack trace: at functionName (src/index.ts:15:3) or at src/index.ts:15:3
        static JS_TRACE_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
        let js_trace_re = JS_TRACE_RE.get_or_init(|| {
            regex::Regex::new(r#"at\s+(?:([a-zA-Z0-9_$.<>]+)\s+\()?(?:file://)?([a-zA-Z0-9_\-\./\\]+\.(?:ts|tsx|js|jsx)):(\d+):(\d+)\)?"#).ok()
        });

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(re) = compiler_re {
                if let Some(caps) = re.captures(line) {
                    if let (Some(f), Some(l)) = (caps.get(1), caps.get(2)) {
                        if let Ok(line_num) = l.as_str().parse::<usize>() {
                            let col = caps.get(3).and_then(|c| c.as_str().parse::<usize>().ok());
                            frames.push(StackTraceFrame {
                                file_path: f.as_str().replace('\\', "/"),
                                line_number: line_num,
                                column: col,
                                function_hint: None,
                            });
                            continue;
                        }
                    }
                }
            }

            if let Some(re) = rust_panic_re {
                if let Some(caps) = re.captures(line) {
                    if let (Some(f), Some(l)) = (caps.get(1), caps.get(2)) {
                        if let Ok(line_num) = l.as_str().parse::<usize>() {
                            let col = caps.get(3).and_then(|c| c.as_str().parse::<usize>().ok());
                            frames.push(StackTraceFrame {
                                file_path: f.as_str().replace('\\', "/"),
                                line_number: line_num,
                                column: col,
                                function_hint: None,
                            });
                            continue;
                        }
                    }
                }
            }

            if let Some(re) = py_trace_re {
                if let Some(caps) = re.captures(line) {
                    if let (Some(f), Some(l)) = (caps.get(1), caps.get(2)) {
                        if let Ok(line_num) = l.as_str().parse::<usize>() {
                            let func = caps.get(3).map(|s| s.as_str().to_string());
                            frames.push(StackTraceFrame {
                                file_path: f.as_str().replace('\\', "/"),
                                line_number: line_num,
                                column: None,
                                function_hint: func,
                            });
                            continue;
                        }
                    }
                }
            }

            if let Some(re) = js_trace_re {
                if let Some(caps) = re.captures(line) {
                    if let (Some(f), Some(l)) = (caps.get(2), caps.get(3)) {
                        if let Ok(line_num) = l.as_str().parse::<usize>() {
                            let col = caps.get(4).and_then(|c| c.as_str().parse::<usize>().ok());
                            let func = caps.get(1).map(|s| s.as_str().to_string());
                            frames.push(StackTraceFrame {
                                file_path: f.as_str().replace('\\', "/"),
                                line_number: line_num,
                                column: col,
                                function_hint: func,
                            });
                            continue;
                        }
                    }
                }
            }
        }

        // Deduplicate frames preserving order
        let mut unique = Vec::new();
        for frame in frames {
            if !unique.iter().any(|f: &StackTraceFrame| {
                f.file_path == frame.file_path && f.line_number == frame.line_number
            }) {
                unique.push(frame);
            }
        }

        unique
    }

    /// Executes the 3-tier hierarchical fault localization funnel:
    /// 1. Repository-to-File Localization via Multi-Modal Hybrid Index (BM25 + Dense Vectors + PageRank)
    /// 2. File-to-Symbol Localization via Tree-sitter AST and Caller Blast Radius
    /// 3. Symbol-to-Line Slicing generating 1-indexed context envelopes for surgical patch editing
    pub fn localize(
        &self,
        query: &str,
        max_files: Option<usize>,
        include_callers: Option<bool>,
        candidate_hints: Option<&[String]>,
    ) -> Result<FaultLocalizationReport> {
        let max_files = max_files
            .unwrap_or(DEFAULT_FAULT_LOCALIZE_MAX_FILES)
            .clamp(1, MAX_FAULT_LOCALIZE_FILES);
        let include_callers = include_callers.unwrap_or(true);

        let detected_frames = Self::extract_trace_frames(query);

        let query_tokens: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|w| w.trim().to_lowercase())
            .filter(|w| w.len() >= 3)
            .collect();

        // ---------------------------------------------------------------------
        // Tier 1: Candidate File Retrieval
        // ---------------------------------------------------------------------
        let mut hybrid = HybridIndex::new();
        let _ = hybrid.build_index(&self.workspace_root);

        let hybrid_hits = hybrid.search(query, max_files * 4, true);

        // Group hybrid hits by relative file path
        let mut file_map: HashMap<String, FileHitAccumulator> = HashMap::new();

        for hit in hybrid_hits {
            let rel_path = self.normalize_rel_path(&hit.file_path);
            let entry = file_map.entry(rel_path).or_insert(FileHitAccumulator {
                total_score: 0.0,
                max_pagerank: 0.0,
                hit_lines: Vec::new(),
            });
            entry.total_score += hit.combined_score;
            if hit.pagerank_score > entry.max_pagerank {
                entry.max_pagerank = hit.pagerank_score;
            }
            entry.hit_lines.push((hit.start_line, hit.end_line));
        }

        // Boost files extracted from stack traces
        for frame in &detected_frames {
            let rel = self.normalize_rel_path(&frame.file_path);
            let full = self.workspace_root.join(&rel);
            if full.is_file() {
                let entry = file_map.entry(rel).or_insert(FileHitAccumulator {
                    total_score: 0.0,
                    max_pagerank: 0.1,
                    hit_lines: Vec::new(),
                });
                entry.total_score += 50.0;
                entry.hit_lines.push((frame.line_number, frame.line_number));
            }
        }

        // Direct path and filename hint matching (e.g. "src/session/store.rs" in query)
        for token in &query_tokens {
            if token.ends_with(".rs")
                || token.ends_with(".py")
                || token.ends_with(".ts")
                || token.ends_with(".js")
            {
                let full = self.workspace_root.join(token);
                if full.is_file() {
                    let rel = self.normalize_rel_path(token);
                    file_map
                        .entry(rel)
                        .and_modify(|e| e.total_score += 15.0)
                        .or_insert(FileHitAccumulator {
                            total_score: 15.0,
                            max_pagerank: 0.05,
                            hit_lines: Vec::new(),
                        });
                }
            }
        }

        // Optional candidate hints provided by caller
        if let Some(hints) = candidate_hints {
            for hint in hints {
                let rel = self.normalize_rel_path(hint);
                let full = self.workspace_root.join(&rel);
                if full.is_file() {
                    file_map
                        .entry(rel)
                        .and_modify(|e| e.total_score += 30.0)
                        .or_insert(FileHitAccumulator {
                            total_score: 30.0,
                            max_pagerank: 0.05,
                            hit_lines: Vec::new(),
                        });
                }
            }
        }

        // If file_map is still sparse, perform lexical scan over workspace source files
        if file_map.len() < max_files {
            self.fallback_file_scan(&query_tokens, &mut file_map);
        }

        // Rank candidate files by composite score
        let mut ranked_files: Vec<(String, FileHitAccumulator)> = file_map.into_iter().collect();
        ranked_files.sort_by(|a, b| {
            b.1.total_score
                .partial_cmp(&a.1.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked_files.truncate(max_files);

        // ---------------------------------------------------------------------
        // Tier 2 & 3: Symbol Localization, Call Graph & Slicing
        // ---------------------------------------------------------------------
        let mut repomap = RepoMapExtractor::new();
        let mut code_graph = CodeGraph::new();
        let graph_ready = if include_callers {
            code_graph.build_graph(&self.workspace_root).is_ok()
        } else {
            false
        };

        let mut final_hits = Vec::new();

        for (rel_path, acc) in ranked_files {
            let abs_path = self.workspace_root.join(&rel_path);
            let content = match std::fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let lines: Vec<&str> = content.lines().collect();
            let symbols = repomap.extract_file_symbols(&abs_path).unwrap_or_default();

            let localized_symbols = self.score_and_slice_symbols(
                &symbols,
                &lines,
                &query_tokens,
                &acc.hit_lines,
                if graph_ready { Some(&code_graph) } else { None },
            );

            final_hits.push(LocalizedFileHit {
                file_path: rel_path,
                score: acc.total_score,
                pagerank: acc.max_pagerank,
                symbols: localized_symbols,
            });
        }

        Ok(FaultLocalizationReport {
            query: query.to_string(),
            stack_frames: detected_frames,
            total_files_scanned: final_hits.len(),
            hits: final_hits,
        })
    }

    /// Normalizes path to be clean relative to workspace root
    fn normalize_rel_path(&self, p: &str) -> String {
        let path = Path::new(p);
        if let Ok(rel) = path.strip_prefix(&self.workspace_root) {
            rel.display().to_string()
        } else {
            p.trim_start_matches("./").to_string()
        }
    }

    /// Fallback scan using ignore::WalkBuilder when hybrid search returns few hits
    fn fallback_file_scan(
        &self,
        query_tokens: &[String],
        file_map: &mut HashMap<String, FileHitAccumulator>,
    ) {
        if query_tokens.is_empty() {
            return;
        }

        let walker = WalkBuilder::new(&self.workspace_root)
            .hidden(true)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }

            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(
                ext,
                "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "go" | "c" | "cpp" | "java"
            ) {
                continue;
            }

            let rel_str = self.normalize_rel_path(&path.display().to_string());
            let lower_rel = rel_str.to_lowercase();

            let mut match_count = 0;
            for token in query_tokens {
                if lower_rel.contains(token) {
                    match_count += 1;
                }
            }

            if match_count > 0 {
                let score = (match_count as f64) * 4.0;
                file_map
                    .entry(rel_str)
                    .and_modify(|e| e.total_score += score)
                    .or_insert(FileHitAccumulator {
                        total_score: score,
                        max_pagerank: 0.01,
                        hit_lines: Vec::new(),
                    });
            }
        }
    }

    /// Scores extracted AST symbols and generates 1-indexed code slices with context margins
    fn score_and_slice_symbols(
        &self,
        symbols: &[SymbolDef],
        lines: &[&str],
        query_tokens: &[String],
        hit_lines: &[(usize, usize)],
        graph: Option<&CodeGraph>,
    ) -> Vec<LocalizedSymbol> {
        let total_lines = lines.len();

        // Score symbols
        let mut scored_symbols: Vec<(f64, &SymbolDef)> = symbols
            .iter()
            .map(|sym| {
                let mut score = 0.0;
                let lower_name = sym.name.to_lowercase();
                let lower_sig = sym.signature.to_lowercase();
                let lower_doc = sym
                    .doc_comment
                    .as_ref()
                    .map(|d| d.to_lowercase())
                    .unwrap_or_default();

                for token in query_tokens {
                    if lower_name == *token {
                        score += 15.0;
                    } else if lower_name.contains(token) {
                        score += 6.0;
                    }

                    if lower_sig.contains(token) {
                        score += 3.0;
                    }

                    if lower_doc.contains(token) {
                        score += 2.0;
                    }
                }

                // Check intersection with search hit lines or stack frames
                for &(hl_start, hl_end) in hit_lines {
                    if sym.line_number <= hl_end && sym.end_line >= hl_start {
                        score += 35.0;
                    }
                }

                (score, sym)
            })
            .collect();

        // Sort by score descending
        scored_symbols.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Pick top symbols
        let top_symbols: Vec<(f64, &SymbolDef)> = if scored_symbols.is_empty() {
            Vec::new()
        } else if scored_symbols[0].0 > 0.0 {
            scored_symbols
                .into_iter()
                .filter(|(s, _)| *s > 0.0)
                .take(FAULT_LOCALIZE_MAX_SYMBOLS_PER_FILE)
                .collect()
        } else {
            // If no symbol scored positively, take the first 1-2 symbols from the file
            scored_symbols
                .into_iter()
                .take(FAULT_LOCALIZE_MAX_SYMBOLS_PER_FILE.min(2))
                .collect()
        };

        let mut result = Vec::new();

        for (score, sym) in top_symbols {
            let mut direct_callers = Vec::new();
            let mut relevant_tests = Vec::new();

            if let Some(cg) = graph {
                if let Ok(blast) = cg.get_blast_radius(&sym.name, &self.workspace_root) {
                    direct_callers = blast.direct_caller_symbols.into_iter().take(4).collect();
                    relevant_tests = blast
                        .test_coverage
                        .into_iter()
                        .take(3)
                        .chain(
                            blast
                                .direct_dependents
                                .into_iter()
                                .filter(|d| d.contains("test"))
                                .take(2),
                        )
                        .collect();
                    relevant_tests.sort();
                    relevant_tests.dedup();
                }
            }

            // Generate 1-indexed code slice with envelope
            let start = sym
                .line_number
                .saturating_sub(FAULT_LOCALIZE_ENVELOPE_MARGIN)
                .max(1);
            let end = sym
                .end_line
                .saturating_add(FAULT_LOCALIZE_ENVELOPE_MARGIN)
                .min(total_lines);

            let mut code_slice = String::new();
            let line_count = end.saturating_sub(start) + 1;

            if line_count <= FAULT_LOCALIZE_MAX_SLICE_LINES {
                for line_idx in start..=end {
                    if line_idx <= total_lines {
                        code_slice.push_str(&format!("{}: {}\n", line_idx, lines[line_idx - 1]));
                    }
                }
            } else {
                let head_len = FAULT_LOCALIZE_MAX_SLICE_LINES.saturating_sub(6);
                for line_idx in start..(start + head_len) {
                    if line_idx <= total_lines {
                        code_slice.push_str(&format!("{}: {}\n", line_idx, lines[line_idx - 1]));
                    }
                }
                let omitted = end.saturating_sub(start + head_len);
                code_slice.push_str(&format!("  [... {} lines omitted ...]\n", omitted));
                for line_idx in (end.saturating_sub(4))..=end {
                    if line_idx <= total_lines {
                        code_slice.push_str(&format!("{}: {}\n", line_idx, lines[line_idx - 1]));
                    }
                }
            }

            result.push(LocalizedSymbol {
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                signature: sym.signature.clone(),
                start_line: sym.line_number,
                end_line: sym.end_line,
                relevance_score: score,
                doc_comment: sym.doc_comment.clone(),
                direct_callers,
                relevant_tests,
                code_slice,
            });
        }

        // If file had no AST symbols at all, synthesize a text hit if matching query tokens
        if result.is_empty() && !lines.is_empty() {
            let mut matched_line = None;
            for (i, line) in lines.iter().enumerate() {
                let lower = line.to_lowercase();
                for token in query_tokens {
                    if lower.contains(token) {
                        matched_line = Some(i + 1);
                        break;
                    }
                }
                if matched_line.is_some() {
                    break;
                }
            }

            let center = matched_line.unwrap_or(1);
            let start = center.saturating_sub(3).max(1);
            let end = (center + 5).min(total_lines);

            let mut code_slice = String::new();
            for line_idx in start..=end {
                if line_idx <= total_lines {
                    code_slice.push_str(&format!("{}: {}\n", line_idx, lines[line_idx - 1]));
                }
            }

            result.push(LocalizedSymbol {
                name: "file_scope".to_string(),
                kind: "block".to_string(),
                signature: lines.first().unwrap_or(&"").trim().to_string(),
                start_line: start,
                end_line: end,
                relevance_score: 5.0,
                doc_comment: None,
                direct_callers: Vec::new(),
                relevant_tests: Vec::new(),
                code_slice,
            });
        }

        result
    }

    /// Extracts exact 1-indexed line envelope and raw code content around a symbol or range,
    /// ready to be used as `search_block` in `repair_patch` or `patch_file`.
    #[allow(dead_code)]
    pub fn build_repair_slate(&self, file_path: &str, symbol_name: &str) -> Result<Option<String>> {
        let abs_path = self.workspace_root.join(file_path);
        if !abs_path.is_file() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&abs_path)?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let mut repomap = RepoMapExtractor::new();
        let symbols = repomap.extract_file_symbols(&abs_path).unwrap_or_default();

        let target_symbol = symbols.iter().find(|s| s.name == symbol_name).or_else(|| {
            symbols
                .iter()
                .find(|s| s.name.ends_with(symbol_name) || s.name.contains(symbol_name))
        });

        if let Some(sym) = target_symbol {
            let start = sym
                .line_number
                .saturating_sub(FAULT_LOCALIZATION_CONTEXT_LINES)
                .max(1);
            let end = sym
                .end_line
                .saturating_add(FAULT_LOCALIZATION_CONTEXT_LINES)
                .min(total_lines);

            let mut slate = String::new();
            for i in start..=end {
                if i <= total_lines {
                    slate.push_str(lines[i - 1]);
                    slate.push('\n');
                }
            }
            Ok(Some(slate))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_trace_frames_compiler_diagnostic() {
        let text = "error[E0425]: cannot find function `process_record` in this scope\n  --> src/agent/runner.rs:42:15\n   |\n42 |     let res = process_record(data);";
        let frames = FaultLocalizer::extract_trace_frames(text);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].file_path, "src/agent/runner.rs");
        assert_eq!(frames[0].line_number, 42);
        assert_eq!(frames[0].column, Some(15));
    }

    #[test]
    fn test_extract_trace_frames_rust_panic() {
        let text = "thread 'main' panicked at 'index out of bounds', src/tools/fs.rs:189:9";
        let frames = FaultLocalizer::extract_trace_frames(text);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].file_path, "src/tools/fs.rs");
        assert_eq!(frames[0].line_number, 189);
        assert_eq!(frames[0].column, Some(9));
    }

    #[test]
    fn test_extract_trace_frames_python_traceback() {
        let text = "Traceback (most recent call last):\n  File \"app/server.py\", line 88, in handle_request\n    return run_query()\nTypeError: missing argument";
        let frames = FaultLocalizer::extract_trace_frames(text);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].file_path, "app/server.py");
        assert_eq!(frames[0].line_number, 88);
        assert_eq!(frames[0].function_hint.as_deref(), Some("handle_request"));
    }

    #[test]
    fn test_extract_trace_frames_js_ts_stack() {
        let text = "Error: Connection refused\n    at Database.connect (src/db/client.ts:54:12)\n    at initServer (src/index.ts:19:5)";
        let frames = FaultLocalizer::extract_trace_frames(text);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].file_path, "src/db/client.ts");
        assert_eq!(frames[0].line_number, 54);
        assert_eq!(frames[0].column, Some(12));
        assert_eq!(frames[0].function_hint.as_deref(), Some("Database.connect"));
        assert_eq!(frames[1].file_path, "src/index.ts");
        assert_eq!(frames[1].line_number, 19);
        assert_eq!(frames[1].column, Some(5));
        assert_eq!(frames[1].function_hint.as_deref(), Some("initServer"));
    }
}
