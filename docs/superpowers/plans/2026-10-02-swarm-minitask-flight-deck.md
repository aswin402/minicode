# Swarm MiniTask Integration & Dynamic Multi-Style Flight Deck — Implementation Plan

**Phase:** Phase 153  
**Status:** In Progress  
**Spec Reference:** `docs/superpowers/specs/2026-10-02-swarm-minitask-flight-deck-design.md`  
**Target Version:** `minicode v0.3.45`  
**Tool Count Invariant:** `TOTAL_TOOL_COUNT = 186`

---

## Task Breakdown

### Task 1: Theme & Config System: 5 Swarm Styles & `/theme` Modal Tab
- **Files Modified:**
  - `src/config.rs`: Add `swarm_style: String` (default: `"stylish"`) to `UiConfig` and `SwarmDashboardStyle` enum (`Stylish`, `GitGraph`, `Modern`, `Minimal`, `Cockpit`).
  - `src/ui/modals/mod.rs`: Add `SwarmStyles` variant to `ThemeModalTab`.
  - `src/ui/modals/theme_select.rs`: Add `SWARM_STYLE_OPTIONS` with ASCII/Unicode previews, descriptions, and render tab for swarm styles.
  - `src/app/commands.rs`: Add `/swarm-style [style]` command and update `/theme` to initialize with `SwarmStyles` context.
  - `src/app/modals.rs`: Add keyboard routing for `ThemeModalTab::SwarmStyles`.
- **Targeted Tests:** `cargo test -j 1 --lib config::tests`, `cargo test -j 1 --lib ui::modals::theme_select::tests`.

### Task 2: MiniTask Process Registry Expansion & Swarm Worker Supervision
- **Files Modified:**
  - `src/dev/models.rs`: Add `DevProcessType::Swarm` to `DevProcessType` enum, update `from_str_loose` and `Display`.
  - `src/agent/swarm/scheduler.rs`: Register parent swarm and worker child processes into `get_global_dev_registry()` upon launch, stream worker logs to ring buffer, and mark complete upon termination.
  - `src/tools/registry/dev_tools.rs`: Update `minitask` dispatch to handle `DevProcessType::Swarm` filtering and status queries.
- **Targeted Tests:** `cargo test -j 1 --lib dev::registry::tests`, `cargo test -j 1 --lib tools::registry::dev_tools::tests`.

### Task 3: Swarm Flight Deck Data Model & 5 Dynamic Theme Renderers
- **Files Created:**
  - `src/ui/modals/swarm_deck.rs`: Swarm flight deck data gatherer and 5 layout renderers:
    - `render_stylish()`: Minimal + Stylish (Default ⭐)
    - `render_gitgraph()`: Minimal + GitGraph Pipeline
    - `render_modern()`: Clean Modernist
    - `render_minimal()`: Ultra-Minimalist
    - `render_cockpit()`: High-Density Cockpit
  - All renderers take `&Theme` to ensure 100% dynamic theming.
- **Files Modified:**
  - `src/ui/modals/mod.rs`: Export `pub mod swarm_deck;`.
- **Targeted Tests:** `cargo test -j 1 --lib ui::modals::swarm_deck::tests`.

### Task 4: Interactive Integration into Processes & Task Modal (`/tasks`)
- **Files Modified:**
  - `src/ui/modals/processes.rs`: Add `ProcessesTab::Swarm` to `ProcessesTab::all()`, tab navigation (`next`, `prev`), and delegate rendering to `swarm_deck` when active.
  - `src/app/modals.rs`: Wire interactive keyboard events for `ProcessesTab::Swarm` (`[Tab]` switch tabs, `[↑/↓]` select worker, `[Space]` logs, `[m]` bus transcript, `[k]` kill worker, `[s]` cycle style, `[r]` report).
- **Targeted Tests:** `cargo test -j 1 --lib ui::modals::processes::tests`.

### Task 5: End-to-End Integration Suite, Todo Documentation & Global Release
- **Files Created:**
  - `tests/integration_swarm_minitask_flight_deck.rs`: 4 comprehensive integration tests:
    1. Swarm and worker registration in `MiniDevRegistry`
    2. Swarm dashboard style switching via `/swarm-style` and config persistence
    3. Live inter-worker `bus.jsonl` message tailing in flight deck
    4. Theme adaptation verification across multiple color palettes
- **Files Modified:**
  - `onpkg_docs/core/todo.md`: Update task tracker with Phase 153.
  - `Cargo.toml`: Bump version to `0.3.45`.
- **Verification Commands:**
  - `cargo test -j 1 --test integration_swarm_minitask_flight_deck`
  - `cargo clippy -j 1 --bin minicode -- -D warnings`
  - `cargo fmt --check`
  - `cargo build --release -j 2`
  - Install binary: `cp target/release/minicode ~/.local/bin/minicode`
  - Verify: `~/.local/bin/minicode --version`
