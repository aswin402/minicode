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

/// Configuration options for multi-route hybrid search (`zvec-grep` search pipeline).
#[derive(Debug, Clone)]
pub struct HybridSearchOptions<'a> {
    pub include_symbols: bool,
    pub prefer_symbol: bool,
    pub fts_anchors: &'a [String],
    pub file_pattern: Option<&'a str>,
}

impl<'a> Default for HybridSearchOptions<'a> {
    fn default() -> Self {
        Self {
            include_symbols: true,
            prefer_symbol: true,
            fts_anchors: &[],
            file_pattern: None,
        }
    }
}

/// Extracts likely code symbol identifiers (`CamelCase`, `snake_case`, `SCREAMING_SNAKE`,
/// `Owner::member`, or backtick-wrapped identifiers) from a natural-language or mixed query
/// (`zvec-grep` `extractSymbolNames` / `prefer-symbol` routing).
pub fn extract_query_symbol_identifiers(query: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "the", "and", "for", "with", "from", "that", "this", "where", "what", "when", "how",
        "does", "are", "was", "were", "will", "would", "could", "should", "into", "about",
        "search", "find", "locate", "function", "method", "struct", "class", "trait", "impl",
        "file", "code", "error", "handle", "return", "value", "type", "string", "true", "false",
    ];

    let mut out = Vec::new();
    let mut push_unique = |cand: &str| {
        let cleaned = cand.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if cleaned.len() < 3 {
            return;
        }
        let lower = cleaned.to_lowercase();
        if STOPWORDS.contains(&lower.as_str()) {
            return;
        }
        if !out.iter().any(|existing: &String| existing == cleaned) {
            out.push(cleaned.to_string());
        }
    };

    // 1. Extract explicit backtick-quoted tokens first
    let mut in_ticks = false;
    let mut tick_buf = String::new();
    for ch in query.chars() {
        if ch == '`' {
            if in_ticks {
                let trimmed = tick_buf.trim().trim_end_matches("()");
                if !trimmed.is_empty() && !trimmed.contains(' ') {
                    push_unique(trimmed);
                    for part in trimmed.split([':', '.']) {
                        if !part.is_empty() {
                            push_unique(part);
                        }
                    }
                }
                tick_buf.clear();
                in_ticks = false;
            } else {
                in_ticks = true;
            }
        } else if in_ticks {
            tick_buf.push(ch);
        }
    }

    // 2. Scan whitespace/punctuation-delimited tokens for code-shaped identifiers
    for raw_token in query.split_whitespace() {
        let token = raw_token.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '_' && c != ':' && c != '.' && c != '('
        });
        if token.is_empty() {
            continue;
        }

        let has_call_parens = token.contains('(');
        let base_token = token.split('(').next().unwrap_or(token);

        if base_token.contains("::") || base_token.contains('.') {
            for part in base_token.split([':', '.']) {
                if !part.is_empty() {
                    let has_upper = part.chars().any(|c| c.is_ascii_uppercase());
                    let has_lower = part.chars().any(|c| c.is_ascii_lowercase());
                    let has_underscore = part.contains('_');
                    if has_underscore || (has_upper && has_lower) || part.len() >= 4 {
                        push_unique(part);
                    }
                }
            }
            continue;
        }

        let has_underscore = base_token.contains('_');
        let has_upper = base_token.chars().any(|c| c.is_ascii_uppercase());
        let has_lower = base_token.chars().any(|c| c.is_ascii_lowercase());
        let upper_transitions = base_token
            .chars()
            .zip(base_token.chars().skip(1))
            .filter(|(a, b)| a.is_ascii_lowercase() && b.is_ascii_uppercase())
            .count();
        let is_camel_or_pascal =
            has_upper && has_lower && (upper_transitions > 0 || base_token.len() >= 5);
        let is_screaming_snake = has_underscore && has_upper && !has_lower && base_token.len() >= 4;

        if has_underscore || is_camel_or_pascal || is_screaming_snake || has_call_parens {
            push_unique(base_token);
        }
    }

    out
}

fn matches_file_filter(file_path: &str, pattern: Option<&str>) -> bool {
    let Some(pat) = pattern.map(str::trim).filter(|p| !p.is_empty()) else {
        return true;
    };
    let file_name = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file_path);
    crate::utils::strings::wildcard_match(file_path, pat)
        || crate::utils::strings::wildcard_match(file_name, pat)
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
        let opts = HybridSearchOptions {
            include_symbols,
            ..Default::default()
        };
        self.search_with_options(query, limit, &opts)
    }

    /// Executes a multi-route hybrid search combining BM25, dedicated `fts_anchors` lexical routes,
    /// `prefer_symbol` identifier routes, dense semantic vectors, and PageRank with RRF fusion.
    pub fn search_with_options(
        &self,
        query: &str,
        limit: usize,
        opts: &HybridSearchOptions<'_>,
    ) -> Vec<HybridHit> {
        // Adaptive recall pool sizing (`zvec-grep` resolveRecallLimit: widen recall when path/glob filter is active)
        let pool_size = if opts.file_pattern.is_some() {
            limit.max(10) * 8
        } else {
            limit.max(10) * 3
        };

        // 1. Retrieve Lexical BM25 Matches
        let lexical_matches = self.symbol_index.search_symbols(query, pool_size);

        // 2. Retrieve Dense Vector Semantic Matches
        let vector_matches = self.semantic_index.search(query, pool_size);

        // 3. Optional Symbol Vector Matches
        let symbol_vector_matches = if opts.include_symbols {
            self.semantic_index.search_symbols(query, pool_size)
        } else {
            Vec::new()
        };

        // 4. Dedicated `prefer_symbol` Identifier Route (`zvec-grep` preferSymbol)
        let mut symbol_route_matches = Vec::new();
        if opts.prefer_symbol {
            for ident in extract_query_symbol_identifiers(query) {
                for sym in self.symbol_index.locate_symbol(&ident) {
                    symbol_route_matches.push(sym);
                    if symbol_route_matches.len() >= pool_size {
                        break;
                    }
                }
            }
        }

        // 5. Dedicated `fts_anchors` Lexical Routes (`zvec-grep` fts array)
        let mut fts_anchor_symbol_matches = Vec::new();
        let mut fts_anchor_vector_matches = Vec::new();
        let active_anchors: Vec<&str> = opts
            .fts_anchors
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        for anchor in &active_anchors {
            for sym in self.symbol_index.search_symbols(anchor, pool_size / 2 + 5) {
                fts_anchor_symbol_matches.push(sym);
            }
            for vec_hit in self.semantic_index.search(anchor, pool_size / 2 + 5) {
                fts_anchor_vector_matches.push(vec_hit);
            }
        }

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
            symbol_route_rank: Option<usize>,
            anchor_rank: Option<usize>,
            pagerank_score: f64,
        }

        let mut candidates: Vec<Candidate> = Vec::new();

        let find_match = |candidates: &[Candidate],
                          file_path: &str,
                          start_line: usize,
                          end_line: usize,
                          sym_name: Option<&str>|
         -> Option<usize> {
            candidates.iter().position(|c| {
                if c.file_path != file_path {
                    return false;
                }
                if let (Some(s1), Some(s2)) = (c.symbol_name.as_deref(), sym_name) {
                    if s1 == s2 {
                        return true;
                    }
                }
                !(c.end_line < start_line || end_line < c.start_line)
            })
        };

        // Index Lexical Hits
        for (rank, lex) in lexical_matches.into_iter().enumerate() {
            let file_path = lex.file_path.display().to_string();
            if !matches_file_filter(&file_path, opts.file_pattern) {
                continue;
            }
            let start_line = lex.line_number;
            let end_line = lex.line_number + 10;
            let sym_name = Some(lex.name.as_str());

            let pr = self
                .pagerank_map
                .get(&lex.name)
                .or_else(|| self.pagerank_map.get(&file_path))
                .copied()
                .unwrap_or(0.0);

            if let Some(idx) = find_match(&candidates, &file_path, start_line, end_line, sym_name) {
                let c = &mut candidates[idx];
                if c.lexical_rank.is_none() {
                    c.lexical_rank = Some(rank + 1);
                }
                if c.symbol_name.is_none() {
                    c.symbol_name = Some(lex.name);
                    c.symbol_kind = Some(lex.kind);
                }
                if c.pagerank_score == 0.0 {
                    c.pagerank_score = pr;
                }
            } else {
                candidates.push(Candidate {
                    file_path,
                    start_line,
                    end_line,
                    snippet: lex.signature,
                    symbol_name: Some(lex.name),
                    symbol_kind: Some(lex.kind),
                    lexical_rank: Some(rank + 1),
                    vector_rank: None,
                    symbol_route_rank: None,
                    anchor_rank: None,
                    pagerank_score: pr,
                });
            }
        }

        // Index Dedicated Symbol Route Hits (`prefer_symbol`)
        for (rank, sym) in symbol_route_matches.into_iter().enumerate() {
            let file_path = sym.file_path.display().to_string();
            if !matches_file_filter(&file_path, opts.file_pattern) {
                continue;
            }
            let start_line = sym.line_number;
            let end_line = sym.line_number + 10;
            let sym_name = Some(sym.name.as_str());

            let pr = self
                .pagerank_map
                .get(&sym.name)
                .or_else(|| self.pagerank_map.get(&file_path))
                .copied()
                .unwrap_or(0.0);

            if let Some(idx) = find_match(&candidates, &file_path, start_line, end_line, sym_name) {
                let c = &mut candidates[idx];
                if c.symbol_route_rank.is_none() {
                    c.symbol_route_rank = Some(rank + 1);
                }
                if c.symbol_name.is_none() {
                    c.symbol_name = Some(sym.name);
                    c.symbol_kind = Some(sym.kind);
                }
                if c.pagerank_score == 0.0 {
                    c.pagerank_score = pr;
                }
            } else {
                candidates.push(Candidate {
                    file_path,
                    start_line,
                    end_line,
                    snippet: sym.signature,
                    symbol_name: Some(sym.name),
                    symbol_kind: Some(sym.kind),
                    lexical_rank: None,
                    vector_rank: None,
                    symbol_route_rank: Some(rank + 1),
                    anchor_rank: None,
                    pagerank_score: pr,
                });
            }
        }

        // Index Dedicated FTS Anchor Symbol Hits
        for (rank, sym) in fts_anchor_symbol_matches.into_iter().enumerate() {
            let file_path = sym.file_path.display().to_string();
            if !matches_file_filter(&file_path, opts.file_pattern) {
                continue;
            }
            let start_line = sym.line_number;
            let end_line = sym.line_number + 10;
            let sym_name = Some(sym.name.as_str());

            let pr = self
                .pagerank_map
                .get(&sym.name)
                .or_else(|| self.pagerank_map.get(&file_path))
                .copied()
                .unwrap_or(0.0);

            if let Some(idx) = find_match(&candidates, &file_path, start_line, end_line, sym_name) {
                let c = &mut candidates[idx];
                if c.anchor_rank.is_none() {
                    c.anchor_rank = Some(rank + 1);
                }
                if c.symbol_name.is_none() {
                    c.symbol_name = Some(sym.name);
                    c.symbol_kind = Some(sym.kind);
                }
                if c.pagerank_score == 0.0 {
                    c.pagerank_score = pr;
                }
            } else {
                candidates.push(Candidate {
                    file_path,
                    start_line,
                    end_line,
                    snippet: sym.signature,
                    symbol_name: Some(sym.name),
                    symbol_kind: Some(sym.kind),
                    lexical_rank: None,
                    vector_rank: None,
                    symbol_route_rank: None,
                    anchor_rank: Some(rank + 1),
                    pagerank_score: pr,
                });
            }
        }

        // Index Dedicated FTS Anchor Vector Chunk Hits (verified to contain the anchor text)
        for (rank, vec_hit) in fts_anchor_vector_matches.into_iter().enumerate() {
            let file_path = vec_hit.file_path;
            if !matches_file_filter(&file_path, opts.file_pattern) {
                continue;
            }
            let lower_snip = vec_hit.snippet.to_lowercase();
            let lower_sym = vec_hit.symbol_name.as_deref().unwrap_or("").to_lowercase();
            let has_exact_anchor = active_anchors.iter().any(|a| {
                let al = a.to_lowercase();
                lower_snip.contains(&al) || lower_sym.contains(&al)
            });
            if !has_exact_anchor {
                continue;
            }

            let start_line = vec_hit.start_line;
            let end_line = vec_hit.end_line;
            let sym_name = vec_hit.symbol_name.as_deref();

            let pr = sym_name
                .and_then(|name| self.pagerank_map.get(name))
                .or_else(|| self.pagerank_map.get(&file_path))
                .copied()
                .unwrap_or(0.0);

            if let Some(idx) = find_match(&candidates, &file_path, start_line, end_line, sym_name) {
                let c = &mut candidates[idx];
                if c.anchor_rank.is_none() || c.anchor_rank.unwrap_or(usize::MAX) > rank + 1 {
                    c.anchor_rank = Some(rank + 1);
                }
            } else {
                candidates.push(Candidate {
                    file_path,
                    start_line,
                    end_line,
                    snippet: vec_hit.snippet,
                    symbol_name: vec_hit.symbol_name,
                    symbol_kind: vec_hit.symbol_kind,
                    lexical_rank: None,
                    vector_rank: None,
                    symbol_route_rank: None,
                    anchor_rank: Some(rank + 1),
                    pagerank_score: pr,
                });
            }
        }

        // Index Vector Chunk Hits
        for (rank, vec_hit) in vector_matches.into_iter().enumerate() {
            let file_path = vec_hit.file_path;
            if !matches_file_filter(&file_path, opts.file_pattern) {
                continue;
            }
            let start_line = vec_hit.start_line;
            let end_line = vec_hit.end_line;
            let sym_name = vec_hit.symbol_name.as_deref();

            let pr = sym_name
                .and_then(|name| self.pagerank_map.get(name))
                .or_else(|| self.pagerank_map.get(&file_path))
                .copied()
                .unwrap_or(0.0);

            if let Some(idx) = find_match(&candidates, &file_path, start_line, end_line, sym_name) {
                let c = &mut candidates[idx];
                if c.vector_rank.is_none() {
                    c.vector_rank = Some(rank + 1);
                }
                if c.snippet.is_empty() {
                    c.snippet = vec_hit.snippet;
                }
                if c.symbol_name.is_none() && vec_hit.symbol_name.is_some() {
                    c.symbol_name = vec_hit.symbol_name;
                    c.symbol_kind = vec_hit.symbol_kind;
                }
                if c.pagerank_score == 0.0 {
                    c.pagerank_score = pr;
                }
                c.start_line = c.start_line.min(start_line);
                c.end_line = c.end_line.max(end_line);
            } else {
                candidates.push(Candidate {
                    file_path,
                    start_line,
                    end_line,
                    snippet: vec_hit.snippet,
                    symbol_name: vec_hit.symbol_name,
                    symbol_kind: vec_hit.symbol_kind,
                    lexical_rank: None,
                    vector_rank: Some(rank + 1),
                    symbol_route_rank: None,
                    anchor_rank: None,
                    pagerank_score: pr,
                });
            }
        }

        // Index Symbol Vector Hits
        for (rank, sym_hit) in symbol_vector_matches.into_iter().enumerate() {
            let file_path = sym_hit.file_path;
            if !matches_file_filter(&file_path, opts.file_pattern) {
                continue;
            }
            let start_line = sym_hit.start_line;
            let end_line = sym_hit.end_line;
            let sym_name = sym_hit.symbol_name.as_deref();

            let pr = sym_name
                .and_then(|name| self.pagerank_map.get(name))
                .or_else(|| self.pagerank_map.get(&file_path))
                .copied()
                .unwrap_or(0.0);

            if let Some(idx) = find_match(&candidates, &file_path, start_line, end_line, sym_name) {
                let c = &mut candidates[idx];
                if c.vector_rank.is_none() || c.vector_rank.unwrap_or(usize::MAX) > rank + 1 {
                    c.vector_rank = Some(rank + 1);
                }
                if c.snippet.is_empty() {
                    c.snippet = sym_hit.snippet;
                }
                if c.symbol_name.is_none() && sym_hit.symbol_name.is_some() {
                    c.symbol_name = sym_hit.symbol_name;
                    c.symbol_kind = sym_hit.symbol_kind;
                }
                if c.pagerank_score == 0.0 {
                    c.pagerank_score = pr;
                }
                c.start_line = c.start_line.min(start_line);
                c.end_line = c.end_line.max(end_line);
            } else {
                candidates.push(Candidate {
                    file_path,
                    start_line,
                    end_line,
                    snippet: sym_hit.snippet,
                    symbol_name: sym_hit.symbol_name,
                    symbol_kind: sym_hit.symbol_kind,
                    lexical_rank: None,
                    vector_rank: Some(rank + 1),
                    symbol_route_rank: None,
                    anchor_rank: None,
                    pagerank_score: pr,
                });
            }
        }

        // 6. Compute RRF Scores
        let mut results: Vec<HybridHit> = candidates
            .into_iter()
            .map(|c| {
                let mut rrf_score = 0.0;
                let mut sources = Vec::new();

                if let Some(l_rank) = c.lexical_rank {
                    rrf_score += RRF_WEIGHT_LEXICAL / (RRF_K + l_rank as f64);
                    sources.push(format!("BM25 #{}", l_rank));
                }

                if let Some(s_rank) = c.symbol_route_rank {
                    rrf_score += (RRF_WEIGHT_LEXICAL * 1.2) / (RRF_K + s_rank as f64);
                    sources.push(format!("SymbolRoute #{}", s_rank));
                }

                if let Some(a_rank) = c.anchor_rank {
                    rrf_score += RRF_WEIGHT_LEXICAL / (RRF_K + a_rank as f64);
                    sources.push(format!("FTS Anchor #{}", a_rank));
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
                    lexical_rank: c.lexical_rank.or(c.symbol_route_rank).or(c.anchor_rank),
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
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.start_line.cmp(&b.start_line))
                .then_with(|| a.symbol_name.cmp(&b.symbol_name))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_extract_query_symbol_identifiers() {
        let idents = extract_query_symbol_identifiers(
            "where does HybridIndex::search_with_options call validate_jwt_token() or check MAX_AST_CHUNK_LINES?",
        );
        assert!(idents.contains(&"HybridIndex".to_string()));
        assert!(idents.contains(&"search_with_options".to_string()));
        assert!(idents.contains(&"validate_jwt_token".to_string()));
        assert!(idents.contains(&"MAX_AST_CHUNK_LINES".to_string()));
        assert!(!idents.contains(&"where".to_string()));
    }

    #[test]
    fn test_hybrid_search_with_options_fts_anchors_and_file_pattern() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_hybrid_opts_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        fs::write(
            temp_dir.join("auth_guard.rs"),
            "pub struct TokenVerifier;\nimpl TokenVerifier {\n    pub fn verify_hmac_signature(&self, payload: &str) -> bool { true }\n}\n",
        )
        .unwrap();
        fs::write(
            temp_dir.join("other_notes.md"),
            "# Architecture\n\nGeneral overview of request handling.\n",
        )
        .unwrap();

        let mut index = HybridIndex::new();
        index.build_index(&temp_dir).unwrap();

        let anchors = vec!["verify_hmac_signature".to_string()];
        let opts = HybridSearchOptions {
            include_symbols: true,
            prefer_symbol: true,
            fts_anchors: &anchors,
            file_pattern: Some("*.rs"),
        };

        let hits = index.search_with_options(
            "how does TokenVerifier check cryptographic signature",
            5,
            &opts,
        );
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|h| h.file_path.ends_with("auth_guard.rs")));
        assert!(
            hits.iter().any(|h| h
                .match_sources
                .iter()
                .any(|s| s.contains("SymbolRoute") || s.contains("FTS Anchor"))),
            "Expected SymbolRoute or FTS Anchor in match_sources: {:?}",
            hits
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
