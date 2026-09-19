# Task 4 Brief: CLI Alignment (`minicode setup`), Alias Wiring & Integration Tests

## Objective
Promote `minicode setup` as the primary configuration command with `configure` and `config` aliases, update user-facing tip strings across `src/main.rs`, and implement an end-to-end integration test suite in `tests/integration_setup_wizard.rs`.

## Files to Create / Modify
- Modify: `src/main.rs` (update `Commands::Configure` to `Commands::Setup` with aliases, update dispatch and tips)
- Create: `tests/integration_setup_wizard.rs` (integration tests)

## Constraints & Requirements
1. **Compilation Concurrency:** ONLY run `cargo check -j 1` and `cargo test -j 1`.
2. **Targeted Test Execution:** ONLY run `cargo test -j 1 --test integration_setup_wizard`. NEVER run the full test suite.
3. **Zero Unwraps:** No `.unwrap()` or `.expect()` in non-test code.
4. **Command Definition & Dispatch in `src/main.rs`:**
   ```rust
   /// Interactive configuration wizard (setup active provider, models, and API keys)
   #[command(alias = "configure", alias = "config")]
   Setup,
   ```
   Dispatch:
   ```rust
   if let Some(Commands::Setup) = cli.command {
       ui::setup::SetupWizard::run(&workspace_canonical).await?;
       return Ok(());
   }
   ```
5. **Tip Strings Alignment in `src/main.rs`:**
   - Update startup and error tips mentioning `minicode configure` to `minicode setup` (e.g. lines ~665, ~823, ~1088).
6. **Integration Test Suite (`tests/integration_setup_wizard.rs`):**
   - Test 1: `test_setup_help_command` runs `minicode setup --help` and asserts success.
   - Test 2: `test_configure_alias_help` runs `minicode configure --help` and asserts success.
   - Test 3: `test_config_alias_help` runs `minicode config --help` and asserts success.
   - Test 4: `test_setup_non_interactive_fails_gracefully` runs `minicode setup` with `.stdin(Stdio::null())` and asserts failure with message explaining interactive terminal is required (exit code != 0, no panic or hang).
   - Test 5: `test_setup_wizard_provider_catalog_completeness` imports and asserts `SetupWizard::provider_catalog()` has all 10 canonical providers and valid env var mappings.
7. **Code Quality:**
   - `cargo fmt`
   - `cargo clippy -j 1 --bin minicode -- -D warnings`
8. **Commit:**
   - `feat(cli): promote minicode setup as primary configuration command with configure alias`
