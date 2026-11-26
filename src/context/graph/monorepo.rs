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
    /// Matched `CODEOWNERS` owners for this package root (last-matching-rule-wins).
    #[serde(default)]
    pub owners: Vec<String>,
    /// Number of commits touching files inside this package in the last 30 days.
    #[serde(default)]
    pub recent_commits_30d: usize,
}

/// Comprehensive report of the workspace monorepo architecture and package topology
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonorepoReport {
    pub workspace_type: String,
    pub packages: Vec<PackageInfo>,
    pub topological_order: Vec<String>,
    pub cross_package_cycles: Vec<Vec<String>>,
    /// Per-language Code / Comments / Blanks LOC breakdown computed via byte-level FSM (`tokei` model).
    #[serde(default)]
    pub code_stats: Option<crate::context::workspace_profile::CodeStatsReport>,
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
        out.push_str("### 📦 Package Inventory & Service Briefing Matrix\n\n");
        out.push_str(
            "| Package Name | Path | Type | Owners | 30d Commits | Files | Internal Deps | External Deps |\n",
        );
        out.push_str("| :--- | :--- | :--- | :--- | ---: | ---: | :--- | ---: |\n");

        for pkg in &self.packages {
            let int_deps = if pkg.internal_dependencies.is_empty() {
                "*(none)*".to_string()
            } else {
                format!("`{}`", pkg.internal_dependencies.join("`, `"))
            };
            let owners_str = if pkg.owners.is_empty() {
                "*(unassigned)*".to_string()
            } else {
                pkg.owners.join(", ")
            };

            out.push_str(&format!(
                "| **`{}`** | `{}` | {} | {} | {} | {} | {} | {} |\n",
                pkg.name,
                pkg.root_path,
                pkg.package_type,
                owners_str,
                pkg.recent_commits_30d,
                pkg.file_count,
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
                let clean_id = sanitize_mermaid_id(&pkg.name);
                out.push_str(&format!("    {}[\"📦 {}\"]\n", clean_id, pkg.name));
            }
            for pkg in &self.packages {
                let from_id = sanitize_mermaid_id(&pkg.name);
                for dep in &pkg.internal_dependencies {
                    let to_id = sanitize_mermaid_id(dep);
                    out.push_str(&format!("    {} --> {}\n", from_id, to_id));
                }
            }
            out.push_str("```\n\n");
        }

        if let Some(ref stats) = self.code_stats {
            out.push_str(&stats.format_markdown());
        }

        out
    }
}

fn sanitize_mermaid_id(name: &str) -> String {
    let mut id: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if id.is_empty() || id.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        id.insert_str(0, "pkg_");
    }
    id
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
                file_count: 0,
                symbol_count: 0,
                owners: Vec::new(),
                recent_commits_30d: 0,
            });
        }

        // Resolve internal vs external dependencies & topological sort before target filtering
        let package_names: HashSet<String> = packages.iter().map(|p| p.name.clone()).collect();
        for pkg in &mut packages {
            let total_deps = pkg.internal_dependencies.len();
            pkg.internal_dependencies
                .retain(|dep| package_names.contains(dep) && dep != &pkg.name);
            pkg.external_dependencies_count =
                total_deps.saturating_sub(pkg.internal_dependencies.len());
        }

        // Enrich local service briefings with CODEOWNERS, real file counts, and single-pass 30d git churn
        let codeowners = CodeOwnersMatcher::load(workspace_root);
        let churn_by_file = Self::collect_recent_git_churn_30d(workspace_root);
        let all_files =
            crate::context::walker::WorkspaceWalker::new(workspace_root).collect_relative_files();

        for pkg in &mut packages {
            let norm_root = pkg.root_path.trim_matches('/').replace('\\', "/");
            let is_root = norm_root.is_empty() || norm_root == ".";
            let prefix = if is_root {
                String::new()
            } else {
                format!("{norm_root}/")
            };

            pkg.file_count = all_files
                .iter()
                .filter(|f| {
                    let nf = f.replace('\\', "/");
                    is_root || nf.starts_with(&prefix)
                })
                .count();

            // Match owners for package directory (and fallback to a sample path inside it)
            if let Some(ref matcher) = codeowners {
                let probe = if is_root {
                    "README.md".to_string()
                } else {
                    format!("{norm_root}/")
                };
                pkg.owners = matcher.owners_for(&probe);
            }

            // Count unique commits touching files under this package root
            let mut touched_commits: HashSet<usize> = HashSet::new();
            for (rel_file, commit_ids) in &churn_by_file {
                if is_root || rel_file.starts_with(&prefix) {
                    touched_commits.extend(commit_ids.iter().copied());
                }
            }
            pkg.recent_commits_30d = touched_commits.len();
        }

        let (topological_order, cross_package_cycles) = Self::compute_topology(&packages);

        // Filter by target package if specified
        if let Some(target) = target_package {
            packages.retain(|p| p.name == target || p.root_path == target);
        }

        let code_stats = Some(
            crate::context::workspace_profile::WorkspaceProfile::compute_code_stats(workspace_root),
        );

        Ok(MonorepoReport {
            workspace_type,
            packages,
            topological_order,
            cross_package_cycles,
            code_stats,
        })
    }

    fn discover_cargo_workspace(workspace_root: &Path, _content: &str) -> Vec<PackageInfo> {
        let mut packages = Vec::new();
        let mut sub_cargos = Vec::new();

        // Search for all nested Cargo.toml files
        let mut builder = ignore::WalkBuilder::new(workspace_root);
        builder.max_depth(Some(4)).filter_entry(|entry| {
            if let Some(name) = entry.file_name().to_str() {
                !crate::context::walker::is_excluded_dir_name(name)
            } else {
                true
            }
        });
        let walker = builder.build();

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
                let all_deps = Self::extract_cargo_deps(&c);

                packages.push(PackageInfo {
                    name,
                    root_path: rel_path,
                    package_type: "Cargo Crate".to_string(),
                    internal_dependencies: all_deps,
                    external_dependencies_count: 0,
                    file_count: 0,
                    symbol_count: 0,
                    owners: Vec::new(),
                    recent_commits_30d: 0,
                });
            }
        }

        packages
    }

    fn discover_npm_workspace(workspace_root: &Path, _content: &str) -> Vec<PackageInfo> {
        let mut packages = Vec::new();
        let mut builder = ignore::WalkBuilder::new(workspace_root);
        builder.max_depth(Some(4)).filter_entry(|entry| {
            if let Some(name) = entry.file_name().to_str() {
                !crate::context::walker::is_excluded_dir_name(name)
            } else {
                true
            }
        });
        let walker = builder.build();

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

                        let mut all_deps = Vec::new();
                        for section in ["dependencies", "devDependencies"] {
                            if let Some(deps) = val.get(section).and_then(|d| d.as_object()) {
                                for (dep, _) in deps {
                                    if !all_deps.contains(dep) {
                                        all_deps.push(dep.clone());
                                    }
                                }
                            }
                        }

                        packages.push(PackageInfo {
                            name,
                            root_path: rel_path,
                            package_type: "npm/pnpm Package".to_string(),
                            internal_dependencies: all_deps,
                            external_dependencies_count: 0,
                            file_count: 0,
                            symbol_count: 0,
                            owners: Vec::new(),
                            recent_commits_30d: 0,
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
            if !trimmed.starts_with('#') {
                if let Some((k, v)) = trimmed.split_once('=') {
                    if k.trim() == key {
                        return Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
                    }
                }
            }
        }
        None
    }

    fn extract_cargo_deps(content: &str) -> Vec<String> {
        let mut in_deps = false;
        let mut deps = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(header) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                let header = header.trim();
                if header == "dependencies" || header == "dev-dependencies" {
                    in_deps = true;
                } else {
                    in_deps = false;
                    let sub_dep = header
                        .strip_prefix("dependencies.")
                        .or_else(|| header.strip_prefix("dev-dependencies."));
                    if let Some(dep_name) = sub_dep {
                        let dep_name = dep_name.trim().to_string();
                        if !dep_name.is_empty() && !deps.contains(&dep_name) {
                            deps.push(dep_name);
                        }
                    }
                }
                continue;
            }

            if in_deps && !trimmed.starts_with('#') {
                if let Some((k, _)) = trimmed.split_once('=') {
                    let dep_name = k.trim().to_string();
                    if !dep_name.is_empty() && !deps.contains(&dep_name) {
                        deps.push(dep_name);
                    }
                }
            }
        }

        deps
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

    /// Collects 30-day commit churn across all workspace files in a single `git log` pass
    /// (`O(commits)` instead of spawning git per package).
    fn collect_recent_git_churn_30d(workspace_root: &Path) -> HashMap<String, Vec<usize>> {
        let mut by_file: HashMap<String, Vec<usize>> = HashMap::new();
        let Ok(output) = std::process::Command::new("git")
            .args([
                "log",
                "--since=30.days",
                "--name-only",
                "--pretty=format:---MINICODE_COMMIT---",
            ])
            .current_dir(workspace_root)
            .output()
        else {
            return by_file;
        };
        if !output.status.success() {
            return by_file;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut commit_idx = 0usize;
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "---MINICODE_COMMIT---" {
                commit_idx += 1;
                continue;
            }
            let norm = trimmed.replace('\\', "/");
            let entry = by_file.entry(norm).or_default();
            if entry.last().copied() != Some(commit_idx) {
                entry.push(commit_idx);
            }
        }
        by_file
    }
}

/// Pure-Rust `CODEOWNERS` parser and DP glob matcher (zero regexes, Git last-matching-rule-wins).
#[derive(Debug, Clone)]
pub struct CodeOwnersMatcher {
    rules: Vec<(String, Vec<String>)>,
}

impl CodeOwnersMatcher {
    /// Loads the first existing `CODEOWNERS` file from standard repo locations
    /// (`.github/CODEOWNERS`, `CODEOWNERS`, `docs/CODEOWNERS`).
    pub fn load(workspace_root: &Path) -> Option<Self> {
        const CANDIDATES: &[&str] = &[".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];
        for rel in CANDIDATES {
            let path = workspace_root.join(rel);
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    return Some(Self::parse(&content));
                }
            }
        }
        None
    }

    /// Parses raw `CODEOWNERS` text into ordered `(pattern, owners)` rules.
    pub fn parse(content: &str) -> Self {
        let mut rules = Vec::new();
        for raw_line in content.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Strip inline comments starting with ` #`
            let effective = trimmed.split(" #").next().unwrap_or(trimmed).trim();
            let mut parts = effective.split_whitespace();
            let Some(pattern) = parts.next() else {
                continue;
            };
            let owners: Vec<String> = parts.map(str::to_string).collect();
            if !owners.is_empty() {
                rules.push((pattern.to_string(), owners));
            }
        }
        Self { rules }
    }

    /// Returns the matching owners for a workspace-relative path using Git's last-match-wins rule.
    pub fn owners_for(&self, rel_path: &str) -> Vec<String> {
        let norm = rel_path.trim_start_matches('/').replace('\\', "/");
        for (pattern, owners) in self.rules.iter().rev() {
            if Self::pattern_matches(pattern, &norm) {
                return owners.clone();
            }
        }
        Vec::new()
    }

    fn pattern_matches(pattern: &str, path: &str) -> bool {
        let is_dir_rule = pattern.ends_with('/');
        let anchored = pattern.starts_with('/') || pattern.trim_end_matches('/').contains('/');
        let clean_pat = pattern.trim_start_matches('/');
        let clean_path = path.trim_start_matches('/');

        if is_dir_rule {
            let dir_pat = clean_pat.trim_end_matches('/');
            let probe = if clean_path.ends_with('/') {
                clean_path.trim_end_matches('/')
            } else {
                clean_path
            };
            if anchored {
                if Self::glob_match_dp(dir_pat, probe) {
                    return true;
                }
                let nested_pat = format!("{dir_pat}/**");
                return Self::glob_match_dp(&nested_pat, probe);
            }
            // Unanchored directory rule (e.g. `docs/`) matches any path segment or nested path
            if Self::glob_match_dp(&format!("**/{dir_pat}"), probe)
                || Self::glob_match_dp(&format!("**/{dir_pat}/**"), probe)
                || Self::glob_match_dp(dir_pat, probe)
                || Self::glob_match_dp(&format!("{dir_pat}/**"), probe)
            {
                return true;
            }
            return false;
        }

        if anchored {
            Self::glob_match_dp(clean_pat, clean_path)
                || Self::glob_match_dp(&format!("{clean_pat}/**"), clean_path)
        } else {
            // Bare pattern like `*` or `*.rs` matches full path or basename
            let basename = clean_path.rsplit('/').next().unwrap_or(clean_path);
            Self::glob_match_dp(clean_pat, clean_path)
                || Self::glob_match_dp(clean_pat, basename)
                || Self::glob_match_dp(&format!("**/{clean_pat}"), clean_path)
        }
    }

    /// $O(|P| \cdot |T|)$ dynamic-programming path glob matcher supporting `*` (single segment),
    /// `**` (cross-segment), and `?` (single non-`/` char) without regexes or exponential backtracking.
    fn glob_match_dp(pattern: &str, text: &str) -> bool {
        let p: Vec<char> = pattern.chars().collect();
        let t: Vec<char> = text.chars().collect();
        let pn = p.len();
        let tn = t.len();

        let mut dp = vec![vec![false; tn + 1]; pn + 1];
        dp[0][0] = true;

        for i in 1..=pn {
            if p[i - 1] == '*' {
                dp[i][0] = dp[i - 1][0];
            } else if i >= 3 && p[i - 1] == '/' && p[i - 2] == '*' && p[i - 3] == '*' {
                // Leading `**/` can match zero directory prefixes
                dp[i][0] = dp[i - 3][0];
            }
        }

        for i in 1..=pn {
            let is_double_star =
                p[i - 1] == '*' && ((i >= 2 && p[i - 2] == '*') || (i < pn && p[i] == '*'));
            for j in 1..=tn {
                if p[i - 1] == '*' {
                    // Zero characters matched OR consume current char (crossing `/` only for `**`)
                    let zero = dp[i - 1][j];
                    let more = if is_double_star || t[j - 1] != '/' {
                        dp[i][j - 1]
                    } else {
                        false
                    };
                    dp[i][j] = zero || more;
                } else if p[i - 1] == '?' {
                    dp[i][j] = t[j - 1] != '/' && dp[i - 1][j - 1];
                } else {
                    let exact = p[i - 1] == t[j - 1] && dp[i - 1][j - 1];
                    // Allow `**/` to match empty directory prefix mid-pattern
                    let skip_globstar_slash = i >= 3
                        && p[i - 1] == '/'
                        && p[i - 2] == '*'
                        && p[i - 3] == '*'
                        && dp[i - 3][j];
                    dp[i][j] = exact || skip_globstar_slash;
                }
            }
        }

        dp[pn][tn]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codeowners_matcher_last_rule_wins_and_glob_semantics() {
        let raw = r#"
# Default repository owners
* @org/core-team
*.rs @org/rust-maintainers
/crates/minibrowse/ @org/browser-team
/crates/minidoctor/** @org/doctor-team # inline comment
"#;
        let matcher = CodeOwnersMatcher::parse(raw);

        assert_eq!(matcher.owners_for("README.md"), vec!["@org/core-team"]);
        assert_eq!(
            matcher.owners_for("src/main.rs"),
            vec!["@org/rust-maintainers"]
        );
        // More specific later rule overrides earlier `*.rs` rule
        assert_eq!(
            matcher.owners_for("crates/minibrowse/"),
            vec!["@org/browser-team"]
        );
        assert_eq!(
            matcher.owners_for("crates/minibrowse/src/lib.rs"),
            vec!["@org/browser-team"]
        );
        assert_eq!(
            matcher.owners_for("crates/minidoctor/src/check.rs"),
            vec!["@org/doctor-team"]
        );
    }

    #[test]
    fn test_monorepo_analyze_workspace_populates_owners_and_file_counts() {
        let ws = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(ws.path().join(".github")).unwrap();
        std::fs::create_dir_all(ws.path().join("crates/alpha/src")).unwrap();
        std::fs::create_dir_all(ws.path().join("crates/beta/src")).unwrap();

        std::fs::write(
            ws.path().join(".github/CODEOWNERS"),
            "* @team/platform\n/crates/beta/ @team/beta\n",
        )
        .unwrap();
        std::fs::write(
            ws.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/alpha\", \"crates/beta\"]\n",
        )
        .unwrap();
        std::fs::write(
            ws.path().join("crates/alpha/Cargo.toml"),
            "[package]\nname = \"alpha\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(ws.path().join("crates/alpha/src/lib.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(
            ws.path().join("crates/beta/Cargo.toml"),
            "[package]\nname = \"beta\"\nversion = \"0.1.0\"\n\n[dependencies]\nalpha = { path = \"../alpha\" }\n",
        )
        .unwrap();
        std::fs::write(ws.path().join("crates/beta/src/lib.rs"), "pub fn b() {}\n").unwrap();

        let report = MonorepoOrchestrator::analyze_workspace(ws.path(), false, None)
            .expect("analyze_workspace succeeds");
        assert_eq!(report.packages.len(), 2);

        let alpha = report.packages.iter().find(|p| p.name == "alpha").unwrap();
        let beta = report.packages.iter().find(|p| p.name == "beta").unwrap();
        assert_eq!(alpha.owners, vec!["@team/platform"]);
        assert_eq!(beta.owners, vec!["@team/beta"]);
        assert!(alpha.file_count >= 2);
        assert!(beta.file_count >= 2);

        let md = report.format_markdown();
        assert!(md.contains("@team/beta"));
        assert!(md.contains("30d Commits"));
    }
}
