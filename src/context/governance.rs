use crate::error::Result;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayerViolation {
    pub from_module: String,
    pub to_module: String,
    pub file_path: String,
    pub rule: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureReport {
    pub health_score: u32,
    pub total_files: usize,
    pub total_loc: usize,
    pub circular_cycles: Vec<Vec<String>>,
    pub layer_violations: Vec<LayerViolation>,
    pub god_files: Vec<(String, usize)>,
    pub fan_out_spikes: Vec<(String, usize)>,
}

impl ArchitectureReport {
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🏛️ Architectural Health Report (Score: {}/100)\n\n",
            self.health_score
        );

        out.push_str(&format!(
            "📊 **Codebase Metrics:** {} source files, {} total lines of code.\n\n",
            self.total_files, self.total_loc
        ));

        if self.circular_cycles.is_empty() {
            out.push_str("✔ **Acyclicity:** 100% DAG compliant — zero circular dependency cycles detected.\n\n");
        } else {
            out.push_str("⚠️ **Circular Dependency Cycles Detected:**\n");
            for cycle in &self.circular_cycles {
                out.push_str(&format!("- 🔄 Cycle: `{}`\n", cycle.join(" -> ")));
            }
            out.push('\n');
        }

        if self.layer_violations.is_empty() {
            out.push_str("✔ **Layer Isolation:** All architectural module boundaries intact.\n\n");
        } else {
            out.push_str("⚠️ **Layer Boundary Violations:**\n");
            for v in &self.layer_violations {
                out.push_str(&format!(
                    "- 🚫 `{}` in `{}`: `{}`\n",
                    v.from_module, v.file_path, v.rule
                ));
            }
            out.push('\n');
        }

        if !self.god_files.is_empty() {
            out.push_str("⚠️ **High-Complexity / God Files (>1,000 LOC):**\n");
            for (path, loc) in &self.god_files {
                out.push_str(&format!("- 📄 `{}` ({} lines)\n", path, loc));
            }
            out.push('\n');
        }

        if !self.fan_out_spikes.is_empty() {
            out.push_str("⚠️ **High Fan-Out Modules (>10 Imports):**\n");
            for (path, count) in &self.fan_out_spikes {
                out.push_str(&format!("- 📦 `{}` ({} imports)\n", path, count));
            }
            out.push('\n');
        }

        out
    }
}

pub struct ArchitectureGovernor;

impl ArchitectureGovernor {
    /// Scans workspace source files and validates architectural acyclicity, modularity, and layer boundaries
    pub fn scan_workspace(workspace_root: &Path) -> Result<ArchitectureReport> {
        let mut total_files = 0;
        let mut total_loc = 0;
        let mut file_imports: HashMap<String, HashSet<String>> = HashMap::new();
        let mut file_locs: HashMap<String, usize> = HashMap::new();

        let rel_files = crate::context::walker::WorkspaceWalker::new(workspace_root)
            .extensions(&["rs", "py", "ts", "js"])
            .collect_relative_files();

        for rel_path in rel_files {
            let full_path = workspace_root.join(&rel_path);
            let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if let Ok(content) = fs::read_to_string(&full_path) {
                let lines = content.lines().count();
                total_files += 1;
                total_loc += lines;
                file_locs.insert(rel_path.clone(), lines);

                let imports = Self::extract_imports(&rel_path, &content, ext);
                file_imports.insert(rel_path, imports);
            }
        }

        // Build Petgraph DiGraph for SCC Cycle Detection
        let mut graph = DiGraph::<String, ()>::new();
        let mut node_indices: HashMap<String, NodeIndex> = HashMap::new();

        for file in file_imports.keys() {
            let idx = graph.add_node(file.clone());
            node_indices.insert(file.clone(), idx);
        }

        for (from_file, imports) in &file_imports {
            if let Some(&from_idx) = node_indices.get(from_file) {
                for imp in imports {
                    // Try exact file match or module prefix match
                    for to_file in file_imports.keys() {
                        if to_file != from_file
                            && (to_file == imp || to_file.starts_with(imp) || imp.contains(to_file))
                        {
                            if let Some(&to_idx) = node_indices.get(to_file) {
                                graph.add_edge(from_idx, to_idx, ());
                            }
                        }
                    }
                }
            }
        }

        // Tarjan SCC Cycle Detection
        let sccs = tarjan_scc(&graph);
        let mut circular_cycles = Vec::new();
        for scc in sccs {
            if scc.len() > 1 {
                let cycle_names: Vec<String> =
                    scc.into_iter().map(|idx| graph[idx].clone()).collect();
                circular_cycles.push(cycle_names);
            }
        }

        // Layer Boundary Violations
        let mut layer_violations = Vec::new();
        for (file, imports) in &file_imports {
            let module_layer = Self::get_module_layer(file);
            for imp in imports {
                if let Some(violation_rule) = Self::check_forbidden_layer(&module_layer, imp) {
                    layer_violations.push(LayerViolation {
                        from_module: module_layer.clone(),
                        to_module: imp.clone(),
                        file_path: file.clone(),
                        rule: violation_rule,
                    });
                }
            }
        }

        // God files (>1,000 LOC)
        let mut god_files: Vec<(String, usize)> = file_locs
            .iter()
            .filter(|(_, &loc)| loc > 1000)
            .map(|(p, &l)| (p.clone(), l))
            .collect();
        god_files.sort_by(|a, b| b.1.cmp(&a.1));

        // High Fan-out modules (>10 imports)
        let mut fan_out_spikes: Vec<(String, usize)> = file_imports
            .iter()
            .filter(|(_, imps)| imps.len() > 10)
            .map(|(p, imps)| (p.clone(), imps.len()))
            .collect();
        fan_out_spikes.sort_by(|a, b| b.1.cmp(&a.1));

        // Compute 0-100 Modularity Health Score
        let mut score = 100u32;
        score = score.saturating_sub((circular_cycles.len() as u32) * 20);
        score = score.saturating_sub((layer_violations.len() as u32) * 10);
        score = score.saturating_sub((god_files.len() as u32) * 5);
        score = score.saturating_sub((fan_out_spikes.len() as u32) * 3);

        Ok(ArchitectureReport {
            health_score: score.max(10),
            total_files,
            total_loc,
            circular_cycles,
            layer_violations,
            god_files,
            fan_out_spikes,
        })
    }

    fn extract_imports(_rel_path: &str, content: &str, ext: &str) -> HashSet<String> {
        let mut imports = HashSet::new();
        match ext {
            "rs" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("use crate::") || trimmed.starts_with("use super::") {
                        let sub = trimmed
                            .trim_start_matches("use crate::")
                            .trim_start_matches("use super::")
                            .trim_end_matches(';');
                        if let Some(mod_name) = sub.split("::").next() {
                            imports.insert(format!("src/{}", mod_name));
                        }
                    }
                }
            }
            "py" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                        let parts: Vec<&str> = trimmed.split_whitespace().collect();
                        if parts.len() >= 2 {
                            imports.insert(parts[1].replace('.', "/"));
                        }
                    }
                }
            }
            "ts" | "js" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if (trimmed.starts_with("import ") || trimmed.starts_with("require("))
                        && trimmed.contains("from ")
                    {
                        if let Some(start) = trimmed.find('"').or_else(|| trimmed.find('\'')) {
                            let rest = &trimmed[start + 1..];
                            if let Some(end) = rest.find('"').or_else(|| rest.find('\'')) {
                                let imp = &rest[..end];
                                imports.insert(imp.to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        imports
    }

    fn get_module_layer(file_path: &str) -> String {
        let parts: Vec<&str> = file_path.split('/').collect();
        if parts.len() > 1 {
            format!("{}/{}", parts[0], parts[1])
        } else {
            file_path.to_string()
        }
    }

    fn check_forbidden_layer(from_layer: &str, to_import: &str) -> Option<String> {
        // Strict architectural boundaries:
        // 1. Lower layer (tools, context, git, lsp, sandbox, session) cannot depend on upper layer (ui, app, main)
        let lower_layers = [
            "src/tools",
            "src/context",
            "src/git",
            "src/lsp",
            "src/sandbox",
            "src/session",
        ];
        let upper_layers = ["src/ui", "src/app", "src/main"];

        for lower in lower_layers {
            if from_layer.starts_with(lower) {
                for upper in upper_layers {
                    if to_import.starts_with(upper) {
                        return Some(format!(
                            "Layer Boundary Violation: Core/Tool layer `{}` cannot import from Presentation layer `{}`",
                            from_layer, upper
                        ));
                    }
                }
            }
        }
        None
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
            "use crate::tools::mod;\npub fn render() {}\n",
        )
        .unwrap();

        let report = ArchitectureGovernor::scan_workspace(dir.path()).unwrap();
        assert!(report.health_score >= 80);
        assert!(report.circular_cycles.is_empty());
        assert!(report.layer_violations.is_empty());
    }

    #[test]
    fn test_architecture_layer_violation_detection() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let tools = src.join("tools");
        let ui = src.join("ui");

        fs::create_dir_all(&tools).unwrap();
        fs::create_dir_all(&ui).unwrap();

        // Illegal: tools importing from ui
        fs::write(
            tools.join("mod.rs"),
            "use crate::ui::view;\npub fn helper() {}\n",
        )
        .unwrap();
        fs::write(ui.join("view.rs"), "pub fn render() {}\n").unwrap();

        let report = ArchitectureGovernor::scan_workspace(dir.path()).unwrap();
        assert_eq!(report.layer_violations.len(), 1);
        assert!(report.layer_violations[0]
            .rule
            .contains("Layer Boundary Violation"));
    }
}
