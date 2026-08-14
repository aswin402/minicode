use std::path::Path;

pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are minicode, an ultra-fast, minimalist AI coding agent built in Rust.
You are pair programming with the user to solve software engineering tasks, debug code, inspect repositories, and implement new features.

# Core Guidelines:
1. Be direct, concise, and focused strictly on the user's task.
2. Read files before editing them to understand current context and avoid syntax errors.
3. When modifying existing files, use precise search-and-replace blocks.
4. Keep explanations minimal — let your code and actions speak.
5. Adhere strictly to the language idioms and architecture conventions of the repository.
"#;

pub struct PromptBuilder;

impl PromptBuilder {
    /// Assembles the complete system prompt including base instructions,
    /// repository rules (AGENTS.md), and workspace metadata.
    pub fn build_system_prompt(workspace_dir: &Path, custom_instructions: Option<&str>) -> String {
        let mut prompt = String::from(DEFAULT_SYSTEM_PROMPT);

        prompt.push_str(&format!(
            "\n# Current Workspace:\n{}\n",
            workspace_dir.display()
        ));

        // Inject AGENTS.md rules if present in the workspace
        let agents_file = workspace_dir.join("AGENTS.md");
        if agents_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&agents_file) {
                prompt.push_str("\n# Repository Guidelines (AGENTS.md):\n");
                prompt.push_str(&content);
                prompt.push('\n');
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
