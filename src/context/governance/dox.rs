use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const CANDIDATE_RULE_FILES: &[&str] = &["AGENTS.md", ".agents.md", "CLAUDE.md", ".cursorrules"];

pub struct DoxEngine;

impl DoxEngine {
    /// Resolves hierarchical developer rules by inspecting the workspace root and all parent
    /// directories leading to the currently active files.
    /// Deduplicates rule files and presents them in an ordered hierarchy (Root -> Subdirectory).
    pub fn resolve_scoped_rules(
        workspace_root: &Path,
        active_files: &[impl AsRef<Path>],
    ) -> String {
        let mut discovered_files: Vec<PathBuf> = Vec::new();
        let mut visited_paths: HashSet<PathBuf> = HashSet::new();

        // 1. Root rules
        for candidate in CANDIDATE_RULE_FILES {
            let root_file = workspace_root.join(candidate);
            if root_file.is_file() && visited_paths.insert(root_file.clone()) {
                discovered_files.push(root_file);
                break; // One primary rule file per directory
            }
        }

        // 2. Traversal down active file paths
        for file in active_files {
            let file_ref = file.as_ref();
            let full_path = if file_ref.is_absolute() {
                file_ref.to_path_buf()
            } else {
                workspace_root.join(file_ref)
            };

            // Collect directory chain from workspace_root to file's parent
            let mut current = full_path.parent();
            let mut chain = Vec::new();
            while let Some(dir) = current {
                if dir == workspace_root || !dir.starts_with(workspace_root) {
                    break;
                }
                chain.push(dir.to_path_buf());
                current = dir.parent();
            }

            // Reverse to process top-down (e.g. crates/ -> crates/core/)
            chain.reverse();

            for dir in chain {
                for candidate in CANDIDATE_RULE_FILES {
                    let rule_file = dir.join(candidate);
                    if rule_file.is_file() && visited_paths.insert(rule_file.clone()) {
                        discovered_files.push(rule_file);
                        break;
                    }
                }
            }
        }

        if discovered_files.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        for file in discovered_files {
            if let Ok(content) = fs::read_to_string(&file) {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let rel = file
                    .strip_prefix(workspace_root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| file.display().to_string());

                if !output.is_empty() {
                    output.push_str("\n\n");
                }
                output.push_str(&format!("<!-- Scoped Rules: {} -->\n{}", rel, trimmed));
            }
        }

        output
    }
}
