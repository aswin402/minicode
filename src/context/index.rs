#![allow(dead_code)]

use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::error::Result;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    postings: HashMap<String, Vec<usize>>, // token -> Vec<symbol_idx>
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            postings: HashMap::new(),
        }
    }

    /// Splits an identifier (`camelCase`, `snake_case`, `SCREAMING_SNAKE`) into searchable tokens
    pub fn tokenize(text: &str) -> Vec<String> {
        let mut tokens = HashSet::new();
        let lower_full = text.to_lowercase();
        tokens.insert(lower_full);

        // Split by non-alphanumeric
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

        tokens.into_iter().filter(|t| t.len() >= 2).collect()
    }

    /// Builds the symbol index by scanning the workspace and extracting AST symbols
    pub fn build_index(&mut self, workspace_root: &Path) -> Result<()> {
        self.symbols.clear();
        self.postings.clear();

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
                    if matches!(ext, "rs" | "py" | "js" | "ts" | "jsx" | "tsx") {
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

        Ok(())
    }

    /// Adds a single symbol entry to the index
    pub fn add_symbol(&mut self, file_path: PathBuf, sym: SymbolDef) {
        let sym_idx = self.symbols.len();
        let tokens = Self::tokenize(&sym.name);

        for token in tokens {
            self.postings.entry(token).or_default().push(sym_idx);
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
    }

    /// Locates an exact or prefix match for a symbol name
    pub fn locate_symbol(&self, name: &str) -> Vec<SymbolMatch> {
        let query_lower = name.trim().to_lowercase();
        let mut results = Vec::new();

        for sym in &self.symbols {
            let sym_name_lower = sym.name.to_lowercase();
            if sym_name_lower == query_lower {
                let mut exact = sym.clone();
                exact.score = 100.0;
                results.push(exact);
            } else if sym_name_lower.starts_with(&query_lower) {
                let mut prefix = sym.clone();
                prefix.score = 50.0;
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

    /// Searches the symbol index with ranking boosts for declaration kinds and penalty for test files
    pub fn search_symbols(&self, query: &str, limit: usize) -> Vec<SymbolMatch> {
        let query_tokens = Self::tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        let mut scores: HashMap<usize, f64> = HashMap::new();

        for q_token in &query_tokens {
            if let Some(postings) = self.postings.get(q_token) {
                for &sym_idx in postings {
                    *scores.entry(sym_idx).or_insert(0.0) += 10.0;
                }
            } else {
                // Prefix match in vocabulary
                for (vocab_token, postings) in &self.postings {
                    if vocab_token.starts_with(q_token) {
                        for &sym_idx in postings {
                            *scores.entry(sym_idx).or_insert(0.0) += 4.0;
                        }
                    }
                }
            }
        }

        let mut ranked = Vec::new();
        for (sym_idx, base_score) in scores {
            if let Some(sym) = self.symbols.get(sym_idx) {
                let mut score = base_score;

                // Kind boosts
                match sym.kind.as_str() {
                    "struct" | "class" | "interface" | "trait" | "enum" => score += 3.0,
                    "function" => score += 2.0,
                    _ => {}
                }

                // Test file down-ranking
                let path_str = sym.file_path.to_string_lossy().to_lowercase();
                if path_str.contains("test") || path_str.contains("mock") {
                    score *= 0.5;
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
}
