# JIT Repository Onboarding, Drift Arbitration & Deferred Setup Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement non-intrusive startup, deferred AI provider configuration, and just-in-time (JIT) repository analysis / workspace drift permission dialogs for minicode.

**Architecture:** Startup is 100% silent and clean across all repositories (`initial_modal = ModalState::None`). Prompt submission passes through two JIT gates: Gate 1 verifies provider readiness (intercepting unconfigured providers with a modal and preserving user input), and Gate 2 evaluates prompt intent (bypassing general queries, while asking permission on unindexed repos or drifted existing repos before performing AST analysis).

**Tech Stack:** Rust 2021, Ratatui, Crossterm, Petgraph, Ignore (WalkBuilder), Tokio async runtime.

## Global Constraints
- Compile checks and tests MUST use `-j 1`: `cargo check -j 1`, `cargo test -j 1`.
- Release build compiles with `-j 2`: `CARGO_BUILD_JOBS=2 cargo build --release`.
- Pure Rust networking and libraries; no external C SSL dependencies.
- Zero `.unwrap()` or `.expect()` calls in non-test code.
- NEVER run workspace-wide `cargo test`. ONLY run targeted tests for the specific module or integration test.
- Total tool count must remain synchronized with `TOTAL_TOOL_COUNT` (135).

---

### Task 1: Zero-AST Graph Drift Detection Engine (`CodeGraph::check_drift`)

**Files:**
- Modify: `src/context/graph/graph.rs`
- Modify: `src/context/graph/mod.rs`
- Test: `src/context/graph/graph.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct GraphDriftReport {
      pub is_stale: bool,
      pub modified_count: usize,
      pub added_count: usize,
      pub removed_count: usize,
      pub total_current_files: usize,
      pub cached_files_count: usize,
  }

  impl CodeGraph {
      pub fn check_drift(&self, workspace_root: &Path) -> Result<GraphDriftReport>;
  }
  ```

- [ ] **Step 1: Write unit tests in `src/context/graph/graph.rs`**
  - Add tests:
    - `test_drift_detector_clean_repo`: Validates zero drift when workspace files match cached snapshot hashes.
    - `test_drift_detector_stale_threshold`: Validates `is_stale == true` when $> 10$ files are modified/added or $> 20\%$ drift occurs.
- [ ] **Step 2: Run targeted test to verify failure**
  - Run `cargo test -j 1 --lib context::graph::graph::tests::test_drift_detector`
- [ ] **Step 3: Implement `GraphDriftReport` and `check_drift`**
  - Use `ignore::WalkBuilder` to collect current files with extensions in `SUPPORTED_LANG_EXTENSIONS`.
  - Compare file paths and `(len, mtime)` against `self.file_tracker.hashes`.
  - Only read and hash files whose `(len, mtime)` changed.
  - Return calculated `GraphDriftReport` with `is_stale = total_drift >= 10 || (cached > 0 && total_drift * 5 >= cached)`.
- [ ] **Step 4: Run targeted test to verify it passes**
  - Run `cargo test -j 1 --lib context::graph::graph::tests::test_drift_detector`
- [ ] **Step 5: Format and lint**
  - Run `cargo fmt --check` and `cargo clippy -j 1 --bin minicode -- -D warnings`
- [ ] **Step 6: Commit changes**
  - `feat(graph): implement zero-AST workspace drift detection engine (Phase 136)`

---

### Task 2: Provider Setup & Workspace Drift Modal UI Primitives

**Files:**
- Modify: `src/ui/modals/mod.rs`
- Modify: `src/ui/modals/workspace_analysis.rs`
- Test: `src/ui/modals/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum ModalState {
      // Existing variants...
      ProviderSetupRequired {
          provider_name: String,
          pending_prompt_preview: String,
          selected_index: usize,
      },
      WorkspaceDrift {
          modified_count: usize,
          added_count: usize,
          removed_count: usize,
          selected_index: usize,
      },
  }

  impl ModalState {
      pub fn new_provider_setup_required(provider_name: &str, prompt: &str) -> Self;
      pub fn new_workspace_drift(report: &crate::context::graph::GraphDriftReport) -> Self;
  }
  ```

- [ ] **Step 1: Write unit tests in `src/ui/modals/mod.rs`**
  - Test constructor initialization and state flags for `ProviderSetupRequired` and `WorkspaceDrift`.
- [ ] **Step 2: Run targeted test to verify failure**
  - Run `cargo test -j 1 --lib ui::modals::tests`
- [ ] **Step 3: Implement modal variants and rendering functions**
  - In `src/ui/modals/workspace_analysis.rs`:
    - Add `render_workspace_drift(frame, area, theme, modified, added, removed, selected_index)` with options:
      - `[1] ⚡ Incremental Sync (Update modified files, ~20ms)`
      - `[2] 🔄 Full Rebuild (Re-scan entire repository)`
      - `[3] ⏩ Skip (Continue with existing cached graph)`
    - Add `render_provider_setup_required(frame, area, theme, provider, prompt_preview, selected_index)` with options:
      - `[1] ⚡ Configure Provider & Key (Launch Setup Wizard)`
      - `[2] ◄ Cancel Prompt`
  - Wire dispatch in `ModalState::render`.
- [ ] **Step 4: Run targeted test to verify it passes**
  - Run `cargo test -j 1 --lib ui::modals::tests`
- [ ] **Step 5: Format and lint**
  - Run `cargo fmt --check` and `cargo clippy -j 1 --bin minicode -- -D warnings`
- [ ] **Step 6: Commit changes**
  - `feat(ui): add ProviderSetupRequired and WorkspaceDrift modal dialog primitives (Phase 136)`

---

### Task 3: JIT Intent Classifier & CRUD Gate Heuristics

**Files:**
- Modify: `src/agent/intent.rs`
- Test: `src/agent/intent.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn is_repository_crud_intent(prompt: &str, matched_intent: Option<&IntentMatch>) -> bool;
  ```

- [ ] **Step 1: Write unit tests in `src/agent/intent.rs`**
  - Test positive cases: file edits (`"add login in auth.rs"`), mutations (`"refactor the sql queries"`), `/explore`, `/map`, `/diff`, `/undo`.
  - Test negative cases: general queries (`"what is a mutex"`, `"explain python decorators"`), slash commands (`/help`, `/model`, `/theme`, `/context`).
- [ ] **Step 2: Run targeted test to verify failure**
  - Run `cargo test -j 1 --lib agent::intent::tests::test_crud_intent`
- [ ] **Step 3: Implement `is_repository_crud_intent`**
  - Classify based on recognized `AgentIntent` (`CodeReview`, `GitDiff`, `UndoRollback`, `RepoMap`, `CodeExplore`, `MilestonePlan`, `AutonomousGoal`).
  - Scan prompt for file path patterns (e.g. `src/`, `.rs`, `.py`, `.ts`, `.go`, `.json`).
  - Detect action verbs: `add`, `create`, `implement`, `fix`, `refactor`, `delete`, `remove`, `update`, `modify`, `edit`, `build`, `test`, `write`.
  - Return `false` for questions starting with `what is`, `how to`, `why does`, `explain`, `tell me about`.
- [ ] **Step 4: Run targeted test to verify it passes**
  - Run `cargo test -j 1 --lib agent::intent::tests::test_crud_intent`
- [ ] **Step 5: Format and lint**
  - Run `cargo fmt --check` and `cargo clippy -j 1 --bin minicode -- -D warnings`
- [ ] **Step 6: Commit changes**
  - `feat(agent): implement repository CRUD intent classifier for JIT gates (Phase 136)`

---

### Task 4: App JIT Interceptors, State Management & Execution Gate Wiring

**Files:**
- Modify: `src/app/mod.rs`
- Modify: `src/app/commands.rs`
- Modify: `src/app/modals.rs`
- Test: `src/app/mod.rs`

**Interfaces:**
- App state extensions:
  ```rust
  pub struct PendingSubmission {
      pub prompt: String,
      pub display: String,
  }

  // Fields on App:
  pub pending_submission: Option<PendingSubmission>,
  pub session_skipped_indexing: bool,
  pub session_skipped_drift: bool,
  ```

- [ ] **Step 1: Write unit tests in `src/app/mod.rs`**
  - Test `App::new` initializes with `initial_modal == ModalState::None` on unindexed repositories.
  - Test pending prompt preservation when opening setup modal.
- [ ] **Step 2: Run targeted test to verify failure**
  - Run `cargo test -j 1 --lib app::tests`
- [ ] **Step 3: Wire silent startup and JIT execution gates**
  - In `src/app/mod.rs`:
    - Remove the eager `if !graph_file.exists() { ModalState::new_workspace_analysis }` startup block. Set `initial_modal = ModalState::None`.
    - Add `pending_submission`, `session_skipped_indexing`, `session_skipped_drift` to `App`.
  - In `src/app/commands.rs` inside `handle_command_or_prompt`:
    - Check if prompt is `"analyze the full project"`, `"analyze project"`, `/init`, `/index`, `/analyze` $\to$ trigger analysis.
    - **Gate 1:** If provider is unconfigured (missing key, not local):
      - Store `pending_submission = Some(PendingSubmission { prompt, display })`.
      - Set `self.modal = ModalState::new_provider_setup_required(&self.config.provider.default, &prompt)`.
      - Return `Ok(CommandAction::Continue)`.
    - **Gate 2:** If `is_repository_crud_intent(&prompt, matched_intent)`:
      - If `.minicode/graph.json` does not exist and `!self.session_skipped_indexing`:
        - Store `pending_submission`.
        - Set `self.modal = ModalState::new_workspace_analysis(&self.workspace_root)`.
        - Return `Ok(CommandAction::Continue)`.
      - If `.minicode/graph.json` exists and `!self.session_skipped_drift`:
        - Load graph and check drift via `graph.check_drift(&self.workspace_root)`.
        - If `drift.is_stale`:
          - Store `pending_submission`.
          - Set `self.modal = ModalState::new_workspace_drift(&drift)`.
          - Return `Ok(CommandAction::Continue)`.
  - In `src/app/modals.rs`:
    - Implement keyboard handlers for `ProviderSetupRequired` and `WorkspaceDrift`.
    - On action execution (index, sync, rebuild, skip, or key entry), pop `pending_submission` and re-dispatch prompt to `control_tx`!
- [ ] **Step 4: Run targeted test to verify it passes**
  - Run `cargo test -j 1 --lib app::tests`
- [ ] **Step 5: Format and lint**
  - Run `cargo fmt --check` and `cargo clippy -j 1 --bin minicode -- -D warnings`
- [ ] **Step 6: Commit changes**
  - `feat(app): wire silent startup, JIT provider gate, and drift permission dialogs (Phase 136)`

---

### Task 5: End-to-End Integration Test Suite

**Files:**
- Create: `tests/integration_jit_onboarding.rs`

- [ ] **Step 1: Write integration tests**
  - `test_unindexed_repo_silent_startup`: Verifies `App::new` on empty temp directory produces `modal == ModalState::None`.
  - `test_general_query_bypasses_analysis`: Verifies general questions do not open `WorkspaceAnalysis`.
  - `test_crud_prompt_intercepted_on_unindexed_repo`: Verifies code edit prompt opens `WorkspaceAnalysis` on unindexed repo.
  - `test_unconfigured_provider_triggers_setup_modal`: Verifies prompt triggers `ProviderSetupRequired` when API key is missing.
  - `test_existing_repo_drift_detection`: Verifies modifying 15 files in an indexed repo triggers `WorkspaceDrift`.
- [ ] **Step 2: Run integration tests**
  - Run `cargo test -j 1 --test integration_jit_onboarding`
- [ ] **Step 3: Format and lint**
  - Run `cargo fmt --check` and `cargo clippy -j 1 --test integration_jit_onboarding -- -D warnings`
- [ ] **Step 4: Commit changes**
  - `test(onboarding): add integration test suite for JIT onboarding and drift gates (Phase 136)`

---

### Task 6: Quality Gates, Version Bump (v0.3.37), Release Build & Real-World Validation

**Files:**
- Modify: `Cargo.toml`
- Modify: `onpkg_docs/todo.md`

- [ ] **Step 1: Bump version in `Cargo.toml` to `0.3.37`**
- [ ] **Step 2: Update `onpkg_docs/todo.md` with Phase 136 details**
- [ ] **Step 3: Run full quality gates**
  - `cargo check -j 1`
  - Targeted unit & integration tests:
    - `cargo test -j 1 --lib context::graph::graph::tests`
    - `cargo test -j 1 --lib ui::modals::tests`
    - `cargo test -j 1 --lib agent::intent::tests`
    - `cargo test -j 1 --test integration_jit_onboarding`
  - `cargo clippy -j 1 --bin minicode -- -D warnings`
  - `cargo fmt --check`
- [ ] **Step 4: Compile release binary via `./localupdate.sh`**
- [ ] **Step 5: Commit release and push**
  - `release(v0.3.37): deliver JIT repository onboarding, drift arbitration, and deferred setup engine (Phase 136)`
