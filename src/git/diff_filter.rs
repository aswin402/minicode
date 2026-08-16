use crate::constants::{GIT_DIFF_MAX_BYTES, GIT_LOCKFILES};

/// Filters and condenses git diff outputs to preserve LLM token budget.
pub struct DiffFilter;

impl DiffFilter {
    /// Condenses large lockfiles and trims the diff if it exceeds `GIT_DIFF_MAX_BYTES`.
    pub fn filter_diff(raw_diff: &str) -> String {
        if raw_diff.trim().is_empty() {
            return String::new();
        }

        let mut filtered_blocks = Vec::new();
        let mut current_file: Option<String> = None;
        let mut current_block_lines = Vec::new();
        let mut is_lockfile = false;
        let mut additions = 0;
        let mut deletions = 0;

        for line in raw_diff.lines() {
            if line.starts_with("diff --git ") {
                // Flush previous block
                if let Some(file) = current_file.take() {
                    if is_lockfile {
                        filtered_blocks.push(format!(
                            "diff --git a/{file} b/{file}\n[{file} lockfile diff truncated: +{additions} / -{deletions} lines]\n"
                        ));
                    } else {
                        filtered_blocks.push(current_block_lines.join("\n"));
                    }
                }

                // Start new file block
                let file_name = line
                    .split_whitespace()
                    .last()
                    .unwrap_or("")
                    .trim_start_matches("b/")
                    .to_string();

                is_lockfile = GIT_LOCKFILES.iter().any(|&lock| file_name.ends_with(lock));
                current_file = Some(file_name);
                current_block_lines.clear();
                current_block_lines.push(line.to_string());
                additions = 0;
                deletions = 0;
            } else {
                if is_lockfile {
                    if line.starts_with('+') && !line.starts_with("+++") {
                        additions += 1;
                    } else if line.starts_with('-') && !line.starts_with("---") {
                        deletions += 1;
                    }
                } else {
                    current_block_lines.push(line.to_string());
                }
            }
        }

        // Flush last block
        if let Some(file) = current_file {
            if is_lockfile {
                filtered_blocks.push(format!(
                    "diff --git a/{file} b/{file}\n[{file} lockfile diff truncated: +{additions} / -{deletions} lines]\n"
                ));
            } else {
                filtered_blocks.push(current_block_lines.join("\n"));
            }
        }

        let combined = if filtered_blocks.is_empty() {
            raw_diff.to_string()
        } else {
            filtered_blocks.join("\n")
        };

        // Enforce maximum bytes
        if combined.len() > GIT_DIFF_MAX_BYTES {
            let mut truncated = String::new();
            let mut byte_count = 0;
            for line in combined.lines() {
                if byte_count + line.len() + 1 > GIT_DIFF_MAX_BYTES {
                    truncated.push_str("\n\n[... Git diff truncated to preserve token budget ...]");
                    break;
                }
                truncated.push_str(line);
                truncated.push('\n');
                byte_count += line.len() + 1;
            }
            truncated
        } else {
            combined
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_diff_condenses_lockfile() {
        let raw = "diff --git a/Cargo.lock b/Cargo.lock\n--- a/Cargo.lock\n+++ b/Cargo.lock\n+line1\n+line2\n-old_line\ndiff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n+fn new_fn() {}\n";
        let filtered = DiffFilter::filter_diff(raw);
        assert!(filtered.contains("Cargo.lock lockfile diff truncated: +2 / -1 lines"));
        assert!(filtered.contains("fn new_fn() {}"));
    }

    #[test]
    fn test_filter_diff_handles_empty() {
        assert_eq!(DiffFilter::filter_diff(""), "");
        assert_eq!(DiffFilter::filter_diff("   \n "), "");
    }
}
