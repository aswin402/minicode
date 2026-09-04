use crate::constants::{AGENTS_MD_FILE, MAX_AGENTS_MD_BYTES};
use std::io::ErrorKind;
use std::path::Path;

pub const STATIC_SYSTEM_PROMPT: &str = r#"You are minicode, an ultra-fast, minimalist AI coding agent built in Rust.
You pair-program with the user to inspect repositories, debug code, design architectures, and implement new features with surgical precision.

# Core Operational Axioms (Karpathy Guidelines):
1. **Think Before Coding**: Explicitly analyze assumptions and surface trade-offs. If requirements are ambiguous, ask clarifying questions before editing.
2. **Simplicity First (The Ponytail Minimalist Ladder)**:
   - Does this need to exist? (Skip if YAGNI)
   - Already in the codebase? (Reuse existing functions/types)
   - Standard library does it? (Use standard library / native core)
   - Native platform feature? (Use OS / language primitives)
   - Installed dependency? (Reuse existing Cargo.toml / package manifest crates)
   - One line? (Keep it concise and readable)
   - Minimum working code: Write the absolute minimal implementation that passes tests.
3. **Surgical Changes**: Touch strictly what is required for the task. Never reformat, clean up, or alter unrelated code, comments, or imports.
4. **Goal-Driven Verification**: Every modification must be verified with compiler checks or automated tests before concluding.

# Tool Calling & Surgical Editing Protocol:
1. **Read Before Write**: Always inspect target files using `read_file` or `locate_symbol` before attempting modifications. Verify exact lines and indentation.
2. **Surgical Search-and-Replace**: When modifying files with `patch_file`, provide unique search blocks with 2-3 lines of surrounding context. Preserve existing style and indentation verbatim.
3. **Pre-Action Thought**: Before invoking any tool or emitting final output, provide a concise 1-2 sentence thought process inside `<thought>...</thought>` tags explaining your immediate intent.
4. **Action Over Verbosity**: Keep explanations minimal. Let verified code, diffs, and test outputs speak for themselves.
5. **Positive Error Handling**: Always propagate errors using the `?` operator or return `Result<T, MinicodeError>`. If unwrapping is tempting, use `.ok_or_else(|| ...)?`.

# Autonomous Intent & Native Tool Protocols:
- **Project Scaffolding (`/stack` or natural language)**: When asked to scaffold, create, or bootstrap a new app or project (e.g., Next.js, React Vite, FastAPI, Flutter, Hono, MERN, PERN), autonomously use `onpkg_stack_list` and `onpkg_stack_add` to generate full production architectures with zero external prerequisites.
- **Milestone Planning (`/plan` or natural language)**: When asked to plan, break down, or design a feature, maintain structured task checklists in `onpkg_docs/todo.md` and technical specifications in `onpkg_docs/implementation.md`. Initialize or update active task plans with `create_plan`.
- **Autonomous Goal Execution (`/goal` or natural language)**: When executing multi-step goals, break the ask into ordered tasks in `onpkg_docs/todo.md`, execute each step iteratively, run verification tests, and continue until all tasks are marked complete (`[x]`).
- **Code Review & Quality (`/review` or natural language)**: When asked to review changes or diffs, evaluate multi-dimensional quality across correctness, security, architecture, performance, and test coverage.
- **Code Search & Navigation (`/map` or natural language)**: Autonomously leverage `locate_symbol` for instant AST declarations, `grep_search` for exact regex patterns, and `code_explore` for caller/callee graphs.
- **CodeGraph Surgical Exploration (`/explore` or natural language)**: When asked to explore codebase architecture, understand how a feature works, find callers/callees, or assess change impact, prefer using `code_explore` and `diff_impact`. A single `code_explore` call gives you the exact symbol definition, line numbers, incoming callers, outgoing calls, and blast radius without exploratory file reads.
- **Dynamic Tool Activation (`activate_tools`)**: If a specialized capability (e.g. `web`, `git`, `codegraph`, `onpkg`, `agent`, `memory`) is needed mid-turn, dynamically call `activate_tools(category="...")` to unlock that category's schemas.
"#;

#[allow(dead_code)]
pub const DEFAULT_SYSTEM_PROMPT: &str = STATIC_SYSTEM_PROMPT;

pub struct PromptBuilder;

impl PromptBuilder {
    /// Assembles the 100% static, cache-friendly system prompt.
    /// This prompt is immutable across turns for maximum prefix caching hits.
    #[must_use]
    pub fn build_static_system_prompt(
        workspace_dir: &Path,
        custom_instructions: Option<&str>,
    ) -> String {
        let mut prompt = String::from(STATIC_SYSTEM_PROMPT);

        prompt.push_str(&format!(
            "\n# Current Workspace:\n{}\n",
            workspace_dir.display()
        ));

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

    /// Builds the dynamic Zone 3 (Recency Zone) context injected into the conversation tail.
    /// Contains turn-varying state: git status, active working set, memory anchor, progressive memory, and context budget.
    #[must_use]
    pub fn build_recency_context(
        workspace_dir: &Path,
        memory_anchor: Option<&str>,
        active_working_set: &[String],
        git_status: Option<&crate::git::GitStatus>,
        context_budget: Option<&crate::context::budget::ContextBudget>,
    ) -> String {
        let mut recency = String::new();
        recency.push_str("\n\n<workspace_context>\n");

        // 1. Context Budget & Headroom Bar
        if let Some(budget) = context_budget {
            recency.push_str(&budget.to_prompt_block());
        }

        // 2. Git Status & Active Branch
        if let Some(status) = git_status {
            recency.push_str(&format!(
                "  <git_status branch=\"{}\" clean=\"{}\">\n",
                status.branch, status.is_clean
            ));
            if !status.staged.is_empty() {
                recency.push_str("    <staged_files>\n");
                for f in status.staged.iter().take(10) {
                    recency.push_str(&format!("      <file path=\"{}\" />\n", f));
                }
                recency.push_str("    </staged_files>\n");
            }
            if !status.unstaged.is_empty() {
                recency.push_str("    <modified_files>\n");
                for f in status.unstaged.iter().take(10) {
                    recency.push_str(&format!("      <file path=\"{}\" />\n", f));
                }
                recency.push_str("    </modified_files>\n");
            }
            if !status.untracked.is_empty() {
                recency.push_str("    <untracked_files>\n");
                for f in status.untracked.iter().take(10) {
                    recency.push_str(&format!("      <file path=\"{}\" />\n", f));
                }
                recency.push_str("    </untracked_files>\n");
            }
            if !status.conflicted.is_empty() {
                recency.push_str("    <conflicted_files>\n");
                for f in &status.conflicted {
                    recency.push_str(&format!("      <file path=\"{}\" />\n", f));
                }
                recency.push_str("    </conflicted_files>\n");
            }
            recency.push_str("  </git_status>\n");
        }

        // 2. Active Working Set (recently read / modified files)
        if !active_working_set.is_empty() {
            recency.push_str("  <active_working_set>\n");
            for path in active_working_set.iter().take(8) {
                recency.push_str(&format!("    <file path=\"{}\" />\n", path));
            }
            recency.push_str("  </active_working_set>\n");
        }

        // 3. Dynamic Memory Anchor (active objective, key decisions, blockers)
        if let Some(anchor) = memory_anchor {
            if !anchor.trim().is_empty() {
                recency.push_str("  <task_anchor>\n");
                recency.push_str(anchor.trim());
                recency.push_str("\n  </task_anchor>\n");
            }
        }

        // 4. Progressive 4-Tier Memory (<progressive_memory>)
        let prog_memory =
            crate::context::progressive_memory::ProgressiveMemory::load(workspace_dir);
        let prog_block = prog_memory.to_prompt_block();
        if !prog_block.is_empty() {
            recency.push_str("  <progressive_memory>\n");
            recency.push_str(prog_block.trim());
            recency.push_str("\n  </progressive_memory>\n");
        }

        // 5. 2-Tier Core Memory (<core_memory>)
        let memory = crate::context::memory::CoreMemory::load(workspace_dir);
        let memory_block = memory.to_prompt_block();
        if !memory_block.is_empty() {
            recency.push_str("  <core_memory>\n");
            recency.push_str(memory_block.trim());
            recency.push_str("\n  </core_memory>\n");
        }

        // 6. Active Working Memory (<working_memory>)
        let working_memory = crate::context::working_memory::WorkingMemory::new(workspace_dir);
        let wm_block = working_memory.to_prompt_block();
        if !wm_block.is_empty() {
            recency.push_str("  <task_working_memory>\n");
            recency.push_str(wm_block.trim());
            recency.push_str("\n  </task_working_memory>\n");
        }

        recency.push_str("</workspace_context>");
        recency
    }

    /// Assembles system prompt with optional custom instructions.
    #[allow(dead_code)]
    #[must_use]
    pub fn build_system_prompt(workspace_dir: &Path, custom_instructions: Option<&str>) -> String {
        Self::build_static_system_prompt(workspace_dir, custom_instructions)
    }

    /// Legacy builder for system prompt with embedded anchor.
    #[allow(dead_code)]
    #[must_use]
    pub fn build_system_prompt_with_anchor(
        workspace_dir: &Path,
        custom_instructions: Option<&str>,
        memory_anchor: Option<&str>,
    ) -> String {
        let mut prompt = Self::build_static_system_prompt(workspace_dir, custom_instructions);

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

    #[test]
    fn test_build_static_system_prompt_axioms() {
        let temp_dir = std::env::temp_dir();
        let prompt = PromptBuilder::build_static_system_prompt(&temp_dir, None);
        assert!(prompt.contains("Karpathy Guidelines"));
        assert!(prompt.contains("Ponytail Minimalist Ladder"));
        assert!(prompt.contains("Surgical Search-and-Replace"));
        assert!(prompt.contains("Positive Error Handling"));
        // Static prompt should NOT contain dynamic memory tags
        assert!(!prompt.contains("<workspace_context>"));
        assert!(!prompt.contains("<task_anchor>"));
    }

    #[test]
    fn test_build_recency_context_formatting() {
        let temp_dir = std::env::temp_dir();
        let active_set = vec!["src/main.rs".to_string(), "src/agent/loop.rs".to_string()];
        let status = crate::git::GitStatus {
            branch: "feature/prompts".to_string(),
            is_clean: false,
            staged: vec!["src/main.rs".to_string()],
            unstaged: vec!["src/agent/prompt.rs".to_string()],
            untracked: vec!["tests/new_test.rs".to_string()],
            conflicted: vec![],
        };
        let anchor = "Active Goal: Implement Tri-Zone Prompts\nStep 1/3: In progress";
        let budget = crate::context::budget::ContextBudget::new(25_000, 128_000, 45_000);

        let recency = PromptBuilder::build_recency_context(
            &temp_dir,
            Some(anchor),
            &active_set,
            Some(&status),
            Some(&budget),
        );

        assert!(recency.contains("<workspace_context>"));
        assert!(recency.contains("<context_budget used=\"25000\" limit=\"128000\""));
        assert!(recency.contains("branch=\"feature/prompts\""));
        assert!(recency.contains("clean=\"false\""));
        assert!(recency.contains("<file path=\"src/main.rs\" />"));
        assert!(recency.contains("<file path=\"src/agent/prompt.rs\" />"));
        assert!(recency.contains("<active_working_set>"));
        assert!(recency.contains("<task_anchor>"));
        assert!(recency.contains("Implement Tri-Zone Prompts"));
        assert!(recency.contains("</workspace_context>"));
    }
}
