use crate::context::hybrid::{HybridHit, HybridIndex};
use crate::context::layers::LayerClassifier;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A refined, cross-encoder reranked search result hit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RerankedHit {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub snippet: String,
    pub original_rrf_score: f64,
    pub rerank_score: f64,
    pub match_reasons: Vec<String>,
}

/// Comprehensive semantic search result set with cross-encoder rankings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticSearchResult {
    pub query: String,
    pub total_candidates: usize,
    pub hits: Vec<RerankedHit>,
}

impl SemanticSearchResult {
    /// Formats the semantic search results into a clean markdown document
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🔍 Semantic Code Search Results for `{}`\n\n\
            📊 Evaluated {} hybrid candidates, returning top {} reranked matches.\n\n",
            self.query,
            self.total_candidates,
            self.hits.len()
        );

        if self.hits.is_empty() {
            out.push_str("⚠️ No matching code snippets or AST symbols found for this query.\n");
            return out;
        }

        for (idx, hit) in self.hits.iter().enumerate() {
            let symbol_title =
                if let (Some(name), Some(kind)) = (&hit.symbol_name, &hit.symbol_kind) {
                    format!("`{}` ({})", name, kind)
                } else {
                    "Code Block".to_string()
                };

            out.push_str(&format!(
                "### {}. {} — `{}:lines {}-{}` (Confidence: {:.1}%)\n\n",
                idx + 1,
                symbol_title,
                hit.file_path,
                hit.start_line,
                hit.end_line,
                hit.rerank_score * 100.0
            ));

            if !hit.match_reasons.is_empty() {
                out.push_str(&format!(
                    "🎯 **Matching Factors:** {}\n\n",
                    hit.match_reasons.join(" • ")
                ));
            }

            if !hit.snippet.is_empty() {
                let ext = hit.file_path.split('.').next_back().unwrap_or("text");
                out.push_str(&format!("```{}\n{}\n```\n\n", ext, hit.snippet.trim()));
            }
        }

        out
    }
}

pub struct CrossEncoderReranker;

impl CrossEncoderReranker {
    /// Executes hybrid retrieval (BM25 + Dense Vectors + PageRank) followed by cross-encoder intent reranking
    pub fn search_and_rerank(
        workspace_root: &Path,
        query: &str,
        limit: usize,
        target_layer: Option<&str>,
    ) -> Result<SemanticSearchResult> {
        let mut hybrid_index = HybridIndex::new();
        hybrid_index.build_index(workspace_root)?;

        let pool_size = limit.max(5) * 4;
        let candidates = hybrid_index.search(query, pool_size, true);
        let total_candidates = candidates.len();

        let query_tokens: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut reranked: Vec<RerankedHit> = candidates
            .into_iter()
            .map(|hit| Self::score_hit(hit, query, &query_tokens, target_layer))
            .collect();

        // Sort descending by rerank score
        reranked.sort_by(|a, b| {
            b.rerank_score
                .partial_cmp(&a.rerank_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        reranked.truncate(limit.max(1));

        Ok(SemanticSearchResult {
            query: query.to_string(),
            total_candidates,
            hits: reranked,
        })
    }

    fn score_hit(
        hit: HybridHit,
        raw_query: &str,
        query_tokens: &[String],
        target_layer: Option<&str>,
    ) -> RerankedHit {
        let mut score = hit.combined_score * 0.35;
        let mut reasons = Vec::new();

        let lower_snippet = hit.snippet.to_lowercase();
        let lower_path = hit.file_path.to_lowercase();
        let lower_symbol = hit.symbol_name.as_deref().unwrap_or("").to_lowercase();
        let lower_query = raw_query.to_lowercase();

        // 1. Exact query phrase match
        if lower_snippet.contains(&lower_query) || lower_symbol.contains(&lower_query) {
            score += 0.30;
            reasons.push("Exact phrase match".to_string());
        }

        // 2. Token overlap ratio
        if !query_tokens.is_empty() {
            let matched_tokens = query_tokens
                .iter()
                .filter(|&t| {
                    lower_snippet.contains(t) || lower_symbol.contains(t) || lower_path.contains(t)
                })
                .count();

            let ratio = matched_tokens as f64 / query_tokens.len() as f64;
            score += ratio * 0.25;
            if ratio > 0.6 {
                reasons.push(format!("{:.0}% term coverage", ratio * 100.0));
            }
        }

        // 3. Symbol name match bonus
        if let Some(ref sym) = hit.symbol_name {
            for token in query_tokens {
                if sym.to_lowercase().contains(token) {
                    score += 0.15;
                    reasons.push(format!("Symbol identifier `{}` matches query", sym));
                    break;
                }
            }
        }

        // 4. Target Layer Alignment
        let layer = LayerClassifier::classify_path(Path::new(&hit.file_path));
        if let Some(tl) = target_layer {
            if layer
                .display_name()
                .to_lowercase()
                .contains(&tl.to_lowercase())
                || layer.badge().to_lowercase().contains(&tl.to_lowercase())
            {
                score += 0.20;
                reasons.push(format!("Target layer {} matched", layer.badge()));
            }
        }

        // 5. Centrality boost
        if hit.pagerank_score > 0.03 {
            score += hit.pagerank_score * 0.5;
            reasons.push(format!(
                "High architectural centrality ({:.3})",
                hit.pagerank_score
            ));
        }

        let final_score = score.clamp(0.05, 1.0);

        RerankedHit {
            file_path: hit.file_path,
            start_line: hit.start_line,
            end_line: hit.end_line,
            symbol_name: hit.symbol_name,
            symbol_kind: hit.symbol_kind,
            snippet: hit.snippet,
            original_rrf_score: hit.combined_score,
            rerank_score: final_score,
            match_reasons: reasons,
        }
    }
}
