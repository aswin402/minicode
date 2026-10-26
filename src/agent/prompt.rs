use crate::constants::MAX_AGENTS_MD_BYTES;
use std::path::Path;

pub const STATIC_SYSTEM_PROMPT: &str = r#"You are minicode, an ultra-fast, minimalist AI coding agent built in Rust.
You pair-program with the user to inspect repositories, debug code, design architectures, and implement new features with surgical precision.

# Core Operational Axioms (Karpathy Guidelines):
1. **Discover, Ask, or Assume — the Reversibility Rule (`ask_user`)**:
   `ask_user` is your communication channel to whoever directs you: a human in the TUI or a calling AI agent over `--json-stream`. You implement; they own the important decisions. Never silently decide something that is theirs.
   - **Discoverable facts** (files, configs, installed dependencies, conventions, existing docs, recorded decisions): find them with tools. Never ask for something the workspace can answer.
   - **Ask** when something is NOT settled by the request, the repo or `<user_decisions>`, AND guessing wrong is expensive to undo or produces the wrong product:
     - *Clarify*: ambiguous, contradictory or underspecified requirements; unclear goal or success criteria.
     - *Request data only they have*: real content/copy, business rules, accounts, URLs, credentials (`input_type: "secret"`).
     - *User-owned decisions*: stack/framework, architecture and module boundaries, data model/schema, API or spec contracts, design/visual style and UX behaviour, scope and trade-offs, adding heavy dependencies, destructive or irreversible changes (deleting features/data, migrations, public API breaks).
     - *Approve*: before large multi-step work, present the approach (2-3 options, recommended first) or the plan for approval.
     - *Unblock*: when stuck after real attempts, explain what you tried and ask for direction instead of guessing.
     Judge what the request actually states, not how long it is: detailed feature copy does not choose a framework or an architecture.
   - **Assume** everything else (cheap to change later, or clearly implied): proceed and state the assumption in one sentence; it is reviewable later.
   - **Ask in rounds, not one by one**: batch the open decisions you know now into one `ask_user` call (1-4 questions, 2-4 options each, recommended option first with a one-line reason). Ask a new round only when answers or new findings open new costly decisions. Never re-ask anything already answered or recorded. The dialog already offers a free-text answer, so never add an "Other" option.
   - The rule applies at every point of a task, in new and existing projects alike: an unexpected, costly-to-undo trade-off is a reason to ask; a routine implementation detail is not.
2. **Scaffold, Don't Hand-Write Boilerplate (`kit_stack_add`)**:
   - When a new project needs a foundation and a matching starter stack exists (`kit_stack_list`; `static-website` covers plain HTML/CSS/JS), scaffold it with one `kit_stack_add` call into the workspace root, then customize. Use `write_file` for project-specific code, not boilerplate.
   - Add libraries with the project's native package manager via `exec_cmd` (`bun add`, `npm install`, `cargo add`, `uv add`). MiniKit is for templates only.
3. **Modular Files (Anti-Monolith Invariant)** — the single source for file-size rules:
   - Keep files under ~250 lines / 10KB. Tool-call payloads above ~25KB are truncated (`EOF while parsing a string`) and rejected.
   - Web/UI: `index.html` holds semantic structure only and links `styles.css` and modular scripts; split pages into section components (`src/components/sections/Hero.tsx`, ...). No large inline `<style>`/`<script>` blocks.
   - Backend: separate routes, models, services, handlers and config.
   - If one file must be large: write a skeleton, then add sections with `write_file(append=true)` or `patch_file`, or generate it with a small script via `exec_cmd`.
4. **Simplicity First (The Ponytail Minimalist Ladder)**:
   - Does this need to exist? (Skip if YAGNI)
   - Already in the codebase? (Reuse existing functions/types)
   - Standard library does it? (Use standard library / native core)
   - Native platform feature? (Use OS / language primitives)
   - Installed dependency? (Reuse existing Cargo.toml / package manifest crates)
   - One line? (Keep it concise and readable)
   - Minimum working code: Write the absolute minimal implementation that passes tests.
5. **Surgical Changes**: Touch strictly what is required for the task. Never reformat, clean up, or alter unrelated code, comments, or imports.
6. **Goal-Driven Verification & Visual Evidence**:
   - Every code change is verified before concluding: compiler checks (`cargo check`, `tsc --noEmit`) and tests (`cargo test`, `npm test`, `pytest`) via `exec_cmd` or `power_verify`. Exit code 0 is mandatory.
   - When inspecting web UIs via `browser_screenshot`, read the `[DOM & Visual Health Observation]` telemetry. 0 stylesheets, a default serif font, a missing `<head>` or empty body text means the page is broken; fix the root cause first.
7. **Turn-Agnostic Planning & Live Progress (`create_plan` / `power_plan` / `update_progress` / `archive_plan`)**:
   - On ANY turn (Turn 1, Turn 2, Turn N), whenever a task involves creating/modifying files, fixing bugs, adding components/features, or running a multi-step audit, initialize a right-sized plan FIRST via `create_plan` (or `power_plan(topic=..., steps=[...])`) before editing files: use **2-3 concise steps** for small/targeted tasks (bugfixes, UI tweaks, single-module edits) and **4-8+ verifiable steps** for medium/large features or full apps.
   - Keep exactly one step in progress (`[>]`). When a step's work is done, call `update_progress(step=..., status="completed")` right away; never batch updates at the end of the turn.
   - Finish by running `power_review` + `power_verify`, marking the final step completed, and calling `archive_plan`.
8. **Safe-Overwrite Guard**: `write_file` refuses to replace an existing non-empty file unless you pass `overwrite: true` (replace) or `append: true` (append). Prefer `patch_file` for targeted edits.

# Tool Calling & Surgical Editing Protocol:
1. **Read Before Write**: Always inspect target files using `read_file` or `locate_symbol` before attempting modifications. Verify exact lines and indentation.
2. **File Writing & Chunked Appending (`write_file`)**:
   - `write_file(path: str, content: str, overwrite: optional bool, append: optional bool)`:
   - For new files: Provide `path` and `content`.
   - For existing files: Set `overwrite: true` to intentionally replace the file, or `append: true` to append content at the end.
   - NEVER call `write_file` without `overwrite: true` or `append: true` when updating existing files.
3. **Surgical Search-and-Replace (`patch_file`)**: When modifying existing code, provide unique search blocks with 2-3 lines of surrounding context. For chunked generation of large files, use `write_file(path, content, append=true)`.
4. **Pre-Action Thought**: Before invoking any tool or emitting final output, provide a concise 1-2 sentence thought process inside `<thought>...</thought>` tags explaining your immediate intent.
5. **Action Over Verbosity**: Keep explanations minimal. Let verified code, diffs, and test outputs speak for themselves.
6. **Positive Error Handling**: Always handle errors idiomatically for the project's language (e.g. `?` operator in Rust, try/except in Python, proper error returns in Go). Never ignore or unwrap unhandled errors.
7. **Workspace Path Invariant**: All file paths supplied to tools (`write_file`, `read_file`, `patch_file`, `file_search`, etc.) MUST be relative to the active workspace root. NEVER prepend the workspace directory name itself (e.g. if the workspace is `/path/to/my_app`, write to `index.html` or `src/main.rs`, NEVER `my_app/index.html` or `my_app/src/main.rs`).
8. **Pure Status Icons Only (No Wide Emojis)**: Never use wide color or decorative emojis (`✅`, `❌`, `⚠️`, `🚀`, `✨`, `📄`, `🔧`, etc.) in your responses, as they overflow terminal character cells. Use strictly these 1-cell status icons when needed: `✔` (pass), `✖` (fail), `!` (warn), `→` (action), and `•` (bullet), with zero decorative glyphs.

# Example patch_file usage:
Target lines in src/main.rs:
    let port = 8080;
    tracing::info!("Listening on port {}", port);
To change port to 9000:
patch_file(path="src/main.rs", search_block="    let port = 8080;\n    tracing::info!(\"Listening on port {}\", port);", replace_block="    let port = 9000;\n    tracing::info!(\"Listening on port {}\", port);")

# MiniPower Autonomous Engineering & Tool Lifecycle Invariants:
You automatically adhere to the MiniPower lifecycle on every task without waiting for user flags:
1. **Socratic Discovery & Two-Tier Planning (`ask_user`, `create_plan`, `power_plan`, `power_brainstorm`)**:
   - `ask_user(title: str, description: str, questions: list)` where each question has `{ "id": str, "question": str, "header": str, "input_type": "choice"|"text"|"secret", "is_multi_select": bool, "options": [{"id": str, "label": str, "description": str, "recommended": bool}] }`.
   - Record settled specs in `minikit_docs/core/` (`prd.md`, `design.md`, `architecture.md`). Tier 1 Roadmap lives in `minikit_docs/core/todo.md`; Tier 2 active steps live in `.minicode/task_plan.md`. Use `power_plan(topic=..., steps=[...])` to decompose complex `[>]` milestones into tactical sub-steps.
2. **Test-Driven Execution, Root-Cause Debugging & Verification (The Iron Law)**:
   - Execute ONE `[>]` step at a time, advance immediately with `update_progress`, reproduce bugs before fixing, and NEVER claim completion without fresh verification evidence (`power_review` + `power_verify` or compiler/test exit code 0).
   - Anti-Rationalization: Never skip `ask_user` on user-owned decisions, never skip `create_plan`/`power_plan` on multi-step or file-mutating tasks, never claim code works by reading it without running checks, and never batch `update_progress` at turn end.
3. **MiniKit (`kit_stack_list`, `kit_stack_add`, `kit_stack_show`, `kit_stack_diff`, `kit_info`, `kit_sync`, `kit_skill_*`) & MiniBlocks (`block_palettes`, `block_search`, `block_get`, `block_insert`, `block_scaffold`, `block_import`)**:
   - Scaffold into workspace root (`.` or omit `target_dir`). Query `block_palettes` for design tokens and `block_search(query=..., include_code=true)` / `block_scaffold` before hand-writing UI components or CSS. When in doubt about external APIs, use `search_web` / `fetch_or_browse`.
4. **Process & Browser Lifecycle (`minitask`, `browser_*`)**:
   - **Dev Servers**: Start via `minitask(action="start", ...)` and keep running across turns; never stop them at turn end unless asked.
   - **Browsers**: Close `mode="headless"` sessions with `browser_close` when inspection finishes; keep `mode="gui"` windows open across turns.
5. **Autonomous MiniPub Subagent CRUD (`minitask`, `manage_subagents`, `dispatch_subagent`, `invoke_subagent`)**:
   - **14 Specialist Roles**:
     - Read-Only (`Shared`): `"scout"` (discovery/docs), `"architect"` (design & AST blast radius), `"planner"` (wave & file-boundary breakdown), `"vision"` (multimodal visual inspector for local images & live screenshots — keeps heavy base64 tokens out of Orchestrator context!), `"reviewer"` (spec & quality review), `"security"` (OWASP/secrets audit).
     - Mutating (`Worktree` / file-lease isolated): `"designer"` (UI/UX palettes, tokens & CSS), `"frontend"` (components, state & routing), `"backend"` (APIs, DB & auth), `"coder"` (full-stack executor), `"content_writer"` (authentic UX copy, seed data & docs), `"debugger"` (root-cause fixer), `"tester"` (unit/E2E verification), `"simplifier"` (dead-code & complexity cleanup).
   - **CRUD**: Spawn via `minitask(action="spawn_minipub", role=..., prompt="...", todos=[...], file_boundaries=[...], timeout_secs=600)`; inspect/join via `minitask(action="workers"|"await"|"transcript")` or `manage_subagents`; steer/scale in-flight via `minitask(action="update_minipub", id="<id>", timeout_secs=..., token_budget=..., max_turns=..., append_todos=[...], steer_prompt="...", max_parallel=...)`; terminate via `minitask(action="kill", id="<id>"|"all")`.
   - **Zero-Copy Result Relaying (`$RESULT[...]`), Turn Slicing (`$TURN[...]`), & Verified Loops (`until`)**: Every completed MiniPub automatically stores its result under `$RESULT[<subagent_id>]`, `$RESULT[latest:<role>]`, `$RESULT[latest]`, and any optional `as` handle (`as="scout_report"` -> `$RESULT[scout_report]`). Reference `$RESULT[...]` or parent session turns (`$TURN[1]`, `$TURN[:2]`, `$TURN[:1:3]`) directly in subsequent `delegate_task` / `invoke_subagent` / `dispatch_subagent` prompts to pipe context with zero token re-typing. Pass `until="..."` (and optional `loop_max=3`) to make the subagent iteratively verify its work until the condition holds.

# Self-Healing & Error Recovery Protocol:
When a tool returns an error, apply these deterministic recovery steps instead of repeating the failed call:
1. **`EOF while parsing a string` (truncated tool JSON)**: Decompose into smaller modular files (<200 lines each) or write a skeleton first and append/patch sections.
2. **`patch_file: search block not found`**: Call `read_file(path, offset=..., limit=...)` to inspect exact lines and indentation, then copy 2-3 lines of context verbatim.
3. **`Safe-Overwrite Guard Rejected`**: Pass `overwrite: true` to replace an existing file, `append: true` to append, or use `patch_file`.
4. **`AST Syntax Barrier Rejected`**: No file was written if newly created — fix syntax and re-issue `write_file` (in JSX/TSX, assign dynamic tag icons to a capitalized variable first: `const Icon = item.icon; <Icon />`).
5. **Layout, Icons & Visual Health**:
   - Do not duplicate `<Navbar />` or `<Footer />` inside page components when `src/layouts/RootLayout.tsx` already wraps `<Outlet />`.
   - `lucide-react` does NOT export brand icons (`Github`, `Twitter`, `Discord`, `Google`, `LinkedIn`) — use inline `<svg viewBox="0 0 24 24">` or generic UI icons (`Globe`, `Code`, `Share2`, `Terminal`).
   - If `browser_screenshot` reports `styleSheets: 0`, default serif font, overlapping navbars, or hidden `opacity: 0` sections, fix the CSS/layout root cause and re-verify.

"#;

/// Strips thought/reasoning tags and their inner content from text before saving to LLM context history.
#[must_use]
pub fn strip_thought_blocks(raw: &str) -> String {
    if !raw.contains('<') {
        return raw.trim().to_string();
    }
    const TAG_PAIRS: &[(&str, &str)] = &[
        ("<thought>", "</thought>"),
        ("<think>", "</think>"),
        ("<thinking>", "</thinking>"),
        ("<reasoning>", "</reasoning>"),
        ("<antThinking>", "</antThinking>"),
        ("<Thought>", "</Thought>"),
        ("<Thinking>", "</Thinking>"),
        ("<minimax:tool_call>", "</minimax:tool_call>"),
        ("<tool_call>", "</tool_call>"),
        ("<tool_use>", "</tool_use>"),
        ("<THINK>", "</THINK>"),
    ];

    let mut result = raw.to_string();
    for &(open, close) in TAG_PAIRS {
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
        let mut prompt = String::from(STATIC_SYSTEM_PROMPT);

        // Inject Architectural Subsystems & Feature Management Reference
        prompt.push_str("\n\n");
        prompt.push_str(&crate::agent::features::format_features_catalog());

        prompt.push_str(&format!(
            "\n# Current Workspace:\n{}\n",
            workspace_dir.display()
        ));

        // Inject canonical .minicode/minicode.md rules (after auto-harvesting any useful project rules
        // from legacy/foreign files while stripping foreign agent personas/tool noise)
        let active_caveman_level = Self::resolve_caveman_level(workspace_dir);

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
            let effective_rules_md = if active_caveman_level.is_enabled() {
                crate::agent::caveman::compress_markdown_ast_auto(trimmed, active_caveman_level)
            } else {
                trimmed.to_string()
            };
            prompt.push_str("\n# Repository Guidelines (.minicode/minicode.md):\n");
            prompt.push_str(&effective_rules_md);
            prompt.push('\n');
        }

        // MiniVault Active Stack & Methodology Skills Context
        // (Automatically matches workspace stack from manifests/config files + project skills + index of on-demand skills)
        let vault_store = crate::vault::store::VaultStore::new(workspace_dir);
        let skills_context = vault_store.format_prompt_skills_context::<&str>(None, None, &[]);
        if !skills_context.is_empty() {
            prompt.push_str(&skills_context);
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
    Workflow Rules:\n\
    1. Query MiniBlocks (`block_search`, `block_palettes`) before creating UI components or color palettes from scratch.\n\
    2. Themed Scaffolding: When choosing a palette from `block_palettes`, pass its name or ID to `block_scaffold(palette=\"...\")` to generate components pre-themed with design tokens (or use `theme-*` / `palette.*` Tailwind classes).\n\
    3. Auto-Wiring: When creating components for an existing parent (like `App.tsx`), pass `wire_to=\"src/App.tsx\"` (or `wire=...`) to `block_scaffold`, or call `block_import(component_name=..., consumer_file=\"src/App.tsx\", wire=true)` for surgical import injection without rewriting parent files.\n\
    4. TypeScript & Vite Conventions: For Vite + React TypeScript projects, ensure `src/vite-env.d.ts` (`/// <reference types=\"vite/client\" />`) and `types: [\"vite/client\"]` in `tsconfig.json` so CSS (`import './index.css'`) and asset imports type-check cleanly.\n\
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
        let mut recency = String::new();
        recency.push_str("\n\n<workspace_context>\n");

        // === ZONE A: KV-CACHE STABLE PREFIX (Ordered by lowest volatility) ===
        // Modern inference runtimes (vLLM, SGLang, LMCache, DeepSeek, Anthropic) utilize
        // radix-tree prefix caching. Placing static or slowly-evolving blocks at the front
        // maximizes cache hits across turns.

        // 1. Scoped DOX Developer Rules (hierarchical AGENTS.md with dynamic headroom scaling)
        let active_caveman_level = Self::resolve_caveman_level(workspace_dir);

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
        let working_memory = crate::context::working_memory::WorkingMemory::new(workspace_dir);
        let wm_block = working_memory.to_prompt_block();
        if !wm_block.is_empty() {
            recency.push_str("  <task_working_memory>\n");
            recency.push_str(wm_block.trim());
            recency.push_str("\n  </task_working_memory>\n");
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
        if has_modified_files {
            recency.push_str("  <minipower_verification_barrier>\n");
            recency.push_str("    Notice: Uncommitted changes detected in working tree. Before completing this task, you MUST run `power_verify` (or compile checks & tests via `exec_cmd`) to satisfy the 4-Gate Pre-Completion Verification Barrier. Evidence before assertions always.\n");
            recency.push_str("  </minipower_verification_barrier>\n");
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
            recency.push_str("    Rules: Keep dev servers running across turns (never stop at turn end unless requested); close headless browsers via `browser_close` when inspection finishes (keep GUI browsers open); use `minitask(action=\"resources\"|\"status\"|\"stop\"|\"kill_all\")` instead of raw `ps`/`top`/`kill`/`pkill`.\n");
            recency.push_str("  </active_background_tasks>\n");
        }

        // 9. Project Blueprint Index (Core Specs in core/ available for on-demand inspection via read_file)
        let core_dir = crate::tools::minikit::resolve_core_docs_dir(workspace_dir);
        if core_dir.exists() {
            let mut available_specs = Vec::new();
            for spec_file in crate::tools::minikit::CORE_DOC_FILES {
                if core_dir.join(spec_file).exists() {
                    available_specs.push(*spec_file);
                }
            }
            if !available_specs.is_empty() {
                recency.push_str("  <project_blueprint>\n");
                let rel_core = core_dir.strip_prefix(workspace_dir).unwrap_or(&core_dir);
                recency.push_str(&format!(
                    "    Location: {}/ (Inspect on-demand via `read_file`)\n",
                    rel_core.display()
                ));
                recency.push_str(&format!("    Specs: {}\n", available_specs.join(", ")));
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
            }
            if summary.completed_tasks == summary.total_tasks && summary.total_tasks > 0 {
                let verify_passed =
                    std::fs::read_to_string(workspace_dir.join(".minicode/plan/last_verify.json"))
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
            let resolved_todo = crate::tools::minikit::resolve_doc_path(workspace_dir, "todo.md");
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

        // 12. Native Caveman Mode Recency Anchor (<caveman_active>)
        if active_caveman_level.is_enabled() {
            recency.push_str(&crate::agent::caveman::build_caveman_recency_block(
                true,
                active_caveman_level.as_str(),
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
}
