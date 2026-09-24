# Task 4 Brief: MiniBlocks Tool Suite Implementation (10 Tools)

## Goal
Implement the 10-tool MiniBlocks suite (`src/tools/registry/block_tools.rs`) and integrate it into `ToolRegistry`, `ToolCategory`, concurrency classification, intent filtering, and `TOTAL_TOOL_COUNT` (updated from 156 to 166).

## Target Files
- Create: `src/tools/registry/block_tools.rs`
- Modify: `src/tools/registry/mod.rs` (expose `pub mod block_tools;`)
- Modify: `src/tools/mod.rs` (include `block_tools::get_schemas()` in `get_tool_schemas()` and dispatch in `dispatch_tool()`)
- Modify: `src/tools/category.rs` (add `ToolCategory::Blocks` to `ToolCategory::ALL`, description, schema routing, FromStr parsing)
- Modify: `src/tools/concurrency.rs` (classify `block_search`, `block_get`, `block_palettes`, `block_gradients`, `block_stats` as `ReadOnly`, and `block_insert`, `block_save`, `block_update`, `block_delete`, `block_scaffold` as `Mutating`)
- Modify: `src/context/search/intent_filter.rs` (detect `ToolCategory::Blocks` for UI/component keywords)
- Modify: `src/constants.rs` (`TOTAL_TOOL_COUNT = 166`)
- Tests: Inline in `src/tools/registry/block_tools.rs` and `cargo test -j 1 --lib tools::tests::test_total_tool_count`

## Global Constraints
1. **Targeted Tests ONLY:** Run ONLY:
   `cargo test -j 1 --lib tools::registry::block_tools::tests`
   `cargo test -j 1 --lib tools::tests::test_total_tool_count`
   `cargo test -j 1 --lib constants::tool_count_validation::total_tool_count_matches_registry`
   Never run the full test suite.
2. **Error Handling:** Zero `.unwrap()` or `.expect()` in non-test code. Return `ToolError` or `crate::error::Result<T>`.
3. **Concurrency:** Always use `-j 1` for `cargo check` and `cargo test`.
4. **Pure Rust:** No external C libraries or Python scripts.
5. **No `cd` commands.**
6. **Tool Count Invariant:** `TOTAL_TOOL_COUNT` must equal 166 and match live `ToolRegistry::get_tool_schemas().len()`.

## The 10 Tools to Implement

### 1. `block_search` (ReadOnly)
- Parameters:
  - `query` (optional string): Keyword search query across name, description, tags, and code.
  - `category` (optional string): Category filter (e.g. navbar, hero, footer, card, modal, pricing).
  - `framework` (optional string): Framework filter (react, tailwind, svelte, shadcn, css). If omitted, auto-detects from workspace using `detect_project_framework`.
  - `tags` (optional array of strings): Tag filters.
  - `limit` (optional integer, default 10, max 50).
- Returns: Markdown table/formatted list of matching components with UUID, name, category, framework, version, tags, score, description.

### 2. `block_get` (ReadOnly)
- Parameters:
  - `id` (optional string): UUID of the component.
  - `name` (optional string): Exact slug or name of the component.
- Returns: Full component details: UUID, name, category, framework, version, dependencies, tags, and complete source code block.

### 3. `block_insert` (Mutating)
- Parameters:
  - `target_file` (required string): Path to file in workspace to inject component into.
  - `component_id` (optional string): UUID or name of component to inject.
  - `code` (optional string): Raw code snippet to insert if component_id is omitted.
  - `mode` (optional string): "append", "prepend", "create", or "replace" (default: "append").
- Returns: Result summary showing target file, lines inserted, and dependencies to install.

### 4. `block_save` (Mutating)
- Parameters:
  - `name` (required string): Unique component name/slug.
  - `description` (required string): Description of UI component.
  - `category` (required string): Category name.
  - `code` (required string): Source code.
  - `framework` (optional string): Target framework (defaults to workspace detection or tailwind).
  - `dependencies` (optional array of strings): Required packages.
  - `tags` (optional array of strings): Search tags.
- Returns: Created component ID, initial version 1, and confirmation.

### 5. `block_update` (Mutating)
- Parameters:
  - `id` (required string): Component UUID.
  - `code` (optional string): New source code.
  - `description` (optional string): Updated description.
  - `tags` (optional array of strings): Updated tags.
- Returns: Updated component summary with incremented version number.

### 6. `block_delete` (Mutating)
- Parameters:
  - `id` (required string): Component UUID.
- Returns: Confirmation of deletion.

### 7. `block_palettes` (ReadOnly)
- Parameters:
  - `query` (optional string): Tag or name search filter.
  - `limit` (optional integer, default 10).
- Returns: List of 4-hex palettes with [Background, Surface, Accent, Text] hex tokens, tags, and CSS variable export suggestions.

### 8. `block_gradients` (ReadOnly)
- Parameters:
  - `query` (optional string): Tag or name search filter.
  - `limit` (optional integer, default 10).
- Returns: List of CSS gradients with name, CSS rule, colors, and tags.

### 9. `block_scaffold` (Mutating)
- Parameters:
  - `template_name` (optional string): Template name or ID (e.g. "landing", "portfolio", "dashboard").
  - `target_dir` (optional string): Target directory in workspace (default "src/components").
  - `framework` (optional string): Framework override.
- Returns: Scaffolding summary with files written and component layout assembly details.

### 10. `block_stats` (ReadOnly)
- Parameters: none.
- Returns: Formatted markdown overview of total components, palettes, gradients, templates, category distribution, and framework distribution.
