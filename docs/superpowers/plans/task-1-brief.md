# Task 1 Brief: Terminal Raw Mode Guard & Interactive List Selector Primitives

## Objective
Implement `TerminalGuard` (RAII terminal raw mode, cursor visibility, bracketed paste) and `InteractiveSelector` (inline arrow-key navigation, `k`/`j` keys, `Enter` selection, `Esc` back/cancel) in `src/ui/setup/`.

## Files to Create / Modify
- Create: `src/ui/setup/guard.rs`
- Create: `src/ui/setup/selector.rs`
- Create: `src/ui/setup/mod.rs`
- Modify: `src/ui/mod.rs` (expose `pub mod setup;`)
- Tests: `src/ui/setup/selector.rs` (inline unit tests)

## Constraints & Requirements
1. **Compilation Concurrency:** ONLY run `cargo check -j 1` and `cargo test -j 1`.
2. **Targeted Test Execution:** ONLY run `cargo test -j 1 --lib ui::setup::selector::tests`. NEVER run the full test suite.
3. **Zero Unwraps:** No `.unwrap()` or `.expect()` in non-test code. Propagate `io::Result<T>`.
4. **Terminal Safety:** `TerminalGuard` MUST implement `Drop` to ensure `disable_raw_mode()` and `cursor::Show` are always called, preventing broken terminal states.
5. **Interactive Selector UX:**
   - Input keys:
     - `KeyCode::Up` / `KeyCode::Char('k')` -> `prev_index(current, total)`
     - `KeyCode::Down` / `KeyCode::Char('j')` -> `next_index(current, total)`
     - `KeyCode::Enter` -> `Ok(Some(current_index))`
     - `KeyCode::Esc` -> `Ok(None)`
     - `KeyCode::Char('c')` with `KeyModifiers::CONTROL` -> `Ok(None)` or graceful exit
   - Rendering:
     - Draw header / prompt: `\x1b[1m<prompt>\x1b[0m`
     - Highlight line: `\x1b[1;36m  ❯ \x1b[0m\x1b[1m{label}\x1b[0m {badge} \x1b[90m{hint}\x1b[0m`
     - Normal line: `    {label} {badge} \x1b[90m{hint}\x1b[0m`
     - Footer: `\x1b[90m  ──────────────────────────────────────────────────────────\x1b[0m\n  \x1b[90m↑/↓ Navigate • ↵ Select • Esc Back\x1b[0m`
     - Redraw loop: Move cursor up by line count (`\x1b[{}A`), clear line (`\x1b[2K\r`), and redraw.
     - On completion: Clear drawn lines so the next screen draws cleanly.
   - Index helpers:
     ```rust
     pub fn prev_index(current: usize, total: usize) -> usize {
         if total == 0 { 0 } else if current == 0 { total - 1 } else { current - 1 }
     }
     pub fn next_index(current: usize, total: usize) -> usize {
         if total == 0 { 0 } else if current + 1 >= total { 0 } else { current + 1 }
     }
     ```
6. **Code Quality:**
   - `cargo fmt`
   - `cargo clippy -j 1 --bin minicode -- -D warnings`
7. **Commit:**
   - `feat(ui): implement TerminalGuard and InteractiveSelector primitives for setup wizard`
