# Task 2 Brief: API Key Paste Box & Masked Input Primitive

## Objective
Implement API key masking, bracketed paste input handling, and text prompt primitives in `src/ui/setup/input.rs`.

## Files to Create / Modify
- Create: `src/ui/setup/input.rs`
- Modify: `src/ui/setup/mod.rs` (export `pub mod input;` and re-export `mask_api_key`, `prompt_api_key`, `prompt_text`)
- Test: `src/ui/setup/input.rs` (inline unit tests)

## Constraints & Requirements
1. **Compilation Concurrency:** ONLY run `cargo check -j 1` and `cargo test -j 1`.
2. **Targeted Test Execution:** ONLY run `cargo test -j 1 --lib ui::setup::input::tests`. NEVER run the full test suite.
3. **Zero Unwraps:** No `.unwrap()` or `.expect()` in non-test code. Return `std::io::Result`.
4. **API Key Masking:**
   ```rust
   pub fn mask_api_key(key: &str) -> String
   ```
   - If empty: return `""`.
   - If length <= 10: return bullets for each char (e.g. `•`.repeat(len)).
   - If length > 10: take first 6 chars (e.g. `sk-min`), append `••••`, and take last 4 chars (e.g. `sk-min••••6789`).
5. **Interactive API Key Prompt:**
   ```rust
   pub fn prompt_api_key(provider_name: &str, current_key: Option<&str>) -> io::Result<Option<String>>
   ```
   - Uses `TerminalGuard` to ensure raw mode.
   - Renders a styled input card:
     ```text
     ┌─ Configure {provider_name} API Key ───────────────────────────────────────────┐
     │ Current: {masked_current}                                                     │
     │ Paste key: {bullets} ({len} chars)                                            │
     │                                                                               │
     │ [Enter] Save & Set as Active    [Esc] Cancel                                  │
     └───────────────────────────────────────────────────────────────────────────────┘
     ```
   - Event Handling:
     - `Event::Paste(pasted)`: Appends pasted string to buffer.
     - `Event::Key`:
       - Skip `KeyEventKind::Release`.
       - `Ctrl+C` (case-insensitive): clear prompt block and return `Ok(None)`.
       - `Esc`: clear prompt block and return `Ok(None)`.
       - `Backspace`: pop last char.
       - `Char(c)`: append char.
       - `Enter`:
         - If buffer is not empty: trim whitespace, clear prompt block, return `Ok(Some(trimmed))`.
         - If buffer is empty and `current_key` is present: clear prompt block, return `Ok(Some(current_key.to_string()))`.
         - If buffer is empty and no current key: clear prompt block, return `Ok(None)`.
   - Redraw loop: rewrite in place with `\x1b[{}A` and `\x1b[2K\r`.
   - Teardown: erase prompt lines so the screen stays clean.
6. **Generic Text Prompt:**
   ```rust
   pub fn prompt_text(prompt_label: &str, default_value: Option<&str>, allow_empty: bool) -> io::Result<Option<String>>
   ```
   - For custom provider identifier name and base URL.
   - Handles typing, backspace, paste, Enter, Esc, Ctrl+C.
7. **Code Quality:**
   - `cargo fmt`
   - `cargo clippy -j 1 --bin minicode -- -D warnings`
8. **Commit:**
   - `feat(ui): implement masked API key input and text prompt primitives`
