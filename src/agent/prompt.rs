use crate::constants::MAX_AGENTS_MD_BYTES;
use std::path::Path;

pub const STATIC_SYSTEM_PROMPT: &str = r#"You are minicode, an ultra-fast, minimalist AI coding agent built in Rust.
You pair-program with the user to inspect repositories, debug code, design architectures, and implement new features with surgical precision.

# Core Operational Axioms (Karpathy Guidelines):
1. **Think Before Coding — Discover or Ask (`ask_user` / Reversibility Rule)**:
   - For conversational greetings or general questions with no repository task, reply directly in text with zero tool calls and never proactively inspect the workspace or execute pre-existing `todo.md` / `task_plan.md` items unless asked.
   - When a task requires repository changes or investigation, inspect discoverable workspace facts (`read_file`, `grep_search`, `file_search`, `.minicode/decisions.md`) first.
   - Whenever requirements, architecture, stack, UX, or destructive actions are unsettled, call `ask_user` (2–6 options per question, recommended option first with `recommended: true`) instead of guessing.
2. **Scaffold, Don't Hand-Write Boilerplate (`kit_stack_add`)**:
   - Use `kit_stack_add` into the workspace root when bootstrapping a new project stack, and add dependencies via the native package manager (`cargo`, `bun`, `npm`, `uv`).
3. **Modular Files (Anti-Monolith Invariant)**:
   - Keep files under ~250 lines / 10KB. Separate routes, models, services, handlers, and UI components. For large files, write a skeleton first and append/patch sections.
4. **Simplicity First (The Ponytail Minimalist Ladder)**:
   - Climb the 7-rung ladder: (1) Skip if YAGNI, (2) Reuse existing symbols (`<reusable_symbol_map>` / `repo_map`), (3) Standard library, (4) Native platform feature, (5) Installed dependency, (6) One-liner, (7) Minimum working code (`// shortcut: <ceiling>, <when to upgrade>` + honesty footer).
5. **Surgical Changes**: Touch strictly what is required for the task. Never reformat or alter unrelated code, comments, or imports.
6. **Goal-Driven Verification**: Verify every code change via compiler/test checks (`exec_cmd` / `power_verify`, exit code 0 mandatory) and inspect DOM health telemetry on web UIs.
7. **Turn-Agnostic Planning & Live Progress (`create_plan` / `update_progress` / `archive_plan`)**:
   - For multi-step tasks, initialize a right-sized plan via `create_plan`, keep one step `[>]` in progress, advance immediately via `update_progress`, and conclude with `archive_plan`.
8. **Safe-Overwrite Guard**: `write_file` requires `overwrite: true` (replace) or `append: true` (append) on existing files. Prefer `patch_file` for targeted edits.
9. **Skill Discovery (`vault_show` / `vault_load`)**: Load domain/workflow skills from `<available_skills>` on demand and persist reusable lessons via `vault_gotcha_add`.

# Tool Calling & Surgical Editing Protocol:
1. **Read Before Write**: Inspect target files with `read_file` or `locate_symbol` before modifying.
2. **File Writing & Parallel Batching (`write_file`)**: Pass `overwrite: true` or `append: true` when updating existing files. Emit multiple `write_file` / `patch_file` calls targeting distinct files in a single response to execute in parallel.
3. **Surgical Search-and-Replace (`patch_file`)**: Provide unique search blocks with 2-3 lines of context.
4. **Pre-Action Thought**: Emit a concise 1-2 sentence `<thought>...</thought>` before tool calls or final output.
5. **Action Over Verbosity**: Keep explanations minimal; let verified diffs and test outputs speak for themselves.
6. **Positive Error Handling**: Handle errors idiomatically (`?` in Rust, `Result<T, E>`). Never use `.unwrap()` or `.expect()` in production code.
7. **Workspace Path Invariant**: All tool file paths MUST be relative to the workspace root.
8. **Pure Status Icons Only**: Use only 1-cell ASCII/status icons (`✔`, `✖`, `!`, `→`, `•`); never use wide emojis.

# Example patch_file usage:
Target lines in src/main.rs:
    let port = 8080;
    tracing::info!("Listening on port {}", port);
To change port to 9000:
patch_file(path="src/main.rs", search_block="    let port = 8080;\n    tracing::info!(\"Listening on port {}\", port);", replace_block="    let port = 9000;\n    tracing::info!(\"Listening on port {}\", port);")

# Autonomous Subsystems & Self-Healing Protocol:
- **Dynamic Tool Activation (`activate_tools`)**: Only Core tools are loaded as full schemas initially. Call `activate_tools(category="...")` (or `select:tool_a,tool_b`) or invoke any deferred tool directly when needed.
- **Async Background Tasks (`minitask`) & Subagents (`delegate_task`, `manage_subagents`, `fanout_subagents`, `execute_task_dag`)**: Run long builds/servers asynchronously in `minitask` (`is_daemon: true` for dev servers) and delegate parallelizable or context-heavy subtasks to specialist `MiniPub` roles (`scout`, `architect`, `planner`, `designer`, `frontend`, `backend`, `coder`, `vision`, `debugger`, `reviewer`, `security`, `tester`, `simplifier`).
- **Self-Healing**: On truncated JSON (`EOF`), split into smaller edits; on `search block not found`, `read_file` exact lines first; on `AST Syntax Barrier`, fix syntax and retry.
"#;

/// Strips thought/reasoning tags and their inner content from text before saving to LLM context history.
#[must_use]
pub fn strip_thought_blocks(raw: &str) -> String {
    crate::context::auto_compact::AutoCompactor::strip_thought_tags(raw)
}

/// Demultiplexed segment produced by [`ThoughtStreamSplitter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThoughtSegment {
    Thinking(String),
    Content(String),
}

/// Streaming O(N) state machine that separates `<thought>...</thought>`, `<think>...</think>`,
/// and `<thinking>...</thinking>` blocks from assistant content deltas — even when XML tags
/// arrive split across arbitrary network chunk boundaries.
#[derive(Debug, Default, Clone)]
pub struct ThoughtStreamSplitter {
    in_thought: bool,
    pending_tag_buf: String,
    strip_leading_newline_after_close: bool,
}

impl ThoughtStreamSplitter {
    const OPEN_TAGS: &'static [&'static str] = &[
        "<thought>",
        "<think>",
        "<thinking>",
        "<reasoning>",
        "<antThinking>",
        "<Thought>",
        "<Think>",
        "<Thinking>",
        "<Reasoning>",
        "<THINK>",
    ];
    const CLOSE_TAGS: &'static [&'static str] = &[
        "</thought>",
        "</think>",
        "</thinking>",
        "</reasoning>",
        "</antThinking>",
        "</Thought>",
        "</Think>",
        "</Thinking>",
        "</Reasoning>",
        "</THINK>",
    ];

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn earliest_tag_match<'a>(text: &str, tags: &[&'a str]) -> Option<(usize, &'a str)> {
        let mut best: Option<(usize, &'a str)> = None;
        for &tag in tags {
            if let Some(idx) = text.find(tag) {
                if best.is_none_or(|(b_idx, _)| idx < b_idx) {
                    best = Some((idx, tag));
                }
            }
        }
        best
    }

    fn trailing_partial_tag_start(text: &str, tags: &[&str]) -> Option<usize> {
        let pos = text.rfind('<')?;
        let tail = text.get(pos..)?;
        if tags.iter().any(|t| t.starts_with(tail) && *t != tail) {
            Some(pos)
        } else {
            None
        }
    }

    pub fn push(&mut self, delta: &str) -> Vec<ThoughtSegment> {
        if delta.is_empty() {
            return Vec::new();
        }
        let mut working = if self.pending_tag_buf.is_empty() {
            delta.to_string()
        } else {
            let mut combined = std::mem::take(&mut self.pending_tag_buf);
            combined.push_str(delta);
            combined
        };

        let mut out = Vec::new();
        while !working.is_empty() {
            if !self.in_thought && self.strip_leading_newline_after_close {
                let trimmed = working.trim_start_matches(['\r', '\n']);
                let stripped_len = working.len() - trimmed.len();
                if stripped_len > 0 {
                    working.drain(..stripped_len);
                    if working.is_empty() {
                        break;
                    }
                }
                self.strip_leading_newline_after_close = false;
            }

            if self.in_thought {
                if let Some((end_idx, close_tag)) =
                    Self::earliest_tag_match(&working, Self::CLOSE_TAGS)
                {
                    let thought_part = &working[..end_idx];
                    if !thought_part.is_empty() {
                        out.push(ThoughtSegment::Thinking(thought_part.to_string()));
                    }
                    self.in_thought = false;
                    self.strip_leading_newline_after_close = true;
                    working.drain(..end_idx + close_tag.len());
                } else if let Some(partial_pos) =
                    Self::trailing_partial_tag_start(&working, Self::CLOSE_TAGS)
                {
                    if partial_pos > 0 {
                        out.push(ThoughtSegment::Thinking(working[..partial_pos].to_string()));
                    }
                    self.pending_tag_buf = working[partial_pos..].to_string();
                    break;
                } else {
                    out.push(ThoughtSegment::Thinking(working));
                    break;
                }
            } else if let Some((start_idx, open_tag)) =
                Self::earliest_tag_match(&working, Self::OPEN_TAGS)
            {
                let content_part = &working[..start_idx];
                if !content_part.is_empty() {
                    out.push(ThoughtSegment::Content(content_part.to_string()));
                }
                self.in_thought = true;
                working.drain(..start_idx + open_tag.len());
            } else if let Some(partial_pos) =
                Self::trailing_partial_tag_start(&working, Self::OPEN_TAGS)
            {
                if partial_pos > 0 {
                    out.push(ThoughtSegment::Content(working[..partial_pos].to_string()));
                }
                self.pending_tag_buf = working[partial_pos..].to_string();
                break;
            } else {
                out.push(ThoughtSegment::Content(working));
                break;
            }
        }
        out
    }

    pub fn flush(&mut self) -> Vec<ThoughtSegment> {
        let mut out = Vec::new();
        if !self.pending_tag_buf.is_empty() {
            let rem = std::mem::take(&mut self.pending_tag_buf);
            if self.in_thought {
                out.push(ThoughtSegment::Thinking(rem));
            } else {
                out.push(ThoughtSegment::Content(rem));
            }
        }
        self.in_thought = false;
        self.strip_leading_newline_after_close = false;
        out
    }
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

/// Formats unread peer messages into a structured XML block for turn-start context injection.
#[must_use]
pub fn format_peer_messages_prompt(messages: &[crate::agent::swarm::bus::SwarmMessage]) -> String {
    if messages.is_empty() {
        return String::new();
    }

    let mut out =
        String::from("\n<peer_messages>\n### INCOMING PEER WORKER COORDINATION MESSAGES:\n");
    for msg in messages {
        out.push_str(&msg.format_for_prompt());
        out.push('\n');
    }
    out.push_str("</peer_messages>\n");
    out
}

#[allow(dead_code)]
pub const DEFAULT_SYSTEM_PROMPT: &str = STATIC_SYSTEM_PROMPT;

pub struct PromptBuilder;

impl PromptBuilder {
    /// Resolves the active Caveman compression level from workspace config or `.caveman-active` flag.
    fn resolve_caveman_level(workspace_dir: &Path) -> crate::agent::caveman::CavemanLevel {
        let app_cfg = crate::config::Config::load_for_workspace(workspace_dir);
        let flag_level = crate::agent::caveman::read_flag_safe(
            &crate::agent::caveman::default_flag_path(workspace_dir),
        );
        let caveman_enabled =
            app_cfg.agent.caveman_mode || flag_level.map(|l| l.is_enabled()).unwrap_or(false);
        if caveman_enabled {
            flag_level.filter(|l| l.is_enabled()).unwrap_or_else(|| {
                let parsed = crate::agent::caveman::CavemanLevel::from_str_loose(
                    &app_cfg.agent.caveman_level,
                );
                if parsed == crate::agent::caveman::CavemanLevel::Off {
                    crate::agent::caveman::CavemanLevel::Full
                } else {
                    parsed
                }
            })
        } else {
            crate::agent::caveman::CavemanLevel::Off
        }
    }

    /// Loads `ProgressiveMemory`, syncs legacy `CoreMemory` entries, prunes decayed facts, and formats the prompt block.
    fn load_synced_progressive_memory_block(workspace_dir: &Path) -> String {
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
        prog_memory.to_prompt_block()
    }

    /// Assembles the 100% static, cache-friendly system prompt.
    /// This prompt is immutable across turns for maximum prefix caching hits.
    #[must_use]
    pub fn build_static_system_prompt(
        workspace_dir: &Path,
        custom_instructions: Option<&str>,
    ) -> String {
        let active_caveman_level = Self::resolve_caveman_level(workspace_dir);
        let mut prompt = format!(
            "{}\n\n{}",
            STATIC_SYSTEM_PROMPT,
            crate::agent::features::format_features_catalog()
        );

        prompt.push_str(&format!(
            "\n# Current Workspace:\n{}\n",
            workspace_dir.display()
        ));

        // Inject canonical .minicode/minicode.md rules (after auto-harvesting any useful project rules
        // from legacy/foreign files while stripping foreign agent personas/tool noise)

        if let Some(content) =
            crate::context::governance::dox::DoxEngine::read_canonical_root_rules(workspace_dir)
        {
            let trimmed = if content.len() > MAX_AGENTS_MD_BYTES {
                tracing::warn!(
                    size = content.len(),
                    max = MAX_AGENTS_MD_BYTES,
                    ".minicode/minicode.md exceeds size limit; truncating"
                );
                let valid_end = content.floor_char_boundary(MAX_AGENTS_MD_BYTES);
                &content[..valid_end]
            } else {
                &content
            };
            let effective_rules_md =
                crate::agent::caveman::compress_markdown_ast_auto(trimmed, active_caveman_level);
            prompt.push_str("\n# Repository Guidelines (.minicode/minicode.md):\n");
            prompt.push_str(&effective_rules_md);
            prompt.push('\n');
        }

        // MiniVault Active Stack & Methodology Skills Context
        // (Automatically matches workspace stack from manifests/config files + workspace files + project skills + index of on-demand skills)
        let vault_store = crate::vault::store::VaultStore::new(workspace_dir);
        let (_, mut workspace_files, doc_files) =
            crate::agent::orchestrator::WorkflowRouter::scan_repository_state(workspace_dir);
        workspace_files.extend(doc_files);
        let _ = vault_store.sync_project_active_skills(&workspace_files);
        let skills_context = vault_store.format_prompt_skills_context(None, None, &workspace_files);
        if !skills_context.is_empty() {
            let effective_skills = crate::agent::caveman::compress_markdown_ast_auto(
                &skills_context,
                active_caveman_level,
            );
            prompt.push_str(&effective_skills);
        }

        // Append custom user/turn instructions if provided
        if let Some(custom) = custom_instructions {
            prompt.push_str("\n# Additional Instructions:\n");
            prompt.push_str(custom);
            prompt.push('\n');
        }

        // Native Caveman Mode (<caveman_mode>) if enabled in Config or .caveman-active flag
        if active_caveman_level.is_enabled() {
            prompt.push_str(&crate::agent::caveman::build_caveman_system_block(
                true,
                active_caveman_level.as_str(),
            ));
        }

        // MiniBlocks Native UI Component & Design Warehouse Blueprint
        let stack_info = crate::blocks::seed::ProjectStackInfo::detect(workspace_dir);
        let stack_directive = stack_info.format_prompt_directive();

        prompt.push_str(&format!(
            "\n  <miniblocks_warehouse>\n\
    Native UI warehouse: 1,080+ components, 105 palettes, 212 gradients, 3 templates.\n\
    Tools: `block_search`, `block_get`, `block_insert`, `block_palettes`, `block_gradients`, `block_scaffold`, `block_import`.\n\
    {}\n\
  </miniblocks_warehouse>\n",
            stack_directive.trim()
        ));

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
        workflow_enrichment: Option<&str>,
    ) -> String {
        Self::build_recency_context_with_signal(
            workspace_dir,
            memory_anchor,
            active_working_set,
            git_status,
            context_budget,
            intent_ledger,
            workflow_enrichment,
            None,
        )
    }

    /// Signal-gated Recency Zone builder using `PromptSignalVector` to allocate context
    /// proportionally to the turn's actionable task energy and plan-continuation affinity.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn build_recency_context_with_signal(
        workspace_dir: &Path,
        memory_anchor: Option<&str>,
        active_working_set: &[String],
        git_status: Option<&crate::git::GitStatus>,
        context_budget: Option<&crate::context::budget::ContextBudget>,
        intent_ledger: Option<&crate::context::memory::intent::IntentLedger>,
        workflow_enrichment: Option<&str>,
        signal: Option<crate::context::search::intent_filter::PromptSignalVector>,
    ) -> String {
        let mut recency = String::new();
        recency.push_str("\n\n<workspace_context>\n");
        let active_caveman_level = Self::resolve_caveman_level(workspace_dir);

        let include_task_state = signal.is_none_or(|s| s.has_actionable_task_intent());
        let include_plan_state = signal.is_none_or(|s| s.has_plan_continuation_affinity());
        let include_code_graph = signal.is_none_or(|s| s.has_code_exploration_intent());

        if !include_task_state {
            if let Some(telemetry) = Self::read_session_tool_telemetry(workspace_dir) {
                recency.push_str("  <session_tool_telemetry>\n");
                recency.push_str("    Executed tools in this session (ground-truth provenance — cite ONLY these when asked which tools or skills were used; never claim uninvoked tools):\n    ");
                recency.push_str(&telemetry);
                recency.push_str("\n  </session_tool_telemetry>\n");
            }
            if let Some(enrichment) = workflow_enrichment {
                if !enrichment.trim().is_empty() {
                    recency.push_str("  ");
                    recency.push_str(enrichment.trim());
                    recency.push('\n');
                }
            }
            if active_caveman_level.is_enabled() {
                recency.push_str(&crate::agent::caveman::build_caveman_recency_block(
                    true,
                    active_caveman_level.as_str(),
                ));
            }
            recency.push_str("</workspace_context>");
            return recency;
        }

        let scoped_rules =
            crate::context::governance::dox::DoxEngine::resolve_scoped_rules_with_headroom(
                workspace_dir,
                active_working_set,
                context_budget,
            );
        if !scoped_rules.is_empty() {
            let effective_rules = if active_caveman_level.is_enabled() {
                crate::agent::caveman::compress_markdown_ast_auto(
                    scoped_rules.trim(),
                    active_caveman_level,
                )
            } else {
                scoped_rules.trim().to_string()
            };
            recency.push_str("  <scoped_developer_rules>\n");
            recency.push_str(&effective_rules);
            recency.push_str("\n  </scoped_developer_rules>\n");
        }

        // 2. Progressive 4-Tier Memory (<progressive_memory>)
        let prog_block = Self::load_synced_progressive_memory_block(workspace_dir);
        if !prog_block.is_empty() {
            // Note: prog_block already wraps itself in <progressive_memory> tags
            recency.push_str("  ");
            recency.push_str(prog_block.trim());
            recency.push('\n');
        }

        // 3. Active Working Memory (<task_working_memory>)
        if include_plan_state {
            let working_memory = crate::context::working_memory::WorkingMemory::new(workspace_dir);
            let wm_block = working_memory.to_prompt_block();
            if !wm_block.is_empty() {
                recency.push_str("  <task_working_memory>\n");
                recency.push_str(wm_block.trim());
                recency.push_str("\n  </task_working_memory>\n");
            }
        }

        // 3b. Session Wisdom Notepad (<session_wisdom_anchor>) — persists subagent learnings & pre-compaction operational facts
        let wisdom_path = workspace_dir
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::SESSION_WISDOM_FILE);
        if let Ok(wisdom_raw) = std::fs::read_to_string(&wisdom_path) {
            let trimmed_w = wisdom_raw.trim();
            if !trimmed_w.is_empty() {
                let tail_w = if trimmed_w.len() > 1_200 {
                    let start_idx = trimmed_w.floor_char_boundary(trimmed_w.len() - 1_200);
                    format!("...\n{}", &trimmed_w[start_idx..])
                } else {
                    trimmed_w.to_string()
                };
                recency.push_str("  <session_wisdom_anchor>\n");
                recency.push_str(tail_w.trim());
                recency.push_str("\n  </session_wisdom_anchor>\n");
            }
        }

        // 4. Active Working Set (recently read / modified files)
        if !active_working_set.is_empty() {
            recency.push_str("  <active_working_set>\n");
            for path in active_working_set.iter().take(8) {
                recency.push_str(&format!("    <file path=\"{}\" />\n", path));
            }
            recency.push_str("  </active_working_set>\n");
        }

        // 4b. Compact PageRank-Ordered Symbol Reuse Map (`dir/: sym1, sym2, ...`)
        if include_code_graph {
            if let Some(graph) =
                crate::context::graph::CodeGraph::try_get_or_load_fast(workspace_dir)
            {
                let active_paths: Vec<std::path::PathBuf> = active_working_set
                    .iter()
                    .map(|p| workspace_dir.join(p))
                    .collect();
                let reuse_map = graph.format_compact_reuse_map(
                    workspace_dir,
                    &active_paths,
                    crate::constants::REUSE_MAP_DEFAULT_CHAR_BUDGET,
                );
                if !reuse_map.is_empty() {
                    recency.push_str(
                        "  <reusable_symbol_map hint=\"Existing workspace symbols ranked by CodeGraph PageRank & inbound edges — reuse before creating new helpers\">\n",
                    );
                    for line in reuse_map.lines() {
                        recency.push_str("    ");
                        recency.push_str(line);
                        recency.push('\n');
                    }
                    recency.push_str("  </reusable_symbol_map>\n");
                }
            }
        }

        // 5. Living Execution Ledger & Goal Anchor (<goal_anchor>, <execution_ledger>)
        if let Some(ledger) = intent_ledger {
            let ledger_block = ledger.to_prompt_block();
            if !ledger_block.trim().is_empty() {
                recency.push_str(&ledger_block);
            }
        }

        // 6. Verified Active Architectural Invariants (Bi-Temporal Memory)
        let temp_mem =
            crate::context::temporal_memory::TemporalMemoryStore::load_or_default(workspace_dir);
        let temp_mem_block = temp_mem.format_active_context();
        if !temp_mem_block.is_empty() {
            recency.push_str("  ");
            recency.push_str(temp_mem_block.trim());
            recency.push('\n');
        }

        // 7. Recent Architectural Decisions Provenance
        let recent_decisions =
            crate::context::decision_record::DecisionLogger::format_recent_for_context(
                workspace_dir,
                5,
            );
        if !recent_decisions.is_empty() {
            recency.push_str("  ");
            recency.push_str(recent_decisions.trim());
            recency.push('\n');
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
        if has_modified_files && (include_plan_state || !active_working_set.is_empty()) {
            recency.push_str("  <minipower_verification_barrier>\n");
            recency.push_str("    Notice: Uncommitted changes detected in working tree. Before completing this task, you MUST run `power_verify` (or compile checks & tests via `exec_cmd`) to satisfy the 4-Gate Pre-Completion Verification Barrier. Evidence before assertions always.\n");
            recency.push_str("  </minipower_verification_barrier>\n");
            if let Some(status) = git_status {
                let mut touched: Vec<String> = status.staged.clone();
                touched.extend(status.unstaged.clone());
                let dox_closeout =
                    crate::context::governance::dox::DoxEngine::execute_closeout_pass(
                        workspace_dir,
                        &touched,
                        false,
                    );
                let dox_block = dox_closeout.to_prompt_block();
                if !dox_block.is_empty() {
                    recency.push_str(&dox_block);
                }
            }
        }

        // 8b. Live Ponytail Quality Telemetry (`reuse`, `open_callers`, `test_freshness`)
        if let Some(quality_block) =
            crate::agent::verification_barrier::SessionQualityTelemetry::format_prompt_block(
                workspace_dir,
            )
        {
            recency.push_str(&quality_block);
        }

        // 9. Active Background Tasks & Services (MiniTask Manager & Live Browser)
        let (active_task_count, total_rss_mb, avg_cpu) =
            crate::dev::registry::get_global_dev_registry().get_telemetry_snapshot();
        let browser_live = crate::tools::browser::BrowserManager::is_live_engine_running_sync();
        if active_task_count > 0 || browser_live {
            recency.push_str("  <active_background_tasks>\n");
            recency.push_str(&format!(
                "    Status: {} background task(s) supervised under MiniTask Manager (Total RAM: {:.1}MB, CPU: {:.1}%){}\n",
                active_task_count,
                total_rss_mb,
                avg_cpu,
                if browser_live { " | Live browser running" } else { "" }
            ));
            recency.push_str("    Rules: Keep dev servers running across turns (NEVER stop a dev server at turn end unless explicitly requested by the user); close headless browsers via `browser_close` when inspection finishes (keep GUI browsers open); use `minitask(action=\"resources\"|\"status\"|\"stop\"|\"kill_all\")` internally instead of raw `ps`/`top`/`kill`/`pkill`, and never tell the human user to type internal tool calls like `minitask(...)` in final prose.\n");
            recency.push_str("  </active_background_tasks>\n");
        }

        // 9. Project Blueprint Index (Core Specs in core/ + Root DESIGN.md available for on-demand inspection via read_file)
        let core_dir = crate::tools::minikit::resolve_core_docs_dir(workspace_dir);
        let has_root_design_md = workspace_dir.join("DESIGN.md").is_file();
        if core_dir.exists() || has_root_design_md {
            let mut available_specs = Vec::new();
            if core_dir.exists() {
                for spec_file in crate::tools::minikit::CORE_DOC_FILES {
                    if core_dir.join(spec_file).exists() {
                        available_specs.push(*spec_file);
                    }
                }
            }
            if !available_specs.is_empty() || has_root_design_md {
                recency.push_str("  <project_blueprint>\n");
                if !available_specs.is_empty() {
                    let rel_core = core_dir.strip_prefix(workspace_dir).unwrap_or(&core_dir);
                    recency.push_str(&format!(
                        "    Location: {}/ (Inspect on-demand via `read_file`)\n",
                        rel_core.display()
                    ));
                    recency.push_str(&format!("    Specs: {}\n", available_specs.join(", ")));
                }
                if has_root_design_md {
                    recency.push_str(
                        "    Root Design System: DESIGN.md (VoltAgent/Stitch 9-Section Design System — adhere strictly to its tokens, typography, and component rules)\n",
                    );
                }
                recency.push_str("  </project_blueprint>\n");
            }
        }

        // 9b. Recorded user decisions (from ask_user), so settled choices survive
        // compaction and later turns and are never re-asked.
        if let Some(decisions) = crate::agent::decisions::read_for_prompt(workspace_dir) {
            recency.push_str("  <user_decisions>\n");
            recency.push_str("    Settled by the user (source: .minicode/decisions.md). Build on these; do not re-ask.\n");
            for line in decisions.lines() {
                recency.push_str("    ");
                recency.push_str(line);
                recency.push('\n');
            }
            recency.push_str("  </user_decisions>\n");
        }

        // 9c. Factual Session Tool & Skill Telemetry (from structured session JSONL log)
        // Ensures the agent has 100% accurate ground-truth provenance of tools and skills
        // actually executed in the session even after context compaction.
        if let Some(telemetry) = Self::read_session_tool_telemetry(workspace_dir) {
            recency.push_str("  <session_tool_telemetry>\n");
            recency.push_str("    Executed tools in this session (ground-truth provenance — cite ONLY these when asked which tools or skills were used; never claim uninvoked tools):\n    ");
            recency.push_str(&telemetry);
            recency.push_str("\n  </session_tool_telemetry>\n");
        }

        // 10. Active MiniPower Plan & Pending Tasks (prioritizes active task plan, falls back to roadmap)
        if include_plan_state {
            let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_dir);
            if let Some(summary) = wm.read_active_plan_summary() {
                recency.push_str("  <minipower_active_plan>\n");
                recency.push_str(&format!("    Phase / Goal: {}\n", summary.phase_label));
                recency.push_str(&format!(
                    "    Progress: {}/{} tasks completed\n",
                    summary.completed_tasks, summary.total_tasks
                ));
                if let Some(ref active) = summary.active_task {
                    recency.push_str(&format!("    ► Current Step: [>] \"{}\"\n", active));
                }
                recency.push_str("    Task Steps:\n");
                for task in &summary.tasks {
                    let badge = match task.status {
                        crate::context::memory::working_memory::TaskItemStatus::Completed => "[x]",
                        crate::context::memory::working_memory::TaskItemStatus::InProgress => "[>]",
                        crate::context::memory::working_memory::TaskItemStatus::Pending => "[ ]",
                    };
                    recency.push_str(&format!("      • {} {}\n", badge, task.title));
                    for sub in &task.sub_tasks {
                        let sub_badge = match sub.status {
                            crate::context::memory::working_memory::TaskItemStatus::Completed => {
                                "[x]"
                            }
                            crate::context::memory::working_memory::TaskItemStatus::InProgress => {
                                "[>]"
                            }
                            crate::context::memory::working_memory::TaskItemStatus::Pending => {
                                "[ ]"
                            }
                        };
                        recency.push_str(&format!("        ◦ {} {}\n", sub_badge, sub.title));
                    }
                }
                if summary.completed_tasks == summary.total_tasks && summary.total_tasks > 0 {
                    let verify_passed = std::fs::read_to_string(
                        workspace_dir.join(".minicode/plan/last_verify.json"),
                    )
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .and_then(|v| v.get("passed").and_then(|b| b.as_bool()))
                    .unwrap_or(false);
                    if verify_passed {
                        recency.push_str("    STATUS: All steps [x] and `power_verify` PASSED. Call `archive_plan` (and optionally `power_review`) to finalize.\n");
                    } else {
                        recency.push_str("    STATUS: All steps [x] completed! Run `power_verify` (4-Gate Verification Barrier) and optionally `power_review`, then call `archive_plan`.\n");
                    }
                } else {
                    recency.push_str("    INVARIANT: Focus strictly on executing the current step ([>]). If the current step is multi-part, you can decompose it into sub-steps with `power_plan(topic=\"...\", steps=[...])`. Once completed, call `update_progress(step=\"active\", status=\"completed\")` immediately to advance to the next step. Never batch progress updates!\n");
                }
                recency.push_str("  </minipower_active_plan>\n");
            } else {
                let resolved_todo =
                    crate::tools::minikit::resolve_doc_path(workspace_dir, "todo.md");
                let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_dir);
                let todo_candidates = [
                    resolved_todo,
                    docs_dir.join("todo.md"),
                    workspace_dir.join("todo.md"),
                ];
                for todo_path in &todo_candidates {
                    if todo_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(todo_path) {
                            let pending: Vec<&str> = content
                                .lines()
                                .filter(|l| {
                                    let trimmed = l.trim();
                                    trimmed.starts_with("- [ ]")
                                        || trimmed.starts_with("* [ ]")
                                        || trimmed.starts_with("- [>]")
                                        || trimmed.starts_with("* [>]")
                                })
                                .take(5)
                                .collect();
                            if !pending.is_empty() {
                                recency.push_str("  <minipower_active_plan>\n");
                                let rel_path =
                                    todo_path.strip_prefix(workspace_dir).unwrap_or(todo_path);
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

        // 11. Workflow-Specific Autonomous Guidance & Warehouse Enrichments
        if let Some(enrichment) = workflow_enrichment {
            if !enrichment.trim().is_empty() {
                recency.push_str("  ");
                recency.push_str(enrichment.trim());
                recency.push('\n');
            }
        }

        // 12. Native Caveman Mode Recency Anchor (<caveman_active>) & Rock 6 Reusable Verification Blocks
        if active_caveman_level.is_enabled() {
            recency.push_str(&crate::agent::caveman::build_caveman_recency_block(
                true,
                active_caveman_level.as_str(),
            ));
        }
        let saved_blocks = crate::agent::caveman::CavemanBlockStore::list_blocks(workspace_dir);
        if !saved_blocks.is_empty() {
            let block_list: Vec<String> = saved_blocks
                .iter()
                .take(8)
                .map(|(slug, rel)| format!("`{}` (`bash {}`)", slug, rel))
                .collect();
            recency.push_str(&format!(
                "  <caveman_blocks>\n    Reusable verification/inspection scripts (Rock 6 — run in 1 line instead of regenerating pipelines): {}\n  </caveman_blocks>\n",
                block_list.join(", ")
            ));
        }

        recency.push_str("</workspace_context>");
        recency
    }

    /// Reads the latest structured session JSONL log in `.minicode/sessions/` and aggregates
    /// the ordered set of tools and skills actually executed during the session.
    fn read_session_tool_telemetry(workspace_dir: &Path) -> Option<String> {
        let sessions_dir = workspace_dir.join(".minicode/sessions");
        let entries = std::fs::read_dir(&sessions_dir).ok()?;
        let latest_jsonl = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
            .max_by_key(|p| {
                p.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            })?;

        let content = std::fs::read_to_string(&latest_jsonl).ok()?;
        let mut counts: Vec<(String, usize)> = Vec::new();
        for line in content.lines() {
            let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if val.get("event").and_then(|e| e.as_str()) != Some("tool_call") {
                continue;
            }
            let Some(tool) = val.get("tool").and_then(|t| t.as_str()) else {
                continue;
            };
            let args = val.get("args");
            let label = match tool {
                "skill_load" => args
                    .and_then(|a| a.get("name"))
                    .and_then(|s| s.as_str())
                    .map(|name| format!("skill_load[{name}]"))
                    .unwrap_or_else(|| "skill_load".to_string()),
                "kit_stack_add" => args
                    .and_then(|a| a.get("stack"))
                    .and_then(|s| s.as_str())
                    .map(|stack| format!("kit_stack_add[{stack}]"))
                    .unwrap_or_else(|| "kit_stack_add".to_string()),
                _ => tool.to_string(),
            };
            if let Some(entry) = counts.iter_mut().find(|(k, _)| *k == label) {
                entry.1 += 1;
            } else {
                counts.push((label, 1));
            }
        }

        if counts.is_empty() {
            return None;
        }

        let summary = counts
            .into_iter()
            .map(|(k, count)| format!("{k} ({count}x)"))
            .collect::<Vec<_>>()
            .join(", ");
        Some(summary)
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
        let prog_block = Self::load_synced_progressive_memory_block(workspace_dir);
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
        assert!(prompt.contains("<miniblocks_warehouse>"));
        assert!(prompt.contains("Native UI warehouse: 1,080+ components"));
        assert!(prompt.contains("Detected Project Stack:"));
    }

    #[test]
    fn test_build_system_prompt_with_detected_stack_react_tailwind() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_stack_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let pkg = r#"{
            "name": "portal",
            "dependencies": {
                "react": "^18.2.0",
                "tailwindcss": "^3.4.0"
            }
        }"#;
        std::fs::write(temp_dir.join("package.json"), pkg).unwrap();

        let prompt = PromptBuilder::build_system_prompt(&temp_dir, None);
        assert!(prompt.contains("<miniblocks_warehouse>"));
        assert!(prompt.contains("Detected Project Stack: React with Tailwind CSS."));
        assert!(prompt.contains("framework=\"react\" or framework=\"tailwind\""));

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
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
        assert!(prompt.contains("Reversibility Rule"));
        assert!(!prompt.contains("MUST invoke `ask_user`"));
        assert!(prompt.contains("ask_user"));
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
        let recency = PromptBuilder::build_recency_context(
            &temp_dir,
            None,
            &[],
            None,
            None,
            Some(&ledger),
            None,
        );
        assert!(recency.contains("<goal_anchor>"));
        assert!(recency.contains("<root_objective>Build AgentBench sandbox</root_objective>"));
        assert!(recency.contains("<execution_ledger progress=\"0/1 completed\">"));
        assert!(recency.contains("[-] Dashboard metrics"));
        assert!(!recency.contains("<intent_focus>"));

        // With drift
        ledger.consecutive_turns_without_progress = 6;
        let recency_drift = PromptBuilder::build_recency_context(
            &temp_dir,
            None,
            &[],
            None,
            None,
            Some(&ledger),
            None,
        );
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
            None,
        );

        assert!(recency.contains("<minipower_verification_barrier>"));
        assert!(recency.contains("4-Gate Pre-Completion Verification Barrier"));
        assert!(recency.contains("<minipower_active_plan>"));
        assert!(recency.contains("Task 1: Autonomous MiniPower wiring"));
    }

    #[test]
    fn test_build_recency_context_with_workflow_enrichment() {
        let temp_dir = tempfile::tempdir().unwrap();
        let enrichment =
            "<recommended_miniblocks>\n  • `navbar_simple` (React)\n</recommended_miniblocks>";
        let recency = PromptBuilder::build_recency_context(
            temp_dir.path(),
            None,
            &[],
            None,
            None,
            None,
            Some(enrichment),
        );
        assert!(recency.contains("<workspace_context>"));
        assert!(recency.contains("<recommended_miniblocks>"));
        assert!(recency.contains("`navbar_simple` (React)"));
        assert!(recency.contains("</workspace_context>"));
    }

    #[test]
    fn test_format_peer_messages_block() {
        let msg = crate::agent::swarm::bus::SwarmMessage::new(
            "swarm-1",
            "t1_api",
            None,
            crate::agent::swarm::bus::SwarmMessageIntent::PublishContract,
            "User Schema",
            "export interface User { id: string; }",
        );
        let block = crate::agent::prompt::format_peer_messages_prompt(&[msg]);
        assert!(block.contains("<peer_messages>"));
        assert!(block.contains("export interface User"));
        assert!(block.contains("</peer_messages>"));

        // Verify empty slice produces empty string
        assert_eq!(crate::agent::prompt::format_peer_messages_prompt(&[]), "");
    }

    #[test]
    fn test_build_recency_context_with_signal_gates_plan_and_barrier_on_conversational_turns() {
        let temp_dir = tempfile::tempdir().unwrap();
        let todo_file = temp_dir.path().join("todo.md");
        let plan_md =
            "# Task Tracker\n- [ ] Task 1: Autonomous MiniPower wiring\n- [x] Task 0: Done\n";
        std::fs::write(&todo_file, plan_md).unwrap();

        let git_status = crate::git::GitStatus {
            branch: "main".to_string(),
            staged: vec!["src/agent/prompt.rs".to_string()],
            unstaged: vec![],
            untracked: vec![],
            conflicted: vec![],
            is_clean: false,
        };

        // 1. Conversational turn ("hii") -> zero task energy -> omits plan, barrier, and repo noise
        let sig_hii = crate::context::search::intent_filter::IntentClassifier::analyze_signal(
            "hii",
            Some(plan_md),
        );
        let recency_hii = PromptBuilder::build_recency_context_with_signal(
            temp_dir.path(),
            None,
            &[],
            Some(&git_status),
            None,
            None,
            None,
            Some(sig_hii),
        );
        assert!(!recency_hii.contains("<minipower_active_plan>"));
        assert!(!recency_hii.contains("<minipower_verification_barrier>"));
        assert!(!recency_hii.contains("<task_working_memory>"));

        // 2. Plan continuation turn ("continue the todo") -> includes plan and barrier
        let sig_cont = crate::context::search::intent_filter::IntentClassifier::analyze_signal(
            "continue the todo",
            Some(plan_md),
        );
        let recency_cont = PromptBuilder::build_recency_context_with_signal(
            temp_dir.path(),
            None,
            &[],
            Some(&git_status),
            None,
            None,
            None,
            Some(sig_cont),
        );
        assert!(recency_cont.contains("<minipower_active_plan>"));
        assert!(recency_cont.contains("<minipower_verification_barrier>"));
    }

    #[test]
    fn test_prompt_tool_references_resolve_in_meta() {
        let prompt = STATIC_SYSTEM_PROMPT;
        let expected_tools = [
            "read_file",
            "grep_search",
            "file_search",
            "ask_user",
            "kit_stack_add",
            "repo_map",
            "exec_cmd",
            "power_verify",
            "create_plan",
            "update_progress",
            "archive_plan",
            "write_file",
            "patch_file",
            "vault_show",
            "vault_load",
            "vault_gotcha_add",
            "locate_symbol",
            "activate_tools",
            "minitask",
            "delegate_task",
            "manage_subagents",
            "fanout_subagents",
            "execute_task_dag",
        ];
        for tool in expected_tools {
            assert!(
                prompt.contains(tool),
                "STATIC_SYSTEM_PROMPT should reference canonical tool `{}`",
                tool
            );
            assert!(
                tool == "activate_tools"
                    || crate::tools::meta::is_registered(tool)
                    || crate::tools::meta::is_registered(crate::tools::meta::canonical_name(tool)),
                "Prompt tool `{}` must be registered in tools::meta",
                tool
            );
        }
        for foreign in [
            "spawn_agent",
            "symbol_search",
            "fanout_agents",
            "dispatch_subagent",
        ] {
            assert!(
                !prompt.contains(foreign),
                "STATIC_SYSTEM_PROMPT must not reference deprecated/foreign tool `{}`",
                foreign
            );
        }
    }

    #[test]
    fn test_thought_stream_splitter_handles_split_tags_and_preserves_angle_brackets() {
        let mut splitter = super::ThoughtStreamSplitter::new();
        let chunks = [
            "<tho",
            "ught>Step 1: verify a < b and <div />",
            "</th",
            "ought>\n\nFinal answer: x < y.",
            " <thi",
            "nk>Quick check</think> Done!",
        ];

        let mut thinking = String::new();
        let mut content = String::new();
        for ch in chunks {
            for seg in splitter.push(ch) {
                match seg {
                    super::ThoughtSegment::Thinking(t) => thinking.push_str(&t),
                    super::ThoughtSegment::Content(c) => content.push_str(&c),
                }
            }
        }
        for seg in splitter.flush() {
            match seg {
                super::ThoughtSegment::Thinking(t) => thinking.push_str(&t),
                super::ThoughtSegment::Content(c) => content.push_str(&c),
            }
        }

        assert_eq!(thinking, "Step 1: verify a < b and <div />Quick check");
        assert_eq!(content.trim(), "Final answer: x < y.  Done!");
        assert!(!content.contains("<thought>"));
        assert!(!content.contains("</thought>"));
        assert!(!content.contains("<think>"));
    }

    #[test]
    fn test_thought_stream_splitter_multibyte_utf8_char_boundaries() {
        // Exact crash geometry: 21-byte chunk with 3-byte '—' spanning bytes 9..12
        let exact_crash_chunk = "123456789—123456789";
        assert_eq!(exact_crash_chunk.len(), 21);
        let mut splitter = super::ThoughtStreamSplitter::new();
        let segs = splitter.push(exact_crash_chunk);
        assert_eq!(
            segs,
            vec![super::ThoughtSegment::Content(
                exact_crash_chunk.to_string()
            )]
        );

        // Exhaustive split across every valid UTF-8 char boundary in a multi-byte stream
        let full = "<thought>Step 1 — verify • icons ✔ → CJK 你好</thought>\nResult — ready • ✔!";
        let boundaries: Vec<usize> = full
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(full.len()))
            .collect();

        for step in 1..=13 {
            let mut s = super::ThoughtStreamSplitter::new();
            let mut got_think = String::new();
            let mut got_content = String::new();
            let mut idx = 0;
            while idx + 1 < boundaries.len() {
                let next_idx = (idx + step).min(boundaries.len() - 1);
                let chunk = &full[boundaries[idx]..boundaries[next_idx]];
                for seg in s.push(chunk) {
                    match seg {
                        super::ThoughtSegment::Thinking(t) => got_think.push_str(&t),
                        super::ThoughtSegment::Content(c) => got_content.push_str(&c),
                    }
                }
                idx = next_idx;
            }
            for seg in s.flush() {
                match seg {
                    super::ThoughtSegment::Thinking(t) => got_think.push_str(&t),
                    super::ThoughtSegment::Content(c) => got_content.push_str(&c),
                }
            }
            assert_eq!(got_think, "Step 1 — verify • icons ✔ → CJK 你好");
            assert_eq!(got_content, "Result — ready • ✔!");
        }
    }
}
