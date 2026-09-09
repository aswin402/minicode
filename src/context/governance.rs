//! Architectural governance, dependency graph analysis, and boundary enforcement.

use crate::context::arch_parser::{ArchParser, ImportReference};
use crate::context::arch_rules::{ArchRuleEngine, BoundaryViolation};
use crate::error::Result;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Backward-compatible alias for boundary violations.
#[allow(dead_code)]
pub type LayerViolation = BoundaryViolation;

/// Software architecture metrics measuring coupling and stability per module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleCouplingMetric {
    /// Canonical module name (e.g. "src/ui", "src/agent", "src/tools").
    pub module_name: String,
    /// Number of distinct external modules that depend on this module (Afferent Coupling).
    pub afferent_coupling: usize,
    /// Number of distinct external modules this module depends on (Efferent Coupling).
    pub efferent_coupling: usize,
    /// Robert C. Martin's Instability metric: I = Ce / (Ca + Ce). Range [0.0, 1.0].
    /// 0.0 indicates maximal stability (hard to change), 1.0 indicates maximal instability (flexible/leaf).
    pub instability: f64,
    /// Total lines of code in this module.
    pub total_loc: usize,
    /// Number of source files contained in this module.
    pub file_count: usize,
}

/// Comprehensive architectural health report for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureReport {
    /// Overall architectural health index (0 to 100).
    pub health_score: u32,
    /// Total number of source files analyzed.
    pub total_files: usize,
    /// Total lines of code scanned.
    pub total_loc: usize,
    /// Strongly connected components representing circular import cycles.
    pub circular_cycles: Vec<Vec<String>>,
    /// List of detected layer boundary violations.
    pub layer_violations: Vec<BoundaryViolation>,
    /// Coupling and instability metrics per module.
    pub coupling_metrics: Vec<ModuleCouplingMetric>,
    /// Source files exceeding complexity limits (>1,000 LOC).
    pub god_files: Vec<(String, usize)>,
    /// Files with excessive outgoing dependencies (>15 imports).
    pub fan_out_spikes: Vec<(String, usize)>,
}

impl ArchitectureReport {
    /// Formats the architectural analysis report as GitHub Flavored Markdown.
    #[must_use]
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🏛️ Architectural Health Report (Score: {}/100)\n\n",
            self.health_score
        );

        out.push_str(&format!(
            "📊 **Codebase Overview:** {} source files across {} lines of code.\n\n",
            self.total_files, self.total_loc
        ));

        // 1. Circular Dependencies (Acyclicity DAG check)
        if self.circular_cycles.is_empty() {
            out.push_str(
                "✔ **Acyclicity:** 100% DAG compliant — zero circular dependency cycles detected.\n\n",
            );
        } else {
            out.push_str("⚠️ **Circular Dependency Cycles Detected:**\n");
            for cycle in &self.circular_cycles {
                out.push_str(&format!("- 🔄 Cycle: `{}`\n", cycle.join(" ➔ ")));
            }
            out.push('\n');
        }

        // 2. Layer Isolation Boundaries
        if self.layer_violations.is_empty() {
            out.push_str("✔ **Layer Isolation:** All architectural module boundaries intact.\n\n");
        } else {
            out.push_str("⚠️ **Layer Boundary Violations:**\n");
            for v in &self.layer_violations {
                out.push_str(&format!(
                    "- 🚫 [{}] `{}` (line {}): `{}` ➔ `{}`\n  _{}_\n",
                    v.rule_id,
                    v.source_file,
                    v.line_number,
                    v.source_layer.display_name(),
                    v.target_layer.display_name(),
                    v.message
                ));
            }
            out.push('\n');
        }

        // 3. Module Instability & Coupling Matrix
        if !self.coupling_metrics.is_empty() {
            out.push_str("### 📐 Module Coupling & Instability Matrix\n\n");
            out.push_str(
                "| Module | Files | LOC | Afferent ($C_a$) | Efferent ($C_e$) | Instability ($I$) | Role |\n",
            );
            out.push_str("| :--- | :---: | :---: | :---: | :---: | :---: | :--- |\n");
            for m in &self.coupling_metrics {
                let role = if m.instability < 0.3 {
                    "🛡️ Highly Stable (Core Base)"
                } else if m.instability > 0.7 {
                    "🍃 Highly Flexible (Presentation/Leaf)"
                } else {
                    "⚖️ Balanced (Service Mediator)"
                };

                out.push_str(&format!(
                    "| `{}` | {} | {} | {} | {} | {:.2} | {} |\n",
                    m.module_name,
                    m.file_count,
                    m.total_loc,
                    m.afferent_coupling,
                    m.efferent_coupling,
                    m.instability,
                    role
                ));
            }
            out.push('\n');
        }

        // 4. God Files / Bloat Warnings
        if !self.god_files.is_empty() {
            out.push_str("⚠️ **High-Complexity Files (>1,000 LOC):**\n");
            for (path, loc) in &self.god_files {
                out.push_str(&format!("- 📄 `{}` ({} lines)\n", path, loc));
            }
            out.push('\n');
        }

        // 5. Fan-out Spikes
        if !self.fan_out_spikes.is_empty() {
            out.push_str("⚠️ **High Fan-Out Files (>15 Imports):**\n");
            for (path, count) in &self.fan_out_spikes {
                out.push_str(&format!("- 📦 `{}` ({} imports)\n", path, count));
            }
            out.push('\n');
        }

        out
    }

    /// Serializes report as JSON.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Architectural governor that scans workspaces, builds module dependency graphs, and audits boundaries.
pub struct ArchitectureGovernor;

impl ArchitectureGovernor {
    /// Scans workspace source files and validates architectural acyclicity, modularity, and layer boundaries.
    pub fn scan_workspace(workspace_root: &Path) -> Result<ArchitectureReport> {
        let mut total_files = 0;
        let mut total_loc = 0;
        let mut file_imports: Vec<(String, Vec<ImportReference>)> = Vec::new();
        let mut file_locs: HashMap<String, usize> = HashMap::new();

        let rel_files = crate::context::walker::WorkspaceWalker::new(workspace_root)
            .extensions(&["rs", "py", "ts", "tsx", "js", "jsx"])
            .collect_relative_files();

        for rel_path in rel_files {
            let full_path = workspace_root.join(&rel_path);
            let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if let Ok(content) = fs::read_to_string(&full_path) {
                let lines = content.lines().count();
                total_files += 1;
                total_loc += lines;
                file_locs.insert(rel_path.clone(), lines);

                let imports = ArchParser::extract_imports(&rel_path, &content, ext);
                file_imports.push((rel_path, imports));
            }
        }

        // 1. Layer boundary violations via ArchRuleEngine
        let rule_engine = ArchRuleEngine;
        let layer_violations = rule_engine.evaluate_workspace(&file_imports);

        // 2. Map files to module directories
        let mut module_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut module_loc: HashMap<String, usize> = HashMap::new();

        for (file, _) in &file_imports {
            let mod_name = ArchParser::get_top_level_module(file);
            let loc = file_locs.get(file).copied().unwrap_or(0);
            *module_loc.entry(mod_name.clone()).or_insert(0) += loc;
            module_files.entry(mod_name).or_default().push(file.clone());
        }

        // 3. Module-to-Module directed dependency graph
        let mut module_outgoing: HashMap<String, HashSet<String>> = HashMap::new();
        let mut module_incoming: HashMap<String, HashSet<String>> = HashMap::new();

        let mut mod_graph = DiGraph::<String, ()>::new();
        let mut mod_node_indices: HashMap<String, NodeIndex> = HashMap::new();

        for mod_name in module_files.keys() {
            let idx = mod_graph.add_node(mod_name.clone());
            mod_node_indices.insert(mod_name.clone(), idx);
            module_outgoing.entry(mod_name.clone()).or_default();
            module_incoming.entry(mod_name.clone()).or_default();
        }

        for (from_file, imports) in &file_imports {
            let from_mod = ArchParser::get_top_level_module(from_file);
            for imp in imports {
                let to_mod = ArchParser::get_top_level_module(&imp.canonical_module);
                if from_mod != to_mod && mod_node_indices.contains_key(&to_mod) {
                    module_outgoing
                        .entry(from_mod.clone())
                        .or_default()
                        .insert(to_mod.clone());
                    module_incoming
                        .entry(to_mod.clone())
                        .or_default()
                        .insert(from_mod.clone());

                    if let (Some(&from_idx), Some(&to_idx)) = (
                        mod_node_indices.get(&from_mod),
                        mod_node_indices.get(&to_mod),
                    ) {
                        // Avoid duplicate graph edges
                        if !mod_graph.contains_edge(from_idx, to_idx) {
                            mod_graph.add_edge(from_idx, to_idx, ());
                        }
                    }
                }
            }
        }

        // 4. Tarjan SCC cycle detection on module graph
        let mut circular_cycles = Vec::new();
        let sccs = tarjan_scc(&mod_graph);
        for scc in sccs {
            if scc.len() > 1 {
                let cycle_names: Vec<String> =
                    scc.into_iter().map(|idx| mod_graph[idx].clone()).collect();
                circular_cycles.push(cycle_names);
            }
        }

        // 5. File-level cycle detection within modules
        let mut file_graph = DiGraph::<String, ()>::new();
        let mut file_indices: HashMap<String, NodeIndex> = HashMap::new();

        for (file, _) in &file_imports {
            let idx = file_graph.add_node(file.clone());
            file_indices.insert(file.clone(), idx);
        }

        for (from_file, imports) in &file_imports {
            if let Some(&from_idx) = file_indices.get(from_file) {
                for imp in imports {
                    // Check if imported target corresponds to a known source file
                    for (to_file, _) in &file_imports {
                        if to_file != from_file
                            && (to_file == &imp.canonical_module
                                || to_file.starts_with(&imp.canonical_module))
                        {
                            if let Some(&to_idx) = file_indices.get(to_file) {
                                if !file_graph.contains_edge(from_idx, to_idx) {
                                    file_graph.add_edge(from_idx, to_idx, ());
                                }
                            }
                        }
                    }
                }
            }
        }

        let file_sccs = tarjan_scc(&file_graph);
        for scc in file_sccs {
            if scc.len() > 1 {
                let cycle: Vec<String> =
                    scc.into_iter().map(|idx| file_graph[idx].clone()).collect();
                if !circular_cycles.iter().any(|c| c == &cycle) {
                    circular_cycles.push(cycle);
                }
            }
        }

        // 6. Compute Module Coupling & Instability Metrics
        let mut coupling_metrics = Vec::new();
        for (mod_name, files) in &module_files {
            let ce = module_outgoing.get(mod_name).map_or(0, |s| s.len());
            let ca = module_incoming.get(mod_name).map_or(0, |s| s.len());
            let total_coupling = ca + ce;
            let instability = if total_coupling > 0 {
                ce as f64 / total_coupling as f64
            } else {
                0.0
            };

            let loc = module_loc.get(mod_name).copied().unwrap_or(0);
            coupling_metrics.push(ModuleCouplingMetric {
                module_name: mod_name.clone(),
                afferent_coupling: ca,
                efferent_coupling: ce,
                instability,
                total_loc: loc,
                file_count: files.len(),
            });
        }

        // Sort modules by LOC descending
        coupling_metrics.sort_by_key(|m| std::cmp::Reverse(m.total_loc));

        // 7. God files (>1,000 LOC)
        let mut god_files: Vec<(String, usize)> = file_locs
            .iter()
            .filter(|(_, &loc)| loc > 1000)
            .map(|(p, &l)| (p.clone(), l))
            .collect();
        god_files.sort_by_key(|(_, size)| std::cmp::Reverse(*size));

        // 8. High fan-out files (>15 imports)
        let mut fan_out_spikes: Vec<(String, usize)> = file_imports
            .iter()
            .filter(|(_, imps)| imps.len() > 15)
            .map(|(p, imps)| (p.clone(), imps.len()))
            .collect();
        fan_out_spikes.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        // 9. Composite Architecture Health Score
        let mut score = 100u32;
        score = score.saturating_sub((circular_cycles.len() as u32) * 25);
        score = score.saturating_sub((layer_violations.len() as u32) * 10);
        score = score.saturating_sub((god_files.len() as u32) * 5);
        score = score.saturating_sub((fan_out_spikes.len() as u32) * 3);

        Ok(ArchitectureReport {
            health_score: score.max(10),
            total_files,
            total_loc,
            circular_cycles,
            layer_violations,
            coupling_metrics,
            god_files,
            fan_out_spikes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_scan_clean_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let tools = src.join("tools");
        let ui = src.join("ui");

        fs::create_dir_all(&tools).unwrap();
        fs::create_dir_all(&ui).unwrap();

        fs::write(tools.join("mod.rs"), "pub fn helper() {}\n").unwrap();
        fs::write(
            ui.join("view.rs"),
            "use crate::tools::helper;\npub fn render() { helper(); }\n",
        )
        .unwrap();

        let report = ArchitectureGovernor::scan_workspace(dir.path()).unwrap();
        assert_eq!(report.total_files, 2);
        assert_eq!(report.circular_cycles.len(), 0);
        assert_eq!(report.layer_violations.len(), 0);
        assert!(report.health_score >= 90);
    }

    #[test]
    fn test_architecture_scan_detects_layer_violation() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let tools = src.join("tools");
        let ui = src.join("ui");

        fs::create_dir_all(&tools).unwrap();
        fs::create_dir_all(&ui).unwrap();

        fs::write(ui.join("view.rs"), "pub fn render() {}\n").unwrap();
        // Lower layer (tools) imports upper layer (ui)
        fs::write(
            tools.join("exec.rs"),
            "use crate::ui::view::render;\npub fn run() { render(); }\n",
        )
        .unwrap();

        let report = ArchitectureGovernor::scan_workspace(dir.path()).unwrap();
        assert_eq!(report.layer_violations.len(), 1);
        assert_eq!(report.layer_violations[0].rule_id, "R1_CORE_NO_UI");
        assert!(report.health_score < 100);
    }

    #[test]
    fn test_architecture_scan_detects_circular_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let a = src.join("mod_a");
        let b = src.join("mod_b");

        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(a.join("lib.rs"), "use crate::mod_b::lib::bar;\n").unwrap();
        fs::write(b.join("lib.rs"), "use crate::mod_a::lib::foo;\n").unwrap();

        let report = ArchitectureGovernor::scan_workspace(dir.path()).unwrap();
        assert!(!report.circular_cycles.is_empty());
        assert!(report.health_score <= 75);
    }
}
