# Task 1 Brief: MiniBlocks Domain Models & Error Definitions

## Goal
Implement the domain models, error types, category/framework enums, and hex color validation for MiniBlocks in `minicode`.

## Target Files
- Create: `src/blocks/models.rs`
- Create: `src/blocks/mod.rs`
- Modify: `src/lib.rs` (expose `pub mod blocks;`)
- Tests: Inline in `src/blocks/models.rs`

## Global Constraints
1. **Targeted Tests ONLY:** Run ONLY `cargo test -j 1 --lib blocks::models::tests`. Never run the full test suite.
2. **Error Handling:** Zero `.unwrap()` or `.expect()` in non-test code. Use `thiserror` for `BlockError`.
3. **Concurrency:** Always use `-j 1` for `cargo check` and `cargo test`.
4. **Pure Rust:** No external C libraries or Python scripts.
5. **No `cd` commands.**

## Interfaces & Types to Produce

### 1. `BlockCategory` (`src/blocks/models.rs`)
An enum with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]` and `#[serde(rename_all = "lowercase")]`:
Variants: `Navbar`, `Hero`, `Footer`, `Sidebar`, `Card`, `Form`, `Modal`, `Table`, `Pricing`, `Testimonial`, `Cta`, `Feature`, `Faq`, `Contact`, `Auth`, `Dashboard`, `Settings`, `Profile`, `Landing`, `Blog`, `Ecommerce`, `Error`, `Loading`, `Notification`, `Button`, `Input`, `Section`, `Other`.
Implement `from_str_loose(s: &str) -> Self` with flexible case-insensitive and hyphen/underscore normalization.

### 2. `BlockFramework` (`src/blocks/models.rs`)
An enum with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]` and `#[serde(rename_all = "lowercase")]`:
Variants: `Tailwind`, `Css`, `Scss`, `Shadcn`, `React`, `Svelte`.
Implement `from_str_loose(s: &str) -> Self` (normalizing `tailwindcss` -> `Tailwind`, `vanillacss` -> `Css`, `sass` -> `Scss`, `shadcnui` -> `Shadcn`, `jsx`/`tsx` -> `React`).

### 3. `BlockComponent` (`src/blocks/models.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockComponent {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: String,
    pub category: BlockCategory,
    pub framework: BlockFramework,
    pub code: String,
    pub dependencies: Vec<String>,
    pub tags: Vec<String>,
    pub version: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
```
Implement `BlockComponent::new(name, description, category, framework, code, dependencies, tags) -> Self`.

### 4. `BlockPalette` (`src/blocks/models.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPalette {
    pub id: uuid::Uuid,
    pub name: String,
    pub colors: [String; 4], // Exactly 4 hex tokens
    pub tags: Vec<String>,
}
```
Implement `BlockPalette::new(name, colors: [String; 4], tags: Vec<String>) -> Result<Self, BlockError>`.
Validate that all 4 entries match a valid hex code (e.g. `#RRGGBB` or `#RGB`, 4 or 7 chars starting with `#` and valid hex digits). Return `BlockError::InvalidHexColor(color)` if invalid.

### 5. `BlockGradient` (`src/blocks/models.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockGradient {
    pub id: uuid::Uuid,
    pub name: String,
    pub css: String,
    pub colors: Vec<String>,
    pub tags: Vec<String>,
}
```
Implement `BlockGradient::new(name, css, colors, tags) -> Self`.

### 6. `BlockTemplate` (`src/blocks/models.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: String,
    pub component_ids: Vec<uuid::Uuid>,
    pub base_layout: String,
    pub default_variables: serde_json::Value,
}
```

### 7. `BlockStats` (`src/blocks/models.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockStats {
    pub total_components: usize,
    pub total_palettes: usize,
    pub total_gradients: usize,
    pub total_templates: usize,
    pub category_counts: std::collections::HashMap<BlockCategory, usize>,
    pub framework_counts: std::collections::HashMap<BlockFramework, usize>,
}
```

### 8. `BlockError` (`src/blocks/mod.rs`)
Using `thiserror`:
```rust
#[derive(Debug, thiserror::Error)]
pub enum BlockError {
    #[error("Component not found: {0}")]
    ComponentNotFound(String),
    #[error("Palette not found: {0}")]
    PaletteNotFound(String),
    #[error("Gradient not found: {0}")]
    GradientNotFound(String),
    #[error("Template not found: {0}")]
    TemplateNotFound(String),
    #[error("Invalid hex color code: '{0}' (expected #RGB or #RRGGBB)")]
    InvalidHexColor(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
```

## Unit Tests
Implement unit tests in `src/blocks/models.rs` testing:
1. `test_category_serialization_and_parsing`
2. `test_framework_serialization_and_parsing`
3. `test_palette_hex_validation` (valid 4-hex codes pass; invalid 3-char without `#` or malformed strings fail)
4. `test_component_instantiation`

## Deliverables
1. Run `cargo test -j 1 --lib blocks::models::tests` -> must pass!
2. Run `cargo fmt` and `cargo clippy -j 1 --bin minicode -- -D warnings`.
3. Commit with: `feat(blocks): implement MiniBlocks domain models, categories, and palette validation`
4. Write execution report to `docs/superpowers/plans/task-1-report.md`.
