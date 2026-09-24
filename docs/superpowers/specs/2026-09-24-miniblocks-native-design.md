# MiniBlocks: Native UI Component & Design Warehouse Specification

> **Status:** APPROVED  
> **Author:** Antigravity Team  
> **Target Subsystem:** `minicode::blocks` & `minicode::tools::registry::block_tools`  
> **Date:** 2026-09-24  

---

## 1. Executive Summary & Vision

**MiniBlocks** is a fast, local-first, in-process UI component, palette, gradient, and layout template warehouse built directly into `minicode`.

Instead of relying on an external subprocess MCP server (like OpenBlocks over stdio child-process pipes), MiniBlocks is embedded natively within `minicode`'s multi-threaded Tokio runtime. It gives AI coding agents and human developers instant (<1ms), zero-token-overhead access to a curated library of **over 1,080 production-ready UI components**, **105 color palettes**, **212 CSS gradients**, and **multi-section layout templates**.

MiniBlocks integrates with:
1. **AST CodeGraph & Tech Stack Auto-Detection:** Automatically detects whether the user's project uses React, Tailwind CSS, Svelte, or Vanilla CSS and auto-scopes recommendations.
2. **MiniKit Documentation System:** Exposes design system guidelines and component catalogs in `minikit_docs/skills/miniblocks.md`.
3. **MiniPower Autonomous Execution:** Powers `power_plan` and `power_verify` with pre-tested building blocks rather than hallucinating hundreds of lines of CSS/HTML from scratch.
4. **Interactive TUI Warehouse Modal (`/blocks`):** A full-featured Ratatui browser for searching, inspecting with syntax highlighting, and inserting components directly into project files.

---

## 2. Technical Invariants & Constraints

1. **In-Process & Pure Rust Portability:**
   - Must run inside the `minicode` binary with zero external child processes, zero python dependencies, and zero C OpenSSL dependencies.
   - Pure Rust storage with thread-safe `Arc<RwLock<BlockStore>>` providing sub-millisecond query execution.
2. **Offline-First & Zero Setup:**
   - The entire seeded catalog (1,082 components, 105 palettes, 212 gradients, 3 templates) is embedded or bundled directly into `minicode`.
   - Never requires internet access or API keys to retrieve components.
3. **Dual-Layer Persistence:**
   - **Global Layer:** User-custom blocks, ratings, and edits stored in `~/.local/share/minicode/miniblocks/`.
   - **Project Layer:** Project-specific blocks stored in `.minicode/blocks/` (auto-ignored by git via `.gitignore`).
4. **Error Handling & Code Safety:**
   - `thiserror` for crate errors (`BlockError`).
   - Zero `.unwrap()` or `.expect()` in non-test production code.
5. **Tool Registry Synchronization:**
   - Adds 10 new tools (`block_*`), updating `TOTAL_TOOL_COUNT` from 156 to 166.
   - Passes `tools::tests::test_total_tool_count` and `test_total_tool_count_matches`.

---

## 3. Data Models & Architecture

### 3.1 Domain Models (`src/blocks/models.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockCategory {
    Navbar, Hero, Footer, Sidebar, Card, Form, Modal, Table,
    Pricing, Testimonial, Cta, Feature, Faq, Contact, Auth,
    Dashboard, Settings, Profile, Landing, Blog, Ecommerce,
    Error, Loading, Notification, Button, Input, Section, Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockFramework {
    Tailwind, Css, Scss, Shadcn, React, Svelte,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockComponent {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub category: BlockCategory,
    pub framework: BlockFramework,
    pub code: String,
    pub dependencies: Vec<String>,
    pub tags: Vec<String>,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPalette {
    pub id: Uuid,
    pub name: String,
    pub colors: [String; 4], // Exactly 4 hex tokens: [Background, Surface, Accent, Text]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockGradient {
    pub id: Uuid,
    pub name: String,
    pub css: String,
    pub colors: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub component_ids: Vec<Uuid>,
    pub base_layout: String,
    pub default_variables: serde_json::Value,
}
```

### 3.2 In-Memory Store & Fast Inverted Index (`src/blocks/store.rs`)

```rust
pub struct BlockStore {
    components: HashMap<Uuid, BlockComponent>,
    palettes: HashMap<Uuid, BlockPalette>,
    gradients: HashMap<Uuid, BlockGradient>,
    templates: HashMap<Uuid, BlockTemplate>,
    // Inverted indices for O(1) filtering
    category_index: HashMap<BlockCategory, HashSet<Uuid>>,
    framework_index: HashMap<BlockFramework, HashSet<Uuid>>,
    tag_index: HashMap<String, HashSet<Uuid>>,
    // Metadata cache for fuzzy token matching
    search_tokens: HashMap<Uuid, Vec<String>>,
    custom_store_path: PathBuf,
}
```

---

## 4. MiniBlocks Tool Suite (10 Tools)

1. **`block_search`**: Fuzzy multi-token search over components (name, description, tags, category, framework).
2. **`block_get`**: Fetch full source code, preview, and dependencies by UUID or slug name.
3. **`block_insert`**: Directly inject a component or snippet into a target file in the active workspace (`append`, `prepend`, `create`, or `replace_section`).
4. **`block_save`**: Save a project component into the user's permanent MiniBlocks library.
5. **`block_update`**: Update an existing component with automated version incrementing.
6. **`block_delete`**: Delete a user-created block.
7. **`block_palettes`**: Query and retrieve 4-hex design token palettes.
8. **`block_gradients`**: Query and retrieve CSS linear/radial gradient definitions.
9. **`block_scaffold`**: Assemble a complete multi-section layout template into workspace files.
10. **`block_stats`**: Return library breakdown (total components, categories, frameworks, palettes, gradients).

---

## 5. TUI Interactive Warehouse (`/blocks` Modal)

- **Entrypoints:** `/blocks` command or `F6` shortcut.
- **Layout:**
  - Tab 1: Components (Search input, category tree, framework selector, component list, syntax-highlighted code preview).
  - Tab 2: Palettes (Interactive palette cards with rendered color swatches and hex copy).
  - Tab 3: Gradients (Rendered CSS gradient color preview cards).
  - Tab 4: Templates (Page scaffolding assistant).
- **Actions:**
  - `[Enter]`: Insert into current or target file.
  - `[c]`: Copy code to clipboard / prompt buffer.
  - `[s]`: Save current project selection as a new block.
  - `[Esc]`: Close modal.

---

## 6. MiniKit & MiniPower Synergy

- **MiniKit Sync:** Automatically creates `minikit_docs/skills/miniblocks.md` detailing all component categories, popular palette tokens, and instructions for how the AI should assemble UIs.
- **MiniPower Planning:** When a plan touches frontend UI files, MiniPower prompts the planner to consult `block_search` first, preventing token-heavy hallucinated CSS.
- **CodeGraph Detection:** If the AST repomap detects `React` or `Tailwind` dependencies in the project, `block_search` automatically prioritizes React and Tailwind variants.
