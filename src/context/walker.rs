use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Standard directory names always ignored during codebase analysis
pub const STANDARD_EXCLUDED_DIRS: &[&str] = &[
    "target",
    ".git",
    "node_modules",
    ".venv",
    "venv",
    "dist",
    "build",
    ".minicode/backups",
    ".cache",
];

/// Canonical workspace file walker engine for minicode
#[derive(Debug, Clone)]
pub struct WorkspaceWalker {
    root: PathBuf,
    max_depth: Option<usize>,
    include_hidden: bool,
    extensions: Vec<String>,
}

impl WorkspaceWalker {
    /// Creates a new walker rooted at the given workspace path
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            max_depth: None,
            include_hidden: false,
            extensions: Vec::new(),
        }
    }

    /// Sets an optional maximum traversal depth
    #[allow(dead_code)]
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Sets whether to include hidden files (defaults to false)
    #[allow(dead_code)]
    pub fn include_hidden(mut self, include: bool) -> Self {
        self.include_hidden = include;
        self
    }

    /// Restricts returned files to specific file extensions (e.g. `["rs", "py", "js", "ts"]`)
    pub fn extensions(mut self, exts: &[&str]) -> Self {
        self.extensions = exts
            .iter()
            .map(|e| e.trim_start_matches('.').to_lowercase())
            .collect();
        self
    }

    /// Returns a list of relative file path strings sorted alphabetically
    pub fn collect_relative_files(&self) -> Vec<String> {
        let mut files = Vec::new();
        let mut builder = WalkBuilder::new(&self.root);
        builder
            .hidden(!self.include_hidden)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true);

        if let Some(depth) = self.max_depth {
            builder.max_depth(Some(depth));
        }

        for entry in builder.build().flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(rel) = path.strip_prefix(&self.root) {
                    let rel_str = rel.to_string_lossy().to_string();

                    // Check standard excluded directories
                    if Self::is_excluded(&rel_str) {
                        continue;
                    }

                    // Check extension filter
                    if !self.extensions.is_empty() {
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        if !self.extensions.iter().any(|e| e == &ext) {
                            continue;
                        }
                    }

                    files.push(rel_str);
                }
            }
        }

        files.sort();
        files
    }

    /// Returns a list of absolute `PathBuf` for all matching files
    #[allow(dead_code)]
    pub fn collect_absolute_files(&self) -> Vec<PathBuf> {
        self.collect_relative_files()
            .into_iter()
            .map(|rel| self.root.join(rel))
            .collect()
    }

    /// Helper to test if a relative path string falls inside an excluded directory
    fn is_excluded(rel_str: &str) -> bool {
        for excluded in STANDARD_EXCLUDED_DIRS {
            if rel_str == *excluded
                || rel_str.starts_with(&format!("{}/", excluded))
                || rel_str.starts_with(&format!("{}\\", excluded))
            {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_workspace_walker_filters_standard_exclusions() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();

        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
        fs::write(root.join("target/debug/app"), "binary\n").unwrap();
        fs::write(root.join(".git/config"), "[core]\n").unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "console.log();\n").unwrap();

        let walker = WorkspaceWalker::new(root);
        let files = walker.collect_relative_files();

        assert_eq!(files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_workspace_walker_filters_by_extension() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("src/styles.css"), "body {}\n").unwrap();
        fs::write(root.join("src/script.py"), "print(1)\n").unwrap();

        let walker = WorkspaceWalker::new(root).extensions(&["rs", ".py"]);
        let files = walker.collect_relative_files();

        assert_eq!(files, vec!["src/main.rs", "src/script.py"]);
    }
}
