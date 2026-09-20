# Design Specification: Just-In-Time (JIT) Repository Onboarding, Drift Arbitration & Deferred Setup Engine

**Date:** 2026-09-20  
**Status:** Approved  
**Target Version:** minicode v0.3.37 (Phase 136)  

---

## 1. Executive Summary & Problem Statement

### 1.1 Problems Solved
1. **Intrusive Startup Modal in New Repositories:** When launching `minicode` in a new repository (where `.minicode/graph.json` has not been generated), `App::new` immediately forces `ModalState::new_workspace_analysis` onto the screen. This covers the Aura welcome screen with an unsolicited prompt asking whether to index the workspace before the user has even typed a character.
2. **Ugly Runtime Errors on Unconfigured Providers:** When a first-time user opens `minicode` without having configured an API key or provider, submitting any prompt triggers a raw background provider error (`Provider 'xxx' is not configured: ...`) in the chat timeline, rather than intercepting the prompt cleanly with a native modal dialog and saving the user's input.
3. **Repeated Scans in Existing Repositories:** When returning to an established repository where `minicode` was previously run, users should never be prompted to analyze the full project unless they explicitly ask for it (e.g. `"analyze the full project"`, `/init`, `/index`), OR when significant workspace drift is detected (e.g. $>10$ files modified/added/deleted since the last index).
4. **Sub-optimal Utilization of `.minicode/` Directory:** The project-local `.minicode/` directory must be utilized cleanly and safely—ensured to be excluded from Git, and used as a fast, $<5$ms cache for AST symbols, session histories, backups, and goal tracking.

---

## 2. Architectural Design & Lifecycle States

### 2.1 Workspace Lifecycle States
When `minicode` boots up in any directory, it classifies the repository into one of three states without displaying any upfront modals:

```
                  ┌─────────────────────────────────────┐
                  │          App::new(workspace)        │
                  │   initial_modal = ModalState::None  │
                  └──────────────────┬──────────────────┘
                                     │
                 ┌───────────────────┴───────────────────┐
                 ▼                                       ▼
    [.minicode/graph.json Missing]          [.minicode/graph.json Present]
    ┌────────────────────────────┐          ┌────────────────────────────┐
    │   State 1: New / Unindexed │          │ State 2 / 3: Existing Repo │
    │   - Instant Welcome Screen │          │ - Hydrate Graph in <5ms    │
    │   - Zero Upfront Popups    │          │ - Instant Welcome Screen   │
    └────────────────────────────┘          └──────────────┬─────────────┘
                                                           │
                                   ┌───────────────────────┴───────────────────────┐
                                   ▼                                               ▼
                         [Drift <= 3 Files]                              [Drift > 10 Files]
                    ┌────────────────────────────┐                 ┌────────────────────────────┐
                    │ State 2: Existing (Fresh)  │                 │ State 3: Existing (Drifted)│
                    │ - Background sync if needed│                 │ - Flag drift for CRUD gate │
                    │ - Zero Popups              │                 │ - Zero Startup Popups      │
                    └────────────────────────────┘                 └────────────────────────────┘
```

---

## 3. The 2-Tier Just-In-Time (JIT) Execution Gates

When the user submits a prompt in the input dock (`Enter`), execution flows through two non-blocking gates before reaching the background agent actor (`AgentLoop`):

### Gate 1: Deferred Provider & API Key Interceptor
Before touching the repository or dispatching prompt execution:
1. **Check:** Verify if the active provider is operational:
   - Does `config.get_api_key(&config.provider.default)` return a valid key?
   - OR is it a known local zero-key provider (`ollama`, `local`, `lmstudio`, `vllm`)?
2. **If Configured:** Proceed directly to Gate 2.
3. **If Unconfigured:**
   - Intercept prompt submission.
   - Save user prompt in `App.pending_prompt: Option<PendingSubmission>`.
   - Open native modal dialog: **`ModalState::ProviderSetupRequired`**.
   - Options:
     - `[1] ⚡ Configure Provider & Key (Launch Setup Wizard / Key Prompt)`
     - `[2] ◄ Cancel & Return to Prompt`
   - Upon successful key configuration:
     - Save API key and update config.
     - Automatically pop and dispatch the saved `pending_prompt` without requiring the user to retype it.
   - Upon cancellation (`Esc` / `[2]`):
     - Restore prompt text into `input_dock.textarea`.

---

### Gate 2: Workspace Analysis & Drift Permission Interceptor
Once Gate 1 passes:
1. **Intent Analysis:** Evaluate the prompt using `crate::agent::intent::match_intent` and keyword heuristics:
   - **General Queries & Read-Only Inquiries:**
     - Examples: *"What is an Arc in Rust?"*, *"Explain how OAuth works"*, *"Write a standalone quicksort algorithm"*, `/help`, `/model`, `/theme`, `/context`.
     - **Action:** **Bypass Gate 2 immediately**. Zero analysis, zero delays, executes turn instantly.
   - **CRUD & Code Modification Inquiries:**
     - Examples: *"Add a login route in src/auth.rs"*, *"Refactor the database queries"*, *"Fix the bug in token expiration"*, *"Delete unused helpers"*, `/explore`, `/map`.
     - **Action:** Check repository index status:
       - **Case A (New / Unindexed Repository):**
         - If `session_skipped_indexing == true`: Bypass gate, proceed in lightweight text search mode.
         - If `session_skipped_indexing == false`:
           - Intercept prompt submission and save into `App.pending_prompt`.
           - Open **`ModalState::new_workspace_analysis(workspace_root)`** with:
             - `[1] ⚡ Quick Index (AST symbols & PageRank graph — recommended)`
             - `[2] 🔬 Deep Scan (Symbol graph + architecture scan)`
             - `[3] ⏩ Skip for Now (Lightweight file search only)`
           - Selection `[1]` / `[2]`: Build AST graph, write `.minicode/graph.json`, post confirmation card to timeline, and dispatch `pending_prompt`.
           - Selection `[3]` / `Esc`: Set `session_skipped_indexing = true`, post lightweight mode notice to timeline, and dispatch `pending_prompt`.
       - **Case B (Existing Repository — Drift Detected):**
         - If $> 10$ files modified/added/deleted since last index and `session_skipped_drift == false`:
           - Intercept prompt submission and save into `App.pending_prompt`.
           - Open **`ModalState::new_workspace_drift(drift_stats)`** with:
             - `[1] ⚡ Incremental Sync (Update changed files, ~20ms)`
             - `[2] 🔄 Full Rebuild (Re-scan entire repository)`
             - `[3] ⏩ Skip (Use existing cached graph)`
           - Selection `[1]`: Run `CodeGraph::incremental_update`, persist `.minicode/graph.json`, post receipt, and dispatch `pending_prompt`.
           - Selection `[2]`: Run `CodeGraph::full_rebuild`, persist, post receipt, and dispatch `pending_prompt`.
           - Selection `[3]` / `Esc`: Set `session_skipped_drift = true`, dispatch `pending_prompt`.
       - **Case C (Existing Repository — Minor / No Drift):**
         - Files modified $\le 3$: Automatically apply `incremental_update` seamlessly in the background without modal interruption.
       - **Case D (User Explicitly Requests Analysis):**
         - Prompt matches `"analyze the full project"`, `"analyze project"`, `/init`, `/index`, `/analyze`.
         - Open `WorkspaceAnalysis` modal or run full re-index directly.

---

## 4. Optimal Utilization of `.minicode/`

The `.minicode/` workspace directory is structured into dedicated operational zones:

| Path | Purpose | Lifecycle / Eviction Policy |
| :--- | :--- | :--- |
| `.minicode/graph.json` | AST Symbol & PageRank code graph snapshot + file hashes | Updated on index/sync; $<5$ms cold load |
| `.minicode/cache/semantic_index.bin` | Turbovec 1-bit quantized vector embeddings | Persisted AST semantic index |
| `.minicode/intent_anchor.json` | Living goal ledger & execution checklist | Persisted across turns; edited via `/goal` |
| `.minicode/sessions/` | Project-scoped JSON session trajectories | Retained locally; accessible via `/resume` |
| `.minicode/backups/` | Turn-by-turn diff snapshots & undo checkpoints | Auto-pruned after turn depth $> 20$ |
| `.minicode/worktrees/` | Isolated subagent git worktrees | Ephemeral; cleaned up on subagent finish/merge |
| `.minicode/config.toml` | Optional project-local configuration overrides | Loaded in 3-tier config hierarchy |

### Git Hygiene
On startup, `minicode` verifies that `.git/info/exclude` contains `.minicode/` (via `src/git/service.rs`). This guarantees zero Git pollution, zero uncommitted file noise, and zero accidental check-ins into user repositories.

---

## 5. UI Modal Specifications

### 5.1 Provider Setup Dialog Modal (`ModalState::ProviderSetupRequired`)
- **Dimensions:** 62 columns $\times$ 14 lines, centered overlay.
- **Keybindings:**
  - `↑` / `k` / `Down` / `j`: Move selection.
  - `Enter`: Select option (Option 0 launches Setup Wizard / Key Prompt).
  - `Esc` / `q`: Cancel and restore pending prompt to input dock.
- **Content:**
  ```text
  ┌──────────────────────────────────────────────────────────┐
  │ 🔑 AI Provider Configuration Required                    │
  │                                                          │
  │ Active Provider: [ minimax ]                             │
  │ Status: No API key configured.                           │
  │ Pending Prompt: "fix auth token expiration"              │
  │                                                          │
  │   [1] ⚡ Run Interactive Setup (Configure Provider & Key) │
  │   [2] ◄ Cancel Prompt                                    │
  │                                                          │
  │ [↑/↓/j/k] Navigate   [ENTER] Select   [ESC] Cancel       │
  └──────────────────────────────────────────────────────────┘
  ```

### 5.2 Workspace Drift Dialog Modal (`ModalState::WorkspaceDrift`)
- **Dimensions:** 68 columns $\times$ 14 lines, centered overlay.
- **Keybindings:**
  - `↑` / `k` / `Down` / `j`: Move selection.
  - `1`, `2`, `3`: Direct number selection.
  - `Enter`: Select highlighted option.
  - `Esc` / `q`: Skip drift update for current session and proceed with prompt.
- **Content:**
  ```text
  ┌──────────────────────────────────────────────────────────────────┐
  │ ⚠️ Code Graph Out of Date (Workspace Drift Detected)             │
  │                                                                  │
  │ Detected 24 modified, added, or removed files since last index.  │
  │ Updating the graph ensures accurate symbol navigation.           │
  │                                                                  │
  │   [1] ⚡ Incremental Sync (Update modified files, ~20ms)         │
  │   [2] 🔄 Full Rebuild (Re-scan entire repository from scratch)   │
  │   [3] ⏩ Skip (Continue with existing cached graph)              │
  │                                                                  │
  │ [↑/↓/j/k] Navigate   [ENTER] Select   [ESC] Skip                 │
  └──────────────────────────────────────────────────────────────────┘
  ```

---

## 6. Drift Detection Algorithm

A lightweight, zero-AST-parsing drift detector is added to `CodeGraph`:

```rust
pub struct GraphDriftReport {
    pub is_stale: bool,
    pub modified_count: usize,
    pub added_count: usize,
    pub removed_count: usize,
    pub total_current_files: usize,
    pub cached_files_count: usize,
}
```

**Drift Calculation:**
1. Collect current files matching `SUPPORTED_LANG_EXTENSIONS` using `ignore::WalkBuilder` with `.git` and hidden file filtering.
2. Compare with `snapshot.file_hashes`:
   - Files in current set not in snapshot $\to$ `added_count`.
   - Files in snapshot not in current set $\to$ `removed_count`.
   - Files with differing `(mtime, len)` metadata $\to$ check hash; if differing $\to$ `modified_count`.
3. Total drift = `modified_count + added_count + removed_count`.
4. `is_stale = total_drift >= 10 || (cached_files_count > 0 && (total_drift as f64 / cached_files_count as f64) >= 0.20)`.

This check completes in $<5$ms on repos with 1,000 files because it only checks file metadata (`mtime` / `len`) before computing cryptographic hashes.

---

## 7. Testing & Verification Strategy

1. **Unit Tests:**
   - `test_drift_detector_clean_repo`: Asserts zero drift on unchanged repo.
   - `test_drift_detector_stale_threshold`: Asserts `is_stale` triggers when $>10$ files change.
   - `test_intent_classifier_crud_vs_general_query`: Asserts clear separation between general programming questions and repository-modifying prompts.
2. **Integration Tests (`tests/integration_jit_onboarding.rs`):**
   - Verify unindexed repo launches with `initial_modal == ModalState::None`.
   - Verify general query runs without triggering `WorkspaceAnalysis`.
   - Verify code edit prompt on unindexed repo triggers `WorkspaceAnalysis`.
   - Verify unconfigured provider intercepts prompt, preserves prompt text, and resumes execution upon key entry.
   - Verify drifted repo triggers `WorkspaceDrift` on CRUD prompts, while fresh repo runs immediately.
3. **Quality Gates:**
   - `cargo check -j 1`
   - `cargo test -j 1` (targeted test suites)
   - `cargo clippy -j 1 -- -D warnings`
   - `cargo fmt --check`
   - Recompile release binary via `./localupdate.sh`.
