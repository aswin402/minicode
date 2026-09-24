# Task 6 Brief: MiniKit & MiniPower Synthesis and End-to-End Testing

## Goal
Wire MiniBlocks warehouse awareness into MiniKit docs generation (`minikit_docs/skills/miniblocks.md`), MiniPower planning instructions (`format_plan_prompt`), and agent prompt blueprint, then implement a comprehensive end-to-end integration test suite in `tests/integration_miniblocks_warehouse.rs` covering all 10 tools, path sandboxing, and warehouse synthesis.

## Target Files
- Modify: `src/tools/minikit/sync.rs` (generate `skills/miniblocks.md` in `ensure_workflow_docs`)
- Modify: `src/agent/minipower/mod.rs` (add MiniBlocks UI reuse guidance in `format_plan_prompt`)
- Modify: `src/agent/prompt.rs` (add `<miniblocks_warehouse>` blueprint in `build_system_prompt`)
- Create: `tests/integration_miniblocks_warehouse.rs`
- Tests: `cargo test -j 1 --test integration_miniblocks_warehouse`

## Global Constraints
1. **Targeted Tests ONLY:** Run ONLY `cargo test -j 1 --test integration_miniblocks_warehouse`. Never run the full test suite.
2. **Error Handling:** Zero `.unwrap()` or `.expect()` in non-test code.
3. **Concurrency:** Always use `-j 1` for `cargo check` and `cargo test`.
4. **Pure Rust:** No external C libraries or Python scripts.
5. **No `cd` commands.**

## Deliverables

### 1. MiniKit `skills/miniblocks.md` Generation (`src/tools/minikit/sync.rs`)
In `MiniKitSyncEngine::ensure_workflow_docs(docs_dir: &Path, project_name: &str, runtime: &str)`:
Ensure `skills_dir.join("miniblocks.md")` is generated with:
- Overview of MiniBlocks native warehouse (1,082+ components, 105 palettes, 212 gradients, 3 templates).
- Available categories (navbar, hero, footer, card, modal, pricing, form, table, etc.).
- Summary of the 10 `block_*` tools.
- AI Agent behavioral instructions: query `block_search` or `block_palettes` before creating UI components or color palettes from scratch; inject with `block_insert`.
- Keyboard shortcut (`F6`) and slash command (`/blocks`).

### 2. MiniPower Planning Guidance (`src/agent/minipower/mod.rs`)
In `format_plan_prompt`:
Add instruction:
"4. If this implementation involves frontend components, UI, styles, or page layouts:
   - Search the MiniBlocks warehouse first (`block_search`, `block_palettes`, `block_scaffold`) to reuse verified components and design tokens instead of hallucinating CSS from scratch."

### 3. Agent System Prompt Blueprint (`src/agent/prompt.rs`)
In `build_system_prompt`:
Append `<miniblocks_warehouse>` section:
```xml
  <miniblocks_warehouse>
    Native UI warehouse: 1,080+ components, 105 palettes, 212 gradients, 3 templates.
    Tools: `block_search`, `block_get`, `block_insert`, `block_palettes`, `block_gradients`, `block_scaffold`.
    Rule: Query MiniBlocks before creating UI components or color palettes from scratch.
  </miniblocks_warehouse>
```

### 4. Comprehensive E2E Integration Test Suite (`tests/integration_miniblocks_warehouse.rs`)
Implement async tests using `ToolRegistry::dispatch`:
- `test_e2e_miniblocks_catalog_and_stats`: verifies `block_stats` reports 1,080+ components, 105 palettes, 212 gradients, 3 templates.
- `test_e2e_miniblocks_search_and_get`: searches for "navbar", gets full source code by ID.
- `test_e2e_miniblocks_palettes_and_gradients`: queries palettes and gradients, validates CSS output.
- `test_e2e_miniblocks_insert_and_path_sandbox`: inserts component into workspace file (`create` and `append` modes), and asserts path escaping (`../outside.html`) is rejected with an error.
- `test_e2e_miniblocks_save_update_delete_lifecycle`: saves a custom component, searches for it, updates code (version 2), and deletes it.
- `test_e2e_miniblocks_scaffold`: scaffolds landing template into workspace directory, asserts files created.
- `test_e2e_minikit_sync_generates_miniblocks_skill`: runs `MiniKitSyncEngine::ensure_workflow_docs` and asserts `skills/miniblocks.md` exists with valid content.
