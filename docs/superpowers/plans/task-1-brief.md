# Task 1 Brief: Universal API Key Masking & Dual-Layer Workspace Registry (`workspaces.toml`)

## Requirements
1. Implement `mask_api_key(key: &str) -> String` in `src/config.rs`:
   - Empty/whitespace -> `""`
   - `<= 8` chars -> `"••••••••"`
   - `> 8` chars -> first 4 chars + `"..."` + last 4 chars (e.g. `"sk-a...cdef"`)
2. Add constant in `src/constants.rs`:
   - `pub const WORKSPACES_FILE_NAME: &str = "workspaces.toml";`
3. Implement `WorkspacePreference` and `WorkspaceRegistry` in `src/config.rs`:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
   pub struct WorkspacePreference {
       pub provider: String,
       pub model: String,
       #[serde(default)]
       pub last_used: String,
   }

   #[derive(Debug, Clone, Serialize, Deserialize, Default)]
   pub struct WorkspaceRegistry {
       #[serde(default)]
       pub workspaces: std::collections::HashMap<String, WorkspacePreference>,
   }
   ```
4. Implement persistence helpers:
   - `load_workspace_preference_from_file(workspace_root: &Path, registry_path: &Path) -> Option<WorkspacePreference>`
   - `save_workspace_preference_to_file(workspace_root: &Path, provider: &str, model: &str, registry_path: &Path) -> anyhow::Result<()>`
   - `Config::get_workspace_registry_path() -> Option<PathBuf>`
   - `Config::load_workspace_preference(workspace_root: &Path) -> Option<WorkspacePreference>`
   - `Config::save_workspace_preference(workspace_root: &Path, provider: &str, model: &str) -> anyhow::Result<()>`
5. Follow TDD:
   - Add unit tests `test_mask_api_key_variations` and `test_workspace_preference_roundtrip` in `src/config.rs`.
   - Run targeted tests:
     `cargo test -j 1 --lib config::tests::test_mask_api_key_variations`
     `cargo test -j 1 --lib config::tests::test_workspace_preference_roundtrip`
   - Run formatting: `cargo fmt`
   - Run clippy: `cargo clippy -j 1 --bin minicode -- -D warnings`
6. Commit with message: `feat(config): implement universal API key masking and dual-layer workspace registry`
