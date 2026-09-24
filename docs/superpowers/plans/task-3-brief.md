# Task 3 Brief: Seed Catalog Ingestion & CodeGraph Stack Detection

## Goal
Implement offline embedded starter catalog ingestion and automated project stack/framework detection in `src/blocks/seed.rs`. This provides minicode with 1,082 components, 105 palettes, 212 gradients, and 3 starter templates embedded directly into the binary with zero network dependency, and stack detection prioritizing components matching the active workspace (React, Tailwind, Svelte, Shadcn, etc.).

## Target Files
- Create: `src/blocks/seed.rs`
- Modify: `src/blocks/mod.rs` (expose `pub mod seed;`, re-export `seed_default_blocks`, `detect_project_framework`)
- Modify: `src/blocks/store.rs` (call `seed_default_blocks` during `get_global_block_store()` when store has 0 components)
- Tests: Inline in `src/blocks/seed.rs`

## Global Constraints
1. **Targeted Tests ONLY:** Run ONLY `cargo test -j 1 --lib blocks::seed::tests`. Never run the full test suite.
2. **Error Handling:** Zero `.unwrap()` or `.expect()` in non-test code. Return `Result<T, BlockError>`.
3. **Concurrency:** Always use `-j 1` for `cargo check` and `cargo test`.
4. **Pure Rust:** No external C libraries or Python scripts.
5. **No `cd` commands.**

## Interfaces & Functions to Produce

### 1. `seed_default_blocks` (`src/blocks/seed.rs`)
```rust
pub fn seed_default_blocks(store: &mut BlockStore) -> Result<usize, BlockError>
```
- Ingests embedded catalog (`include_str!("seed_catalog.json")`).
- Safely populates components, palettes, gradients, and templates into `store`.
- If a component/palette/gradient/template already exists by ID, it skips it to protect user modifications.
- Returns the number of new components seeded (usize).

### 2. `detect_project_framework` (`src/blocks/seed.rs`)
```rust
pub fn detect_project_framework(project_root: &Path) -> Option<BlockFramework>
```
- Inspects `project_root`:
  1. `package.json`:
     - Checks `dependencies` and `devDependencies` for keys:
       - `"shadcn"` or `"@shadcn/"` or `"@radix-ui/"` or root `components.json` exists -> `Some(BlockFramework::Shadcn)`
       - `"react"` or `"next"` -> `Some(BlockFramework::React)`
       - `"svelte"` or `"@sveltejs/"` or root `svelte.config.js` exists -> `Some(BlockFramework::Svelte)`
       - `"tailwindcss"` or root `tailwind.config.*` exists -> `Some(BlockFramework::Tailwind)`
       - `"sass"` or `"scss"` -> `Some(BlockFramework::Scss)`
  2. Root config files:
     - `components.json` -> `Some(BlockFramework::Shadcn)`
     - `svelte.config.js` / `svelte.config.ts` -> `Some(BlockFramework::Svelte)`
     - `tailwind.config.js` / `tailwind.config.ts` / `tailwind.config.cjs` / `tailwind.config.mjs` -> `Some(BlockFramework::Tailwind)`
  3. Returns `None` if no frontend framework is detected.

### 3. Automatic Seeding in `get_global_block_store()` (`src/blocks/store.rs`)
When `get_global_block_store()` initializes the global store, if `store.stats().total_components == 0`, call:
```rust
if let Err(e) = crate::blocks::seed::seed_default_blocks(&mut store) {
    tracing::warn!("Failed to seed default blocks into global store: {}", e);
}
```

### 4. Tests to Implement (`src/blocks/seed.rs`)
- `test_seed_catalog_population`: ensures `seed_default_blocks` populates >1000 components, >=100 palettes, >=200 gradients, and >=3 templates.
- `test_detect_project_framework_react`: asserts `detect_project_framework` returns `Some(BlockFramework::React)`.
- `test_detect_project_framework_tailwind`: asserts `Some(BlockFramework::Tailwind)`.
- `test_detect_project_framework_svelte`: asserts `Some(BlockFramework::Svelte)`.
- `test_detect_project_framework_shadcn`: asserts `Some(BlockFramework::Shadcn)`.
- `test_detect_project_framework_none`: asserts `None` on empty directory.
