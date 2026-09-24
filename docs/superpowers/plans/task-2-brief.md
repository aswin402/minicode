# Task 2 Brief: Fast In-Memory BlockStore & Inverted Index Engine

## Goal
Implement the thread-safe in-memory `BlockStore` engine with inverted indices, multi-token fuzzy search, full CRUD operations across all 4 asset domains (components, palettes, gradients, templates), version history, JSON disk persistence, and global singleton store initializer.

## Target Files
- Create: `src/blocks/store.rs`
- Modify: `src/blocks/mod.rs` (expose `pub mod store;`, re-export `BlockStore`, `BlockSearchFilter`, `BlockSearchResult`, `get_global_block_store`)
- Tests: Inline in `src/blocks/store.rs`

## Global Constraints
1. **Targeted Tests ONLY:** Run ONLY `cargo test -j 1 --lib blocks::store::tests`. Never run the full test suite.
2. **Error Handling:** Zero `.unwrap()` or `.expect()` in non-test code. Return `Result<T, BlockError>`.
3. **Concurrency:** Always use `-j 1` for `cargo check` and `cargo test`.
4. **Pure Rust:** No external C libraries or Python scripts.
5. **No `cd` commands.**

## Interfaces & Types to Produce

### 1. `BlockSearchFilter` (`src/blocks/store.rs`)
```rust
#[derive(Debug, Clone, Default)]
pub struct BlockSearchFilter {
    pub query: Option<String>,
    pub category: Option<BlockCategory>,
    pub framework: Option<BlockFramework>,
    pub tags: Option<Vec<String>>,
    pub limit: usize,
}
```

### 2. `BlockSearchResult` (`src/blocks/store.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSearchResult {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: String,
    pub category: BlockCategory,
    pub framework: BlockFramework,
    pub tags: Vec<String>,
    pub version: u32,
    pub score: f64,
}
```

### 3. `BlockStore` (`src/blocks/store.rs`)
```rust
pub struct BlockStore {
    components: HashMap<Uuid, BlockComponent>,
    component_versions: HashMap<Uuid, Vec<(u32, String, DateTime<Utc>)>>,
    palettes: HashMap<Uuid, BlockPalette>,
    gradients: HashMap<Uuid, BlockGradient>,
    templates: HashMap<Uuid, BlockTemplate>,
    // Inverted indices for O(1) filtering
    category_index: HashMap<BlockCategory, HashSet<Uuid>>,
    framework_index: HashMap<BlockFramework, HashSet<Uuid>>,
    tag_index: HashMap<String, HashSet<Uuid>>,
    // Persistence path
    persistence_path: Option<PathBuf>,
}
```

Implement the following methods on `BlockStore`:
- `pub fn new() -> Self` and `pub fn new_isolated(persistence_path: PathBuf) -> Self`
- **Components:**
  - `pub fn insert_component(&mut self, component: BlockComponent) -> Result<(), BlockError>`
  - `pub fn get_component(&self, id: &Uuid) -> Option<&BlockComponent>`
  - `pub fn get_component_by_name(&self, name: &str) -> Option<&BlockComponent>`
  - `pub fn search_components(&self, filter: &BlockSearchFilter) -> Vec<BlockSearchResult>`
  - `pub fn update_component(&mut self, id: &Uuid, new_code: Option<&str>, new_description: Option<&str>, new_tags: Option<Vec<String>>) -> Result<BlockComponent, BlockError>` (records old code into `component_versions`, increments `version += 1`, updates `updated_at = Utc::now()`)
  - `pub fn delete_component(&mut self, id: &Uuid) -> Result<(), BlockError>`
  - `pub fn get_component_history(&self, id: &Uuid) -> Vec<(u32, String, DateTime<Utc>)>`
- **Palettes:**
  - `pub fn insert_palette(&mut self, palette: BlockPalette) -> Result<(), BlockError>`
  - `pub fn get_palette(&self, id: &Uuid) -> Option<&BlockPalette>`
  - `pub fn list_palettes(&self) -> Vec<&BlockPalette>`
  - `pub fn search_palettes(&self, query: &str) -> Vec<&BlockPalette>`
  - `pub fn delete_palette(&mut self, id: &Uuid) -> Result<(), BlockError>`
- **Gradients:**
  - `pub fn insert_gradient(&mut self, gradient: BlockGradient) -> Result<(), BlockError>`
  - `pub fn get_gradient(&self, id: &Uuid) -> Option<&BlockGradient>`
  - `pub fn list_gradients(&self) -> Vec<&BlockGradient>`
  - `pub fn search_gradients(&self, query: &str) -> Vec<&BlockGradient>`
  - `pub fn delete_gradient(&mut self, id: &Uuid) -> Result<(), BlockError>`
- **Templates:**
  - `pub fn insert_template(&mut self, template: BlockTemplate) -> Result<(), BlockError>`
  - `pub fn get_template(&self, id: &Uuid) -> Option<&BlockTemplate>`
  - `pub fn list_templates(&self) -> Vec<&BlockTemplate>`
  - `pub fn delete_template(&mut self, id: &Uuid) -> Result<(), BlockError>`
- **Stats & Persistence:**
  - `pub fn stats(&self) -> BlockStats`
  - `pub fn save_to_disk(&self, path: &Path) -> Result<(), BlockError>`
  - `pub fn load_from_disk(path: &Path) -> Result<Self, BlockError>`
  - `pub fn persist_if_configured(&self) -> Result<(), BlockError>`

### 4. Global Singleton Initializer
```rust
pub fn get_global_block_store() -> &'static std::sync::RwLock<BlockStore>
```
Using `std::sync::OnceLock`. Defaults to `~/.local/share/minicode/miniblocks/store.json`.

## Unit Tests to Implement in `src/blocks/store.rs`
1. `test_store_crud_and_indexing`:
   - Insert component, retrieve by id and name.
   - Search with query, category, and framework filters.
   - Update component, verify version increases to 2 and history is preserved.
   - Delete component, verify removal from indexes and store.
2. `test_palette_and_gradient_search`:
   - Insert palettes and gradients, search by name/tags, list, delete.
3. `test_disk_persistence_roundtrip`:
   - Save store to tempfile, reload, verify all assets intact.

## Deliverables
1. Run `cargo test -j 1 --lib blocks::store::tests` -> must pass!
2. Run `cargo fmt` and `cargo clippy -j 1 --bin minicode -- -D warnings`.
3. Commit with: `feat(blocks): implement in-memory BlockStore with inverted indices and persistence`
4. Write execution report to `docs/superpowers/plans/task-2-report.md`.
