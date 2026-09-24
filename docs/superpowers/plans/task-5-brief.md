# Task 5 Brief: Interactive TUI Warehouse Modal (`/blocks` & `F6`)

## Goal
Implement the interactive Ratatui warehouse modal dialog (`src/ui/modals/blocks.rs`) allowing developers to visually explore, search, preview (with syntax styling), and 1-click inject UI components, design token palettes, CSS gradients, and page templates into their workspace directly from the TUI with `/blocks` or shortcut key `F6`.

## Target Files
- Create: `src/ui/modals/blocks.rs`
- Modify: `src/ui/modals/mod.rs` (expose `pub mod blocks;`, add `ModalState::Blocks(blocks::BlocksModalState)`, `ModalState::new_blocks()`, and render dispatch)
- Modify: `src/app/commands.rs` (handle `/blocks` and `/miniblocks` slash commands)
- Modify: `src/app/mod.rs` (bind key `F(6)` to toggle `ModalState::Blocks`)
- Modify: `src/app/modals.rs` (handle keyboard interaction when `ModalState::Blocks` is active)
- Modify: `src/ui/modals/command_catalog.rs` (add `/blocks` item)
- Modify: `src/ui/modals/help.rs` (add `/blocks` and `F6` shortcut to help modal)
- Tests: Inline in `src/ui/modals/blocks.rs`

## Global Constraints
1. **Targeted Tests ONLY:** Run ONLY `cargo test -j 1 --lib ui::modals::blocks::tests`. Never run the full test suite.
2. **Error Handling:** Zero `.unwrap()` or `.expect()` in non-test code.
3. **Concurrency:** Always use `-j 1` for `cargo check` and `cargo test`.
4. **Pure Rust:** No external C libraries or Python scripts.
5. **No `cd` commands.**
6. **Alternate screen safety:** All UI rendering must cleanly stay within bounds and avoid out-of-screen panics.

## Interfaces & Architecture to Produce

### 1. `BlocksTab` (`src/ui/modals/blocks.rs`)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlocksTab {
    Components,
    Palettes,
    Gradients,
    Templates,
}
```
With `next(&self) -> Self`, `prev(&self) -> Self`, `title(&self) -> &'static str`.

### 2. `BlocksModalState` (`src/ui/modals/blocks.rs`)
```rust
#[derive(Debug, Clone)]
pub struct BlocksModalState {
    pub workspace_root: PathBuf,
    pub active_tab: BlocksTab,
    pub search_query: String,
    pub selected_index: usize,
    pub preview_scroll_offset: usize,
    pub filtered_component_ids: Vec<uuid::Uuid>,
    pub filtered_palette_ids: Vec<uuid::Uuid>,
    pub filtered_gradient_ids: Vec<uuid::Uuid>,
    pub filtered_template_ids: Vec<uuid::Uuid>,
    pub status_message: Option<String>,
}
```
Methods:
- `pub fn new(workspace: &Path) -> Self`
- `pub fn next_tab(&mut self)`
- `pub fn prev_tab(&mut self)`
- `pub fn select_next(&mut self)`
- `pub fn select_prev(&mut self)`
- `pub fn scroll_preview_up(&mut self)`
- `pub fn scroll_preview_down(&mut self)`
- `pub fn handle_char(&mut self, c: char)`
- `pub fn handle_backspace(&mut self)`
- `pub fn refresh_filtered(&mut self)`
- `pub fn get_selected_code(&self) -> Option<String>`

### 3. Rendering (`render_blocks_modal`)
- Layout: 90% width, 85% height centered dialog.
- Top: Tab bar (`[1] Components (1082+)`, `[2] Palettes (105)`, `[3] Gradients (212)`, `[4] Templates (3)`).
- Tab 1: Components
  - Left pane (35%): Search input box + scrollable list of filtered components with category badge and framework badge.
  - Right pane (65%): Component title, category, framework, tags, dependencies, and code preview with line numbers and syntax styling.
- Tab 2: Palettes
  - Visual cards showing 4-color swatches with hex tokens, name, and tags.
- Tab 3: Gradients
  - Visual cards with gradient CSS definition and color stops.
- Tab 4: Templates
  - Scaffolding assistant cards with constituent component count and layout details.
- Bottom: Hotkey bar (`[Tab] Next Tab  [↑/↓] Select  [PgUp/PgDn] Scroll  [Enter] Insert  [c] Copy  [Esc] Close`).

### 4. Keybinding & Slash Command
- `/blocks` and `/miniblocks` in `src/app/commands.rs` opens `ModalState::new_blocks(&self.workspace_root)`.
- `F6` in `src/app/mod.rs` toggles `ModalState::Blocks`.
- Key handling in `src/app/modals.rs`:
  - `Tab` / `Right` -> `next_tab()`
  - `BackTab` / `Left` -> `prev_tab()`
  - `1`..`4` -> switch to specific tab
  - `Up` / `Down` -> `select_prev()` / `select_next()`
  - `PageUp` / `PageDown` -> `scroll_preview_up()` / `scroll_preview_down()`
  - `Enter` -> inject into workspace / copy
  - `Esc` -> close modal
  - Typing characters / Backspace -> updates `search_query` and calls `refresh_filtered()`.

### 5. Tests (`src/ui/modals/blocks.rs`)
- `test_blocks_modal_navigation_and_tabs`: tab cycling and boundary clamping.
- `test_blocks_modal_filtering`: typing query filters components and updates `filtered_component_ids`.
- `test_blocks_modal_preview_scrolling`: scroll preview up/down.
