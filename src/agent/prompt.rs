use crate::constants::{AGENTS_MD_FILE, MAX_AGENTS_MD_BYTES};
use std::io::ErrorKind;
use std::path::Path;

pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are minicode, an ultra-fast, minimalist AI coding agent built in Rust.
You are pair programming with the user to solve software engineering tasks, debug code, inspect repositories, and implement new features.

# Core Guidelines:
1. Be direct, concise, and focused strictly on the user's task.
2. Read files before editing them to understand current context and avoid syntax errors.
3. When modifying existing files, use precise search-and-replace blocks.
4. Keep explanations minimal — let your code and actions speak.
5. Adhere strictly to the language idioms and architecture conventions of the repository.
6. Before calling tools or providing your final response, provide a brief 1-2 sentence thought process inside `<thought>...</thought>` tags.
"#;

pub struct PromptBuilder;

impl PromptBuilder {
    /// Assembles the complete system prompt including base instructions,
    /// repository rules (AGENTS.md), and workspace metadata.
    #[must_use]
    pub fn build_system_prompt(workspace_dir: &Path, custom_instructions: Option<&str>) -> String {
        let mut prompt = String::from(DEFAULT_SYSTEM_PROMPT);

        prompt.push_str(&format!(
            "\n# Current Workspace:\n{}\n",
            workspace_dir.display()
        ));

        // Inject 2-Tier Core Memory (<core_memory>)
        let memory = crate::context::memory::CoreMemory::load(workspace_dir);
        let memory_block = memory.to_prompt_block();
        if !memory_block.is_empty() {
            prompt.push_str("\n# Persistent Memory:\n");
            prompt.push_str(&memory_block);
            prompt.push('\n');
        }

        // Inject Active Working Memory (<working_memory>)
        let working_memory = crate::context::working_memory::WorkingMemory::new(workspace_dir);
        let wm_block = working_memory.to_prompt_block();
        if !wm_block.is_empty() {
            prompt.push_str("\n# Task Working Memory:\n");
            prompt.push_str(&wm_block);
            prompt.push('\n');
        }

        // Inject AGENTS.md rules if present in the workspace
        let agents_file = workspace_dir.join(AGENTS_MD_FILE);
        match std::fs::read_to_string(&agents_file) {
            Ok(content) => {
                let trimmed = if content.len() > MAX_AGENTS_MD_BYTES {
                    tracing::warn!(
                        size = content.len(),
                        max = MAX_AGENTS_MD_BYTES,
                        "AGENTS.md exceeds size limit; truncating"
                    );
                    let valid_end = content.floor_char_boundary(MAX_AGENTS_MD_BYTES);
                    &content[..valid_end]
                } else {
                    &content
                };
                prompt.push_str("\n# Repository Guidelines (AGENTS.md):\n");
                prompt.push_str(trimmed);
                prompt.push('\n');
            }
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::warn!(path = %agents_file.display(), error = %e, "Failed to read AGENTS.md");
                }
            }
        }

        // Append custom user/turn instructions if provided
        if let Some(custom) = custom_instructions {
            prompt.push_str("\n# Additional Instructions:\n");
            prompt.push_str(custom);
            prompt.push('\n');
        }

        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_build_system_prompt_default() {
        let temp_dir = std::env::temp_dir();
        let prompt = PromptBuilder::build_system_prompt(&temp_dir, None);
        assert!(prompt.contains("You are minicode"));
        assert!(prompt.contains(&temp_dir.display().to_string()));
    }

    #[test]
    fn test_build_system_prompt_with_agents_md() {
        let temp_dir = std::env::temp_dir().join(format!("minicode_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let agents_path = temp_dir.join("AGENTS.md");
        let mut file = File::create(&agents_path).unwrap();
        writeln!(file, "Rule: Never use unwrap!").unwrap();

        let prompt = PromptBuilder::build_system_prompt(&temp_dir, Some("Focus on speed"));
        assert!(prompt.contains("Rule: Never use unwrap!"));
        assert!(prompt.contains("Focus on speed"));

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
