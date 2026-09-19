# Modern Interactive Setup Wizard (`minicode setup`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform `minicode configure` into a streamlined, modern interactive wizard (`minicode setup`, with `configure` alias) featuring raw-mode arrow navigation (`↑`/`↓`/`j`/`k`), `Enter` selection, `Esc` back navigation, and a focused Provider configuration workflow.

**Architecture:** A modular inline interactive selector engine in `src/ui/setup/` utilizing `crossterm` raw mode, bracketed paste, and RAII terminal guards. The menu hierarchy isolates Provider onboarding (Available Providers, Custom Provider) with live configuration status badges, inline API key paste input, and automatic configuration persistence to `config.toml` and `.env`.

**Tech Stack:** Rust 2021, `crossterm` 0.28, `ratatui` 0.29, `tokio`, `toml`, `dotenvy`.

## Global Constraints

- Use `-j 1` on `cargo check` and `cargo test`.
- Use `-j 2` on `cargo build --release`.
- Only run targeted tests: never run full test suite across the entire project.
- Error handling: zero `.unwrap()` or `.expect()` in non-test code.
- Pure Rust, zero OS-level OpenSSL dependencies.
- Never use `println!` or `eprintln!` inside library code when raw mode is active without restoring the terminal.
- Maintain backwards compatibility: `minicode configure` and `minicode config` must continue to dispatch identically to `minicode setup`.

---

### Task 1: Terminal Raw Mode Guard & Interactive List Selector Primitives

**Files:**
- Create: `src/ui/setup/guard.rs`
- Create: `src/ui/setup/selector.rs`
- Create: `src/ui/setup/mod.rs`
- Test: `src/ui/setup/selector.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `crossterm::terminal::{enable_raw_mode, disable_raw_mode}`, `crossterm::event::{read, Event, KeyCode, KeyEvent, KeyModifiers}`
- Produces:
  - `pub struct TerminalGuard;` — RAII guard for raw mode, cursor hiding, and bracketed paste.
  - `pub struct SelectorItem { pub id: String, pub label: String, pub badge: Option<String>, pub hint: Option<String> }`
  - `pub struct InteractiveSelector { ... }`
  - `InteractiveSelector::select(&self, prompt: &str, items: &[SelectorItem], initial_index: usize) -> io::Result<Option<usize>>`

- [ ] **Step 1: Write unit tests for selector index navigation**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_item_builder() {
        let item = SelectorItem::new("provider", "⚡ Provider")
            .with_badge("Active")
            .with_hint("Configure LLM providers");
        assert_eq!(item.id, "provider");
        assert_eq!(item.label, "⚡ Provider");
        assert_eq!(item.badge.as_deref(), Some("Active"));
    }

    #[test]
    fn test_navigation_wrap_and_bounds() {
        assert_eq!(InteractiveSelector::prev_index(0, 3), 2);
        assert_eq!(InteractiveSelector::prev_index(1, 3), 0);
        assert_eq!(InteractiveSelector::next_index(2, 3), 0);
        assert_eq!(InteractiveSelector::next_index(1, 3), 2);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 --lib ui::setup::selector::tests`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement `TerminalGuard` and `InteractiveSelector`**

In `src/ui/setup/guard.rs`:
```rust
use std::io::{self, stdout, Write};

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            stdout(),
            crossterm::cursor::Hide,
            crossterm::event::EnableBracketedPaste
        )?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            stdout(),
            crossterm::cursor::Show,
            crossterm::event::DisableBracketedPaste
        );
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = stdout().flush();
    }
}
```

In `src/ui/setup/selector.rs`:
Implement `SelectorItem`, `InteractiveSelector` with arrow navigation, `k`/`j` support, ANSI cursor positioning (`\x1b[2K\r`), highlight cursor `❯ `, and `Esc` handling.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -j 1 --lib ui::setup::selector::tests`
Expected: PASS

- [ ] **Step 5: Verify formatting and clippy**

Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: PASS with zero warnings

- [ ] **Step 6: Commit**

```bash
git add src/ui/setup/
git commit -m "feat(ui): implement TerminalGuard and InteractiveSelector primitives for setup wizard"
```

---

### Task 2: API Key Paste Box & Masked Input Primitive

**Files:**
- Create: `src/ui/setup/input.rs`
- Modify: `src/ui/setup/mod.rs`
- Test: `src/ui/setup/input.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `TerminalGuard`, `crossterm::event::{read, Event, KeyCode, KeyModifiers}`
- Produces:
  - `pub fn mask_api_key(key: &str) -> String`
  - `pub fn prompt_api_key(provider_name: &str, current_key: Option<&str>) -> io::Result<Option<String>>`
  - `pub fn prompt_text(prompt_label: &str, default_value: Option<&str>, allow_empty: bool) -> io::Result<Option<String>>`

- [ ] **Step 1: Write unit tests for API key masking**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("short"), "•••••");
        assert_eq!(mask_api_key("sk-minimax-123456789"), "sk-min••••6789");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 --lib ui::setup::input::tests`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement `mask_api_key` and interactive prompt functions**

In `src/ui/setup/input.rs`:
- Implement `mask_api_key`: For keys > 12 chars, preserve first 7 chars (e.g. `sk-xxxx`) and last 4 chars, masking the middle with `••••`. For short keys, mask completely.
- Implement `prompt_api_key`: Reads input in raw mode, supports bracketed paste events (pasting long API keys directly), `Backspace`, `Enter` to confirm, and `Esc` to return `None`. Shows masked bullets with a key length counter.
- Implement `prompt_text`: Generic single-line prompt for custom provider name and base URL.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -j 1 --lib ui::setup::input::tests`
Expected: PASS

- [ ] **Step 5: Verify formatting and clippy**

Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: PASS with zero warnings

- [ ] **Step 6: Commit**

```bash
git add src/ui/setup/
git commit -m "feat(ui): implement masked API key input and text prompt primitives"
```

---

### Task 3: Provider Workflow & Interactive Menu Hierarchy

**Files:**
- Create: `src/ui/setup/wizard.rs`
- Modify: `src/ui/setup/mod.rs`
- Modify: `src/ui/configure.rs` (delegate to `SetupWizard`)
- Test: `src/ui/setup/wizard.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `InteractiveSelector`, `TerminalGuard`, `prompt_api_key`, `prompt_text`, `Config`, `save_all`
- Produces:
  - `pub struct SetupWizard;`
  - `SetupWizard::run(workspace: &Path) -> Result<()>`
  - `SetupWizard::menu_providers(config: &mut Config, workspace: &Path) -> Result<()>`
  - `SetupWizard::menu_available_providers(config: &mut Config, workspace: &Path) -> Result<()>`
  - `SetupWizard::menu_custom_provider(config: &mut Config, workspace: &Path) -> Result<()>`

- [ ] **Step 1: Write unit tests for provider catalog list mapping**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_catalog_contains_standard_providers() {
        let catalog = SetupWizard::provider_catalog();
        let ids: Vec<&str> = catalog.iter().map(|(id, _, _)| *id).collect();
        assert!(ids.contains(&"minimax"));
        assert!(ids.contains(&"z.ai"));
        assert!(ids.contains(&"openrouter"));
        assert!(ids.contains(&"gemini"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"deepseek"));
        assert!(ids.contains(&"groq"));
        assert!(ids.contains(&"ollama"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 --lib ui::setup::wizard::tests`
Expected: FAIL

- [ ] **Step 3: Implement `SetupWizard`**

In `src/ui/setup/wizard.rs`:
- Check `std::io::stdin().is_terminal()`; return clean error if non-interactive.
- Implement Level 1 Main Menu:
  - `[1] ⚡ Provider`
  - `[2] ◄ Back / Exit`
- Implement Level 2 Provider Menu:
  - `[1] 🌐 Available Providers`
  - `[2] 🔌 Custom Provider`
  - `[0] ◄ Back`
- Implement Level 3 Available Providers Menu:
  - Iterate catalog of 10 providers.
  - Compute badges:
    - If `config.provider.default == id` -> `● Active`
    - Else if key exists -> `✔ Configured (sk-••••)`
    - Else if `ollama` -> `○ Localhost`
    - Else -> `○ Not Set`
  - On select:
    - If NOT configured (and not ollama): call `prompt_api_key`. If entered, save key, set `config.provider.default = id`, call `ConfigMenu::save_all`, print success banner.
    - If ALREADY configured: prompt 2-choice selector:
      - `⚡ Set as Active Provider` -> set default, save, print success.
      - `🔑 Reconfigure API Key` -> prompt new key, save, print success.
      - `◄ Back` -> return to list.
- Implement Level 3 Custom Provider Menu:
  - Prompt name, base URL, optional key -> save and set as active.
- Wire `ConfigMenu::run_interactive` in `src/ui/configure.rs` to call `SetupWizard::run(workspace)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -j 1 --lib ui::setup::wizard::tests`
Expected: PASS

- [ ] **Step 5: Verify formatting and clippy**

Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: PASS with zero warnings

- [ ] **Step 6: Commit**

```bash
git add src/ui/setup/ src/ui/configure.rs
git commit -m "feat(ui): implement modern hierarchical interactive setup wizard"
```

---

### Task 4: CLI Alignment (`minicode setup`), Alias Wiring & Integration Tests

**Files:**
- Modify: `src/main.rs:125-131` (rename `Configure` -> `Setup`, add aliases `configure`, `config`)
- Modify: `src/main.rs:298-301` (dispatch `Commands::Setup`)
- Modify: `src/main.rs` (update user tip strings from `minicode configure` to `minicode setup`)
- Create: `tests/integration_setup_wizard.rs`

**Interfaces:**
- Consumes: `Commands::Setup`, `SetupWizard::run`
- Produces:
  - Working CLI commands: `minicode setup`, `minicode configure`, `minicode config`

- [ ] **Step 1: Write integration tests for CLI setup command dispatch**

In `tests/integration_setup_wizard.rs`:
```rust
use std::process::Command;

#[test]
fn test_setup_help_mentions_setup_and_aliases() {
    let output = Command::new(env!("CARGO_BIN_EXE_minicode"))
        .args(["setup", "--help"])
        .output()
        .expect("runs setup --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Interactive configuration wizard") || stdout.contains("Setup"));
}

#[test]
fn test_configure_alias_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_minicode"))
        .args(["configure", "--help"])
        .output()
        .expect("runs configure --help");
    assert!(output.status.success());
}
```

- [ ] **Step 2: Run test to verify it fails or needs updates**

Run: `cargo test -j 1 --test integration_setup_wizard`

- [ ] **Step 3: Update `src/main.rs` with `Setup` command**

In `src/main.rs`:
```rust
    /// Interactive configuration wizard (setup active provider, models, and API keys)
    #[command(alias = "configure", alias = "config")]
    Setup,
```
Update dispatch:
```rust
    if let Some(Commands::Setup) = cli.command {
        ui::setup::SetupWizard::run(&workspace_canonical).await?;
        return Ok(());
    }
```
Update startup and error tip strings across `src/main.rs` to display `minicode setup`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -j 1 --test integration_setup_wizard`
Expected: PASS

- [ ] **Step 5: Verify formatting and clippy**

Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: PASS with zero warnings

- [ ] **Step 6: Commit**

```bash
git add src/main.rs tests/integration_setup_wizard.rs
git commit -m "feat(cli): promote minicode setup as primary configuration command with configure alias"
```

---

### Task 5: Quality Gates, Version Bump (v0.3.36), Release Build & Real-World Validation

**Files:**
- Modify: `Cargo.toml` (bump version to `0.3.36`)
- Modify: `onpkg_docs/todo.md` (add Phase 135 checklist)
- Script: `./localupdate.sh`

- [ ] **Step 1: Bump version in `Cargo.toml`**

Change `version = "0.3.35"` to `version = "0.3.36"`.

- [ ] **Step 2: Update `onpkg_docs/todo.md`**

Update `Current Phase` to `Phase 135 (v0.3.36) Modern Interactive Setup Wizard (minicode setup)`.
Add Phase 135 completion checklist items.

- [ ] **Step 3: Verify formatting and clippy**

Run: `cargo fmt --check && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: PASS with zero warnings

- [ ] **Step 4: Recompile release binary and install globally**

Run: `./localupdate.sh`
Expected: `minicode v0.3.36` compiled and installed to `~/.local/bin/minicode`.

- [ ] **Step 5: Run real-world CLI smoke check**

Run: `minicode setup --help` and `minicode --version`
Verify both `minicode setup` and `minicode configure` are responsive.

- [ ] **Step 6: Commit release**

```bash
git add Cargo.toml Cargo.lock onpkg_docs/todo.md
git commit -m "release(v0.3.36): deliver modern interactive setup wizard with arrow navigation (Phase 135)"
```
