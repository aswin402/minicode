use crate::constants::{RRF_K, RRF_WEIGHT_LEXICAL, RRF_WEIGHT_PAGERANK, RRF_WEIGHT_VECTOR};
use crate::context::graph::CodeGraph;
use crate::context::index::SymbolIndex;
use crate::context::semantic::SemanticIndex;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A ranked search hit produced by fusing lexical BM25, dense semantic vectors, and graph PageRank.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HybridHit {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub combined_score: f64,
    pub lexical_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub pagerank_score: f64,
    pub match_sources: Vec<String>,
}

/// Unified multi-modal search engine combining BM25 lexical matching,
/// dense subword embedding vectors, and CodeGraph PageRank centrality with Reciprocal Rank Fusion (RRF).
pub struct HybridIndex {
    symbol_index: SymbolIndex,
    semantic_index: SemanticIndex,
    pagerank_map: HashMap<String, f64>,
}

impl Default for HybridIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl HybridIndex {
    pub fn new() -> Self {
        Self {
            symbol_index: SymbolIndex::new(),
            semantic_index: SemanticIndex::new(),
            pagerank_map: HashMap::new(),
        }
    }

    /// Builds the full multi-modal index (BM25 inverted index, vector semantic index, PageRank graph)
    pub fn build_index(&mut self, workspace_root: &Path) -> Result<()> {
        self.symbol_index.build_index(workspace_root)?;
        self.semantic_index.build_index(workspace_root)?;

        let mut graph = CodeGraph::new();
        if graph.build_graph(workspace_root).is_ok() {
            let symbol_pr = graph.compute_symbol_pagerank(&[]);
            for (sym, score) in symbol_pr {
                self.pagerank_map.insert(sym.name, score);
                self.pagerank_map.insert(sym.qualified_name, score);
            }

            let file_pr = graph.compute_pagerank(&[]);
            for (path, score) in file_pr {
                let rel = path
                    .strip_prefix(workspace_root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                self.pagerank_map.insert(rel, score);
            }
        }

        Ok(())
    }

    /// Executes a hybrid search combining BM25, semantic vectors, and PageRank with RRF fusion.
    pub fn search(&self, query: &str, limit: usize, include_symbols: bool) -> Vec<HybridHit> {
        let pool_size = limit.max(10) * 3;

        // 1. Retrieve Lexical BM25 Matches
        let lexical_matches = self.symbol_index.search_symbols(query, pool_size);

        // 2. Retrieve Dense Vector Semantic Matches
        let vector_matches = self.semantic_index.search(query, pool_size);

        // 3. Optional Symbol Vector Matches
        let symbol_vector_matches = if include_symbols {
            self.semantic_index.search_symbols(query, pool_size)
        } else {
            Vec::new()
        };

        // Key: (file_path, start_line, end_line)
        struct Candidate {
            file_path: String,
            start_line: usize,
            end_line: usize,
            snippet: String,
            symbol_name: Option<String>,
            symbol_kind: Option<String>,
            lexical_rank: Option<usize>,
            vector_rank: Option<usize>,
            pagerank_score: f64,
        }

        let mut candidate_map: HashMap<(String, usize, usize), Candidate> = HashMap::new();

        // Index Lexical Hits
        for (rank, lex) in lexical_matches.into_iter().enumerate() {
            let key = (
                lex.file_path.display().to_string(),
                lex.line_number,
                lex.line_number + 10,
            );
            let pr = self
                .pagerank_map
                .get(&lex.name)
                .or_else(|| self.pagerank_map.get(&key.0))
                .copied()
                .unwrap_or(0.0);

            candidate_map
                .entry(key.clone())
                .and_modify(|c| {
                    if c.lexical_rank.is_none() {
                        c.lexical_rank = Some(rank + 1);
                    }
                })
                .or_insert_with(|| Candidate {
                    file_path: key.0.clone(),
                    start_line: lex.line_number,
                    end_line: lex.line_number + 10,
                    snippet: lex.signature.clone(),
                    symbol_name: Some(lex.name.clone()),
                    symbol_kind: Some(lex.kind.clone()),
                    lexical_rank: Some(rank + 1),
                    vector_rank: None,
                    pagerank_score: pr,
                });
        }

        // Index Vector Chunk Hits
        for (rank, vec_hit) in vector_matches.into_iter().enumerate() {
            let key = (
                vec_hit.file_path.clone(),
                vec_hit.start_line,
                vec_hit.end_line,
            );
            let pr = vec_hit
                .symbol_name
                .as_ref()
                .and_then(|name| self.pagerank_map.get(name))
                .or_else(|| self.pagerank_map.get(&key.0))
                .copied()
                .unwrap_or(0.0);

            candidate_map
                .entry(key.clone())
                .and_modify(|c| {
                    if c.vector_rank.is_none() {
                        c.vector_rank = Some(rank + 1);
                    }
                    if c.snippet.is_empty() {
                        c.snippet = vec_hit.snippet.clone();
                    }
                })
                .or_insert_with(|| Candidate {
                    file_path: key.0.clone(),
                    start_line: vec_hit.start_line,
                    end_line: vec_hit.end_line,
                    snippet: vec_hit.snippet.clone(),
                    symbol_name: vec_hit.symbol_name.clone(),
                    symbol_kind: vec_hit.symbol_kind.clone(),
                    lexical_rank: None,
                    vector_rank: Some(rank + 1),
                    pagerank_score: pr,
                });
        }

        // Index Symbol Vector Hits
        for (rank, sym_hit) in symbol_vector_matches.into_iter().enumerate() {
            let key = (
                sym_hit.file_path.clone(),
                sym_hit.start_line,
                sym_hit.end_line,
            );
            let pr = sym_hit
                .symbol_name
                .as_ref()
                .and_then(|name| self.pagerank_map.get(name))
                .or_else(|| self.pagerank_map.get(&key.0))
                .copied()
                .unwrap_or(0.0);

            candidate_map
                .entry(key.clone())
                .and_modify(|c| {
                    if c.vector_rank.is_none() || c.vector_rank.unwrap_or(usize::MAX) > rank + 1 {
                        c.vector_rank = Some(rank + 1);
                    }
                })
                .or_insert_with(|| Candidate {
                    file_path: key.0.clone(),
                    start_line: sym_hit.start_line,
                    end_line: sym_hit.end_line,
                    snippet: sym_hit.snippet.clone(),
                    symbol_name: sym_hit.symbol_name.clone(),
                    symbol_kind: sym_hit.symbol_kind.clone(),
                    lexical_rank: None,
                    vector_rank: Some(rank + 1),
                    pagerank_score: pr,
                });
        }

        // 4. Compute RRF Scores
        let mut results: Vec<HybridHit> = candidate_map
            .into_values()
            .map(|c| {
                let mut rrf_score = 0.0;
                let mut sources = Vec::new();

                if let Some(l_rank) = c.lexical_rank {
                    rrf_score += RRF_WEIGHT_LEXICAL / (RRF_K + l_rank as f64);
                    sources.push(format!("BM25 #{}", l_rank));
                }

                if let Some(v_rank) = c.vector_rank {
                    rrf_score += RRF_WEIGHT_VECTOR / (RRF_K + v_rank as f64);
                    sources.push(format!("Vector #{}", v_rank));
                }

                if c.pagerank_score > 0.001 {
                    rrf_score += RRF_WEIGHT_PAGERANK * c.pagerank_score;
                    sources.push(format!("PageRank {:.3}", c.pagerank_score));
                }

                HybridHit {
                    file_path: c.file_path,
                    start_line: c.start_line,
                    end_line: c.end_line,
                    snippet: c.snippet,
                    symbol_name: c.symbol_name,
                    symbol_kind: c.symbol_kind,
                    combined_score: rrf_score,
                    lexical_rank: c.lexical_rank,
                    vector_rank: c.vector_rank,
                    pagerank_score: c.pagerank_score,
                    match_sources: sources,
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit);
        results
    }

    /// Formats the hybrid search results into clean markdown for agent / TUI presentation
    pub fn format_markdown(query: &str, hits: &[HybridHit]) -> String {
        if hits.is_empty() {
            return format!(
                "### 🔍 Hybrid Retrieval: \"{}\"\n\nNo code snippets or symbols matched the query.\n",
                query
            );
        }

        let mut out = format!(
            "### 🔍 Hybrid Retrieval (BM25 + Vector + PageRank): \"{}\" ({} hits)\n\n",
            query,
            hits.len()
        );

        for (i, hit) in hits.iter().enumerate() {
            let symbol_badge = if let Some(ref name) = hit.symbol_name {
                let kind = hit.symbol_kind.as_deref().unwrap_or("symbol");
                format!(" • `{}` **`{}`**", kind, name)
            } else {
                String::new()
            };

            let sources_badge = hit.match_sources.join(" | ");

            out.push_str(&format!(
                "{}. **`{}:{}-{}`**{}\n   - **Score:** `{:.4}` `[{}]`\n   ```\n   {}\n   ```\n\n",
                i + 1,
                hit.file_path,
                hit.start_line,
                hit.end_line,
                symbol_badge,
                hit.combined_score,
                sources_badge,
                hit.snippet.trim()
            ));
        }

        out
    }
}
