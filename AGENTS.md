# minicode — Agent Guidelines & Repository Instructions 🧠

> Managed with `onpkg` and configured for dual-mode human/AI agent operations.

## Project Summary
- **Name:** `minicode`
- **Runtime:** Rust 2021 Edition (Tokio Multi-threaded Async Runtime)
- **Package Manager:** `cargo`
- **Primary Domain:** Fast, Minimalist TUI + CLI AI Coding Agent

## Essential Commands
- **Check Compilation:** `cargo check`
- **Run Development:** `cargo run`
- **Run Tests:** `cargo test`
- **Build Release Binary:** `cargo build --release`
- **Lint:** `cargo clippy -- -D warnings`
- **Format:** `cargo fmt --check` (auto-fix: `cargo fmt`)

## Architecture & Code Conventions
1. **Entrypoint:** `src/main.rs` dispatches between Interactive Ratatui TUI mode and Headless Machine-Readable NDJSON streaming mode (`--json-stream`).
2. **Error Handling:** Use `thiserror` for internal crate errors (`src/agent/`, `src/context/`, `src/tools/`) and `anyhow::Result` at the application/CLI boundaries. Never use `.unwrap()` or `.expect()` in non-test code.
3. **Logging:** Use `tracing` macros (`tracing::info!`, `tracing::debug!`, etc.) everywhere. Never use `println!` or `eprintln!` in library code — they corrupt the ratatui alternate screen. Logs go to file via `tracing-appender`.
4. **No Monolithic Dashboard Panes:** Maintain an inline streaming timeline in `src/ui/view.rs` instead of rigid multi-window dashboard widgets.
5. **AST Code Graph:** Keep Tree-sitter parsers modular in `src/context/repomap.rs` and graph algorithms in `src/context/graph.rs`.
6. **Pure Rust Networking:** Use `rustls-tls-webpki-roots` with `reqwest` to maintain 100% portability without requiring OS-level `libssl-dev` packages.
7. **Tree-sitter Version Alignment:** ALL tree-sitter crates (core + grammars) MUST use the same ABI version. Mixing versions causes segfaults.
8. **Testing:** Use `#[tokio::test]` for async tests. Place unit tests inline in `src/` modules (`#[cfg(test)] mod tests { ... }`). Integration tests go in `tests/`.
9. **Dependencies:** Ask before adding heavy new dependencies. Prefer pure-Rust crates. The `walkdir` crate is intentionally excluded — use `ignore::WalkBuilder` instead.

## Active Documentation & Specifications
- [Product Requirements Document (PRD)](file://./onpkg_docs/prd.md)
- [Terminal & UI Design Specification](file://./onpkg_docs/design.md)
- [Technical Implementation Plan](file://./onpkg_docs/implementation.md)
- [Task Tracker (todo.md)](file://./onpkg_docs/todo.md)
- [CLI & Protocol Reference](file://./onpkg_docs/content.md)
