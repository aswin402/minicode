<p align="center">
  <img src="assets/logo.png" alt="minicode logo" width="180" style="border-radius: 24px;" />
</p>

<h1 align="center">minicode</h1>

<p align="center">
  <strong>Fast, Minimalist AI Coding Agent in Pure Rust — Built for Both Humans and AI Swarms.</strong>
</p>

<p align="center">
  <a href="https://github.com/aswin402/minicode/actions/workflows/ci.yml"><img src="https://github.com/aswin402/minicode/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="https://github.com/daltonmenezes/aura-theme"><img src="https://img.shields.io/badge/Theme-Aura_Dark-a277ff?style=flat-square" alt="Aura Theme" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-2021_Edition-dea584?style=flat-square&logo=rust" alt="Rust 2021" /></a>
  <a href="https://github.com/ratatui/ratatui"><img src="https://img.shields.io/badge/TUI-Ratatui-61ffca?style=flat-square" alt="Ratatui" /></a>
  <a href="#license"><img src="https://img.shields.io/badge/License-MIT-82e2ff?style=flat-square" alt="License" /></a>
  <a href="#verification--tests"><img src="https://img.shields.io/badge/Tests-118_Passing-61ffca?style=flat-square" alt="Tests" /></a>
</p>

---

## ⚡ Highlights

* **Dual-Mode Operation**: Seamlessly switch between an interactive full-screen **Ratatui TUI** for human developers and a machine-readable **NDJSON streaming protocol** (`--json-stream`) for AI agent orchestrators.
* **Dynamic Skill Creation & Hot-Reloading**: Create, validate, and hot-load project-specific skill packages (`.minicode/skills/<name>/SKILL.md`) with YAML frontmatter without restarting the session.
* **Interactive Embedded PTY Drawer (`Ctrl+T`)**: Embedded bottom terminal pane inside Ratatui with a 1000-line bounded ring buffer to run live shell commands, watch dev servers, and test REPLs without leaving the TUI.
* **ARIA Web Browser & Local UI Dev Inspector**: Pure-Rust accessibility tree generator and page inspector that maps interactive elements into numbered references (`@e1`, `@e2`) to test and debug local web applications (`http://localhost:3000`).
* **Cognitive Memory Decay & Repository Knowledge Wiki**: Biological Ebbinghaus exponential memory retention math combined with a persistent Markdown repository knowledge wiki (`.minicode/wiki/`) and automated index cataloging.
* **Sequential Thinking & Graph of Thoughts (GoT)**: Petgraph-tracked dynamic hypothesis branching, revision tracking, confidence scoring, and solution synthesis for complex architectural tasks.
* **Topological Task DAG & Actor-Critic Loop**: Petgraph-powered dependency DAG engine with topological execution sorting, complexity scoring heuristics (1–10), and automated compiler/linter quality gates.
* **Interactive Diff Inspector & Permission Modal**: Syntax-highlighted Aura Theme unified diff viewer with an interactive 4-option permission gate (`Accept`, `Reject`, `Allow Session`, `Type Custom Instructions`).
* **Language Server Protocol & Compiler Self-Healing**: 2-Tier diagnostics (instant < 200ms CLI checks + deep semantic LSP via `rust-analyzer`, `pyright`, `tsc`, and `gopls`) that automatically inspects and heals compiler errors after file modifications.
* **Autonomous Git Engine**: Fully autonomous git operations with automatic post-turn Conventional Commits, lockfile filtering, branch management, and reversible undo rollbacks.
* **Multi-Agent Worktree Orchestration**: Delegate tasks to isolated parallel AI subagents in dedicated Git Worktrees (`subagent/<id>`) without file lock race conditions.
* **Aura Theme Design**: Sleek, distraction-free inline streaming timeline inspired by Dalton Menezes' [Aura Theme](https://github.com/daltonmenezes/aura-theme). Zero cluttered dashboard widgets.
* **Universal Multi-Provider Engine**: Native support for **OpenRouter** (including free-tier models), **Google Gemini**, **OpenAI**, **DeepSeek**, **Groq**, **Together AI**, and local **Ollama/vLLM** endpoints with automatic rate-limit backoff retries.
* **41 Built-in Agent Tools**: Specialized primitives across dynamic skill creation & inspection, web browser ARIA trees, repository knowledge wikis, cognitive memory decay, sequential thinking DAGs, topological task DAGs, Actor-Critic validation, web search & scraping, file editing, LSP diagnostics & code navigation, AST repository mapping, fact memories, plan workflows, git operations, and subagent delegation.
* **AST Code Graph Engineering**: Multi-language Tree-sitter parsers (Rust, Python, JS, TS) with Petgraph dependency graphs and Personalized PageRank for precise repository mapping.
* **OS-Level Kernel Sandbox**: Confinement powered by Linux kernel **Landlock**, workspace path canonicalization, and strict environment variable sanitization.
* **Turn-Level Safety & Undo**: Automatic pre-mutation file snapshots with an instantaneous `/undo` rollback engine.

---

## 📸 Interactive TUI Aesthetic

```text
• Ran git status --short --branch
  └ ## main...origin/main [ahead 1]
    M src/grounding.rs

• Running rustfmt --edition 2021 src/grounding.rs
✔ You approved minicode to run rustfmt --edition 2021 src/grounding.rs this time

• Ran rustfmt --edition 2021 src/grounding.rs
  └ (no output)

• Working (4s • esc to interrupt)
─────────────────────────────────────────────────────────────────────────────
› Implement {feature}
  liquid/lfm-2.5-2.6b:free · ~/programming/my_project · main [default]
```

---

## 🚀 Quick Start

### 1. Installation

#### One-Line Binary Installer (Linux & macOS)
```bash
curl -fsSL https://raw.githubusercontent.com/aswin402/minicode/main/install.sh | bash
```

#### Build From Source (Cargo)
```bash
# Install directly from GitHub
cargo install --git https://github.com/aswin402/minicode.git

# Or clone and build
git clone https://github.com/aswin402/minicode.git
cd minicode
cargo build --release
```

### 2. Interactive Configuration

Run the configuration wizard to set your default provider, model, and API keys:

```bash
minicode configure
# or
just configure
```

Alternatively, configure your environment in `.env`:

```env
# OpenRouter (Supports 100+ models: Claude 3.5 Sonnet, DeepSeek-R1, Qwen2.5-Coder, etc.)
OPENROUTER_API_KEY=sk-or-v1-your-openrouter-key-here
MINICODE_PROVIDER=openrouter
MINICODE_MODEL=liquid/lfm-2.5-2.6b:free
```

### 3. Launching minicode

```bash
# Launch interactive Aura TUI in the current workspace
minicode

# Or run with a one-shot autonomous task
minicode run "Inspect src/main.rs and add comprehensive error handling"

# Or run in accessible plain REPL mode
minicode --plain
```

---

## 🕹️ TUI Keyboard Controls

| Key Shortcut | Action |
| :--- | :--- |
| **`Enter`** | Submit prompt or task to minicode |
| **`Ctrl+J`** / **`Alt+Enter`** | Insert a newline for multi-line prompts |
| **`PageUp`** / **`PageDown`** | Scroll conversation timeline up/down |
| **`Esc`** / **`Ctrl+C`** | Interrupt running execution or exit |
| **`/clear`** | Clear conversation timeline buffer |
| **`/exit`** / **`/quit`** | Gracefully quit session |

---

## 🤖 Dual-Mode Architecture

### 1. Human Mode (Interactive Ratatui TUI)
Full-screen terminal interface with smooth 20 FPS redraws, live execution timers, collapsible tool outputs, and syntax-highlighted Markdown rendering.

### 2. AI Mode (Machine-Readable NDJSON Streaming)
Designed for AI orchestrators (e.g., Claude Code, Antigravity, OpenCode, Codex). Communicates via newline-delimited JSON over `stdin` and `stdout`:

```bash
minicode --json-stream
```

```json
{"type":"heartbeat","timestamp":"2026-08-14T20:00:00Z","status":"ready"}
{"type":"stream_delta","turn_id":1,"delta":"Searching codebase..."}
{"type":"tool_call","turn_id":1,"tool":"grep_search","args":{"query":"Theme"}}
{"type":"tool_result","turn_id":1,"tool":"grep_search","success":true,"output":"src/ui/theme.rs:8","duration_ms":58}
{"type":"turn_end","turn_id":1,"total_tokens_used":3408,"files_modified":[]}
```

---

## 🛠️ The 6 Core Coding Tools

| Tool | Description | Security & Guards |
| :--- | :--- | :--- |
| `read_file` | Reads file content with optional line ranges (`start_line`, `end_line`) | Path traversal blocked; bounds checked |
| `write_file` | Writes full file content atomically with auto-parent directory creation | Automatic backup snapshot in `.minicode/backups/` |
| `patch_file` | Search-and-replace block replacement (Exact → Whitespace → Fuzzy) | Confidence threshold validation |
| `exec_cmd` | Executes shell commands in workspace root | Landlock FS confinement + TCP deny + 30s timeout |
| `grep_search` | Ripgrep-speed regex matching across workspace files | Respects `.gitignore` and hidden file rules |
| `fetch_or_browse` | Scrapes external URL documentation to clean Markdown | HTML readability extraction + size bounds |

---

## 🏗️ Architecture & Subsystems

```text
minicode
├── src/
│   ├── main.rs              # CLI parser, command dispatcher & REPL
│   ├── app.rs               # Ratatui application runner & async event pump
│   ├── error.rs             # thiserror error taxonomy
│   ├── config.rs            # TOML loader, .env & env var override hierarchy
│   ├── logging.rs           # Non-blocking file logger (~/.config/minicode/logs/)
│   ├── lsp/                 # JSON-RPC 2.0 stdio client & 2-tier compiler diagnostics
│   ├── git/                 # Git service, worktrees, token-budgeted diff filters & commits
│   ├── agent/               # ReAct loop, providers (OpenRouter, Gemini, OpenAI) & subagents
│   ├── tools/               # 6 core coding tool implementations & ToolRegistry
│   ├── sandbox/             # Landlock kernel isolation, env sanitization & path boundaries
│   ├── context/             # Tree-sitter AST queries, Petgraph PageRank & token compactor
│   ├── session/             # Turn backup snapshots, /undo engine & JSONL store
│   └── ui/                  # Aura Theme, timeline view, input dock & configure wizard
```

---

## 💻 Developer & Contributor Guide

The repository includes a dedicated [`Justfile`](file://./Justfile) configured for **low-resource compilation** (`-j 2` concurrency limits):

```bash
just check          # Fast compilation check (2 jobs)
just test           # Run all unit/integration tests (2 jobs, 2 threads)
just clippy         # Run clippy with strict zero-warning policy (-D warnings)
just fmt            # Auto-format all Rust source code
just ci             # Full pre-commit verification suite
```

---

## 📄 License

Distributed under the **MIT License**. See [`LICENSE`](file://./LICENSE) for details.
