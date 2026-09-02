use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use tokio::process::Command;

/// Resolution strategy for Git merge conflict blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeStrategy {
    Auto,
    Ours,
    Theirs,
    UnionImports,
    Concatenate,
}

impl FromStr for MergeStrategy {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self::from_strategy_str(s))
    }
}

impl MergeStrategy {
    pub fn from_strategy_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ours" => Self::Ours,
            "theirs" => Self::Theirs,
            "union_imports" | "union" => Self::UnionImports,
            "concat" | "concatenate" => Self::Concatenate,
            _ => Self::Auto,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "Semantic Auto",
            Self::Ours => "Take Ours (HEAD)",
            Self::Theirs => "Take Theirs (Incoming)",
            Self::UnionImports => "Union of Imports",
            Self::Concatenate => "Concatenate Blocks",
        }
    }
}

/// Extracted region of a merge conflict
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictBlock {
    pub start_line: usize,
    pub end_line: usize,
    pub ours_label: String,
    pub ours_text: String,
    pub theirs_label: String,
    pub theirs_text: String,
    pub base_text: Option<String>,
}

/// Resolution result for a single file
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResolutionReport {
    pub file_path: String,
    pub total_conflicts: usize,
    pub resolved_conflicts: usize,
    pub strategy_applied: String,
    pub was_staged: bool,
}

/// Overall workspace conflict resolution summary
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictResolverReport {
    pub files_scanned: usize,
    pub files_resolved: usize,
    pub reports: Vec<FileResolutionReport>,
}

impl ConflictResolverReport {
    pub fn format_markdown(&self) -> String {
        let mut out = format!(
            "# ⚔️ Autonomous Git Conflict Resolution Report\n\n\
            - **Files Scanned:** {}\n\
            - **Files Successfully Resolved:** {}\n\n",
            self.files_scanned, self.files_resolved
        );

        if self.reports.is_empty() {
            out.push_str("✔ No merge conflict markers found in the workspace!\n");
            return out;
        }

        out.push_str("### 📄 Resolved Files\n\n");
        out.push_str("| File Path | Conflicts Fixed | Strategy Applied | Staged in Git |\n");
        out.push_str("| :--- | :---: | :--- | :---: |\n");
        for r in &self.reports {
            out.push_str(&format!(
                "| `{}` | **{}/{}** | {} | {} |\n",
                r.file_path,
                r.resolved_conflicts,
                r.total_conflicts,
                r.strategy_applied,
                if r.was_staged {
                    "✔ Staged"
                } else {
                    "Unstaged"
                }
            ));
        }
        out.push('\n');

        out
    }
}

pub struct ConflictResolver;

impl ConflictResolver {
    /// Extracts all conflict blocks from raw file content
    pub fn parse_conflict_blocks(content: &str) -> Vec<ConflictBlock> {
        let mut blocks = Vec::new();
        let mut in_conflict = false;
        let mut in_theirs = false;
        let mut in_base = false;

        let mut start_line = 0;
        let mut ours_label = String::new();
        let mut ours_lines = Vec::new();
        let mut theirs_lines = Vec::new();
        let mut base_lines = Vec::new();

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;

            if line.starts_with("<<<<<<<") {
                in_conflict = true;
                in_theirs = false;
                in_base = false;
                start_line = line_num;
                ours_label = line.trim_start_matches("<<<<<<<").trim().to_string();
                ours_lines.clear();
                theirs_lines.clear();
                base_lines.clear();
            } else if in_conflict && line.starts_with("|||||||") {
                in_base = true;
            } else if in_conflict && line.starts_with("=======") {
                in_theirs = true;
                in_base = false;
            } else if in_conflict && line.starts_with(">>>>>>>") {
                let theirs_label = line.trim_start_matches(">>>>>>>").trim().to_string();
                blocks.push(ConflictBlock {
                    start_line,
                    end_line: line_num,
                    ours_label: ours_label.clone(),
                    ours_text: ours_lines.join("\n"),
                    theirs_label: theirs_label.clone(),
                    theirs_text: theirs_lines.join("\n"),
                    base_text: if base_lines.is_empty() {
                        None
                    } else {
                        Some(base_lines.join("\n"))
                    },
                });
                in_conflict = false;
                in_theirs = false;
                in_base = false;
            } else if in_conflict {
                if in_theirs {
                    theirs_lines.push(line);
                } else if in_base {
                    base_lines.push(line);
                } else {
                    ours_lines.push(line);
                }
            }
        }

        blocks
    }

    /// Resolves all conflict markers in a file string using the chosen merge strategy
    pub fn resolve_content(content: &str, strategy: MergeStrategy) -> (String, usize) {
        let mut result = Vec::new();
        let mut in_conflict = false;
        let mut in_theirs = false;
        let mut in_base = false;

        let mut ours_lines = Vec::new();
        let mut theirs_lines = Vec::new();
        let mut resolved_count = 0;

        for line in content.lines() {
            if line.starts_with("<<<<<<<") {
                in_conflict = true;
                in_theirs = false;
                in_base = false;
                ours_lines.clear();
                theirs_lines.clear();
            } else if in_conflict && line.starts_with("|||||||") {
                in_base = true;
            } else if in_conflict && line.starts_with("=======") {
                in_theirs = true;
                in_base = false;
            } else if in_conflict && line.starts_with(">>>>>>>") {
                // Perform block resolution
                let resolved_block =
                    Self::resolve_single_block(&ours_lines, &theirs_lines, strategy);
                result.push(resolved_block);
                resolved_count += 1;

                in_conflict = false;
                in_theirs = false;
                in_base = false;
            } else if in_conflict {
                if in_theirs {
                    theirs_lines.push(line);
                } else if !in_base {
                    ours_lines.push(line);
                }
            } else {
                result.push(line.to_string());
            }
        }

        let mut joined = result.join("\n");
        if content.ends_with('\n') && !joined.ends_with('\n') {
            joined.push('\n');
        }

        (joined, resolved_count)
    }

    fn resolve_single_block(ours: &[&str], theirs: &[&str], strategy: MergeStrategy) -> String {
        match strategy {
            MergeStrategy::Ours => ours.join("\n"),
            MergeStrategy::Theirs => theirs.join("\n"),
            MergeStrategy::UnionImports => Self::union_imports(ours, theirs),
            MergeStrategy::Concatenate => {
                let mut combined = Vec::new();
                for l in ours {
                    combined.push(*l);
                }
                for l in theirs {
                    if !ours.contains(l) {
                        combined.push(*l);
                    }
                }
                combined.join("\n")
            }
            MergeStrategy::Auto => {
                // Heuristic: If all lines are imports or annotations, use UnionImports
                let is_all_imports = ours.iter().chain(theirs.iter()).all(|l| {
                    let t = l.trim();
                    t.is_empty()
                        || t.starts_with("use ")
                        || t.starts_with("import ")
                        || t.starts_with('#')
                        || t.starts_with("//")
                });

                if is_all_imports {
                    Self::union_imports(ours, theirs)
                } else {
                    // Default to cleanly appending unique non-empty lines
                    let mut combined = Vec::new();
                    for l in ours {
                        combined.push(*l);
                    }
                    for l in theirs {
                        if !ours.contains(l) {
                            combined.push(*l);
                        }
                    }
                    combined.join("\n")
                }
            }
        }
    }

    fn union_imports(ours: &[&str], theirs: &[&str]) -> String {
        let mut unique_lines = BTreeSet::new();
        for l in ours.iter().chain(theirs.iter()) {
            let t = l.trim();
            if !t.is_empty() {
                unique_lines.insert(*l);
            }
        }
        let list: Vec<&str> = unique_lines.into_iter().collect();
        list.join("\n")
    }

    /// Resolves conflicts across files in the workspace
    pub async fn resolve_workspace(
        workspace_root: &Path,
        target_file: Option<&str>,
        strategy: MergeStrategy,
        stage: bool,
    ) -> Result<ConflictResolverReport> {
        let mut reports = Vec::new();
        let mut candidate_files = Vec::new();

        if let Some(tf) = target_file {
            candidate_files.push(workspace_root.join(tf));
        } else {
            let walker = ignore::WalkBuilder::new(workspace_root)
                .hidden(true)
                .parents(true)
                .git_ignore(true)
                .build();

            for entry in walker.flatten() {
                let path = entry.path();
                if path.is_file() {
                    candidate_files.push(path.to_path_buf());
                }
            }
        }

        let mut files_scanned = 0;
        let mut files_resolved = 0;

        for file_path in candidate_files {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if content.contains("<<<<<<<") && content.contains(">>>>>>>") {
                    files_scanned += 1;
                    let blocks = Self::parse_conflict_blocks(&content);
                    let (resolved_text, resolved_count) = Self::resolve_content(&content, strategy);

                    if fs::write(&file_path, &resolved_text).is_ok() {
                        let rel_path = file_path
                            .strip_prefix(workspace_root)
                            .unwrap_or(&file_path)
                            .display()
                            .to_string();

                        let mut was_staged = false;
                        if stage {
                            let stage_res = Command::new("git")
                                .args(["add", &rel_path])
                                .current_dir(workspace_root)
                                .output()
                                .await;
                            was_staged = stage_res.map(|o| o.status.success()).unwrap_or(false);
                        }

                        reports.push(FileResolutionReport {
                            file_path: rel_path,
                            total_conflicts: blocks.len(),
                            resolved_conflicts: resolved_count,
                            strategy_applied: strategy.as_str().to_string(),
                            was_staged,
                        });
                        files_resolved += 1;
                    }
                }
            }
        }

        Ok(ConflictResolverReport {
            files_scanned,
            files_resolved,
            reports,
        })
    }
}
