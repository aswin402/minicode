# MiniBlocks Native Warehouse Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a native, in-process UI component, palette, gradient, and layout template warehouse (`miniblocks`) embedded directly in `minicode` with zero subprocess overhead, 10 typed tools, stack-aware search, interactive TUI browsing, and MiniKit/MiniPower synthesis.

**Architecture:** Pure Rust thread-safe `Arc<RwLock<BlockStore>>` with in-memory inverted indices for O(1) tag/category/framework filtering and fast token matching. Bundles 1,080+ components, 105 palettes, and 212 gradients into offline storage. Dispatches 10 `block_*` tools through `ToolRegistry` and presents an interactive Ratatui warehouse modal (`/blocks` or `F6`).

**Tech Stack:** Rust 2021 Edition, Tokio async runtime, Ratatui 0.29, Serde, Syntect syntax highlighting, Simsearch/token indexer, thiserror.

## Global Constraints

- **Pure Rust Portability:** Zero external Python scripts, zero child-process MCP servers, zero C OpenSSL dependencies.
- **Zero `.unwrap()` / `.expect()`:** Never use `.unwrap()` or `.expect()` in non-test production code. Return `crate::error::Result<T>` or `ToolError`.
- **Concurrency & Resource Limits:** Always use `-j 1` for `cargo check`, `cargo clippy`, and `cargo test`. Max `-j 2` for release builds. Never propose a `cd` command.
- **Tool Count Invariant:** Adding 10 `block_*` tools strictly increments `TOTAL_TOOL_COUNT` from 156 to 166, validated by `tools::tests::test_total_tool_count`.
- **TUI alternate screen safety:** All logging uses `tracing` macros (`tracing::info!`, `tracing::debug!`); never call `println!` or `eprintln!` in library code.

---

## File Structure

```
src/
├── blocks/
│   ├── mod.rs          # Re-exports, BlockError, get_global_block_store()
│   ├── models.rs       # BlockCategory, BlockFramework, BlockComponent, BlockPalette, BlockGradient, BlockTemplate
│   ├── seed.rs         # Offline bundled components, palettes, gradients, and starter templates
│   └── store.rs        # In-memory BlockStore with inverted index, CRUD, version history, and disk persistence
├── tools/
│   ├── registry/
│   │   ├── block_tools.rs # 10 block tools implementation and schemas
│   │   └── mod.rs      # Tool registry dispatch integration
│   └── mod.rs          # Category mapping and schema aggregation
├── ui/
│   └── modals/
│       ├── blocks.rs   # Interactive Ratatui UI modal for browsing/inspecting/inserting blocks
│       └── mod.rs      # ModalState::Blocks variant
├── constants.rs        # TOTAL_TOOL_COUNT = 166
└── app/
    ├── commands.rs     # Slash command /blocks dispatch
    └── modals.rs       # Keybinding F6 and modal navigation
tests/
└── integration_miniblocks_warehouse.rs # End-to-end integration tests
```

---

### Task 1: MiniBlocks Domain Models & Error Definitions

**Files:**
- Create: `src/blocks/models.rs`
- Create: `src/blocks/mod.rs`
- Modify: `src/lib.rs`
- Test: `src/blocks/models.rs` (inline unit tests)

**Interfaces:**
- Produces: `BlockCategory`, `BlockFramework`, `BlockComponent`, `BlockPalette`, `BlockGradient`, `BlockTemplate`, `BlockStats`, `BlockError`.

- [x] **Step 1: Write the failing unit tests for models**
- [x] **Step 2: Run test to verify it fails**
- [x] **Step 3: Implement `src/blocks/models.rs` and `src/blocks/mod.rs`**
- [x] **Step 4: Run test to verify it passes**
- [x] **Step 5: Format and commit**

---

### Task 2: Fast In-Memory Store & Inverted Index Engine

**Files:**
- Create: `src/blocks/store.rs`
- Modify: `src/blocks/mod.rs`
- Test: `src/blocks/store.rs` (inline unit tests)

**Interfaces:**
- Consumes: Models from `src/blocks/models.rs`.
- Produces: `BlockStore`, `BlockSearchFilter`, `BlockSearchResult`, `get_global_block_store()`.

- [x] **Step 1: Write the failing unit tests for `BlockStore`**
- [x] **Step 2: Run test to verify it fails**
- [x] **Step 3: Implement `src/blocks/store.rs`**
- [x] **Step 4: Run test to verify it passes**
- [x] **Step 5: Format and commit**

---

### Task 3: Seed Catalog Ingestion & CodeGraph Stack Detection

**Files:**
- Create: `src/blocks/seed.rs`
- Modify: `src/blocks/store.rs`
- Modify: `src/blocks/mod.rs`
- Test: `src/blocks/seed.rs` (inline unit tests)

**Interfaces:**
- Consumes: `openblocks` data catalog or embedded JSON snapshots.
- Produces: `seed_default_blocks(&mut BlockStore)`, `detect_project_framework(&Path) -> Option<BlockFramework>`.

- [x] **Step 1: Write the failing unit tests for seed data & stack detection**
- [x] **Step 2: Run test to verify it fails**
- [x] **Step 3: Implement `src/blocks/seed.rs`**
- [x] **Step 4: Run test to verify it passes**
- [x] **Step 5: Format and commit**

---

### Task 4: MiniBlocks Tool Suite Implementation (10 Tools)

**Files:**
- Create: `src/tools/registry/block_tools.rs`
- Modify: `src/tools/registry/mod.rs`
- Modify: `src/tools/category.rs`
- Modify: `src/constants.rs`
- Test: `src/tools/registry/block_tools.rs` (inline unit tests)

**Interfaces:**
- Produces: 10 tools (`block_search`, `block_get`, `block_insert`, `block_save`, `block_update`, `block_delete`, `block_palettes`, `block_gradients`, `block_scaffold`, `block_stats`), `TOTAL_TOOL_COUNT = 166`.

- [x] **Step 1: Write failing unit tests for tool schemas & count**
- [x] **Step 2: Run test to verify it fails**
- [x] **Step 3: Implement `src/tools/registry/block_tools.rs` and update registry**
- [x] **Step 4: Run test to verify tool count and schemas pass**
- [x] **Step 5: Format and commit**

---

### Task 5: Interactive TUI Warehouse Modal (`/blocks` & `F6`)

**Files:**
- Create: `src/ui/modals/blocks.rs`
- Modify: `src/ui/modals/mod.rs`
- Modify: `src/app/commands.rs`
- Modify: `src/app/modals.rs`
- Test: `src/ui/modals/blocks.rs` (inline unit tests)

**Interfaces:**
- Produces: `BlocksModalState`, `render_blocks_modal`, `/blocks` command dispatch, `F6` shortcut handler.

- [x] **Step 1: Write failing unit tests for modal state navigation**
- [x] **Step 2: Run test to verify it fails**
- [x] **Step 3: Implement `src/ui/modals/blocks.rs` and wire into App**
- [x] **Step 4: Run test to verify it passes**
- [x] **Step 5: Format and commit**

---

### Task 6: MiniKit & MiniPower Synthesis and End-to-End Testing

**Files:**
- Modify: `src/tools/minikit/sync.rs`
- Modify: `src/agent/minipower/mod.rs`
- Modify: `src/agent/prompt.rs`
- Create: `tests/integration_miniblocks_warehouse.rs`
- Test: `tests/integration_miniblocks_warehouse.rs`

**Interfaces:**
- Produces: `minikit_docs/skills/miniblocks.md` generation, prompt blueprint UI block awareness, end-to-end integration test suite.

- [x] **Step 1: Write E2E integration test suite**
- [x] **Step 2: Run test to verify it fails**
- [x] **Step 3: Wire MiniKit sync & MiniPower prompts and pass tests**
- [x] **Step 4: Run full targeted test suite**
- [x] **Step 5: Format and commit**

---

## Self-Review Checklist

- [x] **Spec coverage:** Domain models, fast in-memory store, 1,000+ seed catalog, 10 typed tools, TUI modal, stack detection, and MiniKit/MiniPower synthesis all covered across Tasks 1–6.
- [x] **Placeholder scan:** No "TBD", "TODO", or pseudo-code steps. Every task specifies exact file paths, interfaces, commands, and expected outputs.
- [x] **Type consistency:** `BlockCategory`, `BlockFramework`, `BlockComponent`, `BlockPalette`, `BlockStore` consistently named and typed throughout all tasks.
- [x] **Invariants respected:** Pure Rust, zero `.unwrap()` in non-test code, `-j 1` concurrency, total tool count synchronized to 166.
