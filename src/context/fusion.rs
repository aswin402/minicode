use crate::context::episodic::EpisodicMemory;
use crate::context::graph::CodeGraph;
use crate::context::hybrid::{HybridHit, HybridIndex};
use crate::context::semantic::SemanticIndex;
use crate::context::wiki::WikiManager;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// Individual ranked code declaration or snippet match.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FusedCodeHit {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub pagerank_score: f64,
    pub fused_score: f64,
    pub match_sources: Vec<String>,
}

/// AST call-graph connectivity and caller/callee context for a matched symbol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FusedCallGraphNode {
    pub symbol: String,
    pub file_path: String,
    pub incoming_callers: Vec<String>,
    pub outgoing_callees: Vec<String>,
}

/// Relevant architectural wiki document or decision record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FusedWikiHit {
    pub topic: String,
    pub title: String,
    pub relevance_score: f32,
    pub snippet: String,
}

/// Relevant cross-session episodic memory of past solved tasks or lessons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FusedEpisodeHit {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub relevance_score: f32,
    pub code_references: Vec<String>,
}

/// Synthesized, multi-modal knowledge package returned by `hybrid_retrieve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedKnowledgeBundle {
    pub query: String,
    pub primary_code_matches: Vec<FusedCodeHit>,
    pub call_graph_context: Vec<FusedCallGraphNode>,
    pub wiki_insights: Vec<FusedWikiHit>,
    pub episodic_memories: Vec<FusedEpisodeHit>,
    pub total_candidates_evaluated: usize,
    pub retrieval_duration_ms: u64,
}

/// Multi-Modal Knowledge Fusion Engine uniting AST CodeGraph, BM25, Semantic Embeddings,
/// Architectural Wiki, and Episodic Memory.
pub struct KnowledgeFusionEngine;

impl KnowledgeFusionEngine {
    /// Retrieves and synthesizes multi-modal context for a query across code, graph, wiki, and memory.
    pub fn retrieve(
        workspace_root: &Path,
        query: &str,
        limit: usize,
        include_graph: bool,
        include_wiki: bool,
        include_memory: bool,
    ) -> Result<FusedKnowledgeBundle> {
        let start = Instant::now();
        let query_trimmed = query.trim();

        // 1. Build and query HybridIndex (BM25 + Dense Semantic + PageRank)
        let mut hybrid_index = HybridIndex::new();
        let _ = hybrid_index.build_index(workspace_root);
        let raw_hits: Vec<HybridHit> = hybrid_index.search(query_trimmed, limit, true);
        let total_candidates_evaluated = raw_hits.len();

        let primary_code_matches: Vec<FusedCodeHit> = raw_hits
            .into_iter()
            .map(|hit| FusedCodeHit {
                file_path: hit.file_path,
                start_line: hit.start_line,
                end_line: hit.end_line,
                snippet: hit.snippet,
                symbol_name: hit.symbol_name,
                symbol_kind: hit.symbol_kind,
                pagerank_score: hit.pagerank_score,
                fused_score: hit.combined_score,
                match_sources: hit.match_sources,
            })
            .collect();

        // 2. Query AST Call Graph Topology for top matching symbols
        let mut call_graph_context = Vec::new();
        if include_graph {
            let mut graph = CodeGraph::new();
            if graph.build_graph(workspace_root).is_ok() {
                let mut inspected_symbols = HashSet::new();
                for code_hit in &primary_code_matches {
                    if let Some(ref sym) = code_hit.symbol_name {
                        if inspected_symbols.insert(sym.clone()) {
                            let (callers, callees) =
                                Self::extract_call_topology(&graph, sym, workspace_root);
                            if !callers.is_empty() || !callees.is_empty() {
                                call_graph_context.push(FusedCallGraphNode {
                                    symbol: sym.clone(),
                                    file_path: code_hit.file_path.clone(),
                                    incoming_callers: callers,
                                    outgoing_callees: callees,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 3. Query Workspace Architectural Wiki Entries
        let mut wiki_insights = Vec::new();
        if include_wiki {
            wiki_insights = Self::retrieve_wiki_insights(workspace_root, query_trimmed, 3);
        }

        // 4. Query Persistent Episodic Memories
        let mut episodic_memories = Vec::new();
        if include_memory {
            if let Ok(mem) = EpisodicMemory::load(workspace_root) {
                let scored = mem.search(query_trimmed, 3);
                for sc in scored {
                    episodic_memories.push(FusedEpisodeHit {
                        id: sc.item.id,
                        title: sc.item.title,
                        summary: sc.item.summary,
                        relevance_score: sc.score,
                        code_references: sc.item.code_references,
                    });
                }
            }
        }

        let retrieval_duration_ms = start.elapsed().as_millis() as u64;

        Ok(FusedKnowledgeBundle {
            query: query_trimmed.to_string(),
            primary_code_matches,
            call_graph_context,
            wiki_insights,
            episodic_memories,
            total_candidates_evaluated,
            retrieval_duration_ms,
        })
    }

    /// Extracts incoming callers and outgoing callees for a given symbol from the CodeGraph.
    fn extract_call_topology(
        graph: &CodeGraph,
        symbol_name: &str,
        _workspace_root: &Path,
    ) -> (Vec<String>, Vec<String>) {
        let mut callers = Vec::new();
        let mut callees = Vec::new();

        if let Some(nodes) = graph.name_to_nodes().get(symbol_name) {
            for &idx in nodes {
                // Incoming callers
                for neighbor in graph
                    .graph()
                    .neighbors_directed(idx, petgraph::Direction::Incoming)
                {
                    if let Some(node) = graph.graph().node_weight(neighbor) {
                        if !callers.contains(&node.name) && node.name != symbol_name {
                            callers.push(node.name.clone());
                        }
                    }
                }
                // Outgoing callees
                for neighbor in graph
                    .graph()
                    .neighbors_directed(idx, petgraph::Direction::Outgoing)
                {
                    if let Some(node) = graph.graph().node_weight(neighbor) {
                        if !callees.contains(&node.name) && node.name != symbol_name {
                            callees.push(node.name.clone());
                        }
                    }
                }
            }
        }

        (callers, callees)
    }

    /// Scans the workspace `.minicode/wiki` directory for articles matching the query.
    fn retrieve_wiki_insights(
        workspace_root: &Path,
        query: &str,
        max_results: usize,
    ) -> Vec<FusedWikiHit> {
        let dir = WikiManager::wiki_dir(workspace_root);
        if !dir.exists() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let query_vec = SemanticIndex::embed(query);
        let mut hits = Vec::new();

        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    let topic = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_string();

                    if topic == "index" {
                        continue;
                    }

                    if let Ok(content) = fs::read_to_string(&path) {
                        let title = content
                            .lines()
                            .find(|l| l.starts_with("# "))
                            .map(|l| l.trim_start_matches("# ").trim())
                            .unwrap_or(&topic)
                            .to_string();

                        // Compute hybrid semantic + keyword score
                        let doc_vec = SemanticIndex::embed(&content);
                        let sem_sim = SemanticIndex::cosine_similarity(&query_vec, &doc_vec);

                        let content_lower = content.to_lowercase();
                        let kw_match = if content_lower.contains(&query_lower)
                            || topic.to_lowercase().contains(&query_lower)
                        {
                            0.4
                        } else {
                            0.0
                        };

                        let relevance_score = (0.6 * sem_sim) + kw_match;

                        // First non-heading paragraph as preview snippet
                        let snippet = content
                            .lines()
                            .filter(|l| {
                                !l.starts_with('#') && !l.starts_with("---") && !l.trim().is_empty()
                            })
                            .take(2)
                            .collect::<Vec<&str>>()
                            .join(" ");

                        if relevance_score > 0.1 || kw_match > 0.0 {
                            hits.push(FusedWikiHit {
                                topic,
                                title,
                                relevance_score,
                                snippet,
                            });
                        }
                    }
                }
            }
        }

        hits.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(max_results);
        hits
    }
}

/// Formats a `FusedKnowledgeBundle` into an LLM-ready markdown knowledge briefing.
pub fn format_fused_bundle(bundle: &FusedKnowledgeBundle) -> String {
    let mut out = format!(
        "### 🌐 Multi-Modal Knowledge Retrieval: \"{}\"\n\
         *[Duration: {}ms | Evaluated: {} candidates across CodeGraph, Semantic Vectors, Wiki, & Memory]*\n\n",
        bundle.query, bundle.retrieval_duration_ms, bundle.total_candidates_evaluated
    );

    // 1. Primary Code Matches
    out.push_str("#### 📌 Primary Code Declarations & Snippets\n");
    if bundle.primary_code_matches.is_empty() {
        out.push_str("*No direct code declarations matched query.*\n\n");
    } else {
        for (i, hit) in bundle.primary_code_matches.iter().enumerate() {
            let sym_info = match (&hit.symbol_name, &hit.symbol_kind) {
                (Some(name), Some(kind)) => format!(" (`{} {}`)", kind, name),
                (Some(name), None) => format!(" (`{}`)", name),
                _ => String::new(),
            };
            out.push_str(&format!(
                "{}. **{}:{}-{}**{} — *Score: {:.4} | Sources: {} | PageRank: {:.3}*\n",
                i + 1,
                hit.file_path,
                hit.start_line,
                hit.end_line,
                sym_info,
                hit.fused_score,
                hit.match_sources.join(", "),
                hit.pagerank_score
            ));
            if !hit.snippet.is_empty() {
                out.push_str("   ```\n");
                for line in hit.snippet.lines().take(4) {
                    out.push_str(&format!("   {}\n", line));
                }
                out.push_str("   ```\n");
            }
        }
        out.push('\n');
    }

    // 2. Call Graph Topology
    if !bundle.call_graph_context.is_empty() {
        out.push_str("#### 🕸️ Call Graph & Dependency Topology\n");
        for node in &bundle.call_graph_context {
            out.push_str(&format!("• **`{}`** (`{}`)\n", node.symbol, node.file_path));
            if !node.incoming_callers.is_empty() {
                out.push_str(&format!(
                    "  - *Callers ({}):* {}\n",
                    node.incoming_callers.len(),
                    node.incoming_callers.join(", ")
                ));
            }
            if !node.outgoing_callees.is_empty() {
                out.push_str(&format!(
                    "  - *Callees ({}):* {}\n",
                    node.outgoing_callees.len(),
                    node.outgoing_callees.join(", ")
                ));
            }
        }
        out.push('\n');
    }

    // 3. Architectural Wiki Insights
    if !bundle.wiki_insights.is_empty() {
        out.push_str("#### 📖 Architectural Wiki & Governance Invariants\n");
        for wiki in &bundle.wiki_insights {
            out.push_str(&format!(
                "• **{}** (Topic: `{}` | Relevance: {:.0}%)\n",
                wiki.title,
                wiki.topic,
                wiki.relevance_score * 100.0
            ));
            if !wiki.snippet.is_empty() {
                out.push_str(&format!("  > {}\n", wiki.snippet));
            }
        }
        out.push('\n');
    }

    // 4. Episodic Memories
    if !bundle.episodic_memories.is_empty() {
        out.push_str("#### 🧠 Cross-Session Episodic Memory & Prior Lessons\n");
        for ep in &bundle.episodic_memories {
            out.push_str(&format!(
                "• **{}** (`{}` | Relevance: {:.0}%)\n",
                ep.title,
                ep.id,
                ep.relevance_score * 100.0
            ));
            out.push_str(&format!("  {}\n", ep.summary));
            if !ep.code_references.is_empty() {
                out.push_str(&format!(
                    "  *Referenced files:* {}\n",
                    ep.code_references.join(", ")
                ));
            }
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fusion_bundle_format_empty() {
        let bundle = FusedKnowledgeBundle {
            query: "test query".to_string(),
            primary_code_matches: Vec::new(),
            call_graph_context: Vec::new(),
            wiki_insights: Vec::new(),
            episodic_memories: Vec::new(),
            total_candidates_evaluated: 0,
            retrieval_duration_ms: 12,
        };

        let formatted = format_fused_bundle(&bundle);
        assert!(formatted.contains("test query"));
        assert!(formatted.contains("Primary Code Declarations"));
    }

    #[test]
    fn test_fusion_bundle_format_rich() {
        let bundle = FusedKnowledgeBundle {
            query: "transaction rollback".to_string(),
            primary_code_matches: vec![FusedCodeHit {
                file_path: "src/session/transaction.rs".to_string(),
                start_line: 50,
                end_line: 75,
                snippet: "pub fn rollback(&mut self) -> Result<()>".to_string(),
                symbol_name: Some("rollback".to_string()),
                symbol_kind: Some("fn".to_string()),
                pagerank_score: 0.042,
                fused_score: 0.098,
                match_sources: vec!["bm25".to_string(), "vector".to_string()],
            }],
            call_graph_context: vec![FusedCallGraphNode {
                symbol: "rollback".to_string(),
                file_path: "src/session/transaction.rs".to_string(),
                incoming_callers: vec!["rollback_transaction".to_string()],
                outgoing_callees: vec!["restore_snapshot".to_string()],
            }],
            wiki_insights: vec![FusedWikiHit {
                topic: "transactions".to_string(),
                title: "Atomic Transactions".to_string(),
                relevance_score: 0.85,
                snippet: "All mutations are checkpointed in write-ahead log.".to_string(),
            }],
            episodic_memories: vec![FusedEpisodeHit {
                id: "ep-001".to_string(),
                title: "Fixed Rollback TOCTOU".to_string(),
                summary: "Ensured target dirs exist before atomic restore.".to_string(),
                relevance_score: 0.79,
                code_references: vec!["src/session/transaction.rs".to_string()],
            }],
            total_candidates_evaluated: 15,
            retrieval_duration_ms: 25,
        };

        let formatted = format_fused_bundle(&bundle);
        assert!(formatted.contains("transaction rollback"));
        assert!(formatted.contains("src/session/transaction.rs"));
        assert!(formatted.contains("Call Graph"));
        assert!(formatted.contains("Atomic Transactions"));
        assert!(formatted.contains("Fixed Rollback TOCTOU"));
    }
}
