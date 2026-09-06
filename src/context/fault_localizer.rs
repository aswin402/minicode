use crate::constants::{
    DEFAULT_FAULT_LOCALIZE_MAX_FILES, FAULT_LOCALIZE_ENVELOPE_MARGIN,
    FAULT_LOCALIZE_MAX_SLICE_LINES, FAULT_LOCALIZE_MAX_SYMBOLS_PER_FILE, MAX_FAULT_LOCALIZE_FILES,
};
use crate::context::graph::CodeGraph;
use crate::context::hybrid::HybridIndex;
use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::error::Result;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    pub hits: Vec<LocalizedFileHit>,
    pub total_files_scanned: usize,
}

impl FaultLocalizationReport {
    /// Formats the report into a clean, token-efficient markdown representation for LLMs
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("### 🎯 Hierarchical Fault Localization Report\n\n");
        out.push_str(&format!("**Query:** \"{}\"\n", self.query));

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

    /// Executes the 3-tier hierarchical fault localization funnel:
    /// 1. Repository-to-File Localization via Multi-Modal Hybrid Index (BM25 + Dense Vectors + PageRank)
    /// 2. File-to-Symbol Localization via Tree-sitter AST and Caller Blast Radius
    /// 3. Symbol-to-Line Slicing generating 1-indexed context envelopes for surgical patch editing
    pub fn localize(
        &self,
        query: &str,
        max_files: Option<usize>,
        include_callers: Option<bool>,
    ) -> Result<FaultLocalizationReport> {
        let max_files = max_files
            .unwrap_or(DEFAULT_FAULT_LOCALIZE_MAX_FILES)
            .clamp(1, MAX_FAULT_LOCALIZE_FILES);
        let include_callers = include_callers.unwrap_or(true);

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

                // Check intersection with search hit lines
                for &(hl_start, hl_end) in hit_lines {
                    if sym.line_number <= hl_end && sym.end_line >= hl_start {
                        score += 8.0;
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
}
