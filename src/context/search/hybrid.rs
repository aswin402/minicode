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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<String>,
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

/// Extracts a compact structural code outline (`calls: ...` for functions/methods or `members: ...`
/// for structs/classes/traits/impls) using Tree-sitter AST symbol boundaries and shared lexical call
/// extraction (`zvec-grep` `extraction/code/outline.rs`).
pub fn extract_code_outline(snippet: &str, symbol_kind: Option<&str>) -> Option<String> {
    extract_code_outline_with_context(
        snippet,
        symbol_kind,
        None,
        1,
        snippet.lines().count().max(1),
        None,
    )
}

fn extract_code_outline_with_context(
    snippet: &str,
    symbol_kind: Option<&str>,
    file_path: Option<&str>,
    start_line: usize,
    end_line: usize,
    symbol_index: Option<&SymbolIndex>,
) -> Option<String> {
    let max_items = crate::constants::HYBRID_OUTLINE_MAX_ITEMS;
    let kind_lower = symbol_kind.unwrap_or("").to_ascii_lowercase();
    let is_structural = matches!(
        kind_lower.as_str(),
        "struct" | "class" | "trait" | "interface" | "enum" | "impl" | "module"
    );

    if is_structural {
        let mut members = Vec::new();
        let push_member = |list: &mut Vec<String>, m: String| {
            if !m.is_empty() && !list.contains(&m) && list.len() < max_items {
                list.push(m);
            }
        };

        // 1. Query pre-built `SymbolIndex` for nested AST symbols inside `(file_path, start_line..=end_line)`
        if let (Some(idx), Some(fp)) = (symbol_index, file_path) {
            for sym in idx.symbols() {
                if sym.line_number > start_line
                    && sym.line_number <= end_line
                    && sym.kind != "import"
                    && sym.file_path.to_string_lossy().ends_with(fp)
                {
                    let formatted = if matches!(sym.kind.as_str(), "function" | "method") {
                        format!("{}()", sym.name)
                    } else {
                        sym.name.clone()
                    };
                    push_member(&mut members, formatted);
                }
            }
        }

        // 2. Parse snippet with Tree-sitter AST (`RepoMapExtractor`) to capture nested methods/declarations
        if members.len() < max_items {
            let extractor = crate::context::repomap::RepoMapExtractor::new();
            let ext = file_path
                .and_then(|p| Path::new(p).extension())
                .and_then(|e| e.to_str())
                .unwrap_or("rs");
            if let Ok(ast_syms) = extractor.extract_symbols_from_str(ext, snippet) {
                for sym in ast_syms {
                    if sym.line_number > 1 && sym.kind != "import" {
                        let formatted = if matches!(sym.kind.as_str(), "function" | "method") {
                            format!("{}()", sym.name)
                        } else {
                            sym.name
                        };
                        push_member(&mut members, formatted);
                    }
                }
            }
        }

        // 3. Extract typed field declarations (`<modifiers> <ident>: <Type>`) via token-boundary scanning
        if members.len() < max_items {
            for (line_idx, raw_line) in snippet.lines().enumerate() {
                if line_idx == 0 {
                    continue;
                }
                let clean = crate::utils::strings::strip_code_strings_and_comments(raw_line);
                let trimmed = clean.trim();
                if trimmed.is_empty() || trimmed.ends_with('{') || trimmed.contains("::") {
                    continue;
                }
                if let Some((lhs, _rhs)) = trimmed.split_once(':') {
                    if !lhs.contains('(') {
                        if let Some(field_ident) = lhs
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .filter(|tok| !tok.is_empty())
                            .next_back()
                        {
                            if field_ident
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                            {
                                push_member(&mut members, field_ident.to_string());
                            }
                        }
                    }
                }
            }
        }

        if !members.is_empty() {
            return Some(format!("members: {}", members.join(", ")));
        }
    }

    // Extract outgoing function/method calls from the body using the shared lexical/AST call extractor
    let self_name = snippet
        .lines()
        .next()
        .and_then(|first_line| {
            first_line
                .split('(')
                .next()
                .and_then(|before_paren| {
                    before_paren
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .filter(|s| !s.is_empty())
                        .next_back()
                })
        })
        .unwrap_or("");
    let body_slice = if let Some(brace_pos) = snippet.find('{') {
        &snippet[brace_pos + 1..]
    } else if let Some(nl_pos) = snippet.find('\n') {
        &snippet[nl_pos + 1..]
    } else {
        snippet
    };
    let body_lines: Vec<&str> = body_slice.lines().collect();
    let calls = crate::context::search::semantic::extract_lightweight_calls(
        &body_lines,
        self_name,
        max_items,
    );

    if !calls.is_empty() {
        Some(format!("calls: {}", calls.join(", ")))
    } else {
        None
    }
}

/// Computes `zvec-grep` Rule Ranking static bonus (`def_bonus + path_bonus`, capped at `RULE_RANK_MAX_STATIC_BONUS`)
/// and path-role multiplier for a candidate using tokenized word/identifier boundaries.
fn compute_rule_rank_adjustment(
    query: &str,
    query_idents: &[String],
    query_tokens: &[String],
    file_path: &str,
    symbol_name: Option<&str>,
    symbol_kind: Option<&str>,
    has_explicit_file_pattern: bool,
) -> (f64, f64, Option<String>) {
    const USAGE_INTENT_TOKENS: &[&str] = &[
        "caller",
        "callers",
        "called",
        "usage",
        "usages",
        "references",
        "callsite",
        "callsites",
    ];
    const IMPL_INTENT_TOKENS: &[&str] = &[
        "implement",
        "implements",
        "implementation",
        "implementations",
        "impl",
    ];
    const TEST_ROLE_TOKENS: &[&str] = &[
        "test", "tests", "testing", "mock", "mocks", "fixture", "fixtures", "assert", "spec",
        "specs",
    ];

    let q_lower = query.trim().to_ascii_lowercase();
    let has_where_is = query_tokens
        .windows(2)
        .any(|w| w[0] == "where" && w[1] == "is");
    let is_usage_query = has_where_is
        || query_tokens
            .iter()
            .any(|t| USAGE_INTENT_TOKENS.contains(&t.as_str()));
    let is_impl_query = query_tokens
        .iter()
        .any(|t| IMPL_INTENT_TOKENS.contains(&t.as_str()));
    let is_test_query = query_tokens
        .iter()
        .any(|t| TEST_ROLE_TOKENS.contains(&t.as_str()));

    // 1. Definition bonus
    let mut def_bonus = 0.0;
    if !is_usage_query {
        if let Some(sym) = symbol_name {
            let kind = symbol_kind.unwrap_or("").to_ascii_lowercase();
            let skip_interface_decl =
                is_impl_query && matches!(kind.as_str(), "trait" | "interface");
            if !skip_interface_decl {
                let sym_leaf = sym.rsplit("::").next().unwrap_or(sym);
                if q_lower == sym.to_ascii_lowercase() || q_lower == sym_leaf.to_ascii_lowercase() {
                    def_bonus = crate::constants::RULE_RANK_BARE_SYMBOL_DEF_BONUS;
                } else if query_idents
                    .iter()
                    .any(|id| id.eq_ignore_ascii_case(sym) || id.eq_ignore_ascii_case(sym_leaf))
                {
                    def_bonus = if query_idents.len() == 1 {
                        crate::constants::RULE_RANK_SINGLE_IDENT_DEF_BONUS
                    } else {
                        crate::constants::RULE_RANK_MULTI_IDENT_DEF_BONUS
                    };
                }
            }
        }
    }

    // 2. Positive path affinity (`RULE_RANK_MAX_PATH_BONUS` max from file stem and nearest 2 directories)
    let path_obj = Path::new(file_path);
    let stem_tokens: Vec<String> = path_obj
        .file_stem()
        .and_then(|s| s.to_str())
        .map(SymbolIndex::tokenize)
        .unwrap_or_default();
    let mut dir_tokens: Vec<String> = Vec::new();
    if let Some(parent) = path_obj.parent() {
        for comp in parent.components().rev().take(2) {
            if let Some(s) = comp.as_os_str().to_str() {
                dir_tokens.extend(SymbolIndex::tokenize(s));
            }
        }
    }

    let mut path_bonus: f64 = 0.0;
    for tok in query_tokens {
        if tok.len() < 3 {
            continue;
        }
        if stem_tokens
            .iter()
            .any(|st| st == tok || st.starts_with(tok.as_str()))
        {
            path_bonus += crate::constants::RULE_RANK_STEM_AFFINITY_BONUS;
        } else if dir_tokens
            .iter()
            .any(|dt| dt == tok || dt.starts_with(tok.as_str()))
        {
            path_bonus += crate::constants::RULE_RANK_DIR_AFFINITY_BONUS;
        }
    }
    path_bonus = path_bonus.min(crate::constants::RULE_RANK_MAX_PATH_BONUS);

    let static_bonus = (def_bonus + path_bonus).min(crate::constants::RULE_RANK_MAX_STATIC_BONUS);

    // 3. Tokenized path-role penalty for test/fixture files unless query or glob targets tests
    let is_test_path = path_obj.components().any(|comp| {
        comp.as_os_str()
            .to_str()
            .map(SymbolIndex::tokenize)
            .is_some_and(|tokens| {
                tokens
                    .iter()
                    .any(|t| TEST_ROLE_TOKENS.contains(&t.as_str()))
            })
    });
    let path_factor = if is_test_path && !is_test_query && !has_explicit_file_pattern {
        crate::constants::RULE_RANK_TEST_PATH_FACTOR
    } else {
        1.0
    };

    let badge = if def_bonus >= crate::constants::RULE_RANK_SINGLE_IDENT_DEF_BONUS {
        Some(format!("DefBonus +{:.2}", def_bonus))
    } else if path_bonus >= crate::constants::RULE_RANK_DIR_AFFINITY_BONUS * 1.5 {
        Some(format!("PathAffinity +{:.2}", path_bonus))
    } else {
        None
    };

    (static_bonus, path_factor, badge)
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

        // 6. Compute Normalized RRF + `zvec-grep` Rule Ranking + Greedy Soft File Diversity
        let query_idents = extract_query_symbol_identifiers(query);
        let query_tokens = SymbolIndex::tokenize(query);
        let has_explicit_file_pattern = opts
            .file_pattern
            .map(str::trim)
            .is_some_and(|p| !p.is_empty());

        let max_possible_rrf = (RRF_WEIGHT_LEXICAL / (RRF_K + 1.0))
            + (RRF_WEIGHT_VECTOR / (RRF_K + 1.0))
            + if opts.prefer_symbol && !query_idents.is_empty() {
                (RRF_WEIGHT_LEXICAL * crate::constants::RRF_SYMBOL_ROUTE_WEIGHT_MULTIPLIER)
                    / (RRF_K + 1.0)
            } else {
                0.0
            }
            + if !active_anchors.is_empty() {
                RRF_WEIGHT_LEXICAL / (RRF_K + 1.0)
            } else {
                0.0
            };
        let norm_denom = max_possible_rrf.max(1e-6);

        // Count distinct symbol definitions per file for `zvec-grep` file-support bonus
        let mut defs_per_file: HashMap<String, usize> = HashMap::new();
        for c in &candidates {
            if c.symbol_name.is_some() {
                *defs_per_file.entry(c.file_path.clone()).or_insert(0) += 1;
            }
        }

        let mut pool: Vec<HybridHit> = candidates
            .into_iter()
            .map(|c| {
                let mut rrf_score = 0.0;
                let mut sources = Vec::new();

                if let Some(l_rank) = c.lexical_rank {
                    rrf_score += RRF_WEIGHT_LEXICAL / (RRF_K + l_rank as f64);
                    sources.push(format!("BM25 #{}", l_rank));
                }

                if let Some(s_rank) = c.symbol_route_rank {
                    rrf_score += (RRF_WEIGHT_LEXICAL
                        * crate::constants::RRF_SYMBOL_ROUTE_WEIGHT_MULTIPLIER)
                        / (RRF_K + s_rank as f64);
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

                let normalized_rrf = rrf_score / norm_denom;
                let (static_bonus, path_factor, rule_badge) = compute_rule_rank_adjustment(
                    query,
                    &query_idents,
                    &query_tokens,
                    &c.file_path,
                    c.symbol_name.as_deref(),
                    c.symbol_kind.as_deref(),
                    has_explicit_file_pattern,
                );
                if let Some(badge) = rule_badge {
                    sources.push(badge);
                }

                let extra_defs = defs_per_file
                    .get(&c.file_path)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(1)
                    .min(crate::constants::RULE_RANK_MAX_EXTRA_FILE_DEFS);
                let file_support_bonus = if c.symbol_name.is_some() && extra_defs > 0 {
                    crate::constants::RULE_RANK_FILE_SUPPORT_STEP * (extra_defs as f64)
                } else {
                    0.0
                };

                let rule_score =
                    ((normalized_rrf + static_bonus) * path_factor) + file_support_bonus;
                let outline = extract_code_outline_with_context(
                    &c.snippet,
                    c.symbol_kind.as_deref(),
                    Some(&c.file_path),
                    c.start_line,
                    c.end_line,
                    Some(&self.symbol_index),
                );

                HybridHit {
                    file_path: c.file_path,
                    start_line: c.start_line,
                    end_line: c.end_line,
                    snippet: c.snippet,
                    symbol_name: c.symbol_name,
                    symbol_kind: c.symbol_kind,
                    outline,
                    combined_score: rule_score,
                    lexical_rank: c.lexical_rank.or(c.symbol_route_rank).or(c.anchor_rank),
                    vector_rank: c.vector_rank,
                    pagerank_score: c.pagerank_score,
                    match_sources: sources,
                }
            })
            .collect();

        pool.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.start_line.cmp(&b.start_line))
                .then_with(|| a.symbol_name.cmp(&b.symbol_name))
        });

        // Greedy soft file-diversity selection (`1 / (1 + HYBRID_FILE_DIVERSITY_DECAY * selected_count)` per file)
        let distinct_files_gt_1 = {
            let first_file = pool.first().map(|h| h.file_path.as_str());
            pool.iter().any(|h| Some(h.file_path.as_str()) != first_file)
        };

        if !distinct_files_gt_1 || limit == 0 {
            pool.truncate(limit);
            return pool;
        }

        let target_len = limit.min(pool.len());
        let mut selected = Vec::with_capacity(target_len);
        let mut file_counts: HashMap<String, usize> = HashMap::new();

        while selected.len() < target_len && !pool.is_empty() {
            let mut best_idx = 0;
            let mut best_effective_score = f64::NEG_INFINITY;

            for (idx, cand) in pool.iter().enumerate() {
                let count = file_counts.get(&cand.file_path).copied().unwrap_or(0);
                let diversity_factor = 1.0
                    / (1.0 + crate::constants::HYBRID_FILE_DIVERSITY_DECAY * (count as f64));
                let eff = cand.combined_score * diversity_factor;
                if eff > best_effective_score {
                    best_effective_score = eff;
                    best_idx = idx;
                }
            }

            let mut chosen = pool.remove(best_idx);
            let prev_count = file_counts.entry(chosen.file_path.clone()).or_insert(0);
            if *prev_count > 0 {
                let div = 1.0
                    / (1.0
                        + crate::constants::HYBRID_FILE_DIVERSITY_DECAY * (*prev_count as f64));
                chosen.combined_score = best_effective_score;
                chosen.match_sources.push(format!("Diversity x{:.2}", div));
            }
            *prev_count += 1;
            selected.push(chosen);
        }

        selected
    }

    /// Executes multiple query groups in a single call (`zvec-grep` `queries` + `fuse`).
    /// When `fuse` is `true`, collapses all query groups into a single ranked & deduplicated list.
    /// When `fuse` is `false`, searches each query group independently and formats them with `-- query i/N --` headers.
    pub fn search_multi_queries(
        &self,
        queries: &[String],
        limit: usize,
        fuse: bool,
        opts: &HybridSearchOptions<'_>,
    ) -> String {
        let active_queries: Vec<&str> = queries
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .collect();

        if active_queries.is_empty() {
            return "### 🔍 Hybrid Retrieval\n\nNo valid search queries provided.\n".to_string();
        }

        if active_queries.len() == 1 {
            let q = active_queries[0];
            let hits = self.search_with_options(q, limit, opts);
            return Self::format_markdown(q, &hits);
        }

        if fuse {
            let mut fused_map: HashMap<(String, usize), HybridHit> = HashMap::new();
            for (q_idx, &q) in active_queries.iter().enumerate() {
                let hits = self.search_with_options(q, limit * 2, opts);
                for (rank, hit) in hits.into_iter().enumerate() {
                    let rrf_contrib = 1.0 / (RRF_K + (rank + 1) as f64);
                    let key = (hit.file_path.clone(), hit.start_line);
                    if let Some(existing) = fused_map.get_mut(&key) {
                        if hit.combined_score > existing.combined_score {
                            existing.combined_score = hit.combined_score
                                + crate::constants::HYBRID_MULTI_QUERY_FUSE_SECONDARY_WEIGHT
                                    * existing.combined_score;
                        } else {
                            existing.combined_score +=
                                crate::constants::HYBRID_MULTI_QUERY_FUSE_SECONDARY_WEIGHT
                                    * hit.combined_score
                                    + rrf_contrib;
                        }
                    } else {
                        fused_map.insert(key.clone(), hit);
                    }
                    if let Some(entry) = fused_map.get_mut(&key) {
                        let group_tag = format!("Q{} #{}", q_idx + 1, rank + 1);
                        if !entry.match_sources.contains(&group_tag) {
                            entry.match_sources.push(group_tag);
                        }
                    }
                }
            }
            let mut fused_hits: Vec<HybridHit> = fused_map.into_values().collect();
            fused_hits.sort_by(|a, b| {
                b.combined_score
                    .partial_cmp(&a.combined_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            fused_hits.truncate(limit);
            let joined_title = active_queries.join(" + ");
            return Self::format_markdown(&joined_title, &fused_hits);
        }

        let total = active_queries.len();
        let mut sections = Vec::with_capacity(total);
        for (idx, &q) in active_queries.iter().enumerate() {
            let hits = self.search_with_options(q, limit, opts);
            sections.push(format!(
                "-- query {}/{}: {} --\n{}",
                idx + 1,
                total,
                q,
                Self::format_markdown(q, &hits)
            ));
        }
        sections.join("\n")
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
            let outline_line = if let Some(ref outline) = hit.outline {
                format!("   - **Outline:** `{}`\n", outline)
            } else {
                String::new()
            };

            out.push_str(&format!(
                "{}. **`{}:{}-{}`**{}\n   - **Score:** `{:.4}` `[{}]`\n{}   ```\n   {}\n   ```\n\n",
                i + 1,
                hit.file_path,
                hit.start_line,
                hit.end_line,
                symbol_badge,
                hit.combined_score,
                sources_badge,
                outline_line,
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

    #[test]
    fn test_extract_code_outline_calls_and_members() {
        let fn_snippet = "pub fn authenticate_request(req: &Request) -> bool {\n    let token = extract_bearer_token(req);\n    verify_jwt_signature(&token)\n}";
        let fn_outline = extract_code_outline(fn_snippet, Some("function")).expect("outline");
        assert!(
            fn_outline.contains("calls: extract_bearer_token, verify_jwt_signature"),
            "Unexpected fn outline: {}",
            fn_outline
        );

        let struct_snippet = "pub struct ServerConfig {\n    pub host: String,\n    pub port: u16,\n    pub tls_enabled: bool,\n}";
        let struct_outline =
            extract_code_outline(struct_snippet, Some("struct")).expect("struct outline");
        assert!(
            struct_outline.contains("members: host, port, tls_enabled"),
            "Unexpected struct outline: {}",
            struct_outline
        );
    }

    #[test]
    fn test_multi_query_group_and_fused_search() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_hybrid_multi_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        fs::write(
            temp_dir.join("jwt.rs"),
            "pub fn decode_jwt_claims(token: &str) -> bool { validate_expiry(token) }\n",
        )
        .unwrap();
        fs::write(
            temp_dir.join("rate_limit.rs"),
            "pub fn check_rate_limit_bucket(ip: &str) -> bool { refill_tokens(ip) }\n",
        )
        .unwrap();

        let mut index = HybridIndex::new();
        index.build_index(&temp_dir).unwrap();

        let queries = vec![
            "decode_jwt_claims".to_string(),
            "check_rate_limit_bucket".to_string(),
        ];
        let opts = HybridSearchOptions::default();

        let grouped = index.search_multi_queries(&queries, 3, false, &opts);
        assert!(grouped.contains("-- query 1/2: decode_jwt_claims --"));
        assert!(grouped.contains("-- query 2/2: check_rate_limit_bucket --"));

        let fused = index.search_multi_queries(&queries, 5, true, &opts);
        assert!(fused.contains("jwt.rs"));
        assert!(fused.contains("rate_limit.rs"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
