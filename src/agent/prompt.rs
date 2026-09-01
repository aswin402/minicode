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

# Autonomous Intent & Native Tool Protocols:
- **Project Scaffolding (`/stack` or natural language)**: When asked to scaffold, create, or bootstrap a new app or project (e.g., Next.js, React Vite, FastAPI, Flutter, Hono, MERN, PERN), autonomously use `onpkg_stack_list` and `onpkg_stack_add` to generate full production architectures with zero external prerequisites.
- **Milestone Planning (`/plan` or natural language)**: When asked to plan, break down, or design a feature, maintain structured task checklists in `onpkg_docs/todo.md` and technical specifications in `onpkg_docs/implementation.md`. Initialize or update active task plans with `create_plan`.
- **Autonomous Goal Execution (`/goal` or natural language)**: When executing multi-step goals, break the ask into ordered tasks in `onpkg_docs/todo.md`, execute each step iteratively, run verification tests, and continue until all tasks are marked complete (`[x]`).
- **Code Review & Quality (`/review` or natural language)**: When asked to review changes or diffs, evaluate multi-dimensional quality across correctness, security, architecture, performance, and test coverage.
- **Code Search & Navigation (`/map` or natural language)**: Autonomously leverage `semantic_search` for intent-based code discovery, `locate_symbol` for instant AST declarations, and `grep_search` for exact regex patterns.
- **CodeGraph Surgical Exploration (`/explore` or natural language)**: When asked to explore codebase architecture, understand how a feature works, find callers/callees, or assess change impact, prefer using `code_explore` and `diff_impact`. A single `code_explore` call gives you the exact symbol definition, line numbers, incoming callers, outgoing calls, and blast radius without needing multiple exploratory file reads.
- **Safe Checkpoints & History (`/undo`, `/history` or natural language)**: Understand that all file modifications are safely checkpointed. When asked to undo, explain the turn boundaries and file restorations.
"#;

pub struct PromptBuilder;

impl PromptBuilder {
    /// Assembles the complete system prompt including base instructions,
    /// repository rules (AGENTS.md), and workspace metadata.
    #[allow(dead_code)]
    #[must_use]
    pub fn build_system_prompt(workspace_dir: &Path, custom_instructions: Option<&str>) -> String {
        Self::build_system_prompt_with_anchor(workspace_dir, custom_instructions, None)
    }

    /// Assembles the complete system prompt with optional persistent memory anchor.
    #[must_use]
    pub fn build_system_prompt_with_anchor(
        workspace_dir: &Path,
        custom_instructions: Option<&str>,
        memory_anchor: Option<&str>,
    ) -> String {
        let mut prompt = String::from(DEFAULT_SYSTEM_PROMPT);

        prompt.push_str(&format!(
            "\n# Current Workspace:\n{}\n",
            workspace_dir.display()
        ));

        // Inject Progressive 4-Tier Memory (<progressive_memory>)
        let prog_memory =
            crate::context::progressive_memory::ProgressiveMemory::load(workspace_dir);
        let prog_block = prog_memory.to_prompt_block();
        if !prog_block.is_empty() {
            prompt.push_str("\n# Progressive Multi-Tier Memory:\n");
            prompt.push_str(&prog_block);
            prompt.push('\n');
        }

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

        // Inject Session Memory Anchor if present
        if let Some(anchor) = memory_anchor {
            if !anchor.trim().is_empty() {
                prompt.push_str(anchor);
                prompt.push('\n');
            }
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
