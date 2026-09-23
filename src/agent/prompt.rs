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
2. **Surgical Search-and-Replace**: When modifying files with `patch_file`, provide unique search blocks with 2-3 lines of surrounding context to ensure exact, unambiguous matches.
3. **Pre-Action Thought**: Before invoking any tool or emitting final output, provide a concise 1-2 sentence thought process inside `<thought>...</thought>` tags explaining your immediate intent.
4. **Action Over Verbosity**: Keep explanations minimal. Let verified code, diffs, and test outputs speak for themselves.
5. **Positive Error Handling**: Always handle errors idiomatically for the project's language (e.g. `?` operator in Rust, try/except in Python, proper error returns in Go). Never ignore or unwrap unhandled errors.

# Example patch_file usage:
Target lines in src/main.rs:
    let port = 8080;
    tracing::info!("Listening on port {}", port);
To change port to 9000:
patch_file(path="src/main.rs", search_block="    let port = 8080;\n    tracing::info!(\"Listening on port {}\", port);", replace_block="    let port = 9000;\n    tracing::info!(\"Listening on port {}\", port);")

# Autonomous Intent & Core Tools:
- **Code Search & Navigation**: Autonomously leverage `locate_symbol` for instant AST declarations, `grep_search` for exact regex patterns, `file_search` to find files, and `read_file` to inspect lines.
- **Command Execution & Verification**: Run build checks and tests with `exec_cmd` (e.g. `cargo check`, `cargo test`, `npm test`, `pytest`).
- **Autonomous MiniKit (Skills, Packages, Stacks & Drift)**:
  - **Dynamic Domain Skills**: You decide when to consult or apply domain coding skills, or when instructed by the user (e.g. "use nextjs skill", "show tailwind guidelines", "install react skill"). Autonomously invoke `kit_skill_show(skill_name)` to inspect technology standards on-demand (e.g. React, Next.js, FastAPI, Flutter, Hono, Rust, Tailwind, Prisma, Vite, Express, etc.), `kit_skill_list` to discover available skills, `kit_skill_install(skill_name)` to persist a skill in the project, or `kit_skill_remove(skill_name)` to uninstall it.
  - **Dependency Management**: When code introduces a new external library, or when the user asks to add, check, or remove a package (e.g. "add zod", "install axum with kit", "remove express"):
    - Do NOT guess versions or run raw shell commands.
    - Autonomously invoke `kit_info(name)` to verify package availability, upstream version, and metadata across npm, PyPI, crates.io, or pub.dev.
    - Autonomously invoke `kit_add(name, is_dev)` to update the project manifest (`package.json`, `Cargo.toml`, `requirements.txt`, `pubspec.yaml`) and synchronize dependencies.
    - Autonomously invoke `kit_remove(name)` to prune obsolete or conflicting dependencies.
  - **Architecture Scaffolding, Custom Templates & Self-Healing**: When bootstrapping a project or when instructed by the user, use `kit_stack_list` and `kit_stack_add`. To author custom architecture specifications, use `kit_stack_new` and `kit_stack_remove`. When investigating broken project structures, missing files, or when the user asks to check drift, run `kit_stack_diff(apply: true)` to self-heal the repository architecture.
- **Subagent Delegation & Parallelism**: For large multi-step features, deep codebase audits, or independent research tasks, autonomously delegate to specialized workers using `invoke_subagent` (roles: "researcher", "code_reviewer", "test_engineer", "security_auditor") or `delegate_task` for isolated worktrees.
- **Task Planning**: When planning complex features, track progress in `minikit_docs/todo.md` and spec in `minikit_docs/implementation.md`.
- **Autonomous MiniPower (Methodology, Verification Barrier, Worktrees & Planning)**:
  - **Inspection & Methodology**: Autonomously invoke `power_status` to inspect active engineering principles, anti-rationalization guardrails, and verification rules.
  - **Spec Before Code & Brainstorming**: When a user's prompt is high-level, open-ended, or ambiguous, do NOT immediately write code. Autonomously call `power_brainstorm(topic)` to explore trade-offs, clarify scope, and propose concrete architectural options.
  - **Bite-Sized Atomic Planning**: For multi-step implementations, refactors, or new features, call `power_plan(topic, save_to_docs: true)` to structure tasks into 2-5 minute atomic units with verifiable acceptance criteria in `todo.md`.
  - **Isolated Ephemeral Worktree Execution**: When implementing risky features or running parallel trials, use `power_worktree_task(task)` to execute the task in an isolated Git worktree sandbox with automated 3-way merge arbitration.
  - **Two-Stage Multi-Agent Code Review**: Before finishing any substantial changes, autonomously run `power_review` to conduct an adversarial review across Stage 1 (Spec Compliance) and Stage 2 (Code Quality, zero unwraps/panics, security).
  - **4-Gate Pre-Completion Verification Barrier**: Never conclude a task or claim completion without running `power_verify` or executing compiler/test checks via `exec_cmd`. `power_verify` programmatically guarantees Gate 1 (Compiler & Syntax Integrity), Gate 2 (Unit & Regression Tests), Gate 3 (Merge Conflicts & Structural Integrity), and Gate 4 (Clean Diff & Secret Leak Prevention).
  - **Strict TDD & Anti-Rationalization Guardrails**:
    - "This is a simple one-liner, no tests needed" -> False. Small unverified edits break systems. Write a test first.
    - "I'll write tests after implementation" -> False. TDD enforces Red before Green.
    - "I can verify this by reading the code" -> False. Reading is not execution. Run `power_verify` or `exec_cmd`.
"#;

/// Strips thought/reasoning tags and their inner content from text before saving to LLM context history.
#[must_use]
pub fn strip_thought_blocks(raw: &str) -> String {
    let tag_pairs = [
        ("<thought>", "</thought>"),
        ("<think>", "</think>"),
        ("<thinking>", "</thinking>"),
        ("<reasoning>", "</reasoning>"),
        ("<antThinking>", "</antThinking>"),
        ("<Thought>", "</Thought>"),
        ("<Thinking>", "</Thinking>"),
        ("<Reasoning>", "</Reasoning>"),
        ("<THINK>", "</THINK>"),
    ];

    let mut result = raw.to_string();
    for (open, close) in tag_pairs {
        while let Some(start) = result.find(open) {
            if let Some(end) = result[start..].find(close) {
                let full_end = start + end + close.len();
                result.replace_range(start..full_end, "");
            } else {
                // Unclosed tag: strip to end
                result.truncate(start);
                break;
            }
        }
    }
    result.trim().to_string()
}

/// Sanitizes past user messages by stripping stale <workspace_context> snapshots and unwrapping <user_request>.
#[allow(dead_code)]
#[must_use]
pub fn sanitize_past_user_message(content: &str) -> String {
    // If the message has <user_request>...</user_request>, extract it directly
    if let (Some(start), Some(end)) = (
        content.find("<user_request>"),
        content.rfind("</user_request>"),
    ) {
        if start < end {
            let inner = &content[start + "<user_request>".len()..end];
            return inner.trim().to_string();
        }
    }

    // Otherwise, handle legacy format where <workspace_context> was appended
    let mut cleaned = content.to_string();
    if let Some(start) = cleaned.find("<workspace_context>") {
        if let Some(end) = cleaned[start..].find("</workspace_context>") {
            let full_end = start + end + "</workspace_context>".len();
            cleaned.replace_range(start..full_end, "");
        } else {
            cleaned.truncate(start);
        }
    }
    cleaned.trim().to_string()
}

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

        // Progressive Agent Skills (Tier 1 index + Tier 2 auto-activation based on active workspace/files)
        let progressive_skills =
            crate::tools::minikit::skills::MiniKitSkillsManager::format_progressive_prompt_context(
                workspace_dir,
                &[],
            );
        if !progressive_skills.is_empty() {
            prompt.push_str(&progressive_skills);
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
        intent_ledger: Option<&crate::context::memory::intent::IntentLedger>,
    ) -> String {
        let mut recency = String::new();
        recency.push_str("\n\n<workspace_context>\n");

        // === ZONE A: KV-CACHE STABLE PREFIX (Ordered by lowest volatility) ===
        // Modern inference runtimes (vLLM, SGLang, LMCache, DeepSeek, Anthropic) utilize
        // radix-tree prefix caching. Placing static or slowly-evolving blocks at the front
        // maximizes cache hits across turns.

        // 1. Scoped DOX Developer Rules (hierarchical AGENTS.md for active working set)
        if !active_working_set.is_empty() {
            let scoped_rules = crate::context::governance::dox::DoxEngine::resolve_scoped_rules(
                workspace_dir,
                active_working_set,
            );
            if !scoped_rules.is_empty() {
                recency.push_str("  <scoped_developer_rules>\n");
                recency.push_str(scoped_rules.trim());
                recency.push_str("\n  </scoped_developer_rules>\n");
            }
        }

        // 2. Progressive 4-Tier Memory (<progressive_memory>)
        let mut prog_memory =
            crate::context::progressive_memory::ProgressiveMemory::load(workspace_dir);
        // Sync any legacy CoreMemory entries into ProgressiveMemory
        let core_memory = crate::context::memory::CoreMemory::load(workspace_dir);
        for g in &core_memory.global_entries {
            prog_memory.add_l3_preference(&g.key, &g.value, "core_memory");
        }
        for l in &core_memory.local_entries {
            prog_memory.add_l2_fact(&l.key, &l.value, "core_memory", 1.0);
        }
        // Apply biological decay retention filter
        prog_memory.prune_decayed(0.15);

        let prog_block = prog_memory.to_prompt_block();
        if !prog_block.is_empty() {
            // Note: prog_block already wraps itself in <progressive_memory> tags
            recency.push_str("  ");
            recency.push_str(prog_block.trim());
            recency.push('\n');
        }

        // 3. Active Working Memory (<task_working_memory>)
        let working_memory = crate::context::working_memory::WorkingMemory::new(workspace_dir);
        let wm_block = working_memory.to_prompt_block();
        if !wm_block.is_empty() {
            recency.push_str("  <task_working_memory>\n");
            recency.push_str(wm_block.trim());
            recency.push_str("\n  </task_working_memory>\n");
        }

        // 4. Active Working Set (recently read / modified files)
        if !active_working_set.is_empty() {
            recency.push_str("  <active_working_set>\n");
            for path in active_working_set.iter().take(8) {
                recency.push_str(&format!("    <file path=\"{}\" />\n", path));
            }
            recency.push_str("  </active_working_set>\n");
        }

        // 5. Living Execution Ledger & Goal Anchor (<goal_anchor>, <execution_ledger>)
        if let Some(ledger) = intent_ledger {
            let ledger_block = ledger.to_prompt_block();
            if !ledger_block.trim().is_empty() {
                recency.push_str(&ledger_block);
            }
        }

        // === KV-CACHE ANCHOR DELIMITER ===
        // Delimits invariant/slowly-evolving context from volatile turn-by-turn state
        recency.push_str("  <!-- KV_CACHE_ANCHOR -->\n");

        // === ZONE B: VOLATILE PER-TURN STATE ===

        // 6. Active Workspace Transaction (if one is open)
        if let Ok(Some(active_tx)) =
            crate::session::transaction::TransactionManager::get_active(workspace_dir)
        {
            recency.push_str(&format!(
                "  <active_transaction id=\"{}\" files_tracked=\"{}\">\n    <description>{}</description>\n  </active_transaction>\n",
                active_tx.tx_id,
                active_tx.files.len(),
                active_tx.description
            ));
        }

        // 7. Git Status & Active Branch
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

        // 8. MiniPower Verification Barrier Reminder (if modifications exist)
        let has_modified_files = git_status
            .as_ref()
            .map(|s| !s.staged.is_empty() || !s.unstaged.is_empty())
            .unwrap_or(false);
        if has_modified_files {
            recency.push_str("  <minipower_verification_barrier>\n");
            recency.push_str("    Notice: Uncommitted changes detected in working tree. Before completing this task, you MUST run `power_verify` (or compile checks & tests via `exec_cmd`) to satisfy the 4-Gate Pre-Completion Verification Barrier. Evidence before assertions always.\n");
            recency.push_str("  </minipower_verification_barrier>\n");
        }

        // 9. Active MiniPower Plan & Pending Tasks (from todo.md)
        let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_dir);
        let todo_candidates = [docs_dir.join("todo.md"), workspace_dir.join("todo.md")];
        for todo_path in &todo_candidates {
            if todo_path.exists() {
                if let Ok(content) = std::fs::read_to_string(todo_path) {
                    let pending: Vec<&str> = content
                        .lines()
                        .filter(|l| l.trim().starts_with("- [ ]") || l.trim().starts_with("* [ ]"))
                        .take(5)
                        .collect();
                    if !pending.is_empty() {
                        recency.push_str("  <minipower_active_plan>\n");
                        let rel_path = todo_path.strip_prefix(workspace_dir).unwrap_or(todo_path);
                        recency.push_str(&format!("    Source: {}\n", rel_path.display()));
                        for task in pending {
                            recency.push_str(&format!("    {}\n", task.trim()));
                        }
                        recency.push_str("  </minipower_active_plan>\n");
                        break;
                    }
                }
            }
        }

        // 10. Dynamic Memory Anchor (active objective, key decisions, blockers)
        if let Some(anchor) = memory_anchor {
            if !anchor.trim().is_empty() {
                recency.push_str("  <task_anchor>\n");
                recency.push_str(anchor.trim());
                recency.push_str("\n  </task_anchor>\n");
            }
        }

        // 9. Context Budget & Headroom Bar (most volatile, placed at tail to avoid invalidating KV prefix)
        if let Some(budget) = context_budget {
            recency.push_str(&budget.to_prompt_block());
        }

        // 10. Intent Course-Correction Drift Warning (<intent_focus>)
        if let Some(drift_warning) = intent_ledger
            .and_then(|l| l.check_drift(crate::constants::DEFAULT_INTENT_DRIFT_WARNING_TURNS))
        {
            recency.push_str("  <intent_focus>\n");
            recency.push_str(&format!("    {}\n", drift_warning.trim()));
            recency.push_str("  </intent_focus>\n");
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
        let mut prog_memory =
            crate::context::progressive_memory::ProgressiveMemory::load(workspace_dir);
        let core_memory = crate::context::memory::CoreMemory::load(workspace_dir);
        for g in &core_memory.global_entries {
            prog_memory.add_l3_preference(&g.key, &g.value, "core_memory");
        }
        for l in &core_memory.local_entries {
            prog_memory.add_l2_fact(&l.key, &l.value, "core_memory", 1.0);
        }
        prog_memory.prune_decayed(0.15);

        let prog_block = prog_memory.to_prompt_block();
        if !prog_block.is_empty() {
            prompt.push_str("\n# Progressive Multi-Tier Memory:\n");
            prompt.push_str(&prog_block);
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
            None,
        );

        assert!(recency.contains("<workspace_context>"));
        assert!(recency.contains("<context_budget used=\"25000\" limit=\"128000\""));
        assert!(recency.contains("branch=\"feature/prompts\""));
        assert!(recency.contains("clean=\"false\""));
        assert!(recency.contains("<file path=\"src/main.rs\" />"));
        assert!(recency.contains("<file path=\"src/agent/prompt.rs\" />"));
        assert!(recency.contains("<active_working_set>"));
        assert!(recency.contains("<!-- KV_CACHE_ANCHOR -->"));
        assert!(recency.contains("<task_anchor>"));
        assert!(recency.contains("Implement Tri-Zone Prompts"));
        assert!(recency.contains("</workspace_context>"));

        // Verify KV-cache prefix stability: stable active_working_set precedes volatile context_budget
        let active_set_pos = recency
            .find("<active_working_set>")
            .expect("must contain active_working_set");
        let anchor_marker_pos = recency
            .find("<!-- KV_CACHE_ANCHOR -->")
            .expect("must contain KV_CACHE_ANCHOR");
        let budget_pos = recency
            .find("<context_budget")
            .expect("must contain context_budget");
        assert!(active_set_pos < anchor_marker_pos);
        assert!(anchor_marker_pos < budget_pos);
    }

    #[test]
    fn test_build_recency_context_with_intent_ledger_and_drift() {
        let temp_dir = std::env::temp_dir();
        let mut ledger =
            crate::context::memory::intent::IntentLedger::new("Build AgentBench sandbox");
        let id = ledger.add_item("Dashboard metrics", None, vec![]);
        ledger.set_status(
            &id,
            crate::context::memory::intent::RequirementStatus::InProgress,
        );

        // Without drift
        let recency =
            PromptBuilder::build_recency_context(&temp_dir, None, &[], None, None, Some(&ledger));
        assert!(recency.contains("<goal_anchor>"));
        assert!(recency.contains("<root_objective>Build AgentBench sandbox</root_objective>"));
        assert!(recency.contains("<execution_ledger progress=\"0/1 completed\">"));
        assert!(recency.contains("[-] Dashboard metrics"));
        assert!(!recency.contains("<intent_focus>"));

        // With drift
        ledger.consecutive_turns_without_progress = 6;
        let recency_drift =
            PromptBuilder::build_recency_context(&temp_dir, None, &[], None, None, Some(&ledger));
        assert!(recency_drift.contains("<intent_focus>"));
        assert!(recency_drift.contains("⚠️ Task Drift Warning"));
        assert!(recency_drift.contains("Dashboard metrics"));

        // Verify order: <goal_anchor> in Zone 1 (before KV_CACHE_ANCHOR), <intent_focus> at tail (after KV_CACHE_ANCHOR)
        let anchor_pos = recency_drift
            .find("<goal_anchor>")
            .expect("must contain goal_anchor");
        let kv_pos = recency_drift
            .find("<!-- KV_CACHE_ANCHOR -->")
            .expect("must contain KV_CACHE_ANCHOR");
        let focus_pos = recency_drift
            .find("<intent_focus>")
            .expect("must contain intent_focus");
        assert!(
            anchor_pos < kv_pos,
            "goal_anchor must be in Zone 1 before KV_CACHE_ANCHOR"
        );
        assert!(
            kv_pos < focus_pos,
            "intent_focus must be at tail after KV_CACHE_ANCHOR"
        );
    }

    #[test]
    fn test_build_recency_context_minipower_barrier_and_plan() {
        let temp_dir = tempfile::tempdir().unwrap();
        let todo_file = temp_dir.path().join("todo.md");
        std::fs::write(
            &todo_file,
            "# Task Tracker\n- [ ] Task 1: Autonomous MiniPower wiring\n- [x] Task 0: Done\n",
        )
        .unwrap();

        let git_status = crate::git::GitStatus {
            branch: "main".to_string(),
            staged: vec!["src/agent/prompt.rs".to_string()],
            unstaged: vec![],
            untracked: vec![],
            conflicted: vec![],
            is_clean: false,
        };

        let recency = PromptBuilder::build_recency_context(
            temp_dir.path(),
            None,
            &[],
            Some(&git_status),
            None,
            None,
        );

        assert!(recency.contains("<minipower_verification_barrier>"));
        assert!(recency.contains("4-Gate Pre-Completion Verification Barrier"));
        assert!(recency.contains("<minipower_active_plan>"));
        assert!(recency.contains("Task 1: Autonomous MiniPower wiring"));
    }
}
