use crate::context::graph::{CodeGraph, SymbolKind};
use crate::context::layers::LayerClassifier;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Configuration options for architecture documentation synthesis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureDocOptions {
    pub include_mermaid: bool,
    pub include_symbol_catalog: bool,
    pub write_to_file: bool,
}

impl Default for ArchitectureDocOptions {
    fn default() -> Self {
        Self {
            include_mermaid: true,
            include_symbol_catalog: true,
            write_to_file: false,
        }
    }
}

/// Report containing generated architecture documentation and write status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureDocReport {
    pub workspace_name: String,
    pub total_files: usize,
    pub total_symbols: usize,
    pub markdown_content: String,
    pub file_written: Option<String>,
}

pub struct ArchitectureDocSynthesizer;

impl ArchitectureDocSynthesizer {
    /// Synthesizes comprehensive architecture documentation from AST and graph analysis
    pub fn synthesize(
        workspace_root: &Path,
        options: ArchitectureDocOptions,
    ) -> Result<ArchitectureDocReport> {
        let mut graph = CodeGraph::new();
        let _ = graph.build_graph(workspace_root);

        let workspace_name = workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Workspace")
            .to_string();

        let mut layer_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut total_files = 0;
        let mut total_symbols = 0;

        let petgraph_ref = graph.graph();

        for node_idx in petgraph_ref.node_indices() {
            if let Some(node) = petgraph_ref.node_weight(node_idx) {
                if node.kind == SymbolKind::File {
                    total_files += 1;
                    let rel_path = node
                        .file_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&node.file_path)
                        .display()
                        .to_string();

                    let layer = LayerClassifier::classify_path(&node.file_path);
                    layer_files
                        .entry(layer.display_name().to_string())
                        .or_default()
                        .push(rel_path);
                } else {
                    total_symbols += 1;
                }
            }
        }

        let mut md = format!(
            "# 🏛️ Architecture Documentation: {}\n\n\
            > *Automatically synthesized by `minicode` AST & CodeGraph Engine*\n\n\
            ## 📊 System Overview\n\
            - **Workspace Root:** `{}`\n\
            - **Total Source Files:** {}\n\
            - **Total Analyzed Symbols:** {}\n\n",
            workspace_name,
            workspace_root.display(),
            total_files,
            total_symbols
        );

        // 1. Layer Matrix
        md.push_str("## 📐 Clean Architecture Layer Breakdown\n\n");
        md.push_str("| Layer | File Count | Primary Components |\n");
        md.push_str("| :--- | :--- | :--- |\n");

        for (layer, files) in &layer_files {
            let sample_files = if files.len() <= 3 {
                files.join(", ")
            } else {
                format!("{}, +{} more", files[..2].join(", "), files.len() - 2)
            };
            md.push_str(&format!(
                "| **{}** | {} | `{}` |\n",
                layer,
                files.len(),
                sample_files
            ));
        }
        md.push('\n');

        // 2. Mermaid Diagram
        if options.include_mermaid {
            md.push_str("## 🗺️ Architectural Component & Data Flow\n\n");
            md.push_str("```mermaid\nflowchart TD\n");
            md.push_str("    subgraph Presentation[\"🖥️ Presentation / UI Layer\"]\n");
            md.push_str("        UI[\"TUI Views & Prompts\"]\n");
            md.push_str("    end\n");
            md.push_str("    subgraph Service[\"⚙️ Service & Agent Layer\"]\n");
            md.push_str("        AgentLoop[\"Agent Loop & Tools\"]\n");
            md.push_str("    end\n");
            md.push_str("    subgraph Domain[\"🏛️ Domain / Context Core\"]\n");
            md.push_str("        Graph[\"CodeGraph & RepoMap\"]\n");
            md.push_str("        Invariants[\"Invariant & Smell Linters\"]\n");
            md.push_str("    end\n");
            md.push_str("    subgraph Data[\"💾 Data & Storage Layer\"]\n");
            md.push_str("        Store[\"Session Store & Index\"]\n");
            md.push_str("    end\n");
            md.push_str("    UI --> AgentLoop\n");
            md.push_str("    AgentLoop --> Graph\n");
            md.push_str("    AgentLoop --> Invariants\n");
            md.push_str("    Graph --> Store\n");
            md.push_str("```\n\n");
        }

        // 3. High-Centrality Symbol Registry
        if options.include_symbol_catalog {
            md.push_str("## 🔑 Core High-Centrality Symbols (PageRank)\n\n");
            let symbol_pr = graph.compute_symbol_pagerank(&[]);
            let mut top_symbols: Vec<_> = symbol_pr.into_iter().take(10).collect();
            top_symbols.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            if top_symbols.is_empty() {
                md.push_str("*(No symbols indexed)*\n\n");
            } else {
                md.push_str("| Symbol | Kind | Centrality (PageRank) |\n");
                md.push_str("| :--- | :--- | :--- |\n");
                for (sym, score) in top_symbols {
                    md.push_str(&format!(
                        "| `{}` | `{:?}` | `{:.4}` |\n",
                        sym.name, sym.kind, score
                    ));
                }
                md.push('\n');
            }
        }

        // 4. File Write
        let file_written = if options.write_to_file {
            let doc_path = workspace_root.join("ARCHITECTURE.md");
            fs::write(&doc_path, &md).map_err(|e| crate::error::ToolError::FileOp {
                path: doc_path.display().to_string(),
                source: e,
            })?;
            Some("ARCHITECTURE.md".to_string())
        } else {
            None
        };

        Ok(ArchitectureDocReport {
            workspace_name,
            total_files,
            total_symbols,
            markdown_content: md,
            file_written,
        })
    }
}
