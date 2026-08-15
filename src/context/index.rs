use crate::constants::{
    BM25_B, BM25_K1, SYMBOL_DEF_KIND_BOOST, SYMBOL_EXACT_MATCH_SCORE, SYMBOL_FUNC_KIND_BOOST,
    SYMBOL_PREFIX_MATCH_SCORE, SYMBOL_TEST_PENALTY_FACTOR,
};
use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::error::Result;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A ranked symbol match from the inverted symbol index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub file_path: PathBuf,
    pub line_number: usize,
    pub score: f64,
    pub doc_comment: Option<String>,
}

/// In-memory inverted symbol and identifier index for sub-millisecond code navigation
pub struct SymbolIndex {
    symbols: Vec<SymbolMatch>,
    postings: BTreeMap<String, Vec<usize>>, // token -> Vec<symbol_idx>
    doc_lengths: Vec<usize>,                // token count per symbol
    doc_freq: HashMap<String, usize>, // token -> document frequency (count of symbols with token)
    doc_count: usize,                 // total indexed symbols
    avg_doc_len: f64,                 // average token length per symbol
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            postings: BTreeMap::new(),
            doc_lengths: Vec::new(),
            doc_freq: HashMap::new(),
            doc_count: 0,
            avg_doc_len: 1.0,
        }
    }

    /// Splits an identifier (`camelCase`, `snake_case`, `SCREAMING_SNAKE`) into searchable tokens
    pub fn tokenize(text: &str) -> Vec<String> {
        let mut tokens = HashSet::new();
        let lower_full = text.to_lowercase();
        tokens.insert(lower_full.clone());

        // Split by non-alphanumeric (underscores, dots, dashes, colons)
        let parts: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|p| !p.is_empty())
            .collect();

        for part in parts {
            tokens.insert(part.to_lowercase());

            // CamelCase split
            let mut current = String::new();
            for (i, c) in part.chars().enumerate() {
                if c.is_uppercase() && i > 0 && !current.is_empty() {
                    tokens.insert(current.to_lowercase());
                    current.clear();
                }
                current.push(c);
            }
            if !current.is_empty() {
                tokens.insert(current.to_lowercase());
            }
        }

        // Keep tokens >= 2 chars, or single char if it matches the full identifier exactly
        tokens
            .into_iter()
            .filter(|t| t.len() >= 2 || t == &lower_full)
            .collect()
    }

    /// Builds the symbol index by scanning the workspace and extracting AST symbols
    pub fn build_index(&mut self, workspace_root: &Path) -> Result<()> {
        self.symbols.clear();
        self.postings.clear();
        self.doc_lengths.clear();
        self.doc_freq.clear();
        self.doc_count = 0;
        self.avg_doc_len = 1.0;

        let mut extractor = RepoMapExtractor::new();

        let walker = WalkBuilder::new(workspace_root)
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .build();

        for result in walker.flatten() {
            if result.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                let path = result.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if crate::constants::SUPPORTED_LANG_EXTENSIONS.contains(&ext) {
                        if let Ok(file_symbols) = extractor.extract_file_symbols(path) {
                            for sym in file_symbols {
                                if sym.kind != "import" {
                                    self.add_symbol(path.to_path_buf(), sym);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.recompute_corpus_stats();
        Ok(())
    }

    /// Recomputes global corpus statistics (doc count & average doc length)
    fn recompute_corpus_stats(&mut self) {
        self.doc_count = self.symbols.len();
        if self.doc_count > 0 {
            let total_len: usize = self.doc_lengths.iter().sum();
            self.avg_doc_len = total_len as f64 / self.doc_count as f64;
        } else {
            self.avg_doc_len = 1.0;
        }
    }

    /// Adds a single symbol entry to the index
    pub fn add_symbol(&mut self, file_path: PathBuf, sym: SymbolDef) {
        let sym_idx = self.symbols.len();
        let tokens = Self::tokenize(&sym.name);
        self.doc_lengths.push(tokens.len().max(1));

        for token in &tokens {
            self.postings
                .entry(token.clone())
                .or_default()
                .push(sym_idx);
            *self.doc_freq.entry(token.clone()).or_default() += 1;
        }

        self.symbols.push(SymbolMatch {
            name: sym.name,
            kind: sym.kind,
            signature: sym.signature,
            file_path,
            line_number: sym.line_number,
            score: 0.0,
            doc_comment: sym.doc_comment,
        });

        self.doc_count = self.symbols.len();
    }

    /// Locates an exact or prefix match for a symbol name
    pub fn locate_symbol(&self, name: &str) -> Vec<SymbolMatch> {
        let query_lower = name.trim().to_lowercase();
        let mut results = Vec::new();

        for sym in &self.symbols {
            let sym_name_lower = sym.name.to_lowercase();
            if sym_name_lower == query_lower {
                let mut exact = sym.clone();
                exact.score = SYMBOL_EXACT_MATCH_SCORE;
                results.push(exact);
            } else if sym_name_lower.starts_with(&query_lower) {
                let mut prefix = sym.clone();
                prefix.score = SYMBOL_PREFIX_MATCH_SCORE;
                results.push(prefix);
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Searches the symbol index using BM25 ranking, kind boosts, and test file penalties
    pub fn search_symbols(&self, query: &str, limit: usize) -> Vec<SymbolMatch> {
        let query_tokens = Self::tokenize(query);
        if query_tokens.is_empty() || self.doc_count == 0 {
            return Vec::new();
        }

        let mut scores: HashMap<usize, f64> = HashMap::new();
        let n = self.doc_count as f64;
        let avg_dl = if self.avg_doc_len > 0.0 {
            self.avg_doc_len
        } else {
            1.0
        };

        for q_token in &query_tokens {
            let mut matched_symbols = HashMap::new();

            if let Some(postings) = self.postings.get(q_token) {
                for &sym_idx in postings {
                    matched_symbols.insert(sym_idx, 1.0);
                }
            } else {
                // O(log N + K) BTreeMap prefix range search
                let mut prefix_end = q_token.clone();
                if let Some(last_char) = prefix_end.pop() {
                    if let Some(next_char) = char::from_u32(last_char as u32 + 1) {
                        prefix_end.push(next_char);
                        for (_vocab_token, postings) in
                            self.postings.range(q_token.clone()..prefix_end)
                        {
                            for &sym_idx in postings {
                                matched_symbols
                                    .entry(sym_idx)
                                    .or_insert(crate::constants::BM25_PREFIX_WEIGHT);
                            }
                        }
                    }
                }
            }

            if !matched_symbols.is_empty() {
                let df = (matched_symbols.len() as f64).max(1.0);
                // Standard Robertson-Spärck Jones IDF with smoothing
                let idf = (((n - df + 0.5) / (df + 0.5)) + 1.0).ln().max(0.1);

                for (sym_idx, weight) in matched_symbols {
                    let dl = self.doc_lengths.get(sym_idx).copied().unwrap_or(1) as f64;
                    let tf = 1.0 * weight;
                    let bm25 = idf * (tf * (BM25_K1 + 1.0))
                        / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (dl / avg_dl)));

                    *scores.entry(sym_idx).or_insert(0.0) += bm25;
                }
            }
        }

        let mut ranked = Vec::new();
        for (sym_idx, base_score) in scores {
            if let Some(sym) = self.symbols.get(sym_idx) {
                let mut score = base_score;

                // Kind boosts
                match sym.kind.as_str() {
                    "struct" | "class" | "interface" | "trait" | "enum" => {
                        score += SYMBOL_DEF_KIND_BOOST;
                    }
                    "function" => {
                        score += SYMBOL_FUNC_KIND_BOOST;
                    }
                    _ => {}
                }

                // Test file down-ranking
                let path_str = sym.file_path.to_string_lossy().to_lowercase();
                if path_str.contains("test") || path_str.contains("mock") {
                    score *= SYMBOL_TEST_PENALTY_FACTOR;
                }

                let mut matched = sym.clone();
                matched.score = score;
                ranked.push(matched);
            }
        }

        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(limit);
        ranked
    }

    /// Formats search results into a clean markdown block for the agent
    pub fn format_matches(&self, matches: &[SymbolMatch], workspace_root: &Path) -> String {
        if matches.is_empty() {
            return "No matching symbols found.".to_string();
        }

        let mut lines = Vec::new();
        lines.push(format!("Found {} symbol declaration(s):", matches.len()));

        for m in matches {
            let rel_path = m
                .file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&m.file_path)
                .display();

            lines.push(format!(
                "- `{}` ({}) at `{}:{}`",
                m.name, m.kind, rel_path, m.line_number
            ));
            lines.push(format!("  Signature: `{}`", m.signature));
            if let Some(ref doc) = m.doc_comment {
                lines.push(format!("  Doc: *{}*", doc));
            }
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_identifiers() {
        let tokens = SymbolIndex::tokenize("executeTurn");
        assert!(tokens.contains(&"execute".to_string()));
        assert!(tokens.contains(&"turn".to_string()));

        let tokens_snake = SymbolIndex::tokenize("user_profile_handler");
        assert!(tokens_snake.contains(&"user".to_string()));
        assert!(tokens_snake.contains(&"profile".to_string()));
        assert!(tokens_snake.contains(&"handler".to_string()));

        // Single character identifier preserved
        let tokens_single = SymbolIndex::tokenize("x");
        assert!(tokens_single.contains(&"x".to_string()));
    }

    #[test]
    fn test_symbol_index_lookup_and_ranking() {
        let mut index = SymbolIndex::new();

        index.add_symbol(
            PathBuf::from("src/auth.rs"),
            SymbolDef {
                name: "verify_token".to_string(),
                kind: "function".to_string(),
                signature: "pub fn verify_token(token: &str) -> bool".to_string(),
                line_number: 12,
                end_line: 18,
                doc_comment: Some("Verifies JWT token".to_string()),
            },
        );

        index.add_symbol(
            PathBuf::from("tests/auth_test.rs"),
            SymbolDef {
                name: "test_verify_token".to_string(),
                kind: "function".to_string(),
                signature: "fn test_verify_token()".to_string(),
                line_number: 5,
                end_line: 10,
                doc_comment: None,
            },
        );

        let exact = index.locate_symbol("verify_token");
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].file_path, PathBuf::from("src/auth.rs"));

        let search = index.search_symbols("verify token", 10);
        assert!(!search.is_empty());
        // Production file ranked higher than test file due to down-ranking
        assert_eq!(search[0].file_path, PathBuf::from("src/auth.rs"));
    }

    #[test]
    fn test_bm25_idf_discrimination() {
        let mut index = SymbolIndex::new();

        // 3 symbols share "handler", but only 1 has "crypto"
        for i in 1..=3 {
            index.add_symbol(
                PathBuf::from(format!("src/handler_{}.rs", i)),
                SymbolDef {
                    name: format!("request_handler_{}", i),
                    kind: "function".to_string(),
                    signature: "fn handler()".to_string(),
                    line_number: 1,
                    end_line: 5,
                    doc_comment: None,
                },
            );
        }

        index.add_symbol(
            PathBuf::from("src/crypto.rs"),
            SymbolDef {
                name: "crypto_handler".to_string(),
                kind: "struct".to_string(),
                signature: "struct CryptoHandler;".to_string(),
                line_number: 10,
                end_line: 15,
                doc_comment: None,
            },
        );

        // Searching "crypto handler" should rank crypto_handler first because "crypto" has much higher IDF
        let results = index.search_symbols("crypto handler", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "crypto_handler");
    }
}
