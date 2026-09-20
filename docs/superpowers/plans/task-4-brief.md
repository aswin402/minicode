# Task 4 Brief: Interactive In-TUI `/settings` Command Modal & Permission Confirmation Card

## Requirements
1. **Create `src/ui/modals/settings.rs`:**
   - Define `SettingsTab` enum with:
     - `Providers`: Lists supported providers and their configured default models
     - `Workspace`: Displays workspace root path, assigned provider & model, and scope toggle ("Workspace only" vs "Globally")
     - `Autonomy`: Configures `auto_approve`, `thinking_budget` (0, 4096, 8192, 16384, 32768), and `approval_policy`
     - `Probes`: Action to probe all configured providers, displaying latency and connection status
   - Define `SettingsModalState` struct with fields:
     - `active_tab`: `SettingsTab`
     - `selected_index`: `usize`
     - `active_provider`: `String`
     - `active_model`: `String`
     - `workspace_path`: `String`
     - `save_to_workspace`: `bool`
     - `auto_approve`: `bool`
     - `thinking_budget`: `usize`
     - `approval_policy`: `String`
     - `probe_results`: `Option<Vec<crate::tools::registry::agent_tools::config_tools::ConnectionTestResult>>`
     - `probing`: `bool`
   - Implement `SettingsModalState::from_config(config: &Config, workspace_root: &Path) -> Self`
   - Implement rendering:
     - `pub fn render_settings(frame: &mut Frame, area: Rect, theme: &Theme, state: &SettingsModalState)`:
       Renders tab bar at top, clean border with title "⚙️ minicode settings", tab content area, and keyboard helper footer (`[Tab] Next Tab  [↑/↓] Navigate  [Enter] Select/Toggle  [Esc] Save & Close`).
     - `pub fn render_config_approval(frame: &mut Frame, area: Rect, theme: &Theme, proposal: &crate::tools::registry::agent_tools::config_tools::ConfigChangeProposal, selected_index: usize)`:
       Renders confirmation card:
       ```text
       ⚙️ minicode requests permission to modify configuration:
          • Active Provider : ollama → anthropic
          • Active Model    : qwen2.5-coder → claude-3-7-sonnet-20250219
          • Scope           : Workspace (.minicode/config.toml)
       [1] Allow Change   [2] Deny Change
       ```

2. **Update `src/ui/modals/mod.rs`:**
   - Add `pub mod settings;`
   - Add variants to `ModalState`:
     ```rust
     Settings(settings::SettingsModalState),
     ConfigApproval {
         proposal: crate::tools::registry::agent_tools::config_tools::ConfigChangeProposal,
         selected_index: usize,
     },
     ```
   - Add helper methods:
     - `pub fn new_settings(config: &Config, workspace_root: &Path) -> Self`
     - `pub fn new_config_approval(proposal: crate::tools::registry::agent_tools::config_tools::ConfigChangeProposal) -> Self`
   - In `ModalState::render`: dispatch to `settings::render_settings` and `settings::render_config_approval`.

3. **Update `src/app/modals.rs`:**
   - In `handle_modal_key`:
     - Handle `ModalState::Settings`:
       - `Tab` / `BackTab`: Cycle through tabs (`Providers` -> `Workspace` -> `Autonomy` -> `Probes`). Reset `selected_index = 0`.
       - `Up` / `Down` / `k` / `j`: Move selection within active tab.
       - `Enter` / `Space`:
         - On `Providers`: switch provider default model or open selection.
         - On `Workspace`: toggle `save_to_workspace`.
         - On `Autonomy`: toggle `auto_approve` or cycle `thinking_budget` (0 -> 4096 -> 8192 -> 16384 -> 32768 -> 0).
         - On `Probes`: execute provider connection probes (calls `test_provider_connection`).
       - `Esc` / `q`: Apply pending settings modifications to `self.config`, call `self.config.save(...)`, update workspace preference if changed, and set `self.modal = ModalState::None`.
     - Handle `ModalState::ConfigApproval`:
       - `Left` / `Right` / `Tab`: toggle `selected_index` (0 vs 1).
       - `1` or `Enter` when `selected_index == 0`:
         Apply proposal using `crate::tools::registry::agent_tools::config_tools::apply_proposal(&proposal, &self.workspace_root)`.
         Update `self.config` in-memory.
         Add success status to `self.timeline`.
         Close modal (`self.modal = ModalState::None`).
       - `2` or `Esc` or `Enter` when `selected_index == 1`:
         Reject proposal.
         Add status to `self.timeline`: `"Config change proposal declined."`.
         Close modal (`self.modal = ModalState::None`).

4. **Update `src/app/commands.rs`:**
   - Handle `/settings`, `/config`, `/preferences`:
     - If args provided:
       - `/settings model <provider> <model>`:
         Insert into `self.config.provider.default_models`, save config, add confirmation to timeline.
       - `/settings auto_approve <on|off|true|false>`:
         Update `self.config.agent.auto_approve`, save config, add confirmation to timeline.
       - `/settings thinking <tokens>`:
         Parse tokens, update `self.config.agent.thinking_budget`, save config, add confirmation to timeline.
     - Else:
       - `self.modal = ModalState::new_settings(&self.config, &self.workspace_root);`
     - Return `Ok(CommandAction::Continue);`

5. **Follow TDD:**
   - Add unit tests in `src/ui/modals/settings.rs`:
     - Test `SettingsModalState` initialization and tab switching.
     - Test `SettingsModalState` rendering with `ratatui::backend::TestBackend`.
     - Test `ConfigApproval` rendering with `ratatui::backend::TestBackend`.
   - Targeted tests:
     `cargo test -j 1 --lib ui::modals::settings::tests`
     `cargo test -j 1 --lib ui::modals::tests`
   - Quality checks:
     `cargo fmt --check`
     `cargo clippy -j 1 --bin minicode -- -D warnings`

6. **Commit message:**
   `feat(ui): implement interactive in-TUI /settings modal and config permission card`
