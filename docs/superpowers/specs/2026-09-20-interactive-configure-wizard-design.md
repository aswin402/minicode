# Design Specification: Modern Interactive Configuration Wizard for `minicode configure`

- **Date:** 2026-09-20
- **Author:** Antigravity & Pair Programming Engineer
- **Status:** Approved by User / Ready for Implementation Planning
- **Target Version:** v0.3.36 (Phase 135)

---

## 1. Executive Summary & Problem Statement

Currently, executing `minicode configure` displays a monolithic 6-option numbered CLI wizard requiring manual numeric typing (`1`, `2`, `3` ... `Enter`) to navigate. It couples disparate concerns (API key entry, provider switching, tool approval policies, model fetching, and status bar toggles) into a single flat menu.

This specification modernizes `minicode configure` into an **inline interactive terminal wizard** powered by `crossterm` raw-mode event processing with arrow-key navigation (`↑`/`↓`/`k`/`j`), single-keystroke selection (`Enter`), cancellation (`Esc` / `◄ Back`), and streamlined provider onboarding.

---

## 2. User Experience & Navigation Architecture

### 2.1 Control Scheme
- **`↑` / `k`**: Move selection highlight up.
- **`↓` / `j`**: Move selection highlight down.
- **`Enter` (`↵`)**: Select highlighted item and transition to next view.
- **`Esc`**: Return to previous menu level (or cancel active prompt).
- **`Ctrl+C`**: Immediately exit and cleanly restore the terminal.

### 2.2 Visual Hierarchy

```text
Level 1: Main Menu
  ├── ⚡ Provider (Setup & Manage Providers)
  └── ◄ Back / Exit (Save & return to shell)

Level 2: Provider Submenu
  ├── 🌐 Available Providers (Pre-configured catalog)
  ├── 🔌 Custom Provider (OpenAI-compatible endpoint)
  └── ◄ Back

Level 3A: Available Providers List
  ├── OpenRouter         ○ Not Set
  ├── Google Gemini      ○ Not Set
  ├── OpenAI             ○ Not Set
  ├── DeepSeek           ○ Not Set
  ├── Groq               ○ Not Set
  ├── MiniMax            ● Active       (sk-mini••••)
  ├── Z.ai / Zhipu GLM   ✔ Configured   (sk-zhip••••)
  ├── Together AI        ○ Not Set
  ├── Mistral AI         ○ Not Set
  ├── Ollama (Local)     ○ Localhost
  └── ◄ Back

Level 3B: Provider Selection Action
  ├── Case 1: Provider NOT Configured
  │     └── Direct Key Input Modal (Paste key -> Enter saves & activates)
  │
  └── Case 2: Provider ALREADY Configured
        ├── ⚡ Set as Active Provider
        ├── 🔑 Reconfigure API Key (Paste new key)
        └── ◄ Back

Level 3C: Custom Provider
  ├── Provider Name Input (e.g. 'vllm-local')
  ├── Base URL Input (e.g. 'http://localhost:8000/v1')
  ├── API Key Input (optional)
  └── ◄ Back
```

---

## 3. Detailed Component Specifications

### 3.1 Terminal RAII Raw-Mode Guard
To prevent terminal corruption or leaving the user's shell in a broken state, an RAII guard manages terminal state:
```rust
pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            io::stdout(),
            crossterm::cursor::Hide,
            crossterm::event::EnableBracketedPaste
        )?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            io::stdout(),
            crossterm::cursor::Show,
            crossterm::event::DisableBracketedPaste
        );
        let _ = crossterm::terminal::disable_raw_mode();
    }
}
```

### 3.2 Inline Interactive Selector Engine
An inline selector renders a fixed number of rows using ANSI escape sequences (`\x1b[2K\r` to clear line, cursor up/down) without clearing the scrollback buffer:
- **Highlighted Line**: Bold accent color (`Cyan`/`Purple`), leading pointer `❯ `, inverted or distinct background/foreground styling.
- **Unselected Lines**: Dim or regular text, leading spaces `  `.
- **Status Badges**:
  - `● Active`: Green/Cyan bold with active indicator.
  - `✔ Configured`: Green badge with masked key suffix `(sk-••••)`.
  - `○ Not Set`: Muted gray.
  - `○ Localhost`: Muted yellow (no key required).
- **Footer**: `↑/↓ Navigate • ↵ Select • Esc Back`.

### 3.3 Key Pasting & Masked Input Box
When prompting for an API key:
- Raw mode accepts typed characters and bracketed paste chunks.
- Characters can be toggled between masked bullets (`•`) or visible characters.
- Supports `Backspace` to delete, `Enter` to confirm, and `Esc` to cancel.
- Trims whitespace automatically before storing.

### 3.4 Persistence & Synchronization
Once a key is provided or active provider is switched:
1. `config.provider.default` is set to the selected provider name.
2. `config.provider.api_keys` is updated with the new key.
3. Automatically synchronized to:
   - Global config: `~/.config/minicode/config.toml`
   - Global env: `~/.config/minicode/.env`
   - Workspace env: `<workspace>/.env` (if applicable)
4. Displays a clean confirmation receipt before returning.

---

## 4. Error Handling & Edge Cases

1. **Non-TTY Environments (Piped Stdin / CI)**:
   - Detects `!std::io::stdin().is_terminal()`.
   - In non-TTY environments, gracefully prints an informative error message explaining that `minicode configure` requires an interactive terminal, rather than crashing or freezing.
2. **Empty Input on Key Paste**:
   - If user confirms an empty string on a non-configured provider, alerts with an inline warning: `API key cannot be empty. Press Esc to cancel.`
3. **Special Case: Local Providers (Ollama)**:
   - For `ollama`, does not require an API key. Selecting it immediately offers to set as active or test connection (`http://localhost:11434/v1`).
4. **Ctrl+C Interrupts**:
   - Immediate interception of `Event::Key(KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL })` cleanly drops the `TerminalGuard`, restores cursor, prints newline, and exits with code 0.

---

## 5. Verification Plan

1. **Unit Testing**:
   - Helper test for key masking: verifies keys with various lengths produce correctly truncated strings (`sk-mini••••`).
   - Provider catalog completeness test: verifies all 10 built-in providers and env mappings exist.
2. **Clippy & Formatting**:
   - `cargo clippy -j 1 --bin minicode -- -D warnings`
   - `cargo fmt --check`
3. **Interactive Verification**:
   - Run `minicode configure` in a real terminal session.
   - Test `↑`/`↓`/`j`/`k` navigation.
   - Configure a new provider (e.g. DeepSeek), paste key, verify `.env` and `config.toml` update.
   - Re-select already configured provider (e.g. MiniMax), verify the 2-choice sub-prompt works.
   - Test `Esc` and `◄ Back` across all levels.
