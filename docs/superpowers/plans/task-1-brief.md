# Task 1 Brief: Data Structures and Configuration for Intent Tracking

## Goal
Define the data structures (`RequirementStatus`, `RequirementItem`, `IntentLedger`) and configuration options (`IntentConfig`) for intent anchoring and execution tracking in minicode.

## Files to Touch
- Create: `src/context/memory/intent.rs`
- Modify: `src/context/memory/mod.rs`
- Modify: `src/config.rs`
- Modify: `src/constants.rs`
- Test: `src/context/memory/intent.rs` (inline `mod tests`)

## Detailed Requirements

### 1. `src/constants.rs`
Add:
```rust
pub const DEFAULT_INTENT_PERSISTENCE_FILE: &str = ".minicode/intent_anchor.json";
pub const DEFAULT_INTENT_DRIFT_WARNING_TURNS: usize = 4;
pub const DEFAULT_INTENT_MAX_ITEMS: usize = 32;
```

### 2. `src/config.rs`
Add `IntentConfig`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_intent_persistence_file")]
    pub persistence_file: String,
    #[serde(default = "default_intent_drift_warning_turns")]
    pub drift_warning_turns: usize,
    #[serde(default = "default_true")]
    pub auto_extract: bool,
    #[serde(default = "default_intent_max_items")]
    pub max_ledger_items: usize,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            persistence_file: default_intent_persistence_file(),
            drift_warning_turns: default_intent_drift_warning_turns(),
            auto_extract: true,
            max_ledger_items: default_intent_max_items(),
        }
    }
}
```
Include `intent: IntentConfig` in `AgentConfig`, `RawAgentConfig`, and config merging/defaults.

### 3. `src/context/memory/intent.rs`
Implement:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequirementItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: RequirementStatus,
    #[serde(default)]
    pub related_files: Vec<String>,
    #[serde(default)]
    pub created_turn: usize,
    #[serde(default)]
    pub updated_turn: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IntentLedger {
    pub root_objective: String,
    pub items: Vec<RequirementItem>,
    pub active_item_id: Option<String>,
    pub consecutive_turns_without_progress: usize,
}
```
Methods on `IntentLedger`:
- `new(root_objective: &str) -> Self`
- `add_item(&mut self, title: &str, description: Option<&str>, related_files: Vec<String>) -> String` (returns item id)
- `set_status(&mut self, id: &str, status: RequirementStatus) -> bool` (updates item status, updates `active_item_id`, resets drift counter if status moved to `Completed` or `InProgress`)
- `get_item(&self, id: &str) -> Option<&RequirementItem>`
- `save_to_disk(&self, path: &std::path::Path) -> std::io::Result<()>` (atomic write using `.tmp` file and rename)
- `load_from_disk(path: &std::path::Path) -> std::io::Result<Self>`

### 4. Re-export in `src/context/memory/mod.rs`
Add `pub mod intent;` and `pub use intent::*;`.

### 5. Verification Constraints
- Strict: ONLY run targeted test: `cargo test -j 1 --lib context::memory::intent::tests`
- Zero clippy warnings, zero non-test unwraps, clean formatting.
