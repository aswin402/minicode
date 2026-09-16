use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const CANDIDATE_RULE_FILES: &[&str] = &[
    "AGENTS.md",
    ".agents.md",
    "CLAUDE.md",
    ".cursorrules",
    ".dox",
    ".dox.md",
    ".rules.md",
    "RULES.md",
    ".minicode/rules.md",
    ".minicode/dox.md",
];

pub const DEFAULT_MAX_DOX_CHARS: usize = 4500;

pub struct DoxEngine;

impl DoxEngine {
    /// Resolves hierarchical developer rules by inspecting the workspace root and all parent
    /// directories leading to the currently active files.
    /// Deduplicates rule files and presents them in an ordered hierarchy (Root -> Subdirectory).
    pub fn resolve_scoped_rules(
        workspace_root: &Path,
        active_files: &[impl AsRef<Path>],
    ) -> String {
        Self::resolve_scoped_rules_with_budget(workspace_root, active_files, DEFAULT_MAX_DOX_CHARS)
    }

    /// Resolves hierarchical developer rules with a maximum character budget and smart
    /// language-aware markdown section filtering.
    pub fn resolve_scoped_rules_with_budget(
        workspace_root: &Path,
        active_files: &[impl AsRef<Path>],
        max_chars: usize,
    ) -> String {
        let mut discovered_files: Vec<PathBuf> = Vec::new();
        let mut visited_paths: HashSet<PathBuf> = HashSet::new();

        // Detect file extensions in the active working set
        let mut active_exts: HashSet<String> = HashSet::new();
        for file in active_files {
            let p = file.as_ref();
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                active_exts.insert(ext.to_lowercase());
            }
        }

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
                let filtered = Self::filter_markdown_sections(&content, &active_exts);
                let trimmed = filtered.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let rel = file
                    .strip_prefix(workspace_root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| file.display().to_string());

                let header = format!("<!-- Scoped Rules: {} -->\n", rel);
                let needed = header.len() + trimmed.len() + 2;

                if output.len() + needed > max_chars {
                    // Check if we can fit at least a partial block
                    let remaining = max_chars.saturating_sub(output.len() + header.len() + 50);
                    if remaining > 150 {
                        if !output.is_empty() {
                            output.push_str("\n\n");
                        }
                        output.push_str(&header);
                        let truncated: String = trimmed.chars().take(remaining).collect();
                        output.push_str(&truncated);
                        output.push_str(
                            "\n<!-- [... rule file truncated to preserve context budget ...] -->",
                        );
                    }
                    break;
                }

                if !output.is_empty() {
                    output.push_str("\n\n");
                }
                output.push_str(&header);
                output.push_str(trimmed);
            }
        }

        output
    }

    /// Filters markdown content by section headers when the active working set has
    /// specific languages, omitting sections clearly dedicated to disjoint languages.
    pub fn filter_markdown_sections(content: &str, active_exts: &HashSet<String>) -> String {
        if active_exts.is_empty() {
            return content.to_string();
        }

        let is_rust = active_exts.contains("rs");
        let is_python = active_exts.contains("py");
        let is_frontend = active_exts.iter().any(|ext| {
            matches!(
                ext.as_str(),
                "ts" | "tsx" | "js" | "jsx" | "html" | "css" | "scss" | "vue" | "svelte"
            )
        });
        let is_go = active_exts.contains("go");

        let mut result = Vec::new();
        let mut current_section_header = String::new();
        let mut current_section_lines = Vec::new();

        for line in content.lines() {
            if line.starts_with('#') {
                // Process previous section
                if !current_section_lines.is_empty() || !current_section_header.is_empty() {
                    if Self::should_keep_section(
                        &current_section_header,
                        is_rust,
                        is_python,
                        is_frontend,
                        is_go,
                    ) {
                        if !current_section_header.is_empty() {
                            result.push(current_section_header.clone());
                        }
                        result.append(&mut current_section_lines);
                    } else {
                        current_section_lines.clear();
                    }
                }
                current_section_header = line.to_string();
            } else {
                current_section_lines.push(line.to_string());
            }
        }

        // Flush last section
        if (!current_section_lines.is_empty() || !current_section_header.is_empty())
            && Self::should_keep_section(
                &current_section_header,
                is_rust,
                is_python,
                is_frontend,
                is_go,
            )
        {
            if !current_section_header.is_empty() {
                result.push(current_section_header);
            }
            result.append(&mut current_section_lines);
        }

        result.join("\n")
    }

    fn should_keep_section(
        header: &str,
        is_rust: bool,
        is_python: bool,
        is_frontend: bool,
        is_go: bool,
    ) -> bool {
        if header.is_empty() {
            return true;
        }

        let lower = header.to_lowercase();

        // Check if the section targets a specific technology
        let targets_rust = lower.contains("rust") || lower.contains("cargo");
        let targets_python =
            lower.contains("python") || lower.contains("pytest") || lower.contains("pip");
        let targets_frontend = lower.contains("frontend")
            || lower.contains("react")
            || lower.contains("vue")
            || lower.contains("svelte")
            || lower.contains("css")
            || lower.contains("tailwind")
            || lower.contains("javascript")
            || lower.contains("typescript");
        let targets_go = lower.contains("golang") || lower.contains("go modules");

        // If it exclusively targets another language not present in active working set, omit it
        if targets_python && !is_python && (is_rust || is_frontend || is_go) {
            return false;
        }
        if targets_rust && !is_rust && (is_python || is_frontend || is_go) {
            return false;
        }
        if targets_frontend && !is_frontend && (is_rust || is_python || is_go) {
            return false;
        }
        if targets_go && !is_go && (is_rust || is_python || is_frontend) {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_dox_candidate_discovery() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        // Test with .dox file
        fs::write(ws.join(".dox"), "# Dox Global Invariants\n- Always test").unwrap();

        let rules = DoxEngine::resolve_scoped_rules(ws, &["src/main.rs"]);
        assert!(rules.contains("Dox Global Invariants"));
    }

    #[test]
    fn test_dox_section_filtering() {
        let content = r#"# Project Rules
General rule for all files.

## Rust Guidelines
Use thiserror and tracing.

## Python Guidelines
Use poetry and black.

## Testing Standards
Write comprehensive tests.
"#;

        let mut rust_exts = HashSet::new();
        rust_exts.insert("rs".to_string());

        let filtered_rust = DoxEngine::filter_markdown_sections(content, &rust_exts);
        assert!(filtered_rust.contains("Rust Guidelines"));
        assert!(filtered_rust.contains("General rule"));
        assert!(filtered_rust.contains("Testing Standards"));
        assert!(!filtered_rust.contains("Python Guidelines"));
        assert!(!filtered_rust.contains("Use poetry and black"));
    }

    #[test]
    fn test_dox_budget_capping() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let huge_rules = "A".repeat(5000);
        fs::write(ws.join("AGENTS.md"), format!("# Big Rules\n{}", huge_rules)).unwrap();

        let capped = DoxEngine::resolve_scoped_rules_with_budget(ws, &["src/main.rs"], 500);
        assert!(capped.len() <= 600);
        assert!(capped.contains("truncated to preserve context budget"));
    }
}
