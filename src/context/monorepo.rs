use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

/// Metadata representing an individual package/crate within a monorepo workspace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageInfo {
    pub name: String,
    pub root_path: String,
    pub package_type: String,
    pub internal_dependencies: Vec<String>,
    pub external_dependencies_count: usize,
    pub file_count: usize,
    pub symbol_count: usize,
}

/// Comprehensive report of the workspace monorepo architecture and package topology
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonorepoReport {
    pub workspace_type: String,
    pub packages: Vec<PackageInfo>,
    pub topological_order: Vec<String>,
    pub cross_package_cycles: Vec<Vec<String>>,
}

impl MonorepoReport {
    /// Formats the monorepo topology into a markdown scorecard with visual Mermaid diagrams
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# 🏢 Workspace Monorepo & Multi-Package Architecture Report\n\n\
            📊 **Workspace Topology Summary:**\n\
            - **Workspace Type:** {}\n\
            - **Discovered Packages:** {}\n\
            - **Topological Build Order:** {}\n\n",
            self.workspace_type,
            self.packages.len(),
            if self.topological_order.is_empty() {
                "N/A".to_string()
            } else {
                self.topological_order.join(" ➔ ")
            }
        );

        if !self.cross_package_cycles.is_empty() {
            out.push_str("⚠️ **Circular Package Dependencies Detected:**\n");
            for cycle in &self.cross_package_cycles {
                out.push_str(&format!("- ⟲ `{}`\n", cycle.join(" ➔ ")));
            }
            out.push('\n');
        }

        // Package Breakdown Table
        out.push_str("### 📦 Package Inventory & Boundary Matrix\n\n");
        out.push_str("| Package Name | Path | Type | Internal Deps | External Deps |\n");
        out.push_str("| :--- | :--- | :--- | :--- | :--- |\n");

        for pkg in &self.packages {
            let int_deps = if pkg.internal_dependencies.is_empty() {
                "*(none)*".to_string()
            } else {
                format!("`{}`", pkg.internal_dependencies.join("`, `"))
            };

            out.push_str(&format!(
                "| **`{}`** | `{}` | {} | {} | {} |\n",
                pkg.name,
                pkg.root_path,
                pkg.package_type,
                int_deps,
                pkg.external_dependencies_count
            ));
        }
        out.push('\n');

        // Mermaid Architecture Flowchart
        if self.packages.len() > 1 {
            out.push_str("### 🗺️ Cross-Package Dependency Graph\n\n");
            out.push_str("```mermaid\nflowchart TD\n");
            for pkg in &self.packages {
                let clean_id = pkg.name.replace('-', "_");
                out.push_str(&format!("    {}[\"📦 {}\"]\n", clean_id, pkg.name));
            }
            for pkg in &self.packages {
                let from_id = pkg.name.replace('-', "_");
                for dep in &pkg.internal_dependencies {
                    let to_id = dep.replace('-', "_");
                    out.push_str(&format!("    {} --> {}\n", from_id, to_id));
                }
            }
            out.push_str("```\n\n");
        }

        out
    }
}

pub struct MonorepoOrchestrator;

impl MonorepoOrchestrator {
    /// Analyzes the workspace root to discover monorepo packages, manifests, and cross-package links
    pub fn analyze_workspace(
        workspace_root: &Path,
        _include_external: bool,
        target_package: Option<&str>,
    ) -> Result<MonorepoReport> {
        let mut packages = Vec::new();
        let mut workspace_type = "Single Package / Root".to_string();

        // 1. Check Cargo Workspace
        let root_cargo = workspace_root.join("Cargo.toml");
        if root_cargo.exists() {
            if let Ok(content) = fs::read_to_string(&root_cargo) {
                if content.contains("[workspace]") {
                    workspace_type = "Cargo Multi-Crate Workspace".to_string();
                    packages = Self::discover_cargo_workspace(workspace_root, &content);
                }
            }
        }

        // 2. Check npm/pnpm Workspace
        if packages.is_empty() {
            let root_pkg_json = workspace_root.join("package.json");
            if root_pkg_json.exists() {
                if let Ok(content) = fs::read_to_string(&root_pkg_json) {
                    if content.contains("\"workspaces\"") {
                        workspace_type = "npm/pnpm Monorepo".to_string();
                        packages = Self::discover_npm_workspace(workspace_root, &content);
                    }
                }
            }
        }

        // 3. Fallback: Single Root Project
        if packages.is_empty() {
            let root_name = workspace_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("root")
                .to_string();

            packages.push(PackageInfo {
                name: root_name,
                root_path: ".".to_string(),
                package_type: "Standalone Root Project".to_string(),
                internal_dependencies: Vec::new(),
                external_dependencies_count: 0,
                file_count: 1,
                symbol_count: 1,
            });
        }

        // Filter by target package if specified
        if let Some(target) = target_package {
            packages.retain(|p| p.name == target || p.root_path == target);
        }

        // Resolve internal dependencies & topological sort
        let package_names: HashSet<String> = packages.iter().map(|p| p.name.clone()).collect();
        for pkg in &mut packages {
            pkg.internal_dependencies
                .retain(|dep| package_names.contains(dep));
        }

        let (topological_order, cross_package_cycles) = Self::compute_topology(&packages);

        Ok(MonorepoReport {
            workspace_type,
            packages,
            topological_order,
            cross_package_cycles,
        })
    }

    fn discover_cargo_workspace(workspace_root: &Path, _content: &str) -> Vec<PackageInfo> {
        let mut packages = Vec::new();
        let mut sub_cargos = Vec::new();

        // Search for all nested Cargo.toml files
        let walker = ignore::WalkBuilder::new(workspace_root)
            .max_depth(Some(4))
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.file_name().and_then(|n| n.to_str()) == Some("Cargo.toml")
                && path != workspace_root.join("Cargo.toml")
            {
                sub_cargos.push(path.to_path_buf());
            }
        }

        for cargo_path in &sub_cargos {
            if let Ok(c) = fs::read_to_string(cargo_path) {
                let pkg_dir = cargo_path.parent().unwrap_or(workspace_root);
                let rel_path = pkg_dir
                    .strip_prefix(workspace_root)
                    .unwrap_or(pkg_dir)
                    .display()
                    .to_string();

                let name = Self::extract_toml_value(&c, "name").unwrap_or_else(|| rel_path.clone());
                let (internal_deps, ext_count) = Self::extract_cargo_deps(&c);

                packages.push(PackageInfo {
                    name,
                    root_path: rel_path,
                    package_type: "Cargo Crate".to_string(),
                    internal_dependencies: internal_deps,
                    external_dependencies_count: ext_count,
                    file_count: 0,
                    symbol_count: 0,
                });
            }
        }

        packages
    }

    fn discover_npm_workspace(workspace_root: &Path, _content: &str) -> Vec<PackageInfo> {
        let mut packages = Vec::new();
        let walker = ignore::WalkBuilder::new(workspace_root)
            .max_depth(Some(4))
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.file_name().and_then(|n| n.to_str()) == Some("package.json")
                && path != workspace_root.join("package.json")
                && !path.to_string_lossy().contains("node_modules")
            {
                if let Ok(c) = fs::read_to_string(path) {
                    let pkg_dir = path.parent().unwrap_or(workspace_root);
                    let rel_path = pkg_dir
                        .strip_prefix(workspace_root)
                        .unwrap_or(pkg_dir)
                        .display()
                        .to_string();

                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&c) {
                        let name = val
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or(&rel_path)
                            .to_string();

                        let mut internal_deps = Vec::new();
                        let mut ext_count = 0;

                        if let Some(deps) = val.get("dependencies").and_then(|d| d.as_object()) {
                            for (dep, _) in deps {
                                internal_deps.push(dep.clone());
                                ext_count += 1;
                            }
                        }

                        packages.push(PackageInfo {
                            name,
                            root_path: rel_path,
                            package_type: "npm/pnpm Package".to_string(),
                            internal_dependencies: internal_deps,
                            external_dependencies_count: ext_count,
                            file_count: 0,
                            symbol_count: 0,
                        });
                    }
                }
            }
        }

        packages
    }

    fn extract_toml_value(content: &str, key: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(key) && trimmed.contains('=') {
                let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
                if parts.len() == 2 {
                    return Some(
                        parts[1]
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string(),
                    );
                }
            }
        }
        None
    }

    fn extract_cargo_deps(content: &str) -> (Vec<String>, usize) {
        let mut in_deps = false;
        let mut internal_deps = Vec::new();
        let mut ext_count = 0;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_deps = trimmed == "[dependencies]"
                    || trimmed == "[dev-dependencies]"
                    || trimmed.starts_with("[dependencies.");
                continue;
            }

            if in_deps && !trimmed.starts_with('#') && trimmed.contains('=') {
                let dep_name = trimmed.split('=').next().unwrap_or("").trim().to_string();
                if !dep_name.is_empty() {
                    if trimmed.contains("path =") {
                        internal_deps.push(dep_name);
                    } else {
                        ext_count += 1;
                    }
                }
            }
        }

        (internal_deps, ext_count)
    }

    fn compute_topology(packages: &[PackageInfo]) -> (Vec<String>, Vec<Vec<String>>) {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        for pkg in packages {
            in_degree.entry(pkg.name.clone()).or_insert(0);
            adj.entry(pkg.name.clone()).or_default();
        }

        for pkg in packages {
            for dep in &pkg.internal_dependencies {
                adj.entry(dep.clone()).or_default().push(pkg.name.clone());
                *in_degree.entry(pkg.name.clone()).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(name, _)| name.clone())
            .collect();

        let mut order = Vec::new();
        while let Some(curr) = queue.pop_front() {
            order.push(curr.clone());
            if let Some(neighbors) = adj.get(&curr) {
                for n in neighbors {
                    if let Some(deg) = in_degree.get_mut(n) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(n.clone());
                        }
                    }
                }
            }
        }

        let mut cycles = Vec::new();
        if order.len() < packages.len() {
            let unvisited: Vec<String> = packages
                .iter()
                .filter(|p| !order.contains(&p.name))
                .map(|p| p.name.clone())
                .collect();
            if !unvisited.is_empty() {
                cycles.push(unvisited);
            }
        }

        (order, cycles)
    }
}
